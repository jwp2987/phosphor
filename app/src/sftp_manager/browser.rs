//! SFTP browser main view
//!
//! Implements the BackingView trait, serving as the pane's core view component.
//! Provides complete remote file browsing, upload/download, directory
//! navigation, and related functionality.
//! author: logic
//! date: 2026-05-26

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use pathfinder_geometry::vector::Vector2F;
use warp_core::ui::appearance::Appearance;
use warp_core::ui::icons::Icon;
use warp_ssh_manager::{KeychainSecretStore, SshRepository};
use warpui::elements::{
    Align, Border, ChildAnchor, ChildView, ClippedScrollStateHandle, ClippedScrollable,
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, DispatchEventResult, Element,
    EventHandler, Fill, Flex, Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle,
    OffsetPositioning, ParentAnchor, ParentElement, ParentOffsetBounds, Radius, SavePosition,
    ScrollbarWidth, Shrinkable, Stack, Text,
};
use warpui::platform::{Cursor, FilePickerConfiguration, SaveFilePickerConfiguration};
use warpui::{
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle,
};
use warpui::r#async::SpawnedFutureHandle;

use crate::editor::{
    EditorView, Event as EditorEvent, SingleLineEditorOptions, TextColors, TextOptions,
};
use crate::pane_group::focus_state::PaneFocusHandle;
use crate::pane_group::pane::view;
use crate::pane_group::{BackingView, PaneConfiguration, PaneEvent};
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

use super::context_menu::ContextMenuState;
use super::sftp_backend::{LiveSftpBackend, SftpBackend};
use super::sftp_ops;
use super::sftp_ops::normalize_remote_path;
use super::types::{
    ConnectionState, Dialog, FileEntry, FileEntryType, TransferDirection, TransferState,
    TransferTask,
};

/// Toolbar button size
const TOOLBAR_BTN_SIZE: f32 = 28.0;
/// Toolbar icon size
const TOOLBAR_ICON_SIZE: f32 = 16.0;
/// Toolbar spacing
const TOOLBAR_SPACING: f32 = 4.0;
/// Panel padding
const PANEL_PADDING: f32 = 8.0;
/// SFTP panel position ID (used for SavePosition to locate the context menu)
pub(crate) const SFTP_PANEL_POSITION_ID: &str = "sftp_browser_panel_root";

/// SFTP browser action
#[derive(Debug, Clone)]
pub enum SftpBrowserAction {
    /// Navigate to the given path
    NavigateTo(PathBuf),
    /// Go to the parent directory
    GoUp,
    /// Go back (history)
    GoBack,
    /// Go forward (history)
    GoForward,
    /// Refresh the current directory
    Refresh,
    /// Select the entry at the given index
    SelectEntry(usize),
    /// Open the entry at the given index (enter if a directory, download if a file)
    OpenEntry(usize),
    /// Delete the entry at the given index
    DeleteEntry(usize),
    /// Rename the entry at the given index
    RenameEntry(usize),
    /// Download the entry at the given index
    DownloadEntry(usize),
    /// Upload a file
    UploadFile,
    /// Create a new folder
    NewFolder,
    /// Confirm delete
    ConfirmDelete,
    /// Confirm rename
    ConfirmRename,
    /// Confirm new folder
    ConfirmNewFolder,
    /// Confirm overwrite
    ConfirmOverwrite,
    /// Show the context menu
    ContextMenu {
        index: usize,
        position: Vector2F,
    },
    /// Close the context menu
    CloseContextMenu,
    /// Close the dialog
    CloseDialog,
    /// View entry details
    DetailsEntry(usize),
    /// Set the search filter
    SetSearchFilter(String),
    /// Clear the search filter
    ClearSearchFilter,
    /// Go to parent (keyboard shortcut)
    NavigateUp,
    /// Delete the selected entry (keyboard shortcut)
    DeleteSelected,
    /// Create a folder (keyboard shortcut)
    CreateFolder,
    /// Files dragged into the browser area
    DragFilesEnter,
    /// Files dragged out of the browser area
    DragFilesLeave,
    /// Drag-and-drop file upload
    DragAndDropFiles(Vec<PathBuf>),
    /// Execute the upload
    ExecuteUpload(String),
    /// Execute save-as download (user has already chosen a path)
    DownloadSaveAs { index: usize, local_path: String },
    /// Confirm move
    ConfirmMove,
    /// Cancel a transfer task
    CancelTransfer(usize),
    /// Toggle transfer panel visibility
    ToggleTransferPanel,
    /// Confirm closing the transfer panel (cancels all transfers and clears the records)
    ConfirmCloseTransferPanel,
}

/// SFTP browser view
pub struct SftpBrowserView {
    /// Associated SSH server node ID
    node_id: String,
    /// Pane configuration handle
    pane_configuration: ModelHandle<PaneConfiguration>,
    /// Focus handle
    focus_handle: Option<PaneFocusHandle>,
    // ---- Connection ----
    /// Connection state
    pub(crate) connection: ConnectionState,
    /// SFTP session
    _session: Option<zap_sftp::SftpSession>,
    /// SFTP operation channel
    sftp: Option<Arc<dyn SftpBackend>>,
    // ---- Navigation ----
    /// Current path
    pub(crate) current_path: PathBuf,
    /// File entries in the current directory
    pub(crate) entries: Vec<FileEntry>,
    /// Set of selected entry indices
    pub(crate) selected: HashSet<usize>,
    /// Path history
    pub(crate) path_history: Vec<PathBuf>,
    /// Current position in the history
    pub(crate) history_index: usize,
    // ---- Transfers ----
    /// List of transfer tasks
    pub(crate) transfers: Vec<TransferTask>,
    /// Next transfer task ID
    pub(crate) next_transfer_id: usize,
    // ---- UI state ----
    /// Currently open dialog
    pub(crate) dialog: Option<Dialog>,
    /// Whether it's currently loading
    pub(crate) is_loading: bool,
    /// Context menu state
    pub(crate) context_menu: Option<ContextMenuState>,
    /// Search filter text
    pub(crate) search_filter: Option<String>,
    /// Whether a file drag is hovering over the browser
    pub(crate) is_drag_hovering: bool,
    // ---- Mouse handles ----
    /// Refresh button
    refresh_btn: MouseStateHandle,
    /// Parent-directory button
    up_btn: MouseStateHandle,
    /// Back button
    back_btn: MouseStateHandle,
    /// Forward button
    forward_btn: MouseStateHandle,
    /// Upload button
    upload_btn: MouseStateHandle,
    /// New-folder button
    new_folder_btn: MouseStateHandle,
    /// Dialog confirm button
    dialog_confirm_btn: MouseStateHandle,
    /// Dialog cancel button
    dialog_cancel_btn: MouseStateHandle,
    /// Dialog close button (title bar X button)
    dialog_close_btn: MouseStateHandle,
    // ---- Transfer panel ----
    /// Whether the transfer panel has been hidden by the user
    transfer_panel_hidden: bool,
    /// Transfer panel close button
    transfer_panel_close_btn: MouseStateHandle,
    // ---- Dialog editors ----
    /// Rename editor
    pub(crate) rename_editor: ViewHandle<EditorView>,
    /// New-folder editor
    pub(crate) new_folder_editor: ViewHandle<EditorView>,
    /// Search filter editor
    search_editor: ViewHandle<EditorView>,
    // ---- File row mouse handles ----
    /// Mouse state handle for each file entry row
    row_mouse_handles: Vec<MouseStateHandle>,
    // ---- Scrolling ----
    /// Scroll state handle
    scroll_state: ClippedScrollStateHandle,
    // ---- Async tasks ----
    /// Future handle for the current connection task
    connect_handle: Option<SpawnedFutureHandle>,
    /// Future handle for the current directory refresh
    refresh_handle: Option<SpawnedFutureHandle>,
    /// Mapping from transfer task ID to future handle
    transfer_handles: HashMap<usize, SpawnedFutureHandle>,
    /// Pending queue for drag-and-drop batch uploads
    pending_uploads: Vec<PathBuf>,
}

