//! Global HTTP network proxy settings.
//!
//! See Issue #72. Provides a user-configurable global proxy setting whose value is injected
//! into both the `http_client::Client` and `websocket` exit points, thereby covering all
//! outbound HTTP/WS requests: BYOP calls, autoupdate, conversation loading, MCP OAuth, cloud
//! workflow fetch, etc.
//!
//! Three fields:
//! - `proxy_mode`: `system` / `custom` / `off` (defaults to `system`, matching reqwest's
//!   existing behavior).
//! - `proxy_url`: used in `Custom` mode, e.g. `http://proxy.corp:8080`.
//! - `proxy_no_proxy`: comma-separated list of host exceptions, e.g.
//!   `localhost,127.0.0.1,.internal`.
//!
//! Username / password are not here: the username will go into a separate setting (or be
//! written into the URL), and the password goes through `managed_secrets` (same pattern as
//! the BYOP API key), managed separately by the UI.
//!
//! To keep the first version simple, a username field is also provided here; the password is
//! still managed by managed_secrets.
//!
//! # Why the default is `system` (see Issue #638)
//!
//! It briefly defaulted to `off`, on the theory that a system proxy reqwest picked up on its
//! own could surprise a user by intercepting local calls. That is a divergence from Warp, not
//! a hardening measure, and it is the kind §5.10 forbids: at the pinned oracle
//! (`ORACLE.md`), Warp honours the environment proxy *unconditionally* and offers no switch
//! at all. `crates/websocket/src/native.rs::connect` calls `proxy::resolve_proxy` on every
//! connection, which reads `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY`, and
//! `http_client::Client::new` builds reqwest with default features (system-proxy detection
//! on) — only `new_for_test` adds `.no_proxy()`, and that is for test speed. So a Warp user
//! behind a corporate proxy gets proxying; on `off` the same user got silence, with both the
//! settings page and this module doc telling them otherwise.
//!
//! The concern that motivated `off` is real but is answered by the setting existing: a
//! machine carrying a stale `HTTPS_PROXY` can be set to `off`, which is an escape hatch Warp
//! does not have. Defaulting to `off` inverted that — it broke the common case to protect the
//! rare one, and made the fork's own ported Warp tests need a non-default global installed
//! before they would exercise the env-var path at all.

use serde::{Deserialize, Serialize};
use settings::{macros::define_settings_group, SupportedPlatforms, SyncToCloud};

/// The user-visible proxy mode.
///
/// Corresponds one-to-one with `http_client::ProxyMode` / `websocket::ProxyMode`; it is
/// defined separately to decouple the config layer from the infrastructure layer, and because
/// this type needs to implement `JsonSchema` and other traits required by the settings system.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "HTTP proxy mode: system follows the system/environment proxy (default); custom uses an explicit URL; off disables proxying entirely, including environment variables.",
    rename_all = "snake_case"
)]
pub enum ProxyMode {
    /// Forcibly disables the proxy, including `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY`.
    ///
    /// Not the default. This is the opt-out for a machine whose environment carries a stale
    /// or unreachable proxy — a case Warp gives the user no way to escape at all.
    Off,
    /// Follows the system proxy / environment variables (reqwest's default behavior).
    ///
    /// The default, matching what Warp does unconditionally; see the module doc.
    #[default]
    System,
    /// Uses the URL the user filled in.
    Custom,
}

impl ProxyMode {
    /// Converts to `http_client::ProxyMode`.
    pub fn to_http_client_mode(self) -> http_client::ProxyMode {
        match self {
            ProxyMode::System => http_client::ProxyMode::System,
            ProxyMode::Custom => http_client::ProxyMode::Custom,
            ProxyMode::Off => http_client::ProxyMode::Off,
        }
    }

    /// Converts to `websocket::ProxyMode` (an independent mirror; see the comment at the top of websocket/proxy.rs).
    pub fn to_websocket_mode(self) -> websocket::ProxyMode {
        match self {
            ProxyMode::System => websocket::ProxyMode::System,
            ProxyMode::Custom => websocket::ProxyMode::Custom,
            ProxyMode::Off => websocket::ProxyMode::Off,
        }
    }
}

define_settings_group!(NetworkSettings, settings: [
    proxy_mode: ProxyModeSetting {
        type: ProxyMode,
        // `System`, not `Off`: Warp honours the environment proxy unconditionally, so `Off`
        // would be a silent behavioural divergence. See the module doc and Issue #638.
        default: ProxyMode::System,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "network.proxy_mode",
        description: "HTTP proxy mode: system (default) / custom / off.",
    },
    proxy_url: ProxyUrlSetting {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "network.proxy_url",
        description: "The proxy URL used in Custom mode, e.g. http://proxy.corp:8080.",
    },
    proxy_username: ProxyUsernameSetting {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "network.proxy_username",
        description: "The proxy username used in Custom mode; empty means no basic auth or no username.",
    },
    proxy_no_proxy: ProxyNoProxySetting {
        type: String,
        default: String::new(),
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "network.proxy_no_proxy",
        description: "Comma-separated list of host exceptions, e.g. localhost,127.0.0.1,.internal.",
    },
]);

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
