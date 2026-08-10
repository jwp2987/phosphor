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
//!
//! # Reranking
//!
//! [`HttpRerankProvider`] is the same idea for `POST {base}/rerank`: the pin
//! reranked with a server-side cross-encoder, and where the user's provider
//! sells one this buys it rather than approximating it. It is resolved as a
//! capability rather than a setting — see [`SUPPORTED_RERANK_MODELS`] — and
//! returning `None` is an ordinary outcome, because the index has a hybrid
//! fallback that needs no reranking model at all.

use std::sync::{Arc, Mutex};

use ai::index::full_source_code_embedding::EmbeddingConfig;
use ai::index::full_source_code_embedding::Error as IndexError;
use ai::index::full_source_code_embedding::local_store_client::{EmbeddingProvider, RerankProvider};
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

/// Every reranking model this fork knows how to call, in preference order.
///
/// # Why a list and not a setting
///
/// A reranker is a *capability*, not a choice: either the provider the user
/// already configured has one or it does not, and there is nothing for them to
/// decide. So this is resolved exactly the way the embedding model is — find a
/// usable provider whose model list contains one of these ids — rather than
/// adding a setting whose only correct value is "whatever my provider offers".
///
/// Order is newest-and-strongest first within each family, Voyage before Cohere
/// to match [`SUPPORTED_EMBEDDING_MODELS`]'s preference for Voyage. A user who
/// has configured several gets the best one they are paying for.
///
/// Both families take `POST {base}/rerank` with `{model, query, documents}` and
/// answer with a list of `{index, relevance_score}`; they disagree only on
/// whether that list is called `data` (Voyage) or `results` (Cohere), which
/// [`RerankResponse`] accepts either of. A provider whose reranker does not
/// speak this shape must not be added here — it would fail every call, and
/// reranking would silently fall back on every search.
pub const SUPPORTED_RERANK_MODELS: &[&str] = &[
    "rerank-2.5",
    "rerank-2.5-lite",
    "rerank-2",
    "rerank-2-lite",
    "rerank-lite-1",
    "rerank-v3.5",
    "rerank-english-v3.0",
    "rerank-multilingual-v3.0",
];

/// The reranking model the user has configured, and where to reach it.
///
/// `None` — no usable provider lists any model in [`SUPPORTED_RERANK_MODELS`] —
/// is the ordinary case, not an error. `LocalStoreClient` then reranks with
/// hybrid vector + lexical scoring, which needs no provider capability at all.
pub fn resolve_rerank_endpoint(app: &AppContext) -> Option<(EmbeddingEndpoint, &'static str)> {
    let providers = AISettings::as_ref(app).agent_providers.value().clone();

    for model_id in SUPPORTED_RERANK_MODELS.iter().copied() {
        let Some(provider) = providers.iter().find(|provider| {
            provider.is_usable()
                && provider
                    .models
                    .iter()
                    .any(|model| model.id == model_id && !model.disabled)
        }) else {
            continue;
        };

        let Ok(base_url) = normalize_base_url(&provider.resolved_base_url()) else {
            continue;
        };
        let api_key = AgentProviderSecrets::as_ref(app)
            .get(&provider.id)
            .map(str::to_owned)
            .unwrap_or_default();

        return Some((EmbeddingEndpoint { base_url, api_key }, model_id));
    }

    None
}

/// Calls a user-configured `/rerank` endpoint over HTTP.
///
/// This is the one piece of the pin's retrieval quality that the bi-encoder
/// cannot reproduce: a cross-encoder reads the query and the fragment *together*
/// and scores their interaction, where a bi-encoder compares two vectors that
/// were computed without ever seeing each other. Buying it from the provider the
/// user already brought is cheaper — in code, in binary size and in latency —
/// than shipping a local model, and it is the only option that costs this fork
/// no model dependency at all.
pub struct HttpRerankProvider {
    client: Client,
    endpoint: EmbeddingEndpoint,
    model_id: &'static str,
}

