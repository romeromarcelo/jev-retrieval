//! Output renderers.
//!
//! The default mode prints `path:start-end  score` lines: the line range of the
//! best-matching window makes each result directly actionable for an agent
//! (open the file at that range) without flooding its context window.
//! --snippets adds the window body with rg-style per-line numbers; --json emits
//! the full verdicts for programmatic use. `top_k` caps every mode.

use crate::verify::FileVerdict;
use std::io::{self, Write};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    PathsOnly,
    Snippets,
    Json,
}

/// A kept file in final order: its verdict plus the Stage 3 rank probability
/// (None when the rerank call failed and order degraded to Noul score).
#[derive(serde::Serialize)]
pub struct RankedFile {
    #[serde(flatten)]
    pub verdict: FileVerdict,
    pub rank_probability: Option<f64>,
    /// True when nothing cleared the threshold and this is a best-effort
    /// low-confidence result (see Config::empty_fallback).
    pub fallback: bool,
}

/// Render results to stdout. A closed pipe (e.g. `| head`) is a normal way
/// for a caller to stop reading, so BrokenPipe ends rendering silently
/// instead of panicking like `println!` would. `snippet_lines` caps each
/// printed snippet in --snippets so a result set can't flood a calling
/// agent's tool-output budget (windows run up to 100 lines).
pub fn render(
    mode: Mode,
    ranked: &[RankedFile],
    top_k: usize,
    snippet_lines: usize,
) -> anyhow::Result<()> {
    let stdout = io::stdout();
    match write_results(&mut stdout.lock(), mode, ranked, top_k, snippet_lines) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

fn write_results(
    out: &mut impl Write,
    mode: Mode,
    ranked: &[RankedFile],
    top_k: usize,
    snippet_lines: usize,
) -> io::Result<()> {
    match mode {
        Mode::PathsOnly => {
            for r in ranked.iter().take(top_k) {
                match r.verdict.best_window {
                    Some((start, end, _)) => writeln!(
                        out,
                        "{}:{start}-{end}  {:.2}",
                        r.verdict.file, r.verdict.score
                    )?,
                    None => writeln!(out, "{}  {:.2}", r.verdict.file, r.verdict.score)?,
                }
            }
        }
        Mode::Snippets => {
            for r in ranked.iter().take(top_k) {
                match (&r.verdict.best_window, &r.verdict.best_snippet) {
                    (Some((start, end, _)), Some(snippet)) => {
                        writeln!(
                            out,
                            "{}:{start}-{end}  {:.2}",
                            r.verdict.file, r.verdict.score
                        )?;
                        let total = snippet.lines().count();
                        for (offset, line) in snippet.lines().take(snippet_lines).enumerate() {
                            writeln!(out, "{}: {line}", start + offset)?;
                        }
                        // Unnumbered marker so an agent knows the range holds
                        // more than what printed and can Read the full span.
                        if total > snippet_lines {
                            writeln!(
                                out,
                                "... {} more lines through {end} (Read {}:{}-{end} for the rest)",
                                total - snippet_lines,
                                r.verdict.file,
                                start + snippet_lines,
                            )?;
                        }
                        writeln!(out)?;
                    }
                    _ => writeln!(out, "{}  {:.2}\n", r.verdict.file, r.verdict.score)?,
                }
            }
        }
        Mode::Json => {
            let shown: Vec<&RankedFile> = ranked.iter().take(top_k).collect();
            let json = serde_json::to_string_pretty(&shown).map_err(io::Error::other)?;
            writeln!(out, "{json}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked(snippet: &str) -> Vec<RankedFile> {
        vec![RankedFile {
            verdict: FileVerdict {
                file: "src/lib.rs".into(),
                score: 0.95,
                best_window: Some((10, 10 + snippet.lines().count() - 1, 0.95)),
                best_snippet: Some(snippet.into()),
                error: None,
            },
            rank_probability: None,
            fallback: false,
        }]
    }

    fn snippets_output(snippet: &str, cap: usize) -> String {
        let mut buf = Vec::new();
        write_results(&mut buf, Mode::Snippets, &ranked(snippet), 10, cap).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn long_snippets_are_capped_with_a_read_pointer() {
        let snippet: String = (0..40).map(|i| format!("line{i}\n")).collect();
        let out = snippets_output(snippet.trim_end(), 3);
        assert!(out.contains("10: line0"));
        assert!(out.contains("12: line2"));
        assert!(!out.contains("13: line3"));
        assert!(out.contains("... 37 more lines through 49 (Read src/lib.rs:13-49 for the rest)"));
    }

    #[test]
    fn short_snippets_print_whole_without_a_marker() {
        let out = snippets_output("only\ntwo", 25);
        assert!(out.contains("10: only"));
        assert!(out.contains("11: two"));
        assert!(!out.contains("more lines"));
    }
}
