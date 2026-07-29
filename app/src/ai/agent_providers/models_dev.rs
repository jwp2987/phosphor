//! models.dev data source integration.
//!
//! When the user opens the Providers settings page, `https://models.dev/api.json`
//! is fetched asynchronously in the background and cached to
//! `${cache_dir}/models-dev.json`. The next launch reads the cache directly; if the
//! cache is present and hasn't exceeded its TTL (24h by default), no new request is
//! sent; when it's stale or missing, it fetches again.
//!
//! The data structure mirrors opencode's `provider/models.ts`: the top level is
//! `{ <provider_id>: Provider }`, and Provider contains `models: { <model_id>: Model }`.
//! We only care about the few fields the UI's "quick select" needs:
//! - provider: id / name / api / env (hints at which env var is needed)
//! - model:    id / name / limit.context / limit.output / reasoning / tool_call
//!
//! Any field not listed here is tolerated via `serde(default)` + `#[allow(dead_code)]`.
//!
//! Design tradeoff: **synchronous cache reads, asynchronous network fetch**. The
//! read side is used by the UI and needs to be fast; the fetch side is spawned in
//! the background, failures are only logged (no error popup), and if the cache is
//! unavailable the UI just shows empty data with "models.dev hasn't been fetched
//! yet, please check your network".

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::sync::RwLock;
use std::time::{Duration, SystemTime};

use http_client::Client;
use serde::{Deserialize, Serialize};

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const CACHE_FILENAME: &str = "models-dev.json";
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// `models.dev`'s top-level data — provider_id → Provider.
pub type Catalog = BTreeMap<String, Provider>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Provider {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// The upstream API base URL, e.g. `https://api.deepseek.com/v1`.
    #[serde(default)]
    pub api: Option<String>,
    /// The environment variable name(s) this provider typically needs, e.g.
    /// `["DEEPSEEK_API_KEY"]`.
    #[serde(default)]
    pub env: Vec<String>,
    /// Available models, keyed by model id.
    #[serde(default)]
    pub models: BTreeMap<String, Model>,
    /// Documentation URL (some providers have this).
    #[serde(default)]
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Model {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default = "default_true")]
    pub tool_call: bool,
    /// Whether file attachments are supported (the attachment field complements
    /// modalities: modalities describes native multimodality; attachment covers
    /// PDF / generic file attachment protocols).
    #[serde(default)]
    pub attachment: bool,
    /// Input / output modalities, typical values: `text` / `image` / `audio` /
    /// `video` / `pdf`.
    #[serde(default)]
    pub modalities: ModelModalities,
    /// Context window limit.
    #[serde(default)]
    pub limit: ModelLimit,
    /// "alpha" / "beta" / "deprecated" tag.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelModalities {
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub output: Vec<String>,
}

impl ModelModalities {
    pub fn supports_input(&self, modality: &str) -> bool {
        self.input.iter().any(|m| m.eq_ignore_ascii_case(modality))
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelLimit {
    #[serde(default)]
    pub context: u32,
    #[serde(default)]
    pub output: u32,
}

// ── In-process singleton cache ──────────────────────────────────────────────

#[derive(Debug, Default)]
struct State {
    /// The loaded catalog. `None` means it has never loaded successfully.
    catalog: Option<Catalog>,
    /// The cache's last-modified time (used to check for staleness).
    loaded_at: Option<SystemTime>,
}

fn state() -> &'static RwLock<State> {
    static S: OnceLock<RwLock<State>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(State::default()))
}

fn cache_path() -> PathBuf {
    let mut p = warp_core::paths::cache_dir();
    p.push(CACHE_FILENAME);
    p
}

/// Reads a copy of the loaded catalog (no lock wait — just clones it).
/// Returns `None` when there's no data; the UI should show a "fetching" state /
/// retry button.
pub fn cached() -> Option<Catalog> {
    state().read().ok().and_then(|s| s.catalog.clone())
}

/// A snapshot of a model's capabilities extracted from models.dev, used by the BYOP
/// UI / chat_stream to decide attachment types.
#[derive(Debug, Clone, Default)]
pub struct ModelCaps {
    pub vision: bool,
    pub pdf: bool,
    pub audio: bool,
    pub attachment: bool,
}

