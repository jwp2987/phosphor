//! SSH manager main panel — the left-side Tool Panel content: a tree list +
//! toolbar + context menu + inline folder rename.
//!
//! UX rules:
//! - **Click a server**: connect directly (opens a terminal pane running
//!   ssh). Use right-click to edit.
//! - **Click a folder**: selects it only (highlight); rename via right-click
//!   "Rename" or by typing immediately after creating it.
//! - **Immediately enters rename mode after creating a new folder** (Drive-style).
//! - Right-click server: Edit / Connect / Delete
//! - Right-click folder: New Folder / New Server / Rename / Delete
//! - Right-click blank area: New Folder / New Server
//!
//! Visual polish follows the constants in `app/src/drive/index.rs`
//! (ITEM_FONT_SIZE=14 / indent 16 / row padding 4×8).

use std::collections::HashMap;

use pathfinder_geometry::vector::Vector2F;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    AcceptedByDropTarget, Border, ChildAnchor, ConstrainedBox, Container, CornerRadius,
    CrossAxisAlignment, Dismiss, Draggable, DraggableState, DropTarget, DropTargetData, Element,
    Empty, Fill as ElementFill, Flex, Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle,
    OffsetPositioning, ParentAnchor, ParentElement, ParentOffsetBounds, Radius, SavePosition,
    ScrollbarWidth, Stack, Text,
};
use warpui::elements::{ClippedScrollStateHandle, ClippedScrollable};
use warpui::platform::Cursor;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Entity, FocusContext, ModelHandle, SingletonEntity, TypedActionView, View,
    ViewContext, ViewHandle,
};

use warp_ssh_manager::{
    AuthType, KeychainSecretStore, NodeKind, SecretKind, SshNode, SshRepository, SshSecretStore,
    SshServerInfo,
};

use settings::Setting;

use crate::editor::{
    EditorView, Event as EditorEvent, SingleLineEditorOptions, TextColors, TextOptions,
};
use crate::settings::SshSettings;
use crate::ssh_manager::candidates::{CandidateRow, CandidatesViewModel};
use crate::ssh_manager::{SshTreeChangedEvent, SshTreeChangedNotifier};

// ---- Visual constants (based on Drive) ----
const TOOLBAR_BUTTON_SIZE: f32 = 26.0;
const TOOLBAR_ICON_SIZE: f32 = 14.0;
const ITEM_PADDING_VERTICAL: f32 = 5.0;
const ITEM_PADDING_HORIZONTAL: f32 = 8.0;
const ITEM_ICON_TEXT_SPACING: f32 = 8.0;
const ITEM_MARGIN_BOTTOM: f32 = 2.0;
const ITEM_ICON_SIZE: f32 = 14.0;
const FOLDER_DEPTH_INDENT: f32 = 16.0;
const PANEL_HORIZONTAL_PADDING: f32 = 8.0;

const CONTEXT_MENU_WIDTH: f32 = 200.0;
const CONTEXT_MENU_ITEM_PADDING_V: f32 = 7.0;
const CONTEXT_MENU_ITEM_PADDING_H: f32 = 12.0;
const MAX_CONTEXT_MENU_ITEMS: usize = 5;
const SSH_PANEL_POSITION_ID: &str = "ssh_manager_panel_root";

#[derive(Clone, Debug)]
pub enum SshManagerPanelAction {
    /// Toolbar button: always creates a folder at the root level.
    AddRootFolder,
    /// Context menu: decides the parent based on context.
    AddFolder,
    AddServer,
    DeleteSelected,
    Connect,
    Edit,
    CloneServer(String),
    /// Row click; the handling logic depends on the node kind:
    /// - server: select + emit OpenSshTerminal (connect directly)
    /// - folder: select only
    Click(String),
    StartRename(String),
    CommitRename,
    CancelRename,
    OpenContextMenu {
        target: Option<String>,
        position: Vector2F,
    },
    DismissContextMenu,
    /// Drag completed → moves `node_id` under `new_parent_id` (None = root).
    MoveNode {
        node_id: String,
        new_parent_id: Option<String>,
    },
    /// Collapses/expands a single folder. Server nodes are ignored.
    ToggleNodeCollapsed(String),
    /// Top button: smart toggle — if any folder is still expanded, collapse
    /// all; otherwise expand all.
    ToggleAllFolders,
    /// Double-clicking a server row = connect (opens a new tab). Double-clicking
    /// a folder = two toggles cancel out to a no-op.
    DoubleClick(String),
    /// Right-clicking a server → "File Manager": opens the SFTP file browser pane.
    OpenSftp,
    /// "Candidates" section: copies a candidate from `~/.ssh/config` into the saved tree.
    ImportCandidate {
        alias: String,
    },
    /// Re-reads `~/.ssh/config` (user clicks the Refresh button after editing the config).
    RefreshCandidates,
    /// Collapses/expands the "Candidates" section (manually collapsible when the list is long).
    ToggleCandidatesSection,
}

#[derive(Clone, Debug)]
pub enum SshManagerPanelEvent {
    /// The user right-clicked "Edit" on a server; the central pane should
    /// open/focus that server's editor (`Workspace::open_ssh_server`).
    OpenServerEditor {
        node_id: String,
    },
    /// The user clicked a server or right-clicked "Connect"; requests
    /// opening a terminal pane to run ssh + SecretInjector.
    OpenSshTerminal {
        node_id: String,
        server: SshServerInfo,
    },
    /// The user right-clicked "SFTP Browse"; requests opening the SFTP file
    /// browser pane.
    OpenSftpPane {
        node_id: String,
        server: SshServerInfo,
    },
    PersistenceError(String),
}

struct RenameState {
    node_id: String,
    editor: ViewHandle<EditorView>,
    /// Whether this rename was automatically triggered by creating a new folder.
    is_newly_created: bool,
}

/// Content fields for a single candidate row — bundles the few Options that
/// matter during rendering into one struct, to avoid
/// `render_candidate_row`'s parameter list getting too long
/// (clippy::too_many_arguments).
struct CandidateRowFields<'a> {
    alias: &'a str,
    hostname: Option<&'a str>,
    user: Option<&'a str>,
    port: Option<u16>,
    added: bool,
}

/// Theme color pairing — already-imported rows use muted, regular rows use main.
#[derive(Copy, Clone)]
struct CandidateRowColors {
    main: warp_core::ui::theme::Fill,
    muted: warp_core::ui::theme::Fill,
}

/// Drop-target metadata. `parent_id = None` means dropped onto the panel's
/// blank area (put back at root); `Some(folder_id)` means dropped into that
/// folder; dropping directly onto a server is **not allowed** (a server
/// can't have children) — in that case, drop_data is interpreted as "dropped
/// at the server's sibling position", i.e. `parent_id = server.parent_id`,
/// which is already expanded by the time the action is dispatched.
#[derive(Debug, Clone)]
struct SshDropData {
    parent_id: Option<String>,
}

impl DropTargetData for SshDropData {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct SshManagerPanel {
    nodes: Vec<SshNode>,
    depths: HashMap<String, usize>,
    selected_id: Option<String>,

    add_folder_btn: MouseStateHandle,
    add_server_btn: MouseStateHandle,
    toggle_all_btn: MouseStateHandle,
    row_states: HashMap<String, MouseStateHandle>,
    /// DraggableState for each row — drag progress must persist across
    /// renders, so it has to be cached in the view state.
    row_drag_states: HashMap<String, DraggableState>,

    context_menu_position: Option<Vector2F>,
    context_menu_target: Option<String>,
    context_menu_item_states: Vec<MouseStateHandle>,

    /// The node currently being renamed (editor + node_id).
    rename_state: Option<RenameState>,

    /// `~/.ssh/config` candidates view model — PRODUCT.md decision A/B/C/D/E.
    candidates: ModelHandle<CandidatesViewModel>,
    /// Hover state for each candidate row (key = alias).
    candidate_row_states: HashMap<String, MouseStateHandle>,
    /// Hover state for each candidate row's "+" / "Added" button (key = alias).
    candidate_add_states: HashMap<String, MouseStateHandle>,
    /// Hover state for the section header's Refresh / Toggle buttons.
    candidates_refresh_btn: MouseStateHandle,
    candidates_toggle_btn: MouseStateHandle,

    content_scroll_state: ClippedScrollStateHandle,
}

impl SshManagerPanel {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let candidates = ctx.add_model(|_| CandidatesViewModel::new());

