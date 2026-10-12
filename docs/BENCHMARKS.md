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
the merged patch. Harness: [`benchmarks/swebench.py`](../benchmarks/swebench.py).

| Metric | jevr | Published anchors |
|---|---|---|
| Recall@1 (any gold) | 0.72 | — |
| any-gold@10 | **0.90** | BM25@27K: 0.513 |
| all-gold@10 | **0.90** | Agentless (GPT-4o): 0.697 · LocAgent (Claude-3.5, @5): ≈0.927 |
| MRR (first gold) | 0.81 | — |

Measured 2026-10-12 with the 0.2.1 pipeline (32,847 billed requests, ~690
per instance) and replayable offline (§11). The first measurement, on
2026-09-20, predates the document lane and the response cache: two runs that
day both gave 0.760 / 0.900 / 0.900 / 0.827. Two fewer instances now rank
their gold file first; pipeline changes and draw noise are not separated.
The checkouts sit under `benchmarks/repos/`, where this repository's
`.gitignore` applies: its `debug/` rule hides two Django test templates from
the document lane, which the scores do not use.
This sample is the regression gate for score-affecting changes, run as a
same-day A/B. Caveats: one 50-instance sample, Python-only repos, plausibly
in every model's training data.

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
Reproduction scripts: [`benchmarks/`](../benchmarks/). Both numbers replay
offline and for free from the run's response cache (§11).

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
- **Cheaper question text and content-anchored windows** were measured
  without being implemented (§10). Neither saves more than fractions of a
  cent per query, and both would change calibrated behavior.
- **A packed SQLite response cache** was prototyped (§10). It shrinks the
  cache's disk use 13.75× but adds 1.7 MB of peak memory and 1.9 MB of binary.

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
inserting jevr. All qrels graded, nDCG@10, single draw. HAKARI's own scorer
counts every judged document as relevance 1; only NanoCUREv1 has graded
qrels, and scored that way jevr's means rise by 0.002–0.004.

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

## 9. Latency and memory (byte-identical output)

Measured 2026-10-11 on an Apple M3 Max (14 cores). The main corpus is a
shallow clone of Django: 4,140 searchable files, 27 MB, 3.6 M tokens, mixed
code and documents. "Warm" means a full cache replay. Medians of 5 runs.

| Stage 1 on Django | Before | After |
|---|---|---|
| Walk | 100 ms | 83 ms (133 ms while `git grep` runs alongside) |
| BM25 build + top | 550 ms | 55 ms |
| `git grep` (on the critical path) | 175 ms | ≈1 ms left to wait (runs during walk + BM25) |
| Stage 1 total | 826 ms | 190 ms |
| BM25 index peak RSS | 36.7 MB | 14.0 MB |

What changed in Stage 1:

- **BM25 postings cover only the query terms.** Every token still counts
  toward document length, and `top()` is unchanged. The old index held
  postings for the whole vocabulary, which no query ever reads.
- **The tokenizer allocates nothing per token.** It works on byte offsets,
  and ASCII lowercasing reuses one buffer.
- **Files are read and tokenized on every core,** then merged in walk order.
- **The walk uses the entry's `readdir` type** instead of a `stat` per entry.
- **`git grep` starts before the walk, on a background thread,** with
  pathspecs limiting it to searchable extensions. Any other hit would be
  dropped by the walked-set filter anyway. On repos that track large
  non-source files, those pathspecs also removed most of the `git grep`
  child's memory.

| Warm query | Latency before → after | Peak footprint (jevr) | Peak RSS incl. `git grep` |
|---|---|---|---|
| Django, mixed, `--json` | 0.86 → 0.20 s | 29.4 → 11.0 MB | 68.3 → 19.2 MB |
| Django `docs/`, documents only | 0.20 → 0.05 s | 14.0 → 7.8 MB | 33.2 → 16.0 MB |
| tsai-rs (403 files) | 0.16 → 0.03 s | 8.3 → 6.1 MB | 117.5 → 14.3 MB |
| this repo (41 files) | 0.05 → 0.04 s | 5.2 → 4.4 MB | 13.2 → 12.6 MB |

**Concurrent lane reranks.** The rerank-only cold cost of a mixed-corpus
query dropped from 0.68 s to 0.42 s (6 interleaved runs each, ranges
disjoint). The A/B deleted only the cached Choice responses, so each run
billed exactly the 2 rerank requests in both versions.

**Exactness evidence.**

