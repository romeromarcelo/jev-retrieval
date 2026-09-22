//! Serde mirror of the TypeSafe System One wire format.
//!
//! Why a hand-rolled mirror: the tool speaks raw HTTP (no Rust SDK exists), and the
//! format was captured byte-for-byte from the official Python SDK's debug log, so
//! these types are the contract — tests/wire.rs replays the captured bodies.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct JevRequest<S: Serialize> {
    pub state: S,
    pub model: String,
    pub questions: BTreeMap<String, Question>,
}

/// Only the two primitives the pipeline uses are serializable (KISS): Noul for
/// membership verification, Choice for the listwise rerank. Score exists on the
/// wire but no stage asks one.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    Noul {
        instructions: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria>,
    },
    Choice {
        instructions: String,
        /// Option key → description; `None` when the key text is self-describing
        /// (wire: {"py": "Python", "rs": null}).
        criteria: BTreeMap<String, Option<String>>,
    },
}

#[derive(Serialize)]
pub struct NoulCriteria {
    /// Wire keys are the literal strings "true"/"false".
    #[serde(rename = "true")]
    pub true_means: String,
    #[serde(rename = "false")]
    pub false_means: String,
}

#[derive(Deserialize)]
pub struct JevResponse {
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    pub usage: Usage,
}

/// All three answer shapes are parsed so an unexpected primitive in a response
/// never fails deserialization of the whole batch. Noul answers never carry
/// confidence; choice/score do, inline. Score's "legend" map is ignored.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        confidence: Option<f64>,
        probabilities: BTreeMap<String, f64>,
    },
    Score {
        score: f64,
        confidence: Option<f64>,
        probabilities: BTreeMap<String, f64>,
    },
}

#[derive(Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