        let mut me = Self {
            nodes: Vec::new(),
            depths: HashMap::new(),
            selected_id: None,
            add_folder_btn: MouseStateHandle::default(),
            add_server_btn: MouseStateHandle::default(),
            toggle_all_btn: MouseStateHandle::default(),
            row_states: HashMap::new(),
            row_drag_states: HashMap::new(),
            context_menu_position: None,
            context_menu_target: None,
            context_menu_item_states: (0..MAX_CONTEXT_MENU_ITEMS)
                .map(|_| MouseStateHandle::default())
                .collect(),
            rename_state: None,
            candidates,
            candidate_row_states: HashMap::new(),
            candidate_add_states: HashMap::new(),
            candidates_refresh_btn: MouseStateHandle::default(),
            candidates_toggle_btn: MouseStateHandle::default(),
            content_scroll_state: ClippedScrollStateHandle::default(),
        };
        // First time the panel opens → immediately read ssh_config once (PRODUCT.md decision A).
        me.candidates.update(ctx, |vm, ctx| vm.refresh(ctx));
        me.refresh_tree(ctx);

        ctx.subscribe_to_model(
            &SshTreeChangedNotifier::handle(ctx),
            |me, _, event, ctx| match event {
                SshTreeChangedEvent::TreeChanged => me.refresh_tree(ctx),
            },
        );

        // Listen for SshSettings changes, refreshing the candidates section when the auto-discovery toggle flips.
        ctx.subscribe_to_model(&SshSettings::handle(ctx), |me, _, _, ctx| {
            me.candidates.update(ctx, |vm, ctx| vm.refresh(ctx));
            me.sync_candidate_row_states(ctx);
            ctx.notify();
        });

        me
    }

    fn refresh_tree(&mut self, ctx: &mut ViewContext<Self>) {
        match warp_ssh_manager::with_conn(|c| Ok(SshRepository::list_nodes(c)?)) {
            Ok(nodes) => {
                self.depths = compute_depths(&nodes);
                self.nodes = sort_for_display(nodes, &self.depths);
                if let Some(id) = self.selected_id.clone() {
                    if !self.nodes.iter().any(|n| n.id == id) {
                        self.selected_id = None;
                    }
                }
                // If the node currently being renamed was deleted externally, clear rename_state
                if let Some(rs) = self.rename_state.as_ref() {
                    if !self.nodes.iter().any(|n| n.id == rs.node_id) {
                        self.rename_state = None;
                    }
                }
            }
            Err(e) => {
                log::error!("ssh_manager: failed to load tree: {e:?}");
                ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            }
        }

        let active_ids: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.id.as_str()).collect();
        self.row_states
            .retain(|k, _| active_ids.contains(k.as_str()));
        self.row_drag_states
            .retain(|k, _| active_ids.contains(k.as_str()));
        for n in &self.nodes {
            self.row_states.entry(n.id.clone()).or_default();
            self.row_drag_states.entry(n.id.clone()).or_default();
        }

        // Tree changed → recompute the "Added" set (PRODUCT.md decision E).
        // "Already imported" is determined by `server.host ==
        // candidate.alias` — aligned with how ImportCandidate writes it
        // (decision I: `server.host = alias` on import).
        let auto_discover = *SshSettings::as_ref(ctx).enable_ssh_auto_discovery.value();
        if auto_discover {
            let hosts = list_server_hosts();
            self.candidates
                .update(ctx, |vm, ctx| vm.on_tree_changed(hosts, ctx));
            self.sync_candidate_row_states(ctx);
        }

