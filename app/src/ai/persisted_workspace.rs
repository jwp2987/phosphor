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
//! ## The codebase-indexing leg — restored (Delta D2c)
//!
//! The pin drives `ai::index::full_source_code_embedding::manager::
//! CodebaseIndexManager` from this model, and it does again: the subsystem is
//! back under `crates/ai/src/index/full_source_code_embedding/`, and every place
//! marked `INDEXING SEAM` below is wired. Three differences from the pin, each
//! noted at its call site:
//!
//! * The settings subscription listens to `CodeSettings` rather than
//!   `UserWorkspacesEvent::CodebaseContextEnablementChanged`, because that event
//!   announced a server-pushed organization policy that has no local meaning.
//! * `UserWorkspaces::is_codebase_context_enabled` drops the same organization
//!   override, keeping only the pin's own user-setting branch.
//! * `all_working_directories` is called from
//!   `crate::ai::terminal_working_directories`, its one canonical home, not
//!   re-introduced as the pin's free function at the bottom of this file.
//!
//! The remaining cut leg is LSP's driver half, below. It is not cut because it
//! is cloud — there is nothing cloud in it — but because the code it calls does
//! not exist in this fork.
//!
//! ## The LSP leg — the state half is restored, the driver half is not
//!
//! The pin stores, per workspace, which language servers the user has enabled,
//! AND drives install/spawn/detect of those servers. `crates/lsp` is back, so
//! the **state** half is restored here: `EnablementState`,
//! `Workspace::language_servers`, and the query/mutate methods over it
//! (`enable_lsp_server_for_path`, `disable_lsp_server_for_path`,
//! `has_enabled_lsp_server_for_file_path`, `set_lsp_server_for_path`,
//! `enabled_lsp_servers`, `all_lsp_servers`, `total_lsp_server_count`), plus
//! the `clean_up_expired_metadata` guard that depends on it. These persist
//! through `ModelEvent::UpsertWorkspaceLanguageServer` into the
//! `workspace_language_server` table, restored by
//! `2026-08-10-000000_restore_workspace_language_server`.
//!
//! The **driver** half is now restored too: `LspTask`, `LspRepoStatus`,
//! `LSPInstallationStatus`, the `lsp_installation_status` field,
//! `execute_lsp_task`, `handle_spawn_lsp`, `handle_install_lsp`,
//! `detect_lsp_workspace_status`, `detect_available_servers_for_workspaces`
//! and the four install/detect `PersistedWorkspaceEvent` variants. The
//! interactive-shell PATH capture they depend on
//! (`LocalShellState::get_interactive_path_env_var`) was never removed — only
//! the LSP-specific entry point that called it was — so nothing new had to be
//! built for this.
//!
//! ## Cloud
//!
//! The pin obtains an HTTP client from `crate::server::server_api::
//! ServerApiProvider` in four places, all inside the driver half. None is
//! restored: each is `Arc::new(http_client::Client::new())` here, the fork's
//! standard non-cloud client (as used by `autoupdate`, `changelog_model` and
//! `settings_view::network_page`). The client only fetches GitHub release
//! metadata and downloads server binaries, so no cloud backend is involved, and
//! no `crate::server::` import survives into this file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use ai::index::full_source_code_embedding::manager::{
    CodebaseIndexManager, CodebaseIndexManagerEvent,
};
use ai::project_context::model::ProjectContextModel;
use ai::workspace::{WorkspaceMetadata, WorkspaceMetadataEvent};
use anyhow::Context;
use chrono::Utc;
use itertools::Itertools;
use lsp::LanguageId;
#[cfg(feature = "local_fs")]
use lsp::LspEvent;
use lsp::supported_servers::LSPServerType;
#[cfg(feature = "local_fs")]
use lsp::{LspManagerModel, LspServerConfig};
#[cfg(feature = "local_fs")]
use repo_metadata::RepoMetadataModel;
#[cfg(feature = "local_fs")]
use repo_metadata::repositories::DetectedRepositories;
use serde::{Deserialize, Serialize};
use settings::Setting;
#[cfg(feature = "local_fs")]
use warp_core::channel::ChannelState;
use warp_core::features::FeatureFlag;
#[cfg(feature = "local_fs")]
use warp_util::standardized_path::StandardizedPath;
#[cfg(feature = "local_fs")]
use warpui::windowing::WindowManager;
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::ai::blocklist::{BlocklistAIHistoryEvent, BlocklistAIHistoryModel};
#[cfg(feature = "local_fs")]
use crate::ai::codebase_auto_indexing::{
    CodebaseAutoIndexingSurface, auto_index_candidate_roots, should_auto_index_codebase,
};
use crate::ai::request_usage_model::AIRequestUsageModel;
#[cfg(feature = "local_fs")]
use crate::ai::terminal_working_directories::all_working_directories;
#[cfg(feature = "local_fs")]
use crate::code::language_server_shutdown_manager::LanguageServerShutdownManager;
#[cfg(feature = "local_fs")]
use crate::code::lsp_telemetry::LspTelemetryEvent;
use crate::persistence::ModelEvent;
use crate::report_if_error;
#[cfg(feature = "local_fs")]
use crate::send_telemetry_from_ctx;
use crate::settings::CodeSettings;
#[cfg(feature = "local_fs")]
use crate::terminal::local_shell::LocalShellState;
use crate::terminal::view::TerminalView;
use crate::workspaces::user_workspaces::UserWorkspaces;
#[cfg(feature = "local_fs")]
use crate::{view_components::DismissibleToast, workspace::ToastStack};

