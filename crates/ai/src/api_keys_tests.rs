use super::*;
use warpui::App;

// Ported from Warp's `crates/ai/src/api_keys_tests.rs` at the pinned oracle
// (`02b53fcd8`, release `2026.07.29.09.05` stable — see `ORACLE.md`), which has
// 903 lines / 67 `#[test]`s. An earlier pass measured against `warp/master`
// (71 tests); the classification below is re-verified against the pin and the
// buckets are: 10 ported / 5 blocked / 16 superseded / 36 cloud = 67.
//
// Every test below keeps Warp's assertions verbatim; only *shape* is adapted
// (Warp's `persist_provider_key(LLMProvider, ..)` -> this fork's per-provider
// setters, and the dropped `geap_binding` parameter on `api_keys_for_request`).
// See issues #142 and #210.
//
// The remaining Warp tests are blocked, and the reason matters more than the
// count: in this fork `ApiKeyManager` is *not* the BYOP surface. The BYOP key
// store is `AgentProviderSecrets` (`app/src/ai/agent_providers/secrets.rs`,
// "Design modeled after `crates/ai/src/api_keys.rs::ApiKeyManager`"), which
// supports arbitrary user-defined providers with their own `base_url` rather
// than Warp's fixed four-provider list plus `custom_endpoints`. See
// `app/src/tui_export.rs` ("this fork's arbitrary-provider BYOP model instead
// of upstream's fixed 4-provider `ApiKeyManager`"). Its own persistence
// coverage now lives in `app/src/ai/agent_providers/secrets_tests.rs`.
//
// Blocked because the fork lacks the symbol (5 tests):
//
//   - `LLMProvider::from_api_key_slug` — the enum exists in this fork
//     (`app/src/ai/llms.rs`) but not the slug parser, because the parser only
//     exists at the pin to back the `--set-provider-api-key` /
//     `--clear-provider-api-key` CLI flags on Warp's TUI
//     (`crates/warp_tui/src/session.rs`), which this fork does not have.
//     Porting the tests means porting that CLI flow first; tracked as #225.
//     Blocks `llm_provider_parses_supported_api_key_provider_names` and
//     `llm_provider_rejects_unsupported_api_key_provider` (2).
//   - `ApiKeys::provider_key_count` — method absent. It is dead code at the
//     pin too (defined and tested, zero non-test call sites), so adding it
//     here would import dead code purely to host a test. Blocks the three
//     `provider_key_count_*` tests (3).
//
// Also absent, but not blocking any pinned test on their own:
//
//   - `ApiKeyManager::persist_provider_key` — Warp writes secure storage
//     *first* and returns `Result`, so a failed keychain write is surfaced
//     instead of being published as if it had been saved; the fork's setters
//     publish first and only `log::error!` a failed write. The two
//     `persisted_provider_api_key_*` tests are ported onto the setters below.
//   - `ApiKeyManager::reload_keys_from_secure_storage` — absent. Warp uses it
//     so a key written by a separate process (its TUI setup commands) is
//     picked up by the live app. Untested at the pin.
//   - `ApiKeyManager::has_any_key` — absent on the manager (present on
//     `ApiKeys`). At the pin it only adds "…or a connected Grok
//     subscription", so its four `manager_has_any_key_*` tests are counted
//     under cloud below.
//
// Superseded by the fork's own BYOP provider store, not simply dropped
// (16 tests) — `CustomEndpoint` / `CustomEndpointModel` /
// `CustomEndpointSchema` / `ApiKeys::custom_endpoints` /
// `ApiKeyManager::custom_model_providers_for_request` and their
// add/save/remove/clear methods. Warp's `custom_model_providers` payload is the
// wire format that tells Warp's *server* how to reach a custom endpoint on the
// user's behalf; this fork calls the endpoint directly through genai
// (`chat_stream::build_client`) and has no such proxy. The pinned
// `warp_multi_agent_api` rev also has no `CustomModelProviders` message.
// Porting them would stand up a second, competing provider store. The
// equivalent coverage lives on `AgentProviderSecrets`: 13 behaviour tests in
// `app/src/ai/agent_providers/mod_test.rs` plus the persistence tests in
// `app/src/ai/agent_providers/secrets_tests.rs`. Blocks:
// `custom_model_providers_preserves_configured_schema`,
// `serde_round_trip_with_custom_endpoints`,
// `serde_legacy_endpoint_defaults_to_chat_completions`,
// `has_any_key_true_for_custom_endpoints_only`,
// `has_any_key_false_for_endpoint_with_empty_api_key`,
// `custom_model_providers_none_when_empty`,
// `custom_model_providers_none_when_byo_disabled`,
// `custom_model_providers_populates_single_endpoint`,
// `multiple_endpoints_all_serialize`,
// `byok_disabled_returns_none_even_with_endpoints`,
// `empty_api_key_endpoints_are_skipped`,
// `endpoints_with_only_empty_models_are_skipped`, the three `display_label_*`,
// and `api_keys_for_request_none_for_custom_endpoints_only`.
//
// Cloud-only, no local/BYOP equivalent exists to run them against (36 tests):
//
//   - 24 tests — the `grok_*`, `has_grok_subscription_*` and
//     `manager_has_any_key_*` groups — cover `GrokTokens`, an OAuth token pair
//     minted by Warp's hosted xAI-subscription connect flow and refreshed by
//     `crate::grok_subscription` against Warp's servers. There is no
//     pasted-key path to run them against; a BYO xAI key is an ordinary
//     custom provider here. The two `manager_has_any_key_*` cases that do not
//     involve Grok are exact duplicates of `has_any_key_false_when_empty` and
//     `has_any_key_true_for_openai_only`, which are ported below.
//   - 12 tests — the `geap_*` group — cover Gemini Enterprise credentials
//     minted by Warp's backend for a `GeapMintBinding` (user uid +
//     workload-identity audience + service account). The binding is issued by
//     the cloud, so there is nothing local to bind to.
//
// Not a port, but noted: the AWS Bedrock branch of `api_keys_for_request` is
// byte-identical to the pin's and is untested at the pin as well as here.
// Tracked as #226.

