//! `AgentProviderSecrets`: stores each custom provider's API key in the OS key store.
//!
//! Data shape: `HashMap<provider_id, api_key>`, serialized via `serde_json` and
//! written under the `AgentProviderSecrets` key in `secure_storage`.
//!
//! Design modeled after `crates/ai/src/api_keys.rs::ApiKeyManager`.

use std::collections::HashMap;

use warpui::{Entity, ModelContext, SingletonEntity};
use warpui_extras::secure_storage::{self, AppContextExt};

const SECURE_STORAGE_KEY: &str = "AgentProviderSecrets";

/// Emitted whenever any provider's API key changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProviderSecretsEvent {
    KeysUpdated,
}

/// Singleton: manages the user's custom provider API keys.
pub struct AgentProviderSecrets {
    keys: HashMap<String, String>,
}

impl AgentProviderSecrets {
    /// Reads all keys from secure storage at startup.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        Self {
            keys: Self::load_from_storage(ctx),
        }
    }

    /// Reads the API key for the given provider; returns `None` if not configured.
    pub fn get(&self, provider_id: &str) -> Option<&str> {
        self.keys.get(provider_id).map(String::as_str)
    }

    /// Sets/updates the API key for a provider.
    /// Passing an empty string is equivalent to deleting it.
    pub fn set(&mut self, provider_id: &str, api_key: String, ctx: &mut ModelContext<Self>) {
        if api_key.is_empty() {
            self.keys.remove(provider_id);
        } else {
            self.keys.insert(provider_id.to_owned(), api_key);
        }
        ctx.emit(AgentProviderSecretsEvent::KeysUpdated);
        self.persist(ctx);
    }

    /// Removes a provider (along with its secret).
    pub fn remove(&mut self, provider_id: &str, ctx: &mut ModelContext<Self>) {
        if self.keys.remove(provider_id).is_some() {
            ctx.emit(AgentProviderSecretsEvent::KeysUpdated);
            self.persist(ctx);
        }
    }

    fn load_from_storage(ctx: &mut ModelContext<Self>) -> HashMap<String, String> {
        let raw = match ctx.secure_storage().read_value(SECURE_STORAGE_KEY) {
            Ok(json) => json,
            Err(secure_storage::Error::NotFound) => return HashMap::new(),
            Err(e) => {
                log::error!("Failed to read agent provider secrets: {e:#}");
                return HashMap::new();
            }
        };
        serde_json::from_str(&raw).unwrap_or_else(|e| {
            log::error!("Failed to deserialize agent provider secrets: {e:#}");
            HashMap::new()
        })
    }

    fn persist(&self, ctx: &mut ModelContext<Self>) {
        let json = match serde_json::to_string(&self.keys) {
            Ok(json) => json,
            Err(e) => {
                log::error!("Failed to serialize agent provider secrets: {e:#}");
                return;
            }
        };
        if let Err(e) = ctx.secure_storage().write_value(SECURE_STORAGE_KEY, &json) {
            log::error!("Failed to write agent provider secrets: {e:#}");
        }
    }
}

impl Entity for AgentProviderSecrets {
    type Event = AgentProviderSecretsEvent;
}

impl SingletonEntity for AgentProviderSecrets {}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
