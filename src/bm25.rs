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
use std::sync::atomic::{AtomicUsize, Ordering};

/// One scanned document: path, token count, and per-vocabulary-term counts.
type Scanned = (String, u64, Vec<u32>);

/// One BM25-indexed corpus: postings by term plus per-doc lengths.
pub struct Bm25Index {
    k1: f64,
    b: f64,
    paths: Vec<String>,
    doc_len: Vec<f64>,
    avgdl: f64,
    /// query term → (doc id, term frequency); built once, queried per variant.
    postings: HashMap<String, Vec<(u32, u32)>>,
}

impl Bm25Index {
    /// Read and index every walked file (repo-relative paths, path tokens
    /// included in the document). Unreadable/non-UTF-8 files are skipped with
    /// a warning, never fatal — they simply drop out of the candidate pool.
    /// Reading + tokenizing dominates Stage 1, so files are scanned on every
    /// core; results merge in walk order, identical to a serial build.
    pub fn build_from_files(
        root: &Path,
        walked: &[PathBuf],
        k1: f64,
        b: f64,
        queries: &[String],
    ) -> Self {
        let vocabulary = vocabulary(queries);
        let next = AtomicUsize::new(0);
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        let mut scanned: Vec<(usize, Result<Scanned, String>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads.min(walked.len()))
                .map(|_| {
                    scope.spawn(|| {
                        let mut done = Vec::new();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(abs) = walked.get(i) else {
                                return done;
                            };
                            let rel = abs
                                .strip_prefix(root)
                                .unwrap_or(abs)
                                .to_string_lossy()
                                .into_owned();
                            let doc = match std::fs::read_to_string(abs) {
                                Ok(content) => Ok(scan(rel, &content, &vocabulary)),
                                Err(_) => Err(rel),
                            };
                            done.push((i, doc));
                        }
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
                .collect()
        });
        scanned.sort_unstable_by_key(|&(i, _)| i);
        let docs = scanned.into_iter().filter_map(|(_, doc)| {
            doc.inspect_err(|rel| eprintln!("jevr: skipping unreadable {rel}"))
                .ok()
        });
        Self::assemble(docs, &vocabulary, k1, b)
    }

    /// Index (path, content) documents: tokenize content + path, store
    /// postings and lengths. Eager and exact — no fieldnorm compression.
    /// Every token counts toward document length, but postings are kept only
    /// for the terms of `queries` — the index is rebuilt per invocation and
    /// [`Bm25Index::top`] is only ever asked those queries, so a full-vocabulary
    /// index would only cost Stage 1 time and memory.
    pub fn build(
        docs: impl Iterator<Item = (String, String)>,
        k1: f64,
        b: f64,
        queries: &[String],
    ) -> Self {
        let vocabulary = vocabulary(queries);
        let docs = docs.map(|(path, content)| scan(path, &content, &vocabulary));
        Self::assemble(docs, &vocabulary, k1, b)
    }

    /// Postings and lengths from scanned documents, doc ids in iteration order.
    fn assemble(
        docs: impl Iterator<Item = Scanned>,
        vocabulary: &[String],
        k1: f64,
        b: f64,
    ) -> Self {
        let mut paths = Vec::new();
        let mut doc_len = Vec::new();
        let mut postings: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
        for (path, len, tf) in docs {
            let doc = paths.len() as u32;
            for (term, &count) in vocabulary.iter().zip(&tf) {
                if count > 0 {
                    postings.entry(term.clone()).or_default().push((doc, count));
                }
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

/// Sorted, deduplicated tokens of every query: the only indexed terms.
fn vocabulary(queries: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = queries.iter().flat_map(|q| tokenize(q)).collect();
    terms.sort_unstable();
    terms.dedup();
    terms
}

/// Tokenize one document (content + path): every token counts toward its
/// length; only `vocabulary` terms are counted individually.
fn scan(path: String, content: &str, vocabulary: &[String]) -> Scanned {
    let mut tf = vec![0u32; vocabulary.len()];
    let mut len = 0u64;
    let mut observe = |token: &str| {
        if let Some(term) = vocabulary.iter().position(|t| t == token) {
            tf[term] += 1;
        }
        len += 1;
    };
    for_each_token(content, &mut observe);
    for_each_token(&path, &mut observe);
    (path, len, tf)
}

/// Code-aware tokenizer: split on non-alphanumerics (Unicode-aware, so accented
/// and non-Latin prose survives instead of fragmenting), then split identifiers
/// on `_` and camelCase boundaries (acronym runs preserved: `HTTPResponse` →
/// `http`, `response`). Emits lowercase subtokens plus the whole lowercase
/// identifier when it splits, so `sendCampaign` matches both `send` and
/// `sendcampaign` queries. Pure numbers and >40-byte blobs are dropped.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for_each_token(text, |token| tokens.push(token.to_owned()));
    tokens
}

/// Allocation-free core of [`tokenize`]: calls `emit` with each token in
/// order. Stage 1 tokenizes every walked file, so per-token `String`s would
/// dominate its cost.
pub fn for_each_token(text: &str, mut emit: impl FnMut(&str)) {
    let mut lower = String::new();
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.is_empty() || word.len() > 40 {
            continue;
        }
        let mut runs = 0;
        for part in word.split('_') {
            camel_split(part, |run| {
                emit(lowercase(run, &mut lower));
                runs += 1;
            });
        }
        if runs > 1 && word.chars().any(char::is_alphabetic) {
            emit(lowercase(word, &mut lower));
        }
    }
}

/// Emit the camelCase runs of one `_`-free part: uppercase-acronym runs,
/// Capitalized words, lowercase runs, digit runs. Pure-digit runs are dropped.
/// Indices are byte offsets of chars in `part`.
fn camel_split(part: &str, mut push: impl FnMut(&str)) {
    let mut start = 0;
    let mut prev: Option<(usize, char)> = None;
    for (index, c) in part.char_indices() {
        if let Some((prev_index, p)) = prev {
            let boundary = index > start
                && (
                    // lower/digit → Upper starts a new word
                    (c.is_ascii_uppercase() && !p.is_ascii_uppercase())
                    // acronym run ends before its last Upper + lower ("HTTPServer")
                    || (c.is_ascii_lowercase() && p.is_ascii_uppercase() && prev_index > start)
                    // letter ↔ digit boundary
                    || (c.is_ascii_digit() != p.is_ascii_digit())
                );
            if boundary {
                let end = if c.is_ascii_lowercase() && p.is_ascii_uppercase() {
                    prev_index
                } else {
                    index
                };
                push_run(&part[start..end], &mut push);
                start = end;
            }
        }
        prev = Some((index, c));
    }
    push_run(&part[start..], &mut push);
}

fn push_run(run: &str, push: &mut impl FnMut(&str)) {
    if run.is_empty() || run.bytes().all(|b| b.is_ascii_digit()) {
        return;
    }
    push(run);
}

/// `text.to_lowercase()` into a reused buffer; ASCII text (nearly every
/// token) is lowercased in place, byte-identical to `to_lowercase`.
fn lowercase<'a>(text: &str, buffer: &'a mut String) -> &'a str {
    buffer.clear();
    if text.is_ascii() {
        buffer.push_str(text);
        buffer.make_ascii_lowercase();
    } else {
        buffer.push_str(&text.to_lowercase());
    }
    buffer
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
        let queries = ["kelly criterion bet size".into(), "zzz-no-such-term".into()];
        let index = Bm25Index::build(docs.into_iter(), 1.2, 0.75, &queries);
        assert_eq!(index.top(&queries[0], 2), vec!["kelly.py"]);
        assert!(index.top(&queries[1], 30).is_empty());
    }

    #[test]
    fn path_tokens_match_even_without_content_hits() {
        let docs = vec![
            ("src/circuit_breaker.py".into(), "x = 1".into()),
            ("src/other.py".into(), "y = 2".into()),
        ];
        let queries = ["circuit breaker".into()];
        let index = Bm25Index::build(docs.into_iter(), 1.2, 0.75, &queries);
        assert_eq!(index.top(&queries[0], 30), vec!["src/circuit_breaker.py"]);
    }
}
