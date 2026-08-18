//! Unit tests for the `proxy` module.
//!
//! reqwest 0.13 doesn't have a public API for us to query the `Proxy` registered
//! on a `ClientBuilder`, so this can only do minimal verification via observable
//! behavior (whether the `Client` constructed after `apply` succeeds).
//! Finer-grained "does it actually go through the proxy" checks are left to
//! integration tests (which require a local mitm setup).
//!
//! Note: reqwest's `.build()` wants a rustls crypto provider to be resolvable,
//! and historically panicked when none was installed. Production code installs
//! one via `app/src/lib.rs::init_common`; in the unit test process we install it
//! ourselves. Kept after the 0.13 bump (0.13 bundles aws-lc-rs, so this is now
//! defensive rather than load-bearing) because `install_default` is idempotent
//! here — the return value is discarded.

use super::*;
use std::sync::Once;

static INSTALL_CRYPTO_PROVIDER: Once = Once::new();

/// Called before tests that run reqwest `.build()`; only takes effect the first time.
fn ensure_crypto_provider() {
    INSTALL_CRYPTO_PROVIDER.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// Builds a builder with native CA loading disabled, avoiding build failures in environments where system certificates are hard to obtain.
fn test_builder() -> reqwest::ClientBuilder {
    ensure_crypto_provider();
    reqwest::ClientBuilder::new().tls_certs_only([])
}

#[test]
fn proxy_mode_from_str_lenient_handles_variants() {
    assert_eq!(ProxyMode::from_str_lenient("system"), ProxyMode::System);
    assert_eq!(ProxyMode::from_str_lenient("SYSTEM"), ProxyMode::System);
    assert_eq!(ProxyMode::from_str_lenient("custom"), ProxyMode::Custom);
    assert_eq!(ProxyMode::from_str_lenient("off"), ProxyMode::Off);
    assert_eq!(ProxyMode::from_str_lenient("disabled"), ProxyMode::Off);
    assert_eq!(ProxyMode::from_str_lenient("none"), ProxyMode::Off);
    // Unknown values fall back to Off, matching the default, avoiding an unexpected system proxy.
    assert_eq!(ProxyMode::from_str_lenient("wat"), ProxyMode::Off);
}

#[test]
fn proxy_mode_as_str_roundtrip() {
    for mode in [ProxyMode::System, ProxyMode::Custom, ProxyMode::Off] {
        let s = mode.as_str();
        assert_eq!(ProxyMode::from_str_lenient(s), mode);
    }
}

#[test]
fn apply_system_returns_default_builder() {
    let cfg = ProxyConfig {
        mode: ProxyMode::System,
        ..Default::default()
    };
    // Verify it doesn't panic and can build successfully.
    let builder = cfg.apply(test_builder()).no_proxy();
    // The extra no_proxy() above is only to avoid actually resolving the system
    // proxy during build; the core assertion is that apply doesn't panic.
    let _client = builder.build().expect("System mode should build successfully");
}

#[test]
fn apply_off_disables_proxy_without_error() {
    let cfg = ProxyConfig {
        mode: ProxyMode::Off,
        ..Default::default()
    };
    let builder = cfg.apply(test_builder());
    let _client = builder.build().expect("Off mode should build successfully");
}

#[test]
fn apply_custom_with_valid_url_succeeds() {
    let cfg = ProxyConfig {
        mode: ProxyMode::Custom,
        url: "http://proxy.corp:8080".to_string(),
        ..Default::default()
    };
    let builder = cfg.apply(test_builder());
    let _client = builder
        .build()
        .expect("Custom mode + valid URL should build successfully");
}

#[test]
fn apply_custom_with_basic_auth_succeeds() {
    let cfg = ProxyConfig {
        mode: ProxyMode::Custom,
        url: "http://proxy.corp:8080".to_string(),
        username: "alice".to_string(),
        password: "s3cret".to_string(),
        ..Default::default()
    };
    let builder = cfg.apply(test_builder());
    let _client = builder.build().expect("Custom + auth should build successfully");
}

#[test]
fn apply_custom_with_no_proxy_list_succeeds() {
    let cfg = ProxyConfig {
        mode: ProxyMode::Custom,
        url: "http://proxy.corp:8080".to_string(),
        no_proxy: "localhost,127.0.0.1,.internal".to_string(),
        ..Default::default()
    };
    let builder = cfg.apply(test_builder());
    let _client = builder.build().expect("Custom + no_proxy should build successfully");
}

#[test]
fn apply_custom_with_empty_url_falls_back_silently() {
    let cfg = ProxyConfig {
        mode: ProxyMode::Custom,
        url: String::new(),
        ..Default::default()
    };
    // Shouldn't panic; equivalent to falling back to System (reqwest default).
    let builder = cfg.apply(test_builder()).no_proxy();
    let _client = builder.build().expect("empty URL should fall back silently");
}

#[test]
fn apply_custom_with_invalid_url_falls_back_silently() {
    let cfg = ProxyConfig {
        mode: ProxyMode::Custom,
        url: "://not a url".to_string(),
        ..Default::default()
    };
    let builder = cfg.apply(test_builder()).no_proxy();
    let _client = builder.build().expect("invalid URL should fall back silently");
}

#[test]
fn set_and_read_global_config_roundtrip() {
    // Note: OnceLock is global, so isolation between tests can't be assumed; this only verifies that what's read back after set matches what was written.
    let cfg = ProxyConfig {
        mode: ProxyMode::Custom,
        url: "http://test-proxy:1234".to_string(),
        username: "u".to_string(),
        password: "p".to_string(),
        no_proxy: "a,b".to_string(),
    };
    set_global_proxy_config(cfg.clone());
    let read_back = current_proxy_config();
    assert_eq!(read_back.mode, cfg.mode);
    assert_eq!(read_back.url, cfg.url);
    assert_eq!(read_back.username, cfg.username);
    assert_eq!(read_back.password, cfg.password);
    assert_eq!(read_back.no_proxy, cfg.no_proxy);

    // Reset back to default to avoid polluting other tests.
    set_global_proxy_config(ProxyConfig::default());
}
