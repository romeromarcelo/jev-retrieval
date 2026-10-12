# Benchmarks

Reproduce the benchmark numbers cited in the top-level README and
[docs/BENCHMARKS.md](../docs/BENCHMARKS.md): document retrieval on two
standard [BEIR](https://github.com/beir-cellar/beir) datasets (below), code
retrieval on SWE-bench Lite (`swebench.py`, docs/BENCHMARKS.md §2), and the
HAKARI-Bench NanoRTEB leaderboard comparison (`rteb.py`, results in
[RTEB.md](RTEB.md)). Every run can be replayed offline from a published
bundle of its responses.

## Requirements

- `jevr` built and on `PATH` (`cargo build --release`, binary at `target/release/jevr`)
- `TYPESAFE_API_KEY` exported, except for `--offline` replays
- Python 3.9+, `curl`, `unzip`
- For `swebench.py` and `rteb.py`: `git`, and pyarrow to read the datasets'
  Parquet files (`uv run` installs it, or `pip install pyarrow`)

## Steps

```sh
# 1. Download the datasets (md5-verified, idempotent)
./download.sh

# 2. Convert each corpus to one .txt file per document
python3 convert.py data/scifact/corpus.jsonl corpora/scifact
python3 convert.py data/nfcorpus/corpus.jsonl corpora/nfcorpus

# 3. Run jevr over every test query (bills the requests not yet in cache/)
python3 run.py data/scifact/queries.jsonl data/scifact/qrels/test.tsv corpora/scifact scifact.results.jsonl
python3 run.py data/nfcorpus/queries.jsonl data/nfcorpus/qrels/test.tsv corpora/nfcorpus nfcorpus.results.jsonl

# 4. Score
python3 score.py scifact.results.jsonl data/scifact/qrels/test.tsv
python3 score.py nfcorpus.results.jsonl data/nfcorpus/qrels/test.tsv
```

`swebench.py` and `rteb.py` do all four steps in one command and print their
scores. They read their datasets from Hugging Face at pinned revisions
(`princeton-nlp/SWE-bench_Lite`, `hakari-bench/NanoRTEB`); `swebench.py` also
clones the sample's 11 repositories into `repos/` (about 650 MB):

```sh
uv run swebench.py swebench.results.jsonl
uv run rteb.py retrieval rteb-retrieval.results.jsonl   # also: cap100, reranking
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

Each script keeps its own response cache in `cache/<benchmark>/` (`beir`,
`swebench`, `rteb`; git-ignored), separate from `~/.cache/jevr/`. Responses
are keyed on the full request body — the exact byte format `convert.py`
emits (title, blank line, text, trailing newline) is part of that key — so
each request is paid for once: repeat runs are free, and so is every request
an experiment shares with an earlier run. A run ends by printing how many
requests it billed and how many warnings jevr printed; a failed request is
not cached, so rerunning bills only those.

Price an experiment before paying for it — a new build on `PATH`, or a
config in `corpora/<name>/.jevr.yaml` — by running it with `--offline`
first. Nothing is sent: a request missing from the cache fails, and the
count of missing requests is what the live run will send (exact for
verification requests; reranks count at most once per query). A change that
only re-ranks or filters answers already cached, such as `doc_rank_weight`,
is scored for free. Move the cache aside when you want an independent draw
instead of a replay.

## Verify the published numbers offline

Each benchmark's responses are published as a replay bundle: its cache and
results files, attached to the
[v0.2.2 release](https://github.com/romeromarcelo/jev-retrieval/releases/tag/v0.2.2)
with their SHA-256 checksums.

| Bundle | Responses | Billed input tokens | Replays |
|---|---|---|---|
| `beir-replay.tar.gz` | 16,726 | 22.8 M | SciFact 0.778, NFCorpus 0.363 |
| `swebench-replay.tar.gz` | 34,553 | 200.2 M | Recall@1 0.720, any-gold@10 0.900 |
| `rteb-replay.tar.gz` | 357,205 | 583.6 M | means 60.5 / 67.8 / 79.0 ([RTEB.md](RTEB.md)) |

Unpacked here (after BEIR's steps 1 and 2; the other scripts fetch their own
data), a bundle reproduces its rankings without an API key or Jev access:

```sh
base=https://github.com/romeromarcelo/jev-retrieval/releases/download/v0.2.2
curl -fL -O "$base/SHA256SUMS" -O "$base/beir-replay.tar.gz"
shasum -a 256 -c --ignore-missing SHA256SUMS
tar -xzf beir-replay.tar.gz
python3 run.py --offline data/scifact/queries.jsonl data/scifact/qrels/test.tsv corpora/scifact scifact.replay.results.jsonl
cmp scifact.results.jsonl scifact.replay.results.jsonl
python3 score.py scifact.replay.results.jsonl data/scifact/qrels/test.tsv
# same for nfcorpus; for the others:
uv run swebench.py --offline swebench.replay.results.jsonl
cmp swebench.results.jsonl swebench.replay.results.jsonl
uv run rteb.py --offline reranking rteb-reranking.replay.results.jsonl
cmp rteb-reranking.results.jsonl rteb-reranking.replay.results.jsonl
```

`cmp` is the check. `--offline` also prints how many requests were missing:
0 for BEIR and SWE-bench, and 84 / 633 / 452 for the three RTEB modes. Those
are requests the live run never got an answer to, and so never cached: reranks
over long legal and financial documents, which the API rejects as too large
on every attempt, plus a few transient failures. jevr falls back the same way
offline as it did live, so the replay still reproduces the published
rankings. Any other count means this code, data or environment does not
produce the bundle's requests: `~/.config/jevr.yaml`, your global gitignore
and any `.gitignore` above this checkout all decide which files jevr reads.

The replay proves that the code, requests and scores line up, not that the
cached responses came from Jev. `--spotcheck` samples that: pick a seed after
the bundle is published, and it re-sends that random sample of queries live
(billed to your key: 20–32 requests per BEIR query, ~100 per RTEB reranking
query, ~690 per SWE-bench instance) and compares every answer with the
cached answer to the identical request:

```sh
python3 run.py --spotcheck <seed> 10 data/scifact/queries.jsonl data/scifact/qrels/test.tsv corpora/scifact
uv run swebench.py --spotcheck <seed> 2
uv run rteb.py --spotcheck <seed> 10 reranking
```

Genuine responses differ only by Jev's draw-to-draw noise: 0.004–0.016 mean
absolute difference per answer on 10-query BEIR samples, against 0.255 for
unrelated responses (docs/BENCHMARKS.md §11). The check works only while
TypeSafe still serves the bundles' model version (`jev-1.13.0`).
