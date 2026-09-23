//! jevr (Jev Retrieval): three-stage concept-to-files retrieval.
//!
//! Stage 1 generates recall-oriented candidates locally (BM25 +
//! keyword-variant union); Stages 2–3 use TypeSafe Jev for precision-oriented
//! verification and a final listwise ranking.

/// Stage 1: per-kind BM25 indexes over subtoken-aware code tokens.
pub mod bm25;
/// Stage 1: BM25 ∪ git-grep candidate union, capped per variant and lane.
pub mod candidates;
/// All tunables in one struct; precedence defaults < YAML < CLI flags.
pub mod config;
/// Stage 1: query keyword-variant expansion feeding the BM25 passes.
pub mod expansion;
/// TypeSafe Jev API layer: client, question builders, wire schemas.
pub mod jev;
/// Output renderers: `path:start-end  score`, `--snippets`, `--json`.
pub mod output;
/// Stage 3: one listwise Choice per lane orders the kept files.
pub mod rerank;
/// Stage 2: concurrent per-file verification against Noul questions.
pub mod verify;
/// Gitignore-aware walk and the code/document `FileKind` split.
pub mod walk;
/// 100/20-line windowing under the 24-window / 24 KB request envelope.
pub mod windows;
