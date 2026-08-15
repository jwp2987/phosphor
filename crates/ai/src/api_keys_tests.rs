use super::*;
use crate::LLMProvider;
use crate::secret_revision;
use warpui::App;
use warpui_extras::secure_storage::SecureStorage;

// Ported from Warp's `crates/ai/src/api_keys_tests.rs` at the pinned oracle
// (`02b53fcd8`, release `2026.07.29.09.05` stable — see `ORACLE.md`), which has
// 903 lines / 67 `#[test]`s. An earlier pass measured against `warp/master`
// (71 tests); the classification below is re-verified against the pin and the
// buckets are: 12 ported / 3 blocked / 16 superseded / 22 declined /
// 2 duplicated / 12 cloud = 67.
//
// Every test below keeps Warp's assertions verbatim; only *shape* is adapted
// (Warp's `persist_provider_key(LLMProvider, ..)` -> this fork's per-provider
// setters, and the dropped `geap_binding` parameter on `api_keys_for_request`).
// See issues #142 and #210.
//
// `llm_provider_parses_supported_api_key_provider_names` and
// `llm_provider_rejects_unsupported_api_key_provider` were blocked pending
// `LLMProvider::from_api_key_slug`, which now exists as `crate::LLMProvider`
// (`llm_provider.rs`) -- ported alongside the `--set-provider-api-key` /
// `--clear-provider-api-key` flags on `warp_tui`'s `TuiArgs`
// (`crates/warp_tui/src/session.rs`) that are its only caller. See issues
// #392 / #225 and `llm_provider.rs`'s module docs for why that type is
// narrower than, and separate from, `app`'s own `ai::llms::LLMProvider`.
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
// Blocked because the fork lacks the symbol (3 tests):
//
//   - `ApiKeys::provider_key_count` — method absent. It is dead code at the
//     pin too (defined and tested, zero non-test call sites), so adding it
//     here would import dead code purely to host a test. Blocks the three
//     `provider_key_count_*` tests (3). These are **not** part of the 16
//     `CustomEndpoint` tests #142 declines — `provider_key_count_zero_when_empty`
//     and `provider_key_count_counts_each_provider_key` never touch a custom
//     endpoint at all, and only the third's endpoint-exclusion assertion does.
//     The blocker is this absent method, which #142 says nothing about.
//
// Also absent, but not blocking any pinned test on their own:
//
//   - `ApiKeyManager::persist_provider_key` — Warp writes secure storage
//     *first* and returns `Result`, so a failed keychain write is surfaced
//     instead of being published as if it had been saved; the fork's setters
//     publish first and only `log::error!` a failed write. The two
//     `persisted_provider_api_key_*` tests are ported onto the setters below.
//   - `ApiKeyManager::has_any_key` — absent on the manager (present on
//     `ApiKeys`). At the pin it only adds "…or a connected Grok
//     subscription", so of its four `manager_has_any_key_*` tests the two that
//     mention Grok are counted under the declined subscription below and the
//     two that do not are counted as duplicates.
//
// No longer absent (this header used to list it):
//
//   - `ApiKeyManager::reload_keys_from_secure_storage` — now present, added
//     alongside the TUI API-key hot-reload hook (`app/src/ai/tui_api_keys.rs`).
//     Untested at the pin, so it carries no pinned tests either way.
//
// Two tests at the end of this file are fork-original rather than ported (they
// do not count against the buckets above): the cross-process revision stamp is
// a fork behaviour, because unlike upstream this fork shares one keyring
// between the GUI and the TUI. See the section header there.
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
// Declined, not cloud — the xAI/Grok subscription (22 tests). The `grok_*`,
// `has_grok_subscription_*`, `api_keys_for_request_*_grok_token*` and the two
// of four `manager_has_any_key_*` tests that mention Grok all cover
// `GrokTokens`, the OAuth token pair a connected xAI *subscription* yields. **That flow is not a cloud drop and this comment used to say it
// was** (#598): at the pin `crates/ai/src/grok_subscription/oauth.rs` holds
// `AUTHORIZE_URL = "https://auth.x.ai/oauth2/authorize"`,
// `TOKEN_URL = "https://auth.x.ai/oauth2/token"`,
// `REDIRECT_HOST = "127.0.0.1"` / `REDIRECT_PORT = 56121`, and
// `refresh_access_token` POSTs `grant_type=refresh_token` straight to that
// same xAI `TOKEN_URL` — no `warp.dev` host appears anywhere in the module's
// three files, and `grok_subscription/mod.rs`'s own doc says "The Grok
// subscription is BYO auth". These tests are out of scope because Phosphor
// declined the *subscription product* as a credential source — `DECLINED.md`
// #319, which spells this out ("The flow is genuinely local ... it is *not* a
// cloud drop — but it is an alternative credential *source*") — not because
// anything here talks to Warp. `DECLINED.md`'s "Not declined — common false
// positives" section names Grok OAuth for the same reason. A BYO xAI key is
// an ordinary custom provider here, so there is no pasted-key path these
// tests could be re-aimed at.
//
// Duplicates of tests already ported (2 tests). The two `manager_has_any_key_*`
// cases that do not involve Grok — `manager_has_any_key_false_when_no_keys_and_no_grok`
// and `manager_has_any_key_true_for_pasted_key_without_grok` — are exact
// duplicates of `has_any_key_false_when_empty` and
// `has_any_key_true_for_openai_only`, which are ported below.
//
// Cloud-only, no local/BYOP equivalent exists to run them against (12 tests).
// The `geap_*` and `api_keys_for_request_*_geap_token*` groups cover Gemini
// Enterprise credentials minted for a `GeapMintBinding` (user uid +
// workload-identity audience + service account).
// Unlike Grok above, this one is verified cloud: the exchange itself goes to
// Google (`STS_TOKEN_URL`, `IAM_GENERATE_ACCESS_TOKEN_URL` in the pin's
// `app/src/ai/geap_credentials.rs`), but its *only* input is a "brand-new Warp
// OIDC JWT" from `ManagedSecretManager::issue_task_identity_token`, which
// travels over `warp_graphql`; and the binding itself needs a Warp account
// (`AuthStateProvider::…user_id()`) plus team-workspace settings
// (`UserWorkspaces::gemini_enterprise_host_settings`). `crates/ai`'s own module
// doc concedes it: "The network-facing mint lives in the app layer, which has
// the workspace settings and Warp OIDC access this crate cannot see." This
// fork wires `ManagedSecretManager` to `DisabledManagedSecretsClient`
// (`app/src/lib.rs`), so there is nothing local to bind to.
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

