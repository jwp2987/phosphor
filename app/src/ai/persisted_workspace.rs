//! `PersistedWorkspace` — the set of repository roots the app knows about.
//!
//! Restored from the pinned oracle `02b53fcd8`
//! (`app/src/ai/persisted_workspace.rs`). It was deleted by `efcaa42b8`, which
//! retired the `lsp` crate and took this model out with it even though most of
//! what lives here is neither LSP nor cloud: it is the "recently used
//! repositories" list that backs the worktree sidecar, the repo pickers, the
//! command palette's repo source and the File ▸ Open Recent menu, plus the
//! project-rules scan that fires when the user adds a repo.
//!
//! # What is not restored, and why
//!
//! Two legs of the pin's version are cut. Neither is cut because it is cloud —
//! there is nothing cloud in either — and neither is cut on principle. Both are
//! cut because the code they call does not exist in this fork, and standing
//! them back up is a larger, separate port.
//!
//! ## The codebase-indexing leg
//!
//! The pin drives `ai::index::full_source_code_embedding::manager::
//! CodebaseIndexManager` from this model. This fork's `crates/ai/src/index/`
//! contains only `file_outline`, `locations.rs` and `mod.rs`; the
//! `full_source_code_embedding` subtree (32 files, ~12.3k lines at the pin) is
//! absent. Every place this model would call into it is marked
//! `INDEXING SEAM` below, with the exact pin API the call needs. Nothing is
//! designed away: the surrounding structure, the persistence events and the
//! metadata types are all present, so restoring indexing is a matter of filling
//! in the marked call sites.
//!
//! ## The LSP leg
//!
//! The pin also stores, per workspace, which language servers the user has
//! enabled, and drives install/spawn/shutdown of those servers. All of it is
//! typed on `lsp::supported_servers::LSPServerType` and `lsp::LspManagerModel`,
//! from the `crates/lsp` crate that `efcaa42b8` deleted outright (24 files). The
//! SQLite table behind it (`workspace_language_server`) was dropped in the same
//! commit. Restoring this leg means restoring that crate first; there is no
//! partial version of it that type-checks, so no stub is invented here. The
//! specific cut points are marked `LSP SEAM`.
//!
//! ## Cloud
//!
//! The pin obtains an HTTP client from `crate::server::server_api::
//! ServerApiProvider` in three places. All three are inside the LSP leg (server
//! download / install / availability probing), so no cloud call survives into
//! this file at all.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use ai::project_context::model::ProjectContextModel;
use ai::workspace::{WorkspaceMetadata, WorkspaceMetadataEvent};
use anyhow::Context;
use chrono::Utc;
use itertools::Itertools;
#[cfg(feature = "local_fs")]
use repo_metadata::RepoMetadataModel;
#[cfg(feature = "local_fs")]
use warp_util::standardized_path::StandardizedPath;
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::persistence::ModelEvent;
use crate::report_if_error;

/// One repository root the app knows about.
///
/// At the pin this also carries `language_servers: HashMap<LSPServerType,
/// EnablementState>`. That field is part of the LSP seam (see module docs) and
/// is not restored; the wrapper struct is kept rather than collapsing this to a
/// bare `WorkspaceMetadata` so that restoring the field is a one-line change.
struct Workspace {
    metadata: WorkspaceMetadata,
}

impl Workspace {
    /// Returns `true` if this workspace has been persisted to SQLite.
    ///
    /// A workspace created solely from available-server detection will have
    /// all metadata timestamps set to `None` and is considered non-persisted.
    ///
    /// LSP SEAM: the pin additionally `debug_assert!`s that a non-persisted
    /// workspace holds only `EnablementState::Suggested` language servers. That
    /// assertion needs the `language_servers` field; it comes back with it.
    fn is_persisted(&self) -> bool {
        self.metadata.navigated_ts.is_some()
            || self.metadata.modified_ts.is_some()
            || self.metadata.queried_ts.is_some()
    }
}

/// Manages a set of code workspaces that the app recognizes. These workspaces define
/// the scope of various repo-based code features like codebase indexing, project rules and LSP.
pub struct PersistedWorkspace {
    workspaces: HashMap<PathBuf, Workspace>,
    model_event_sender: Option<SyncSender<ModelEvent>>,
}

#[derive(Debug, Clone)]
pub enum PersistedWorkspaceEvent {
    /// Emitted when the user explicitly adds a repo via a picker (e.g. the tab-config
    /// params modal's repo dropdown). Subscribers can use this to refresh their list.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    WorkspaceAdded { path: PathBuf },
    //
    // LSP SEAM: the pin also defines `InstallStatusUpdate { server_type,
    // status }`, `InstallationSucceeded`, `InstallationFailed` and
    // `AvailableServersDetected { workspace_path, servers }`. All four carry
    // `lsp::supported_servers::LSPServerType` in their payloads and are emitted
    // only from the LSP install/detect paths, so they come back with the `lsp`
    // crate.
}

