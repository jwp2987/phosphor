use super::*;
use crate::ai::agent_providers::embeddings::{EmbeddingEndpoint, EmbeddingEndpoints};

fn endpoint(host: &str) -> EmbeddingEndpoint {
    EmbeddingEndpoint {
        base_url: format!("https://{host}.example.invalid/v1"),
        api_key: "sk-test".to_string(),
        extra_headers: Vec::new(),
    }
}

fn both() -> EmbeddingEndpoints {
    [
        (EmbeddingConfig::OpenAiTextSmall3_256, endpoint("a")),
        (EmbeddingConfig::Voyage3_5_512, endpoint("b")),
    ]
    .into_iter()
    .collect()
}

#[test]
fn no_configured_provider_means_no_embedder() {
    assert_eq!(choose_config(&EmbeddingEndpoints::default(), None), None);
    assert_eq!(
        choose_config(
            &EmbeddingEndpoints::default(),
            Some("voyage:voyage-3.5:512")
        ),
        None
    );
}

#[test]
fn a_fresh_repository_takes_the_first_configured_model() {
    assert_eq!(
        choose_config(&both(), None),
        Some(EmbeddingConfig::OpenAiTextSmall3_256)
    );
}

#[test]
fn the_model_a_repository_was_indexed_with_is_kept_while_it_is_configured() {
    // Adding a provider that sorts first must not re-embed an index built with another model.
    assert_eq!(
        choose_config(&both(), Some("voyage:voyage-3.5:512")),
        Some(EmbeddingConfig::Voyage3_5_512)
    );
}

#[test]
fn a_removed_or_unknown_model_falls_back_to_what_is_configured() {
    let only_openai =
        EmbeddingEndpoints::single(EmbeddingConfig::OpenAiTextSmall3_256, endpoint("a"));
    assert_eq!(
        choose_config(&only_openai, Some("voyage:voyage-3.5:512")),
        Some(EmbeddingConfig::OpenAiTextSmall3_256)
    );
    assert_eq!(
        choose_config(&only_openai, Some("not-a-key")),
        Some(EmbeddingConfig::OpenAiTextSmall3_256)
    );
}

#[test]
fn the_embedder_reports_the_storage_key_of_its_model() {
    let embedder = GateEmbedder {
        provider: Arc::new(HttpEmbeddingProvider::new(
            http_client::Client::new_for_test(),
            None,
        )),
        config: EmbeddingConfig::Voyage3_5_512,
        batch: 7,
    };
    assert_eq!(
        embedder.storage_key().as_deref(),
        Some("voyage:voyage-3.5:512")
    );
    assert_eq!(embedder.max_batch(), 7);
}

#[test]
fn an_embedding_with_no_endpoint_fails_before_any_request() {
    // `HttpEmbeddingProvider::new(.., None)` resolves nothing: the call errors out as "no
    // provider" without a socket, so this proves nothing leaves the process by default.
    let embedder = GateEmbedder {
        provider: Arc::new(HttpEmbeddingProvider::new(
            http_client::Client::new_for_test(),
            None,
        )),
        config: EmbeddingConfig::Voyage3_5_512,
        batch: 1,
    };
    let result = futures::executor::block_on(embedder.embed(vec!["x".to_string()]));
    assert!(result.is_err());
}
