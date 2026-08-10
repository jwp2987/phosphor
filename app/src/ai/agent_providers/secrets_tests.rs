//! Persistence coverage for [`AgentProviderSecrets`], this fork's BYOP key store.
//!
//! Adapted from Warp's `crates/ai/src/api_keys_tests.rs` at the pinned oracle
//! (`02b53fcd8`, release `2026.07.29.09.05` stable — see `ORACLE.md`). Warp's
//! serde / persistence tests target `ApiKeys` and its `custom_endpoints` list;
//! in this fork that store is superseded by [`AgentProviderSecrets`], which
//! keys secrets by arbitrary provider id rather than by Warp's fixed four
//! providers. Each test below records the Warp test it was adapted from and
//! why the shape changed. See #142 and #210.
//!
//! These tests deliberately do *not* assert anything about
//! `AgentProviderSecretsEvent::KeysUpdated` delivery to the GUI surfaces
//! described in #154; that defect is live and must not be papered over here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use warpui::App;
use warpui_extras::secure_storage::{self, Error, SecureStorage};

use super::*;

/// An in-memory [`SecureStorage`] so a write can actually be read back.
///
/// `secure_storage::register_noop` cannot be used for round-trip assertions:
/// its `read_value` returns `Ok("")` unconditionally, so nothing written is
/// ever observable again.
#[derive(Clone, Default)]
struct InMemorySecureStorage(Rc<RefCell<HashMap<String, String>>>);

impl InMemorySecureStorage {
    /// The raw JSON currently persisted under the secrets key, if any.
    fn raw(&self) -> Option<String> {
        self.0.borrow().get(SECURE_STORAGE_KEY).cloned()
    }

    fn seed(&self, value: &str) {
        self.0
            .borrow_mut()
            .insert(SECURE_STORAGE_KEY.to_owned(), value.to_owned());
    }
}

impl SecureStorage for InMemorySecureStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
        self.0.borrow_mut().insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn read_value(&self, key: &str) -> Result<String, Error> {
        self.0.borrow().get(key).cloned().ok_or(Error::NotFound)
    }

    fn remove_value(&self, key: &str) -> Result<(), Error> {
        self.0.borrow_mut().remove(key);
        Ok(())
    }
}

fn register_storage(app: &mut App, store: &InMemorySecureStorage) {
    let store = store.clone();
    app.update(move |ctx| {
        ctx.add_singleton_model(move |_| -> secure_storage::Model { Box::new(store) });
    });
}

/// The persisted JSON decoded back into the map shape `AgentProviderSecrets`
/// stores.
fn persisted_map(store: &InMemorySecureStorage) -> HashMap<String, String> {
    let raw = store.raw().expect("secrets should have been persisted");
    serde_json::from_str(&raw).expect("persisted secrets should be valid JSON")
}

// ── serde round-trip ────────────────────────────────────────────

/// Adapted from Warp `serde_round_trip_empty`. Warp round-trips an
/// `ApiKeys::default()` value directly; this fork's store is only reachable
/// through secure storage, so the round-trip goes through a write and a fresh
/// load instead.
#[test]
fn serde_round_trip_empty() {
    App::test((), |mut app| async move {
        let store = InMemorySecureStorage::default();
        register_storage(&mut app, &store);
        let secrets = app.add_singleton_model(AgentProviderSecrets::new);

        // Writing then clearing leaves an empty — not absent — persisted blob.
        secrets.update(&mut app, |secrets, ctx| {
            secrets.set("provider-a", "sk-a".to_owned(), ctx);
            secrets.remove("provider-a", ctx);
        });

        assert_eq!(persisted_map(&store), HashMap::new());

        let reloaded = app.add_model(AgentProviderSecrets::new);
        reloaded.read(&app, |reloaded, _| {
            assert_eq!(reloaded.get("provider-a"), None);
        });
    });
}

