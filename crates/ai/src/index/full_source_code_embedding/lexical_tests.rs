//! Tests for the lexical half of the reranker.
//!
//! New, not ported — the pin reranked on the server and had no client-side
//! lexical scoring. The tokenizer gets most of the attention here because it is
//! where a code-search BM25 differs from a prose one, and because a tokenizer
//! bug is invisible downstream: the scores stay plausible and merely stop
//! finding things.

use super::*;

fn tokens(text: &str) -> Vec<String> {
    tokenize(text)
}

#[test]
fn identifiers_are_emitted_whole_and_in_parts() {
    // Both forms matter. The whole form is what a query naming the exact symbol
    // matches, and it is rare enough to carry a lot of weight; the parts are what
    // a query describing the symbol in words matches.
    let emitted = tokens("resolve_embedding_endpoint");
    assert!(emitted.contains(&"resolve".to_owned()));
    assert!(emitted.contains(&"embedding".to_owned()));
    assert!(emitted.contains(&"endpoint".to_owned()));
}

#[test]
fn camel_and_pascal_case_split_the_way_a_reader_would() {
    let emitted = tokens("parseJsonConfig");
    assert!(emitted.contains(&"parsejsonconfig".to_owned()), "{emitted:?}");
    for part in ["parse", "json", "config"] {
        assert!(emitted.contains(&part.to_owned()), "{part} missing from {emitted:?}");
    }

    // An acronym run followed by a word: `HTTPServer` is `HTTP` and `Server`,
    // not `H`, `T`, `T`, `P`, `Server`.
    let emitted = tokens("HTTPServer");
    assert!(emitted.contains(&"http".to_owned()), "{emitted:?}");
    assert!(emitted.contains(&"server".to_owned()), "{emitted:?}");
}

#[test]
fn digits_are_a_boundary() {
    let emitted = tokens("utf8Decode");
    assert!(emitted.contains(&"utf".to_owned()), "{emitted:?}");
    assert!(emitted.contains(&"decode".to_owned()), "{emitted:?}");
}

#[test]
fn punctuation_and_paths_are_separators() {
    let emitted = tokens("crate::ai::index -> Result<Settings>");
    for part in ["crate", "ai", "index", "result", "settings"] {
        assert!(emitted.contains(&part.to_owned()), "{part} missing from {emitted:?}");
    }
}

#[test]
fn a_token_that_does_not_split_is_emitted_once() {
    // Emitting `settings` twice for one occurrence would double its term
    // frequency and quietly distort every score it appears in.
    let emitted = tokens("settings");
    assert_eq!(emitted, vec!["settings".to_owned()]);
}

#[test]
fn single_character_words_survive_but_single_character_fragments_do_not() {
    assert_eq!(tokens("x"), vec!["x".to_owned()]);
    // `aB` would otherwise contribute a meaningless `a`.
    let emitted = tokens("aBc");
    assert!(!emitted.contains(&"a".to_owned()), "{emitted:?}");
}

#[test]
fn a_query_sharing_no_term_scores_every_document_zero() {
    // The contract the fusion depends on: no shared term means no opinion, and
    // an all-zero score vector is how "no opinion" is expressed. Anything else
    // would inject an arbitrary order into the fused ranking.
    let scores = bm25_scores("zzz nonexistent", &["fn alpha() {}", "fn beta() {}"]);
    assert_eq!(scores, vec![0.0, 0.0]);
}

#[test]
fn an_empty_query_or_corpus_is_handled() {
    assert_eq!(bm25_scores("", &["fn alpha() {}"]), vec![0.0]);
    assert!(bm25_scores("alpha", &[]).is_empty());
    assert_eq!(bm25_scores("alpha", &["", ""]), vec![0.0, 0.0]);
}

#[test]
fn a_rare_term_outweighs_a_common_one() {
    // The reason BM25 is worth having on code: an identifier that appears in one
    // fragment discriminates, and one that appears in all of them does not.
    let documents = [
        "fn load_settings(path: &Path) -> Settings {}",
        "fn save_settings(path: &Path, settings: Settings) {}",
        "fn merge_settings(a: Settings, b: Settings) -> Settings {}",
    ];
    let scores = bm25_scores("merge settings", &documents);

    assert!(
        scores[2] > scores[0] && scores[2] > scores[1],
        "the fragment holding the rare term must win: {scores:?}"
    );
}

#[test]
fn a_query_naming_a_symbol_finds_the_fragment_that_defines_it() {
    // The motivating case, end to end through the tokenizer: the query is prose,
    // the symbol is snake_case, and they have to meet.
    let documents = [
        "pub fn build_store_client(app: &AppContext) -> Arc<dyn StoreClient> { todo!() }",
        "pub fn resolve_embedding_endpoint(app: &AppContext) -> Option<EmbeddingEndpoint> { todo!() }",
        "pub fn active_embedding_config(app: &AppContext) -> EmbeddingConfig { todo!() }",
    ];
    let scores = bm25_scores("resolve embedding endpoint", &documents);

    assert!(
        scores[1] > scores[0] && scores[1] > scores[2],
        "the definition must outrank its neighbours: {scores:?}"
    );
}

#[test]
fn length_normalization_stops_a_long_fragment_winning_on_bulk() {
    // Without the `b` term, a fragment that merely mentions a term many times
    // because it is long would outrank a short fragment that is *about* it.
    let short = "fn merge(a: Settings, b: Settings) -> Settings { a.merge(b) }";
    let padded = format!("fn unrelated() {{ {} }}", "let value = compute(); ".repeat(40));
    let long_with_one_mention = format!("{padded} // merge");

    let scores = bm25_scores("merge", &[short, &long_with_one_mention]);
    assert!(
        scores[0] > scores[1],
        "the short, on-topic fragment must win: {scores:?}"
    );
}