impl Entity for PersistedWorkspace {
    type Event = PersistedWorkspaceEvent;
}

impl SingletonEntity for PersistedWorkspace {}

impl PersistedWorkspace {
    /// Test-only constructor: no restored metadata, no persistence channel.
    ///
    /// Kept from the pin. `app/src/workspace/view_test.rs` registers the
    /// singleton with `new(vec![], None, ctx)` (as the pin's harness does), so
    /// this has no in-tree caller today; it stays because it is the documented
    /// way for a future test harness to build one without a writer thread.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            workspaces: HashMap::new(),
            model_event_sender: None,
        }
    }

    /// Builds the model from the rows read out of SQLite at startup.
    ///
    /// The pin's signature is
    /// `new(metadata, workspace_language_servers, model_event_sender, ctx)`.
    /// The second parameter is dropped here — see the LSP seam in the module
    /// docs.
    pub fn new(
        metadata: Vec<WorkspaceMetadata>,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let workspaces: HashMap<PathBuf, Workspace> = metadata
            .into_iter()
            .map(|metadata| (metadata.path.clone(), Workspace { metadata }))
            .collect();

        // INDEXING SEAM (subscriptions). The pin wraps the following four
        // subscriptions in `if FeatureFlag::FullSourceCodeEmbedding.is_enabled()`:
        //
        //   1. `CodebaseIndexManager` →
        //        `CodebaseIndexManagerEvent::IndexMetadataUpdated { root_path, event }`
        //          → `me.handle_index_metadata_event(root_path, *event)`
        //        `CodebaseIndexManagerEvent::RemoveExpiredIndexMetadata { expired_metadata }`
        //          → `me.clean_up_expired_metadata(expired_metadata.clone(), ctx)`
        //      Requires: `ai::index::full_source_code_embedding::manager::{
        //        CodebaseIndexManager, CodebaseIndexManagerEvent}` (absent),
        //      and `warp_features::FeatureFlag::FullSourceCodeEmbedding` (absent).
        //      Both handler methods below are already restored and ready to call.
        //
        //   2. `BlocklistAIHistoryModel` →
        //        `BlocklistAIHistoryEvent::StartedNewConversation { terminal_surface_id, .. }`
        //          → `me.clean_up_deleted_indices(ctx)` (local_fs only)
        //          → `me.trigger_incremental_sync_for_conversation(*terminal_surface_id, ctx)`
        //      Requires: `CodebaseIndexManager::{clean_up_deleted_indices,
        //        trigger_incremental_sync_for_path}` (absent). The model
        //      `BlocklistAIHistoryModel` and its event *do* exist in this fork.
        //
        //   3. `UserWorkspaces` →
        //        `UserWorkspacesEvent::CodebaseContextEnablementChanged`
        //          → `me.on_settings_changed(ctx)`
        //      Requires: that `UserWorkspacesEvent` variant, plus
        //      `UserWorkspaces::is_codebase_context_enabled` — both absent from
        //      this fork's `app/src/workspaces/user_workspaces.rs`.
        //
        //   4. `ProjectContextModel` → `KnownRulesChanged(delta)`
        //          → `ModelEvent::{UpsertProjectRules, DeleteProjectRules}`
        //      This one is NOT a seam and is NOT re-registered here: this fork
        //      already owns it in `crate::ai::project_rules_persister::
        //      ProjectRulesPersister`, unconditionally rather than behind the
        //      indexing feature flag. Re-adding it here would double every
        //      project-rules write. If indexing is restored, leave rule
        //      persistence where it is.

        // `DetectedRepositories` → `DetectedGitRepo` is likewise owned by
        // `ProjectRulesPersister` in this fork (it calls
        // `ProjectContextModel::index_and_store_rules` on repo entry, which is
        // what the pin's `index_repo` does for the rules half). Not re-registered
        // here for the same double-write reason. The pin's version of that
        // subscription additionally called the indexing half of `index_repo`;
        // that part is covered by the INDEXING SEAM inside `index_repo` below.

        // LSP SEAM: the pin ends `new` by kicking off
        // `detect_available_servers_for_workspaces(startup_workspace_paths,
        // true, ctx)` for every restored workspace, so the code footer has fresh
        // available-server state. That whole method is LSP-typed and absent.

        Self {
            workspaces,
            model_event_sender,
        }
    }

    pub fn root_for_workspace<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        path.ancestors()
            .find(|&path| self.workspaces.contains_key(path))
    }

    /// Discovers project rules (WARP.md / AGENTS.md) under `directory_path`.
    ///
    /// INDEXING SEAM. The pin's body is:
    ///
    /// ```ignore
    /// ProjectContextModel::handle(ctx).update(ctx, |model, ctx| {
    ///     let _ = model.index_and_store_rules(
    ///         directory_path.clone(),
    ///         read_project_rule_contents,
    ///         ctx,
    ///     );
    /// });
    /// if FeatureFlag::FullSourceCodeEmbedding.is_enabled()
    ///     && UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx)
    ///     && *CodeSettings::as_ref(ctx).auto_indexing_enabled
    /// {
    ///     CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
    ///         manager.index_directory(directory_path, ctx);
    ///     });
    /// }
    /// ```
    ///
    /// The rules half is kept. The embedding half needs three things this fork
    /// does not have: `FeatureFlag::FullSourceCodeEmbedding`,
    /// `UserWorkspaces::is_codebase_context_enabled(&AppContext) -> bool`, and
    /// `CodeSettings::auto_indexing_enabled` (a `SettingsValue<bool>`), plus
    /// `CodebaseIndexManager::index_directory(PathBuf, &mut ModelContext<..>)`.
    ///
    /// The rules call also differs from the pin by arity: this fork's
    /// `ProjectContextModel::index_and_store_rules` takes `(root_path, ctx)`,
    /// having inlined the pin's `project_rule_content_reader` parameter (the pin
    /// passes `crate::ai::metadata_project_rules::read_project_rule_contents`,
    /// a function that does not exist here).
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    fn index_repo(&self, directory_path: PathBuf, ctx: &mut ModelContext<Self>) {
        ProjectContextModel::handle(ctx).update(ctx, |model, ctx| {
            let _ = model.index_and_store_rules(directory_path, ctx);
        });
    }

    /// Explicitly registers a directory as a workspace, as if the user had navigated there.
    ///
    /// Creates or updates the entry with `navigated_ts = now`, persists to SQLite,
    /// starts full repo-metadata indexing before triggering project-rules scanning,
    /// and emits [`PersistedWorkspaceEvent::WorkspaceAdded`] so subscribers can
    /// refresh their UI.
    pub fn user_added_workspace(&mut self, path: PathBuf, ctx: &mut ModelContext<Self>) {
        let now = Utc::now();

        match self.workspaces.get_mut(&path) {
            Some(workspace) => {
                workspace.metadata.navigated_ts = Some(now);
            }
            None => {
                self.workspaces.insert(
                    path.clone(),
                    Workspace {
                        metadata: WorkspaceMetadata {
                            path: path.clone(),
                            navigated_ts: Some(now),
                            modified_ts: None,
                            queried_ts: None,
                        },
                    },
                );
            }
        }

        self.persist_metadata_for_index(&path);
        #[cfg(feature = "local_fs")]
        match StandardizedPath::from_local_canonicalized(&path) {
            Ok(standardized) => {
                if let Err(error) = RepoMetadataModel::handle(ctx).update(ctx, |model, ctx| {
                    model.index_local_directory_path(&standardized, ctx)
                }) {
                    log::warn!(
                        "Failed to start full repo metadata indexing for {standardized}: {error}"
                    );
                }
            }
            Err(error) => {
                log::warn!(
                    "Failed to canonicalize user-added workspace {} for full repo metadata indexing: {error}",
                    path.display()
                );
            }
        }
        self.index_repo(path.clone(), ctx);
        ctx.emit(PersistedWorkspaceEvent::WorkspaceAdded { path });
    }

    /// All persisted workspaces, most recently touched first, deduplicated by path.
    pub fn workspaces<'a>(&'a self) -> impl Iterator<Item = WorkspaceMetadata> + use<'a> {
        self.workspaces
            .values()
            .filter(|workspace| workspace.is_persisted())
            .map(|workspace| workspace.metadata.clone())
            .sorted_by(WorkspaceMetadata::most_recently_touched)
            .dedup_by(|a, b| a.path == b.path)
    }

    /// Records that the user `cd`-ed into an already-known workspace.
    ///
    /// Deliberately does not create missing entries — the pin behaves the same
    /// way, so merely walking through a directory never adds it to the recent
    /// list. Entries are created by `DetectedRepositories` (via the indexing
    /// seam at the pin) or explicitly by [`Self::user_added_workspace`].
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    pub fn navigated_to_path(&mut self, directory: &PathBuf) {
        if let Some(workspace) = self.workspaces.get_mut(directory) {
            workspace.metadata.navigated_ts = Some(Utc::now());
            self.persist_metadata_for_index(directory);
        }
    }

    /// INDEXING SEAM (handler, fully restored — currently has no caller).
    ///
    /// At the pin this is driven by
    /// `CodebaseIndexManagerEvent::IndexMetadataUpdated { root_path, event }`,
    /// registered in [`Self::new`]. The body is restored verbatim so that
    /// re-subscribing is the only work required; `WorkspaceMetadataEvent` lives
    /// in `ai::workspace` and is restored alongside it.
    #[allow(dead_code)]
    fn handle_index_metadata_event(&mut self, root_path: &PathBuf, event: WorkspaceMetadataEvent) {
        match event {
            WorkspaceMetadataEvent::Queried => {
                if let Some(workspace) = self.workspaces.get_mut(root_path) {
                    workspace.metadata.queried_ts = Some(Utc::now());
                }
                self.persist_metadata_for_index(root_path);
            }
            WorkspaceMetadataEvent::Modified => {
                if let Some(workspace) = self.workspaces.get_mut(root_path) {
                    workspace.metadata.modified_ts = Some(Utc::now());
                }
                self.persist_metadata_for_index(root_path);
            }
            WorkspaceMetadataEvent::Created => {
                let new_metadata = WorkspaceMetadata {
                    path: root_path.clone(),
                    navigated_ts: None,
                    // Count creation as a modification event.
                    modified_ts: Some(Utc::now()),
                    queried_ts: None,
                };

                if let Some(existing) = self.workspaces.get_mut(root_path) {
                    // Preserve existing language server settings when re-creating
                    // workspace metadata (e.g. after an expired index is cleaned up
                    // and the user navigates back to the same directory).
                    existing.metadata = new_metadata;
                } else {
                    self.workspaces.insert(
                        root_path.clone(),
                        Workspace {
                            metadata: new_metadata,
                        },
                    );
                }
                self.persist_metadata_for_index(root_path);
            }
        }
    }

    pub fn workspace_for_path(&self, root_path: &Path) -> Option<WorkspaceMetadata> {
        self.workspaces
            .get(root_path)
            .map(|workspace| workspace.metadata.clone())
    }

    fn persist_metadata_for_index(&self, path: &PathBuf) {
        log::info!("Saving workspace metadata for {path:?} to SQLite");

        if let Some(single_metadata) = self.workspace_for_path(path) {
            self.save_to_db(vec![ModelEvent::UpsertCodebaseIndexMetadata {
                index_metadata: Box::new(single_metadata),
            }]);
        }
    }

    /// INDEXING SEAM (handler, restored — currently has no caller).
    ///
    /// At the pin this is driven by
    /// `CodebaseIndexManagerEvent::RemoveExpiredIndexMetadata { expired_metadata }`,
    /// registered in [`Self::new`]. Expiry is decided by the index manager
    /// (`WorkspaceMetadata::is_expired`, still available in `ai::workspace`), so
    /// with indexing absent nothing ever expires and the recent-repos list only
    /// grows — matching the fork's current behaviour of not having the list at
    /// all, but worth knowing when indexing returns.
    ///
    /// LSP SEAM: the pin's version has a third arm that refuses to delete a
    /// workspace row that still has `Yes`/`No` language-server entries, because
    /// `workspace_language_server` FKs `workspace_metadata` without
    /// `ON DELETE CASCADE` and orphaned child rows silently disappear from the
    /// startup `inner_join`, making enabled servers look disabled. That arm must
    /// come back at the same time as the LSP leg — deleting it and the table
    /// together is safe, restoring one without the other is not.
    #[allow(dead_code)]
    fn clean_up_expired_metadata(
        &self,
        indices_to_remove: Arc<Vec<PathBuf>>,
        _ctx: &mut ModelContext<Self>,
    ) {
        log::info!("Cleaning up index metadata from SQLite");

        let indices_to_remove = indices_to_remove.as_ref();
        self.save_to_db(indices_to_remove.iter().filter_map(|path| {
            let Some(ws) = self.workspaces.get(path) else {
                return Some(ModelEvent::DeleteCodebaseIndexMetadata {
                    repo_path: path.to_path_buf(),
                });
            };

            // Skip non-persisted workspaces — they have no DB row to delete.
            if !ws.is_persisted() {
                return None;
            }

            Some(ModelEvent::DeleteCodebaseIndexMetadata {
                repo_path: path.to_path_buf(),
            })
        }));
    }

    fn save_to_db(&self, events: impl IntoIterator<Item = ModelEvent>) {
        let model_event_sender = self.model_event_sender.clone();
        if let Some(model_event_sender) = &model_event_sender {
            for event in events {
                report_if_error!(
                    model_event_sender
                        .send(event)
                        .with_context(|| "Unable to save codebase index metadata to sqlite")
                );
            }
        }
    }

    // INDEXING SEAM (methods not restored, because their bodies cannot be
    // written against types this fork lacks). Restoring indexing means adding
    // these back from `02b53fcd8:app/src/ai/persisted_workspace.rs`:
    //
    // * `fn on_settings_changed(&mut self, ctx)` and
    //   `pub fn on_user_changed(&self, ctx)` — both just call
    //   `Self::maybe_enable_codebase_indexing(ctx)`. `on_user_changed` is called
    //   from `AuthManager` after the signed-in user changes
    //   (`app/src/auth/auth_manager.rs`, in the branch that updates
    //   `LLMPreferences`); that call site is likewise absent here.
    //
    // * `fn maybe_enable_codebase_indexing(ctx: &mut ModelContext<Self>)` —
    //   reads `CodebaseIndexManager::is_indexing_enabled()` and
    //   `UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx)`, then
    //   either `Self::enable_codebase_indexing(manager, ctx)` or
    //   `manager.reset_codebase_indexing(ctx)`.
    //
    // * `fn enable_codebase_indexing(manager: &mut CodebaseIndexManager,
    //   ctx: &mut ModelContext<CodebaseIndexManager>)` — pushes the account's
    //   limits into the manager via
    //   `AIRequestUsageModel::as_ref(ctx).codebase_context_limits()`
    //   (`max_indices_allowed`, `max_files_per_repo`,
    //   `embedding_generation_batch_size`; `codebase_context_limits` is absent
    //   from this fork's `AIRequestUsageModel`), then, when
    //   `should_auto_index_codebase(CodebaseAutoIndexingSurface::Local, ctx)`,
    //   walks `all_working_directories(ctx)` through
    //   `DetectedRepositories::get_root_for_path` and `auto_index_candidate_roots`
    //   to pick roots for `manager.index_directory(root, ctx)`.
    //   `crate::ai::codebase_auto_indexing` does not exist here.
    //
    // * `fn trigger_incremental_sync_for_conversation(&mut self,
    //   terminal_view_id: warpui::EntityId, ctx)` — finds the `TerminalView`
    //   whose `view_id()` matches, skips it unless
    //   `active_session_is_local(ctx) == Some(true)`, and calls
    //   `CodebaseIndexManager::trigger_incremental_sync_for_path(&PathBuf, ctx)`
    //   with that view's `pwd()`. Every `TerminalView` accessor it uses
    //   (`view_id`, `active_session_is_local`, `pwd`) still exists in this fork;
    //   only the manager is missing.
    //
    // * `fn clean_up_deleted_indices(&self, ctx)` — one call to
    //   `CodebaseIndexManager::clean_up_deleted_indices(ctx)`.
    //
    // * `pub fn all_working_directories(app: &AppContext) -> HashSet<PathBuf>` —
    //   a free function at the bottom of the pin's file. This fork already has a
    //   byte-identical private copy in `app/src/ai/outline/native.rs`, written
    //   when `RepoOutlines` lost its dependency on this module. Restoring
    //   indexing should reunify those two rather than add a third.

    // LSP SEAM (types and methods not restored). From the same pin file:
    // `EnablementState`, `LspTask`, `LSPEnablementResultForFile`,
    // `LspRepoStatus`, `LSPInstallationStatus`, the `lsp_installation_status`
    // field, `enable_lsp_server_for_path`, `disable_lsp_server_for_path`,
    // `has_enabled_lsp_server_for_file_path`, `set_lsp_server_for_path`,
    // `enabled_lsp_servers`, `all_lsp_servers`,
    // `detect_available_servers_for_workspaces`, `total_lsp_server_count`,
    // `handle_install_lsp`, `handle_spawn_lsp`, `execute_lsp_task` and
    // `detect_lsp_workspace_status`. All are typed on `lsp::*`, and their
    // consumers (`app/src/code/footer.rs`, `app/src/code/local_code_editor.rs`,
    // `app/src/settings_view/code_page.rs`,
    // `app/src/code/language_server_shutdown_manager.rs`,
    // `TerminalView::start_lsp_server_in_active_pwd`) were deleted in the same
    // commit. This is a `crates/lsp` port, not a `PersistedWorkspace` change.
}

#[cfg(test)]
#[path = "persisted_workspace_tests.rs"]
mod tests;