impl SftpBrowserView {
    /// Creates a new SFTP browser view
    pub fn new(node_id: String, ctx: &mut ViewContext<Self>) -> Self {
        let pane_configuration = ctx.add_model(|_ctx| PaneConfiguration::new("File Manager"));
        let rename_editor = make_editor("Enter new name", ctx);
        let new_folder_editor = make_editor("Folder name", ctx);
        let search_editor = make_editor("Search files...", ctx);

        let mut me = Self {
            node_id,
            pane_configuration,
            focus_handle: None,
            connection: ConnectionState::Disconnected,
            _session: None,
            sftp: None,
            current_path: PathBuf::from("/"),
            entries: Vec::new(),
            selected: HashSet::new(),
            path_history: vec![PathBuf::from("/")],
            history_index: 0,
            transfers: Vec::new(),
            next_transfer_id: 1,
            dialog: None,
            is_loading: false,
            context_menu: None,
            search_filter: None,
            is_drag_hovering: false,
            refresh_btn: MouseStateHandle::default(),
            up_btn: MouseStateHandle::default(),
            back_btn: MouseStateHandle::default(),
            forward_btn: MouseStateHandle::default(),
            upload_btn: MouseStateHandle::default(),
            new_folder_btn: MouseStateHandle::default(),
            dialog_confirm_btn: MouseStateHandle::default(),
            dialog_cancel_btn: MouseStateHandle::default(),
            dialog_close_btn: MouseStateHandle::default(),
            transfer_panel_hidden: false,
            transfer_panel_close_btn: MouseStateHandle::default(),
            rename_editor,
            new_folder_editor,
            search_editor,
            row_mouse_handles: Vec::new(),
            scroll_state: ClippedScrollStateHandle::default(),
            connect_handle: None,
            refresh_handle: None,
            transfer_handles: HashMap::new(),
            pending_uploads: Vec::new(),
        };

        // Subscribe to rename editor events
        let rename_editor_handle = me.rename_editor.clone();
        ctx.subscribe_to_view(
            &rename_editor_handle,
            |me, _source, event, ctx| match event {
                EditorEvent::Enter => {
                    me.handle_action(&SftpBrowserAction::ConfirmRename, ctx);
                }
                EditorEvent::Escape => {
                    me.dialog = None;
                    ctx.notify();
                }
                _ => {}
            },
        );

        // Subscribe to new-folder editor events
        let new_folder_editor_handle = me.new_folder_editor.clone();
        ctx.subscribe_to_view(
            &new_folder_editor_handle,
            |me, _source, event, ctx| match event {
                EditorEvent::Enter => {
                    me.handle_action(&SftpBrowserAction::ConfirmNewFolder, ctx);
                }
                EditorEvent::Escape => {
                    me.dialog = None;
                    ctx.notify();
                }
                _ => {}
            },
        );

        // Subscribe to search editor events
        let search_editor_handle = me.search_editor.clone();
        ctx.subscribe_to_view(
            &search_editor_handle,
            |me, _source, event, ctx| match event {
                EditorEvent::Escape => {
                    me.search_filter = None;
                    me.search_editor
                        .update(ctx, |e, ctx| e.set_buffer_text("", ctx));
                    ctx.notify();
                }
                _ => {
                    let text = me.search_editor.as_ref(ctx).buffer_text(ctx);
                    let trimmed = text.trim().to_string();
                    if trimmed.is_empty() {
                        me.search_filter = None;
                    } else {
                        me.search_filter = Some(trimmed);
                    }
                    ctx.notify();
                }
            },
        );

        // Initiate the connection
        me.connect_to_server(ctx);

        me
    }

    /// Injects a test backend, simulating the Connected state (test-only)
    #[cfg(test)]
    pub(crate) fn set_backend_for_test(
        &mut self,
        backend: Arc<dyn SftpBackend>,
        start_path: PathBuf,
        ctx: &mut ViewContext<Self>,
    ) {
        self.connection = ConnectionState::Connected;
        self.sftp = Some(backend);
        self.current_path = start_path.clone();
        self.path_history = vec![start_path];
        self.history_index = 0;
        self.refresh_dir_sync(ctx);
    }

    /// Injects a test backend (used by integration tests)
    #[cfg(feature = "integration_tests")]
    pub fn inject_mock_backend(
        &mut self,
        backend: Arc<dyn SftpBackend>,
        start_path: PathBuf,
        ctx: &mut ViewContext<Self>,
    ) {
        self.connection = ConnectionState::Connected;
        self.sftp = Some(backend);
        self.current_path = start_path.clone();
        self.path_history = vec![start_path];
        self.history_index = 0;
        self.refresh_dir_sync(ctx);
    }

