//! The closed provider set behind the non-interactive `--set-provider-api-key`
//! / `--clear-provider-api-key` flags on `warp_tui`'s `TuiArgs`
//! (`crates/warp_tui/src/session.rs`) and their backing store,
//! [`crate::api_keys::ApiKeyManager`].
//!
//! Ported from the pinned oracle (`02b53fcd8`, release `2026.07.29.09.05`
//! stable — see `ORACLE.md`), where this type also lives in `crates/ai` and
//! is used the same way. Issues #392 / #225.
//!
//! This is deliberately narrower than this fork's other, broader provider
//! concept (`app`'s internal `ai::llms::LLMProvider`, which also covers model
//! routing and the arbitrary-provider "Agent providers" BYOP store,
//! `AgentProviderSecrets`): `ApiKeyManager` only ever stores the three
//! pasted-key providers below, so this type exists purely to parse and
//! validate the CLI's closed-set provider slug argument against that store.
//! It is not reachable from `app`'s type (that crate's `ai` module is
//! private, and `crates/ai` cannot depend back on `app` without a cycle), so
//! the two types stay separate rather than one being reused across the
//! boundary.
//!
//! `Xai` (Grok) parses like the other three -- matching the pin's slug
//! grammar -- but [`LLMProvider::supports_pasted_api_key`] excludes it:
//! upstream Warp connects Grok only through a subscription OAuth flow
//! (`crates/warp_tui/src/grok_oauth/`) that this fork deliberately does not
//! port (`DECLINED.md`, xAI / Grok subscription OAuth, #319 -- a product
//! decision, not a cloud drop). `ApiKeyManager` has no field for it either;
//! callers reject `Xai` before it ever reaches the store.
//!
//! **As of #629 no provider reaches the store.** A key in `AiApiKeys` cannot
//! affect anything a user of this fork can reach: the store is read (by
//! `is_using_api_key_for_provider`, `app/src/ai/llms.rs:26`), but only for the
//! provider identities `OpenAI`/`Anthropic`/`Google`, and every model this fork
//! constructs carries `LLMProvider::Unknown` -- so those reads return `false`
//! whatever is stored, and the key is never sent. Both flags are therefore now
//! refused for every provider and point the user at the arbitrary-provider
//! `AgentProviderSecrets` store instead
//! (`session::reject_provider_api_key_flags`). This type is still
//! what parses and validates the slug -- the flags still accept exactly this
//! closed set before refusing it -- so the grammar above is unchanged, but
//! `supports_pasted_api_key` no longer has a production caller, only its test.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LLMProvider {
    OpenAI,
    Anthropic,
    Google,
    Xai,
    Unknown,
}

impl LLMProvider {
    pub const API_KEY_PROVIDERS: [Self; 4] =
        [Self::OpenAI, Self::Anthropic, Self::Google, Self::Xai];
    pub const API_KEY_PROVIDER_VALUE_NAME: &'static str = "openai|anthropic|google|grok";

    /// Whether this provider is connected by pasting a static API key.
    /// `Xai` is excluded: see the module docs.
    pub fn supports_pasted_api_key(self) -> bool {
        matches!(self, Self::OpenAI | Self::Anthropic | Self::Google)
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Google => "Google",
            Self::Xai => "xAI",
            Self::Unknown => "this provider",
        }
    }

    pub fn api_key_slug(self) -> Option<&'static str> {
        match self {
            Self::OpenAI => Some("openai"),
            Self::Anthropic => Some("anthropic"),
            Self::Google => Some("google"),
            Self::Xai => Some("grok"),
            Self::Unknown => None,
        }
    }

    pub fn from_api_key_slug(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "openai" | "open-ai" => Ok(Self::OpenAI),
            "anthropic" => Ok(Self::Anthropic),
            "google" => Ok(Self::Google),
            "grok" | "xai" | "x-ai" => Ok(Self::Xai),
            _ => Err("provider must be one of: anthropic, openai, google, grok".to_owned()),
        }
    }
}

#[cfg(test)]
#[path = "llm_provider_tests.rs"]
mod tests;
