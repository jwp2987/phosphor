//! Tests for the BYOP embeddings client.
//!
//! New, not ported — the pin had no client-side embeddings code at all. These
//! cover the two things that would corrupt an index silently if they were
//! wrong: the truncation-parameter branch between provider families, and the
//! ordering of the returned vectors.

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
