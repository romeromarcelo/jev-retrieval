//! Eager in-memory BM25 Stage 1 backend (Lucene variant, code tokenizer).
//!
//! Why hand-rolled rather than tantivy or the `bm25` crate: at repo scale
//! (≤ ~6 k whole-file docs) an inverted-index engine solves problems we don't
//! have — tantivy hardcodes k1/b (`src/query/bm25.rs`) and adds 50 direct
//! deps, while eager scoring over in-RAM postings is microseconds per query
//! and the cold cost is file I/O + tokenization, not scoring. Why the Lucene
//! formula: variant choice is statistically indistinguishable (Kamphuis et
//! al., ECIR 2020) and the Lucene IDF `ln(1 + (N−df+0.5)/(df+0.5))` cannot go
//! negative on near-universal code tokens (`self`, `return`). What does
//! matter for code retrieval is tokenization — camelCase/snake_case subtoken
//! splitting is the measured lever (Zhang et al., NLP4Prog 2021) — so the
//! tokenizer emits each identifier's subtokens plus the whole identifier.
//! Path tokens are indexed with the content: file paths are strong signals
//! for file-level localization (arXiv:2607.11046).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One BM25-indexed corpus: postings by term plus per-doc lengths.
pub struct Bm25Index {
    k1: f64,
    b: f64,
    paths: Vec<String>,
    doc_len: Vec<f64>,
    avgdl: f64,
    /// term → (doc id, term frequency); built once, queried per variant.
    postings: HashMap<String, Vec<(u32, u32)>>,
}

impl Bm25Index {
    /// Read and index every walked file (repo-relative paths, path tokens
    /// included in the document). Unreadable/non-UTF-8 files are skipped with
    /// a warning, never fatal — they simply drop out of the candidate pool.
    pub fn build_from_files(root: &Path, walked: &[PathBuf], k1: f64, b: f64) -> Self {
        let docs = walked.iter().filter_map(|abs| {
            let rel = abs
                .strip_prefix(root)
                .unwrap_or(abs)
                .to_string_lossy()
                .into_owned();
            match std::fs::read_to_string(abs) {
                Ok(content) => Some((rel, content)),
                Err(_) => {
                    eprintln!("jevr: skipping unreadable {rel}");
                    None
                }
            }
        });
        Self::build(docs, k1, b)
    }

    /// Index (path, content) documents: tokenize content + path, store
    /// postings and lengths. Eager and exact — no fieldnorm compression.
    pub fn build(docs: impl Iterator<Item = (String, String)>, k1: f64, b: f64) -> Self {
        let mut paths = Vec::new();
        let mut doc_len = Vec::new();
        let mut postings: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
        for (path, content) in docs {
            let doc = paths.len() as u32;
            let mut tf: HashMap<String, u32> = HashMap::new();
            let mut len = 0u64;
            for token in tokenize(&content).into_iter().chain(tokenize(&path)) {
                *tf.entry(token).or_insert(0) += 1;
                len += 1;
            }
            for (term, count) in tf {
                postings.entry(term).or_default().push((doc, count));
            }
            paths.push(path);
            doc_len.push(len as f64);
        }
        let n = doc_len.len().max(1) as f64;
        let avgdl = (doc_len.iter().sum::<f64>() / n).max(1.0);
        Self {
            k1,
            b,
            paths,
            doc_len,
            avgdl,
            postings,
        }
    }

    /// Top-`cap` file paths for a query variant, best score first. OR
    /// semantics over query tokens; zero-score docs are never returned.
    pub fn top(&self, query: &str, cap: usize) -> Vec<String> {
        let n = self.paths.len() as f64;
        let mut scores: HashMap<u32, f64> = HashMap::new();
        let mut terms = tokenize(query);
        terms.sort_unstable();
        terms.dedup();
        for term in &terms {
            let Some(list) = self.postings.get(term) else {
                continue;
            };
            let df = list.len() as f64;
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            for &(doc, tf) in list {
                let tf = tf as f64;
                let norm = 1.0 - self.b + self.b * self.doc_len[doc as usize] / self.avgdl;
                *scores.entry(doc).or_insert(0.0) +=
                    idf * tf * (self.k1 + 1.0) / (tf + self.k1 * norm);
            }
        }
        let mut ranked: Vec<(f64, u32)> = scores
            .into_iter()
            .filter(|(_, s)| *s > 0.0)
            .map(|(doc, s)| (s, doc))
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        ranked
            .into_iter()
            .take(cap)
            .map(|(_, doc)| self.paths[doc as usize].clone())
            .collect()
    }
}

