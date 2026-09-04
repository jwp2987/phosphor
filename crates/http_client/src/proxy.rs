//! Global HTTP proxy configuration.
//!
//! See Issue #72: Zap needs a globally configurable proxy setting that uniformly
//! covers all outbound HTTP requests (BYOP model-list fetching, autoupdate,
//! conversation loading, etc.).
//!
//! Design points:
//! - Three tiers of [`ProxyMode`]: `System` / `Custom` / `Off`.
//! - `System` falls back to reqwest's default behavior; the workspace's reqwest
//!   builds with default features, which include system proxy detection,
//!   so macOS reads SystemConfiguration, Windows reads WinINET, and Linux reads
//!   `HTTP_PROXY` and other environment variables — no need to implement this
//!   ourselves.
//! - `Custom` explicitly specifies a URL / basic auth / no_proxy list.
//! - `Off` calls [`reqwest::ClientBuilder::no_proxy`], fully disabling the proxy
//!   (including environment variables).
//!
//! The app injects configuration via [`set_global_proxy_config`] at startup /
//! when settings change; subsequent [`crate::Client::new`] calls read this
//! global value and apply it to reqwest.
//!
//! reqwest does not support switching the proxy of an already-constructed
//! `Client` at runtime, so callers must rebuild the Client instance after
//! changing settings (e.g. `AutoupdateState::new(http_client::Client::new())`).

use std::sync::{OnceLock, RwLock};

/// Global proxy mode.
///
/// Defaults to `System`, the same default as `app::settings::network::ProxyMode`.
///
/// This is the value in effect during cold start, before the app layer has read
/// settings.toml and called [`set_global_proxy_config`], so it decides what any
/// `Client` built in that window does. It used to be `Off`, to keep such a client
/// from picking up a system proxy reqwest detected on its own — but upstream Warp
/// builds reqwest with default features and no `no_proxy()` call outside tests, so
/// `Off` made the fork quietly *less* proxy-aware than the product it forks, both
/// during cold start and (via the settings default) forever after. Keeping the two
/// defaults identical also means the cold-start window and the post-injection state
/// agree for a stock install, instead of flipping proxying on partway through
/// startup. See Issue #638.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProxyMode {
    /// Disable the proxy, including environment variables.
    Off,
    /// Fully follow the system / environment variables (reqwest's default behavior).
    /// The default.
    #[default]
    System,
    /// Use the proxy explicitly configured in [`ProxyConfig::url`].
    Custom,
}

impl ProxyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ProxyMode::System => "system",
            ProxyMode::Custom => "custom",
            ProxyMode::Off => "off",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "system" => ProxyMode::System,
            "custom" => ProxyMode::Custom,
            // `off` / `disabled` / `none` are explicit opt-outs and are honoured as such.
            "off" | "disabled" | "none" => ProxyMode::Off,
            // Anything else is a typo or a value from a newer build. Fall back to the
            // default rather than to a hard-coded variant, so this can never drift away
            // from it again the way it did in Issue #638 — a mis-spelled mode should land
            // a user on the documented default, not silently disable their proxy.
            _ => ProxyMode::default(),
        }
    }
}

/// The resolved global proxy configuration.
///
/// `username` is stored in plaintext in settings.toml; `password` is saved
/// separately via `managed_secrets` (same pattern as the BYOP API key), and the
/// caller injects it into [`Self::password`] before assembling this struct.
#[derive(Clone, Debug, Default)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    /// e.g. `http://proxy.corp:8080`. Only effective under [`ProxyMode::Custom`].
    pub url: String,
    pub username: String,
    pub password: String,
    /// Comma-separated host list; an empty string means no exceptions.
    pub no_proxy: String,
}

impl ProxyConfig {
    /// Applies this configuration to a `reqwest::ClientBuilder`.
    ///
    /// On error (`Custom` mode but the URL is invalid), logs a warning and falls
    /// back to reqwest's default behavior instead of letting `Client::new()` panic.
    pub fn apply(&self, mut builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        match self.mode {
            ProxyMode::System => builder,
            ProxyMode::Off => builder.no_proxy(),
            ProxyMode::Custom => {
                let trimmed = self.url.trim();
                if trimmed.is_empty() {
                    log::warn!("HTTP proxy set to Custom but URL is empty, falling back to reqwest default (reads system proxy)");
                    return builder;
                }

                let proxy_result = reqwest::Proxy::all(trimmed);
                let mut proxy = match proxy_result {
                    Ok(p) => p,
                    Err(err) => {
                        log::warn!("HTTP proxy URL '{trimmed}' is invalid ({err}), falling back to reqwest default");
                        return builder;
                    }
                };

                if !self.username.is_empty() || !self.password.is_empty() {
                    proxy = proxy.basic_auth(&self.username, &self.password);
                }

                if !self.no_proxy.trim().is_empty() {
                    if let Some(no_proxy) = reqwest::NoProxy::from_string(self.no_proxy.trim()) {
                        proxy = proxy.no_proxy(Some(no_proxy));
                    }
                }

                builder = builder.proxy(proxy);
                builder
            }
        }
    }
}

static GLOBAL_PROXY_CONFIG: OnceLock<RwLock<ProxyConfig>> = OnceLock::new();

fn slot() -> &'static RwLock<ProxyConfig> {
    GLOBAL_PROXY_CONFIG.get_or_init(|| RwLock::new(ProxyConfig::default()))
}

/// Installs a new global proxy configuration.
///
/// Only affects `Client`s constructed after this call. Once a `reqwest::Client`
/// is constructed, its proxy can't be switched, so the app layer must rebuild
/// all shared Client instances after changing settings.
pub fn set_global_proxy_config(cfg: ProxyConfig) {
    let lock = slot();
    if let Ok(mut guard) = lock.write() {
        *guard = cfg;
    } else {
        log::error!("Failed to write global HTTP proxy config: RwLock poisoned");
    }
}

/// Reads the current global proxy configuration (returns the default if unset).
pub fn current_proxy_config() -> ProxyConfig {
    let lock = slot();
    match lock.read() {
        Ok(guard) => guard.clone(),
        Err(err) => {
            log::error!("Failed to read global HTTP proxy config: RwLock poisoned ({err})");
            ProxyConfig::default()
        }
    }
}

#[cfg(test)]
#[path = "proxy_tests.rs"]
mod tests;
