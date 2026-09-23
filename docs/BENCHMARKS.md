# jevr — Benchmark Evidence

Every default in `src/config.rs` is a measured operating point, not an
arbitrary constant. This document records the evidence behind those defaults
and the ablations that were tested and rejected, so that changes can be judged
against the same gates. Source-code comments that cite measurements point
here.

Model: TypeSafe Jev, pinned `jev-1.13.0`. Thresholds are calibrated per model
version; changing the model silently invalidates them.

## 1. Protocol notes

- **Billing and instrumentation.** Jev bills input tokens only (output free);
  there is no server-side caching, no repeated-state discount, and no batch
  endpoint — one state per request. `JEVR_DEBUG=1` prints one stderr line per
  billed network request (cache hits excluded), which is how request volume is
  counted below. A single query costs roughly $0.01; a full BEIR dataset pass
  a few dollars.
- **The response cache makes repeats free.** Cache keys are
  sha256(serialized request body) under `~/.cache/jevr/`; file content is
  inside the hash, so entries cannot go stale. Warm reruns replay
  byte-identical output with 0 network requests. Clear the cache (or pass
  `--no-cache`) when an independent scoring draw is required.
- **Variance.** Jev probabilities carry ~±0.05 single-draw variance, so
  per-query F1 can flip near a threshold. Small internal benchmarks are noisy
  (identical-config macro-F1 draws spanned 0.609–0.661 in one day); the
  external gates below are the stable evidence. Never accept or reject a
  score-affecting change on a single noisy draw.

## 2. Code retrieval

### SWE-bench Lite, file localization (50-instance stratified sample)

Deterministic sample (every 6th of the id-sorted 300 instances; 11 repos),
checkout `repo@base_commit`, query = the raw problem statement, no keyword
variants — the autonomous floor on bug-report-style queries, a harder shape
than the concept queries the tool targets. Gold = non-test files touched by
the merged patch.

| Metric | jevr | Published anchors |
|---|---|---|
| Recall@1 (any gold) | 0.76 | — |
| any-gold@10 | **0.90** | BM25@27K: 0.513 |
| all-gold@10 | **0.90** | Agentless (GPT-4o): 0.697 · LocAgent (Claude-3.5, @5): ≈0.927 |
| MRR (first gold) | 0.83 | — |

The unchanged pipeline reproduced 0.760 / 0.900 / 0.900 / 0.827 exactly
across two days — this sample is the stable regression gate for
score-affecting changes. Caveats: one 50-instance sample, Python-only repos,
plausibly in every model's training data.

### Loc-Bench V1, Feature Request category (30-instance sample)

Contamination-resistant benchmark, hardest category, same protocol (every 5th
of the id-sorted 150 feature instances; 18 repos). Anchors from the LocAgent
paper (arXiv 2503.09089, full 560-instance mixed set, strict all-gold): those
systems are multi-turn agent loops or trained code embedders.

| Metric | jevr |
|---|---|
| Recall@1 (any gold) | 0.633 |
| any-gold@10 | 0.700 |
| strict Acc@5 (all gold) | 0.467 |
| MRR | 0.667 |

A single-pass ~$0.01 CLI sits below the multi-turn agents (0.675–0.834
strict) and far above silence. The `empty_fallback` default (3) exists
because of these query shapes: many bug-report runs keep nothing at the 0.90
gate while still having ranked the right file internally — returning the
best three low-confidence guesses (marked `fallback`, exit 1) lifted any-gold
recall by double digits on both external benchmarks.

## 3. Document retrieval (BEIR)

Full test splits, `--threshold 0 --top-k 10 --json`, defaults otherwise.
Corpora are converted to one `.txt` per document — title, blank line, text,
trailing newline; that exact byte format keys the response cache.
Reproduction scripts: [`benchmarks/`](../benchmarks/).

