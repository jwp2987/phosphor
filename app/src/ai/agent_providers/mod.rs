//! Custom Agent provider support.
//!
//! This module is responsible for:
//! - Securely storing each provider's `api_key` in the OS keychain (secure_storage),
//!   while provider metadata (name/base_url/model list) goes through plain
//!   settings.toml.
//! - Calling `${base_url}/models` via `OpenAiCompatibleClient` to fetch the upstream
//!   list of available models (used by the UI's "Fetch models" button).
//!
//! A second phase will implement the `AiProvider` trait on top of this
//! configuration, routing the Agent's multi-agent calls to local providers.

pub mod active_ai;
pub mod attachment_caps;
pub mod chat_stream;
pub mod content_tool_calls;
/// BYOP embeddings, for the codebase index. New in this fork: the pin produced
/// embeddings server-side and had no client for them.
pub mod embeddings;
pub mod llm_id;
pub mod models_dev;
pub mod oneshot;
pub mod openai_compatible;
pub mod prompt_renderer;
pub mod reasoning;
pub mod secrets;
pub mod tools;
pub mod user_context;
pub mod vertex_auth;
pub mod wire_inspector;
pub mod wire_log;

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

#[cfg(test)]
mod cache_stability_tests;

// ---------------------------------------------------------------------------
// http:// endpoint safety (defense-in-depth against plaintext key leakage)
// ---------------------------------------------------------------------------
//
// BYOP's `Authorization: Bearer <api_key>` header is attached regardless of scheme.
// `https://` protects it in transit; a loopback `http://` endpoint (the documented use
// case — Ollama / LM Studio / vLLM running on the same machine) never puts it on a wire
// at all. Any other `http://` host would carry the key in cleartext across the network,
// so [`chat_stream`] and [`openai_compatible`] both gate on [`is_plaintext_bearer_risk`]
// before sending it.

/// Whether `host` is a loopback address: `localhost`, `127.0.0.0/8`, or `::1`.
///
/// Deliberately a literal check, not a DNS lookup: the supported use case is a local
/// runtime addressed by one of these forms, and resolving arbitrary hostnames here would
/// add a network round-trip to every request-build for no real security gain (a host that
/// merely *resolves* to loopback today can trivially be repointed elsewhere tomorrow).
pub(crate) fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // `Url::host_str()` already strips the brackets from a bracketed IPv6 literal, but
    // tolerate a caller passing the raw bracketed form too.
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// Whether sending the BYOP API key as `Authorization: Bearer <key>` to `url_str` would
/// put it on the wire in cleartext: an `http://` URL whose host is not loopback.
/// `https://` is always fine (TLS terminates the risk); a malformed/unparseable URL is
/// treated as "not this function's problem" — the request itself will fail downstream.
pub(crate) fn is_plaintext_bearer_risk(url_str: &str) -> bool {
    match url::Url::parse(url_str.trim()) {
        Ok(u) => u.scheme() == "http" && !u.host_str().is_some_and(is_loopback_host),
        Err(_) => false,
    }
}

#[cfg(test)]
mod plaintext_bearer_risk_tests {
    use super::*;

    #[test]
    fn https_is_never_a_risk() {
        assert!(!is_plaintext_bearer_risk("https://api.example.com/v1/"));
        assert!(!is_plaintext_bearer_risk("https://evil.example.com/v1/"));
    }

    #[test]
    fn http_loopback_is_not_a_risk() {
        assert!(!is_plaintext_bearer_risk("http://localhost:11434/v1/"));
        assert!(!is_plaintext_bearer_risk("http://127.0.0.1:11434/v1/"));
        assert!(!is_plaintext_bearer_risk("http://127.0.0.8:11434/v1/"));
        assert!(!is_plaintext_bearer_risk("http://[::1]:11434/v1/"));
        assert!(!is_plaintext_bearer_risk("HTTP://LOCALHOST:11434/v1/"));
    }

    #[test]
    fn http_non_loopback_is_a_risk() {
        assert!(is_plaintext_bearer_risk("http://api.example.com/v1/"));
        assert!(is_plaintext_bearer_risk("http://192.168.1.50:11434/v1/"));
        assert!(is_plaintext_bearer_risk("http://10.0.0.5:8080/v1/"));
        assert!(is_plaintext_bearer_risk("http://box:11434/v1/"));
    }

