//! Lexical (BM25) scoring over code fragments, for the reranker.
//!
//! # Why a lexical scorer is here at all
//!
//! The pin reranked with a cross-encoder: a model that reads the query and the
//! fragment *together* and scores their interaction. The BYOP replacement scored
//! by cosine similarity between independently-encoded vectors — a bi-encoder —
//! and bi-encoder ordering is reliably worse at the top of the list, which is
//! the only part of a reranked list anyone reads.
//!
//! Where the user's provider offers a reranking endpoint, the fork uses it (see
//! [`RerankProvider`][super::local_store_client::RerankProvider]) and this is
//! not consulted. But not every provider has one, and "no provider, therefore no
//! reranking" would leave the common case at the quality the bi-encoder gives.
//! So the fallback is a hybrid: the bi-encoder's ordering fused with this one.
//!
//! # Why BM25 specifically, for code
//!
//! Code search is unusually well served by lexical matching. Identifiers are
//! rare, long and near-unique, so a query that names one — `resolve_embedding_
//! endpoint`, `SIGWINCH`, `NoEmbeddingProvider` — carries far more information
//! in the token than in its embedding. This is exactly where a bi-encoder is
//! weakest: it will happily retrieve a dozen topically-similar functions and
//! rank the one that actually defines the symbol somewhere in the middle,
//! because all dozen are "about" the same thing.
//!
//! BM25 needs no model, no network call and no provider capability. It is scored
//! over the shortlist the vector search already produced, so it never has to
//! index the repository.
//!
//! # The tokenizer is the interesting part
//!
//! A word tokenizer on code is close to useless: `parse_json_config`,
//! `parseJsonConfig` and `ParseJSONConfig` are three different tokens and none of
//! them matches the query "parse json config". So each identifier is emitted
//! *both* whole and split on case and underscore boundaries. A query naming the
//! exact symbol matches the whole-token form (high inverse document frequency,
//! because that exact identifier is rare); a query describing it in words
//! matches the parts.

use std::collections::HashMap;

/// BM25's term-frequency saturation. The standard value; higher makes repeated
/// occurrences count for more.
const K1: f32 = 1.2;

/// BM25's length normalization. The standard value; `0.0` would ignore fragment
/// length entirely, `1.0` would normalize it fully.
const B: f32 = 0.75;

/// Splits text into match units, in the two forms described in the module docs.
///
/// Non-alphanumeric characters are separators, which already handles
/// `snake_case`, `kebab-case`, `::` paths and punctuation. What is left is
/// split again on case and letter/digit boundaries, so `parseJSONConfig2`
/// yields `parsejsonconfig2`, `parse`, `json`, `config`, `2`.
///
/// Single-character parts are dropped from the split form (they are almost pure
/// noise across a corpus of code) but a single-character *whole* token is kept,
/// so a query for `i` or `x` still has something to match.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    for raw in text.split(|character: char| !character.is_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }

        let whole = raw.to_lowercase();
        tokens.push(whole.clone());

        let parts = split_identifier(raw);
        // A token that does not split adds nothing but a duplicate.
        if parts.len() > 1 {
            for part in parts {
                if part.chars().count() > 1 && part != whole {
                    tokens.push(part);
                }
            }
        }
    }

    tokens
}

/// Splits one identifier on `camelCase`, `PascalCase`, `HTTPServer` and
/// letter/digit boundaries, lowercasing each part.
fn split_identifier(raw: &str) -> Vec<String> {
    let characters: Vec<char> = raw.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();

    for (index, character) in characters.iter().enumerate() {
        let previous = index.checked_sub(1).map(|i| characters[i]);
        let next = characters.get(index + 1).copied();

        let boundary = match previous {
            None => false,
            Some(previous) => {
                // `fooBar` -> `foo` | `Bar`
                (previous.is_lowercase() && character.is_uppercase())
                    // `HTTPServer` -> `HTTP` | `Server`
                    || (previous.is_uppercase()
                        && character.is_uppercase()
                        && next.is_some_and(char::is_lowercase))
                    // `utf8Decode` -> `utf` | `8` | `Decode`
                    || (previous.is_alphabetic() && character.is_numeric())
                    || (previous.is_numeric() && character.is_alphabetic())
            }
        };

        if boundary && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
        }
        current.extend(character.to_lowercase());
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

/// BM25 relevance of `query` to each of `documents`, in the same order.
///
/// The corpus statistics (document frequency, average length) are taken over
/// `documents` alone, because that is the whole corpus this is asked about: a
/// reranker sees the shortlist, never the repository. Within a shortlist that is
/// the right frame anyway — a term that appears in every candidate genuinely
/// does not discriminate between them.
///
/// Returns all-zero scores when the query shares no term with any document,
/// which the caller must treat as "no lexical opinion" rather than as a ranking.
pub fn bm25_scores(query: &str, documents: &[&str]) -> Vec<f32> {
    let mut scores = vec![0.0f32; documents.len()];
    if documents.is_empty() {
        return scores;
    }

    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return scores;
    }

    let tokenized: Vec<Vec<String>> = documents.iter().map(|text| tokenize(text)).collect();

    let mut term_frequencies: Vec<HashMap<&str, f32>> = Vec::with_capacity(tokenized.len());
    let mut document_frequency: HashMap<&str, u32> = HashMap::new();
    let mut total_length = 0.0f64;

    for tokens in &tokenized {
        let mut frequencies: HashMap<&str, f32> = HashMap::new();
        for token in tokens {
            *frequencies.entry(token.as_str()).or_insert(0.0) += 1.0;
        }
        for term in frequencies.keys() {
            *document_frequency.entry(term).or_insert(0) += 1;
        }
        total_length += tokens.len() as f64;
        term_frequencies.push(frequencies);
    }

    let count = documents.len() as f32;
    let average_length = (total_length / documents.len() as f64) as f32;
    // Every document empty: there is nothing to normalize against and nothing
    // to match, so the all-zero "no opinion" answer is the correct one.
    if average_length <= 0.0 {
        return scores;
    }

    // Deduplicated so that a query repeating a word does not weight it twice;
    // BM25 scores a query as a set of terms.
    let mut seen: Vec<&str> = Vec::new();
    for term in &query_terms {
        if !seen.contains(&term.as_str()) {
            seen.push(term.as_str());
        }
    }

    for term in seen {
        let Some(frequency) = document_frequency.get(term).copied() else {
            continue;
        };
        // Robertson/Sparck-Jones IDF with the +1 that keeps it non-negative even
        // for a term present in every document.
        let frequency = frequency as f32;
        let idf = (1.0 + (count - frequency + 0.5) / (frequency + 0.5)).ln();

        for (index, frequencies) in term_frequencies.iter().enumerate() {
            let Some(term_frequency) = frequencies.get(term).copied() else {
                continue;
            };
            let length = tokenized[index].len() as f32;
            let denominator = term_frequency + K1 * (1.0 - B + B * length / average_length);
            if denominator <= 0.0 {
                continue;
            }
            scores[index] += idf * (term_frequency * (K1 + 1.0)) / denominator;
        }
    }

    scores
}

#[cfg(test)]
#[path = "lexical_tests.rs"]
mod tests;
