//! Cross-process API-key hot reload.
//!
//! Ported from Warp's `app/src/ai/tui_api_keys.rs` at the pinned oracle
//! (`02b53fcd8`, Warp `2026.07.29.09.05` stable — see `ORACLE.md`).
//!
//! [`ApiKeyManager`] reads secure storage once, when it is constructed. A key
//! can also be written by a *different* process — `zap-tui
//! --set-provider-api-key <provider>` persists a key and immediately exits
//! (`crates/warp_tui/src/session.rs`). Without a signal, every already-running
//! process keeps serving the keys it read at startup, and the key the user
//! just saved looks like it was ignored until the app is restarted.
//!
//! The signal is a revision file: the writer stamps a fresh UUID into it, and
//! every other process notices via [`WarpManagedPathsWatcher`] and re-reads
//! secure storage. The file's *contents* are not the payload — the keys still
//! come from the OS keyring — so nothing secret is written to disk. Writing a
//! new UUID rather than, say, touching an mtime keeps the change visible to a
//! content-hashing watcher as well as a metadata-watching one.
//!
//! # Deviations from the pin, both required by recorded fork decisions
//!
//! - **One config directory.** The pin resolves the revision file against
//!   `warp_core::paths::tui_config_local_dir()`, which does not exist in this
//!   fork and must not be invented: the GUI and TUI deliberately share one app
//!   id, and therefore one `config_local_dir()` and one keyring namespace (see
//!   `DECLINED.md`, "TUI/GUI shared app id"). The shared
//!   [`warp_core::paths::config_local_dir`] is the correct single-directory
//!   formulation — both sides already agree on that one path, which is all the
//!   mechanism needs. This is the same substitution already made for
//!   `crates/warp_tui/src/zero_state_animation_config.rs`.
//!
//!   That directory is already a watch root:
//!   `WarpManagedPathsWatcher::new_internal` registers `config_local_dir()`
//!   recursively with `WatchFilter::accept_all()` when it differs from
//!   `data_dir()`, and registers `data_dir()` recursively (excluding only
//!   `worktrees/`) otherwise — so the revision file is covered on both
//!   branches, on every platform.
//!
//! - **Not gated on the launch mode.** The pin subscribes only when
//!   `LaunchMode::Tui`, because there its GUI has a *separate* app id and
//!   keyring namespace and so cannot be affected by a TUI-side write. Here
//!   both surfaces read and write the same keyring entry, so a running GUI
//!   goes stale for exactly the same reason a running TUI does, and both
//!   subscribe. Sharing the namespace is what makes this hook matter more in
//!   this fork than upstream, so narrowing it to the TUI would drop the case
//!   that motivates it.
//!
//! # Not wired (deliberately): the GUI-side setters
//!
//! Only the `zap-tui --set-provider-api-key` / `--clear-provider-api-key` CLI
//! calls [`notify_tui_api_keys_changed`], matching the pin's single call site.
//! Because this fork shares one keyring, the GUI's own key-editing surfaces
//! could reasonably notify too, so that a running TUI picks up a key saved
//! from Settings. That is a behaviour change with no pin to port from, so it
//! is left for a maintainer call rather than taken unilaterally.

use ai::api_keys::ApiKeyManager;
use anyhow::Context as _;
use uuid::Uuid;
use warpui::{ModelContext, SingletonEntity as _};

use crate::warp_managed_paths_watcher::{
    WarpManagedPathsWatcher, WarpManagedPathsWatcherEvent, repository_update_touches_path,
};

fn revision_file_path() -> std::path::PathBuf {
    warp_core::paths::config_local_dir().join("api_keys.revision")
}

/// Signals other running Zap processes to reload their API keys from secure
/// storage. Call this *after* the write to secure storage has completed, so a
/// reader that reacts immediately cannot observe the old value.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub fn notify_tui_api_keys_changed() -> anyhow::Result<()> {
    let path = revision_file_path();
    let parent = path
        .parent()
        .context("API-key revision path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
    std::fs::write(&path, Uuid::new_v4().to_string())
        .with_context(|| format!("Failed to update API-key revision {}", path.display()))
}

/// Reloads the shared secure-storage namespace when another process changes it.
pub(crate) trait TuiApiKeyRefresher {
    fn subscribe_to_tui_api_key_changes(&mut self, ctx: &mut ModelContext<Self>)
    where
        Self: Sized;
}

impl TuiApiKeyRefresher for ApiKeyManager {
    fn subscribe_to_tui_api_key_changes(&mut self, ctx: &mut ModelContext<Self>) {
        // Note the closure arity: this fork's `ModelContext::subscribe_to_model`
        // takes `|me, event, ctx|`, not the pin's four-argument
        // `|me, _handle, event, ctx|`.
        ctx.subscribe_to_model(
            &WarpManagedPathsWatcher::handle(ctx),
            |manager, event, ctx| {
                let WarpManagedPathsWatcherEvent::FilesChanged(update) = event;
                if repository_update_touches_path(update, &revision_file_path()) {
                    manager.reload_keys_from_secure_storage(ctx);
                }
            },
        );
    }
}