- **Tokenizer:** identical token streams against the previous tokenizer on
  4,927 files (5.1 M tokens) and on 300 k Unicode fuzz strings (final sigma,
  İ, K, combining marks, non-ASCII digits).
- **Rankings:** identical full uncapped `top()` rankings for 2,138 query
  variants, both serial and parallel builds.
- **`git grep`:** identical walked-filtered hits in 324 comparisons, including
  subdirectory roots and option-like or regex keywords.
- **Walk:** identical output on edge-case trees (symlinks named like skipped
  directories, skip-named or symlinked roots).
- **End to end:** byte-identical stdout, stderr and exit codes on 8 queries
  across every output mode, lane mix, `-k`, single-file and `--no-grep-union`.
  The replay used a warm cache and an invalid API key and made 0 network
  requests, which proves every request body and cache key is unchanged.

**Measured and not adopted.**

- **Full LTO with `codegen-units = 1`:** 0.9 MB lower RSS (−7%) and a
  2.1 MB smaller binary, but incremental `cargo test --release` rose from
  2.6 s to 24.8 s.
- **Thin LTO:** no gain.
- **Parallel walk:** −15 ms only, and it changes how walk errors propagate.
- **The unused rustls stack was removed instead.** It was compiled in but
  never selected, because reqwest chooses native-tls whenever its default TLS
  feature is on. The binary is 21% smaller and TLS behavior is unchanged.
- **Switching the TLS backend:** measured with a cold burst of 32 concurrent
  requests and with client construction time. native-tls stays.
  - HTTP/2, via native-tls ALPN or rustls, did not lower latency: medians of
    464–493 ms for all four setups, with overlapping ranges.
  - rustls with its bundled Mozilla roots peaked about 3 MB lower, but it
    ignores the OS trust store, so corporate TLS-inspection CAs would fail.
  - rustls with OS roots takes 119 ms to build the client on every
    invocation, against 3 ms for native-tls.

## 10. Token, re-billing, timeout and cache suggestions

Four suggestions were tested on 2026-10-11 for $0.06 of API usage in total.
Two changes were adopted: an HTTP read timeout, and refreshing a cache
entry's modification time on a hit so that pruning by age is safe. Token counts come from the responses'
`usage.input_tokens`, which was identical across repeated requests. "Free"
means a cache replay with an invalid API key and 0 network requests.

### Shorter question text: not adopted

Billed input tokens per question, measured by adding and removing questions
over a fixed state:

| Billed item | Code lane | Document lane |
|---|---|---|
| Window question (instruction and criteria) | 114 | 109 |
| ... of which the criteria | 59 | 55 |
| File question (asked twice per request) | 93 | 89 |
| Question key | 0 (keys are not billed) | 0 |
| State `id` field, per window | 10 | 10 |

- **Share of the bill.** Question text is 19% of a 1-window code request
  (3.5 KB window), 12% of a full 6-window code request (7,054 tokens), 29% of
  a typical BEIR document request and 38% of a 0.5 KB document. Across the 487k verification responses in the author's
  cache, it is 17.2% of 1.0 B billed tokens: 8.2% for window questions and
  9.0% for the file-question pair.
- **Shorter instructions** save at most about 2% of tokens, and dropping the
  state `id` field less than 1.3%. Neither justifies invalidating every
  cached response.
- **Dropping the file-question pair** from 1-window requests would save about
  18% of their tokens but changes results. Replayed over 388k cached 1-window
  responses, it loses 6.9% of the files kept at the 0.90 code threshold and
  18.8% at the 0.60 document threshold. Choosing one file question instead of
  the other flips only 0.05% and 0.31% of decisions.
- **Dropping the criteria in the document lane** would save about 17% of a
  BEIR document request. It failed a paired screen of 549 BEIR (query,
  document) pairs taken from the baseline top-10, where each request carried
  the production questions, an identical copy of them (the draw-noise floor)
  and the criteria-free set:
  - The criteria-free score moved 0.061 on average, against 0.006 for the
    copy. It rose by 0.059 on average, more for non-relevant documents.
  - It agreed with production at the 0.60 threshold 94.5% of the time,
    against 99.8% for the copy, and kept 11% more documents.
  - Pooled AUC changed by −0.0065 (95% CI −0.015 to +0.002).

  The criteria carry the document threshold's calibration. In the code lane
  the same change would save 6.7% of a 6-window request and 11.4% of a
  1-window one. It was not screened there, because the code criteria spell
  out the "dedicated supporting code" boundary behind the measured phrasing
  (§4).