        ctx.notify();
    }

    /// Keeps the key sets of `candidate_row_states` / `candidate_add_states`
    /// in sync with the current candidates view-model's aliases. Extra
    /// hover states are removed (freeing memory); missing aliases get a
    /// default state added (so a newly added row's first hover doesn't lose
    /// its state).
    fn sync_candidate_row_states(&mut self, ctx: &mut ViewContext<Self>) {
        let aliases: Vec<String> = self
            .candidates
            .as_ref(ctx)
            .rows()
            .into_iter()
            .filter_map(|r| match r {
                CandidateRow::Candidate { alias, .. } => Some(alias),
                CandidateRow::Header { .. }
                | CandidateRow::NotFound { .. }
                | CandidateRow::Empty { .. }
                | CandidateRow::Error { .. } => None,
            })
            .collect();
        let alias_set: std::collections::HashSet<&str> =
            aliases.iter().map(|s| s.as_str()).collect();
        self.candidate_row_states
            .retain(|k, _| alias_set.contains(k.as_str()));
        self.candidate_add_states
            .retain(|k, _| alias_set.contains(k.as_str()));
        for a in aliases {
            self.candidate_row_states.entry(a.clone()).or_default();
            self.candidate_add_states.entry(a).or_default();
        }
    }

    /// Creates a new folder. Created at the root level when `parent` is None.
    fn on_add_folder_with_parent(&mut self, parent: Option<String>, ctx: &mut ViewContext<Self>) {
        let result = warp_ssh_manager::with_conn(|c| {
            let name = unique_name(c, parent.as_deref(), "New folder")?;
            Ok(SshRepository::create_folder(c, parent.as_deref(), &name)?)
        });
        match result {
            Ok(node) => {
                let new_id = node.id.clone();
                self.selected_id = Some(new_id.clone());
                self.refresh_tree(ctx);
                // Create-then-rename — Drive convention.
                self.enter_rename(new_id, true, ctx);
            }
            Err(e) => {
                log::error!("ssh_manager: create folder failed: {e:?}");
                ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            }
        }
    }

    /// Imports one candidate from `~/.ssh/config` as a new saved server.
    ///
    /// Field mapping strictly follows TECH.md §3.3 / PRODUCT.md decision I/J/K:
    /// - `server.host = alias` (keeps OpenSSH alias semantics, so launching
    ///   `ssh` can still pick up directives like `ProxyJump` from
    ///   `~/.ssh/config`);
    /// - `port = candidate.port.unwrap_or(22)` (decision K's "port=None → 22");
    /// - `auth_type = Key if identity_file.is_some() else Password` (decision J);
    /// - `notes = "Imported from <resolved path>"` (so the user can trace the source later).
    ///
    /// Written via `SshRepository::create_server`, going through the same
    /// persistence path as manual "New server" — any schema change to that
    /// SQLite row is automatically picked up by the import path. On
    /// completion, emits `OpenServerEditor` (consistent with manual
    /// creation) + broadcasts `SshTreeChangedEvent::TreeChanged` so the
    /// `Added` badge flips immediately.
    fn on_import_candidate(&mut self, alias: String, ctx: &mut ViewContext<Self>) {
        let candidate = self
            .candidates
            .read(ctx, |vm, _| vm.find_candidate(&alias).cloned());
        let Some(c) = candidate else {
            log::warn!("ssh_manager: ImportCandidate alias not found: {alias}");
            return;
        };
        let path_display = self
            .candidates
            .read(ctx, |vm, _| vm.path_display())
            .unwrap_or_default();

        let auth_type = if c.identity_file.is_some() {
            AuthType::Key
        } else {
            AuthType::Password
        };
        let info = SshServerInfo {
            node_id: String::new(),
            // PRODUCT.md decision I: store the alias, not the resolved HostName.
            host: c.alias.clone(),
            port: c.port.unwrap_or(22),
            username: c.user.clone().unwrap_or_default(),
            auth_type,
            key_path: c
                .identity_file
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            credential_id: None,
            startup_command: None,
            notes: Some(format!("Imported from {path_display}")),
            last_connected_at: None,
        };

        let parent = self.parent_for_new_node();
        let result = warp_ssh_manager::with_conn(|conn| {
            // Same "auto-dedupe" naming logic as manual "New server"
            // (unique_name); the first candidate uses its alias as the
            // name, and on a collision appends " 2", " 3", …
            let name = unique_name(conn, parent.as_deref(), &c.alias)?;
            Ok(SshRepository::create_server(
                conn,
                parent.as_deref(),
                &name,
                &info,
            )?)
        });
        match result {
            Ok(node) => {
                let new_id = node.id.clone();
                self.selected_id = Some(new_id.clone());
                self.refresh_tree(ctx);
                // Consistent with manual creation: open the central edit
                // pane, letting the user fill in the password / tweak fields.
                ctx.emit(SshManagerPanelEvent::OpenServerEditor { node_id: new_id });
                // Broadcast the tree change — the Candidates section's added_aliases refreshes from this.
                SshTreeChangedNotifier::handle(ctx).update(ctx, |_, ctx| {
                    ctx.emit(SshTreeChangedEvent::TreeChanged);
                });
            }
            Err(e) => {
                log::error!("ssh_manager: import candidate failed: {e:?}");
                ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            }
        }
    }

    fn on_add_server(&mut self, ctx: &mut ViewContext<Self>) {
        let parent = self.parent_for_new_node();
        let info_template = SshServerInfo::new_default(String::new());
        let result = warp_ssh_manager::with_conn(|c| {
            let name = unique_name(c, parent.as_deref(), "New server")?;
            Ok(SshRepository::create_server(
                c,
                parent.as_deref(),
                &name,
                &info_template,
            )?)
        });
        match result {
            Ok(node) => {
                let new_id = node.id.clone();
                self.selected_id = Some(new_id.clone());
                self.refresh_tree(ctx);
                // After creating a server, open the central edit pane (for
                // the user to fill in fields) — name editing happens there
                // together with the other fields, not inline in the tree.
                ctx.emit(SshManagerPanelEvent::OpenServerEditor { node_id: new_id });
            }
            Err(e) => {
                log::error!("ssh_manager: create server failed: {e:?}");
                ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            }
        }
    }

    fn on_clone_server(&mut self, source_id: &str, ctx: &mut ViewContext<Self>) {
        let source_id = source_id.to_string();
        let result = warp_ssh_manager::with_conn(|c| {
            let source_info = SshRepository::get_server(c, &source_id)?
                .ok_or_else(|| warp_ssh_manager::SshRepositoryError::NotFound(source_id.clone()))?;
            let source_node = SshRepository::list_nodes(c)?
                .into_iter()
                .find(|n| n.id == source_id)
                .ok_or_else(|| warp_ssh_manager::SshRepositoryError::NotFound(source_id.clone()))?;

            let parent = source_node.parent_id;
            let cloned_info = SshServerInfo::clone_from_template(&source_info, String::new());
            let name = unique_name(
                c,
                parent.as_deref(),
                &format!("{} (copy)", source_node.name),
            )?;

            let new_node = SshRepository::create_server(c, parent.as_deref(), &name, &cloned_info)?;

            // The source server was already verified to exist above; directly
            // copy the password / private-key passphrase from the keychain to the new node.
            let store = KeychainSecretStore;
            if let Ok(Some(password)) = store.get(&source_id, SecretKind::Password) {
                let _ = store.set(&new_node.id, SecretKind::Password, &password);
            }
            if let Ok(Some(passphrase)) = store.get(&source_id, SecretKind::Passphrase) {
                let _ = store.set(&new_node.id, SecretKind::Passphrase, &passphrase);
            }

            Ok(new_node)
        });
        match result {
            Ok(node) => {
                let new_id = node.id.clone();
                self.selected_id = Some(new_id.clone());
                self.refresh_tree(ctx);
                ctx.emit(SshManagerPanelEvent::OpenServerEditor { node_id: new_id });
            }
            Err(e) => {
                log::error!("ssh_manager: clone server failed: {e:?}");
                ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            }
        }
    }

    fn on_delete_selected(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(id) = self.selected_id.clone() else {
            return;
        };
        let result = warp_ssh_manager::with_conn(|c| Ok(SshRepository::delete_node(c, &id)?));
        if let Err(e) = result {
            log::error!("ssh_manager: delete failed: {e:?}");
            ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            return;
        }
        let store = KeychainSecretStore;
        let _ = store.delete(&id, SecretKind::Password);
        let _ = store.delete(&id, SecretKind::Passphrase);
        let _ = store.delete(&id, SecretKind::RootPassword);

        self.selected_id = None;
        self.refresh_tree(ctx);
        SshTreeChangedNotifier::handle(ctx).update(ctx, |_, ctx| {
            ctx.emit(SshTreeChangedEvent::TreeChanged);
        });
    }

    fn on_connect(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(id) = self.selected_id.clone() else {
            return;
        };
        self.dispatch_connect_for(&id, ctx);
    }

    /// Right-click "SFTP Browse": emits the OpenSftpPane event.
    fn on_open_sftp(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(id) = self.selected_id.clone() else {
            return;
        };
        let kind = self.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
        if !matches!(kind, Some(NodeKind::Server)) {
            return;
        }
        let server = warp_ssh_manager::with_conn(|c| Ok(SshRepository::get_server(c, &id)?))
            .ok()
            .flatten();
        if let Some(server) = server {
            ctx.emit(SshManagerPanelEvent::OpenSftpPane {
                node_id: id,
                server,
            });
        }
    }

    fn dispatch_connect_for(&self, id: &str, ctx: &mut ViewContext<Self>) {
        let kind = self.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
        if !matches!(kind, Some(NodeKind::Server)) {
            return;
        }
        let server = warp_ssh_manager::with_conn(|c| Ok(SshRepository::get_server(c, id)?))
            .ok()
            .flatten();
        if let Some(server) = server {
            ctx.emit(SshManagerPanelEvent::OpenSshTerminal {
                node_id: id.to_string(),
                server,
            });
        }
    }

    fn on_edit(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(id) = self.selected_id.clone() else {
            return;
        };
        let kind = self.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
        if !matches!(kind, Some(NodeKind::Server)) {
            // "Edit" for a folder = rename
            self.enter_rename(id, false, ctx);
            return;
        }
        ctx.emit(SshManagerPanelEvent::OpenServerEditor { node_id: id });
    }

    /// Double-clicking a server = connect (opens a new tab). Double-clicking
    /// a folder = two toggles cancel each other out, no-op.
    fn on_double_click(&mut self, id: String, ctx: &mut ViewContext<Self>) {
        let kind = self.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
        if matches!(kind, Some(NodeKind::Server)) {
            self.dispatch_connect_for(&id, ctx);
        }
    }

    /// Toggles a single folder's collapsed state; server nodes are ignored.
    fn on_toggle_node_collapsed(&mut self, node_id: &str, ctx: &mut ViewContext<Self>) {
        let kind = self.nodes.iter().find(|n| n.id == node_id).map(|n| n.kind);
        if !matches!(kind, Some(NodeKind::Folder)) {
            return;
        }
        let new_collapsed = !self
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.is_collapsed)
            .unwrap_or(false);
        let id = node_id.to_string();
        let result = warp_ssh_manager::with_conn(move |c| {
            Ok(SshRepository::set_collapsed(c, &id, new_collapsed)?)
        });
        if let Err(e) = result {
            log::error!("ssh_manager: toggle collapse failed: {e:?}");
            ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            return;
        }
        self.refresh_tree(ctx);
        SshTreeChangedNotifier::handle(ctx).update(ctx, |_, ctx| {
            ctx.emit(SshTreeChangedEvent::TreeChanged);
        });
    }

    /// Top button: if any folder is currently expanded → collapse all; if
    /// all are already collapsed → expand all.
    fn on_toggle_all_folders(&mut self, ctx: &mut ViewContext<Self>) {
        let any_expanded = self
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Folder) && !n.is_collapsed);
        let new_collapsed = any_expanded; // At least one expanded → collapse all; otherwise expand all
        let result = warp_ssh_manager::with_conn(|c| {
            Ok(SshRepository::set_all_folders_collapsed(c, new_collapsed)?)
        });
        if let Err(e) = result {
            log::error!("ssh_manager: toggle all failed: {e:?}");
            ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            return;
        }
        self.refresh_tree(ctx);
        SshTreeChangedNotifier::handle(ctx).update(ctx, |_, ctx| {
            ctx.emit(SshTreeChangedEvent::TreeChanged);
        });
    }

    /// Whether a node is visually visible — hidden if any ancestor folder is
    /// collapsed. Root-level nodes are always visible.
    fn is_visible(&self, node: &SshNode) -> bool {
        let mut cursor = node.parent_id.as_deref();
        while let Some(pid) = cursor {
            let parent = match self.nodes.iter().find(|n| n.id == pid) {
                Some(p) => p,
                None => return true, // Data inconsistency; show it just in case
            };
            if matches!(parent.kind, NodeKind::Folder) && parent.is_collapsed {
                return false;
            }
            cursor = parent.parent_id.as_deref();
        }
        true
    }

    fn on_click(&mut self, id: String, ctx: &mut ViewContext<Self>) {
        // Clicking another row = exit the current rename (commit)
        if self
            .rename_state
            .as_ref()
            .map(|rs| rs.node_id != id)
            .unwrap_or(false)
        {
            self.commit_rename(ctx);
        }

        // commit_rename clears selected_id for a newly created folder
        // (is_newly_created), but the semantics of a click-to-navigate is to
        // select the clicked item, so immediately overwriting it with the
        // new id here is expected behavior — the clearing only applies to
        // exit paths with no new selection, like Enter/ESC/blur-to-blank; a
        // click itself already provides a new selection context.
        self.selected_id = Some(id.clone());
        let kind = self.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
        match kind {
            Some(NodeKind::Server) => {
                // Clicking a server = select only. **Connecting happens via double-click** (`on_double_click`).
            }
            Some(NodeKind::Folder) => {
                // Clicking a folder = collapse/expand toggle (selection already done above)
                self.on_toggle_node_collapsed(&id, ctx);
                return; // on_toggle already calls ctx.notify internally
            }
            None => {}
        }
        ctx.notify();
    }

    fn on_open_context_menu(
        &mut self,
        target: Option<String>,
        position: Vector2F,
        ctx: &mut ViewContext<Self>,
    ) {
        // Close rename before opening the menu (otherwise the rename buffer would be lost).
        if self.rename_state.is_some() {
            self.commit_rename(ctx);
        }
        if let Some(t) = target.as_ref() {
            self.selected_id = Some(t.clone());
        } else {
            // Right-clicking a blank area means operating at the root level; clear the old selection.
            self.selected_id = None;
        }
        self.context_menu_target = target;
        self.context_menu_position = Some(position);
        ctx.notify();
    }

    fn on_dismiss_context_menu(&mut self, ctx: &mut ViewContext<Self>) {
        self.context_menu_position = None;
        self.context_menu_target = None;
        ctx.notify();
    }

    fn enter_rename(
        &mut self,
        node_id: String,
        is_newly_created: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let current_name = self
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();

        let editor = ctx.add_typed_action_view(move |ctx| {
            let appearance = warp_core::ui::appearance::Appearance::as_ref(ctx);
            let theme = appearance.theme();
            let options = SingleLineEditorOptions {
                is_password: false,
                text: TextOptions {
                    font_size_override: Some(appearance.ui_font_subheading()),
                    font_family_override: Some(appearance.ui_font_family()),
                    text_colors_override: Some(TextColors {
                        default_color: theme.active_ui_text_color(),
                        disabled_color: theme.disabled_ui_text_color(),
                        hint_color: theme.disabled_ui_text_color(),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut editor = EditorView::single_line(options, ctx);
            editor.set_buffer_text(&current_name, ctx);
            editor
        });

        // Listen for Enter / Blurred → commit; Escape → cancel.
        ctx.subscribe_to_view(&editor, |me, _, event, ctx| match event {
            EditorEvent::Enter => me.commit_rename(ctx),
            EditorEvent::Blurred => me.commit_rename(ctx),
            EditorEvent::Escape => me.cancel_rename(ctx),
            _ => {}
        });

        ctx.focus(&editor);
        self.rename_state = Some(RenameState {
            node_id,
            editor,
            is_newly_created,
        });
        ctx.notify();
    }

    fn commit_rename(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(rs) = self.rename_state.take() else {
            return;
        };
        let new_name = rs.editor.as_ref(ctx).buffer_text(ctx).trim().to_string();
        let id = rs.node_id.clone();
        let was_newly_created = rs.is_newly_created;
        if new_name.is_empty() {
            // The name can't be empty: revert. Also clear the selection for a newly created folder.
            if was_newly_created {
                self.selected_id = None;
            }
            ctx.notify();
            return;
        }
        let result =
            warp_ssh_manager::with_conn(|c| Ok(SshRepository::rename_node(c, &id, &new_name)?));
        if let Err(e) = result {
            log::error!("ssh_manager: rename failed: {e:?}");
            ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            return;
        }
        // After finishing the rename of a newly created folder, clear the
        // selection so the next "New Folder" is created at the root level.
        if was_newly_created {
            self.selected_id = None;
        }
        self.refresh_tree(ctx);
        SshTreeChangedNotifier::handle(ctx).update(ctx, |_, ctx| {
            ctx.emit(SshTreeChangedEvent::TreeChanged);
        });
    }

    fn cancel_rename(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(rs) = self.rename_state.take() {
            // After cancelling the rename of a newly created folder, clear
            // the selection so the next "New Folder" is created at the root level.
            if rs.is_newly_created {
                self.selected_id = None;
            }
        }
        ctx.notify();
    }

    /// Checks whether moving `dragged` to `new_parent` would create a cycle
    /// (also directly rejects, to save a write, if `new_parent` is a
    /// descendant of `dragged` / itself / already the current parent).
    fn move_is_legal(&self, dragged: &str, new_parent: Option<&str>) -> bool {
        // Moving under itself: forbidden
        if Some(dragged) == new_parent {
            return false;
        }
        // No-op move: reject (avoids an idempotent write)
        let current_parent = self
            .nodes
            .iter()
            .find(|n| n.id == dragged)
            .and_then(|n| n.parent_id.as_deref());
        if current_parent == new_parent {
            return false;
        }
        // Moving a folder under its own descendant: forbidden (cycle)
        if let Some(target_parent) = new_parent {
            let mut cursor = Some(target_parent);
            while let Some(id) = cursor {
                if id == dragged {
                    return false;
                }
                cursor = self
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .and_then(|n| n.parent_id.as_deref());
            }
        }
        true
    }

    fn on_move_node(
        &mut self,
        node_id: String,
        new_parent_id: Option<String>,
        ctx: &mut ViewContext<Self>,
    ) {
        if !self.move_is_legal(&node_id, new_parent_id.as_deref()) {
            // Upgraded to warn: this log line is easier to find than debug
            // when the user can't see any effect from dragging.
            // Most `false` results come from "dropped onto the current parent / onto itself".
            let current_parent = self
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .and_then(|n| n.parent_id.clone());
            log::warn!(
                "ssh_manager: move rejected. node={node_id} current_parent={current_parent:?} target_parent={new_parent_id:?}"
            );
            return;
        }
        // sort_order takes the target parent's current maximum + 1 (placed
        // last). A simplified approach would use i32::MAX/2 to let the SQL
        // layer put it last (normalized later). Here we run a SQL query to
        // get the real next_sort_order.
        let result = warp_ssh_manager::with_conn(|c| {
            use diesel::prelude::*;
            use persistence::schema::ssh_nodes;
            let max: Option<i32> = match new_parent_id.as_deref() {
                Some(p) => ssh_nodes::table
                    .filter(ssh_nodes::parent_id.eq(p))
                    .select(diesel::dsl::max(ssh_nodes::sort_order))
                    .first(c)?,
                None => ssh_nodes::table
                    .filter(ssh_nodes::parent_id.is_null())
                    .select(diesel::dsl::max(ssh_nodes::sort_order))
                    .first(c)?,
            };
            let next_sort = max.unwrap_or(-1) + 1;
            Ok(SshRepository::move_node(
                c,
                &node_id,
                new_parent_id.as_deref(),
                next_sort,
            )?)
        });
        if let Err(e) = result {
            log::error!("ssh_manager: move failed: {e:?}");
            ctx.emit(SshManagerPanelEvent::PersistenceError(e.to_string()));
            return;
        }
        self.refresh_tree(ctx);
        SshTreeChangedNotifier::handle(ctx).update(ctx, |_, ctx| {
            ctx.emit(SshTreeChangedEvent::TreeChanged);
        });
    }

    fn parent_for_new_node(&self) -> Option<String> {
        resolve_parent_for_new_node(self.selected_id.as_deref(), &self.nodes)
    }

    fn render_toolbar(
        &self,
        appearance: &warp_core::ui::appearance::Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let icon_color = theme.sub_text_color(theme.background());

        let make_btn = |icon: crate::ui_components::icons::Icon,
                        state: MouseStateHandle,
                        action: SshManagerPanelAction|
         -> Box<dyn Element> {
            let icon_el = ConstrainedBox::new(icon.to_warpui_icon(icon_color).finish())
                .with_width(TOOLBAR_ICON_SIZE)
                .with_height(TOOLBAR_ICON_SIZE)
                .finish();
            Hoverable::new(state, move |_| {
                Container::new(
                    ConstrainedBox::new(icon_el)
                        .with_width(TOOLBAR_BUTTON_SIZE)
                        .with_height(TOOLBAR_BUTTON_SIZE)
                        .finish(),
                )
                .with_uniform_padding(2.0)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
                .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(action.clone());
            })
            .finish()
        };

        // Left group: create buttons
        let left_group = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(4.0)
            .with_child(make_btn(
                crate::ui_components::icons::Icon::Folder,
                self.add_folder_btn.clone(),
                SshManagerPanelAction::AddRootFolder,
            ))
            .with_child(make_btn(
                crate::ui_components::icons::Icon::Plus,
                self.add_server_btn.clone(),
                SshManagerPanelAction::AddServer,
            ))
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        // Right side: collapse-all/expand-all button — smart toggle. If any
        // folder is currently expanded → show ChevronUp (meaning "collapse
        // all"), otherwise show ChevronDown (meaning "expand all").
        let any_expanded = self
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Folder) && !n.is_collapsed);
        let toggle_icon = if any_expanded {
            crate::ui_components::icons::Icon::ChevronUp
        } else {
            crate::ui_components::icons::Icon::ChevronDown
        };
        let right_group = make_btn(
            toggle_icon,
            self.toggle_all_btn.clone(),
            SshManagerPanelAction::ToggleAllFolders,
        );

        // The whole toolbar: aligns both ends (MainAxisAlignment::SpaceBetween).
        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(warpui::elements::MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(left_group)
            .with_child(right_group)
            .finish()
    }

    /// The "Candidates" section — a list of importable hosts parsed from `~/.ssh/config`.
    ///
    /// This section is displayed **above** the saved tree; its layout style
    /// (row height, indentation, font size) matches the tree, only adding a
    /// Refresh button + collapse chevron on the section header. Each
    /// candidate row ends with a "+" or "Added" badge (PRODUCT.md decision E).
    fn render_candidates(
        &self,
        appearance: &warp_core::ui::appearance::Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let rows = self.candidates.as_ref(app).rows();
        if rows.is_empty() {
            // refresh hasn't been called yet — don't render the section at
            // all (shouldn't happen before the panel is mounted, since
            // new() calls it immediately, but this is a fallback just in case).
            return Empty::new().finish();
        }

        let muted = theme.sub_text_color(theme.background());
        let main = theme.main_text_color(theme.background());

        let mut col = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);

        for row in rows {
            match row {
                CandidateRow::Header {
                    path_display,
                    count,
                    can_refresh,
                } => {
                    col.add_child(self.render_candidates_header(
                        &path_display,
                        count,
                        can_refresh,
                        appearance,
                        app,
                    ));
                }
                CandidateRow::NotFound { path_display } => {
                    col.add_child(self.render_candidates_message(
                        &crate::t!(
                            "workspace-left-panel-ssh-manager-candidates-not-found",
                            path = path_display
                        ),
                        muted,
                        appearance,
                    ));
                }
                CandidateRow::Empty { path_display } => {
                    col.add_child(self.render_candidates_message(
                        &crate::t!(
                            "workspace-left-panel-ssh-manager-candidates-empty",
                            path = path_display
                        ),
                        muted,
                        appearance,
                    ));
                }
                CandidateRow::Error {
                    path_display,
                    message,
                } => {
                    // Error rows use the error red — `ui_error_color`
                    // returns a ColorU directly, the same pattern used by
                    // the "over-limit character counter" in
                    // `ai_assistant/panel.rs`.
                    let err_color: pathfinder_color::ColorU = theme.ui_error_color();
                    col.add_child(self.render_candidates_message_color(
                        &crate::t!(
                            "workspace-left-panel-ssh-manager-candidates-error",
                            path = path_display,
                            error = message
                        ),
                        err_color,
                        appearance,
                    ));
                }
                CandidateRow::Candidate {
                    alias,
                    hostname,
                    user,
                    port,
                    identity_file: _,
                    added,
                } => {
                    col.add_child(self.render_candidate_row(
                        CandidateRowFields {
                            alias: &alias,
                            hostname: hostname.as_deref(),
                            user: user.as_deref(),
                            port,
                            added,
                        },
                        CandidateRowColors { main, muted },
                        appearance,
                    ));
                }
            }
        }

        col.with_main_axis_size(MainAxisSize::Min).finish()
    }

    fn render_candidates_header(
        &self,
        path_display: &str,
        count: usize,
        can_refresh: bool,
        appearance: &warp_core::ui::appearance::Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let icon_color = theme.sub_text_color(theme.background());
        let muted = theme.sub_text_color(theme.background());

        // Collapsed-state chevron (▶) vs expanded-state (▼) — is_expanded is
        // read directly from the view-model.
        let expanded = self.candidates.as_ref(app).is_expanded();
        let chevron_icon = if expanded {
            crate::ui_components::icons::Icon::ChevronDown
        } else {
            crate::ui_components::icons::Icon::ChevronRight
        };
        let chevron_el = ConstrainedBox::new(chevron_icon.to_warpui_icon(icon_color).finish())
            .with_width(ITEM_ICON_SIZE)
            .with_height(ITEM_ICON_SIZE)
            .finish();

        let header_text = crate::t!(
            "workspace-left-panel-ssh-manager-candidates-header",
            path = path_display
        );
        let label = Text::new_inline(
            header_text,
            appearance.ui_font_family(),
            appearance.ui_font_subheading(),
        )
        .with_color(muted.into())
        .finish();

        let count_label = Text::new_inline(
            format!("({count})"),
            appearance.ui_font_family(),
            appearance.ui_font_body(),
        )
        .with_color(muted.into())
        .finish();

        // Refresh button on the right — refreshing is allowed in any state (NotFound / Error / Loaded).
        let refresh_state = self.candidates_refresh_btn.clone();
        let refresh_icon = ConstrainedBox::new(
            crate::ui_components::icons::Icon::Refresh
                .to_warpui_icon(icon_color)
                .finish(),
        )
        .with_width(ITEM_ICON_SIZE)
        .with_height(ITEM_ICON_SIZE)
        .finish();
        let refresh_btn = if can_refresh {
            Hoverable::new(refresh_state, move |_| {
                Container::new(refresh_icon)
                    .with_uniform_padding(2.0)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.0)))
                    .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(|ctx, _, _| {
                ctx.dispatch_typed_action(SshManagerPanelAction::RefreshCandidates);
            })
            .finish()
        } else {
            refresh_icon
        };

        // The full row: chevron + label (takes up the middle space) + count
        // + Refresh button. Uses MainAxisSize::Max to make the whole row
        // fill the panel width, eliminating trailing whitespace.
        let row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(ITEM_ICON_TEXT_SPACING)
            .with_child(chevron_el)
            .with_child(label)
            .with_child(count_label)
            .with_child(
                ConstrainedBox::new(Empty::new().finish())
                    .with_width(8.0)
                    .finish(),
            )
            .with_child(refresh_btn)
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .finish();

        // Clicking anywhere on the header = toggle (similar to a folder row's single-click).
        let toggle_state = self.candidates_toggle_btn.clone();
        Hoverable::new(toggle_state, move |_| {
            Container::new(row)
                .with_padding_top(ITEM_PADDING_VERTICAL)
                .with_padding_bottom(ITEM_PADDING_VERTICAL)
                .with_padding_left(ITEM_PADDING_HORIZONTAL)
                .with_padding_right(ITEM_PADDING_HORIZONTAL)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(|ctx, _, _| {
            ctx.dispatch_typed_action(SshManagerPanelAction::ToggleCandidatesSection);
        })
        .finish()
    }

    fn render_candidates_message(
        &self,
        text: &str,
        color: warp_core::ui::theme::Fill,
        appearance: &warp_core::ui::appearance::Appearance,
    ) -> Box<dyn Element> {
        Container::new(
            Text::new_inline(
                text.to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_body(),
            )
            .with_color(color.into())
            .finish(),
        )
        .with_padding_top(ITEM_PADDING_VERTICAL)
        .with_padding_bottom(ITEM_PADDING_VERTICAL)
        .with_padding_left(ITEM_PADDING_HORIZONTAL + FOLDER_DEPTH_INDENT)
        .with_padding_right(ITEM_PADDING_HORIZONTAL)
        .finish()
    }

    /// Same as `render_candidates_message`, but takes a `ColorU` — Error
    /// rows use the red returned directly by the theme's
    /// `ui_error_color()`, avoiding another round of Fill wrapping.
    fn render_candidates_message_color(
        &self,
        text: &str,
        color: pathfinder_color::ColorU,
        appearance: &warp_core::ui::appearance::Appearance,
    ) -> Box<dyn Element> {
        Container::new(
            Text::new_inline(
                text.to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_body(),
            )
            .with_color(color)
            .finish(),
        )
        .with_padding_top(ITEM_PADDING_VERTICAL)
        .with_padding_bottom(ITEM_PADDING_VERTICAL)
        .with_padding_left(ITEM_PADDING_HORIZONTAL + FOLDER_DEPTH_INDENT)
        .with_padding_right(ITEM_PADDING_HORIZONTAL)
        .finish()
    }

    fn render_candidate_row(
        &self,
        fields: CandidateRowFields<'_>,
        colors: CandidateRowColors,
        appearance: &warp_core::ui::appearance::Appearance,
    ) -> Box<dyn Element> {
        let CandidateRowFields {
            alias,
            hostname,
            user,
            port,
            added,
        } = fields;
        let CandidateRowColors { main, muted } = colors;
        let theme = appearance.theme();
        let icon = crate::ui_components::icons::Icon::Key
            .to_warpui_icon(theme.sub_text_color(theme.background()))
            .finish();
        let icon_el = ConstrainedBox::new(icon)
            .with_width(ITEM_ICON_SIZE)
            .with_height(ITEM_ICON_SIZE)
            .finish();

        // Primary label = alias; subtitle = "user@hostname:port" shorthand,
        // both assembled from whichever parts are present.
        // Already-imported rows have their entire text color dimmed (decision E: dimmed).
        let label_color = if added { muted } else { main };
        let alias_text = Text::new_inline(
            alias.to_string(),
            appearance.ui_font_family(),
            appearance.ui_font_subheading(),
        )
        .with_color(label_color.into())
        .finish();

        let mut subtitle_parts: Vec<String> = Vec::new();
        if let Some(u) = user {
            subtitle_parts.push(u.to_string());
        }
        if let Some(h) = hostname {
            // user@host; only shows host when there's no user
            let last = subtitle_parts.last_mut();
            match last {
                Some(s) => *s = format!("{s}@{h}"),
                None => subtitle_parts.push(h.to_string()),
            }
        }
        if let Some(p) = port {
            // Append :port to the end of the last segment; if both
            // user/hostname are missing, use a standalone :port.
            if let Some(last) = subtitle_parts.last_mut() {
                *last = format!("{last}:{p}");
            } else {
                subtitle_parts.push(format!(":{p}"));
            }
        }
        let subtitle: Option<Box<dyn Element>> = if subtitle_parts.is_empty() {
            None
        } else {
            Some(
                Text::new_inline(
                    subtitle_parts.join(" "),
                    appearance.ui_font_family(),
                    appearance.ui_font_body(),
                )
                .with_color(muted.into())
                .finish(),
            )
        };

        let mut label_col = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_child(alias_text);
        if let Some(s) = subtitle {
            label_col.add_child(s);
        }
        let label_block = label_col.with_main_axis_size(MainAxisSize::Min).finish();

        // The trailing "+" button or "Added" badge.
        let add_state = self
            .candidate_add_states
            .get(alias)
            .cloned()
            .unwrap_or_default();
        let alias_for_click = alias.to_string();
        let trailing: Box<dyn Element> = if added {
            // PRODUCT.md decision E: already imported → show "Added" (no click interaction).
            Text::new_inline(
                crate::t!("workspace-left-panel-ssh-manager-candidates-added"),
                appearance.ui_font_family(),
                appearance.ui_font_body(),
            )
            .with_color(muted.into())
            .finish()
        } else {
            let plus_icon = ConstrainedBox::new(
                crate::ui_components::icons::Icon::Plus
                    .to_warpui_icon(theme.sub_text_color(theme.background()))
                    .finish(),
            )
            .with_width(ITEM_ICON_SIZE)
            .with_height(ITEM_ICON_SIZE)
            .finish();
            Hoverable::new(add_state, move |_| {
                Container::new(plus_icon)
                    .with_uniform_padding(2.0)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.0)))
                    .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshManagerPanelAction::ImportCandidate {
                    alias: alias_for_click.clone(),
                });
            })
            .finish()
        };

        // Uses MainAxisSize::Max to make the candidate row fill the panel width, eliminating trailing whitespace.
        let row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(ITEM_ICON_TEXT_SPACING)
            .with_child(
                ConstrainedBox::new(Empty::new().finish())
                    .with_width(FOLDER_DEPTH_INDENT)
                    .finish(),
            )
            .with_child(icon_el)
            .with_child(label_block)
            .with_child(
                ConstrainedBox::new(Empty::new().finish())
                    .with_width(8.0)
                    .finish(),
            )
            .with_child(trailing)
            .with_main_axis_size(MainAxisSize::Max)
            .finish();

        let row_state = self
            .candidate_row_states
            .get(alias)
            .cloned()
            .unwrap_or_default();
        Hoverable::new(row_state, move |mouse| {
            let mut c = Container::new(row)
                .with_padding_top(ITEM_PADDING_VERTICAL)
                .with_padding_bottom(ITEM_PADDING_VERTICAL)
                .with_padding_left(ITEM_PADDING_HORIZONTAL)
                .with_padding_right(ITEM_PADDING_HORIZONTAL)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)));
            if mouse.is_hovered() {
                c = c.with_background(internal_colors::fg_overlay_3(theme));
            }
            c.finish()
        })
        .finish()
    }

    fn render_tree(&self, appearance: &warp_core::ui::appearance::Appearance) -> Box<dyn Element> {
        let mut col = Flex::column();

        if self.nodes.is_empty() {
            let theme = appearance.theme();
            let muted = theme.sub_text_color(theme.background());
            col.add_child(
                Container::new(
                    Text::new_inline(
                        crate::t!("workspace-left-panel-ssh-manager-tree-empty"),
                        appearance.ui_font_family(),
                        appearance.ui_font_subheading(),
                    )
                    .with_color(muted.into())
                    .finish(),
                )
                .with_padding_top(20.0)
                .with_padding_bottom(20.0)
                .with_padding_left(ITEM_PADDING_HORIZONTAL)
                .with_padding_right(ITEM_PADDING_HORIZONTAL)
                .finish(),
            );
        } else {
            for node in &self.nodes {
                if !self.is_visible(node) {
                    continue;
                }
                col.add_child(self.render_row(node, appearance));
            }
        }
        let inner = col
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Min)
            .finish();
        // Right-clicking a blank area = OpenContextMenu with a None node target.
        let hoverable = Hoverable::new(MouseStateHandle::default(), move |_| inner)
            .on_right_click(|ctx, _, position| {
                let offset = match ctx.element_position_by_id(SSH_PANEL_POSITION_ID) {
                    Some(bounds) => position - bounds.origin(),
                    None => position,
                };
                ctx.dispatch_typed_action(SshManagerPanelAction::OpenContextMenu {
                    target: None,
                    position: offset,
                });
            })
            .finish();
        // The entire tree area is also a drop target, with parent_id=None
        // meaning dropped onto root. Row-level DropTargets have higher
        // priority (lower number), so dropping onto a folder still goes into that folder.
        DropTarget::new(hoverable, SshDropData { parent_id: None }).finish()
    }

    fn render_row(
        &self,
        node: &SshNode,
        appearance: &warp_core::ui::appearance::Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let depth = self.depths.get(&node.id).copied().unwrap_or(0);
        let is_selected = self.selected_id.as_deref() == Some(node.id.as_str());
        let is_renaming = self
            .rename_state
            .as_ref()
            .map(|rs| rs.node_id == node.id)
            .unwrap_or(false);

        let icon = match node.kind {
            NodeKind::Folder => crate::ui_components::icons::Icon::Folder,
            NodeKind::Server => crate::ui_components::icons::Icon::Key,
        };
        let icon_color = theme.sub_text_color(theme.background());
        let icon_el = ConstrainedBox::new(icon.to_warpui_icon(icon_color).finish())
            .with_width(ITEM_ICON_SIZE)
            .with_height(ITEM_ICON_SIZE)
            .finish();

        // Folder rows get a chevron prefix (▼ expanded / ▶ collapsed);
        // Server rows use equal-width blank space as a placeholder so all
        // rows' icons stay aligned.
        let chevron_el: Box<dyn Element> = match node.kind {
            NodeKind::Folder => {
                let chevron_icon = if node.is_collapsed {
                    crate::ui_components::icons::Icon::ChevronRight
                } else {
                    crate::ui_components::icons::Icon::ChevronDown
                };
                ConstrainedBox::new(chevron_icon.to_warpui_icon(icon_color).finish())
                    .with_width(ITEM_ICON_SIZE)
                    .with_height(ITEM_ICON_SIZE)
                    .finish()
            }
            NodeKind::Server => ConstrainedBox::new(Empty::new().finish())
                .with_width(ITEM_ICON_SIZE)
                .finish(),
        };

        // Right half — text or the rename input box.
        // EditorView must be rendered inside a finite-width container,
        // otherwise element.rs:1670 will panic("infinite width constraint
        // on buffer elements"). A Flex::row child has no column-stretch
        // semantics, so it's wrapped here in a ConstrainedBox with a fixed width.
        let label_or_editor: Box<dyn Element> = if is_renaming {
            let editor_handle = self
                .rename_state
                .as_ref()
                .map(|rs| rs.editor.clone())
                .expect("is_renaming implies rename_state.is_some");
            let input = appearance
                .ui_builder()
                .text_input(editor_handle)
                .with_style(UiComponentStyles {
                    padding: Some(Coords {
                        left: 4.0,
                        right: 4.0,
                        top: 1.0,
                        bottom: 1.0,
                    }),
                    background: Some(theme.surface_2().into()),
                    border_color: Some(theme.accent().into()),
                    border_width: Some(1.0),
                    border_radius: Some(CornerRadius::with_all(Radius::Pixels(3.0))),
                    font_size: Some(appearance.ui_font_subheading()),
                    ..Default::default()
                })
                .build()
                .finish();
            ConstrainedBox::new(input).with_width(180.0).finish()
        } else {
            Text::new_inline(
                node.name.clone(),
                appearance.ui_font_family(),
                appearance.ui_font_subheading(),
            )
            .with_color(theme.main_text_color(theme.background()).into())
            .finish()
        };

        // Uses MainAxisSize::Max to make the tree node row fill the panel width, eliminating trailing whitespace.
        let row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(ITEM_ICON_TEXT_SPACING)
            .with_child(
                ConstrainedBox::new(Empty::new().finish())
                    .with_width(depth as f32 * FOLDER_DEPTH_INDENT)
                    .finish(),
            )
            .with_child(chevron_el)
            .with_child(icon_el)
            .with_child(warpui::elements::Shrinkable::new(1.0, label_or_editor).finish())
            .with_main_axis_size(MainAxisSize::Max)
            .finish();

        let state = self.row_states.get(&node.id).cloned().unwrap_or_default();
        let id_for_click = node.id.clone();
        let id_for_double_click = node.id.clone();
        let id_for_right_click = node.id.clone();

        // While renaming, don't intercept clicks/right-clicks (leave them to EditorView).
        if is_renaming {
            return Container::new(row)
                .with_padding_top(ITEM_PADDING_VERTICAL)
                .with_padding_bottom(ITEM_PADDING_VERTICAL)
                .with_padding_left(ITEM_PADDING_HORIZONTAL)
                .with_padding_right(ITEM_PADDING_HORIZONTAL)
                .with_margin_bottom(ITEM_MARGIN_BOTTOM)
                .finish();
        }

        let hoverable = Hoverable::new(state, move |_| {
            let mut c = Container::new(row)
                .with_padding_top(ITEM_PADDING_VERTICAL)
                .with_padding_bottom(ITEM_PADDING_VERTICAL)
                .with_padding_left(ITEM_PADDING_HORIZONTAL)
                .with_padding_right(ITEM_PADDING_HORIZONTAL)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)));
            if is_selected {
                c = c.with_background(internal_colors::fg_overlay_3(theme));
            }
            c.finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(SshManagerPanelAction::Click(id_for_click.clone()));
        })
        .on_double_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(SshManagerPanelAction::DoubleClick(
                id_for_double_click.clone(),
            ));
        })
        .on_right_click(move |ctx, _, position| {
            let offset = match ctx.element_position_by_id(SSH_PANEL_POSITION_ID) {
                Some(bounds) => position - bounds.origin(),
                None => position,
            };
            ctx.dispatch_typed_action(SshManagerPanelAction::OpenContextMenu {
                target: Some(id_for_right_click.clone()),
                position: offset,
            });
        })
        .finish();

        // Wraps the row into an element that's "both draggable and drop-accepting".
        //
        // **Critical nesting**: `DropTarget(Container(Draggable(Hoverable)))`.
        // Without the Container layer there's a bug — `Draggable::origin()`
        // returns `child.origin()`
        // (`crates/warpui_core/src/elements/drag/draggable.rs:746-757`), and
        // while the child is in the Dragging state it's painted at
        // drag_origin, making `child.origin()` = the ghost's position. As a
        // result, if DropTarget wraps Draggable directly, its bounds follow
        // the ghost → the drop target is always right under the mouse and
        // can never land on another row. Container.origin/size locks the
        // layout value in its own paint (`container.rs:288 self.origin =
        // ...`), giving DropTarget stable bounds.
        let drag_state = self
            .row_drag_states
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        let dragged_id = node.id.clone();
        let draggable = Draggable::new(drag_state, hoverable)
            .with_accepted_by_drop_target_fn(move |drop_data, _app| {
                if drop_data.as_any().downcast_ref::<SshDropData>().is_some() {
                    AcceptedByDropTarget::Yes
                } else {
                    AcceptedByDropTarget::No
                }
            })
            .on_drop(move |ctx, _app, _bounds, data| {
                if let Some(drop) = data.and_then(|d| d.as_any().downcast_ref::<SshDropData>()) {
                    ctx.dispatch_typed_action(SshManagerPanelAction::MoveNode {
                        node_id: dragged_id.clone(),
                        new_parent_id: drop.parent_id.clone(),
                    });
                }
            })
            .finish();

        // The middle Container that locks the layout origin — see the comment above.
        let stable_anchor = Container::new(draggable).finish();

        let drop_parent_id = match node.kind {
            NodeKind::Folder => Some(node.id.clone()),
            NodeKind::Server => node.parent_id.clone(),
        };
        DropTarget::new(
            stable_anchor,
            SshDropData {
                parent_id: drop_parent_id,
            },
        )
        .finish()
    }

    fn context_menu_items(&self) -> Vec<(String, SshManagerPanelAction)> {
        match self.context_menu_target.as_ref() {
            None => vec![
                (
                    crate::t!("workspace-left-panel-ssh-manager-menu-new-folder"),
                    SshManagerPanelAction::AddFolder,
                ),
                (
                    crate::t!("workspace-left-panel-ssh-manager-menu-new-server"),
                    SshManagerPanelAction::AddServer,
                ),
            ],
            Some(id) => {
                let kind = self.nodes.iter().find(|n| &n.id == id).map(|n| n.kind);
                match kind {
                    Some(NodeKind::Folder) => vec![
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-new-folder"),
                            SshManagerPanelAction::AddFolder,
                        ),
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-new-server"),
                            SshManagerPanelAction::AddServer,
                        ),
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-rename"),
                            SshManagerPanelAction::StartRename(id.clone()),
                        ),
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-delete"),
                            SshManagerPanelAction::DeleteSelected,
                        ),
                    ],
                    Some(NodeKind::Server) => vec![
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-edit"),
                            SshManagerPanelAction::Edit,
                        ),
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-connect"),
                            SshManagerPanelAction::Connect,
                        ),
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-sftp"),
                            SshManagerPanelAction::OpenSftp,
                        ),
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-clone"),
                            SshManagerPanelAction::CloneServer(id.clone()),
                        ),
                        (
                            crate::t!("workspace-left-panel-ssh-manager-menu-delete"),
                            SshManagerPanelAction::DeleteSelected,
                        ),
                    ],
                    None => vec![],
                }
            }
        }
    }

    fn render_context_menu(
        &self,
        appearance: &warp_core::ui::appearance::Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let items = self.context_menu_items();
        let mut col = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
        for (i, (label, action)) in items.into_iter().enumerate() {
            let state = self
                .context_menu_item_states
                .get(i)
                .cloned()
                .unwrap_or_default();
            let label_el = Text::new_inline(
                label,
                appearance.ui_font_family(),
                appearance.ui_font_subheading(),
            )
            .with_color(theme.main_text_color(theme.background()).into())
            .finish();
            let row_action = action.clone();
            let item = Hoverable::new(state, move |mouse| {
                let mut c = Container::new(label_el)
                    .with_padding_top(CONTEXT_MENU_ITEM_PADDING_V)
                    .with_padding_bottom(CONTEXT_MENU_ITEM_PADDING_V)
                    .with_padding_left(CONTEXT_MENU_ITEM_PADDING_H)
                    .with_padding_right(CONTEXT_MENU_ITEM_PADDING_H)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.0)));
                if mouse.is_hovered() {
                    c = c.with_background(internal_colors::fg_overlay_3(theme));
                }
                c.finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(row_action.clone());
                ctx.dispatch_typed_action(SshManagerPanelAction::DismissContextMenu);
            })
            .finish();
            col.add_child(item);
        }
        let menu_inner = ConstrainedBox::new(
            Container::new(col.with_main_axis_size(MainAxisSize::Min).finish())
                .with_background(theme.surface_2())
                .with_border(Border::all(1.0).with_border_color(theme.surface_3().into()))
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.0)))
                .with_uniform_padding(4.0)
                .finish(),
        )
        .with_width(CONTEXT_MENU_WIDTH)
        .finish();

        Dismiss::new(menu_inner)
            .on_dismiss(|ctx, _| {
                ctx.dispatch_typed_action(SshManagerPanelAction::DismissContextMenu);
            })
            .finish()
    }
}