/// Whether the user has enabled a given language server for a given workspace.
///
/// This is also used in underlying sqlite type persistence. We should be careful
/// not to rename an existing variant, as it will break persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnablementState {
    Yes,
    No,
    /// Server was detected as available for a repo but not yet explicitly
    /// enabled/disabled by the user. Entries with this state live only in
    /// memory and are never persisted to SQLite.
    Suggested,
}

/// Describes an LSP operation to be executed after capturing the interactive shell PATH.
#[cfg(feature = "local_fs")]
pub enum LspTask {
    /// Install and enable an LSP server for a file path.
    Install {
        file_path: PathBuf,
        repo_root: PathBuf,
        server_type: LSPServerType,
    },
    /// Spawn LSP servers for a file path.
    Spawn { file_path: PathBuf },
}

/// The result of asking whether any enabled language server covers a file.
pub enum LSPEnablementResultForFile {
    Enabled,
    UnsupportedLanguage,
    LSPNotEnabled { root_name: Option<String> },
}

/// Tracks whether an LSP server is relevant/installed/enabled for a repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LspRepoStatus {
    /// LSP is enabled and running (view will set this when subscribed to a live server).
    Ready,
    /// LSP is enabled (we don't block on installation checks when enabled).
    Enabled,
    /// We are checking installation status (only for disabled case).
    CheckingForInstallation,
    /// LSP is disabled and globally installed.
    DisabledAndInstalled { server_type: LSPServerType },
    /// LSP is disabled and not installed.
    DisabledAndNotInstalled { server_type: LSPServerType },
    /// LSP is currently being installed.
    Installing { server_type: LSPServerType },
}

/// Global installation status for an LSP server (across all projects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LSPInstallationStatus {
    Installed,
    NotInstalled,
    Checking,
    Installing,
}

impl LspRepoStatus {
    /// Converts an [`LSPInstallationStatus`] (global, per-server-type) into an
    /// [`LspRepoStatus`] (per-repo view of enablement/installation).
    pub fn from_installation_status(
        status: &LSPInstallationStatus,
        server_type: LSPServerType,
    ) -> Self {
        match status {
            LSPInstallationStatus::Installed => Self::DisabledAndInstalled { server_type },
            LSPInstallationStatus::NotInstalled => Self::DisabledAndNotInstalled { server_type },
            LSPInstallationStatus::Checking => Self::CheckingForInstallation,
            LSPInstallationStatus::Installing => Self::Installing { server_type },
        }
    }
}

/// One repository root the app knows about.
struct Workspace {
    metadata: WorkspaceMetadata,
    /// Which language servers the user has enabled for this root.
    ///
    /// `Yes`/`No` entries are persisted to `workspace_language_server`;
    /// `Suggested` entries are in-memory only (see [`EnablementState`]).
    language_servers: HashMap<LSPServerType, EnablementState>,
}

impl Workspace {
    /// Returns `true` if this workspace has been persisted to SQLite.
    ///
    /// A workspace created solely from available-server detection will have
    /// all metadata timestamps set to `None` and is considered non-persisted.
    fn is_persisted(&self) -> bool {
        let persisted = self.metadata.navigated_ts.is_some()
            || self.metadata.modified_ts.is_some()
            || self.metadata.queried_ts.is_some();

        if !persisted {
            debug_assert!(
                self.language_servers
                    .values()
                    .all(|s| *s == EnablementState::Suggested),
                "non-persisted workspace has Yes/No server state; persist metadata first"
            );
        }

        persisted
    }
}

/// Manages a set of code workspaces that the app recognizes. These workspaces define
/// the scope of various repo-based code features like codebase indexing, project rules and LSP.
pub struct PersistedWorkspace {
    workspaces: HashMap<PathBuf, Workspace>,
    model_event_sender: Option<SyncSender<ModelEvent>>,
    /// Global installation status per LSP server type.
    #[cfg(feature = "local_fs")]
    lsp_installation_status: HashMap<LSPServerType, LSPInstallationStatus>,
}

#[derive(Debug, Clone)]
pub enum PersistedWorkspaceEvent {
    /// Emitted when the user explicitly adds a repo via a picker (e.g. the tab-config
    /// params modal's repo dropdown). Subscribers can use this to refresh their list.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    WorkspaceAdded { path: PathBuf },
    /// Emitted when LSP installation status changes.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    InstallStatusUpdate {
        server_type: LSPServerType,
        status: LSPInstallationStatus,
    },
    /// Emitted when LSP installation completes successfully.
    /// Toast notification is shown directly by PersistedWorkspace.
    /// The server is also spawned automatically by PersistedWorkspace.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    InstallationSucceeded,
    /// Emitted when LSP installation fails.
    /// Toast notification is shown directly by PersistedWorkspace.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    InstallationFailed,
    /// Emitted when async detection of available servers for a workspace completes.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    AvailableServersDetected {
        workspace_path: PathBuf,
        servers: Vec<LSPServerType>,
    },
}

impl Entity for PersistedWorkspace {
    type Event = PersistedWorkspaceEvent;
}

impl SingletonEntity for PersistedWorkspace {}

