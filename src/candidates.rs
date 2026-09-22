//! Candidate union + git-grep insurance (Stage 1 support).
//!
//! Why a union across variants: each query variant contributes its own BM25
//! top-`cap` list; Stage 2 re-scores every candidate independently, so order
//! is irrelevant and deduplication is all that matters. Why a git-grep union
//! on top: zero-cost insurance for literal-vocabulary queries whose exact
//! terms appear verbatim in the code but rank poorly under BM25 alone.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// Union of per-variant candidate lists and git-grep hits, deduplicated and
/// sorted for deterministic downstream batching. Order is irrelevant here:
/// Stage 2 re-scores every candidate independently.
pub fn union_candidates(variant_lists: &[Vec<String>], grep_hits: &[String]) -> Vec<String> {
    variant_lists
        .iter()
        .flatten()
        .chain(grep_hits.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// `git grep -il <keyword>` per variant keyword, restricted to the walked set
/// (repo-relative paths). grep exit 1 (no match) and any failure are tolerated —
/// this stage is insurance, never a blocker.
pub fn git_grep_hits(
    repo_root: &Path,
    keywords: &[String],
    walked: &BTreeSet<String>,
) -> Vec<String> {
    let mut hits = BTreeSet::new();
    for keyword in keywords {
        let Ok(output) = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["grep", "-il", keyword])
            .output()
        else {
            continue;
        };
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if walked.contains(line) {
                hits.insert(line.to_owned());
            }
        }
    }
    hits.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_deduplicates_across_variants_and_grep() {
        let variants = vec![
            vec!["a.rs".to_string(), "b.rs".to_string()],
            vec!["b.rs".to_string(), "c.rs".to_string()],
        ];
        let grep = vec!["c.rs".to_string(), "d.rs".to_string()];
        assert_eq!(
            union_candidates(&variants, &grep),
            vec!["a.rs", "b.rs", "c.rs", "d.rs"]
        );
    }
}