impl ModelCaps {
    pub fn from_model(m: &Model) -> Self {
        Self {
            vision: m.modalities.supports_input("image"),
            pdf: m.modalities.supports_input("pdf") || m.attachment,
            audio: m.modalities.supports_input("audio"),
            attachment: m.attachment,
        }
    }
}

/// Looks up model_id in the loaded catalog, returning the capabilities this model
/// declares on models.dev.
///
/// First tries an exact match of `provider_id` against a catalog provider key; on a
/// miss, falls back to "scan the whole catalog for the first model.id match". This
/// allows both exact matching (when the user's provider.id matches models.dev) and
/// handling user-defined provider ids (e.g. aggregator providers like "openrouter"
/// or "siliconflow" that forward to upstream models, whose id differs from the
/// models.dev upstream provider).
pub fn lookup_caps(provider_id: &str, model_id: &str) -> Option<ModelCaps> {
    let s = state().read().ok()?;
    let catalog = s.catalog.as_ref()?;
    if let Some(p) = catalog.get(provider_id) {
        if let Some(m) = p.models.get(model_id) {
            return Some(ModelCaps::from_model(m));
        }
    }
    for p in catalog.values() {
        if let Some(m) = p.models.get(model_id) {
            return Some(ModelCaps::from_model(m));
        }
    }
    None
}

/// Reads the disk cache into memory (synchronous, non-blocking; only called at
/// process startup or the first time the UI needs it).
/// Returns false if the disk cache doesn't exist or fails to parse; the caller
/// should trigger a network fetch.
pub fn load_from_disk() -> bool {
    let path = cache_path();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mtime = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok());
    match serde_json::from_slice::<Catalog>(&bytes) {
        Ok(catalog) => {
            if let Ok(mut s) = state().write() {
                s.catalog = Some(catalog);
                s.loaded_at = mtime;
            }
            true
        }
        Err(e) => {
            log::warn!("[models.dev] failed to parse disk cache ({path:?}): {e}");
            false
        }
    }
}

/// Whether the cache is stale — either missing or past the TTL.
pub fn is_stale() -> bool {
    let s = match state().read() {
        Ok(s) => s,
        Err(_) => return true,
    };
    match s.loaded_at {
        Some(t) => SystemTime::now()
            .duration_since(t)
            .map(|d| d > CACHE_TTL)
            .unwrap_or(true),
        None => true,
    }
}

/// Asynchronously fetches models.dev and writes it to both the disk cache and the
/// in-memory cache.
/// Failures are only logged, not propagated upward (the UI caller decides what to
/// show based on whether `cached()` is `Some`).
pub async fn fetch_and_cache(client: Client) -> Result<(), String> {
    let resp = client
        .get(MODELS_DEV_URL)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?;

    let catalog: Catalog =
        serde_json::from_slice(&bytes).map_err(|e| format!("JSON parse failed: {e}"))?;

    // Write to disk — failure isn't fatal, just logged.
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, &bytes) {
        log::warn!("[models.dev] failed to write disk cache ({path:?}): {e}");
    }

    if let Ok(mut s) = state().write() {
        s.catalog = Some(catalog);
        s.loaded_at = Some(SystemTime::now());
    }
    Ok(())
}

// ── Chip row collapsed/expanded state (process-level, so it survives widget
// rebuilds) ──────────────────────────────────────────────────────────────────

static CHIPS_EXPANDED: AtomicBool = AtomicBool::new(false);

pub fn chips_expanded() -> bool {
    CHIPS_EXPANDED.load(Ordering::Relaxed)
}

pub fn toggle_chips_expanded() {
    CHIPS_EXPANDED.fetch_xor(true, Ordering::Relaxed);
}

// ── Flag for whether the most recent network fetch failed ──────────────────

static FETCH_FAILED: AtomicBool = AtomicBool::new(false);

/// Whether the most recent network fetch failed (meaningful when cached() == None).
pub fn last_fetch_failed() -> bool {
    FETCH_FAILED.load(Ordering::Relaxed)
}

