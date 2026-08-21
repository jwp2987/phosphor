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

/// Whether `provider` is usable and offers `model_id`.
///
/// One definition, so the single-model lookup and the whole-table resolve can
/// never disagree about what "configured for this model" means.
fn serves_model(provider: &AgentProvider, model_id: &str) -> bool {
    provider.is_usable()
        && provider
            .models
            .iter()
            .any(|model| model.id == model_id && !model.disabled)
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

    AISettings::as_ref(app)
        .agent_providers
        .value()
        .iter()
        .find(|provider| serves_model(provider, model_id))
        .cloned()
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

/// Every embedding model the user has a usable provider for, and where each
/// one's requests go.
///
/// # Why a table and not a single endpoint
///
/// A running `CodebaseIndex` caches the model it was built with and only
/// re-reads it on a *full* sync, which is scheduled twenty minutes apart
/// (`REINDEX_INTERVAL`, `crates/ai/.../codebase_index.rs`). Incremental syncs
/// and queries in between pass that cached model down to
/// `HttpEmbeddingProvider::embed`.
///
/// With one shared endpoint slot those two caches can disagree. Adding a Voyage
/// provider to a repo already indexed with OpenAI moves the slot to Voyage
/// immediately — [`resolve_configured_embedding_model`] prefers the first entry
/// of [`SUPPORTED_EMBEDDING_MODELS`] — while every incremental sync for the next
/// twenty minutes still asks for `text-embedding-3-small`. Those requests would
/// go to Voyage, which does not serve that model, and fail with nothing but
/// telemetry to show for it.
///
/// Routing per model removes the window rather than merely reporting it: the
/// previous model still resolves to the provider that actually serves it, so
/// those syncs keep working until the next full sync re-keys the index. A model
/// whose provider the user has genuinely *removed* resolves to nothing, which
/// surfaces as [`IndexError::NoEmbeddingProvider`] naming that model — one loud,
/// accurate error instead of a silent stream of HTTP 400s.
///
/// Entries are in [`SUPPORTED_EMBEDDING_MODELS`] order, so
/// [`preferred_model`][Self::preferred_model] is simply the first one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmbeddingEndpoints {
    entries: Vec<(EmbeddingConfig, EmbeddingEndpoint)>,
}

impl EmbeddingEndpoints {
    /// A table with one entry, for callers and tests that have already resolved
    /// a single model.
    pub fn single(embedding_config: EmbeddingConfig, endpoint: EmbeddingEndpoint) -> Self {
        Self {
            entries: vec![(embedding_config, endpoint)],
        }
    }

    /// Where requests for `embedding_config` go, if anywhere.
    pub fn get(&self, embedding_config: EmbeddingConfig) -> Option<&EmbeddingEndpoint> {
        self.entries
            .iter()
            .find(|(config, _)| *config == embedding_config)
            .map(|(_, endpoint)| endpoint)
    }

    /// The model a new index should be built with: the first configured entry
    /// in [`SUPPORTED_EMBEDDING_MODELS`] order.
    pub fn preferred_model(&self) -> Option<EmbeddingConfig> {
        self.entries.first().map(|(config, _)| *config)
    }
}

/// Order is meaning here — the first entry is
/// [`preferred_model`](EmbeddingEndpoints::preferred_model) — so this preserves
/// the iterator's order rather than sorting or deduplicating.
impl FromIterator<(EmbeddingConfig, EmbeddingEndpoint)> for EmbeddingEndpoints {
    fn from_iter<I: IntoIterator<Item = (EmbeddingConfig, EmbeddingEndpoint)>>(iter: I) -> Self {
        Self {
            entries: iter.into_iter().collect(),
        }
    }
}