    /// Synchronously refreshes directory contents (test-only, avoids async delay)
    #[cfg(any(test, feature = "integration_tests"))]
    fn refresh_dir_sync(&mut self, ctx: &mut ViewContext<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => return,
        };
        let path = self.current_path.clone();
        match sftp.list_dir(&path) {
            Ok(mut entries) => {
                entries.sort_by(|a, b| match (a.file_type, b.file_type) {
                    (FileEntryType::Directory, FileEntryType::Directory) => {
                        a.name.to_lowercase().cmp(&b.name.to_lowercase())
                    }
                    (
                        FileEntryType::Directory,
                        FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                    ) => std::cmp::Ordering::Less,
                    (
                        FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                        FileEntryType::Directory,
                    ) => std::cmp::Ordering::Greater,
                    (
                        FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                        FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                    ) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                });
                self.entries = entries;
                self.selected.clear();
                self.sync_row_mouse_handles();
            }
            Err(_) => {}
        }
        let path = self.current_path.display();
        let title = format!("SFTP: {path}");
        self.pane_configuration.update(ctx, |config, ctx| {
            config.set_title(title, ctx);
        });
        ctx.notify();
    }

    /// Integration-test getter: connection state
    #[cfg(feature = "integration_tests")]
    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection
    }

    /// Integration-test getter: file entry list
    #[cfg(feature = "integration_tests")]
    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// Integration-test getter: selected set
    #[cfg(feature = "integration_tests")]
    pub fn selected(&self) -> &HashSet<usize> {
        &self.selected
    }

    /// Integration-test getter: dialog state
    #[cfg(feature = "integration_tests")]
    pub fn dialog(&self) -> &Option<Dialog> {
        &self.dialog
    }

    /// Integration-test getter: context menu state
    #[cfg(feature = "integration_tests")]
    pub fn context_menu(&self) -> &Option<ContextMenuState> {
        &self.context_menu
    }

    /// Gets the pane configuration
    pub fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    /// Test-only: disconnects and clears state
    #[cfg(test)]
    pub(crate) fn disconnect_for_test(&mut self, ctx: &mut ViewContext<Self>) {
        self.connection = ConnectionState::Disconnected;
        self.sftp = None;
        self.entries.clear();
        self.selected.clear();
        ctx.notify();
    }

    /// Connects to the SSH server and establishes an SFTP channel
    fn connect_to_server(&mut self, ctx: &mut ViewContext<Self>) {
        let node_id = self.node_id.clone();
        let result = warp_ssh_manager::with_conn(|c| {
            let server = SshRepository::get_server(c, &node_id)?;
            Ok(server)
        });

        match result {
            Ok(Some(server)) => {
                // Cancel any previous connection attempt
                if let Some(h) = self.connect_handle.take() {
                    h.abort();
                }

                self.connection = ConnectionState::Connecting;
                self.is_loading = true;
                ctx.notify();

                let secret_store = KeychainSecretStore;
                self.connect_handle = self.run_blocking(
                    ctx,
                    move || sftp_ops::connect_from_server(&server, &secret_store),
                    move |me, result, ctx| {
                        me.is_loading = false;
                        match result {
                            Ok(Ok(session)) => {
                                match session.sftp() {
                                    Ok(sftp) => {
                                        let backend = Arc::new(LiveSftpBackend::new(sftp)) as Arc<dyn SftpBackend>;
                                        // Resolve the user's home directory
                                        if let Ok(home) = backend.realpath(std::path::Path::new(".")) {
                                            me.current_path = normalize_remote_path(&home);
                                        } else {
                                            me.current_path = PathBuf::from("/");
                                        }
                                        me.path_history = vec![me.current_path.clone()];
                                        me.history_index = 0;
                                        me.connection = ConnectionState::Connected;
                                        me._session = Some(session);
                                        me.sftp = Some(backend);
                                        me.refresh_dir(ctx);
                                    }
                                    Err(e) => {
                                        me.connection =
                                            ConnectionState::Failed(format!("Failed to create SFTP channel: {e}"));
                                        me.show_error_toast(format!("Failed to create SFTP channel: {e}"), ctx);
                                    }
                                }
                            }
                            Ok(Err(e)) => {
                                me.connection = ConnectionState::Failed(e.to_string());
                                me.show_error_toast(e.to_string(), ctx);
                            }
                            Err(_) => {
                                // JoinError (aborted or panicked)
                                me.connection = ConnectionState::Failed("Connection cancelled".to_string());
                            }
                        }
                        ctx.notify();
                    },
                );
            }
            Ok(None) => {
                self.connection = ConnectionState::Failed("Server configuration not found".to_string());
                self.show_error_toast("Server configuration not found".to_string(), ctx);
                ctx.notify();
            }
            Err(e) => {
                self.connection = ConnectionState::Failed(format!("Failed to read server configuration: {e}"));
                self.show_error_toast(format!("Failed to read server configuration: {e}"), ctx);
                ctx.notify();
            }
        }
    }

    /// Runs a blocking operation and invokes a callback with the result.
    /// Production: runs on a background thread via ctx.spawn + spawn_blocking.
    /// Test: runs directly, synchronously (avoids async executor timing issues).
    /// Returns a SpawnedFutureHandle for cancellation (test environment returns None).
    fn run_blocking<T: Send + 'static>(
        &mut self,
        ctx: &mut ViewContext<Self>,
        op: impl FnOnce() -> T + Send + 'static,
        callback: impl FnOnce(&mut Self, Result<T, tokio::task::JoinError>, &mut ViewContext<Self>) + 'static,
    ) -> Option<SpawnedFutureHandle> {
        #[cfg(any(test, feature = "integration_tests"))]
        {
            let result = op();
            callback(self, Ok(result), ctx);
            None
        }
        #[cfg(not(any(test, feature = "integration_tests")))]
        {
            Some(ctx.spawn(
                async move { tokio::task::spawn_blocking(op).await },
                move |me, result, ctx| {
                    callback(me, result, ctx);
                },
            ))
        }
    }

    /// Refreshes the current directory contents
    fn refresh_dir(&mut self, ctx: &mut ViewContext<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => {
                self.show_error_toast("Not connected to server".to_string(), ctx);
                ctx.notify();
                return;
            }
        };

        self.is_loading = true;
        ctx.notify();

        let path = self.current_path.clone();
        self.refresh_handle = self.run_blocking(
            ctx,
            move || sftp.list_dir(&path),
            |me, result, ctx| {
                me.is_loading = false;
                match result {
                    Ok(Ok(mut entries)) => {
                        entries.sort_by(|a, b| match (a.file_type, b.file_type) {
                            (FileEntryType::Directory, FileEntryType::Directory) => {
                                a.name.to_lowercase().cmp(&b.name.to_lowercase())
                            }
                            (
                                FileEntryType::Directory,
                                FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                            ) => std::cmp::Ordering::Less,
                            (
                                FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                                FileEntryType::Directory,
                            ) => std::cmp::Ordering::Greater,
                            (
                                FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                                FileEntryType::File | FileEntryType::Symlink | FileEntryType::Other,
                            ) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                        });
                        me.entries = entries;
                        me.selected.clear();
                        me.sync_row_mouse_handles();
                    }
                    Ok(Err(e)) => {
                        me.show_error_toast(format!("Failed to list directory: {e}"), ctx);
                    }
                    Err(_) => {}
                }

                let path = me.current_path.display();
                let title = format!("SFTP: {path}");
                me.pane_configuration.update(ctx, |config, ctx| {
                    config.set_title(title, ctx);
                });
                ctx.notify();
            },
        );
    }

    /// Keeps the number of row mouse handles in sync with the number of entries
    fn sync_row_mouse_handles(&mut self) {
        while self.row_mouse_handles.len() < self.entries.len() {
            self.row_mouse_handles.push(MouseStateHandle::default());
        }
        self.row_mouse_handles.truncate(self.entries.len());
    }

    /// Shows an error toast popup
    fn show_error_toast(&self, message: String, ctx: &mut ViewContext<Self>) {
        let window_id = ctx.window_id();
        ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
            let toast = DismissibleToast::error(message)
                .with_object_id("sftp_error".to_string());
            toast_stack.add_ephemeral_toast(toast, window_id, ctx);
        });
    }

    /// Navigates to the given path and updates the history
    fn navigate_to(&mut self, path: PathBuf, ctx: &mut ViewContext<Self>) {
        let path = normalize_remote_path(&path);
        if path == self.current_path {
            return;
        }
        self.current_path = path;
        // Truncate the forward history
        self.path_history.truncate(self.history_index + 1);
        self.path_history.push(self.current_path.clone());
        self.history_index = self.path_history.len() - 1;
        self.refresh_dir(ctx);
    }

    /// Goes to the parent directory
    fn go_up(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(parent) = self.current_path.parent() {
            let parent = normalize_remote_path(&parent.to_path_buf());
            if parent != self.current_path {
                self.navigate_to(parent, ctx);
            }
        }
    }

    /// Goes back to the previous path in history
    fn go_back(&mut self, ctx: &mut ViewContext<Self>) {
        if self.history_index > 0 {
            self.history_index -= 1;
            self.current_path = self.path_history[self.history_index].clone();
            self.refresh_dir(ctx);
        }
    }

    /// Goes forward to the next path in history
    fn go_forward(&mut self, ctx: &mut ViewContext<Self>) {
        if self.history_index < self.path_history.len() - 1 {
            self.history_index += 1;
            self.current_path = self.path_history[self.history_index].clone();
            self.refresh_dir(ctx);
        }
    }

    /// Opens the entry at the given index
    fn open_entry(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            match entry.file_type {
                FileEntryType::Directory | FileEntryType::Symlink => {
                    self.navigate_to(entry.path.clone(), ctx);
                }
                FileEntryType::File | FileEntryType::Other => {
                    self.download_entry(index, ctx);
                }
            }
        }
    }

    /// Shows the delete confirmation dialog
    fn delete_selected(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            let (paths, is_dirs) = if self.selected.contains(&index) {
                // Delete everything selected
                self.selected
                    .iter()
                    .filter_map(|&i| {
                        self.entries.get(i).map(|e| {
                            (e.path.clone(), matches!(e.file_type, FileEntryType::Directory))
                        })
                    })
                    .unzip()
            } else {
                (
                    vec![entry.path.clone()],
                    vec![matches!(entry.file_type, FileEntryType::Directory)],
                )
            };
            self.dialog = Some(Dialog::DeleteConfirm { paths, is_dirs });
            ctx.notify();
        }
    }

    /// Performs the delete operation
    fn confirm_delete(&mut self, ctx: &mut ViewContext<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => {
                self.show_error_toast("Not connected to server".to_string(), ctx);
                self.dialog = None;
                ctx.notify();
                return;
            }
        };

        let (paths, is_dirs) = match &self.dialog {
            Some(Dialog::DeleteConfirm { paths, is_dirs }) => (paths.clone(), is_dirs.clone()),
            Some(Dialog::Rename { .. })
            | Some(Dialog::CreateFolder { .. })
            | Some(Dialog::Move { .. })
            | Some(Dialog::OverwriteConfirm { .. })
            | Some(Dialog::FileDetails { .. })
            | Some(Dialog::CloseTransferPanelConfirm)
            | None => {
                self.dialog = None;
                ctx.notify();
                return;
            }
        };

        self.dialog = None;
        self.is_loading = true;
        ctx.notify();

        self.run_blocking(
            ctx,
            move || {
                for (path, is_dir) in paths.iter().zip(is_dirs.iter()) {
                    let result = if *is_dir {
                        sftp.delete_dir_recursive(path)
                    } else {
                        sftp.delete_file(path)
                    };
                    if let Err(e) = result {
                        return Err(e.to_string());
                    }
                }
                Ok(())
            },
            move |me, result, ctx| {
                me.is_loading = false;
                me.selected.clear();
                match result {
                    Ok(Ok(())) => {
                        me.refresh_dir(ctx);
                    }
                    Ok(Err(e)) => {
                        me.show_error_toast(format!("Delete failed: {e}"), ctx);
                        me.refresh_dir(ctx);
                    }
                    Err(_) => {
                        // Cancelled
                        me.refresh_dir(ctx);
                    }
                }
                ctx.notify();
            },
        );
    }

    /// Creates a download transfer task
    fn download_entry(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            let default_name = entry.name.clone();
            let idx = index;
            ctx.open_save_file_picker(
                move |path_opt: Option<String>, _me: &mut Self, _ctx: &mut ViewContext<Self>| {
                    if let Some(path) = path_opt {
                        _ctx.dispatch_typed_action_deferred(SftpBrowserAction::DownloadSaveAs {
                            index: idx,
                            local_path: path,
                        });
                    }
                },
                SaveFilePickerConfiguration::new().with_default_filename(default_name),
            );
        }
    }

    /// Shows the entry details dialog
    fn show_details(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            self.dialog = Some(Dialog::FileDetails {
                entry: entry.clone(),
            });
            ctx.notify();
        }
    }

    /// Shows the rename dialog
    fn rename_entry(&mut self, index: usize, ctx: &mut ViewContext<Self>) {
        if let Some(entry) = self.entries.get(index) {
            self.dialog = Some(Dialog::Rename {
                path: entry.path.clone(),
                original_name: entry.name.clone(),
            });
            // Write the current name into the editor
            self.rename_editor
                .update(ctx, |e, ctx| e.set_buffer_text(&entry.name, ctx));
            ctx.notify();
        }
    }

    /// Renders a single toolbar button
    fn render_toolbar_btn(
        &self,
        icon: Icon,
        handle: MouseStateHandle,
        action: SftpBrowserAction,
        _tooltip: &str,
        appearance: &Appearance,
        position_id: &'static str,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let icon_color = theme.sub_text_color(theme.background());

        let icon_el = ConstrainedBox::new(icon.to_warpui_icon(icon_color).finish())
            .with_width(TOOLBAR_ICON_SIZE)
            .with_height(TOOLBAR_ICON_SIZE)
            .finish();

        let btn_el = Hoverable::new(handle, move |_| {
            Container::new(
                ConstrainedBox::new(Container::new(icon_el).with_uniform_padding(6.0).finish())
                    .with_width(TOOLBAR_BTN_SIZE)
                    .with_height(TOOLBAR_BTN_SIZE)
                    .finish(),
            )
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
            .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(action.clone());
        })
        .finish();

        SavePosition::new(btn_el, position_id).finish()
    }

    /// Renders the toolbar
    fn render_toolbar(&self, appearance: &Appearance) -> Box<dyn Element> {
        let nav_buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(TOOLBAR_SPACING)
            .with_child(self.render_toolbar_btn(
                Icon::ChevronLeft,
                self.back_btn.clone(),
                SftpBrowserAction::GoBack,
                "Back",
                appearance,
                "sftp_btn:back",
            ))
            .with_child(self.render_toolbar_btn(
                Icon::ChevronRight,
                self.forward_btn.clone(),
                SftpBrowserAction::GoForward,
                "Forward",
                appearance,
                "sftp_btn:forward",
            ))
            .with_child(self.render_toolbar_btn(
                Icon::ArrowUp,
                self.up_btn.clone(),
                SftpBrowserAction::GoUp,
                "Up",
                appearance,
                "sftp_btn:up",
            ))
            .with_child(self.render_toolbar_btn(
                Icon::Refresh,
                self.refresh_btn.clone(),
                SftpBrowserAction::Refresh,
                "Refresh",
                appearance,
                "sftp_btn:refresh",
            ))
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        let action_buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(TOOLBAR_SPACING)
            .with_child(self.render_toolbar_btn(
                Icon::UploadCloud,
                self.upload_btn.clone(),
                SftpBrowserAction::UploadFile,
                "Upload",
                appearance,
                "sftp_btn:upload",
            ))
            .with_child(self.render_toolbar_btn(
                Icon::Plus,
                self.new_folder_btn.clone(),
                SftpBrowserAction::NewFolder,
                "New folder",
                appearance,
                "sftp_btn:new_folder",
            ))
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(nav_buttons)
            .with_child(action_buttons)
            .finish()
    }

    /// Renders the breadcrumb trail
    fn render_breadcrumb(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = theme.sub_text_color(theme.background());

        let parts: Vec<Box<dyn Element>> =
            super::breadcrumb::render_breadcrumb(&self.current_path, appearance);

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(2.0);

        // Add the root "/" as a clickable entry
        let root_text_color = text_color;
        let root_hoverable = Hoverable::new(Default::default(), move |_| {
            let t = Text::new_inline(
                "/".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(root_text_color.into())
            .finish();
            Container::new(t).finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(SftpBrowserAction::NavigateTo(PathBuf::from("/")));
        })
        .finish();
        let root_el = SavePosition::new(root_hoverable, "sftp_breadcrumb:/").finish();
        row.add_child(root_el);

        for part in parts {
            row.add_child(part);
        }

        Container::new(row.finish())
            .with_padding_left(4.0)
            .with_padding_right(4.0)
            .with_padding_top(4.0)
            .with_padding_bottom(4.0)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
            .with_background(theme.surface_2())
            .finish()
    }

    /// Renders the connection state (when not connected)
    fn render_connection_state(&self, appearance: &Appearance) -> Box<dyn Element> {
        let (msg, icon) = match &self.connection {
            ConnectionState::Connecting => ("Connecting...".to_string(), Icon::Loading),
            ConnectionState::Failed(err) => (err.clone(), Icon::AlertCircle),
            ConnectionState::Disconnected => ("Disconnected".to_string(), Icon::AlertCircle),
            ConnectionState::Connected => {
                return Container::new(Flex::row().finish()).finish();
            }
        };

        render_centered_status(icon, &msg, 12.0, appearance)
    }

    /// Renders the file list
    fn render_file_list(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();

        // Filter entries
        let filtered_indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                self.search_filter.as_ref().map_or(true, |filter| {
                    entry.name.to_lowercase().contains(&filter.to_lowercase())
                })
            })
            .map(|(i, _)| i)
            .collect();

        if filtered_indices.is_empty() {
            let text_el = Text::new_inline(
                "This folder is empty".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(theme.sub_text_color(theme.background()).into())
            .finish();

            return Align::new(Container::new(text_el).with_uniform_padding(24.0).finish())
                .finish();
        }

        // Header
        let header = super::file_list::render_header(appearance);

        // File rows
        let rows = super::file_list::render_file_rows(
            &self.entries,
            &filtered_indices,
            &self.selected,
            &self.row_mouse_handles,
            appearance,
        );

        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(header)
            .with_child(rows)
            .finish()
    }

    /// Renders the transfer panel
    fn render_transfers(&self, appearance: &Appearance) -> Box<dyn Element> {
        super::transfer_panel::render_transfer_panel(
            &self.transfers,
            appearance,
            self.transfer_panel_close_btn.clone(),
        )
    }

    /// Performs an upload (shared entry point for both drag-and-drop upload
    /// and file-picker upload)
    ///
    /// First checks whether a file with the same name already exists in the
    /// remote directory; if so, shows an overwrite confirmation dialog, and
    /// once the user confirms, performs the actual upload via
    /// `execute_upload_confirmed`.
    fn execute_upload(&mut self, local_path: &Path, ctx: &mut ViewContext<Self>) {
        let file_name = local_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let remote_path = match build_upload_remote_path(&self.current_path, &file_name) {
            Some(p) => p,
            None => {
                self.show_error_toast("Filename contains invalid characters".to_string(), ctx);
                return;
            }
        };

        // Check whether a file with the same name already exists in the remote directory
        let existing = self.entries.iter().find(|e| {
            e.name == file_name && matches!(e.file_type, FileEntryType::File)
        });

        if existing.is_some() {
            let local_size = std::fs::metadata(local_path).map(|m| m.len()).unwrap_or(0);
            self.dialog = Some(Dialog::OverwriteConfirm {
                source: local_path.to_path_buf(),
                target: remote_path,
                file_size: local_size,
                direction: TransferDirection::Upload,
            });
            ctx.notify();
            return;
        }

        // No conflict, perform the upload directly
        self.execute_upload_confirmed(local_path, &remote_path, ctx);
    }

    /// Processes the pending upload queue, uploading one by one until a
    /// conflict is hit or the queue is empty.
    ///
    /// For drag-and-drop batch uploads, all files are enqueued and then
    /// processed one at a time. When a same-name conflict is hit, the queue
    /// pauses and shows an overwrite confirmation dialog; once the user
    /// confirms, ConfirmOverwrite calls this method again to continue.
    /// author: logic
    /// date: 2026-06-01
    fn process_pending_uploads(&mut self, ctx: &mut ViewContext<Self>) {
        while let Some(local_path) = self.pending_uploads.pop() {
            self.execute_upload(&local_path, ctx);
            if self.dialog.is_some() {
                // Hit a conflict, pause the queue and wait for user confirmation
                return;
            }
        }
    }

    /// Performs an already-confirmed upload (creates a transfer task and starts a background upload)
    fn execute_upload_confirmed(
        &mut self,
        local_path: &Path,
        remote_path: &Path,
        ctx: &mut ViewContext<Self>,
    ) {
        let total_size = std::fs::metadata(local_path).map(|m| m.len()).unwrap_or(0);

        let task = TransferTask::new(
            self.next_transfer_id,
            local_path.to_path_buf(),
            remote_path.to_path_buf(),
            TransferDirection::Upload,
            total_size,
        );
        self.next_transfer_id += 1;
        let task_id = task.id;
        let cancel_flag = task.cancel_flag.clone();
        self.transfers.push(task);

        if let Some(t) = self.transfers.iter_mut().find(|t| t.id == task_id) {
            t.state = TransferState::InProgress;
        }
        self.transfer_panel_hidden = false;
        ctx.notify();

        if let Some(sftp) = &self.sftp {
            let sftp = sftp.clone();
            let transferred = Arc::new(AtomicU64::new(0));
            let transferred_clone = transferred.clone();

            let progress_cb: sftp_ops::ProgressCallback =
                Box::new(move |bytes, _total| {
                    transferred_clone.store(bytes, Ordering::SeqCst);
                });

            let local_path = local_path.to_path_buf();
            let remote_path = remote_path.to_path_buf();
            if let Some(handle) = self.run_blocking(
                ctx,
                move || {
                    sftp.upload_file(&local_path, &remote_path, Some(&progress_cb), Some(&cancel_flag))
                },
                move |me, result, ctx| {
                    if let Some(t) = me.transfers.iter_mut().find(|t| t.id == task_id) {
                        match &result {
                            Ok(Ok(())) => {
                                t.state = TransferState::Completed;
                                t.transferred = t.total_size;
                            }
                            Ok(Err(e)) => {
                                if matches!(e, super::sftp_ops::SftpOpsError::Cancelled) {
                                    t.state = TransferState::Cancelled;
                                } else {
                                    t.state = TransferState::Failed(e.to_string());
                                }
                                t.transferred = transferred.load(Ordering::SeqCst);
                            }
                            Err(_) => {
                                // JoinError (aborted)
                                t.state = TransferState::Cancelled;
                                t.transferred = transferred.load(Ordering::SeqCst);
                            }
                        }
                    }

                    // Clean up the handle once the transfer completes (the future has ended, no abort needed)
                    me.transfer_handles.remove(&task_id);

                    match &result {
                        Ok(Ok(())) => {
                            me.refresh_dir(ctx);
                        }
                        Ok(Err(e)) => {
                            log::error!("sftp: upload failed: {e}");
                            me.show_error_toast(format!("Upload failed: {e}"), ctx);
                            ctx.notify();
                        }
                        Err(_) => {
                            ctx.notify();
                        }
                    }
                },
            ) {
                self.transfer_handles.insert(task_id, handle);
            }
        } else {
            if let Some(t) = self.transfers.iter_mut().find(|t| t.id == task_id) {
                t.state = TransferState::Failed("Not connected to server".to_string());
            }
            log::error!("sftp: upload failed: not connected to server");
            self.show_error_toast("Upload failed: not connected to server".to_string(), ctx);
            ctx.notify();
        }
    }

    /// Performs the download (shared logic for confirm-overwrite and save-as)
    fn execute_download(
        &mut self,
        remote_path: &Path,
        local_path: &Path,
        file_size: u64,
        ctx: &mut ViewContext<Self>,
    ) {
        let task = TransferTask::new(
            self.next_transfer_id,
            remote_path.to_path_buf(),
            local_path.to_path_buf(),
            TransferDirection::Download,
            file_size,
        );
        self.next_transfer_id += 1;
        let task_id = task.id;
        let cancel_flag = task.cancel_flag.clone();
        self.transfers.push(task);

        if let Some(t) = self.transfers.iter_mut().find(|t| t.id == task_id) {
            t.state = TransferState::InProgress;
        }
        self.transfer_panel_hidden = false;
        ctx.notify();

        if let Some(sftp) = &self.sftp {
            let sftp = sftp.clone();
            let transferred = Arc::new(AtomicU64::new(0));
            let transferred_clone = transferred.clone();

            let progress_cb: sftp_ops::ProgressCallback =
                Box::new(move |bytes, _total| {
                    transferred_clone.store(bytes, Ordering::SeqCst);
                });

            let remote_path = remote_path.to_path_buf();
            let local_path = local_path.to_path_buf();
            if let Some(handle) = self.run_blocking(
                ctx,
                move || {
                    sftp.download_file(&remote_path, &local_path, Some(&progress_cb), Some(&cancel_flag))
                },
                move |me, result, ctx| {
                    if let Some(t) = me.transfers.iter_mut().find(|t| t.id == task_id) {
                        match &result {
                            Ok(Ok(())) => {
                                t.state = TransferState::Completed;
                                t.transferred = t.total_size;
                            }
                            Ok(Err(e)) => {
                                if matches!(e, super::sftp_ops::SftpOpsError::Cancelled) {
                                    t.state = TransferState::Cancelled;
                                } else {
                                    t.state = TransferState::Failed(e.to_string());
                                }
                                t.transferred = transferred.load(Ordering::SeqCst);
                            }
                            Err(_) => {
                                t.state = TransferState::Cancelled;
                                t.transferred = transferred.load(Ordering::SeqCst);
                            }
                        }
                    }

                    // Clean up the handle once the transfer completes (the future has ended, no abort needed)
                    me.transfer_handles.remove(&task_id);

                    if let Ok(Err(e)) = &result {
                        log::error!("sftp: download failed: {e}");
                        me.show_error_toast(format!("Download failed: {e}"), ctx);
                    }
                    ctx.notify();
                },
            ) {
                self.transfer_handles.insert(task_id, handle);
            }
        } else {
            if let Some(t) = self.transfers.iter_mut().find(|t| t.id == task_id) {
                t.state = TransferState::Failed("Not connected to server".to_string());
            }
            log::error!("sftp: download failed: not connected to server");
            self.show_error_toast("Download failed: not connected to server".to_string(), ctx);
            ctx.notify();
        }
    }

    /// Renders the search bar
    fn render_search_bar(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let text_color = theme.sub_text_color(theme.background());

        let search_icon = ConstrainedBox::new(Icon::Search.to_warpui_icon(text_color).finish())
            .with_width(14.0)
            .with_height(14.0)
            .finish();

        let editor_el = Container::new(ChildView::new(&self.search_editor).finish()).finish();

        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(4.0)
                .with_child(search_icon)
                .with_child(Shrinkable::new(1.0, editor_el).finish())
                .finish(),
        )
        .with_padding_left(8.0)
        .with_padding_right(8.0)
        .with_padding_top(4.0)
        .with_padding_bottom(4.0)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
        .with_background(theme.surface_2())
        .finish()
    }

    /// Renders the loading state
    fn render_loading(&self, appearance: &Appearance) -> Box<dyn Element> {
        render_centered_status(Icon::Loading, "Loading...", 8.0, appearance)
    }
}

