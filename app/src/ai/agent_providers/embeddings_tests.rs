//! Tests for the BYOP embeddings client.
//!
//! New, not ported — the pin had no client-side embeddings code at all. These
//! cover the two things that would corrupt an index silently if they were
//! wrong: the truncation-parameter branch between provider families, and the
//! ordering of the returned vectors.

use futures::executor::block_on;

use super::*;

#[test]
fn openai_uses_the_dimensions_field_and_voyage_uses_output_dimension() {
    // These are the same knob under two names. Sending the wrong one is not a
    // hard failure at every provider -- some ignore unknown fields -- so a
    // mix-up would quietly produce full-width vectors that no longer match the
    // width recorded alongside them.
    assert!(!uses_voyage_dimension_field(
        EmbeddingConfig::OpenAiTextSmall3_256
    ));
    for config in [
        EmbeddingConfig::VoyageCode3_512,
        EmbeddingConfig::Voyage3_5_Lite_512,
        EmbeddingConfig::Voyage3_5_512,
        EmbeddingConfig::Voyage4_512,
    ] {
        assert!(
            uses_voyage_dimension_field(config),
            "{config:?} is a Voyage model and must use output_dimension"
        );
    }
}

#[test]
fn the_request_body_sets_exactly_one_dimension_field() {
    let openai = EmbeddingRequest {
        model: EmbeddingConfig::OpenAiTextSmall3_256.model_id(),
        input: vec!["x".to_owned()],
        dimensions: Some(256),
        output_dimension: None,
    };
    let json = serde_json::to_value(&openai).expect("serializable");
    assert_eq!(json["dimensions"], 256);
    assert!(
        json.get("output_dimension").is_none(),
        "a field the provider does not know must be omitted, not sent as null"
    );

    let voyage = EmbeddingRequest {
        model: EmbeddingConfig::Voyage3_5_512.model_id(),
        input: vec!["x".to_owned()],
        dimensions: None,
        output_dimension: Some(512),
    };
    let json = serde_json::to_value(&voyage).expect("serializable");
    assert_eq!(json["output_dimension"], 512);
    assert!(json.get("dimensions").is_none());
}

#[test]
fn responses_are_reordered_by_index() {
    // Providers may answer out of order. Pairing vectors with fragments
    // positionally means an unsorted response silently attaches each vector to
    // the wrong fragment -- a corruption nothing downstream can detect.
    let body = r#"{"data":[
        {"index":2,"embedding":[3.0]},
        {"index":0,"embedding":[1.0]},
        {"index":1,"embedding":[2.0]}
    ]}"#;
    let parsed: EmbeddingResponse = serde_json::from_str(body).expect("parses");
    let mut data = parsed.data;
    data.sort_by_key(|datum| datum.index);

    assert_eq!(
        data.into_iter()
            .map(|datum| datum.embedding)
            .collect::<Vec<_>>(),
        vec![vec![1.0], vec![2.0], vec![3.0]]
    );
}

#[test]
fn a_response_without_an_index_field_keeps_its_position() {
    // Not every provider emits `index`. `#[serde(default)]` gives them all 0,
    // and a stable sort then preserves arrival order, which is the only
    // sensible reading of an unindexed response.
    let body = r#"{"data":[{"embedding":[1.0]},{"embedding":[2.0]}]}"#;
    let parsed: EmbeddingResponse = serde_json::from_str(body).expect("parses");
    let mut data = parsed.data;
    data.sort_by_key(|datum| datum.index);

    assert_eq!(
        data.into_iter()
            .map(|datum| datum.embedding)
            .collect::<Vec<_>>(),
        vec![vec![1.0], vec![2.0]]
    );
}

#[test]
fn every_supported_model_is_listed_exactly_once() {
    // `resolve_configured_embedding_model` walks this list, so a missing entry
    // means a model the user can configure but the index will never pick.
    let mut seen: Vec<&'static str> = SUPPORTED_EMBEDDING_MODELS
        .iter()
        .map(|config| config.storage_key())
        .collect();
    seen.sort_unstable();
    let count = seen.len();
    seen.dedup();
    assert_eq!(count, seen.len(), "no model may appear twice");
    assert_eq!(count, 5, "all five EmbeddingConfig variants must be listed");
}

#[test]
fn the_default_embedding_model_is_preferred_first() {
    // The index was tuned against the pin's default. A user with two providers
    // configured should get that one.
    assert_eq!(SUPPORTED_EMBEDDING_MODELS[0], EmbeddingConfig::default());
}

#[test]
fn an_unset_endpoint_reports_a_missing_provider() {
    let provider = HttpEmbeddingProvider::new(Client::new_for_test(), None);
    let error = provider
        .endpoint(EmbeddingConfig::default())
        .expect_err("no configured endpoint must be an error");

    assert!(
        matches!(error, IndexError::NoEmbeddingProvider { .. }),
        "expected NoEmbeddingProvider, got {error}"
    );
    assert!(
        error
            .to_string()
            .contains(EmbeddingConfig::default().model_id()),
        "the error must name the model the user has to configure: {error}"
    );
}

