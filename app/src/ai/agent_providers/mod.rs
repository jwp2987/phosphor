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
/// Returns `None` if any piece of information is missing, the provider is effectively
/// disabled (explicitly, or because it has no models configured), or the specific model is
/// disabled (the controller caller should map this to an `InvalidApiKey` error). A
/// model disabled mid-use is handled the same way an ordinary deleted provider already is:
/// `AvailableLLMs::new` falls back to the first remaining choice once the disabled
/// provider's/model's entry drops out of `build_byop_llm_infos`.
pub fn lookup_byop(app: &AppContext, id: &ai::LLMId) -> Option<(AgentProvider, String, String)> {
    let (provider_id, model_id) = llm_id::decode(id)?;
    let providers = AISettings::as_ref(app).agent_providers.value().clone();
    let provider = providers.into_iter().find(|p| p.id == provider_id)?;
    if provider.effectively_disabled() {
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