impl HttpRerankProvider {
    pub fn new(client: Client, endpoint: EmbeddingEndpoint, model_id: &'static str) -> Self {
        Self {
            client,
            endpoint,
            model_id,
        }
    }

    /// Builds one from the app's current settings, or `None` when the user's
    /// providers offer no reranking model.
    pub fn from_app(app: &AppContext) -> Option<Self> {
        let (endpoint, model_id) = resolve_rerank_endpoint(app)?;
        Some(Self::new(Client::new(), endpoint, model_id))
    }

    /// Which model this will call. Reported at startup so a user who wonders
    /// whether their reranker is being used can find out from the log.
    pub fn model_id(&self) -> &'static str {
        self.model_id
    }
}

#[derive(Serialize)]
struct RerankRequest<'a> {
    model: &'a str,
    query: &'a str,
    documents: Vec<String>,
}

/// One provider's opinion of one document.
#[derive(Deserialize)]
struct RerankDatum {
    /// Which input document this scores. Providers are permitted to answer in
    /// score order rather than input order, so this is what pairs a score back
    /// to its fragment; ignoring it would shuffle the results silently.
    #[serde(default)]
    index: usize,
    relevance_score: f32,
}

/// Voyage returns the scores under `data`, Cohere under `results`. Nothing else
/// about the two calls differs, so accepting both is one field rather than a
/// second request path.
#[derive(Deserialize)]
struct RerankResponse {
    #[serde(default)]
    data: Option<Vec<RerankDatum>>,
    #[serde(default)]
    results: Option<Vec<RerankDatum>>,
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl RerankProvider for HttpRerankProvider {
    async fn rerank(&self, query: &str, documents: Vec<String>) -> Result<Vec<f32>, IndexError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let expected = documents.len();
        let body = RerankRequest {
            model: self.model_id,
            query,
            documents,
        };

        let url = format!("{}/rerank", self.endpoint.base_url);
        let mut request = self.client.post(&url).json(&body);
        if !self.endpoint.api_key.trim().is_empty() {
            // Same rule as the embedding and chat paths: an API key never goes
            // out as a plaintext bearer token to a non-loopback host.
            if super::is_plaintext_bearer_risk(&self.endpoint.base_url) {
                return Err(IndexError::Other(anyhow::anyhow!(
                    "refusing to send the API key to {} over plaintext HTTP — use https, or point this provider at a local runtime",
                    self.endpoint.base_url
                )));
            }
            request = request.bearer_auth(&self.endpoint.api_key);
        }

        let response = request.send().await.map_err(|error| {
            IndexError::Other(anyhow::anyhow!(error).context("rerank request failed"))
        })?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(IndexError::Other(anyhow::anyhow!(
                "rerank request to {url} failed with HTTP {}: {detail}",
                status.as_u16()
            )));
        }

        let parsed: RerankResponse = response.json().await.map_err(|error| {
            IndexError::Other(anyhow::anyhow!(error).context("failed to parse rerank response"))
        })?;

        let data = parsed
            .data
            .or(parsed.results)
            .ok_or_else(|| IndexError::Other(anyhow::anyhow!(
                "rerank response from {url} has neither a `data` nor a `results` list"
            )))?;
        if data.len() != expected {
            return Err(IndexError::Other(anyhow::anyhow!(
                "rerank provider returned {} scores for {expected} documents",
                data.len()
            )));
        }

        // Placed by index rather than appended, because the provider is allowed
        // to answer in score order. A score landing on the wrong fragment would
        // be an unfalsifiable ranking bug: the results would look plausible and
        // be wrong.
        let mut scores = vec![f32::NEG_INFINITY; expected];
        for datum in data {
            let Some(slot) = scores.get_mut(datum.index) else {
                return Err(IndexError::Other(anyhow::anyhow!(
                    "rerank provider scored document {} of {expected}",
                    datum.index
                )));
            };
            *slot = datum.relevance_score;
        }

        Ok(scores)
    }
}

#[cfg(test)]
#[path = "embeddings_tests.rs"]
mod tests;