/// Set by the caller in the spawn callback (true on failure; success doesn't need
/// to reset it, since cached() is Some by then).
pub fn set_fetch_failed(failed: bool) {
    FETCH_FAILED.store(failed, Ordering::Relaxed);
}

// ── Search filtering for the quick-add chip row ─────────────────────────────

fn search_state() -> &'static RwLock<String> {
    static S: OnceLock<RwLock<String>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(String::new()))
}

pub fn search_query() -> String {
    search_state()
        .read()
        .ok()
        .map(|s| s.clone())
        .unwrap_or_default()
}

pub fn set_search_query(q: String) {
    if let Ok(mut s) = search_state().write() {
        *s = q;
    }
}

/// Filters the catalog by the current search query, case-insensitively matching a
/// substring against provider.name and provider.id.
/// An empty query returns all entries in order. Returns an owned Vec so the UI side
/// can take/iter it.
pub fn filter_catalog(catalog: &Catalog, query: &str) -> Vec<(String, Provider)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return catalog
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
    }
    catalog
        .iter()
        .filter(|(id, p)| id.to_lowercase().contains(&q) || p.name.to_lowercase().contains(&q))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Converts a models.dev Model into the local settings' AgentProviderModel.
///
/// By default, writes the catalog-inferred image/pdf/audio into the fields (so on
/// the user's first sync / quick-add, the model's capabilities are visibly synced
/// into the toml right away, without needing to expand the detail view to see them).
/// On subsequent syncs, the caller only fills new values into `None` slots; `Some(_)`
/// is treated as an explicit user override and skipped.
pub fn into_agent_provider_model(model: &Model) -> crate::settings::AgentProviderModel {
    let caps = ModelCaps::from_model(model);
    crate::settings::AgentProviderModel {
        name: if model.name.is_empty() {
            model.id.clone()
        } else {
            model.name.clone()
        },
        id: model.id.clone(),
        context_window: model.limit.context,
        max_output_tokens: model.limit.output,
        reasoning: model.reasoning,
        tool_call: model.tool_call,
        image: Some(caps.vision),
        pdf: Some(caps.pdf),
        audio: Some(caps.audio),
        disabled: false,
    }
}

/// The canonical catalog id for the merged Google Vertex quick-add chip, see
/// [`quick_add_catalog`].
pub const VERTEX_QUICK_ADD_ID: &str = "google-vertex";

/// Whether a models.dev catalog provider id belongs to the Google Vertex AI family
/// (models.dev lists Vertex-served Gemini and Claude as separate provider entries,
/// e.g. `google-vertex` / `google-vertex-anthropic`).
fn is_vertex_family(catalog_provider_id: &str) -> bool {
    catalog_provider_id.to_lowercase().contains("vertex")
}

/// Maps a models.dev catalog provider id to the [`AgentProviderApiType`] that actually
/// speaks its protocol, instead of always defaulting to the generic `OpenAi`-compatible
/// type. Quick-adding e.g. Anthropic or Vertex with the wrong api_type silently produces
/// a provider that can't authenticate or route correctly.
///
/// Only ids with a dedicated native adapter are mapped; every other catalog entry
/// (DeepInfra, Groq, OpenRouter, ...) is genuinely OpenAI-compatible and keeps the
/// `OpenAi` default.
pub fn infer_api_type(catalog_provider_id: &str) -> crate::settings::AgentProviderApiType {
    use crate::settings::AgentProviderApiType as T;
    if is_vertex_family(catalog_provider_id) {
        return T::Vertex;
    }
    match catalog_provider_id.to_lowercase().as_str() {
        "anthropic" => T::Anthropic,
        "google" | "google-generative-ai" => T::Gemini,
        "deepseek" => T::DeepSeek,
        "ollama" => T::Ollama,
        _ => T::OpenAi,
    }
}

