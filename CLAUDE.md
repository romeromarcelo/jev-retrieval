# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Commands

```sh
cargo build --release                              # binary: target/release/jevr
cargo test --release                               # unit + integration tests
cargo clippy --release --all-targets -- -D warnings   # must stay clean
cargo fmt                                          # rustfmt
```

Runtime needs `TYPESAFE_API_KEY` exported. `JEVR_DEBUG=1` prints one stderr
line per billed network request (cache hits excluded) — the request-counting
hook for benchmarks.

## Architecture

Three-stage pipeline, wired in `src/main.rs` (exit codes: 0 matches, 1 none, 2 error):

Interface: `jevr <QUERY> [PATH]` (see the agentic contract below).

The tool searches **two file kinds** (`walk::FileKind`, decided by extension): code and text documents (md, rst, txt, html, org, tex, ...). Every stage is lane-aware so document support can never perturb code-search behavior.

| Stage | Modules | What happens |
|---|---|---|
| 1 — candidates (local, stateless) | `walk.rs`, `expansion.rs`, `bm25.rs`, `candidates.rs` | gitignore-aware walk (code + doc extensions) → **one BM25 index per kind** (docs are wordier and would dilute the code lane's `candidate_cap` slots) → top `candidate_cap` per variant per lane ∪ git-grep hits. Unicode-aware code tokenizer: camelCase/snake_case subtokens + whole identifiers + path tokens. Rebuilt per invocation; nothing persisted. `--candidates <file>` bypasses the whole stage. |
| 2 — verification (Jev, concurrent) | `windows.rs`, `verify.rs` | 100/20-line windows, ≤24 windows / ≤24 KB per request, one file per request; per-window + duplicated file-level questions averaged to cut flicker. Code files get subsystem-membership questions under a `code` state key; documents get coverage questions under a `text` key. Score = max(best window, file average); keep ≥ `threshold` (code, 0.90) or ≥ `doc_threshold` (docs, 0.60). Per-request failures degrade to warnings. |
| 3 — rerank (Jev, 1 request per lane) | `rerank.rs` | One listwise Choice per non-empty lane over kept files' 25-line heads ("implements" phrasing for code — byte-identical to the all-code contract — "contains" for documents). Code orders by the probabilities directly; documents order by `score + doc_rank_weight (1.0) × rank_probability` — the fusion is measured and the code path is untouched. Falls back to score order on failure. |
| API layer | `jev/client.rs`, `jev/questions.rs`, `jev/schemas.rs` | Single HTTP choke point with retry (3×, 250/500/1000 ms, only 408/429/5xx) and the sha256-keyed response cache; question builders; hand-rolled wire mirror. |

`config.rs` holds every tunable (`deny_unknown_fields`); precedence: defaults < YAML (`--config`, else repo-root `.jevr.yaml`, else `~/.config/jevr.yaml`) < CLI flags.

## Agentic interface contract (`main.rs` args + `output.rs`)

The CLI is the product surface for coding agents; its shape was set by live
A/B observation of fresh Claude Code sessions (docs/BENCHMARKS.md §7) and
must stay grep-familiar:

- `jevr <QUERY> [PATH]` — PATH is positional (directory **or single
  file**) with `-p` as alias; agents type `tool query src/` from grep muscle
  memory. Works identically on code and document corpora.
- Default output is `path:start-end  score` (Read-ready targets); `--snippets`
  adds `N:`-prefixed lines capped at `snippet_lines` (default 25) with a
  `... N more lines (Read path:A-B for the rest)` pointer — uncapped doc
  snippets measured past agent tool-output truncation; `--json` keeps the
  full snippet and honors `top_k` like the other modes.
- When document results print, exactly one stderr line summarizes the lane
  split (`N code result(s) kept at score >= 0.90, then M document result(s)
  kept at score >= 0.60`) — live sessions read mixed sub-0.90 doc scores as
  broken filtering without it. Code-only runs stay stderr-silent.
- Results print code lane first, then documents; `top_k` and the empty-result
  fallback apply per lane, so document matches never displace, reorder, or
  fallback-suppress code matches (both failure modes were caught by parity
  A/Bs, docs/BENCHMARKS.md §6).
- Paths print prefixed with the PATH argument as given, so they open from the
  caller's cwd.
- Below-threshold fallback prints a stderr notice and exits 1 (grep-honest);
  a closed stdout pipe (`| head`) must end rendering silently, never panic —
  `output::render` maps BrokenPipe to `Ok`.
- `--help` is the primary agent documentation (observed: every fresh agent
  runs it first). Keep it example-led and free of internal jargon, and never
  re-add advice to supply `-k` variants (measured worse: 0.614 vs 0.673).

## Invariants — do not change without re-running the benchmark evidence

These values are measured operating points, not arbitrary constants. The
evidence record is `docs/BENCHMARKS.md`; the BEIR gates are reproducible via
`benchmarks/`.

- **Model is pinned** (`jev-1.13.0`): the 0.90 threshold is calibrated per model version; bumping the model silently invalidates it.
- **Request caps stay at 24 KB / 24 windows, one file per request** (`windows.rs`). Doubling the caps cut requests 2.4× but dropped a large concern's F1 0.39→0.24 in two independent runs; multi-file request packing (−15% requests) regressed SWE-bench 0.760→0.720 in a same-day A/B. Both were implemented and reverted — per-token billing means neither saves money anyway (docs/BENCHMARKS.md §5).
- **Question phrasing in `jev/questions.rs` is measured.** The concept string lives in `state`, never interpolated into `instructions` (Jev reads literally). Rewording moved dedicated-support-file scores from 0.05–0.21 to 0.84–0.87.
- **Document questions and `doc_threshold` are measured too** (docs/BENCHMARKS.md §4): the code phrasing under-scores directly relevant prose below the 0.90 gate; the coverage phrasing separates 0.97–0.98 relevant vs 0.01–0.06 incidental. `doc_threshold` 0.60 is the macro-F1 plateau on BEIR SciFact + NFCorpus (623 queries). Doc windows travel under a `text` state key; changing any of this shifts cache keys and calibration.
- **Lane separation is a parity invariant**: per-kind BM25 indexes, per-lane fallback, and the byte-identical all-code rerank body exist because shared candidate slots and a shared fallback each lost gold files in parity A/Bs (docs/BENCHMARKS.md §6). Pure-code and pure-doc corpora must behave as if the other kind did not exist.
- **Wire format is a contract**: `tests/wire.rs` replays bodies captured from the official SDK. Any request/response shape change must extend those golden tests.
- **Cache correctness is structural**: key = sha256(entire serialized body) under `~/.cache/jevr/`; file content is inside the hash so entries cannot go stale. Never write cache or index state into searched repositories.
- **Bare queries beat `-k` variants for BM25** (0.673 vs 0.614 macro-F1) — don't add variants to benchmark runs or examples.

## Benchmarking notes

- **SWE-bench Lite (50-sample) is the stable code gate**: the unchanged pipeline reproduced 0.760/0.900/0.900/0.827 exactly across two days. Score-affecting changes need a same-day A/B there (protocol in docs/BENCHMARKS.md §2). Small hand-labeled benchmarks are noisy — per-concern F1 flips ±0.3 near the 0.90 threshold; never accept or reject a change on one draw.
- **Document retrieval gates: BEIR SciFact and NFCorpus** (docs/BENCHMARKS.md §3): full pipeline nDCG@10 0.778 / 0.363 vs published BM25 0.665 / 0.325 and best verified zero-shot rerankers 0.777 / 0.399. Reproduce with `benchmarks/` — corpora convert to one `.txt` per doc (title, blank line, text, trailing newline — this exact format keys the response cache); run per query with `--threshold 0 --top-k 10 --json`.
- The response cache makes warm repeats free — clear `~/.cache/jevr/` (or pass `--no-cache`) when an independent draw is required. A cold BEIR dataset pass bills a few dollars.
- **docs/BENCHMARKS.md §5 records the measured dead ends** (bigger request caps, multi-file packing, extra per-doc questions, candidate depth 100, rerank draw averaging, multi-round listwise refinement) — re-test before re-proposing any of them.

## Repository facts

- `docs/BENCHMARKS.md` is the evidence record; append new experiment evidence there as numbered sections rather than creating new documents.
- `benchmarks/` holds the reproducible BEIR harness; `skills/` + `.claude-plugin/` ship the Claude Code skill and plugin (`./install.sh`).
