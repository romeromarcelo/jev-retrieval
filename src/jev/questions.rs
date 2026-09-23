//! Question builders.
//!
//! Why this exact phrasing: subsystem-membership phrasing with the concept
//! referenced by backticked state path scores dedicated support files
//! 0.84–0.87, where a naive question interpolating the concept scored the
//! same files 0.05–0.21 — with an unmoved negative control (docs/BENCHMARKS.md).
//! The concept string lives in `state`, never inside `instructions` — Jev
//! reads literally, and interpolating an interrogative sentence malforms the
//! condition.
//!
//! Documents get their own phrasing: the code questions under-score directly
//! relevant prose (0.88/0.66 measured, below the 0.90 threshold), while the
//! coverage phrasing separates cleanly — 0.97–0.98 relevant vs 0.01–0.06
//! incidental/unrelated (docs/BENCHMARKS.md). Doc windows travel under a
//! `text` state key so the instructions never call prose "code".

use super::schemas::{NoulCriteria, Question};
use std::collections::BTreeMap;

/// Per-window code Noul: subsystem membership of one window, dedicated
/// supporting code included.
pub fn window_membership(window_index: usize) -> Question {
    Question::Noul {
        instructions: format!(
            "Is the code in `windows[{window_index}].code` from the file at `file` part of \
             the subsystem responsible for `concept`, either implementing it directly or \
             serving as its dedicated supporting code (features, buffers, configuration, \
             data access)?"
        ),
        criteria: Some(membership_criteria()),
    }
}

/// File-level code Noul over all windows shown; averaged into the file score
/// alongside the per-window questions to cut flicker.
pub fn file_membership() -> Question {
    Question::Noul {
        instructions: "Considering all windows shown, is the file at `file` part of the \
                       subsystem responsible for `concept`, including its dedicated \
                       supporting code?"
            .into(),
        criteria: Some(membership_criteria()),
    }
}

fn membership_criteria() -> NoulCriteria {
    NoulCriteria {
        true_means: "Implements the concept, or is configuration/data/feature code that \
                     exists specifically to support it"
            .into(),
        false_means: "Merely imports, logs, mentions it in a comment, or touches the \
                      concept incidentally while doing something else"
            .into(),
    }
}

/// Per-window document Noul: the coverage phrasing, which never calls prose
/// "code" (see the module docs for the measured separation).
pub fn doc_window_membership(window_index: usize) -> Question {
    Question::Noul {
        instructions: format!(
            "Is the text in `windows[{window_index}].text` from the document at `file` part \
             of the material that covers `concept`, either addressing it directly or serving \
             as its dedicated supporting content (definitions, examples, configuration, \
             references)?"
        ),
        criteria: Some(doc_membership_criteria()),
    }
}

/// File-level document Noul over all windows shown, mirroring
/// [`file_membership`] for the document lane.
pub fn doc_file_membership() -> Question {
    Question::Noul {
        instructions: "Considering all windows shown, is the document at `file` part of the \
                       material that covers `concept`, including its dedicated supporting \
                       content?"
            .into(),
        criteria: Some(doc_membership_criteria()),
    }
}

fn doc_membership_criteria() -> NoulCriteria {
    NoulCriteria {
        true_means: "Covers the concept, or is a definition/example/configuration/reference \
                     that exists specifically to support it"
            .into(),
        false_means: "Merely mentions it in passing, or touches the concept incidentally \
                      while covering something else"
            .into(),
    }
}

/// Listwise rerank: one Choice whose options are candidate file ids — a
/// single ~0.3 s call ranks the whole kept set; probabilities are the rank
/// scores.
/// `any_doc` swaps "implements" for the content-neutral "contains" so prose
/// candidates are not judged as implementations; all-code requests keep the
/// original wording byte-for-byte (cache keys and measured behavior unchanged).
pub fn listwise_rank(file_ids: impl Iterator<Item = String>, any_doc: bool) -> Question {
    let instructions = if any_doc {
        "Which file in `files` most likely contains what `concept` describes?"
    } else {
        "Which file in `files` most likely implements what `concept` describes?"
    };
    Question::Choice {
        instructions: instructions.into(),
        criteria: file_ids.map(|id| (id, None)).collect::<BTreeMap<_, _>>(),
    }
}
