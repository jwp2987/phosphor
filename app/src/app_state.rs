use pathfinder_geometry::rect::RectF;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use warpui::platform::FullscreenState;

use warpui::AppContext;

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent_conversations_model::AgentManagementFilters;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::blocklist::InputConfig;
use crate::ai::blocklist::SerializedBlockListItem;
use crate::code::buffer_location::RemotePath;
use crate::code::editor_management::CodeSource;
use crate::drive::ZapDriveObjectSettings;
use crate::root_view::quake_mode_window_id;
use crate::server::ids::SyncId;
use crate::settings_view::SettingsSection;
use crate::tab::SelectedTabColor;
use crate::terminal::ShellLaunchData;
use crate::themes::theme::{AnsiColorIdentifier, ThemeKind};
use crate::workspace::tab_group::TabGroupId;
use crate::workspace::view::left_panel::ToolPanelView;
use crate::workspace::WorkspaceRegistry;
use warpui::SingletonEntity as _;

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub windows: Vec<WindowSnapshot>,
    pub active_window_index: Option<usize>,
    pub block_lists: Arc<HashMap<PaneUuid, Vec<SerializedBlockListItem>>>,
    pub running_mcp_servers: Vec<uuid::Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaneUuid(pub Vec<u8>);

/// Wrapper for persisting agent management filters to restore.
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedAgentManagementFilters {
    pub filters: AgentManagementFilters,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab_index: usize,
    pub bounds: Option<RectF>,
    pub fullscreen_state: FullscreenState,
    pub quake_mode: bool,
    pub universal_search_width: Option<f32>,
    pub warp_ai_width: Option<f32>,
    pub voltron_width: Option<f32>,
    pub warp_drive_index_width: Option<f32>,
    pub left_panel_open: bool,
    pub vertical_tabs_panel_open: bool,
    pub left_panel_width: Option<f32>,
    pub right_panel_width: Option<f32>,
    pub cli_subagent_width: Option<f32>,
    pub cli_subagent_height: Option<f32>,
    pub agent_management_filters: Option<PersistedAgentManagementFilters>,
    /// The per-window theme override for this window, if the user set one via the
    /// theme chooser's "This window" scope. Re-applied on restore.
    pub theme_override: Option<ThemeKind>,
    /// Tab groups defined in this window. Group order is implicit from
    /// member tabs' positions, so no explicit ordering is persisted.
    pub tab_groups: Vec<TabGroupSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabGroupSnapshot {
    pub id: TabGroupId,
    pub name: Option<String>,
    pub color: SelectedTabColor,
    pub collapsed: bool,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabSnapshot {
    pub custom_title: Option<String>,
    pub root: PaneNodeSnapshot,
    pub default_directory_color: Option<AnsiColorIdentifier>,
    pub selected_color: SelectedTabColor,
    pub left_panel: Option<LeftPanelSnapshot>,
    pub right_panel: Option<RightPanelSnapshot>,
    /// Tab group this tab belongs to, if any.
    pub group_id: Option<TabGroupId>,
    /// True when this tab is pinned to the front of the tab list.
    pub pinned: bool,
}

impl TabSnapshot {
    pub(crate) fn color(&self) -> Option<AnsiColorIdentifier> {
        self.selected_color.resolve(self.default_directory_color)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "LeafSnapshot is significantly larger than BranchSnapshot due to nested snapshot types."
)]
pub enum PaneNodeSnapshot {
    Branch(BranchSnapshot),
    Leaf(LeafSnapshot),
}

impl PaneNodeSnapshot {
    pub fn has_horizontal_split(&self) -> bool {
        match self {
            PaneNodeSnapshot::Leaf(_) => false,
            PaneNodeSnapshot::Branch(BranchSnapshot {
                direction,
                children,
            }) => {
                let self_has_split = *direction == SplitDirection::Horizontal && children.len() > 1;
                self_has_split
                    || children
                        .iter()
                        .any(|(_, child)| child.has_horizontal_split())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BranchSnapshot {
    pub direction: SplitDirection,
    pub children: Vec<(PaneFlex, PaneNodeSnapshot)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeafSnapshot {
    pub is_focused: bool,
    pub custom_vertical_tabs_title: Option<String>,
    pub contents: LeafContents,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LeafContents {
    Terminal(TerminalPaneSnapshot),
    Notebook(NotebookPaneSnapshot),
    /// A read-only image viewer pane backed by a local file.
    Image {
        path: Option<PathBuf>,
    },
    AIDocument(AIDocumentPaneSnapshot),
    Code(CodePaneSnapShot),
    EnvVarCollection(EnvVarCollectionPaneSnapshot),
    // Zap Wave 7-3: the `EnvironmentManagement` LeafContents variant was
    // physically removed along with the Ambient Agent UI subsystem.
    Workflow(WorkflowPaneSnapshot),
    Settings(SettingsPaneSnapshot),
    AIFact(AIFactPaneSnapshot),
    ExecutionProfileEditor,
    CodeReview(CodeReviewPaneSnapshot),
    AmbientAgent(AmbientAgentPaneSnapshot),
    /// An entrypoint pane type to launch other pane types from a search palette. The default view
    /// when creating a tab.
    Welcome {
        startup_directory: Option<PathBuf>,
    },
    /// A new first-time user experience which prioritizes choosing a coding repository.
    GetStarted,
}

#[cfg(feature = "local_fs")]
impl LeafContents {
    /// Whether this pane content should be written to (and later restored
    /// from) the SQLite app-state database.
    ///
    /// Non-persisted pane types are skipped entirely during the pane tree
    /// traversal in `save_app_state`, so no `pane_nodes` row is inserted for
    /// them. This is important: inserting a `pane_nodes` row with
    /// `is_leaf = true` but no matching `pane_leaves` row leaves an orphan
    /// that `read_node` cannot resolve, which causes the surrounding tab's
    /// restoration to fail and the whole tab to disappear on restart.
    pub(crate) fn is_persisted(&self) -> bool {
        match self {
            // Zap Wave 7-3: the `EnvironmentManagement` arm was physically
            // removed along with the variant.
            // Image viewer panes are intentionally not persisted: they render in-session but
            // are not restored after restart.
            LeafContents::Image { .. } => false,
            // Remote-file code pane: the remote buffer depends on an active
            // SSH connection, and the `RemoteFileTree` source can't be
            // restored (`is_restorable() == false`). Writing it to
            // persistence would leave behind an orphan `Code` row that gets
            // skipped during the restore phase, causing the whole tab to be
            // lost — so code panes with a remote source aren't persisted at all.
            LeafContents::Code(CodePaneSnapShot::Local { source, .. }) => {
                source.as_ref().map(|s| s.is_restorable()).unwrap_or(true)
            }
            // Unlike the remote code pane above, `NotebookPaneSnapshot::Remote` is always
            // persisted: `FileNotebookView::open_remote` is a stateless one-shot RPC fetch, not a
            // buffer-sync connection, so reopening it at restore time is safe even if the host
            // isn't connected yet — see `NotebookPaneSnapshot::Remote`'s doc comment.
            LeafContents::Terminal(_)
            | LeafContents::Notebook(_)
            | LeafContents::AIDocument(_)
            | LeafContents::EnvVarCollection(_)
            | LeafContents::Workflow(_)
            | LeafContents::Settings(_)
            | LeafContents::AIFact(_)
            | LeafContents::ExecutionProfileEditor
            | LeafContents::CodeReview(_)
            | LeafContents::AmbientAgent(_)
            | LeafContents::Welcome { .. }
            | LeafContents::GetStarted => true,
        }
    }
}

/// Snapshot of an ambient agent pane.
#[derive(Clone, Debug, PartialEq)]
pub struct AmbientAgentPaneSnapshot {
    pub uuid: Vec<u8>,
    // `task_id` is purposefully optional,
    // as you can have a valid state (i.e. an empty ambient-agent pane) where it is None.
    pub task_id: Option<AmbientAgentTaskId>,
}

/// Snapshot of the contents of a terminal pane.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalPaneSnapshot {
    pub uuid: Vec<u8>,
    pub cwd: Option<String>,
    pub shell_launch_data: Option<ShellLaunchData>,
    pub is_active: bool,
    pub is_read_only: bool,
    pub input_config: Option<InputConfig>,
    pub llm_model_override: Option<String>,
    pub active_profile_id: Option<SyncId>,
    pub conversation_ids_to_restore: Vec<AIConversationId>,
    /// The active conversation ID if the agent view was open in fullscreen mode.
    /// When `Some`, the agent view should be restored to fullscreen for this conversation.
    pub active_conversation_id: Option<AIConversationId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NotebookPaneSnapshot {
    NotebookObject {
        /// The ID of the notebook that was open in this pane. There are 3 possibilities:
        /// 1. The pane contains a newly-created notebook that has not been edited yet. It might not
        ///    have an ID yet (client or server), so this will be `None`.
        /// 2. The pane contains a notebook that hasn't been synced to the server yet, so this will
        ///    contain a client ID that should exist in SQLite.
        /// 3. The pane contains a notebook that's known to the server, so this will contain the
        ///    server ID.
        notebook_id: Option<SyncId>,
        // Settings for the notebook pane when it's opened (such as a folder to focus upon opening)
        settings: ZapDriveObjectSettings,
    },
    LocalFileNotebook {
        /// The path to the local file that was open in this pane. This may be `None` if
        /// the pane contained an unreadable file.
        path: Option<PathBuf>,
    },
    /// A file notebook pane open on a remote host, keyed by the same
    /// [`RemotePath`] (host + standardized path) that
    /// `FileNotebookView::open_remote` and `CodeSource::RemoteFileTree` use.
    /// Unlike remote code panes, this is persisted and restored: the
    /// notebook viewer is read-only and fetches content with a one-shot
    /// `ReadFileContext` RPC rather than the buffer-sync protocol's
    /// stateful, bidirectional SSH connection, so it's safe to eagerly
    /// re-issue at restore time. If the host isn't connected yet, the
    /// reopen attempt fails the same way a manual reload would, and the
    /// pane shows its existing load-error/retry UI rather than silently
    /// appearing empty.
    Remote { remote_path: RemotePath },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AIDocumentPaneSnapshot {
    Local {
        document_id: String,
        version: i32,
        content: Option<String>,
        title: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodePaneTabSnapshot {
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CodePaneSnapShot {
    Local {
        tabs: Vec<CodePaneTabSnapshot>,
        active_tab_index: usize,
        /// The full `CodeSource` for this pane, serialized as JSON in the DB.
        source: Option<CodeSource>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkflowPaneSnapshot {
    WorkflowObject {
        workflow_id: Option<SyncId>,
        // Settings for the workflow pane when it's opened (such as a folder to focus upon opening)
        settings: ZapDriveObjectSettings,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum EnvVarCollectionPaneSnapshot {
    // EnvVarCollectionObject snapshots operate under the same heuristics
    // as NotebookPaneSnapshot::NotebookObject
    EnvVarCollectionObject {
        env_var_collection_id: Option<SyncId>,
    },
}

// Zap Wave 7-3: `EnvironmentManagementPaneSnapshot` was physically removed along with the LeafContents variant.

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsPaneSnapshot {
    Local {
        current_page: SettingsSection,
        search_query: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum AIFactPaneSnapshot {
    Personal,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CodeReviewPaneSnapshot {
    Local {
        terminal_uuid: Vec<u8>,
        repo_path: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LeftPanelDisplayedTab {
    FileTree,
    GlobalSearch,
    ZapDrive,
    ConversationListView,
    SkillManager,
}

impl From<ToolPanelView> for LeftPanelDisplayedTab {
    fn from(view: ToolPanelView) -> Self {
        match view {
            ToolPanelView::ProjectExplorer => LeftPanelDisplayedTab::FileTree,
            ToolPanelView::GlobalSearch { .. } => LeftPanelDisplayedTab::GlobalSearch,
            ToolPanelView::ZapDrive => LeftPanelDisplayedTab::ZapDrive,
            ToolPanelView::ConversationListView => LeftPanelDisplayedTab::ConversationListView,
            ToolPanelView::SkillManager => LeftPanelDisplayedTab::SkillManager,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeftPanelSnapshot {
    pub left_panel_displayed_tab: LeftPanelDisplayedTab,
    pub pane_group_id: String,
    pub width: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RightPanelSnapshot {
    pub pane_group_id: String,
    pub width: usize,
    pub is_maximized: bool,
}

/// Copied from pane group model, which should be private to pane group.
#[derive(Clone, Debug, PartialEq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaneFlex(pub f32);

/// Collects the window snapshots that survive filtering, together with the
/// position the active window occupies **in the collected list**.
///
/// A candidate whose snapshot slot is `None` was filtered out (no workspace, a
/// transient tab-drag preview, or no tabs) and therefore never reaches
/// [`AppState::windows`]. `None` for the returned index means the active window
/// is one of those: it is not in the persisted list, so no entry in that list
/// is the active one.
///
/// # Why this is separate, and why it counts kept windows
///
/// Every consumer of [`AppState::active_window_index`] uses it as an index into
/// the *filtered* list. `open_from_restored` in `root_view.rs` (`:589`, loop at
/// `:609`, read at `:664-668`) and `save_app_state` in
/// `persistence/sqlite.rs` (`:1436`, read at `:1528`) both `enumerate()` over
/// `app_state.windows`. `get_app_state` instead counted over the *unfiltered*
/// `app.window_ids()`, so any filtered-out window ahead of the active one
/// shifted the index by one and the session restored with the wrong window
/// focused, or with none focused at all when the index ran past the end.
///
/// There are **two** producers of the field, not one. `read_sqlite_data` in
/// `persistence/sqlite.rs` (`:3556`) also fills it, at `:3662`, and that one is
/// already consistent: its `idx` enumerates the same `db_windows` vec that
/// becomes `AppState::windows` (`:3589-3594`), with no filtering step in
/// between. It is named here because an earlier version of this comment
/// claimed `get_app_state` was the only producer, which is false and would
/// have sent the next reader looking in one place.
///
/// `LaunchConfig::from_snapshot` is a third position, and the reason this
/// helper is `pub(crate)` rather than private. It does **not** simply index
/// `AppState::windows`: it narrows that list *again*, dropping quake-mode
/// windows, and its own consumer (`root_view.rs:452-470`) enumerates the
/// narrower vec. So it calls this function too, with list positions standing in
/// for window ids. Copying the index across that second filter was the same
/// off-by-one one layer down; see `launch_configs/launch_config.rs`.
///
/// The filtered reading is therefore the one that matches every consumer, and
/// it is also the only reading a persisted `AppState` can express: the
/// unfiltered list is not serialised, so an index into it is meaningless by the
/// time it is read back.
///
/// # Divergence from the pinned oracle
///
/// This is **not** a parity port. `42effe840:app/src/app_state.rs:353-395` has
/// the identical defect — `for (index, window_id) in
/// app.window_ids().enumerate()` with the assignment above the filters, its
/// `windows` vec built by the same three skips — so the oracle shares the bug
/// and we are fixing it ahead of the oracle deliberately.
/// `42effe840:app/src/launch_configs/launch_config.rs:22-33` is likewise
/// byte-identical to the pre-fix `from_snapshot`, so that half is a second
/// deliberate divergence. A re-pin must not "restore parity" by reverting
/// either.
///
/// # What is tested, and what is not
///
/// The tests in `app_state_tests.rs` exercise this function directly. They do
/// **not** exercise [`get_app_state`], and no unit test in this crate can:
/// `warpui::App::test` installs `warpui_core::platform::test::WindowManager`,
/// whose `active_window_id()` returns `None` unconditionally
/// (`crates/warpui_core/src/platform/test/delegate.rs:100-102`), and
/// `WindowState::active_window` is a straight passthrough to it
/// (`crates/warpui_core/src/windowing/state.rs:155-157`). Under that harness
/// `get_app_state` yields `active_window_index: None` no matter what it
/// computes, so such a test would assert `None == None` and pass against the
/// pre-fix count exactly as readily — the same vacuity this batch of work
/// exists to remove.
///
/// That leaves a real gap, recorded rather than papered over: **re-inlining the
/// old unfiltered `enumerate()` at the call site below would leave every test
/// in this crate green.** All that guards it is that the call site no longer
/// computes an index at all. Closing it needs either an active-window-tracking
/// `WindowManager` in `warpui_core`'s test platform — the headless one already
/// tracks it (`crates/warpui/src/platform/headless/windowing.rs:50-53,72-74`)
/// — or a `crates/integration` test on a real platform.
///
/// Generic over the id and snapshot types so the ordering rule can be tested
/// without an `AppContext` and shared by both producers: `get_app_state`
/// instantiates it at `(WindowId, WindowSnapshot)`, `from_snapshot` at
/// `(usize, WindowTemplate)`.
pub(crate) fn collect_windows_with_active_index<Id, S>(
    candidates: impl IntoIterator<Item = (Id, Option<S>)>,
    active_window_id: Option<Id>,
) -> (Vec<S>, Option<usize>)
where
    Id: PartialEq,
{
    let mut windows = Vec::new();
    let mut active_window_index = None;

    for (window_id, snapshot) in candidates {
        let Some(snapshot) = snapshot else {
            continue;
        };
        if active_window_id.as_ref() == Some(&window_id) {
            active_window_index = Some(windows.len());
        }
        windows.push(snapshot);
    }

    (windows, active_window_index)
}

pub fn get_app_state(app: &AppContext) -> AppState {
    let active_window_id = app.windows().active_window();
    let quake_mode_id = quake_mode_window_id();

    // `None` marks a window that is filtered out of the persisted session. The
    // ids are carried alongside so `collect_windows_with_active_index` can
    // count only the kept ones; see its doc comment for why.
    let mut candidates = vec![];

    for window_id in app.window_ids() {
        let mut snapshot = None;

        if let Some(workspace) = WorkspaceRegistry::as_ref(app).get(window_id, app) {
            let ws = workspace.as_ref(app);
            // Transient drag-preview windows are not real user-visible
            // workspaces; skip them so they never end up in the persisted
            // session. (Persistence is also short-circuited entirely while a
            // cross-window drag is active; see `save_app` in
            // `workspace/global_actions.rs`.)
            if !ws.is_tab_drag_preview() {
                let ws_snapshot = ws.snapshot(
                    window_id,
                    quake_mode_id.map(|id| id == window_id).unwrap_or(false),
                    app,
                );
                if !ws_snapshot.tabs.is_empty() {
                    snapshot = Some(ws_snapshot);
                }
            }
        }

        candidates.push((window_id, snapshot));
    }

    let (windows, active_window_index) =
        collect_windows_with_active_index(candidates, active_window_id);

    AppState {
        windows,
        active_window_index,
        block_lists: Default::default(),
        running_mcp_servers: Vec::new(),
    }
}

#[cfg(test)]
#[path = "app_state_tests.rs"]
mod tests;