/// Renders a centered status hint (icon + text)
fn render_centered_status(
    icon: Icon,
    message: &str,
    spacing: f32,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let text_color = theme.sub_text_color(theme.background());

    let icon_el = ConstrainedBox::new(icon.to_warpui_icon(text_color).finish())
        .with_width(24.0)
        .with_height(24.0)
        .finish();

    let text_el = Text::new_inline(
        message.to_string(),
        appearance.ui_font_family(),
        appearance.ui_font_size(),
    )
    .with_color(text_color.into())
    .finish();

    let content = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_spacing(spacing)
        .with_child(icon_el)
        .with_child(text_el)
        .with_main_axis_size(MainAxisSize::Min)
        .finish();

    Align::new(Container::new(content).with_uniform_padding(24.0).finish()).finish()
}

/// Safely joins a filename onto a parent path, preventing path injection and traversal
fn safe_join_name(parent: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.starts_with('/') || name.starts_with('\\') {
        return None;
    }
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return None;
    }
    Some(parent.join(name))
}

/// Builds the full path after a rename
fn build_rename_path(original_path: &PathBuf, new_name: &str) -> Option<PathBuf> {
    let parent = original_path.parent().unwrap_or(Path::new("/"));
    safe_join_name(parent, new_name).map(|p| normalize_remote_path(&p))
}

