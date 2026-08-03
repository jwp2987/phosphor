use super::*;

// NOTE: This fork's `crates/ai/src/api_keys.rs` is a heavily trimmed-down
// version of Warp's (219 lines vs. Warp's 836). The following product
// surfaces referenced by Warp's `api_keys_tests.rs` do not exist here at
// all, so the tests exercising them cannot be ported without inventing new
// product code (out of scope for a test port):
//
//   - `LLMProvider` / `LLMProvider::from_api_key_slug` — type absent from
//     the `ai` crate entirely (searched the whole crate).
//   - `ApiKeyManager::persist_provider_key` — absent; the fork instead has
//     per-provider setters (`set_google_key`, `set_anthropic_key`,
//     `set_openai_key`, `set_open_router_key`), which are infallible
//     (`()`, not `Result`), unlike Warp's fallible `persist_provider_key`.
//   - `ApiKeys::custom_endpoints` / `CustomEndpoint` / `CustomEndpointModel`
//     / `CustomEndpointSchema` — the whole custom-inference-endpoint
//     feature is absent from `ApiKeys`.
//   - `ApiKeys::provider_key_count` — method absent.
//   - `ApiKeyManager::custom_model_providers_for_request` — method absent
//     (depends on `custom_endpoints`, above).
//   - `GrokTokens` / grok subscription OAuth fields
//     (`grok_tokens`, `grok_refresh_allowed`, `grok_refresh_waiters`,
//     `has_grok_subscription`, `grok_expired_refresh_token`, etc.) — the
//     whole connected-Grok-subscription feature is absent.
//   - `GeapCredentials` / `GeapCredentialsState` / `GeapMintBinding` /
//     `GeapFederation` / `LoadGeapCredentialsError` / GEAP_* constants and
//     `ApiKeyManager::geap_*` methods — the whole Gemini Enterprise (GEAP)
//     credential-minting feature is absent.
//
// `ApiKeyManager::api_keys_for_request` also dropped its third parameter
// (`geap_binding: Option<GeapMintBinding>`), since GEAP is absent.
//
// See the PORT task report for the full NEEDS-ADAPTATION breakdown of
// Warp's 71 `api_keys_tests.rs` tests. Only the tests below exercise
// surface that still exists in this fork; their assertions are unchanged
// from Warp's.

fn make_manager(keys: ApiKeys) -> ApiKeyManager {
    ApiKeyManager {
        keys,
        aws_credentials_state: AwsCredentialsState::Missing,
        aws_credentials_refresh_strategy: AwsCredentialsRefreshStrategy::default(),
    }
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
