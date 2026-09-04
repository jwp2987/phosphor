//! Unit tests for the global proxy settings.
//!
//! There is no Warp test to port here: the setting is fork-only (Issue #72), because Warp
//! honours the environment proxy unconditionally and exposes no control over it at all. What
//! these pin down is the one part that *does* have a Warp counterpart — the default. See the
//! module doc for why it is `System` and Issue #638 for how it came to be `Off`.

use super::*;
use settings::Setting;

/// The shipped default must be `System`.
///
/// This asserts the `default:` expression in the `define_settings_group!` invocation
/// directly, so reverting that single line fails here rather than silently. `Off` makes the
/// app ignore `HTTPS_PROXY` / `HTTP_PROXY` on a stock install while three other surfaces —
/// the module doc, the generated schema description, and the settings-page string
/// `settings-network-mode-description` — all continue to say `system`.
#[test]
fn proxy_mode_defaults_to_system() {
    assert_eq!(ProxyModeSetting::default_value(), ProxyMode::System);

    // The same value seen through the path a fresh settings.toml with no `[network]` table
    // takes: `Setting::new(None)` falls back to `default_value()`.
    let setting = ProxyModeSetting::new(None);
    assert_eq!(*setting.value(), ProxyMode::System);
    assert!(!setting.is_value_explicitly_set());
}

/// The default has to arrive as `System` at *both* exit points.
///
/// `http_client` and `websocket` each carry their own mirror of this enum (the mirrors exist
/// to break a `websocket -> http_client -> warp_core -> websocket` dependency cycle), and
/// `settings::init::apply_network_settings_to_global_slots` pushes into both. A mirror left
/// defaulting to `Off` would leave HTTP proxied and WebSockets not, or vice versa — the kind
/// of split that only shows up as "the agent stream works but the model list doesn't".
#[test]
fn default_proxy_mode_reaches_both_transports_as_system() {
    let default = ProxyMode::default();
    assert_eq!(default, ProxyMode::System);
    assert_eq!(default.to_http_client_mode(), http_client::ProxyMode::System);
    assert_eq!(default.to_websocket_mode(), websocket::ProxyMode::System);
}

/// The companion fields stay empty by default.
///
/// `System` mode ignores all three, but an empty `proxy_url` is also what makes a
/// `Custom`-without-URL misconfiguration fall back loudly (a logged warning) instead of
/// pointing at a stale value someone forgot about.
#[test]
fn proxy_url_username_and_no_proxy_default_to_empty() {
    assert_eq!(ProxyUrlSetting::default_value(), String::new());
    assert_eq!(ProxyUsernameSetting::default_value(), String::new());
    assert_eq!(ProxyNoProxySetting::default_value(), String::new());
}
