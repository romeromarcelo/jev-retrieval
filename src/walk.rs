//! Gitignore-aware file walker + extension filters.
//!
//! The skip list exists because dependency and build-output directories
//! dominate raw file counts while contributing zero signal to concept search;
//! the gitignore/hidden filtering behavior is pinned by tests/walk.rs. The
//! extension split matters downstream: a file's `FileKind` selects which Jev
//! question set verifies it (code membership vs document coverage), because
//! the code phrasing measurably under-scores directly relevant prose
//! (docs/BENCHMARKS.md).

use anyhow::Result;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub const SKIP_DIRS: &[&str] = &[
    ".git",
    ".next",
    ".venv",
    "__pycache__",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "target",
    "vendor",
    "venv",
];

/// Content kind of a searchable file: selects the Jev question set that
/// verifies it (Stage 2) and the rerank phrasing (Stage 3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    Code,
    Doc,
}

/// Kind by extension; unknown or missing extensions default to Code so the
/// measured code phrasing keeps covering config-like and extensionless files.
pub fn kind_of(path: &Path) -> FileKind {
    match path.extension() {
        Some(ext) if is_doc_extension(ext) => FileKind::Doc,
        _ => FileKind::Code,
    }
}

/// Walk `root` and return every searchable file (code or document), sorted,
/// honoring gitignore rules and the skip list. `include_hidden` restores
/// dotfiles (still gitignore-filtered).
pub fn searchable_files(root: &Path, include_hidden: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!include_hidden)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true)
        .require_git(false)
        .filter_entry(|entry| !entry.path().is_dir() || !is_skipped_dir(entry.path()));
    for entry in builder.build() {
        let entry = entry?;
        if entry.file_type().is_some_and(|kind| kind.is_file())
            && entry
                .path()
                .extension()
                .is_some_and(|ext| is_code_extension(ext) || is_doc_extension(ext))
        {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

pub fn is_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| SKIP_DIRS.contains(&name))
}

pub fn is_code_extension(extension: &std::ffi::OsStr) -> bool {
    matches!(
        extension.to_str(),
        Some(
            "rs" | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "py"
                | "go"
                | "java"
                | "kt"
                | "kts"
                | "rb"
                | "php"
                | "c"
                | "h"
                | "cpp"
                | "hpp"
                | "cs"
                | "swift"
                | "scala"
                | "sh"
                | "sql"
                | "yaml"
                | "yml"
                | "json"
                | "toml"
        )
    )
}

/// Prose/document extensions: files whose relevance is judged by document
/// coverage rather than subsystem membership. Binary formats (PDF, DOCX) are
/// excluded — no text extraction stage exists, and read_to_string would skip
/// them as unreadable anyway.
pub fn is_doc_extension(extension: &std::ffi::OsStr) -> bool {
    matches!(
        extension.to_str(),
        Some(
            "adoc"
                | "asciidoc"
                | "htm"
                | "html"
                | "markdown"
                | "md"
                | "mdx"
                | "org"
                | "rst"
                | "tex"
                | "text"
                | "txt"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_follows_the_extension_and_defaults_to_code() {
        assert_eq!(kind_of(Path::new("docs/guide.md")), FileKind::Doc);
        assert_eq!(kind_of(Path::new("notes.TXT")), FileKind::Code); // extensions are case-sensitive like grep -r
        assert_eq!(kind_of(Path::new("src/main.rs")), FileKind::Code);
        assert_eq!(kind_of(Path::new("Makefile")), FileKind::Code);
    }
}
