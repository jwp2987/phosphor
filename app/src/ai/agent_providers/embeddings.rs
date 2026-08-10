//! BYOP embeddings: the user's own provider, called directly.
//!
//! This is the outbound half of the codebase index's replacement for the pin's
//! cloud `StoreClient`. The pin produced embeddings server-side; here the user
//! configures a provider the same way they configure a chat provider, and this
//! module posts to its `/embeddings` endpoint.
//!
//! # Configuration, and why there is no new setting for it
//!
//! An embedding model is selected exactly like a chat model: add it to a
//! provider's `models` list under Settings > AI. [`resolve_embedding_endpoint`]
//! looks for a usable [`AgentProvider`] that lists the model id the requested
//! [`EmbeddingConfig`] names (`text-embedding-3-small`, `voyage-3.5`, …). That
//! deliberately reuses `AISettings::agent_providers` and
//! [`AgentProviderSecrets`] rather than inventing a parallel mechanism — one
//! place to put an endpoint, one place to put a key.
//!
//! # Wire format
//!
//! Both provider families take `POST {base}/embeddings` with a JSON body and
//! answer with `{"data": [{"index": n, "embedding": [...]}]}`, so one request
//! path serves both. They disagree on one field name — OpenAI truncates with
//! `dimensions`, Voyage with `output_dimension` — which is the only branch.

use std::sync::{Arc, Mutex};

use ai::index::full_source_code_embedding::EmbeddingConfig;
use ai::index::full_source_code_embedding::Error as IndexError;
use ai::index::full_source_code_embedding::local_store_client::EmbeddingProvider;
use async_trait::async_trait;
use http_client::Client;
use serde::{Deserialize, Serialize};
use settings::Setting;
use warpui::{AppContext, SingletonEntity};

use super::AgentProviderSecrets;
use super::openai_compatible::normalize_base_url;
use crate::settings::{AISettings, AgentProvider};

/// Every embedding model the index knows how to build against, in the order
/// [`resolve_configured_embedding_model`] prefers them.
///
/// Order is the pin's own default first (`EmbeddingConfig::default()`, Voyage
/// 3.5), then the remaining Voyage models, then OpenAI — matching the pin's
/// server-side default, so a user who configures more than one gets the model
/// the index was tuned against.
pub const SUPPORTED_EMBEDDING_MODELS: &[EmbeddingConfig] = &[
    EmbeddingConfig::Voyage3_5_512,
    EmbeddingConfig::VoyageCode3_512,
    EmbeddingConfig::Voyage4_512,
    EmbeddingConfig::Voyage3_5_Lite_512,
    EmbeddingConfig::OpenAiTextSmall3_256,
];

/// A resolved place to send embedding requests.
///
/// Resolved on the main thread (it reads settings and the keychain) and then
/// moved into background futures, which is why it owns its strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddingEndpoint {
    /// Normalized base URL, with no trailing slash. `/embeddings` is appended.
    pub base_url: String,
    /// Empty means "send no `Authorization` header", which is how a local
    /// runtime with no auth is supported — the same tolerance the chat path has.
    pub api_key: String,
}

/// The provider that serves `embedding_config`, if the user has configured one.
///
/// This is the single definition of "a usable provider that lists this model
/// id"; [`resolve_embedding_endpoint`] is the same lookup plus the endpoint and
/// key. It is separate because the settings UI needs the provider itself — it
/// reports *which* provider serves the index, and a name is the only thing a
/// user recognizes.
pub fn resolve_embedding_provider(
    app: &AppContext,
    embedding_config: EmbeddingConfig,
) -> Option<AgentProvider> {
    let model_id = embedding_config.model_id();
    let providers = AISettings::as_ref(app).agent_providers.value().clone();

    providers.into_iter().find(|provider| {
        provider.is_usable()
            && provider
                .models
                .iter()
                .any(|model| model.id == model_id && !model.disabled)
    })
}

/// Finds the provider the user has configured for `embedding_config`.
///
/// Returns `None` when no usable provider lists that model, which is the state a
/// fresh install is in. Callers must surface that as
/// [`IndexError::NoEmbeddingProvider`] rather than as an empty result.
pub fn resolve_embedding_endpoint(
    app: &AppContext,
    embedding_config: EmbeddingConfig,
) -> Option<EmbeddingEndpoint> {
    let provider = resolve_embedding_provider(app, embedding_config)?;

    let base_url = normalize_base_url(&provider.resolved_base_url()).ok()?;
    let api_key = AgentProviderSecrets::as_ref(app)
        .get(&provider.id)
        .map(str::to_owned)
        .unwrap_or_default();

    Some(EmbeddingEndpoint { base_url, api_key })
}

/// The embedding model the user has actually configured, if any.
///
/// At the pin this came back from the server's `codebaseContextConfig` query.
/// Here it is derived from what the user has set up: the first entry of
/// [`SUPPORTED_EMBEDDING_MODELS`] that resolves to a provider.
pub fn resolve_configured_embedding_model(app: &AppContext) -> Option<EmbeddingConfig> {
    SUPPORTED_EMBEDDING_MODELS
        .iter()
        .copied()
        .find(|config| resolve_embedding_endpoint(app, *config).is_some())
}