/// Builds the full path for a new folder
fn build_new_folder_path(parent_path: &PathBuf, folder_name: &str) -> Option<PathBuf> {
    safe_join_name(parent_path, folder_name).map(|p| normalize_remote_path(&p))
}

/// Builds the remote path after an upload
fn build_upload_remote_path(current_path: &PathBuf, local_file_name: &str) -> Option<PathBuf> {
    let name = Path::new(local_file_name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| local_file_name.to_string());
    safe_join_name(current_path, &name).map(|p| normalize_remote_path(&p))
}

impl Entity for SftpBrowserView {
    type Event = PaneEvent;
}

impl TypedActionView for SftpBrowserView {
    type Action = SftpBrowserAction;

    /// Handles all SFTP browser actions
    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SftpBrowserAction::NavigateTo(path) => {
                self.navigate_to(path.clone(), ctx);
            }
            SftpBrowserAction::GoUp => {
                self.go_up(ctx);
            }
            SftpBrowserAction::GoBack => {
                self.go_back(ctx);
            }
            SftpBrowserAction::GoForward => {
                self.go_forward(ctx);
            }
            SftpBrowserAction::Refresh => {
                self.refresh_dir(ctx);
            }
            SftpBrowserAction::SelectEntry(index) => {
                let index = *index;
                self.selected.clear();
                self.selected.insert(index);
                ctx.notify();
            }
            SftpBrowserAction::OpenEntry(index) => {
                let index = *index;
                self.open_entry(index, ctx);
            }
            SftpBrowserAction::DeleteEntry(index) => {
                let index = *index;
                self.delete_selected(index, ctx);
            }
            SftpBrowserAction::RenameEntry(index) => {
                let index = *index;
                self.rename_entry(index, ctx);
            }
            SftpBrowserAction::DownloadEntry(index) => {
                let index = *index;
                self.download_entry(index, ctx);
            }
            SftpBrowserAction::UploadFile => {
                ctx.open_file_picker(
                    move |result, ctx: &mut ViewContext<SftpBrowserView>| match result {
                        Ok(paths) => {
                            for path in paths {
                                ctx.dispatch_typed_action(&SftpBrowserAction::ExecuteUpload(path));
                            }
                        }
                        Err(e) => {
                            log::warn!("sftp: file picker failed: {e}");
                        }
                    },
                    FilePickerConfiguration::new(),
                );
            }
            SftpBrowserAction::NewFolder => {
                self.dialog = Some(Dialog::CreateFolder {
                    parent_path: self.current_path.clone(),
                });
                self.new_folder_editor
                    .update(ctx, |e, ctx| e.set_buffer_text("", ctx));
                ctx.notify();
            }
            SftpBrowserAction::ConfirmDelete => {
                self.confirm_delete(ctx);
            }
            SftpBrowserAction::ConfirmRename => {
                if let Some(Dialog::Rename {
                    path: original_path,
                    ..
                }) = &self.dialog
                {
                    let new_name = self.rename_editor.as_ref(ctx).buffer_text(ctx);
                    let new_name = new_name.trim().to_string();
                    if new_name.is_empty() {
                        self.show_error_toast("Name cannot be empty".to_string(), ctx);
                        return;
                    }
                    let new_path = match build_rename_path(original_path, &new_name) {
                        Some(p) => p,
                        None => {
                            self.show_error_toast("Invalid name: cannot contain path separators".to_string(), ctx);
                            return;
                        }
                    };

                    if let Some(sftp) = &self.sftp {
                        let sftp = sftp.clone();
                        let original_path = original_path.clone();
                        self.dialog = None;
                        ctx.notify();
                        self.run_blocking(
                            ctx,
                            move || sftp.rename(&original_path, &new_path),
                            move |me, result, ctx| {
                                match result {
                                    Ok(Ok(())) => {
                                        me.refresh_dir(ctx);
                                    }
                                    Ok(Err(e)) => {
                                        me.show_error_toast(format!("Rename failed: {e}"), ctx);
                                    }
                                    Err(_) => {}
                                }
                                ctx.notify();
                            },
                        );
                    } else {
                        self.show_error_toast("Not connected to server".to_string(), ctx);
                        self.dialog = None;
                    }
                }
            }
            SftpBrowserAction::ConfirmNewFolder => {
                if let Some(Dialog::CreateFolder { parent_path }) = &self.dialog {
                    let folder_name = self.new_folder_editor.as_ref(ctx).buffer_text(ctx);
                    let folder_name = folder_name.trim().to_string();
                    if folder_name.is_empty() {
                        self.show_error_toast("Folder name cannot be empty".to_string(), ctx);
                        return;
                    }
                    let folder_path = match build_new_folder_path(parent_path, &folder_name) {
                        Some(p) => p,
                        None => {
                            self.show_error_toast("Invalid name: cannot contain path separators".to_string(), ctx);
                            return;
                        }
                    };

                    if let Some(sftp) = &self.sftp {
                        let sftp = sftp.clone();
                        self.dialog = None;
                        ctx.notify();
                        self.run_blocking(
                            ctx,
                            move || sftp.create_dir(&folder_path),
                            move |me, result, ctx| {
                                match result {
                                    Ok(Ok(())) => {
                                        me.refresh_dir(ctx);
                                    }
                                    Ok(Err(e)) => {
                                        me.show_error_toast(format!("Failed to create folder: {e}"), ctx);
                                    }
                                    Err(_) => {}
                                }
                                ctx.notify();
                            },
                        );
                    } else {
                        self.show_error_toast("Not connected to server".to_string(), ctx);
                        self.dialog = None;
                    }
                }
            }
            SftpBrowserAction::ConfirmOverwrite => {
                // Extract the path and transfer direction from the dialog
                let (source, target, file_size, direction) = match &self.dialog {
                    Some(Dialog::OverwriteConfirm { source, target, file_size, direction }) => {
                        (source.clone(), target.clone(), *file_size, *direction)
                    }
                    Some(Dialog::DeleteConfirm { .. })
                    | Some(Dialog::Rename { .. })
                    | Some(Dialog::CreateFolder { .. })
                    | Some(Dialog::Move { .. })
                    | Some(Dialog::FileDetails { .. })
                    | Some(Dialog::CloseTransferPanelConfirm)
                    | None => {
                        self.dialog = None;
                        ctx.notify();
                        return;
                    }
                };

                // Close the dialog
                self.dialog = None;
                match direction {
                    TransferDirection::Download => {
                        self.execute_download(&source, &target, file_size, ctx);
                    }
                    TransferDirection::Upload => {
                        self.execute_upload_confirmed(&source, &target, ctx);
                    }
                }
                // Batch upload queue: after confirming the current file, continue with the next one
                self.process_pending_uploads(ctx);
            }
            SftpBrowserAction::ContextMenu { index, position } => {
                let index = *index;
                let position = *position;
                self.context_menu = Some(ContextMenuState::new(index, position));
                self.selected.clear();
                self.selected.insert(index);
                ctx.notify();
            }
            SftpBrowserAction::CloseContextMenu => {
                self.context_menu = None;
                ctx.notify();
            }
            SftpBrowserAction::CloseDialog => {
                // If the user cancels the overwrite confirmation, clear the rest of the batch upload queue
                let was_upload_overwrite = matches!(
                    self.dialog,
                    Some(Dialog::OverwriteConfirm { direction: TransferDirection::Upload, .. })
                );
                self.dialog = None;
                if was_upload_overwrite {
                    self.pending_uploads.clear();
                }
                ctx.notify();
            }
            SftpBrowserAction::DetailsEntry(index) => {
                let index = *index;
                self.show_details(index, ctx);
            }
            SftpBrowserAction::SetSearchFilter(filter) => {
                self.search_filter = Some(filter.clone());
                ctx.notify();
            }
            SftpBrowserAction::ClearSearchFilter => {
                self.search_filter = None;
                ctx.notify();
            }
            SftpBrowserAction::NavigateUp => {
                self.go_up(ctx);
            }
            SftpBrowserAction::DeleteSelected => {
                if let Some(&index) = self.selected.iter().next() {
                    self.delete_selected(index, ctx);
                }
            }
            SftpBrowserAction::CreateFolder => {
                self.handle_action(&SftpBrowserAction::NewFolder, ctx);
            }
            SftpBrowserAction::ConfirmMove => {
                if let Some(Dialog::Move { source, target_dir }) = &self.dialog {
                    let file_name = source
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let target_path = match safe_join_name(target_dir, &file_name) {
                        Some(p) => normalize_remote_path(&p),
                        None => {
                            self.show_error_toast("Invalid target path".to_string(), ctx);
                            self.dialog = None;
                            ctx.notify();
                            return;
                        }
                    };

                    if let Some(sftp) = &self.sftp {
                        let sftp = sftp.clone();
                        let source = source.clone();
                        self.dialog = None;
                        ctx.notify();
                        self.run_blocking(
                            ctx,
                            move || sftp.rename(&source, &target_path),
                            move |me, result, ctx| {
                                match result {
                                    Ok(Ok(())) => {
                                        me.refresh_dir(ctx);
                                    }
                                    Ok(Err(e)) => {
                                        me.show_error_toast(format!("Move failed: {e}"), ctx);
                                    }
                                    Err(_) => {}
                                }
                                ctx.notify();
                            },
                        );
                    } else {
                        self.show_error_toast("Not connected to server".to_string(), ctx);
                        self.dialog = None;
                    }
                }
            }
            SftpBrowserAction::CancelTransfer(task_id) => {
                let task_id = *task_id;
                // Cooperative cancellation: set the cancel_flag
                if let Some(t) = self.transfers.iter().find(|t| t.id == task_id) {
                    t.cancel();
                }
                // Structural cancellation: abort the spawned future
                if let Some(handle) = self.transfer_handles.remove(&task_id) {
                    handle.abort();
                }
                ctx.notify();
            }
            SftpBrowserAction::ToggleTransferPanel => {
                let has_active = self.transfers.iter().any(|t| {
                    matches!(t.state, TransferState::Pending | TransferState::InProgress)
                });
                if has_active {
                    self.dialog = Some(Dialog::CloseTransferPanelConfirm);
                } else {
                    self.transfers.clear();
                    self.transfer_panel_hidden = true;
                }
                ctx.notify();
            }
            SftpBrowserAction::ConfirmCloseTransferPanel => {
                for task in &self.transfers {
                    task.cancel();
                }
                for (_, handle) in self.transfer_handles.drain() {
                    handle.abort();
                }
                self.transfers.clear();
                self.transfer_panel_hidden = true;
                self.dialog = None;
                ctx.notify();
            }
            SftpBrowserAction::DragFilesEnter => {
                self.is_drag_hovering = true;
                ctx.notify();
            }
            SftpBrowserAction::DragFilesLeave => {
                self.is_drag_hovering = false;
                ctx.notify();
            }
            SftpBrowserAction::DragAndDropFiles(paths) => {
                self.is_drag_hovering = false;
                // Enqueue in reverse order, so pop() retrieves them in the original order
                self.pending_uploads = paths.iter().rev().cloned().collect();
                self.process_pending_uploads(ctx);
            }
            SftpBrowserAction::ExecuteUpload(local_path_str) => {
                let local_path = PathBuf::from(local_path_str);
                self.execute_upload(&local_path, ctx);
            }
            SftpBrowserAction::DownloadSaveAs { index, local_path } => {
                let local_path = PathBuf::from(local_path);
                let (remote_path, file_size) = self
                    .entries
                    .get(*index)
                    .map(|e| (e.path.clone(), e.size))
                    .unzip();
                if let (Some(remote_path), Some(file_size)) = (remote_path, file_size) {
                    self.execute_download(&remote_path, &local_path, file_size, ctx);
                }
            }
        }
    }
}