#[test]
fn set_endpoint_replaces_a_missing_one() {
    let provider = HttpEmbeddingProvider::new(Client::new_for_test(), None);
    provider.set_endpoint(Some(EmbeddingEndpoint {
        base_url: "https://api.voyageai.com/v1".to_owned(),
        api_key: "k".to_owned(),
    }));

    let endpoint = provider
        .endpoint(EmbeddingConfig::default())
        .expect("the endpoint set above must be visible");
    assert_eq!(endpoint.base_url, "https://api.voyageai.com/v1");
}

#[test]
fn embedding_dimensions_match_the_model_names() {
    // The width is requested explicitly rather than taken as the model default,
    // and it is also what the vector store records, so the two must agree.
    assert_eq!(EmbeddingConfig::OpenAiTextSmall3_256.dimensions(), 256);
    assert_eq!(EmbeddingConfig::Voyage3_5_512.dimensions(), 512);
    assert_eq!(EmbeddingConfig::VoyageCode3_512.dimensions(), 512);
    assert_eq!(EmbeddingConfig::Voyage4_512.dimensions(), 512);
    assert_eq!(EmbeddingConfig::Voyage3_5_Lite_512.dimensions(), 512);
}

// --------------------------------------------------------------------------
// Reranking
// --------------------------------------------------------------------------

#[test]
fn a_rerank_response_parses_from_either_field_name() {
    // Voyage returns the scores under `data`, Cohere under `results`. Getting
    // this wrong would not fail loudly -- it would make every rerank fall back to
    // the hybrid path, and the user would just get slightly worse ordering
    // forever with no error anywhere.
    let voyage: RerankResponse =
        serde_json::from_str(r#"{"data":[{"index":0,"relevance_score":0.4}]}"#).expect("parses");
    assert!(voyage.data.is_some() && voyage.results.is_none());

    let cohere: RerankResponse =
        serde_json::from_str(r#"{"results":[{"index":0,"relevance_score":0.9}]}"#).expect("parses");
    assert!(cohere.results.is_some() && cohere.data.is_none());
}

#[test]
fn a_rerank_response_ignores_fields_it_does_not_know() {
    // Both providers send `object`, `model` and `usage` alongside the scores, and
    // Cohere's entries can carry a `document`. An unknown field must not be a
    // parse failure.
    let body = r#"{
        "object": "list",
        "model": "rerank-2.5",
        "usage": {"total_tokens": 12},
        "data": [{"index": 1, "relevance_score": 0.7, "document": "fn main() {}"}]
    }"#;
    let parsed: RerankResponse = serde_json::from_str(body).expect("parses");
    let data = parsed.data.expect("data present");
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].index, 1);
}

#[test]
fn the_rerank_request_body_carries_the_model_query_and_documents() {
    let body = RerankRequest {
        model: "rerank-2.5",
        query: "resolve embedding endpoint",
        documents: vec!["fn a() {}".to_owned(), "fn b() {}".to_owned()],
    };
    let json = serde_json::to_value(&body).expect("serializable");

    assert_eq!(json["model"], "rerank-2.5");
    assert_eq!(json["query"], "resolve embedding endpoint");
    assert_eq!(
        json["documents"],
        serde_json::json!(["fn a() {}", "fn b() {}"])
    );
}

#[test]
fn no_rerank_model_appears_twice_in_the_supported_list() {
    // The list is walked in preference order and the first match wins, so a
    // duplicate would be dead weight at best and a silently unreachable
    // preference at worst.
    let mut seen: Vec<&'static str> = SUPPORTED_RERANK_MODELS.to_vec();
    seen.sort_unstable();
    let count = seen.len();
    seen.dedup();
    assert_eq!(count, seen.len());
    assert!(count >= 2, "at least the two provider families must be listed");
}

#[test]
fn a_rerank_provider_reports_the_model_it_will_call() {
    // Surfaced in the log at startup, because "is my reranker actually being
    // used?" is otherwise unanswerable from outside.
    let provider = HttpRerankProvider::new(
        Client::new_for_test(),
        EmbeddingEndpoint {
            base_url: "https://api.voyageai.com/v1".to_owned(),
            api_key: "k".to_owned(),
        },
        "rerank-2.5",
    );
    assert_eq!(provider.model_id(), "rerank-2.5");
}

#[test]
fn a_rerank_provider_refuses_to_send_a_key_over_plaintext() {
    // Same rule as the chat and embedding paths. A rerank request carries the
    // user's source code as well as their key, so this one is not optional.
    block_on(async {
        let provider = HttpRerankProvider::new(
            Client::new_for_test(),
            EmbeddingEndpoint {
                base_url: "http://example.com/v1".to_owned(),
                api_key: "secret".to_owned(),
            },
            "rerank-2.5",
        );

        let error = provider
            .rerank("query", vec!["fn a() {}".to_owned()])
            .await
            .expect_err("a plaintext host with a key must be refused");
        assert!(
            error.to_string().contains("plaintext"),
            "the error must say why: {error}"
        );
    });
}

#[test]
fn an_empty_rerank_costs_no_request() {
    block_on(async {
        let provider = HttpRerankProvider::new(
            Client::new_for_test(),
            EmbeddingEndpoint {
                base_url: "https://api.voyageai.com/v1".to_owned(),
                api_key: "k".to_owned(),
            },
            "rerank-2.5",
        );
        assert!(
            provider
                .rerank("query", Vec::new())
                .await
                .expect("nothing to rerank is not a failure")
                .is_empty()
        );
    });
}