/// Calls a user-configured `/embeddings` endpoint over HTTP.
///
/// The endpoint lives behind a `Mutex` so it can be refreshed when the user
/// edits their providers, without rebuilding the index manager: the manager
/// holds one `Arc<dyn StoreClient>` for the life of the process.
pub struct HttpEmbeddingProvider {
    client: Client,
    endpoint: Mutex<Option<EmbeddingEndpoint>>,
}

impl HttpEmbeddingProvider {
    pub fn new(client: Client, endpoint: Option<EmbeddingEndpoint>) -> Self {
        Self {
            client,
            endpoint: Mutex::new(endpoint),
        }
    }

    /// Builds one from the app's current settings, for `embedding_config`.
    pub fn from_app(app: &AppContext, embedding_config: EmbeddingConfig) -> Self {
        Self::new(
            Client::new(),
            resolve_embedding_endpoint(app, embedding_config),
        )
    }

    /// Replaces the endpoint, e.g. after the user edits their providers.
    pub fn set_endpoint(&self, endpoint: Option<EmbeddingEndpoint>) {
        if let Ok(mut slot) = self.endpoint.lock() {
            *slot = endpoint;
        }
    }

    fn endpoint(&self, embedding_config: EmbeddingConfig) -> Result<EmbeddingEndpoint, IndexError> {
        self.endpoint
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or(IndexError::NoEmbeddingProvider {
                model: embedding_config.model_id(),
            })
    }
}

/// The request body, covering both provider families.
///
/// `dimensions` and `output_dimension` are the same thing under two names;
/// exactly one is set, and the other is omitted so a provider never sees a field
/// it will reject.
#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dimension: Option<usize>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
    /// Providers are permitted to answer out of order, so results are sorted by
    /// this before being returned. Getting this wrong would pair vectors with
    /// the wrong fragments — a silent, unfalsifiable corruption of the index.
    #[serde(default)]
    index: usize,
    embedding: Vec<f32>,
}

/// Whether this model's truncation parameter is Voyage's `output_dimension`
/// rather than OpenAI's `dimensions`.
fn uses_voyage_dimension_field(embedding_config: EmbeddingConfig) -> bool {
    match embedding_config {
        EmbeddingConfig::OpenAiTextSmall3_256 => false,
        EmbeddingConfig::VoyageCode3_512
        | EmbeddingConfig::Voyage3_5_Lite_512
        | EmbeddingConfig::Voyage3_5_512
        | EmbeddingConfig::Voyage4_512 => true,
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl EmbeddingProvider for HttpEmbeddingProvider {
    async fn embed(
        &self,
        embedding_config: EmbeddingConfig,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, IndexError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let endpoint = self.endpoint(embedding_config)?;
        let expected = texts.len();
        let dimensions = embedding_config.dimensions();
        let voyage = uses_voyage_dimension_field(embedding_config);

        let body = EmbeddingRequest {
            model: embedding_config.model_id(),
            input: texts,
            dimensions: (!voyage).then_some(dimensions),
            output_dimension: voyage.then_some(dimensions),
        };

        let url = format!("{}/embeddings", endpoint.base_url);
        let mut request = self.client.post(&url).json(&body);
        if !endpoint.api_key.trim().is_empty() {
            // Same rule as the chat path: an API key never goes out as a
            // plaintext bearer token to a non-loopback host.
            if super::is_plaintext_bearer_risk(&endpoint.base_url) {
                return Err(IndexError::Other(anyhow::anyhow!(
                    "refusing to send the API key to {} over plaintext HTTP — use https, or point this provider at a local runtime",
                    endpoint.base_url
                )));
            }
            request = request.bearer_auth(&endpoint.api_key);
        }

        let response = request.send().await.map_err(|error| {
            IndexError::Other(anyhow::anyhow!(error).context("embedding request failed"))
        })?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(IndexError::Other(anyhow::anyhow!(
                "embedding request to {url} failed with HTTP {}: {detail}",
                status.as_u16()
            )));
        }

        let parsed: EmbeddingResponse = response.json().await.map_err(|error| {
            IndexError::Other(anyhow::anyhow!(error).context("failed to parse embedding response"))
        })?;

        let mut data = parsed.data;
        if data.len() != expected {
            return Err(IndexError::Other(anyhow::anyhow!(
                "embedding provider returned {} vectors for {expected} inputs",
                data.len()
            )));
        }
        data.sort_by_key(|datum| datum.index);

        Ok(data.into_iter().map(|datum| datum.embedding).collect())
    }
}

/// Convenience alias for the shape the index wants.
pub fn shared_provider(provider: HttpEmbeddingProvider) -> Arc<dyn EmbeddingProvider> {
    Arc::new(provider)
}

#[cfg(test)]
#[path = "embeddings_tests.rs"]
mod tests;