/// Resolves every supported model against the user's current providers.
///
/// One borrow of the provider list and one of the key store, reused across all
/// of [`SUPPORTED_EMBEDDING_MODELS`] — this runs on every settings change, and
/// resolving each model separately would walk and clone the provider list once
/// per model.
pub fn resolve_embedding_endpoints(app: &AppContext) -> EmbeddingEndpoints {
    let providers = AISettings::as_ref(app).agent_providers.value();
    let secrets = AgentProviderSecrets::as_ref(app);

    let entries = SUPPORTED_EMBEDDING_MODELS
        .iter()
        .copied()
        .filter_map(|embedding_config| {
            let provider = providers
                .iter()
                .find(|provider| serves_model(provider, embedding_config.model_id()))?;
            let base_url = normalize_base_url(&provider.resolved_base_url()).ok()?;
            let api_key = secrets
                .get(&provider.id)
                .map(str::to_owned)
                .unwrap_or_default();

            Some((embedding_config, EmbeddingEndpoint { base_url, api_key }))
        })
        .collect();

    EmbeddingEndpoints { entries }
}

/// The embedding model the user has actually configured, if any.
///
/// At the pin this came back from the server's `codebaseContextConfig` query.
/// Here it is derived from what the user has set up: the first entry of
/// [`SUPPORTED_EMBEDDING_MODELS`] that resolves to a provider.
pub fn resolve_configured_embedding_model(app: &AppContext) -> Option<EmbeddingConfig> {
    resolve_embedding_endpoints(app).preferred_model()
}

/// Where an embedding request goes, and for which models.
enum EndpointResolution {
    /// One endpoint that answers for whatever model it is asked for.
    ///
    /// Two callers have this shape: a provider that has not been configured yet
    /// (`None`), and the remote daemon's store client
    /// (`app/src/remote_server/codebase_index_store.rs`), which is told one
    /// endpoint and one model by its client and enforces the model itself.
    AnyModel(Option<EmbeddingEndpoint>),
    /// One endpoint per model, resolved from the user's provider list. See
    /// [`EmbeddingEndpoints`] for why the app side routes rather than sharing a
    /// single slot.
    PerModel(EmbeddingEndpoints),
}

impl EndpointResolution {
    fn endpoint(&self, embedding_config: EmbeddingConfig) -> Option<&EmbeddingEndpoint> {
        match self {
            Self::AnyModel(endpoint) => endpoint.as_ref(),
            Self::PerModel(endpoints) => endpoints.get(embedding_config),
        }
    }
}

/// Calls a user-configured `/embeddings` endpoint over HTTP.
///
/// The resolution lives behind a `Mutex` so it can be refreshed when the user
/// edits their providers, without rebuilding the index manager: the manager
/// holds one `Arc<dyn StoreClient>` for the life of the process.
pub struct HttpEmbeddingProvider {
    client: Client,
    resolution: Mutex<EndpointResolution>,
}

impl HttpEmbeddingProvider {
    pub fn new(client: Client, endpoint: Option<EmbeddingEndpoint>) -> Self {
        Self {
            client,
            resolution: Mutex::new(EndpointResolution::AnyModel(endpoint)),
        }
    }

    /// Replaces the resolution wholesale.
    ///
    /// Poison is recovered from, not skipped. A refresh that silently no-ops
    /// leaves the endpoint frozen for the life of the process — which is
    /// precisely the defect `RefreshingStoreClient` exists to fix, and an
    /// earlier version of this function reintroduced it as a permanent state by
    /// dropping the write on a poisoned lock. Nothing here can be observed
    /// half-written: the slot is replaced whole, so a panicking thread can only
    /// leave a *stale* resolution behind, never a torn one. This matches
    /// `RefreshingStoreClient::reconfigure` and `::client`, which recover the
    /// same way.
    fn set(&self, resolution: EndpointResolution) {
        *self
            .resolution
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = resolution;
    }

    /// Points every model at one endpoint, e.g. the one a remote client sent.
    pub fn set_endpoint(&self, endpoint: Option<EmbeddingEndpoint>) {
        self.set(EndpointResolution::AnyModel(endpoint));
    }

    /// Routes each model to the provider the user configured for it, e.g. after
    /// they edit their providers.
    pub fn set_endpoints(&self, endpoints: EmbeddingEndpoints) {
        self.set(EndpointResolution::PerModel(endpoints));
    }

    fn endpoint(&self, embedding_config: EmbeddingConfig) -> Result<EmbeddingEndpoint, IndexError> {
        self.resolution
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .endpoint(embedding_config)
            .cloned()
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