impl Entity for SshManagerPanel {
    type Event = SshManagerPanelEvent;
}

impl TypedActionView for SshManagerPanel {
    type Action = SshManagerPanelAction;

    fn handle_action(&mut self, action: &SshManagerPanelAction, ctx: &mut ViewContext<Self>) {
        match action {
            SshManagerPanelAction::AddRootFolder => self.on_add_folder_with_parent(None, ctx),
            SshManagerPanelAction::AddFolder => {
                let parent = self.parent_for_new_node();
                self.on_add_folder_with_parent(parent, ctx)
            }
            SshManagerPanelAction::AddServer => self.on_add_server(ctx),
            SshManagerPanelAction::DeleteSelected => self.on_delete_selected(ctx),
            SshManagerPanelAction::Connect => self.on_connect(ctx),
            SshManagerPanelAction::Edit => self.on_edit(ctx),
            SshManagerPanelAction::CloneServer(id) => self.on_clone_server(id, ctx),
            SshManagerPanelAction::Click(id) => self.on_click(id.clone(), ctx),
            SshManagerPanelAction::StartRename(id) => self.enter_rename(id.clone(), false, ctx),
            SshManagerPanelAction::CommitRename => self.commit_rename(ctx),
            SshManagerPanelAction::CancelRename => self.cancel_rename(ctx),
            SshManagerPanelAction::OpenContextMenu { target, position } => {
                self.on_open_context_menu(target.clone(), *position, ctx)
            }
            SshManagerPanelAction::DismissContextMenu => self.on_dismiss_context_menu(ctx),
            SshManagerPanelAction::MoveNode {
                node_id,
                new_parent_id,
            } => self.on_move_node(node_id.clone(), new_parent_id.clone(), ctx),
            SshManagerPanelAction::ToggleNodeCollapsed(id) => {
                self.on_toggle_node_collapsed(id, ctx)
            }
            SshManagerPanelAction::ToggleAllFolders => self.on_toggle_all_folders(ctx),
            SshManagerPanelAction::DoubleClick(id) => self.on_double_click(id.clone(), ctx),
            SshManagerPanelAction::OpenSftp => self.on_open_sftp(ctx),
            SshManagerPanelAction::ImportCandidate { alias } => {
                self.on_import_candidate(alias.clone(), ctx)
            }
            SshManagerPanelAction::RefreshCandidates => {
                self.candidates.update(ctx, |vm, ctx| vm.refresh(ctx));
                self.sync_candidate_row_states(ctx);
                ctx.notify();
            }
            SshManagerPanelAction::ToggleCandidatesSection => {
                self.candidates
                    .update(ctx, |vm, ctx| vm.toggle_expanded(ctx));
                ctx.notify();
            }
        }
    }
}