    #[test]
    fn malformed_url_is_not_flagged_as_a_risk() {
        assert!(!is_plaintext_bearer_risk("not a url"));
        assert!(!is_plaintext_bearer_risk(""));
    }

    #[test]
    fn loopback_host_recognizes_all_forms() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("LocalHost"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("127.0.0.8"));
        assert!(is_loopback_host("127.255.255.255"));
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("192.168.1.1"));
        assert!(!is_loopback_host("example.com"));
        assert!(!is_loopback_host("0.0.0.0"));
    }
}

// Current external use sites:
// - `fetch_openai_compatible_models`: the FetchAgentProviderModels handler in
//   ai_page.rs
// - `AgentProviderSecrets`: several handlers in ai_page.rs and the lib.rs
//   registration point
// The remaining symbols (`OpenAiCompatibleError`/`OpenAiCompatibleModel`/
// `AgentProviderSecretsEvent`) are still reachable via full paths like
// `crate::ai::agent_providers::openai_compatible::*`; we no longer re-export them
// here to avoid `unused_imports` warnings.
pub use openai_compatible::fetch_openai_compatible_models;
pub use secrets::AgentProviderSecrets;

// ---------------------------------------------------------------------------
// LLMInfo synthesis: converts the agent_providers configured in settings into a
// form the picker can use
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use settings::Setting;
use warpui::{AppContext, SingletonEntity};

use crate::ai::llms::{
    AvailableLLMs, DisableReason, LLMContextWindow, LLMInfo, LLMProvider, LLMUsageMetadata,
    ModelsByFeature,
};
use crate::settings::{AISettings, AgentProvider};

/// Synthesizes the list of LLMInfo for all valid (provider, model) pairs of the given
/// providers.
///
/// "Valid" = the provider has a non-empty base_url + at least 1 model.
/// **API key is optional**: local unauthenticated providers (ollama / lm-studio /
/// vllm etc.) are allowed to leave it empty; the model still gets exposed to the
/// picker even without a key, and requests still go out at runtime, just without an
/// `Authorization` header.
/// Invalid providers (missing base_url or no models) are ignored entirely, and their
/// models don't show up in the picker — this lets the user intuitively see "which
/// providers aren't fully filled in → don't appear". Explicitly disabled providers
/// (`AgentProvider::disabled`) are skipped the same way, without touching their
/// stored config or API key. Individual models can also be excluded via
/// `AgentProviderModel::disabled`, so a provider with a huge catalog (some have
/// 200-300 models) can be curated down without deleting the rest.
fn build_byop_llm_infos(app: &AppContext) -> Vec<LLMInfo> {
    let providers = AISettings::as_ref(app).agent_providers.value().clone();
    let mut out = Vec::new();

    for provider in providers {
        // Skips providers missing an endpoint / with no models / explicitly disabled by the
        // user (see AgentProvider::is_usable) — their models don't show up in the picker at
        // all, same treatment as an unconfigured provider.
        if !provider.is_usable() {
            continue;
        }

        let provider_label = if provider.name.trim().is_empty() {
            provider.id.clone()
        } else {
            provider.name.clone()
        };

        for model in &provider.models {
            if model.id.trim().is_empty() || model.disabled {
                continue;
            }
            let display_name = if model.name.trim().is_empty() {
                model.id.clone()
            } else {
                model.name.clone()
            };
            // Three-tier priority resolves the final capability: the user's
            // three-state chip override in settings → models.dev catalog inference →
            // substring fallback.
            // This is the same function chat_stream uses when deciding whether to
            // attach ContentPart::Binary, so UI display and runtime behavior always
            // stay in sync.
            let resolved_caps =
                attachment_caps::resolve_for_model(&provider.id, provider.api_type, model);
            let vision_supported = resolved_caps.images;
            out.push(LLMInfo {
                display_name: format!("{provider_label} / {display_name}"),
                base_model_name: format!("{provider_label} / {display_name}"),
                id: llm_id::encode(&provider.id, &model.id),
                reasoning_level: None,
                usage_metadata: LLMUsageMetadata {
                    request_multiplier: 1,
                    credit_multiplier: None,
                },
                description: None,
                disable_reason: None,
                vision_supported,
                spec: None,
                provider: LLMProvider::Unknown,
                host_configs: HashMap::new(),
                discount_percentage: None,
                context_window: LLMContextWindow::default(),
            });
        }
    }

    out
}

