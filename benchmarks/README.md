# Benchmarks — BEIR SciFact & NFCorpus

Reproduce the document-retrieval numbers cited in the top-level README on two
standard [BEIR](https://github.com/beir-cellar/beir) datasets. The
HAKARI-Bench NanoRTEB results (leaderboard comparison, `candidate_cap`
ablation) live in [RTEB.md](RTEB.md) with their own replication guide.

## Requirements

- `jevr` built and on `PATH` (`cargo build --release`, binary at `target/release/jevr`)
- `TYPESAFE_API_KEY` exported
- Python 3.9+, `curl`, `unzip`

## Steps

```sh
# 1. Download the datasets (md5-verified, idempotent)
./download.sh

# 2. Convert each corpus to one .txt file per document
python3 convert.py data/scifact/corpus.jsonl corpora/scifact
python3 convert.py data/nfcorpus/corpus.jsonl corpora/nfcorpus

# 3. Run jevr over every test query (this is the billed step)
python3 run.py data/scifact/queries.jsonl data/scifact/qrels/test.tsv corpora/scifact scifact.results.jsonl
python3 run.py data/nfcorpus/queries.jsonl data/nfcorpus/qrels/test.tsv corpora/nfcorpus nfcorpus.results.jsonl

# 4. Score
python3 score.py scifact.results.jsonl data/scifact/qrels/test.tsv
python3 score.py nfcorpus.results.jsonl data/nfcorpus/qrels/test.tsv
```

## Datasets

| Dataset | Test queries | Corpus | Download | md5 |
|---|---|---|---|---|
| SciFact | 300 | 5,183 docs | [scifact.zip](https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/scifact.zip) | `5f7d1de60b170fc8027bb7898e2efca1` |
| NFCorpus | 323 | 3,633 docs | [nfcorpus.zip](https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/nfcorpus.zip) | `a89dba18a62ef92f7d323ec890a0d38d` |

## Reference results (nDCG@10)

| System | SciFact | NFCorpus |
|---|---|---|
| **jevr, full pipeline** | **0.778** | 0.363 |
| jevr, Stage 1 only (local BM25) | 0.671 | 0.310 |
| Published BM25 baseline | 0.665 | 0.325 |
| Best verified zero-shot reranker | 0.777 | **0.399** |

jevr edges out the best verified zero-shot reranker on SciFact and clearly
beats the BM25 baseline on both datasets, but it does **not** close the gap to
the best reranker on NFCorpus. And remember: a single benchmark is never
enough — measure on workloads that resemble yours.

## Cost and caching

The run step bills the Jev API per query on a cold cache. Responses are cached
under `~/.cache/jevr/` keyed on the full request body — the exact byte format
`convert.py` emits (title, blank line, text, trailing newline) is part of that
key, so repeat runs over unchanged corpora are free. Pass `--no-cache` (or
clear the cache directory) when you want an independent draw instead of a
replay.
