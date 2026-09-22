//! jevr (Jev Retrieval): three-stage concept-to-files retrieval.
//!
//! Stage 1 generates recall-oriented candidates locally (BM25 +
//! keyword-variant union); Stages 2–3 use TypeSafe Jev for precision-oriented
//! verification and a final listwise ranking.

pub mod bm25;
pub mod candidates;
pub mod config;
pub mod expansion;
pub mod jev;
pub mod output;
pub mod rerank;
pub mod verify;
pub mod walk;
pub mod windows;