fn make_manager(keys: ApiKeys) -> ApiKeyManager {
    ApiKeyManager {
        keys,
        aws_credentials_state: AwsCredentialsState::Missing,
        aws_credentials_refresh_strategy: AwsCredentialsRefreshStrategy::default(),
    }
}

// ── persisted provider keys ─────────────────────────────────────

#[test]
fn persisted_provider_api_key_updates_request_state() {
    App::test((), |mut app| async move {
        app.update(|ctx| secure_storage::register_noop("test", ctx));
        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.update(&mut app, |manager, ctx| {
            manager.set_anthropic_key(Some("sk-ant-test".to_owned()), ctx);
        });

        manager.read(&app, |manager, _| {
            let request_keys = manager
                .api_keys_for_request(true, false)
                .expect("persisted provider key should be available to requests");
            assert_eq!(request_keys.anthropic, "sk-ant-test");
        });
    });
}

#[test]
fn persisted_provider_api_key_can_be_cleared() {
    App::test((), |mut app| async move {
        app.update(|ctx| secure_storage::register_noop("test", ctx));
        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.update(&mut app, |manager, ctx| {
            manager.set_anthropic_key(Some("sk-ant-test".to_owned()), ctx);
            manager.set_anthropic_key(None, ctx);
        });

        manager.read(&app, |manager, _| {
            assert_eq!(manager.keys().anthropic, None);
        });
    });
}

// ── serde round-trip ────────────────────────────────────────────

#[test]
fn serde_round_trip_empty() {
    let keys = ApiKeys::default();
    let json = serde_json::to_string(&keys).unwrap();
    let deser: ApiKeys = serde_json::from_str(&json).unwrap();
    assert_eq!(keys, deser);
}

#[test]
fn serde_round_trip_with_provider_keys() {
    let keys = ApiKeys {
        openai: Some("sk-openai".into()),
        anthropic: Some("sk-ant-abc".into()),
        google: Some("AIzaSy123".into()),
        open_router: Some("sk-or-xxx".into()),
    };
    let json = serde_json::to_string(&keys).unwrap();
    let deser: ApiKeys = serde_json::from_str(&json).unwrap();
    assert_eq!(keys, deser);
}

#[test]
fn serde_ignores_unknown_fields() {
    // Warp's payload verbatim. `custom_endpoints` is one of the unknown fields
    // here rather than a parsed one, so Warp's second assertion
    // (`keys.custom_endpoints.is_empty()`) has no field to read and is not
    // portable; the tolerance it checks for is still exercised.
    let json = r#"{"openai":"sk-x","unknown_field":"value","custom_endpoints":[]}"#;
    let keys: ApiKeys = serde_json::from_str(json).unwrap();
    assert_eq!(keys.openai, Some("sk-x".into()));
}

// ── has_any_key ─────────────────────────────────────────────────

#[test]
fn has_any_key_false_when_empty() {
    assert!(!ApiKeys::default().has_any_key());
}

#[test]
fn has_any_key_true_for_openai_only() {
    let keys = ApiKeys {
        openai: Some("sk-x".into()),
        ..Default::default()
    };
    assert!(keys.has_any_key());
}

// ── api_keys_for_request ────────────────────────────────────────

#[test]
fn api_keys_for_request_none_when_empty() {
    let mgr = make_manager(ApiKeys::default());
    assert!(mgr.api_keys_for_request(true, false).is_none());
}

#[test]
fn api_keys_for_request_populates_provider_keys() {
    let mgr = make_manager(ApiKeys {
        openai: Some("sk-o".into()),
        anthropic: Some("sk-a".into()),
        ..Default::default()
    });
    let result = mgr.api_keys_for_request(true, false).unwrap();
    assert_eq!(result.openai, "sk-o");
    assert_eq!(result.anthropic, "sk-a");
    assert!(result.google.is_empty());
}

#[test]
fn api_keys_for_request_omits_keys_when_byo_disabled() {
    let mgr = make_manager(ApiKeys {
        openai: Some("sk-o".into()),
        ..Default::default()
    });
    // With BYO disabled and no other credentials, returns None.
    assert!(mgr.api_keys_for_request(false, false).is_none());
}