impl View for SftpBrowserView {
    fn ui_name() -> &'static str {
        "SftpBrowserView"
    }

    /// Renders the full UI layout
    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        // 1. Show the connection state when not connected
        if !matches!(self.connection, ConnectionState::Connected) {
            return Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(self.render_connection_state(appearance))
                .finish();
        }

        let mut col = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Max);

        // 2. Breadcrumb
        col.add_child(
            Container::new(self.render_breadcrumb(appearance))
                .with_padding_left(PANEL_PADDING)
                .with_padding_right(PANEL_PADDING)
                .with_padding_top(PANEL_PADDING)
                .finish(),
        );

        // 3. Toolbar
        col.add_child(
            Container::new(self.render_toolbar(appearance))
                .with_padding_left(PANEL_PADDING)
                .with_padding_right(PANEL_PADDING)
                .with_padding_top(4.0)
                .with_padding_bottom(4.0)
                .finish(),
        );

        // 4. Search bar
        col.add_child(
            Container::new(self.render_search_bar(appearance))
                .with_padding_left(PANEL_PADDING)
                .with_padding_right(PANEL_PADDING)
                .with_padding_bottom(4.0)
                .finish(),
        );

        // 5. Loading state / file list
        if self.is_loading {
            col.add_child(Shrinkable::new(1.0, self.render_loading(appearance)).finish());
        } else {
            let file_list = self.render_file_list(appearance);
            let scrollbar_color = theme.disabled_text_color(theme.background()).into();
            let scrollbar_thumb_hover = theme.main_text_color(theme.background()).into();
            let scrollable = ClippedScrollable::vertical(
                self.scroll_state.clone(),
                file_list,
                ScrollbarWidth::Auto,
                scrollbar_color,
                scrollbar_thumb_hover,
                Fill::None,
            )
            .finish();
            col.add_child(Shrinkable::new(1.0, scrollable).finish());
        }

        // 7. Transfer panel (floats at the bottom)
        let mut main_content = col.finish();

        // 8. Transfer panel overlay layer
        if !self.transfers.is_empty() && !self.transfer_panel_hidden {
            let panel_el = Container::new(self.render_transfers(appearance))
                .with_padding_left(PANEL_PADDING)
                .with_padding_right(PANEL_PADDING)
                .with_padding_bottom(PANEL_PADDING)
                .finish();
            let mut stack = Stack::new();
            stack.add_child(main_content);
            stack.add_positioned_overlay_child(
                panel_el,
                OffsetPositioning::offset_from_parent(
                    Vector2F::new(0.0, 0.0),
                    ParentOffsetBounds::ParentBySize,
                    ParentAnchor::BottomLeft,
                    ChildAnchor::BottomLeft,
                ),
            );
            main_content = stack.finish();
        }

        // 9. Context menu
        if let Some(ref cm_state) = self.context_menu {
            let menu_el = super::context_menu::render_context_menu(cm_state, appearance);
            let positioning = OffsetPositioning::offset_from_parent(
                cm_state.position,
                ParentOffsetBounds::ParentByPosition,
                ParentAnchor::TopLeft,
                ChildAnchor::TopLeft,
            );
            let mut stack = Stack::new();
            stack.add_child(main_content);
            stack.add_positioned_overlay_child(menu_el, positioning);
            main_content = stack.finish();
        }

        // 9. Dialog (overlay)
        if let Some(ref dialog) = self.dialog {
            let dialog_el = super::dialogs::render_dialog(
                dialog,
                &self.rename_editor,
                &self.new_folder_editor,
                appearance,
                self.dialog_confirm_btn.clone(),
                self.dialog_cancel_btn.clone(),
                self.dialog_close_btn.clone(),
            );
            let mut stack = Stack::new();
            stack.add_child(main_content);
            stack.add_overlay_child(Align::new(dialog_el).finish());
            main_content = stack.finish();
        }

        // 10. Drag visual feedback
        if self.is_drag_hovering {
            let drop_hint = Text::new_inline(
                "Drop files to upload".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size() + 2.0,
            )
            .with_color(theme.accent().into())
            .finish();
            let overlay = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_main_axis_alignment(MainAxisAlignment::Center)
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(drop_hint)
                .finish();
            let overlay_container = Container::new(overlay)
                .with_background(theme.accent().with_opacity(20))
                .with_border(Border::all(2.0).with_border_fill(theme.accent().into_solid()))
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.0)))
                .finish();
            let mut stack = Stack::new();
            stack.add_child(main_content);
            stack.add_child(overlay_container);
            main_content = stack.finish();
        }

        // 11. Save panel position (used for context menu position calculation)
        let positioned_content =
            SavePosition::new(main_content, SFTP_PANEL_POSITION_ID).finish();

        // 12. Keyboard event interception
        let key_handler = EventHandler::new(positioned_content).on_keydown(
            move |ctx, _app, keystroke| {
                match keystroke.key.as_str() {
                    "delete" => {
                        ctx.dispatch_typed_action(SftpBrowserAction::DeleteSelected);
                        DispatchEventResult::StopPropagation
                    }
                    "backspace" => {
                        ctx.dispatch_typed_action(SftpBrowserAction::NavigateUp);
                        DispatchEventResult::StopPropagation
                    }
                    "escape" => {
                        ctx.dispatch_typed_action(SftpBrowserAction::CloseDialog);
                        DispatchEventResult::StopPropagation
                    }
                    _ => DispatchEventResult::PropagateToParent,
                }
            },
        );

        // 13. Drag event interception
        super::drop_target::SftpDropTargetElement::new(key_handler.finish()).finish()
    }
}

