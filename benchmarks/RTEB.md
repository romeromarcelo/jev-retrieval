# Benchmark — HAKARI-Bench NanoRTEB

Results of running `jevr` on the **NanoRTEB** benchmark of
[HAKARI-Bench](https://github.com/hakari-bench/hakari-bench) (the leaderboard at
[hakari-bench/leaderboard](https://huggingface.co/spaces/hakari-bench/leaderboard)),
run 2026-09-23 with the pinned `jev-1.13.0` model. Three runs are recorded:
both leaderboard modes at defaults, plus a Retrieval re-run with
`candidate_cap: 100` (evidence discussion in
[docs/BENCHMARKS.md §8](../docs/BENCHMARKS.md)).

NanoRTEB is the Nano-scale port of RTEB: 14 English retrieval tasks over
specialized domains — legal (AILACasedocs, AILAStatutes, LegalSummarization),
finance (FinQA, FinanceBench, HC3Finance), healthcare (CUREv1, ChatDoctor) and
code (Apps, DS1000, HumanEval, MBPP, WikiSQL, FreshStack) — with 2,390 judged
queries total and corpora of 82–10,000 documents
([hakari-bench/NanoRTEB](https://huggingface.co/datasets/hakari-bench/NanoRTEB)).

The leaderboard scores every model in two evaluation modes, and jevr maps onto
both natively:

| Leaderboard mode | Meaning | jevr invocation |
|---|---|---|
| **Retrieval** (`score_target='all'`) | search the full task corpus | `jevr "<query>" corpora/<task> --threshold 0 --top-k 10 --json` |
| **Reranking** (`score_target='reranking'`) | reorder a fixed, model-independent candidate set (RRF top-100 of a BM25 + dense hybrid, gold-safeguarded) | same, plus `--candidates <per-query file>` |

## Results (nDCG@10, ×100)

Leaderboard comparison built from the official task-level scores in
[hakari-bench/leaderboard_database](https://huggingface.co/datasets/hakari-bench/leaderboard_database),
with jevr inserted into the pool and every aggregate recomputed by the
leaderboard's own rules (validated to exact equality against the published
NanoRTEB view before use): Borda Score = mean over tasks of
100 × (N − rank)/(N − 1) with competition ranking; the leaderboard's Macro and
Micro Mean both reduce to the plain 14-task mean within a single benchmark.
The Retrieval pool excludes cross-encoder rerankers (they cannot search a full
corpus); the Reranking pool adds the BM25 full-corpus row as the
candidate-order baseline.

### Retrieval — jevr ranks 19 of 77 at defaults, 10 of 77 at `candidate_cap: 100`

At defaults (`candidate_cap` 30):

| Rank | Model | Borda Score | Mean nDCG@10 |
|---|---|---|---|
| 1 | nvidia/Nemotron-3-Embed-8B-BF16 | 99.3 | 80.8 |
| 2 | nvidia/Nemotron-3-Embed-1B-BF16 | 93.3 | 74.9 |
| 3 | voyageai/voyage-4-nano | 92.1 | 74.5 |
| 18 | codefuse-ai/F2LLM-v2-330M | 70.3 | 65.4 |
| **19** | **jevr** | **69.5** | **60.4** |
| 20 | KaLM-embedding-multilingual-mini-instruct-v2.5 | 69.0 | 62.5 |
| 71 | bm25 | 21.0 | 35.5 |
| 77 | sentence-transformers/LaBSE | 3.9 | 28.2 |

With `candidate_cap: 100` (a one-line YAML overlay; every task improved,
mean +7.3):

| Rank | Model | Borda Score | Mean nDCG@10 |
|---|---|---|---|
| 1 | nvidia/Nemotron-3-Embed-8B-BF16 | 99.3 | 80.8 |
| 5 | Qwen/Qwen3-Embedding-8B | 89.1 | 75.0 |
| 9 | perplexity-ai/pplx-embed-v1-0.6b | 85.6 | 71.3 |
| **10** | **jevr, `candidate_cap: 100`** | **82.8** | **67.8** |
| 11 | jinaai/jina-embeddings-v5-text-small | 82.2 | 70.1 |

### Reranking — jevr ranks 2 of 90

| Rank | Model | Borda Score | Mean nDCG@10 |
|---|---|---|---|
| 1 | nvidia/Nemotron-3-Embed-8B-BF16 | 98.9 | 81.1 |
| **2** | **jevr** | **95.1** | **79.0** |
| 3 | nvidia/Nemotron-3-Embed-1B-BF16 | 92.3 | 75.6 |
| 4 | voyageai/voyage-4-nano | 90.9 | 75.1 |
| 5 | Qwen/Qwen3-Embedding-8B | 88.2 | 75.8 |
| 83 | bm25 (candidate order) | 18.8 | 35.5 |
| 90 | sentence-transformers/LaBSE | 4.8 | 31.4 |

### Per-task scores

| Task | Retrieval (cap 30) | Retrieval (cap 100) | Reranking |
|---|---|---|---|
| NanoAILACasedocs | 27.1 | 31.3 | 31.5 |
| NanoAILAStatutes | 38.0 | 60.5 | 60.8 |
| NanoApps | 9.0 | 16.3 | 99.4 |
| NanoCUREv1 | 66.0 | 75.0 | 80.8 |
| NanoChatDoctor | 47.8 | 51.4 | 66.2 |
| NanoDS1000 | 88.9 | 90.6 | 92.3 |
| NanoFinQA | 88.3 | 90.7 | 91.0 |
| NanoFinanceBench | 80.0 | 84.3 | 90.2 |
| NanoFreshStack | 41.3 | 47.3 | 51.2 |
| NanoHC3Finance | 47.8 | 57.9 | 72.7 |
| NanoHumanEval | 78.5 | 91.1 | 100.0 |
| NanoLegalSummarization | 67.0 | 70.4 | 75.4 |
| NanoMBPP | 70.5 | 82.3 | 94.6 |
| NanoWikiSQL | 95.5 | 99.3 | 99.8 |
| **Mean** | **60.4** | **67.8** | **79.0** |

## Reading the split

The three runs isolate jevr's stages. With candidate discovery fixed
(Reranking), Jev verification plus the listwise rerank is second only to an
8B embedder: NanoApps jumps 9.0 → 99.4, and NanoHumanEval reaches a genuine
100.0 (gold ranked first on all 158 queries — verified, not a scoring
artifact). In full-corpus mode the bottleneck is Stage 1's lexical candidate
recall: problem-statement queries share almost no vocabulary with
code-solution corpora (Apps: 8,754 docs), and 223 KB legal case documents
crowd the per-variant slots (AILACasedocs). Where BM25 candidates are
adequate, full-corpus results already sit near the reranking ceiling
(WikiSQL 95.5, DS1000 88.9, FinQA 88.3).

The `candidate_cap: 100` re-run confirms the recall diagnosis directly:
feeding the verifier a deeper Stage-1 pool improved **every one of the 14
tasks** (mean +7.3, largest where lexical recall is worst — AILAStatutes
+22.5, HumanEval +12.6, MBPP +11.8) and moved jevr from #19 to #10 of 77.
The residual Apps gap (16.3 vs 99.4 with oracle-quality candidates) is
pure BM25 recall. Note the contrast with BEIR, where candidate depth 100
bought only +0.011–0.014 nDCG@10 and was rejected as a default
([docs/BENCHMARKS.md §5](../docs/BENCHMARKS.md)) — the payoff of a deeper
pool is proportional to how badly BM25's top 30 misses, so it is a
per-workload configuration choice, not a new default.

All runs clear the BM25 baseline (35.5) by a wide margin.

Two operational facts sit behind these numbers and matter more than rank in
agentic use. First, jevr is the only system in the comparison with **no
corpus-side state**: every other ranked model needs its corpus embedded into
a vector index (rebuilt or resynced whenever a file changes) or a served
cross-encoder — jevr rebuilds BM25 in memory per invocation and was pointed
at each freshly written corpus with zero setup, answering cold queries in
2–3 s even on the 223 KB-document legal task. Second, the client-side
response cache is keyed on request bytes with file content inside the hash:
a warm repeat measured **0.1 s with 0 billed requests**, entries cannot go
stale, and an edit re-bills only the requests whose bytes changed — which is
exactly the repeat-heavy, incrementally-changing access pattern of an
agent's search-edit-search loop.

As always: a single benchmark is never enough — measure on workloads that
resemble yours.

## Reproducing

The protocol is the same as the BEIR harness in this directory:

1. **Convert.** Every document of every
   [hakari-bench/NanoRTEB](https://huggingface.co/datasets/hakari-bench/NanoRTEB)
   `corpus` split becomes one `.txt` file — text plus a trailing newline
   (RTEB corpus rows have no title field). Code tasks included: every task
   runs through the document lane on the raw text all leaderboard models
   receive. Enumerate filenames (`d00000.txt`, …) and keep a stem → corpus-id
   map; RTEB ids are not filename-safe.
2. **Run.** One invocation per judged query:
   `jevr "<query text>" <corpus_dir> --threshold 0 --top-k 10 --json`.
   For the Reranking mode, add `--candidates <file>` where the file lists the
   query's `reranking_hybrid` corpus-ids as corpus-relative paths, one per
   line. For the `candidate_cap` variant, pass `--config` with a YAML
   containing exactly `candidate_cap: 100`.
3. **Score.** nDCG@10 as in `score.py`, with graded gains from the `qrels`
   config (NanoCUREv1 has gains 1 and 2; the rest are binary).
4. **Compare.** Task-level scores for every leaderboard model are in the
   `viewer_task_results` table of
   [hakari-bench/leaderboard_database](https://huggingface.co/datasets/hakari-bench/leaderboard_database)
   (`benchmark='NanoRTEB'`, base rows have `embedding_variant_name IS NULL`;
   `score_target` `'all'` = Retrieval, `'reranking'` = Reranking). Borda
   Score = mean over tasks of 100 × (N − rank)/(N − 1), competition ranking,
   ties share the best rank. The Retrieval pool excludes
   `model_type='reranker'`; the Reranking pool adds bm25's full-corpus row.

A cold pass over both modes issues roughly 300K Jev requests (long legal/code
documents split into multiple verification requests). Responses are cached
client-side under `~/.cache/jevr/`, keyed on the full request body with file
content inside the hash — warm repeats replay in ~0.1 s with zero billed
requests, and the `candidate_cap: 100` re-run only billed the newly admitted
candidate slots because the rebuilt corpora were byte-identical. Pass
`--no-cache` for an independent draw. Single draw — expect per-task noise
near decision boundaries.