| Dataset | Queries | jevr nDCG@10 | Stage-1-only (BM25) | Published BM25 | Published BM25+CE | Best verified zero-shot reranker |
|---|---|---|---|---|---|---|
| SciFact | 300 | **0.778** | 0.671 | 0.665 | 0.688 | 0.777 |
| NFCorpus | 323 | 0.363 | 0.310 | 0.325 | 0.350 | **0.399** |

Anchors: BEIR paper Table 1 (arXiv:2104.08663) for BM25 and BM25+CE;
zero-shot reranker anchors are the best published nDCG@10 we could verify
(SciFact 0.7773; NFCorpus 0.3994). jevr beats BM25 and the cross-encoder
rerank on both sets and edges past the zero-shot frontier on SciFact —
honestly, it does **not** reach the NFCorpus frontier (0.363 vs 0.399; see
§5). Cold cost ≈31 billed requests/query at 1.8–2.2 s; the systems holding
the anchor scores report 30–76 s/query.

The Stage-1-only column doubles as a tokenizer validation: a faithful
replication of the tokenizer + Lucene BM25 lands on the published BM25
anchors (±0.015), confirming the code-oriented tokenizer does not hurt prose.

## 4. Calibration evidence

- **Code threshold 0.90.** The pipeline scores a file as max(best window,
  whole-file average), which inflates scores relative to a single whole-file
  judgment. A sweep over captured verdicts on a 12-query hand-labeled internal
  benchmark: macro-F1 0.406 at 0.65, 0.587 at 0.85, **0.648 at 0.90**, 0.601
  at 0.92 — a stable 0.85–0.92 plateau, not a knife edge.
- **doc_threshold 0.60.** At the code gate 0.90, 194/300 SciFact and 144/323
  NFCorpus queries kept nothing. Sweeps put macro-F1's plateau at 0.55–0.65 on
  both datasets (SciFact 0.493 at 0.60 vs 0.313 at 0.90; NFCorpus 0.163 vs
  0.111), comfortably above the Noul 0.5 coin-flip floor.
- **Question phrasing is measured.** Subsystem-membership phrasing with the
  concept referenced by backticked state path scores dedicated support files
  0.84–0.87 where a naive interpolated question scored them 0.05–0.21, with
  an unmoved negative control. The code phrasing under-scores directly
  relevant prose (0.88/0.66, below the 0.90 gate); the document coverage
  phrasing separates cleanly — relevant 0.97–0.98, incidental mention
  0.04–0.06, unrelated 0.01. Bodies are pinned by `tests/wire.rs`; any
  wording change alters cache keys and calibration.
- **Duplicate file-question averaging.** Identical questions in one request
  return independent draws (measured |Δ| 0.01–0.03); asking the file-level
  question twice and averaging cuts threshold-adjacent flicker by √2 for ~60
  input tokens, and recovered a benchmark concern that oscillated exactly at
  the 0.90 cut.
- **Concurrency 32.** Sweep on the heaviest benchmark query: 12.6 s at 8,
  8.9 s at 16, 3.7 s at 32, plateau by 32–48, no 429s observed.
- **Document ordering fusion (`doc_rank_weight` = 1.0).** A single listwise
  Choice is winner-take-all: sharp when one file holds the answer,
  under-resolved below the winner when many are relevant. Ordering documents
  by `score + w × rank_probability` measured +0.007 SciFact nDCG@10 and
  +0.002 NFCorpus, flat across w ∈ [0.5, 2.0]; w = 1.0 is the plateau center.
  The code lane keeps pure rerank order and its request bodies are
  byte-identical with or without documents present.

## 5. Ablations tested and rejected

Each of these was implemented, measured, and reverted. Re-test before
re-proposing any of them.

- **Doubling request caps to 48 KB / 48 windows** cut requests 2.4× but
  dropped the largest concern's F1 0.39 → 0.24 in two independent runs
  (large-state dilution). The 24 KB / 24-window cap is an accuracy operating
  point, not a transport limit.
