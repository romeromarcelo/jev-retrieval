//! Candidate union + git-grep insurance (Stage 1 support).
//!
//! Why a union across variants: each query variant contributes its own BM25
//! top-`cap` list; Stage 2 re-scores every candidate independently, so order
//! is irrelevant and deduplication is all that matters. Why a git-grep union
//! on top: insurance for literal-vocabulary queries whose exact terms appear
//! verbatim in the code but rank poorly under BM25 alone. The grep is free;
//! every hit it adds is a verified, billed candidate.

use crate::walk::{CODE_EXTENSIONS, DOC_EXTENSIONS};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::thread::JoinHandle;

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

/// Start `git grep -il <keyword>` per variant keyword on a background thread,
/// so the search overlaps the walk and BM25 scan; [`git_grep_hits`] joins it.
/// Pathspecs limit git to searchable extensions: no other hit can survive the
/// walked-set filter, and skipping lockfiles, translations and data files is
/// most of git grep's cost on large repos. Keywords starting with `-` get no
/// pathspecs: git parses them as options, and a trailing `-- <pathspec>` would
/// turn a pathspec into the pattern.
pub fn spawn_git_grep(repo_root: &Path, keywords: Vec<String>) -> JoinHandle<Vec<String>> {
    let root = repo_root.to_path_buf();
    let pathspecs: Vec<String> = CODE_EXTENSIONS
        .iter()
        .chain(DOC_EXTENSIONS)
        .map(|ext| format!("*.{ext}"))
        .collect();
    std::thread::spawn(move || {
        let mut lines = Vec::new();
        for keyword in &keywords {
            let mut grep = Command::new("git");
            grep.arg("-C").arg(&root).args(["grep", "-il", keyword]);
            if !keyword.starts_with('-') {
                grep.arg("--").args(&pathspecs);
            }
            if let Ok(output) = grep.output() {
                lines.extend(
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(str::to_owned),
                );
            }
        }
        lines
    })
}

/// Hits of a [`spawn_git_grep`] search, restricted to the walked set
/// (repo-relative paths). grep exit 1 (no match) and any failure are
/// tolerated — this stage is insurance, never a blocker.
pub fn git_grep_hits(search: JoinHandle<Vec<String>>, walked: &BTreeSet<String>) -> Vec<String> {
    let lines = search.join().unwrap_or_default();
    let hits: BTreeSet<String> = lines.into_iter().filter(|l| walked.contains(l)).collect();
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