impl BackingView for SftpBrowserView {
    type PaneHeaderOverflowMenuAction = SftpBrowserAction;
    type CustomAction = ();
    type AssociatedData = ();

    /// Handles overflow menu actions
    fn handle_pane_header_overflow_menu_action(
        &mut self,
        action: &Self::PaneHeaderOverflowMenuAction,
        ctx: &mut ViewContext<Self>,
    ) {
        self.handle_action(action, ctx);
    }

    /// Closes the view
    fn close(&mut self, ctx: &mut ViewContext<Self>) {
        // Cooperative cancellation: set the cancel_flag on all transfer tasks
        for task in &self.transfers {
            task.cancel();
        }
        // Structural cancellation: abort the spawned future
        for (_, handle) in self.transfer_handles.drain() {
            handle.abort();
        }
        self.pending_uploads.clear();
        self.connect_handle = None;
        self.refresh_handle = None;
        self._session = None;
        self.sftp = None;
        self.connection = ConnectionState::Disconnected;
        ctx.emit(PaneEvent::Close);
    }

    /// Focuses content, setting the window focus to the current view
    fn focus_contents(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.focus_self();
    }

    /// Renders the header content
    fn render_header_content(
        &self,
        _ctx: &view::HeaderRenderContext<'_>,
        _app: &AppContext,
    ) -> view::HeaderContent {
        let path = self.current_path.display();
        let title = format!("SFTP: {path}");
        view::HeaderContent::simple(title)
    }

