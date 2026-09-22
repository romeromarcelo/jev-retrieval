//! Configuration-driven design: all tunables in one struct.
//!
//! Precedence: defaults < YAML file (.jevr.yaml in the searched repo
//! root, else ~/.config/jevr.yaml) < CLI flags. Defaults are the
//! measured operating points (docs/BENCHMARKS.md), and the model is
//! PINNED — calibration shifts between Jev versions would silently
//! invalidate the thresholds.

use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Pinned, not jev-latest: threshold calibration is version-specific.
    pub model: String,
    /// Keep files with score ≥ threshold. The pipeline takes the max over
    /// every window of a file plus a whole-file Noul, which inflates scores
    /// relative to a single whole-file judgment; the measured operating
    /// plateau is 0.85–0.92 with the optimum at 0.90 (docs/BENCHMARKS.md).
    pub threshold: f64,
    /// Keep documents (FileKind::Doc) with score ≥ doc_threshold. The 0.90 code
    /// threshold over-rejects prose: on BEIR SciFact/NFCorpus (623 queries) it
    /// left 45–65% of queries with an empty kept set while macro-F1 peaked in
    /// the 0.55–0.65 band on both datasets (docs/BENCHMARKS.md). 0.60 keeps "more likely
    /// relevant than not" semantics comfortably above the Noul 0.5 coin-flip
    /// floor. An explicit --threshold overrides both thresholds.
    pub doc_threshold: f64,
    /// Document-lane ordering fusion: documents sort by
    /// score + doc_rank_weight × rank_probability (code keeps pure rerank
    /// order). A single listwise Choice is winner-take-all — sharp when one
    /// file holds the answer (code, SciFact-style queries) but poorly resolved
    /// below the winner when many documents are relevant. Fusing the calibrated
    /// verify score with the rerank probability measured +0.007 SciFact nDCG@10
    /// and +0.002 NFCorpus, flat across w ∈ [0.5, 2.0] (docs/BENCHMARKS.md);
    /// w = 1.0 is the plateau center. 0 disables fusion (pure score order for
    /// documents).
    pub doc_rank_weight: f64,
    /// Stage 1 per-variant cap, applied before the candidate union.
    pub candidate_cap: usize,
    /// Concurrent Stage 2 requests. Measured on the heaviest benchmark query:
    /// 12.6 s at 8, 3.7 s at 32, plateau by 32–48, no 429s observed — the
    /// server tolerates many in-flight requests.
    pub concurrency: usize,
    /// When nothing clears the threshold, return this many best-scoring
    /// verdicts marked `fallback` instead of silence (0 disables). Out-of-
    /// envelope queries (bug reports) often score everything below threshold
    /// while still ranking the right file first.
    pub empty_fallback: usize,
    /// Output cap, applied per result lane (code and documents) in every
    /// output mode.
    pub top_k: usize,
    /// Max lines printed per snippet in --snippets (the full window stays in
    /// --json). Windows run up to 100 lines, and agent harnesses truncate long
    /// tool output wholesale: two document results measured 22 KB / 189 lines,
    /// so ten would silently lose the tail. 25 lines matches the rerank head
    /// size — enough to judge relevance and harvest symbols for grep.
    pub snippet_lines: usize,
    /// git grep union as literal-vocabulary insurance for queries whose exact
    /// terms appear verbatim in the code.
    pub git_grep_union: bool,
    /// BM25 term-frequency saturation. Code-search tuning evidence puts the
    /// useful grid at k1 ∈ [0.7, 1.3] (Zhang et al., NLP4Prog 2021).
    pub bm25_k1: f64,
    /// BM25 length normalization; grid b ∈ [0.7, 1.0] for code corpora.
    pub bm25_b: f64,
    /// Client-side Jev response cache under ~/.cache/jevr/ (TypeSafe
    /// bills identical resent states in full — no server-side caching exists).
    /// Keys include file content, so entries never go stale; disable with
    /// --no-cache when a fresh draw is wanted despite identical inputs.
    pub cache: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "jev-1.13.0".into(),
            threshold: 0.90,
            doc_threshold: 0.60,
            doc_rank_weight: 1.0,
            candidate_cap: 30,
            concurrency: 32,
            empty_fallback: 3,
            top_k: 10,
            snippet_lines: 25,
            git_grep_union: true,
            bm25_k1: 1.2,
            bm25_b: 0.75,
            cache: true,
        }
    }
}

impl Config {
    /// Load defaults overlaid with the first YAML file found: an explicit
    /// --config path (required to exist), the repo-root dotfile, or the
    /// user-level config.
    pub fn load(repo_root: &Path, explicit: Option<&Path>) -> Result<Self> {
        if let Some(path) = explicit {
            return Self::from_yaml(path);
        }
        let repo_file = repo_root.join(".jevr.yaml");
        if repo_file.exists() {
            return Self::from_yaml(&repo_file);
        }
        if let Some(home) = std::env::var_os("HOME") {
            let user_file = Path::new(&home).join(".config/jevr.yaml");
            if user_file.exists() {
                return Self::from_yaml(&user_file);
            }
        }
        Ok(Self::default())
    }

    fn from_yaml(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parse config {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_measured_operating_points() {
        let config = Config::default();
        assert_eq!(config.model, "jev-1.13.0");
        assert_eq!(config.threshold, 0.90);
        assert_eq!(config.doc_threshold, 0.60);
        assert_eq!(config.doc_rank_weight, 1.0);
        assert_eq!(config.candidate_cap, 30);
        assert_eq!(config.concurrency, 32);
        assert_eq!(config.empty_fallback, 3);
        assert_eq!(config.snippet_lines, 25);
        assert!(config.git_grep_union);
        assert!(config.cache);
    }

    #[test]
    fn yaml_overlay_replaces_only_present_fields() {
        let dir = std::env::temp_dir().join(format!("jevr-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".jevr.yaml"), "threshold: 0.7\n").unwrap();
        let config = Config::load(&dir, None).unwrap();
        assert_eq!(config.threshold, 0.7);
        assert_eq!(config.model, "jev-1.13.0");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
