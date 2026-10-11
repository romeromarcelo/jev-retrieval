# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/romeromarcelo/jev-retrieval/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/romeromarcelo/jev-retrieval/releases/tag/v0.1.0