### Content-anchored windows: not adopted

The concern was that windows sit at fixed line offsets, so one inserted line
re-bills the whole file. A simulation with jevr's own windowing code replayed
1,813 file edits from the last 200 commits of four repositories (this one,
tsai-rs, cross-market-arb and Django), without API calls.

- An edit re-bills 80% of the edited file's cold cost, and 100% for files
  that fit in one request (38% of edits). The concern is real.
- A scheme that re-billed only the windows touching changed lines would cut
  those bytes by 78%. In money this is about 16k tokens per edited file:
  $0.0007 on average, $0.0002 median, $0.0015 at p90.
- Any such scheme must drop line numbers from the request state and replace
  the measured 100/20 windowing, which means re-running both gates. That is
  not worth fractions of a cent per edit.

### HTTP read timeout: adopted

The HTTP client set no timeouts, so a stalled connection could hang a query
forever. Local mock servers reproduced each stall mode against jevr's exact
reqwest build (0.12.28):

| Stall mode | No timeout (0.2.0) | `connect_timeout` | `read_timeout` | Total `timeout` |
|---|---|---|---|---|
| Unroutable address | hangs | ends | ends | ends |
| Server accepts and never answers | hangs | hangs | ends | ends |
| Server reads the request and never answers | hangs | hangs | ends | ends |
| Headers sent, then the body stalls | hangs | hangs | ends | ends |
| Upload never read (100 KB and 8 MB) | hangs | hangs | ends | ends |
| Slow but steadily progressing response | completes | completes | completes | cut |

- **`read_timeout` alone ends every stall** without cutting a response that
  is still progressing. It is set to 10 s, the official SDKs' per-operation
  default.
- **10 s is far above real latency.** The slowest response measured was
  0.74 s. A burst of 32 concurrent 24 KB requests had a median of 0.42 s and
  a maximum of 0.54 s, and a 60-option rerank took 0.43 s.
- **Timeouts are retried.** A timeout surfaces as a transport error, so a
  request that stalls on every attempt now fails after about 42 s (4 attempts
  plus backoff) with a warning, and the rest of the query continues. A stall
  after the response headers fails that request without a retry, as any body
  error already did.
- **End to end,** through a proxy that never answers, jevr 0.2.0 was still
  hanging after 75 s. The new build exited after 42.05 s with a warning.
- **Requests are unchanged.** Warm replays of 3 code queries and all 623 BEIR
  queries were byte-identical to 0.2.0, with 0 network requests (nDCG@10
  0.778 / 0.363).

### Cache disk usage: touch on hit adopted, packed store not adopted

The author's cache held 496,888 entries: 110 MB of content taking 2.04 GB on
disk, because every file occupies at least one 4 KiB block on APFS (ext4
behaves the same; btrfs stores files this small inline). Nothing was ever
evicted.

A packed SQLite store was prototyped in Python and in Rust. It used bundled
SQLite in WAL mode, a 32-byte key as the primary key, and a fallback that
reads the old per-file entries.

| Measure | Files (0.2.0) | SQLite store |
|---|---|---|
| Disk for 496,888 entries | 2.04 GB | 148 MB (13.75× smaller) |
| Lookup, page cache warm (p50) | 15.5 µs | 4.5 µs |
| Write (p50) | 275 µs | 31.5 µs |
| Release binary | 6.67 MB | 8.53 MB (+1.87 MB) |
| Peak RSS, warm query | 12.4–12.6 MB | 14.1–14.4 MB (+1.65–1.77 MB) |
| Warm query (median of 20) | 32.7–35.0 ms | 34.8–36.1 ms |
| 14 concurrent writer processes | – | 0 busy errors, 0 lost writes |

- **The packed store is not adopted.** It costs 1.7 MB of peak memory on a
  12.5 MB process and 1.9 MB of binary.
  - Linking the system SQLite instead adds only 0.07 MB of binary and 0.7 MB
    of memory, but a `cargo install` on Linux would then need the SQLite
    library installed.
  - The store also frees no disk until the old files are deleted, and one
    corrupt database file would affect every entry.
- **Pruning by age is the remedy, but only if "age" means last use.** With
  modification time meaning write time, `-mtime +14` would have deleted
  446,767 entries (1.83 GB), including every entry from the 2026-09-23
  benchmark day, whether or not those entries were still being replayed.