impl View for SshManagerPanel {
    fn ui_name() -> &'static str {
        "SshManagerPanel"
    }

    fn on_focus(&mut self, _focus_ctx: &FocusContext, _ctx: &mut ViewContext<Self>) {}

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = warp_core::ui::appearance::Appearance::as_ref(app);

        let toolbar = Container::new(self.render_toolbar(appearance))
            .with_uniform_padding(8.0)
            .finish();

        // PRODUCT.md §2: the Candidates section is **above** the saved
        // tree, sharing the same panel horizontal padding. The section
        // returns Empty when the view-model hasn't been refreshed yet,
        // taking up no height. The section isn't rendered when
        // auto-discovery is disabled.
        let auto_discover = *SshSettings::as_ref(app).enable_ssh_auto_discovery.value();
        let candidates_section = if auto_discover {
            Container::new(self.render_candidates(appearance, app))
                .with_padding_left(PANEL_HORIZONTAL_PADDING - ITEM_PADDING_HORIZONTAL)
                .with_padding_right(PANEL_HORIZONTAL_PADDING - ITEM_PADDING_HORIZONTAL)
                .finish()
        } else {
            Empty::new().finish()
        };

        let tree = Container::new(self.render_tree(appearance))
            .with_padding_left(PANEL_HORIZONTAL_PADDING - ITEM_PADDING_HORIZONTAL)
            .with_padding_right(PANEL_HORIZONTAL_PADDING - ITEM_PADDING_HORIZONTAL)
            .finish();

        let root_content = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(candidates_section)
            .with_child(tree)
            .finish();

        let scrollable_content = ClippedScrollable::vertical_centered(
            self.content_scroll_state.clone(),
            root_content,
            ScrollbarWidth::Custom(4.0),
            appearance.theme().nonactive_ui_detail().into(),
            appearance.theme().active_ui_detail().into(),
            ElementFill::None,
        )
        .with_overlayed_scrollbar()
        .finish();

        let panel_content = Container::new(
            Flex::column()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(toolbar)
                .with_child(warpui::elements::Shrinkable::new(1.0, scrollable_content).finish())
                .finish(),
        )
        .finish();

        let positioned_panel = SavePosition::new(panel_content, SSH_PANEL_POSITION_ID).finish();

        let Some(position) = self.context_menu_position else {
            return positioned_panel;
        };

        let menu_el = self.render_context_menu(appearance);
        let positioning = OffsetPositioning::offset_from_parent(
            position,
            ParentOffsetBounds::ParentByPosition,
            ParentAnchor::TopLeft,
            ChildAnchor::TopLeft,
        );

        let mut stack = Stack::new();
        stack.add_child(positioned_panel);
        stack.add_positioned_overlay_child(menu_el, positioning);
        stack.finish()
    }
}