/// Code-aware tokenizer: split on non-alphanumerics (Unicode-aware, so accented
/// and non-Latin prose survives instead of fragmenting), then split identifiers
/// on `_` and camelCase boundaries (acronym runs preserved: `HTTPResponse` →
/// `http`, `response`). Emits lowercase subtokens plus the whole lowercase
/// identifier when it splits, so `sendCampaign` matches both `send` and
/// `sendcampaign` queries. Pure numbers and >40-byte blobs are dropped.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.is_empty() || word.len() > 40 {
            continue;
        }
        let before = tokens.len();
        for part in word.split('_') {
            camel_split(part, &mut tokens);
        }
        if tokens.len() > before + 1 && word.chars().any(char::is_alphabetic) {
            tokens.push(word.to_lowercase());
        }
    }
    tokens
}

/// Push the camelCase runs of one `_`-free part: uppercase-acronym runs,
/// Capitalized words, lowercase runs, digit runs. Pure-digit runs are dropped.
fn camel_split(part: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = part.chars().collect();
    let mut start = 0;
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let boundary = index > start
            && (
                // lower/digit → Upper starts a new word
                (c.is_ascii_uppercase() && !chars[index - 1].is_ascii_uppercase())
                // acronym run ends before its last Upper + lower ("HTTPServer")
                || (c.is_ascii_lowercase()
                    && chars[index - 1].is_ascii_uppercase()
                    && index - 1 > start)
                // letter ↔ digit boundary
                || (c.is_ascii_digit() != chars[index - 1].is_ascii_digit())
            );
        if boundary {
            let end = if c.is_ascii_lowercase() && chars[index - 1].is_ascii_uppercase() {
                index - 1
            } else {
                index
            };
            push_run(&chars[start..end], out);
            start = end;
        }
        index += 1;
    }
    push_run(&chars[start..], out);
}

fn push_run(run: &[char], out: &mut Vec<String>) {
    if run.is_empty() || run.iter().all(|c| c.is_ascii_digit()) {
        return;
    }
    out.push(run.iter().collect::<String>().to_lowercase());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_splits_snake_camel_and_acronyms() {
        assert_eq!(
            tokenize("sendCampaign snake_case HTTPResponse"),
            vec![
                "send",
                "campaign",
                "sendcampaign",
                "snake",
                "case",
                "snake_case",
                "http",
                "response",
                "httpresponse",
            ]
        );
    }

    #[test]
    fn tokenizer_keeps_non_ascii_prose_words_whole() {
        assert_eq!(tokenize("Café in Zürich"), vec!["café", "in", "zürich"]);
    }

    #[test]
    fn tokenizer_drops_numbers_and_oversized_blobs() {
        let blob = "x".repeat(41);
        assert_eq!(
            tokenize(&format!("42 {blob} v2ray")),
            vec!["v", "ray", "v2ray"]
        );
    }

    #[test]
    fn top_ranks_matching_file_first_and_respects_cap() {
        let docs = vec![
            (
                "kelly.py".into(),
                "def kelly_criterion(bankroll): pass".into(),
            ),
            ("fees.py".into(), "def taker_fee(): pass".into()),
            ("misc.py".into(), "unrelated content entirely".into()),
        ];
        let index = Bm25Index::build(docs.into_iter(), 1.2, 0.75);
        assert_eq!(index.top("kelly criterion bet size", 2), vec!["kelly.py"]);
        assert!(index.top("zzz-no-such-term", 30).is_empty());
    }

    #[test]
    fn path_tokens_match_even_without_content_hits() {
        let docs = vec![
            ("src/circuit_breaker.py".into(), "x = 1".into()),
            ("src/other.py".into(), "y = 2".into()),
        ];
        let index = Bm25Index::build(docs.into_iter(), 1.2, 0.75);
        assert_eq!(
            index.top("circuit breaker", 30),
            vec!["src/circuit_breaker.py"]
        );
    }
}