- **The touch costs nothing measurable at once-a-day granularity.** Cost per
  cache hit, over 12,000–15,000 cached files with the page cache warm:

  | Variant | Per hit |
  |---|---|
  | Read only (0.2.0) | 11.9–13.6 µs |
  | Read, then reopen for writing and touch | 47 µs |
  | Touch through the open read handle | 34–39 µs |
  | Touch only when last touched over a day ago (adopted) | 12.9–13.6 µs |

  With the adopted variant, warm queries gave identical output in 60 of 60
  runs, and their median time moved by −0.05 to +0.33 ms on ~30 ms. Every
  stale entry was refreshed, a same-day repeat hit wrote nothing, and the
  BEIR replays stayed byte-identical. The prune command is documented in
  docs/GUIDE.md and never runs automatically.

## 11. Replay bundles

The §2 SWE-bench, §3 BEIR and §8 NanoRTEB numbers were re-derived from
cached responses alone, and each benchmark's responses were packed into a
bundle that anyone can replay offline (`benchmarks/README.md`, "Verify the
published numbers offline"). The bundles are attached to the v0.2.2 release.

- **Extraction.** A clone of the author's cache, with every modification time
  set to 2000-01-01, was replayed with `--offline`; the touch-on-hit of §10
  refreshed exactly the entries the benchmark used, and the rest were deleted.
  The BEIR responses predate the tool's rename and came from
  `~/.cache/jev-navigator` (43,829 entries), the NanoRTEB responses from
  `~/.cache/jevr` (497,131). The 2026-09-20 SWE-bench runs predate the
  cache, so the benchmark was re-measured live on 2026-10-12 (§2); 1,716 of
  its requests were already in `~/.cache/jev-navigator`.

  | Bundle | Entries | Verifications | Reranks | Billed input tokens |
  |---|---|---|---|---|
  | BEIR | 16,726 | 16,128 | 598 | 22.8 M (36.5 k per query) |
  | SWE-bench Lite | 34,553 | 34,453 | 100 | 200.2 M (4.0 M per instance) |
  | NanoRTEB, three modes | 357,205 | 351,156 | 6,049 | 583.6 M |

- **Replay.** BEIR and SWE-bench replay with 0 requests missing and rankings
  byte-identical to the live runs (BEIR also from a copy unpacked outside any
  git repository, with freshly downloaded and converted data). NanoRTEB
  replays every per-task score in `benchmarks/RTEB.md` but one, Retrieval
  NanoAILACasedocs (28.8, not 27.1; explained there), with 84 / 633 / 452
  requests missing in the retrieval / cap100 / reranking modes. Re-sent live,
  437 of the 452, 71 of the 84 and 564 of the 633 were reranks rejected as
  too large (`HTTP 400 max_tokens_exceeded`; 100 candidates' 25-line heads
  of legal or financial documents reach 350 KB–1 MB); the rest had failed
  transiently in the live run. A rejected or failed request is never cached,
  and jevr falls back the same way offline as it did live, so the replay
  reproduces the live rankings.
- **Live spot-check.** `--spotcheck`; every response reported `jev-1.13.0`.
  Each request was byte-identical to its cached twin, so the differences are
  Jev's draw-to-draw noise:

  | Requests | Sample | Billed | Paired | Mean \|Δp\| | Max \|Δp\| |
  |---|---|---|---|---|---|
  | SciFact verification | 10 queries, seed `spotcheck-2026-10-11` | 542 for both datasets | 300 | 0.0044 | 0.080 |
  | SciFact rerank | | | 10 | 0.0022 | 0.080 |
  | NFCorpus verification | 10 queries | | 202 | 0.0162 | 0.200 |
  | NFCorpus rerank | | | 10 | 0.0040 | 0.120 |
  | SWE-bench verification | 2 instances, seed `spotcheck-2026-10-12` | 306 | 298 | 0.0051 | 0.390 |
  | SWE-bench rerank | | | 4 | 0.0200 | 0.070 |
  | NanoRTEB reranking-mode verification | 10 queries, seed `spotcheck-2026-10-12` | 1,022 | 1,000 | 0.0132 | 0.230 |
  | NanoRTEB reranking-mode rerank | | | 9 | 0.0004 | 0.050 |

  For scale: the two identical file questions of one request differ by
  0.0051 on average across the BEIR bundle, and two unrelated verification
  responses by 0.255 (median 0.110). Responses that did not come from these
  requests would move 16–58× more than the BEIR ones did.
- **Limits.** The replay proves that code, requests and scoring line up; the
  spot-check samples whether the responses are Jev's, and only while
  `jev-1.13.0` is served.