// --- helpers --------------------------------------------------------------

/// Computes the parent ID for a new node based on the currently selected
/// item and the node list.
/// - A folder selected → created as a child under that folder
/// - A server selected → created as a sibling (inherits the server's parent)
/// - Nothing selected → created at the root level (returns None)
fn resolve_parent_for_new_node(selected_id: Option<&str>, nodes: &[SshNode]) -> Option<String> {
    let id = selected_id?;
    let node = nodes.iter().find(|n| n.id == id)?;
    match node.kind {
        NodeKind::Folder => Some(node.id.clone()),
        NodeKind::Server => node.parent_id.clone(),
    }
}

fn sort_for_display(nodes: Vec<SshNode>, depths: &HashMap<String, usize>) -> Vec<SshNode> {
    use std::collections::{BTreeMap, HashSet};
    let ids: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let mut by_parent: BTreeMap<Option<String>, Vec<SshNode>> = BTreeMap::new();
    for n in nodes {
        by_parent.entry(n.parent_id.clone()).or_default().push(n);
    }
    for v in by_parent.values_mut() {
        v.sort_by_key(|n| (n.sort_order, n.name.clone()));
    }
    let mut out = Vec::with_capacity(depths.len());
    fn walk(
        parent: Option<&String>,
        by_parent: &BTreeMap<Option<String>, Vec<SshNode>>,
        out: &mut Vec<SshNode>,
        seen: &mut HashSet<String>,
    ) {
        if let Some(children) = by_parent.get(&parent.cloned()) {
            for c in children {
                if !seen.insert(c.id.clone()) {
                    continue;
                }
                out.push(c.clone());
                walk(Some(&c.id), by_parent, out, seen);
            }
        }
    }
    let root_parents: Vec<Option<String>> = by_parent
        .keys()
        .filter(|parent| parent.as_ref().is_none_or(|id| !ids.contains(id)))
        .cloned()
        .collect();
    let mut seen = HashSet::new();
    for parent in root_parents {
        walk(parent.as_ref(), &by_parent, &mut out, &mut seen);
    }
    for children in by_parent.values() {
        for child in children {
            if !seen.insert(child.id.clone()) {
                continue;
            }
            out.push(child.clone());
            walk(Some(&child.id), &by_parent, &mut out, &mut seen);
        }
    }
    out
}

