//! Stage 2: precision-oriented verification.
//!
//! Why fully concurrent: server latency is 0.3–0.8 s per request independent of batch
//! size up to the 24-window/24 KB caps, so total wall time ≈ ceil(batches /
//! concurrency) × 0.8 s. Why max(window, file) scoring: a file can be relevant while
//! no single window is; the extra whole-file Noul rides the same request for free
//! (questions are parallel; output tokens are free). Why failures degrade: one rare
//! transient must not cost the other files' results.

use crate::jev::{client::JevClient, questions, schemas::*};
use crate::walk::FileKind;
use crate::windows::{window_batches, windows, CodeWindow};
use futures::{stream, StreamExt};
use serde_json::json;
use std::collections::BTreeMap;

/// One request-sized unit of work: ≤24 windows / ≤24 KB from a single file.
/// The kind selects the question set: code membership vs document coverage.
pub struct WindowBatch {
    pub file: String,
    pub kind: FileKind,
    pub windows: Vec<CodeWindow>,
}

/// Split one file's text into request-sized batches of overlapping windows.
pub fn build_batches(file: &str, kind: FileKind, text: &str) -> Vec<WindowBatch> {
    let all = windows(text);
    window_batches(&all)
        .into_iter()
        .map(|chunk| WindowBatch {
            file: file.to_owned(),
            kind,
            windows: chunk.to_vec(),
        })
        .collect()
}

/// Final per-file result after aggregating every batch of that file.
#[derive(serde::Serialize)]
pub struct FileVerdict {
    pub file: String,
    /// max(window nouls, file noul) across all batches.
    pub score: f64,
    /// (start_line, end_line, noul) of the best-scoring window, for --snippets.
    pub best_window: Option<(usize, usize, f64)>,
    /// Snippet text of the best-scoring window.
    pub best_snippet: Option<String>,
    /// Set only when every batch for the file failed.
    pub error: Option<String>,
}

struct BatchOutcome {
    file: String,
    score: f64,
    best: Option<(usize, usize, f64, String)>,
    error: Option<String>,
}

/// Verify all batches concurrently; per-batch failures degrade to warnings and
/// an errored verdict only when a file has no successful batch at all.
pub async fn verify_files(
    client: &JevClient,
    model: &str,
    concept: &str,
    batches: Vec<WindowBatch>,
    concurrency: usize,
) -> Vec<FileVerdict> {
    let outcomes: Vec<BatchOutcome> = stream::iter(batches)
        .map(|batch| async move { run_batch(client, model, concept, batch).await })
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    aggregate(outcomes)
}

async fn run_batch(
    client: &JevClient,
    model: &str,
    concept: &str,
    batch: WindowBatch,
) -> BatchOutcome {
    // Per-kind builders: code files keep the measured membership questions and
    // the `code` state key byte-for-byte; documents get coverage phrasing under
    // a `text` key so the instructions stay literally true about their state.
    let is_doc = batch.kind == FileKind::Doc;
    let window_question: fn(usize) -> Question = if is_doc {
        questions::doc_window_membership
    } else {
        questions::window_membership
    };
    let file_question: fn() -> Question = if is_doc {
        questions::doc_file_membership
    } else {
        questions::file_membership
    };
    let content_key = if is_doc { "text" } else { "code" };
    let mut questions: BTreeMap<String, Question> = batch
        .windows
        .iter()
        .enumerate()
        .map(|(i, _)| (format!("window_{i}"), window_question(i)))
        .collect();
    // The file-level question is asked twice and averaged: identical questions
    // in one request return independent draws (measured Δ 0.01–0.03 between
    // draws), so averaging two cuts threshold-adjacent flicker by √2 for ~60
    // input tokens.
    questions.insert("file".into(), file_question());
    questions.insert("file_b".into(), file_question());
    let request = JevRequest {
        state: json!({
            "concept": concept,
            "file": batch.file,
            "windows": batch.windows.iter().enumerate().map(|(i, w)| json!({
                "id": format!("window_{i}"),
                "start_line": w.start,
                "end_line": w.end,
                content_key: w.snippet,
            })).collect::<Vec<_>>(),
        }),
        model: model.into(),
        questions,
    };
    match client.system_one(&request).await {
        Ok(resp) => into_outcome(batch, &resp),
        Err(e) => BatchOutcome {
            file: batch.file,
            score: 0.0,
            best: None,
            error: Some(e.to_string()),
        },
    }
}

fn into_outcome(batch: WindowBatch, resp: &JevResponse) -> BatchOutcome {
    let noul = |key: &str| match resp.answers.get(key) {
        Some(Answer::Noul { noul }) => *noul,
        _ => 0.0,
    };
    let mut best: Option<(usize, usize, f64, String)> = None;
    for (i, window) in batch.windows.iter().enumerate() {
        let p = noul(&format!("window_{i}"));
        if best.as_ref().is_none_or(|(_, _, b, _)| p > *b) {
            best = Some((window.start, window.end, p, window.snippet.clone()));
        }
    }
    let window_best = best.as_ref().map_or(0.0, |(_, _, p, _)| *p);
    let file_score = (noul("file") + noul("file_b")) / 2.0;
    BatchOutcome {
        file: batch.file,
        score: window_best.max(file_score),
        best,
        error: None,
    }
}

fn aggregate(outcomes: Vec<BatchOutcome>) -> Vec<FileVerdict> {
    let mut per_file: BTreeMap<String, FileVerdict> = BTreeMap::new();
    for outcome in outcomes {
        if let Some(error) = &outcome.error {
            eprintln!("jevr: warning: {}: {error}", outcome.file);
        }
        let verdict = per_file
            .entry(outcome.file.clone())
            .or_insert_with(|| FileVerdict {
                file: outcome.file.clone(),
                score: 0.0,
                best_window: None,
                best_snippet: None,
                error: None,
            });
        match outcome.error {
            Some(error) => {
                // Record the failure; overwritten below if any batch succeeds.
                if verdict.best_window.is_none() {
                    verdict.error = Some(error);
                }
            }
            None => {
                verdict.error = None;
                verdict.score = verdict.score.max(outcome.score);
                if let Some((start, end, p, snippet)) = outcome.best {
                    if verdict.best_window.is_none_or(|(_, _, b)| p > b) {
                        verdict.best_window = Some((start, end, p));
                        verdict.best_snippet = Some(snippet);
                    }
                }
            }
        }
    }
    per_file.into_values().collect()
}