- **Multi-file request packing** (several files per request) saved 15% of
  requests but regressed SWE-bench 0.760 → 0.720 in a same-day A/B:
  irrelevant co-packed content acts as a distractor. Per-token billing means
  packing saves no money anyway.
- **`-k` keyword variants for BM25** measure worse than bare queries
  (macro-F1 0.614 vs 0.673): per-variant top-30 unions dilute precision. The
  flag remains for recall emergencies but is deliberately de-emphasized in
  `--help` (an earlier help text recommending variants made every fresh agent
  session pile them on).
- **Richer per-doc questions** (graded Score, topic Noul, usefulness Noul,
  and combinations): the production score's pairwise concordance (0.806) beat
  every added signal and every combination on a stratified 72-pair probe.
- **Candidate depth 100** raises the oracle ceiling but realized only
  +0.011–0.014 nDCG on subsets: the judge's false-positive tail
  (P(relevant | score ≥ 0.9) = 0.54 on NFCorpus grade-0 docs) contaminates in
  proportion, the 100-option rerank request fails outright, and cost is 3.3×.
- **Rerank draw averaging** (4× Choice per request): ±0.004, within noise.
- **Multi-round listwise refinement** (sliding windows, disjoint tournaments,
  pairwise duels, gated deep insertion, permutation-shuffled draws — the
  mechanisms behind the NFCorpus anchor systems): every mechanism measured
  flat or negative within a ≤5 s/query budget. Deep entrants (BM25 rank
  31–100) that cleared the gates had relevance precision 0.00 on SciFact and
  0.17–0.33 on NFCorpus. Assessment: NFCorpus ~0.399 is unreachable for this
  pipeline zero-shot with the pinned judge; the systems that reach ~0.397+
  are either distillation-trained or pay 60–76 s/query.

## 6. Lane separation evidence

Sharing state between the code and document lanes lost gold files three
distinct ways in parity A/Bs on mixed corpora, each fixed structurally:

1. **Stage-1 dilution** — wordy documents crowded code out of a shared BM25
   index's `candidate_cap` slots; several instances lost their gold file
   entirely. Fix: one BM25 index per file kind.
2. **top_k displacement** — kept documents pushed code results out of a
   single printed window. Fix: per-lane top_k, code prints first.
3. **Fallback suppression** — kept documents silenced the code lane's
   below-threshold guesses. Fix: per-lane fallback and stderr notices.

Final parity: code-lane outputs byte-identical between the pre-documents and
lane-separated binaries on every gate (shared response cache proving request
bodies unchanged), with documents strictly appended.

## 7. Interface evidence (live agent sessions)

The CLI surface was shaped by observing fresh, unsteered Claude Code sessions
using the tool with no documentation beyond `--help`:

- Every session ran `--help` first — the help text is the de-facto
  documentation and is treated as part of the measured configuration.
- Grep muscle memory expects `tool query path/`: the positional `[PATH]`
  argument took adoption from 0% to 100%.
- The default output became `path:start-end  score` because agents needed
  Read-ready targets, and the previously printed rank-probability column was
  never used.
- Sub-threshold fallback results printed indistinguishably from real matches
  caused an agent to quote 0.45-scoring guesses as findings — hence the
  stderr notice and exit 1.
- Two uncapped document snippets measured 22 KB — past agent-harness
  tool-output truncation — hence the 25-line snippet cap with an explicit
  `Read path:A-B` continuation pointer.
- Mixed sub-0.90 document scores after 0.9x code scores read as broken
  filtering — hence the one-line stderr lane summary whenever documents
  print.
- `--snippets | head` used to panic on EPIPE; BrokenPipe now ends rendering
  silently.

After the redesign, logged sessions showed first-try-valid invocations,
unprompted iterative narrowing (repo → subtree → file) and threshold probing,
and exit 1 correctly interpreted as "no confident match" — never as tool
failure.

## 8. External leaderboard: HAKARI-Bench NanoRTEB