/// Placeholder entry: when the user has no valid provider configured, the picker
/// still needs at least 1 entry (`AvailableLLMs::new` rejects an empty list). This
/// entry is grayed out via `DisableReason::Unavailable`, can't be selected, and
/// prompts the user to configure one in Settings.
fn placeholder_llm_info() -> LLMInfo {
    LLMInfo {
        display_name: "No custom provider configured — add one in Settings → AI".to_owned(),
        base_model_name: "Not configured".to_owned(),
        id: ai::LLMId::from("byop-placeholder"),
        reasoning_level: None,
        usage_metadata: LLMUsageMetadata {
            request_multiplier: 1,
            credit_multiplier: None,
        },
        description: None,
        disable_reason: Some(DisableReason::Unavailable),
        vision_supported: false,
        spec: None,
        provider: LLMProvider::Unknown,
        host_configs: HashMap::new(),
        discount_percentage: None,
        context_window: LLMContextWindow::default(),
    }
}

/// Builds a `ModelsByFeature` populated entirely with BYOP models.
/// All 4 features (agent_mode / coding / cli_agent / computer_use) share the same
/// model set — custom providers don't distinguish capability, so any model can be
/// used for any feature.
pub fn build_byop_models_by_feature(app: &AppContext) -> ModelsByFeature {
    let mut choices = build_byop_llm_infos(app);
    if choices.is_empty() {
        choices.push(placeholder_llm_info());
    }

    let default_id = choices[0].id.clone();
    let make = || {
        AvailableLLMs::new(default_id.clone(), choices.clone(), None)
            .expect("choices is non-empty by construction")
    };

    ModelsByFeature {
        agent_mode: make(),
        coding: make(),
        cli_agent: Some(make()),
        computer_use: Some(make()),
    }
}

/// Given a BYOP `LLMId`, looks up `(provider, api_key, model_id)` from `AISettings`
/// and secrets.
/// Returns `None` if any piece of information is missing, the provider isn't usable
/// (explicitly disabled, every model disabled, or no endpoint configured -- the same
/// [`AgentProvider::is_usable`] check `build_byop_llm_infos` uses, so the picker and the
/// actual send path can never disagree about which providers are live), or the specific
/// model is disabled (the controller caller should map this to an `InvalidApiKey` error). A
/// model disabled mid-use is handled the same way an ordinary deleted provider already is:
/// `AvailableLLMs::new` falls back to the first remaining choice once the disabled
/// provider's/model's entry drops out of `build_byop_llm_infos`.
pub fn lookup_byop(app: &AppContext, id: &ai::LLMId) -> Option<(AgentProvider, String, String)> {
    let (provider_id, model_id) = llm_id::decode(id)?;
    let providers = AISettings::as_ref(app).agent_providers.value().clone();
    let provider = providers.into_iter().find(|p| p.id == provider_id)?;
    if !provider.is_usable() {
        return None;
    }
    if provider
        .models
        .iter()
        .any(|m| m.id == model_id && m.disabled)
    {
        return None;
    }
    // API key is optional: returns an empty string when there's no key; downstream
    // build_client passes this to genai as `AuthData::from_single("")` —— no
    // `Authorization` header attached, which works for local unauthenticated services
    // like ollama.
    let api_key = AgentProviderSecrets::as_ref(app)
        .get(&provider_id)
        .map(str::to_owned)
        .unwrap_or_default();
    Some((provider, api_key, model_id))
}

/// The configured context window (tokens) for the currently active BYOP base
/// model, or `None` if the model is unset or its context window is 0/blank.
///
/// Mirrors the gating on the send path (see `response_stream.rs`), so the wire
/// inspector can tell the user *why* it will not capture: capture is only
/// meaningful when the active model has a context window defined.
pub fn active_context_window(app: &AppContext) -> Option<u32> {
    let active = crate::ai::llms::LLMPreferences::as_ref(app).get_active_base_model(app, None);
    let (provider_id, model_id) = llm_id::decode(&active.id)?;
    let providers = AISettings::as_ref(app).agent_providers.value().clone();
    let provider = providers.into_iter().find(|p| p.id == provider_id)?;
    provider
        .models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.context_window)
        .filter(|n| *n > 0)
}