/// Returns `catalog` with all Vertex-family entries collapsed into a single
/// `google-vertex` provider named "Google Vertex AI", combining their model lists.
///
/// Un-merged, the quick-add chip row shows two near-identical "vertex" chips (Gemini
/// and Claude publishers) that are easy to confuse with each other and with the
/// API-Type dropdown's own "Vertex AI" option — even though this app's
/// `AgentProviderApiType::Vertex` already serves both publishers under one provider,
/// routed by model name (see `vertex_model_family`). Used only for building the
/// quick-add chip row / resolving a chip click; other consumers (e.g. matching an
/// existing provider by base_url in `SyncProviderModelsFromModelsDev`) keep using the
/// raw catalog, since a concrete Vertex provider has no catalog base_url to match.
pub fn quick_add_catalog(catalog: &Catalog) -> Catalog {
    let mut merged = Catalog::new();
    let mut vertex_models: BTreeMap<String, Model> = BTreeMap::new();
    for (id, provider) in catalog {
        if is_vertex_family(id) {
            vertex_models.extend(provider.models.clone());
            continue;
        }
        merged.insert(id.clone(), provider.clone());
    }
    if !vertex_models.is_empty() {
        merged.insert(
            VERTEX_QUICK_ADD_ID.to_owned(),
            Provider {
                id: VERTEX_QUICK_ADD_ID.to_owned(),
                name: "Google Vertex AI".to_owned(),
                api: None,
                env: Vec::new(),
                models: vertex_models,
                doc: None,
            },
        );
    }
    merged
}

#[cfg(test)]
mod quick_add_tests {
    use super::*;
    use crate::settings::AgentProviderApiType;

    fn provider(name: &str, models: &[&str]) -> Provider {
        Provider {
            id: String::new(),
            name: name.to_owned(),
            api: Some(format!("https://example.invalid/{name}/")),
            env: Vec::new(),
            models: models
                .iter()
                .map(|m| {
                    (
                        (*m).to_owned(),
                        Model {
                            id: (*m).to_owned(),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            doc: None,
        }
    }

    #[test]
    fn infer_api_type_maps_known_ids() {
        assert_eq!(infer_api_type("anthropic"), AgentProviderApiType::Anthropic);
        assert_eq!(infer_api_type("google"), AgentProviderApiType::Gemini);
        assert_eq!(infer_api_type("deepseek"), AgentProviderApiType::DeepSeek);
        assert_eq!(infer_api_type("ollama"), AgentProviderApiType::Ollama);
        assert_eq!(infer_api_type("google-vertex"), AgentProviderApiType::Vertex);
        assert_eq!(
            infer_api_type("google-vertex-anthropic"),
            AgentProviderApiType::Vertex
        );
        // Anything else (Groq, OpenRouter, DeepInfra, ...) is genuinely
        // OpenAI-compatible and keeps the default.
        assert_eq!(infer_api_type("groq"), AgentProviderApiType::OpenAi);
    }

    #[test]
    fn quick_add_catalog_merges_vertex_family_into_one_entry() {
        let mut catalog = Catalog::new();
        catalog.insert(
            "google-vertex".to_owned(),
            provider("Google Vertex", &["gemini-2.5-pro"]),
        );
        catalog.insert(
            "google-vertex-anthropic".to_owned(),
            provider("Google Vertex (Anthropic)", &["claude-sonnet-5"]),
        );
        catalog.insert("anthropic".to_owned(), provider("Anthropic", &["claude"]));

        let merged = quick_add_catalog(&catalog);

        // The two vertex entries collapse into exactly one, under the canonical id.
        assert_eq!(
            merged.keys().filter(|id| id.contains("vertex")).count(),
            1
        );
        let vertex = merged.get(VERTEX_QUICK_ADD_ID).expect("merged vertex entry");
        assert_eq!(vertex.name, "Google Vertex AI");
        assert!(vertex.models.contains_key("gemini-2.5-pro"));
        assert!(vertex.models.contains_key("claude-sonnet-5"));

        // Non-vertex entries pass through untouched.
        assert!(merged.contains_key("anthropic"));
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn quick_add_catalog_is_noop_without_vertex_entries() {
        let mut catalog = Catalog::new();
        catalog.insert("openai".to_owned(), provider("OpenAI", &["gpt-5"]));
        let merged = quick_add_catalog(&catalog);
        assert_eq!(merged.len(), 1);
        assert!(!merged.contains_key(VERTEX_QUICK_ADD_ID));
    }
}