// ── provider slug parsing (#392 / #225) ─────────────────────────

#[test]
fn llm_provider_parses_supported_api_key_provider_names() {
    assert_eq!(
        LLMProvider::from_api_key_slug("anthropic"),
        Ok(LLMProvider::Anthropic)
    );
    assert_eq!(
        LLMProvider::from_api_key_slug("open-ai"),
        Ok(LLMProvider::OpenAI)
    );
    assert_eq!(
        LLMProvider::from_api_key_slug("google"),
        Ok(LLMProvider::Google)
    );
    assert_eq!(LLMProvider::from_api_key_slug("grok"), Ok(LLMProvider::Xai));
}

#[test]
fn llm_provider_rejects_unsupported_api_key_provider() {
    assert_eq!(
        LLMProvider::from_api_key_slug("openrouter"),
        Err("provider must be one of: anthropic, openai, google, grok".to_owned())
    );
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

// ── cross-process revision stamp ────────────────────────────────
//
// Fork-original, not ported: the pin stamps the revision file from exactly one
// place, `zap-tui --set-provider-api-key`, because upstream's GUI has its own
// app id and keyring namespace and so cannot invalidate a TUI's cached keys.
// This fork shares one app id, one keyring and one config directory between the
// GUI and the TUI, so *every* write has to stamp it; the stamp therefore lives
// in this store's write choke point. See `crate::secret_revision` and
// `app/src/ai/tui_api_keys.rs`.

/// A [`SecureStorage`] whose writes always fail, so a test can tell a stamped
/// revision from a merely attempted write.
struct FailingSecureStorage;

impl SecureStorage for FailingSecureStorage {
    fn write_value(&self, _key: &str, _value: &str) -> Result<(), secure_storage::Error> {
        Err(secure_storage::Error::Unknown(anyhow::anyhow!(
            "keyring unavailable"
        )))
    }

    fn read_value(&self, _key: &str) -> Result<String, secure_storage::Error> {
        Err(secure_storage::Error::NotFound)
    }

    fn remove_value(&self, _key: &str) -> Result<(), secure_storage::Error> {
        Ok(())
    }
}

#[test]
fn saving_a_provider_key_stamps_the_cross_process_revision() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().expect("should create temp dir");
        let _revision_guard =
            secret_revision::SecretRevisionDirOverrideGuard::new(dir.path().to_owned());

        app.update(|ctx| secure_storage::register_noop("test", ctx));
        let manager = app.add_singleton_model(ApiKeyManager::new);

        assert_eq!(
            secret_revision::current_revision(),
            None,
            "nothing has been written yet, so there should be no revision file"
        );

        manager.update(&mut app, |manager, ctx| {
            manager.set_anthropic_key(Some("sk-ant-test".to_owned()), ctx);
        });
        let after_save = secret_revision::current_revision().expect(
            "saving a key should stamp the revision so other processes re-read the keyring",
        );

        manager.update(&mut app, |manager, ctx| {
            manager.set_anthropic_key(None, ctx);
        });
        let after_clear = secret_revision::current_revision()
            .expect("clearing a key should stamp the revision too");

        assert_ne!(
            after_save, after_clear,
            "every write needs a fresh stamp; an unchanged file wakes no watcher"
        );
    });
}

#[test]
fn a_failed_keyring_write_does_not_stamp_the_revision() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().expect("should create temp dir");
        let _revision_guard =
            secret_revision::SecretRevisionDirOverrideGuard::new(dir.path().to_owned());

        app.update(|ctx| {
            ctx.add_singleton_model(|_| -> secure_storage::Model {
                Box::new(FailingSecureStorage)
            });
        });
        let manager = app.add_singleton_model(ApiKeyManager::new);

        manager.update(&mut app, |manager, ctx| {
            manager.set_anthropic_key(Some("sk-ant-test".to_owned()), ctx);
        });

        assert_eq!(
            secret_revision::current_revision(),
            None,
            "a key that never reached the keyring must not tell anyone to re-read it -- this \
             process is still serving it from memory, and its own watcher would reload the old \
             value back over it"
        );
    });
}