impl PersistedWorkspace {
    /// Test-only constructor: no restored metadata, no persistence channel.
    ///
    /// Kept from the pin. `app/src/workspace/view_test.rs` registers the
    /// singleton with `new(vec![], Default::default(), None, ctx)` (as the
    /// pin's harness does), so this has no in-tree caller today; it stays
    /// because it is the documented way for a future test harness to build one
    /// without a writer thread.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            workspaces: HashMap::new(),
            model_event_sender: None,
            #[cfg(feature = "local_fs")]
            lsp_installation_status: HashMap::new(),
        }
    }

    /// Builds the model from the rows read out of SQLite at startup.
    pub fn new(
        metadata: Vec<WorkspaceMetadata>,
        workspace_language_servers: HashMap<PathBuf, HashMap<LSPServerType, EnablementState>>,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let workspaces: HashMap<PathBuf, Workspace> = metadata
            .into_iter()
            .map(|metadata| {
                let path = metadata.path.clone();
                let language_servers = workspace_language_servers
                    .get(&path)
                    .cloned()
                    .unwrap_or_default();

                (
                    path,
                    Workspace {
                        metadata,
                        language_servers,
                    },
                )
            })
            .collect();

        // INDEXING SEAM — wired (D2c). Three of the pin's four subscriptions are
        // registered below, behind the pin's own
        // `FeatureFlag::FullSourceCodeEmbedding` gate.
        //
        // The pin's fourth — `ProjectContextModel` → `KnownRulesChanged(delta)`
        // → `ModelEvent::{UpsertProjectRules, DeleteProjectRules}` — is
        // deliberately NOT registered here. This fork already owns it in
        // `crate::ai::project_rules_persister::ProjectRulesPersister`,
        // unconditionally rather than behind the indexing flag, so re-adding it
        // would double every project-rules write.
        if FeatureFlag::FullSourceCodeEmbedding.is_enabled() {
            ctx.subscribe_to_model(
                &CodebaseIndexManager::handle(ctx),
                |me, event, ctx| match event {
                    CodebaseIndexManagerEvent::IndexMetadataUpdated { root_path, event } => {
                        me.handle_index_metadata_event(root_path, *event);
                    }
                    CodebaseIndexManagerEvent::RemoveExpiredIndexMetadata { expired_metadata } => {
                        me.clean_up_expired_metadata(expired_metadata.clone(), ctx);
                    }
                    _ => {}
                },
            );

            // Bring the index up to date when a conversation starts.
            ctx.subscribe_to_model(&BlocklistAIHistoryModel::handle(ctx), |me, event, ctx| {
                if let BlocklistAIHistoryEvent::StartedNewConversation {
                    terminal_surface_id,
                    ..
                } = event
                {
                    #[cfg(feature = "local_fs")]
                    me.clean_up_deleted_indices(ctx);

                    me.trigger_incremental_sync_for_conversation(*terminal_surface_id, ctx);
                }
            });

            // React to the codebase-context setting being turned on or off.
            //
            // The pin subscribes to `UserWorkspacesEvent::CodebaseContextEnablementChanged`
            // instead. That event announced an organization-level policy pushed
            // from Warp's server; with no server there is no such policy and no
            // such event, so the local trigger is the settings group that now
            // owns the flag. `on_settings_changed` re-reads the setting rather
            // than trusting the event payload, so a change from any source is
            // handled identically.
            ctx.subscribe_to_model(&CodeSettings::handle(ctx), |me, _event, ctx| {
                me.on_settings_changed(ctx);
            });
        }

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
            #[cfg(feature = "local_fs")]
            lsp_installation_status: HashMap::new(),
        }
    }

    pub fn root_for_workspace<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        path.ancestors()
            .find(|&path| self.workspaces.contains_key(path))
    }

    /// Given a repo path, enables the specified LSP server. If the workspace doesn't exist, it will be created.
    pub fn enable_lsp_server_for_path(&mut self, path: &Path, server_type: LSPServerType) {
        self.set_lsp_server_for_path(path, server_type, EnablementState::Yes);
    }

    /// Given a repo path, disables the specified LSP server.
    pub fn disable_lsp_server_for_path(&mut self, path: &Path, server_type: LSPServerType) {
        self.set_lsp_server_for_path(path, server_type, EnablementState::No);
    }

    /// Returns the enabled LSP server type (if any) for this file path.
    pub fn has_enabled_lsp_server_for_file_path(&self, path: &Path) -> LSPEnablementResultForFile {
        let Some(language_id) = LanguageId::from_path(path) else {
            return LSPEnablementResultForFile::UnsupportedLanguage;
        };
        let Some(root) = self.root_for_workspace(path) else {
            return LSPEnablementResultForFile::LSPNotEnabled { root_name: None };
        };
        let Some(workspace) = self.workspaces.get(root) else {
            return LSPEnablementResultForFile::LSPNotEnabled {
                root_name: root
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string()),
            };
        };

        for (language_server, enablement) in &workspace.language_servers {
            if *enablement == EnablementState::Yes
                && language_server.languages().contains(&language_id)
            {
                return LSPEnablementResultForFile::Enabled;
            }
        }

        LSPEnablementResultForFile::LSPNotEnabled {
            root_name: root
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string()),
        }
    }

    /// Internal method to set LSP server state for a path.
    fn set_lsp_server_for_path(
        &mut self,
        path: &Path,
        server_type: LSPServerType,
        state: EnablementState,
    ) {
        // Check if the workspace needs to be persisted before we take a
        // mutable borrow, so we can call save_to_db without conflicting borrows.
        let needs_persist = self
            .workspaces
            .get(path)
            .is_some_and(|ws| !ws.is_persisted());

        if needs_persist {
            // Materialize the workspace: set a timestamp and persist metadata
            // so the FK-dependent workspace_language_server row can be written.
            let workspace = self.workspaces.get_mut(path).unwrap();
            workspace.metadata.modified_ts = Some(Utc::now());
            let metadata = workspace.metadata.clone();
            self.save_to_db(vec![ModelEvent::UpsertCodebaseIndexMetadata {
                index_metadata: Box::new(metadata),
            }]);
        }

        match self.workspaces.get_mut(path) {
            Some(workspace) => {
                workspace.language_servers.insert(server_type, state);
            }
            None => {
                let metadata = WorkspaceMetadata {
                    path: path.to_path_buf(),
                    navigated_ts: None,
                    // Consider creation as a modification event.
                    modified_ts: Some(Utc::now()),
                    queried_ts: None,
                };

                self.save_to_db(vec![ModelEvent::UpsertCodebaseIndexMetadata {
                    index_metadata: Box::new(metadata.clone()),
                }]);

                self.workspaces.insert(
                    path.to_path_buf(),
                    Workspace {
                        metadata,
                        language_servers: HashMap::from([(server_type, state)]),
                    },
                );
            }
        }

        // Persist the language server setting to database
        self.save_to_db(vec![ModelEvent::UpsertWorkspaceLanguageServer {
            workspace_path: path.to_path_buf(),
            lsp_type: server_type,
            enabled: state,
        }]);
    }

    /// Returns the enabled lsp servers for a given repo path.
    pub fn enabled_lsp_servers(
        &self,
        path: &Path,
    ) -> Option<impl Iterator<Item = LSPServerType> + use<'_>> {
        let root = self.root_for_workspace(path)?;

        self.workspaces.get(root).map(|workspace| {
            workspace
                .language_servers
                .iter()
                .filter_map(|(server_type, state)| {
                    if *state == EnablementState::Yes {
                        Some(*server_type)
                    } else {
                        None
                    }
                })
        })
    }

    /// Returns LSP servers for a given workspace path.
    ///
    /// When `include_suggested` is `false`, only persisted entries (`Yes`/`No`)
    /// are returned.  When `true`, in-memory `Suggested` entries are included as
    /// well (useful for showing available-for-download servers in the UI).
    pub fn all_lsp_servers(
        &self,
        path: &Path,
        include_suggested: bool,
    ) -> Option<impl Iterator<Item = (LSPServerType, EnablementState)> + use<'_>> {
        let root = self.root_for_workspace(path)?;

        self.workspaces.get(root).map(move |workspace| {
            workspace
                .language_servers
                .iter()
                .filter(move |(_, state)| {
                    include_suggested || **state != EnablementState::Suggested
                })
                .map(|(server_type, state)| (*server_type, *state))
        })
    }

    /// Discovers project rules (WARP.md / AGENTS.md) under `directory_path`, and
    /// starts codebase indexing for it.
    ///
    /// INDEXING SEAM — wired (D2c). Both halves are the pin's. The rules call
    /// differs from the pin by arity: this fork's
    /// `ProjectContextModel::index_and_store_rules` takes `(root_path, ctx)`,
    /// having inlined the pin's `project_rule_content_reader` parameter (the pin
    /// passes `crate::ai::metadata_project_rules::read_project_rule_contents`,
    /// a function that does not exist here).
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    fn index_repo(&self, directory_path: PathBuf, ctx: &mut ModelContext<Self>) {
        ProjectContextModel::handle(ctx).update(ctx, |model, ctx| {
            let _ = model.index_and_store_rules(directory_path.clone(), ctx);
        });

        if FeatureFlag::FullSourceCodeEmbedding.is_enabled()
            && UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx)
            && *CodeSettings::as_ref(ctx).auto_indexing_enabled.value()
        {
            CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
                manager.index_directory(directory_path, ctx);
            });
        }
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
                        language_servers: HashMap::new(),
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
                            language_servers: HashMap::new(),
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

            // Don't delete workspace metadata rows for workspaces that have
            // persisted LSP server settings (Yes/No).
            //
            // Deleting workspace_metadata rows would orphan corresponding
            // workspace_language_server rows (FK'd without ON DELETE CASCADE).
            // On next app load, the inner_join used to load workspace language
            // servers will silently drop orphaned rows, making enabled
            // language servers appear disabled.
            //
            // This fork's FK does declare ON DELETE CASCADE
            // (`2026-08-10-000000_restore_workspace_language_server`), so the
            // orphan state is unrepresentable here. That is a different fix,
            // not a replacement: CASCADE would delete the user's per-workspace
            // LSP choice along with the workspace, whereas this arm preserves
            // the choice by keeping the workspace row alive. Both are needed.
            let has_persisted_servers = ws
                .language_servers
                .values()
                .any(|s| *s != EnablementState::Suggested);
            if has_persisted_servers {
                return None;
            }

            Some(ModelEvent::DeleteCodebaseIndexMetadata {
                repo_path: path.to_path_buf(),
            })
        }));
    }

    /// Returns the total count of LSP servers across all workspaces.
    ///
    /// When `include_suggested` is `false`, only persisted entries (`Yes`/`No`)
    /// are counted.  When `true`, in-memory `Suggested` entries are counted too.
    pub fn total_lsp_server_count(&self, include_suggested: bool) -> usize {
        self.workspaces
            .values()
            .map(|workspace| {
                workspace
                    .language_servers
                    .values()
                    .filter(|state| include_suggested || **state != EnablementState::Suggested)
                    .count()
            })
            .sum()
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

    // INDEXING SEAM — the methods below were the "not restored" list; they are
    // restored now (D2c). One thing on that list is deliberately NOT restored:
    // the pin's `pub fn all_working_directories(app) -> HashSet<PathBuf>`, a
    // free function at the bottom of the pin's file. It lives in
    // `crate::ai::terminal_working_directories` in this fork, which is its one
    // canonical home; `enable_codebase_indexing` below calls it there. Do not
    // add a second copy — see that module's docs.

    /// Re-evaluates indexing after the codebase-context setting changes.
    ///
    /// At the pin this is driven by `UserWorkspacesEvent::CodebaseContextEnablementChanged`.
    /// That event does not exist in this fork, because the setting it announced
    /// was an organization-level policy pushed from the server. The local
    /// equivalent is a `CodeSettings` change; see the subscription in
    /// [`Self::new`].
    fn on_settings_changed(&mut self, ctx: &mut ModelContext<Self>) {
        Self::maybe_enable_codebase_indexing(ctx);
    }

    /// Re-evaluates indexing after the signed-in user changes.
    ///
    /// Restored from the pin, and deliberately left without a caller here: the
    /// pin calls it from `AuthManager` in the branch that updates
    /// `LLMPreferences`, and this fork has no signed-in user for that branch to
    /// react to. It is kept because it is one line, and because a future
    /// account-shaped concept (a BYOP profile switch, say) has an obvious place
    /// to hook.
    #[allow(dead_code)]
    pub fn on_user_changed(&self, ctx: &mut ModelContext<Self>) {
        Self::maybe_enable_codebase_indexing(ctx);
    }

    /// Enables or disables codebase indexing according to the setting.
    ///
    /// Restored verbatim from the pin.
    fn maybe_enable_codebase_indexing(ctx: &mut ModelContext<Self>) {
        CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
            if !manager.is_indexing_enabled() {
                return;
            }
            let codebase_context_enabled =
                UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx);
            if codebase_context_enabled {
                Self::enable_codebase_indexing(manager, ctx);
            } else {
                manager.reset_codebase_indexing(ctx);
            }
        });
    }

    /// Pushes the current limits into the manager and, if auto-indexing is on,
    /// queues every open repository root.
    ///
    /// Restored from the pin. `all_working_directories` comes from
    /// `crate::ai::terminal_working_directories`, not from a local copy — see
    /// the seam note above.
    fn enable_codebase_indexing(
        manager: &mut CodebaseIndexManager,
        ctx: &mut ModelContext<CodebaseIndexManager>,
    ) {
        let request_model = AIRequestUsageModel::handle(ctx);
        let codebase_limits = request_model.as_ref(ctx).codebase_context_limits();
        manager.update_max_limits(
            codebase_limits.max_indices_allowed,
            codebase_limits.max_files_per_repo,
            codebase_limits.embedding_generation_batch_size,
            ctx,
        );

        // Fork drift: the pin's `DetectedRepositories::get_root_for_path` takes
        // and returns a `LocalOrRemotePath`, so it needed unwrapping back to a
        // local path. This fork's takes `&Path` and returns `Option<PathBuf>`
        // directly (`crates/repo_metadata/src/repositories.rs:174`), which is
        // the same thing with the remote arm — irrelevant here, since this
        // surface is `Local` — already resolved away.
        #[cfg(feature = "local_fs")]
        if should_auto_index_codebase(CodebaseAutoIndexingSurface::Local, ctx) {
            let roots = all_working_directories(ctx)
                .into_iter()
                .filter_map(|dir| DetectedRepositories::as_ref(ctx).get_root_for_path(&dir));
            for root in auto_index_candidate_roots(roots, |_| true) {
                manager.index_directory(root, ctx);
            }
        }
    }

    /// Brings the index up to date before a new conversation starts.
    ///
    /// Restored verbatim from the pin.
    fn trigger_incremental_sync_for_conversation(
        &mut self,
        terminal_view_id: warpui::EntityId,
        ctx: &mut ModelContext<Self>,
    ) {
        if !UserWorkspaces::as_ref(ctx).is_codebase_context_enabled(ctx) {
            return;
        }

        // Collect window IDs first to avoid borrowing conflicts.
        let window_ids: Vec<_> = ctx.window_ids().collect();

        for window_id in window_ids {
            let terminal_views = ctx.views_of_type::<TerminalView>(window_id);

            for terminal_view in terminal_views.into_iter().flatten() {
                let terminal_view_ref = terminal_view.as_ref(ctx);
                if terminal_view_ref.view_id() == terminal_view_id {
                    if terminal_view_ref.active_session_is_local(ctx) != Some(true) {
                        log::info!(
                            "Skipping local codebase incremental sync for non-local agent conversation"
                        );
                        return;
                    }

                    let pwd = terminal_view_ref.pwd();
                    if let Some(pwd) = pwd {
                        let directory_path = PathBuf::from(pwd);

                        CodebaseIndexManager::handle(ctx).update(ctx, |codebase_manager, ctx| {
                            if let Err(e) = codebase_manager
                                .trigger_incremental_sync_for_path(&directory_path, ctx)
                            {
                                log::warn!("Failed to trigger incremental sync {e}");
                            }
                        });
                    }
                    return; // Found the terminal view, exit both loops
                }
            }
        }
    }

    /// Drops indices whose repository no longer exists on disk.
    ///
    /// Restored verbatim from the pin.
    #[cfg(feature = "local_fs")]
    fn clean_up_deleted_indices(&self, ctx: &mut ModelContext<Self>) {
        CodebaseIndexManager::handle(ctx).update(ctx, |codebase_manager, ctx| {
            codebase_manager.clean_up_deleted_indices(ctx);
        });
    }

    // ---- LSP driver half ----
    //
    // CLOUD SEAM: the pin obtains the HTTP client for every one of these from
    // `crate::server::server_api::ServerApiProvider::as_ref(ctx).get_http_client()`
    // (four call sites). That is not restored. Each becomes
    // `Arc::new(http_client::Client::new())`, this fork's standard non-cloud
    // client — the same construction `autoupdate`, `changelog_model` and
    // `settings_view::network_page` already use. The client is only ever used to
    // fetch GitHub release metadata and download server binaries, so nothing
    // about the cloud backend is involved; reintroducing `crate::server::` here
    // would also trip `script/check_cloud_boundary`.

    #[cfg(feature = "local_fs")]
    fn handle_install_lsp(
        &mut self,
        file_path: PathBuf,
        repo_root: PathBuf,
        server_type: LSPServerType,
        path_env_var: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Early return if already installing to prevent duplicate installations from repeated clicks
        if self.lsp_installation_status.get(&server_type)
            == Some(&LSPInstallationStatus::Installing)
        {
            return;
        }

        // Set Installing state before spawning async installation
        self.lsp_installation_status
            .insert(server_type, LSPInstallationStatus::Installing);
        ctx.emit(PersistedWorkspaceEvent::InstallStatusUpdate {
            server_type,
            status: LSPInstallationStatus::Installing,
        });

        let repo_root_clone = repo_root.clone();
        let file_path_clone = file_path.clone();
        let executor = lsp::CommandBuilder::new(path_env_var);
        let http_client = Arc::new(http_client::Client::new());
        ctx.spawn(
            async move {
                let candidate = server_type.candidate(http_client);
                let metadata = candidate.fetch_latest_server_metadata().await?;
                candidate.install(metadata, &executor).await?;
                Ok::<_, anyhow::Error>(())
            },
            move |me, result, ctx| match result {
                Ok(()) => {
                    // Enable the LSP server
                    me.enable_lsp_server_for_path(&repo_root_clone, server_type);

                    // Update installation status cache
                    me.lsp_installation_status
                        .insert(server_type, LSPInstallationStatus::Installed);

                    // Show success toast
                    if let Some(window_id) = WindowManager::as_ref(ctx).active_window() {
                        ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                            toast_stack.add_ephemeral_toast(
                                DismissibleToast::success(format!(
                                    "{} installed and enabled successfully.",
                                    server_type.binary_name()
                                )),
                                window_id,
                                ctx,
                            );
                        });
                    }

                    ctx.emit(PersistedWorkspaceEvent::InstallationSucceeded);

                    // Also emit status update so listeners can update their UI
                    ctx.emit(PersistedWorkspaceEvent::InstallStatusUpdate {
                        server_type,
                        status: LSPInstallationStatus::Installed,
                    });

                    // Spawn the server now that it's installed and enabled.
                    // This is done here so it happens exactly once, rather
                    // than relying on each subscriber to spawn independently.
                    me.execute_lsp_task(
                        LspTask::Spawn {
                            file_path: file_path_clone,
                        },
                        ctx,
                    );
                }
                Err(e) => {
                    log::info!("Failed to install LSP server: {e}");

                    // Update installation status to NotInstalled
                    me.lsp_installation_status
                        .insert(server_type, LSPInstallationStatus::NotInstalled);

                    // Show error toast
                    if let Some(window_id) = WindowManager::as_ref(ctx).active_window() {
                        ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                            toast_stack.add_ephemeral_toast(
                                DismissibleToast::error(format!(
                                    "Failed to install {}: {}",
                                    server_type.binary_name(),
                                    e
                                )),
                                window_id,
                                ctx,
                            );
                        });
                    }

                    ctx.emit(PersistedWorkspaceEvent::InstallationFailed);

                    // Also emit status update so listeners can update their UI
                    ctx.emit(PersistedWorkspaceEvent::InstallStatusUpdate {
                        server_type,
                        status: LSPInstallationStatus::NotInstalled,
                    });
                }
            },
        );
    }

    /// Starts all enabled LSP servers for the given file path.
    /// This looks up the workspace root and starts any servers that are enabled but not yet running.
    #[cfg(feature = "local_fs")]
    fn handle_spawn_lsp(
        &self,
        file_path: &Path,
        path_env_var: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(workspace_root) = self.root_for_workspace(file_path) else {
            return;
        };

        let Some(servers) = self.enabled_lsp_servers(workspace_root) else {
            return;
        };

        let supported_servers = servers.collect::<Vec<LSPServerType>>();

        if supported_servers.is_empty() {
            return;
        }

        let mut new_servers_available_to_start = false;
        let workspace_root = workspace_root.to_path_buf();

        for server in supported_servers {
            if LspManagerModel::as_ref(ctx).server_registered_and_started(
                &workspace_root,
                server,
                ctx,
            ) {
                continue;
            }

            log::info!(
                "Starting {} LSP server for {}",
                server.binary_name(),
                workspace_root.display()
            );
            let log_relative_path =
                crate::code::lsp_logs::relative_log_path(server, &workspace_root);
            let http_client = Arc::new(http_client::Client::new());
            let config = LspServerConfig::new(
                server,
                workspace_root.clone(),
                path_env_var.clone(),
                ChannelState::app_id().application_name().to_string(),
                http_client,
            )
            .with_log_relative_path(log_relative_path);

            LspManagerModel::handle(ctx).update(ctx, |manager, m_ctx| {
                manager.register(workspace_root.clone(), config, m_ctx);
            });
            new_servers_available_to_start = true;
        }

        if !new_servers_available_to_start {
            return;
        }

        let lsp_manager_handle = LspManagerModel::handle(ctx);
        lsp_manager_handle.update(ctx, |manager, m_ctx| {
            manager.start_all(workspace_root.clone(), m_ctx);
        });

        // Subscribe to LSP server events to show error toast on failure.
        let workspace_root_display = workspace_root.display().to_string();
        let servers = lsp_manager_handle
            .as_ref(ctx)
            .servers_for_workspace(&workspace_root)
            .cloned()
            .unwrap_or_default();

        for server in servers {
            let workspace_root_display = workspace_root_display.clone();
            let server_type_name = server.as_ref(ctx).server_name();
            ctx.subscribe_to_model(&server, move |_me, event, ctx| match event {
                LspEvent::Started => {
                    send_telemetry_from_ctx!(
                        LspTelemetryEvent::ServerStarted {
                            server_type: server_type_name.clone(),
                        },
                        ctx
                    );
                }
                LspEvent::Failed(e) => {
                    send_telemetry_from_ctx!(
                        LspTelemetryEvent::ServerFailed {
                            server_type: server_type_name.clone(),
                        },
                        ctx
                    );
                    if let Some(window_id) = WindowManager::as_ref(ctx).active_window() {
                        ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                            let toast = DismissibleToast::error(format!(
                                "Failed to start LSP server for {workspace_root_display} with error {e}",
                            ));
                            toast_stack.add_ephemeral_toast(toast, window_id, ctx);
                        });
                    }
                }
                _ => {}
            });
        }

        // Once we start a LSP server, also start the garbage collection process if it is not active.
        LanguageServerShutdownManager::handle(ctx).update(ctx, |shutdown_manager, ctx| {
            if !shutdown_manager.has_in_progress_scan() {
                shutdown_manager.schedule_next_scan(ctx);
            }
        });
    }

    /// Executes an LSP task after capturing the interactive shell PATH.
    /// This is the main entry point for LSP operations that need the full PATH.
    #[cfg(feature = "local_fs")]
    pub fn execute_lsp_task(&mut self, task: LspTask, ctx: &mut ModelContext<Self>) {
        // For Spawn tasks, check synchronously whether there are any enabled LSP
        // servers for this workspace before kicking off the expensive interactive
        // shell PATH capture.
        if let LspTask::Spawn { ref file_path } = task {
            let has_servers = self
                .root_for_workspace(file_path)
                .and_then(|root| self.enabled_lsp_servers(root))
                .is_some_and(|mut servers| servers.next().is_some());
            if !has_servers {
                return;
            }
        }

        // Get a future for the interactive PATH
        let path_future = LocalShellState::handle(ctx).update(ctx, |shell_state, ctx| {
            shell_state.get_interactive_path_env_var(ctx)
        });

        ctx.spawn(path_future, move |me, path_env_var, ctx| match task {
            LspTask::Install {
                file_path,
                repo_root,
                server_type,
            } => {
                me.handle_install_lsp(file_path, repo_root, server_type, path_env_var, ctx);
            }
            LspTask::Spawn { file_path } => {
                me.handle_spawn_lsp(&file_path, path_env_var, ctx);
            }
        });
    }

    /// Asynchronously detects which LSP server types are relevant for the given workspaces
    /// by calling `should_suggest_for_repo` on each `LSPServerType`. Results are stored
    /// as `Suggested` entries in the workspaces map and emitted via `AvailableServersDetected`.
    ///
    /// Workspaces that already have language server entries are skipped (results emitted
    /// immediately) unless `skip_cached` is true, in which case all workspaces are scanned
    /// unconditionally. The workspaces to scan share a single background task and one
    /// interactive PATH capture.
    #[cfg(feature = "local_fs")]
    pub fn detect_available_servers_for_workspaces(
        &mut self,
        workspace_paths: Vec<PathBuf>,
        skip_cached: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        // Workspaces that already have entries get an immediate emit; the rest need scanning.
        // When skip_cached is true (initial startup), always scan to pick up new server types.
        let mut paths_to_scan = Vec::new();
        for workspace_path in workspace_paths {
            if !skip_cached
                && let Some(workspace) = self.workspaces.get(&workspace_path)
                && !workspace.language_servers.is_empty()
            {
                let servers: Vec<LSPServerType> =
                    workspace.language_servers.keys().copied().collect();
                ctx.emit(PersistedWorkspaceEvent::AvailableServersDetected {
                    workspace_path,
                    servers,
                });
                continue;
            }
            paths_to_scan.push(workspace_path);
        }

        if paths_to_scan.is_empty() {
            return;
        }

        // Get interactive PATH for should_suggest_for_repo checks
        let path_future = LocalShellState::handle(ctx).update(ctx, |shell_state, ctx| {
            shell_state.get_interactive_path_env_var(ctx)
        });
        let http_client = Arc::new(http_client::Client::new());

        ctx.spawn(
            async move {
                let path_env_var = path_future.await;
                let executor = lsp::CommandBuilder::new(path_env_var);

                let mut results: Vec<(PathBuf, Vec<LSPServerType>)> = Vec::new();
                for workspace_path in paths_to_scan {
                    let mut suggested = Vec::new();
                    for server_type in LSPServerType::all() {
                        let candidate = server_type.candidate(http_client.clone());
                        if candidate
                            .should_suggest_for_repo(&workspace_path, &executor)
                            .await
                        {
                            suggested.push(server_type);
                        }
                    }
                    if !suggested.is_empty() {
                        results.push((workspace_path, suggested));
                    }
                }
                results
            },
            move |me, results, ctx| {
                for (workspace_path, servers) in results {
                    // Insert Suggested entries into the workspace, without
                    // overwriting existing Yes/No entries.
                    let workspace =
                        me.workspaces
                            .entry(workspace_path.clone())
                            .or_insert_with(|| Workspace {
                                metadata: WorkspaceMetadata {
                                    path: workspace_path.clone(),
                                    navigated_ts: None,
                                    modified_ts: None,
                                    queried_ts: None,
                                },
                                language_servers: HashMap::new(),
                            });

                    for &server_type in &servers {
                        workspace
                            .language_servers
                            .entry(server_type)
                            .or_insert(EnablementState::Suggested);
                    }

                    ctx.emit(PersistedWorkspaceEvent::AvailableServersDetected {
                        workspace_path,
                        servers,
                    });
                }
            },
        );
    }

    /// Kicks off detection (deduped via Checking) and returns the best immediate status.
    /// Uses the interactive shell PATH for detection to ensure gopls and other tools
    /// installed in user-specific locations (like ~/go/bin) are found.
    ///
    /// Logic:
    /// 1. If enabled for repo => Enabled
    /// 2. If not enabled and Installed => DisabledAndInstalled
    /// 3. If NotInstalled => DisabledAndNotInstalled
    /// 4. If Installing => Installing
    /// 5. If Checking or Unknown => set Checking, start detection, return CheckingForInstallation
    #[cfg(feature = "local_fs")]
    pub fn detect_lsp_workspace_status(
        &mut self,
        repo_root: PathBuf,
        server_type: LSPServerType,
        ctx: &mut ModelContext<Self>,
    ) -> LspRepoStatus {
        // Determine enablement
        let is_enabled = self
            .enabled_lsp_servers(&repo_root)
            .map(|mut it| it.any(|s| s == server_type))
            .unwrap_or(false);

        // If enabled, do not check installation.
        if is_enabled {
            return LspRepoStatus::Enabled;
        }

        match self.lsp_installation_status.get(&server_type).copied() {
            Some(LSPInstallationStatus::Installed) => {
                LspRepoStatus::DisabledAndInstalled { server_type }
            }
            Some(LSPInstallationStatus::NotInstalled) => {
                LspRepoStatus::DisabledAndNotInstalled { server_type }
            }
            Some(LSPInstallationStatus::Checking) => LspRepoStatus::CheckingForInstallation,
            Some(LSPInstallationStatus::Installing) => LspRepoStatus::Installing { server_type },
            None => {
                // Mark as checking and start async detection with interactive PATH
                self.lsp_installation_status
                    .insert(server_type, LSPInstallationStatus::Checking);

                // Get a future for the interactive PATH
                let path_future = LocalShellState::handle(ctx).update(ctx, |shell_state, ctx| {
                    shell_state.get_interactive_path_env_var(ctx)
                });

                let http_client = Arc::new(http_client::Client::new());
                ctx.spawn(
                    async move {
                        // Wait for interactive PATH, then check installation
                        let path_env_var = path_future.await;
                        let executor = lsp::CommandBuilder::new(path_env_var);
                        let candidate = server_type.candidate(http_client);
                        candidate.is_installed(&executor).await
                    },
                    move |me, is_installed, ctx| {
                        let status = if is_installed {
                            LSPInstallationStatus::Installed
                        } else {
                            LSPInstallationStatus::NotInstalled
                        };
                        me.lsp_installation_status.insert(server_type, status);
                        ctx.emit(PersistedWorkspaceEvent::InstallStatusUpdate {
                            server_type,
                            status,
                        });
                    },
                );

                LspRepoStatus::CheckingForInstallation
            }
        }
    }
}

#[cfg(test)]
#[path = "persisted_workspace_tests.rs"]
mod tests;
