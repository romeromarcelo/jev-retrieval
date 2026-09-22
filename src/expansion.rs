//! Keyword variants for Stage 1.
//!
//! Why variants come from the caller, not Jev: Jev is System One — it selects, it
//! does not generate. The repeatable `-k` flag lets a caller widen Stage 1 recall
//! with extra keyword variants, though bare natural-language queries measure
//! better than hand-picked variants (docs/BENCHMARKS.md). The fallback strips
//! English stopwords from the concept so a bare invocation avoids indexing
//! interrogative filler ("where does…").

const MAX_VARIANTS: usize = 4;

/// Small, KISS stopword list: interrogative/function words that add BM25
/// noise without narrowing the concept.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "at", "be", "by", "code", "do", "does", "for", "from", "happen",
    "happens", "how", "in", "is", "it", "of", "on", "or", "our", "that", "the", "their", "this",
    "to", "we", "what", "when", "where", "which", "who", "why", "with",
];

/// The Stage 1 query variants: `-k` values pass through (capped at 4), otherwise
/// the stopword-stripped concept is the single variant.
pub fn variants(concept: &str, keywords: &[String]) -> Vec<String> {
    if !keywords.is_empty() {
        return keywords.iter().take(MAX_VARIANTS).cloned().collect();
    }
    let stripped = strip_stopwords(concept);
    let fallback = if stripped.is_empty() {
        concept.to_owned()
    } else {
        stripped
    };
    vec![fallback]
}

fn strip_stopwords(concept: &str) -> String {
    concept
        .split_whitespace()
        .filter(|word| {
            let bare: String = word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            !STOPWORDS.contains(&bare.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_pass_through_capped_at_four() {
        let keywords: Vec<String> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(variants("ignored", &keywords), keywords[..4].to_vec());
    }

    #[test]
    fn fallback_strips_stopwords() {
        assert_eq!(
            variants("where does the websocket reconnection happen?", &[]),
            vec!["websocket reconnection"] // "happen?" strips via its bare form
        );
    }

    #[test]
    fn all_stopword_concept_falls_back_to_itself() {
        assert_eq!(variants("where is it", &[]), vec!["where is it"]);
    }
}
