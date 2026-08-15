//! The cross-process revision stamp for this fork's shared secret stores.
//!
//! # Why this exists
//!
//! Both secret stores read the OS keyring exactly once, when their singleton is
//! constructed:
//!
//! - [`crate::api_keys::ApiKeyManager`] (the pinned oracle's fixed four-provider
//!   BYOK store, secure-storage key `AiApiKeys`), and
//! - `app`'s `AgentProviderSecrets` (this fork's arbitrary-provider BYOP store,
//!   secure-storage key `AgentProviderSecrets`).
//!
//! A key can also be written by a *different* Zap process. Unlike upstream Warp,
//! the GUI and the TUI here deliberately share one app id, and therefore one
//! keyring namespace and one `config_local_dir()` (see `DECLINED.md`, "TUI/GUI
//! shared app id"). So *every* write races every other running process's cached
//! copy, in both directions:
//!
//! - `zap-tui --set-provider-api-key` / `--clear-provider-api-key` writes
//!   `ApiKeyManager` and exits (`crates/warp_tui/src/session.rs`);
//! - the TUI's `/api-keys` picker writes `AgentProviderSecrets`
//!   (`app/src/tui_export.rs::tui_set_agent_provider_api_key`);
//! - the GUI's Settings > AI page writes `AgentProviderSecrets` too
//!   (`app/src/settings_view/ai_page.rs`, the `SaveAgentProviderEdits`,
//!   `UpdateAgentProviderApiKey` and `RemoveAgentProvider` actions).
//!
//! Without a signal, each already-running process keeps serving the keys it read
//! at startup, and the key the user just saved looks like it was ignored until
//! the app is restarted.
//!
//! # The mechanism
//!
//! A revision file in the shared config directory. The writer stamps a fresh
//! UUID into it; every other process notices via `WarpManagedPathsWatcher` and
//! re-reads secure storage. The file's *contents* are not the payload — the keys
//! still come from the OS keyring — so nothing secret is written to disk.
//! Writing a new UUID rather than, say, touching an mtime keeps the change
//! visible to a content-hashing watcher as well as a metadata-watching one.
//!
//! The subscriber half lives in `app/src/ai/tui_api_keys.rs`, because the
//! filesystem watcher it hangs off is an `app` singleton.
//!
//! # Deviations from the pin
//!
//! The pin has this logic inline in `app/src/ai/tui_api_keys.rs` and resolves
//! the path against `warp_core::paths::tui_config_local_dir()`, which does not
//! exist in this fork and must not be invented — see the shared-app-id note
//! above. It is a free function here, rather than in `app`, only so that the
//! store that owns the write (`ApiKeyManager`, in this crate) can stamp the
//! revision itself instead of relying on each caller to remember to.

use std::path::PathBuf;

use anyhow::Context as _;
use uuid::Uuid;

const REVISION_FILE_NAME: &str = "api_keys.revision";

/// The file whose changes mean "re-read secure storage".
///
/// Shared by both secret stores on purpose: a spurious reload costs one keyring
/// read of an unchanged value, whereas a missed reload serves a stale key.
pub fn revision_file_path() -> PathBuf {
    revision_dir().join(REVISION_FILE_NAME)
}

#[cfg(not(any(test, feature = "test-util")))]
fn revision_dir() -> PathBuf {
    warp_core::paths::config_local_dir()
}

/// In test builds the revision file never goes near the developer's real config
/// directory. Tests that assert on the stamp install a
/// [`SecretRevisionDirOverrideGuard`]; tests that merely happen to write a key
/// land in a shared scratch directory whose contents nothing reads.
#[cfg(any(test, feature = "test-util"))]
fn revision_dir() -> PathBuf {
    dir_override::get()
        .unwrap_or_else(|| std::env::temp_dir().join("zap-secret-revision-test-scratch"))
}

/// Signals other running Zap processes to reload their keys from secure storage.
///
/// Call this *after* the write to secure storage has completed, and only if that
/// write succeeded, so a reader that reacts immediately cannot observe the old
/// value — and so a process whose keyring write failed does not talk itself into
/// discarding the in-memory key it is still serving.
pub fn bump() -> anyhow::Result<()> {
    let path = revision_file_path();
    let parent = path
        .parent()
        .context("API-key revision path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
    std::fs::write(&path, Uuid::new_v4().to_string())
        .with_context(|| format!("Failed to update API-key revision {}", path.display()))
}

/// [`bump`] for the in-process write paths, which have no caller to return an
/// error to. A failure here means only that other processes keep stale keys
/// until they restart; the key itself is already saved, so it must not be
/// escalated past a warning.
pub fn bump_or_log() {
    if let Err(error) = bump() {
        log::warn!(
            "A secret was saved, but signalling running Phosphor processes to reload it failed: \
             {error:#}"
        );
    }
}

/// Test-only override for [`revision_file_path`]'s directory, mirroring the
/// thread-local RAII pattern of `app`'s `settings::TomlPathOverrideGuard`. Lets
/// a test point the revision file at a temp directory it owns, so the stamp it
/// asserts on cannot be written by another test running in parallel.
#[cfg(any(test, feature = "test-util"))]
mod dir_override {
    use std::cell::RefCell;
    use std::path::PathBuf;

    thread_local! {
        static DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    pub(crate) fn get() -> Option<PathBuf> {
        DIR_OVERRIDE.with(|dir| dir.borrow().clone())
    }

    /// RAII guard that overrides the revision directory on the current thread
    /// and restores the previous value on drop.
    #[must_use]
    pub struct SecretRevisionDirOverrideGuard(Option<PathBuf>);

    impl SecretRevisionDirOverrideGuard {
        pub fn new(dir: PathBuf) -> Self {
            let previous = DIR_OVERRIDE.with(|slot| slot.borrow_mut().replace(dir));
            Self(previous)
        }
    }

    impl Drop for SecretRevisionDirOverrideGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            DIR_OVERRIDE.with(|slot| *slot.borrow_mut() = previous);
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
pub use dir_override::SecretRevisionDirOverrideGuard;

/// Reads the current stamp, if the revision file exists. Test helper: a
/// changed stamp is what a running process reacts to, so "did this write
/// notify?" is "did this string change?".
#[cfg(any(test, feature = "test-util"))]
pub fn current_revision() -> Option<String> {
    std::fs::read_to_string(revision_file_path()).ok()
}
