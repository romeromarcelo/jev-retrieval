# jevr Technical Guide

How the pipeline works, what every tunable means, and which decisions are
measured invariants. The evidence behind every number cited here lives in
[BENCHMARKS.md](BENCHMARKS.md); this file explains the machinery.

## Pipeline internals

`jevr` is a three-stage pipeline (`src/main.rs` wires it; exit codes: `0`
matches, `1` none above threshold, `2` error). Stage 1 runs locally and is
free; Stages 2–3 call the TypeSafe Jev API.

### Stage 1 — candidates (local, stateless)

- **Walk** (`walk.rs`): a gitignore-aware walk over code *and* document
  extensions (md, rst, txt, html, org, tex, ...). A skip list drops
  dependency and build-output directories — they dominate raw file counts
  while contributing zero signal to concept search. Each file gets a
  `FileKind` (code vs document) from its extension; that kind selects which
  question set verifies it downstream, because code phrasing measurably
  under-scores directly relevant prose.
- **Query variants** (`expansion.rs`): variants come from the caller (`-k`,
  repeatable, ≤4), never from Jev — Jev is System One: it selects, it does
  not generate. A stopword fallback strips interrogative filler ("where
  does...") from bare queries. Bare natural-language queries measure *better*
  than hand-picked variants (macro-F1 0.673 vs 0.614), so `-k` exists only
  for callers who have measured better variants for their domain.
- **BM25** (`bm25.rs`): a hand-rolled, eager, in-memory index — one per file
  kind, so wordy prose never crowds code out of the candidate slots. At repo
  scale (≤ ~6 k whole-file docs) an inverted-index engine solves problems
  jevr doesn't have: tantivy hardcodes k1/b and adds ~50 dependencies, while
  eager scoring over in-RAM postings takes microseconds. The Lucene formula
  is used because variant choice is statistically indistinguishable
  (Kamphuis et al., ECIR 2020) and its IDF cannot go negative on
  near-universal code tokens (`self`, `return`). What *does* matter for code
  retrieval is tokenization (Zhang et al., NLP4Prog 2021): the tokenizer
  emits camelCase/snake_case subtokens plus whole identifiers, and indexes
  path tokens with the content — file paths are strong signals for
  file-level localization (arXiv:2607.11046).
- **Union** (`candidates.rs`): the top `candidate_cap` files per variant per
  lane are unioned with `git grep -il` hits — zero-cost insurance for
  literal-vocabulary queries whose exact terms appear verbatim but rank
  poorly under BM25. Order is irrelevant: Stage 2 re-scores every candidate
  independently. Nothing is persisted; the index is rebuilt per invocation
  (~1 s at repo scale). `--candidates <file>` bypasses the whole stage.

### Stage 2 — verification (Jev, concurrent)

- **Windowing** (`windows.rs`): each candidate splits into overlapping
  100/20-line windows, so a match straddling one boundary lands whole inside
  the neighbor. UTF-8-safe byte chunking handles pathological
  minified/single-line files. Requests are capped at 24 windows / 24 KB, one
  file per request — the measured envelope where server latency stays
  0.3–0.8 s *regardless of batch size*, which is what makes request-level
  concurrency effective (wall time ≈ ceil(batches / concurrency) × 0.8 s;
  the default `concurrency: 32` took the heaviest benchmark query from
  12.6 s at 8 to 3.7 s).
- **Questions** (`jev/questions.rs`): code files get subsystem-membership
  questions under a `code` state key; documents get coverage questions under
  a `text` key. The phrasing is measured, not styled: membership phrasing
  with the concept referenced by backticked state path scores dedicated
  support files 0.84–0.87 where a naive interpolated question scored the
  same files 0.05–0.21; the document coverage phrasing separates 0.97–0.98
  relevant from 0.01–0.06 incidental prose. The concept string always lives
  in `state`, never inside `instructions` — Jev reads literally.
- **Scoring** (`verify.rs`): score = max(best window, whole-file average).
  A file can be relevant while no single window is; the file-level question
  rides the same request for free and is asked twice and averaged —
  identical questions return independent draws (Δ 0.01–0.03), so averaging
  cuts threshold-adjacent flicker by √2 for ~60 input tokens. Per-request
  failures degrade to warnings; a file errors out only when every one of its
  batches failed. Keep gate: ≥ `threshold` (code, 0.90) or ≥ `doc_threshold`
  (documents, 0.60).

### Stage 3 — ranking (Jev, one request per lane)

One listwise Choice question per non-empty lane over the kept files' 25-line
heads. The two signals are complementary: per-file Nouls calibrate keep/drop
against the lane threshold but under-resolve order near the top, while one
listwise Choice ranks the whole kept set in a single ~0.3 s call — the Noul
decides *what* is kept, the Choice distribution decides *order*. Code orders
by the probabilities directly; documents order by
`score + doc_rank_weight × rank_probability`, because a single Choice is
winner-take-all — sharp when one file holds the answer, poorly resolved
below the winner when many documents are relevant. Falls back to score order
on any failure.

### Wire format and API layer

The tool speaks raw HTTP (`jev/client.rs`) — no Rust SDK exists — through a
hand-rolled serde mirror (`jev/schemas.rs`) of the System One wire format,
captured byte-for-byte from the official Python SDK's debug logs.
`tests/wire.rs` replays those captured bodies as golden tests; any
request/response shape change must extend them. Retry policy: 3 attempts,
exponential backoff from 250 ms, only on 408/429/5xx — matching the observed
~1-in-40 transient failure rate.

## Configuration reference

Precedence: defaults < YAML file < CLI flags. The first file found wins: an
explicit `--config` path, then `.jevr.yaml` in the searched repo root, then
`~/.config/jevr.yaml`. Only fields present in the file are overlaid; unknown
fields are rejected (`deny_unknown_fields`).

```yaml
model: jev-1.13.0     # pinned — threshold calibration is version-specific
threshold: 0.90       # keep code files with membership score ≥ this
doc_threshold: 0.60   # keep text documents with coverage score ≥ this
doc_rank_weight: 1.0  # document order = score + this × rank_probability (0 = pure score)
candidate_cap: 30     # Stage-1 files per query variant, pre-union
concurrency: 32       # concurrent Stage-2 requests
empty_fallback: 3     # best-effort results when nothing clears threshold (0 = off)
top_k: 10             # output cap per result lane, all modes
snippet_lines: 25     # max printed lines per snippet in --snippets (--json uncapped)
git_grep_union: true  # literal-vocabulary insurance
bm25_k1: 1.2          # BM25 term-frequency saturation
bm25_b: 0.75          # BM25 length normalization
cache: true           # client-side response cache
```

Why these values (all measured, `src/config.rs` carries the full rationale):

| Key | Rationale |
|---|---|
| `model` | Pinned, not `jev-latest`: threshold calibration shifts between Jev versions and would silently invalidate the 0.90 gate. |
| `threshold` | max-over-windows scoring inflates scores relative to a single whole-file judgment; the measured operating plateau is 0.85–0.92 with the optimum at 0.90. |
| `doc_threshold` | The 0.90 code gate over-rejects prose: on BEIR SciFact/NFCorpus (623 queries) it left 45–65% of queries empty while macro-F1 peaked in the 0.55–0.65 band. 0.60 keeps "more likely relevant than not" semantics above the Noul 0.5 coin-flip floor. `--threshold` overrides both gates. |
| `doc_rank_weight` | Fusing verify score with rank probability measured +0.007 SciFact / +0.002 NFCorpus nDCG@10, flat across w ∈ [0.5, 2.0]; 1.0 is the plateau center. |
| `concurrency` | 12.6 s at 8 → 3.7 s at 32 on the heaviest benchmark query, plateau by 32–48, no 429s observed. |
| `empty_fallback` | Out-of-envelope queries (bug reports) often score everything below threshold while still ranking the right file first — return the best few, flagged, instead of silence. |
| `snippet_lines` | Two document results measured 22 KB / 189 lines — past what agent harnesses keep before truncating tool output wholesale. 25 lines matches the rerank head size. |
| `bm25_k1`, `bm25_b` | Code-search tuning evidence puts the useful grids at k1 ∈ [0.7, 1.3], b ∈ [0.7, 1.0] (Zhang et al., NLP4Prog 2021). |

## Response cache and cost

Jev bills identical resent states in full — there is no server-side caching
or repeated-state discount. Responses are therefore cached client-side under
`~/.cache/jevr/` (`$XDG_CACHE_HOME` honored), keyed by
sha256(entire serialized request body). The key covers model, concept, and
window content in one hash, so entries can never go stale: any file edit
changes the windows and therefore the key. Repeated queries over unchanged
code replay byte-identical results with zero network requests (p50 0.2 s vs
2.8 s cold). Cache writes are atomic (temp + rename) and failures are
deliberately swallowed — a failed write only costs a future cache miss.
Searched repositories are never written to.

Billing is per input token (output tokens are free); a cold query costs on
the order of a cent, warm repeats are free. `JEVR_DEBUG=1` prints one stderr
line per billed network request (cache hits excluded) — the request-counting
hook used by the benchmarks. `--no-cache` disables the cache when an
independent draw is wanted despite identical inputs.

## Development invariants

These are measured operating points, not arbitrary constants — do not change
them without re-running the benchmark evidence
([BENCHMARKS.md](BENCHMARKS.md); reproduce the BEIR gates with
[../benchmarks/](../benchmarks/)):

- **The model is pinned** (`jev-1.13.0`): the 0.90 threshold is calibrated
  per model version.
- **Request caps stay at 24 KB / 24 windows, one file per request.**
  Doubling the caps cut requests 2.4× but dropped a large concern's F1
  0.39 → 0.24 twice; multi-file packing (−15% requests) regressed SWE-bench
  0.760 → 0.720 in a same-day A/B. Per-token billing means neither saves
  money anyway (BENCHMARKS.md §5 records these and the other rejected
  ablations — re-test before re-proposing any of them).
- **Question phrasing and the document thresholds are measured** — changing
  any of it shifts cache keys and calibration.
- **Lane separation is a parity invariant**: per-kind BM25 indexes, per-lane
  fallback, and a byte-identical all-code rerank body exist because shared
  candidate slots and a shared fallback each lost gold files in parity A/Bs
  (BENCHMARKS.md §6). Pure-code and pure-doc corpora must behave as if the
  other kind did not exist.
- **The wire format is a contract**: extend the golden tests in
  `tests/wire.rs` with any shape change.
- **SWE-bench Lite (50-sample) is the stable code gate** (0.760/0.900
  reproduced exactly across two days); score-affecting changes need a
  same-day A/B there. Small hand-labeled benchmarks flip ±0.3 near the
  threshold — never accept or reject a change on one draw.

Verification commands:

```sh
cargo build --release
cargo test --release
cargo clippy --release --all-targets -- -D warnings
cargo fmt --check
```