    /// Sets the focus handle
    fn set_focus_handle(&mut self, focus_handle: PaneFocusHandle, _ctx: &mut ViewContext<Self>) {
        self.focus_handle = Some(focus_handle);
    }
}

/// Creates a single-line editor
fn make_editor(
    placeholder: &str,
    ctx: &mut ViewContext<SftpBrowserView>,
) -> ViewHandle<EditorView> {
    let placeholder = placeholder.to_string();
    ctx.add_typed_action_view(move |ctx| {
        let options = {
            let appearance = Appearance::as_ref(ctx);
            let theme = appearance.theme();
            SingleLineEditorOptions {
                text: TextOptions {
                    font_size_override: Some(appearance.ui_font_size()),
                    font_family_override: Some(appearance.monospace_font_family()),
                    text_colors_override: Some(TextColors {
                        default_color: theme.active_ui_text_color(),
                        disabled_color: theme.disabled_ui_text_color(),
                        hint_color: theme.disabled_ui_text_color(),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }
        };
        let mut editor = EditorView::single_line(options, ctx);
        editor.set_placeholder_text(&placeholder, ctx);
        editor
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ============================================================
    // normalize_remote_path tests
    // ============================================================

    /// Tests backslashes being replaced with forward slashes
    #[test]
    fn test_normalize_remote_path_backslash() {
        let path = PathBuf::from(r"home\user\docs");
        let result = normalize_remote_path(&path);
        assert_eq!(result, PathBuf::from("home/user/docs"));
    }

    /// Tests that a pure forward-slash path is unchanged
    #[test]
    fn test_normalize_remote_path_forward_slash() {
        let path = PathBuf::from("/home/user/docs");
        let result = normalize_remote_path(&path);
        assert_eq!(result, PathBuf::from("/home/user/docs"));
    }

    /// Tests the root path
    #[test]
    fn test_normalize_remote_path_root() {
        let path = PathBuf::from("/");
        let result = normalize_remote_path(&path);
        assert_eq!(result, PathBuf::from("/"));
    }

    /// Tests an empty path
    #[test]
    fn test_normalize_remote_path_empty() {
        let path = PathBuf::from("");
        let result = normalize_remote_path(&path);
        assert_eq!(result, PathBuf::from(""));
    }

    /// Tests a mixed-slash path
    #[test]
    fn test_normalize_remote_path_mixed() {
        let path = PathBuf::from(r"home/user\docs/file.txt");
        let result = normalize_remote_path(&path);
        assert_eq!(result, PathBuf::from("home/user/docs/file.txt"));
    }

    // ============================================================
    // build_rename_path tests
    // ============================================================

    /// Tests building a rename path
    #[test]
    fn test_build_rename_path_basic() {
        let original = PathBuf::from("/home/user/old.txt");
        let result = build_rename_path(&original, "new.txt");
        assert_eq!(result, Some(PathBuf::from("/home/user/new.txt")));
    }

    /// Tests a rename path with no parent directory
    #[test]
    fn test_build_rename_path_no_parent() {
        let original = PathBuf::from("old.txt");
        let result = build_rename_path(&original, "new.txt");
        assert_eq!(result, Some(PathBuf::from("new.txt")));
    }

    /// Tests that a rename path normalizes backslashes
    #[test]
    fn test_build_rename_path_normalizes() {
        let original = PathBuf::from("/home/user/old.txt");
        let result = build_rename_path(&original, "new.txt").unwrap();
        assert!(!result.to_string_lossy().contains('\\'));
    }

    /// Tests that a rename path rejects path injection
    #[test]
    fn test_build_rename_path_rejects_traversal() {
        let original = PathBuf::from("/home/user/old.txt");
        assert_eq!(build_rename_path(&original, "../etc/passwd"), None);
        assert_eq!(build_rename_path(&original, "/etc/passwd"), None);
        assert_eq!(build_rename_path(&original, "sub/name"), None);
        assert_eq!(build_rename_path(&original, ""), None);
    }

    // ============================================================
    // build_new_folder_path tests
    // ============================================================

    /// Tests building a new-folder path
    #[test]
    fn test_build_new_folder_path_basic() {
        let parent = PathBuf::from("/home/user");
        let result = build_new_folder_path(&parent, "new_dir");
        assert_eq!(result, Some(PathBuf::from("/home/user/new_dir")));
    }

    /// Tests that a new-folder path normalizes backslashes
    #[test]
    fn test_build_new_folder_path_normalizes() {
        let parent = PathBuf::from("/home/user");
        let result = build_new_folder_path(&parent, "test").unwrap();
        assert!(!result.to_string_lossy().contains('\\'));
    }

    /// Tests that a new-folder path rejects path injection
    #[test]
    fn test_build_new_folder_path_rejects_traversal() {
        let parent = PathBuf::from("/home/user");
        assert_eq!(build_new_folder_path(&parent, "../etc"), None);
        assert_eq!(build_new_folder_path(&parent, "/etc"), None);
        assert_eq!(build_new_folder_path(&parent, "sub/name"), None);
        assert_eq!(build_new_folder_path(&parent, ""), None);
    }

    // ============================================================
    // build_upload_remote_path tests
    // ============================================================

    /// Tests building an upload remote path
    #[test]
    fn test_build_upload_remote_path_basic() {
        let current = PathBuf::from("/home/user");
        let result = build_upload_remote_path(&current, "upload.txt");
        assert_eq!(result, Some(PathBuf::from("/home/user/upload.txt")));
    }

    /// Tests that an upload remote path normalizes backslashes
    #[test]
    fn test_build_upload_remote_path_normalizes() {
        let current = PathBuf::from("/home/user");
        let result = build_upload_remote_path(&current, "file.txt");
        assert!(result.is_some());
        assert!(!result.unwrap().to_string_lossy().contains('\\'));
    }

    /// Tests that an upload remote path rejects dangerous filenames
    #[test]
    fn test_build_upload_remote_path_rejects_dangerous() {
        let current = PathBuf::from("/home/user");
        // file_name() extracts "passwd" from "../etc/passwd", so the path is safe
        assert_eq!(
            build_upload_remote_path(&current, "../etc/passwd"),
            Some(PathBuf::from("/home/user/passwd"))
        );
        assert_eq!(build_upload_remote_path(&current, ""), None);
        // file_name() extracts "passwd" from "/etc/passwd", so the path is safe
        assert_eq!(
            build_upload_remote_path(&current, "/etc/passwd"),
            Some(PathBuf::from("/home/user/passwd"))
        );
    }

    // ============================================================
    // SftpBrowserAction enum tests
    // ============================================================

    /// Tests the SftpBrowserAction::CancelTransfer variant
    #[test]
    fn test_action_cancel_transfer() {
        let action = SftpBrowserAction::CancelTransfer(42);
        assert!(matches!(action, SftpBrowserAction::CancelTransfer(42)));
    }

    /// Tests the SftpBrowserAction::ConfirmMove variant
    #[test]
    fn test_action_confirm_move() {
        let action = SftpBrowserAction::ConfirmMove;
        assert!(matches!(action, SftpBrowserAction::ConfirmMove));
    }

    /// Tests the SftpBrowserAction::SetSearchFilter variant
    #[test]
    fn test_action_set_search_filter() {
        let action = SftpBrowserAction::SetSearchFilter("test".into());
        assert!(matches!(action, SftpBrowserAction::SetSearchFilter(_)));
    }

    /// Tests the SftpBrowserAction::ClearSearchFilter variant
    #[test]
    fn test_action_clear_search_filter() {
        let action = SftpBrowserAction::ClearSearchFilter;
        assert!(matches!(action, SftpBrowserAction::ClearSearchFilter));
    }

    /// Tests the SftpBrowserAction::DownloadSaveAs variant
    #[test]
    fn test_action_download_save_as() {
        let action = SftpBrowserAction::DownloadSaveAs {
            index: 3,
            local_path: "/tmp/file.txt".into(),
        };
        assert!(matches!(
            action,
            SftpBrowserAction::DownloadSaveAs { index: 3, .. }
        ));
    }
}
