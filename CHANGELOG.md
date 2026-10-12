# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] - 2026-10-12

### Added

- `JEVR_OFFLINE=1` makes a request missing from the response cache fail
  instead of reaching the network, so a replay never bills; `JEVR_DEBUG=1`
  still counts the miss.
- Benchmark harnesses for SWE-bench Lite (`benchmarks/swebench.py`) and
  HAKARI-Bench NanoRTEB (`benchmarks/rteb.py`), next to the BEIR `run.py`.
  Each keeps its own response cache in `benchmarks/cache/` and reports
  billed requests and warnings. `--offline` replays a published bundle, or
  prices an experiment before it runs; `--spotcheck` re-sends a seeded random
  sample live and compares every answer with the bundle's.
- Replay bundles for all three benchmarks, attached to this release with
  their SHA-256 checksums (benchmarks/README.md, docs/BENCHMARKS.md §11).

### Changed

- SWE-bench Lite re-measured on the current pipeline, now replayable:
  Recall@1 0.720, any-gold@10 0.900, all-gold@10 0.900, MRR 0.810 (2026-09-20:
  0.760 / 0.900 / 0.900 / 0.827).

## [0.2.1] - 2026-10-11

### Changed

- A cache hit refreshes the entry's modification time (at most once a day),
  so `find ~/.cache/jevr -name '*.json' -mtime +N -delete` prunes only entries
  unused for N days (docs/GUIDE.md). Output and cache keys are unchanged, and
  warm-query latency is unchanged within noise.

### Fixed

- A stalled connection could hang a query indefinitely because the HTTP client
  set no timeouts. A 10 s read timeout (per socket read, the official SDKs'
  default) now ends every stall, and the request is retried like any other
  transport error. Request bodies and cache keys are unchanged.

## [0.2.0] - 2026-10-11

### Changed

- Stage 1 is ~4× faster with ~3× less memory, and its output is
  byte-identical. BM25 keeps postings only for query terms, tokenizes without
  per-token allocations and scans files on every core. `git grep` runs in the
  background, limited to searchable extensions. On a 4k-file repo, a warm
  query now takes 0.20 s instead of 0.86 s (docs/BENCHMARKS.md §9).
- On mixed corpora the code and document reranks now run concurrently, so a
  cold query pays one round trip instead of two (0.68 s → 0.42 s).
- Dropped the unused rustls stack. reqwest always selected native-tls, so TLS
  behavior is unchanged and the binary is 21% smaller.
- **Breaking (library API):** `bm25::Bm25Index::build` and `build_from_files`
  now take the query variants. `candidates::git_grep_hits` now joins a search started by
  the new `candidates::spawn_git_grep`.

### Added

- `bm25::for_each_token`, `walk::CODE_EXTENSIONS`, `walk::DOC_EXTENSIONS`.

## [0.1.1] - 2026-09-23

### Added

- API documentation for every public item (module declarations, request
  windowing constants, wire-format types, question builders, client entry
  points), completing docs.rs coverage of the public surface.

### Changed

- crates.io keywords: replaced `grep` with `jev` so the crate surfaces on the
  ecosystem keyword page; the grep-shaped interface remains described in the
  crate description and README.

## [0.1.0] - 2026-09-22

First public release, published to [crates.io](https://crates.io/crates/jevr).

### Added

- Three-stage concept-to-files retrieval pipeline: local stateless BM25
  candidates (code-aware tokenizer, per-kind indexes, git-grep union),
  concurrent TypeSafe Jev verification over overlapping 100/20-line windows,
  and a listwise rerank per result lane.
- Dual-lane search over source code **and** plain-text documents (md, rst,
  txt, html, org, tex, ...), each lane with its own calibrated question set,
  threshold (0.90 code / 0.60 documents), rerank, and `top_k` cap.
- Grep-shaped CLI: positional query + optional PATH (directory or single
  file), grep-honest exit codes (`0` matches, `1` none above threshold, `2`
  error), and `--paths-only` / `--snippets` / `--json` output modes.
- Client-side response cache under `~/.cache/jevr/`, keyed by
  sha256(request body) so entries can never go stale; warm repeats are free
  and bill zero network requests.
- Configuration via `.jevr.yaml` / `~/.config/jevr.yaml` / CLI flags, with
  every default a measured, benchmark-derived operating point.
- Claude Code skill and plugin (`./install.sh`,
  `/plugin install jevr@jevr`) teaching agents to use `jevr` for
  conceptual searches.
- Reproducible BEIR benchmark harness (`benchmarks/`) and the full evidence
  record (`docs/BENCHMARKS.md`): 0.900 any-gold@10 on SWE-bench Lite file
  localization, nDCG@10 0.778 on BEIR SciFact and 0.363 on NFCorpus.

[Unreleased]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/romeromarcelo/jev-retrieval/releases/tag/v0.1.0