fn compute_depths(nodes: &[SshNode]) -> HashMap<String, usize> {
    let by_id: HashMap<&str, &SshNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut depths = HashMap::with_capacity(nodes.len());
    for n in nodes {
        let mut d = 0;
        let mut p = n.parent_id.as_deref();
        while let Some(pid) = p {
            let Some(parent) = by_id.get(pid) else {
                break;
            };
            d += 1;
            p = parent.parent_id.as_deref();
            if d > 64 {
                break;
            }
        }
        depths.insert(n.id.clone(), d);
    }
    depths
}

/// Fetches the `host` field of every ssh_servers row in one go. Returns an
/// empty Vec on failure — if SQLite temporarily goes down, the candidates
/// section's "Added" badge is rendered as if "nothing has been imported",
/// rather than crashing the whole panel.
fn list_server_hosts() -> Vec<String> {
    use diesel::prelude::*;
    use persistence::schema::ssh_servers;
    warp_ssh_manager::with_conn(|conn| {
        let hosts: Vec<String> = ssh_servers::table.select(ssh_servers::host).load(conn)?;
        Ok(hosts)
    })
    .unwrap_or_else(|e| {
        log::warn!("ssh_manager: failed to list server hosts for candidates: {e:?}");
        Vec::new()
    })
}

fn unique_name(
    conn: &mut diesel::sqlite::SqliteConnection,
    parent: Option<&str>,
    base: &str,
) -> Result<String, anyhow::Error> {
    use diesel::prelude::*;
    use persistence::schema::ssh_nodes;
    let existing: Vec<String> = match parent {
        Some(p) => ssh_nodes::table
            .filter(ssh_nodes::parent_id.eq(p))
            .select(ssh_nodes::name)
            .load(conn)?,
        None => ssh_nodes::table
            .filter(ssh_nodes::parent_id.is_null())
            .select(ssh_nodes::name)
            .load(conn)?,
    };
    let set: std::collections::HashSet<String> = existing.into_iter().collect();
    if !set.contains(base) {
        return Ok(base.to_string());
    }
    for i in 2..1000 {
        let cand = format!("{base} {i}");
        if !set.contains(&cand) {
            return Ok(cand);
        }
    }
    Ok(format!("{base} {}", uuid::Uuid::new_v4()))
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