Run 2026-09-23 against the [HAKARI-Bench](https://github.com/hakari-bench/hakari-bench)
NanoRTEB benchmark — 14 English specialized-domain tasks (legal, finance,
healthcare, code), 2,390 judged queries per mode, corpora of 82–10,000
documents — and compared against the public
[leaderboard](https://huggingface.co/spaces/hakari-bench/leaderboard)
(database snapshot of the same date; 77 ranked models in Retrieval mode, 90
in Reranking). Protocol, per-task scores, and the replication guide are in
[`benchmarks/RTEB.md`](../benchmarks/RTEB.md); the leaderboard's Borda/pool
rules were validated to exact equality against its published tables before
inserting jevr. All qrels graded, nDCG@10, single draw.

| Run | Mean nDCG@10 | Borda | Leaderboard rank |
|---|---|---|---|
| Reranking (fixed RRF top-100 candidates) | **0.790** | 95.1 | **#2 of 90** |
| Retrieval, defaults (`candidate_cap` 30) | 0.604 | 69.5 | #19 of 77 |
| Retrieval, `candidate_cap: 100` overlay | 0.678 | 82.8 | #10 of 77 |

Three findings:

- **Verification + rerank is near the top of the field when recall is not
  the constraint.** In Reranking mode jevr sits second only to an 8B
  embedding model (Nemotron-3-Embed-8B, 0.811), ahead of every dedicated
  cross-encoder on the board; NanoHumanEval reaches 1.000 with the gold
  solution ranked first on all 158 queries (verified against the raw
  rankings, not a scoring artifact).
- **Full-corpus mode is bounded by Stage-1 lexical recall on these domains.**
  NanoApps (8,754 code documents, problem-statement queries) scores 0.090 at
  defaults vs 0.994 with oracle-quality candidates — the verifier is fine,
  BM25's top-30 misses the gold. Raising `candidate_cap` to 100 improved
  **all 14 tasks** (mean +0.073; largest where lexical overlap is worst:
  AILAStatutes +0.225, HumanEval +0.126, MBPP +0.118) and moved jevr from
  #19 to #10.
- **The §5 candidate-depth rejection is workload-dependent, and the default
  stands.** On BEIR, depth 100 realized only +0.011–0.014 because BM25's
  top 30 already contained the gold; on NanoRTEB the same change buys
  +0.073 because it often does not. The deeper pool costs ~3.3× requests
  and stays behind the fixed-candidate ceiling (0.678 vs 0.790), so
  `candidate_cap` stays 30 by default — changing it would also need the §2
  code gates — and `candidate_cap: 100` via `--config` is the measured
  recommendation for specialized corpora where query and document
  vocabularies diverge (statutes, canonical code solutions, clinical text).
- **jevr is the only stateless system on the board.** Every ranked model
  carries corpus-side state before it can answer its first query: dense and
  late-interaction models embed the entire corpus into a vector index that
  must be rebuilt or resynced on every corpus change, and cross-encoders
  need GPU serving plus an upstream retriever. jevr's Stage 1 rebuilds BM25
  in memory per invocation and nothing is ever written into the searched
  corpus — a fresh or freshly edited corpus is searchable immediately. Cold
  latency stayed in the interactive band throughout: 2.9 s / 104 billed
  requests on the heaviest task's pilot query (NanoAILACasedocs, 223 KB
  documents), 1.8–2.2 s typical (§3).

Latency and caching, measured during these runs: a warm repeat of a
NanoDS1000 query (997-document corpus) replayed byte-identically in
**0.1 s with 0 billed requests** — the sha256(request-body) cache embeds
file content, so entries can never go stale and an agentic
search-edit-search loop re-bills only requests whose input bytes changed.
The same mechanism made the cap-100 re-run incremental: a byte-identical
corpus rebuild replayed all shared verification requests free and billed
only the newly admitted candidate slots. A fully cold two-mode pass is
roughly 300K requests.