/// Adapted from Warp `serde_round_trip_with_provider_keys`. Warp asserts its
/// fixed four fields survive a round trip; this fork keys by arbitrary provider
/// id, so the equivalent assertion is that every configured provider survives.
#[test]
fn serde_round_trip_with_provider_keys() {
    App::test((), |mut app| async move {
        let store = InMemorySecureStorage::default();
        register_storage(&mut app, &store);
        let secrets = app.add_singleton_model(AgentProviderSecrets::new);

        secrets.update(&mut app, |secrets, ctx| {
            secrets.set("openai", "sk-openai".to_owned(), ctx);
            secrets.set("anthropic", "sk-ant-abc".to_owned(), ctx);
            secrets.set("google", "AIzaSy123".to_owned(), ctx);
            secrets.set("open_router", "sk-or-xxx".to_owned(), ctx);
            // Beyond Warp's fixed four: an arbitrary user-defined provider.
            secrets.set("my-ollama", "local-token".to_owned(), ctx);
        });

        let expected: HashMap<String, String> = [
            ("openai", "sk-openai"),
            ("anthropic", "sk-ant-abc"),
            ("google", "AIzaSy123"),
            ("open_router", "sk-or-xxx"),
            ("my-ollama", "local-token"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        assert_eq!(persisted_map(&store), expected);

        let reloaded = app.add_model(AgentProviderSecrets::new);
        reloaded.read(&app, |reloaded, _| {
            assert_eq!(reloaded.get("openai"), Some("sk-openai"));
            assert_eq!(reloaded.get("anthropic"), Some("sk-ant-abc"));
            assert_eq!(reloaded.get("google"), Some("AIzaSy123"));
            assert_eq!(reloaded.get("open_router"), Some("sk-or-xxx"));
            assert_eq!(reloaded.get("my-ollama"), Some("local-token"));
        });
    });
}

/// Adapted from Warp `serde_ignores_unknown_fields`. Warp checks that an
/// unrecognised field in the stored blob does not fail the whole load; this
/// fork's blob is a plain map with no field names, so the equivalent tolerance
/// check is that an undecodable payload degrades to an empty store rather than
/// panicking.
#[test]
fn corrupt_secure_storage_payload_loads_as_empty() {
    App::test((), |mut app| async move {
        let store = InMemorySecureStorage::default();
        store.seed("{ this is not json");
        register_storage(&mut app, &store);

        let secrets = app.add_singleton_model(AgentProviderSecrets::new);
        secrets.read(&app, |secrets, _| {
            assert_eq!(secrets.get("openai"), None);
        });
    });
}

// ── persisted provider keys ─────────────────────────────────────

/// Adapted from Warp `persisted_provider_api_key_updates_request_state`. Warp
/// asserts the persisted key reaches `api_keys_for_request`; this fork sends
/// BYOP keys directly from the store at call time (`chat_stream::build_client`)
/// rather than through a request-settings blob, so the equivalent assertion is
/// that the key survives to a fresh load of the store.
#[test]
fn persisted_provider_api_key_survives_a_reload() {
    App::test((), |mut app| async move {
        let store = InMemorySecureStorage::default();
        register_storage(&mut app, &store);
        let secrets = app.add_singleton_model(AgentProviderSecrets::new);

        secrets.update(&mut app, |secrets, ctx| {
            secrets.set("anthropic", "sk-ant-test".to_owned(), ctx);
        });

        let reloaded = app.add_model(AgentProviderSecrets::new);
        reloaded.read(&app, |reloaded, _| {
            assert_eq!(reloaded.get("anthropic"), Some("sk-ant-test"));
        });
    });
}

/// Adapted from Warp `persisted_provider_api_key_can_be_cleared`. Warp clears
/// by persisting `None` for the provider; this fork clears with `remove`.
#[test]
fn persisted_provider_api_key_can_be_cleared() {
    App::test((), |mut app| async move {
        let store = InMemorySecureStorage::default();
        register_storage(&mut app, &store);
        let secrets = app.add_singleton_model(AgentProviderSecrets::new);

        secrets.update(&mut app, |secrets, ctx| {
            secrets.set("anthropic", "sk-ant-test".to_owned(), ctx);
            secrets.remove("anthropic", ctx);
        });

        secrets.read(&app, |secrets, _| {
            assert_eq!(secrets.get("anthropic"), None);
        });

        // The clear must reach storage too, not just memory: otherwise the key
        // returns on the next launch.
        let reloaded = app.add_model(AgentProviderSecrets::new);
        reloaded.read(&app, |reloaded, _| {
            assert_eq!(reloaded.get("anthropic"), None);
        });
    });
}

/// Adapted from Warp `has_any_key_false_for_endpoint_with_empty_api_key`. Warp
/// treats a blank endpoint key as "no key configured"; this fork's `set`
/// documents an empty key as equivalent to deleting it, so the equivalent
/// assertion is that it clears the provider in memory *and* in storage.
#[test]
fn setting_an_empty_api_key_clears_the_provider() {
    App::test((), |mut app| async move {
        let store = InMemorySecureStorage::default();
        register_storage(&mut app, &store);
        let secrets = app.add_singleton_model(AgentProviderSecrets::new);

        secrets.update(&mut app, |secrets, ctx| {
            secrets.set("openai", "sk-x".to_owned(), ctx);
            secrets.set("openai", String::new(), ctx);
        });

        secrets.read(&app, |secrets, _| {
            assert_eq!(secrets.get("openai"), None);
        });
        assert_eq!(persisted_map(&store), HashMap::new());
    });
}

// ── cross-process reload ────────────────────────────────────────
//
// Fork-original, with no Warp counterpart: this store does not exist at the pin
// (upstream keeps BYOP keys in `ApiKeys::custom_endpoints`), and the pin stamps
// its revision file only from `zap-tui`'s key CLI because upstream's GUI has a
// separate app id and keyring. This fork shares one app id, one keyring and one
// config directory between the GUI and the TUI, so a GUI-side edit and a
// TUI-side edit invalidate each other symmetrically. See `ai::secret_revision`
// and `crate::ai::tui_api_keys`.

/// Covers every GUI and TUI mutation at once: `set` and `remove` are the only
/// ways into `persist`, and the GUI Settings > AI actions
/// (`SaveAgentProviderEdits`, `UpdateAgentProviderApiKey`, `RemoveAgentProvider`)
/// and the TUI's `/api-keys` picker all go through them.
#[test]
fn writing_a_provider_key_stamps_the_cross_process_revision() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().expect("should create temp dir");
        let _revision_guard =
            ::ai::secret_revision::SecretRevisionDirOverrideGuard::new(dir.path().to_owned());

        let store = InMemorySecureStorage::default();
        register_storage(&mut app, &store);
        let secrets = app.add_singleton_model(AgentProviderSecrets::new);

        assert_eq!(
            ::ai::secret_revision::current_revision(),
            None,
            "nothing has been written yet, so there should be no revision file"
        );

        secrets.update(&mut app, |secrets, ctx| {
            secrets.set("my-provider", "sk-a".to_owned(), ctx);
        });
        let after_set = ::ai::secret_revision::current_revision().expect(
            "saving a BYOP key from the GUI should stamp the revision so a running TUI \
             re-reads the keyring",
        );

        secrets.update(&mut app, |secrets, ctx| {
            secrets.remove("my-provider", ctx);
        });
        let after_remove = ::ai::secret_revision::current_revision()
            .expect("deleting a provider should stamp the revision too");

        assert_ne!(
            after_set, after_remove,
            "every write needs a fresh stamp; an unchanged file wakes no watcher"
        );
    });
}

/// The other half of the mechanism: the stamp is only useful if the reload it
/// triggers actually picks up the other process's write.
#[test]
fn reloading_picks_up_another_processs_write() {
    App::test((), |mut app| async move {
        let store = InMemorySecureStorage::default();
        register_storage(&mut app, &store);
        let secrets = app.add_singleton_model(AgentProviderSecrets::new);

        secrets.read(&app, |secrets, _| {
            assert_eq!(secrets.get("my-provider"), None);
        });

        // Stands in for another Zap process writing the shared keyring.
        store.seed(r#"{"my-provider":"sk-from-the-other-process"}"#);

        secrets.update(&mut app, |secrets, ctx| {
            secrets.reload_from_secure_storage(ctx);
        });

        secrets.read(&app, |secrets, _| {
            assert_eq!(
                secrets.get("my-provider"),
                Some("sk-from-the-other-process"),
                "a reload must discard the copy read at startup, not merge with it"
            );
        });
    });
}
