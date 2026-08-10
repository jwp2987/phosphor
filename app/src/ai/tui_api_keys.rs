//! Cross-process API-key hot reload: the subscriber half.
//!
//! Ported from Warp's `app/src/ai/tui_api_keys.rs` at the pinned oracle
//! (`02b53fcd8`, Warp `2026.07.29.09.05` stable — see `ORACLE.md`).
//!
//! The mechanism, the revision file and the writer half are documented on
//! [`ai::secret_revision`]; the subscriber half lives here because the
//! filesystem watcher it hangs off ([`WarpManagedPathsWatcher`]) is an `app`
//! singleton. Both of this fork's secret stores subscribe:
//! [`ApiKeyManager`] (the pin's fixed four-provider BYOK store) and
//! [`AgentProviderSecrets`] (this fork's arbitrary-provider BYOP store).
//!
//! # Deviations from the pin, all required by recorded fork decisions
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
//! - **Every write notifies, not just the `zap-tui` key CLI.** The pin's single
//!   call site is `--set-provider-api-key` / `--clear-provider-api-key`, again
//!   because upstream a GUI-side write cannot reach a TUI's keyring. It can
//!   here, so the notification moved into the two stores' write choke points
//!   (`ApiKeyManager::write_keys_to_secure_storage` and
//!   `AgentProviderSecrets::persist`) and now covers the GUI's Settings > AI
//!   editors and the TUI's `/api-keys` picker as well. The explicit call in
//!   `crates/warp_tui/src/session.rs` is kept: it is what turns a failed stamp
//!   into a message on the CLI's stderr instead of a line in the log.
//!
//! - **`AgentProviderSecrets` subscribes too.** It has no counterpart at the
//!   pin — upstream's BYOP keys live in `ApiKeys::custom_endpoints` — but it is
//!   the store this fork's GUI key editor actually writes, so leaving it out
//!   would have fixed the notification for the store neither surface edits.

use ai::api_keys::ApiKeyManager;
use ai::secret_revision;
use warpui::{ModelContext, SingletonEntity as _};

use crate::ai::agent_providers::AgentProviderSecrets;
use crate::warp_managed_paths_watcher::{
    WarpManagedPathsWatcher, WarpManagedPathsWatcherEvent, repository_update_touches_path,
};

/// Signals other running Zap processes to reload their API keys from secure
/// storage. Call this *after* the write to secure storage has completed, so a
/// reader that reacts immediately cannot observe the old value.
///
/// Both stores already stamp the revision from their own write choke points, so
/// this remains only for `zap-tui`'s key CLI, which wants to report a failed
/// stamp to the user rather than only log it.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub fn notify_tui_api_keys_changed() -> anyhow::Result<()> {
    secret_revision::bump()
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
                if repository_update_touches_path(update, &secret_revision::revision_file_path()) {
                    manager.reload_keys_from_secure_storage(ctx);
                }
            },
        );
    }
}

impl TuiApiKeyRefresher for AgentProviderSecrets {
    fn subscribe_to_tui_api_key_changes(&mut self, ctx: &mut ModelContext<Self>) {
        ctx.subscribe_to_model(
            &WarpManagedPathsWatcher::handle(ctx),
            |secrets, event, ctx| {
                let WarpManagedPathsWatcherEvent::FilesChanged(update) = event;
                if repository_update_touches_path(update, &secret_revision::revision_file_path()) {
                    secrets.reload_from_secure_storage(ctx);
                }
            },
        );
    }
}
