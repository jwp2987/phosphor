use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use warpui_core::{AppContext, Entity, ModelContext, ModelHandle, SingletonEntity};

use crate::config::LanguageId;
use crate::model::LanguageServerId;
use crate::supported_servers::LSPServerType;
use crate::{LspEvent, LspServerConfig, LspServerModel};

#[derive(Debug)]
pub enum LspManagerModelEvent {
    /// ServerStarted is fired when the server is successfully started and reports ready status.
    /// ServerStopped is fired when the server has completed its shutdown.
    /// Both are routed from individual LspServerModel events.
    ServerStarted(PathBuf),
    ServerStopped(PathBuf),
    /// ServerRemoved is fired when a server is removed from the manager.
    /// This happens when the user explicitly removes the server (e.g., from settings or footer menu).
    /// Subscribers should drop their references to the server model.
    /// Contains the workspace path, server type, and the unique server ID.
    ServerRemoved {
        workspace_root: PathBuf,
        server_type: LSPServerType,
        server_id: LanguageServerId,
    },
}

#[derive(Default)]
pub struct LspManagerModel {
    /// Map from workspace root path to server info
    servers: HashMap<PathBuf, Vec<ModelHandle<LspServerModel>>>,
    /// Map from external file paths to the LSP server that should handle them.
    /// This is populated when navigating to definitions in files outside the workspace.
    external_file_servers: HashMap<PathBuf, LanguageServerId>,
    /// Set once [`Self::terminate_for_app_exit`] has run.
    terminated_for_app_exit: bool,
}

impl LspManagerModel {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            external_file_servers: HashMap::new(),
            terminated_for_app_exit: false,
        }
    }

    /// Returns an iterator over all workspace root paths that currently have an LSP server.
    pub fn workspace_roots(&self) -> impl Iterator<Item = &PathBuf> {
        self.servers.keys()
    }

    /// Returns the server handles for a given workspace root path.
    pub fn servers_for_workspace(&self, path: &Path) -> Option<&Vec<ModelHandle<LspServerModel>>> {
        self.servers.get(path)
    }

    /// Returns true if a server of the given type is already registered for this workspace.
    /// This is used to prevent duplicate registrations.
    pub fn server_registered(
        &self,
        path: &Path,
        server_type: LSPServerType,
        ctx: &AppContext,
    ) -> bool {
        let Some(servers) = self.servers.get(path) else {
            return false;
        };

        for server in servers {
            if server.as_ref(ctx).server_type() == server_type {
                return true;
            }
        }

        false
    }

    pub fn server_registered_and_started(
        &self,
        path: &Path,
        server_type: LSPServerType,
        ctx: &AppContext,
    ) -> bool {
        let Some(servers) = self.servers.get(path) else {
            return false;
        };

        for server in servers {
            if server.as_ref(ctx).server_type() == server_type {
                return server.as_ref(ctx).has_started();
            }
        }

        false
    }

    pub fn server_for_path(
        &self,
        path: &Path,
        ctx: &AppContext,
    ) -> Option<ModelHandle<LspServerModel>> {
        // Resolve the language ID - early return if unknown
        let path_lang = LanguageId::from_path(path)?;

        // First check if this is an external file that was registered via goto-definition
        if let Some(server_id) = self.external_file_servers.get(path)
            && let Some(server) = self.server_by_id(*server_id, ctx)
        {
            // Validate that the server supports this file's language
            if server.as_ref(ctx).supports_language(&path_lang) {
                return Some(server);
            }
            log::debug!(
                "External file server for {} does not support language {:?}, falling back to workspace lookup",
                path.display(),
                path_lang
            );
        }

        // Then try workspace-based lookup
        let lsp_model = self.lsp_model_for_path(path)?;

        for server in lsp_model {
            let supported = server.as_ref(ctx).supports_language(&path_lang);

            if supported {
                return Some(server.clone());
            }
        }

        log::debug!(
            "LSP server found for path: {}, but language does not match",
            path.display()
        );

        None
    }

    /// Registers an external file (outside any workspace) to be handled by a specific LSP server.
    /// This is called when navigating to a definition in an external file.
    pub fn maybe_register_external_file(&mut self, path: &Path, server_id: LanguageServerId) {
        // Skip registration if the path is already under an existing workspace scope
        if self.lsp_model_for_path(path).is_some() {
            log::debug!(
                "Skipping external file registration for {} - already under workspace scope",
                path.display()
            );
            return;
        }

        self.external_file_servers
            .insert(path.to_path_buf(), server_id);
    }

    /// Finds an LSP server by its unique ID.
    pub fn server_by_id(
        &self,
        id: LanguageServerId,
        ctx: &AppContext,
    ) -> Option<ModelHandle<LspServerModel>> {
        self.servers
            .values()
            .flatten()
            .find(|server| server.as_ref(ctx).id() == id)
            .cloned()
    }

    /// Register a new LSP server at the given path.
    /// Returns false if a server of the same type is already registered for this workspace.
    pub fn register(
        &mut self,
        path: PathBuf,
        config: LspServerConfig,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        // Check if a server of the same type is already registered for this workspace.
        if self.server_registered(&path, config.server_type(), ctx) {
            log::debug!(
                "LSP server {} already registered for path: {}",
                config.server_type().binary_name(),
                path.display()
            );
            return false;
        }

        log::info!("Registering LSP server for path: {}", path.display());

        let lsp = ctx.add_model(|_| LspServerModel::new(config));

        let path_clone = path.clone();
        // Seam: warp's `ModelContext::subscribe_to_model` passes the emitting
        // `ModelHandle` as the second callback argument; this fork's does not.
        // Warp ignored that argument here, so dropping it changes nothing.
        ctx.subscribe_to_model(&lsp, move |_, event, ctx| match event {
            LspEvent::Started => {
                ctx.emit(LspManagerModelEvent::ServerStarted(path_clone.clone()));
            }
            LspEvent::Stopped => {
                ctx.emit(LspManagerModelEvent::ServerStopped(path_clone.clone()));
            }
            _ => {}
        });

        self.servers.entry(path).or_default().push(lsp);
        true
    }

    pub fn start_all(&mut self, path: PathBuf, ctx: &mut ModelContext<Self>) {
        let Some(servers) = self.servers.get(&path) else {
            log::warn!(
                "No server registered for startup at path: {}",
                path.display()
            );
            return;
        };

        for server in servers.iter() {
            // Skip servers that were manually stopped by the user
            if !server.as_ref(ctx).can_auto_start() {
                log::info!(
                    "Skipping auto-start for manually stopped LSP server at path: {}",
                    path.display()
                );
                continue;
            }

            let result = server.update(ctx, |server, ctx| server.start(ctx));

            if let Err(e) = &result {
                log::warn!(
                    "Failed to start LSP server at path: {}: {e}",
                    path.display()
                );
            }
        }
    }

    pub fn stop_all(&mut self, path: PathBuf, ctx: &mut ModelContext<Self>) {
        let Some(servers) = self.servers.get(&path) else {
            log::warn!("No server registered to stop at path: {}", path.display());
            return;
        };

        for server in servers {
            let result = server.update(ctx, |server, ctx| server.stop(false, ctx));

            if let Err(e) = &result {
                log::warn!("Failed to stop LSP server at path: {}: {e}", path.display())
            }
        }
    }

    /// Removes a specific LSP server from the manager.
    /// This stops the server and removes it from the internal HashMap.
    /// Emits a ServerRemoved event so subscribers can drop their references.
    pub fn remove_server(
        &mut self,
        workspace_root: &Path,
        server_type: LSPServerType,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(servers) = self.servers.get_mut(workspace_root) else {
            log::warn!(
                "No server registered to remove at path: {}",
                workspace_root.display()
            );
            return;
        };

        // Find and remove the server with matching type, capturing its ID first
        let mut removed_server_id: Option<LanguageServerId> = None;
        servers.retain(|server| {
            let server_ref = server.as_ref(ctx);
            if server_ref.server_type() == server_type {
                // Capture the server ID before removing
                removed_server_id = Some(server_ref.id());
                // Always attempt to stop the server before removing (manually_stopped = true).
                // The stop() method handles state checks internally.
                let _ = server.update(ctx, |s, ctx| s.stop(true, ctx));
                false // Remove from vec
            } else {
                true // Keep in vec
            }
        });

        // Clean up empty entries
        if servers.is_empty() {
            self.servers.remove(workspace_root);
        }

        if let Some(server_id) = removed_server_id {
            log::info!(
                "Removed {} LSP server for {}",
                server_type.binary_name(),
                workspace_root.display()
            );
            ctx.emit(LspManagerModelEvent::ServerRemoved {
                workspace_root: workspace_root.to_path_buf(),
                server_type,
                server_id,
            });
        }
    }

    /// Terminate all LSP servers for all workspaces.
    /// This should be called during app shutdown.
    pub fn terminate(&mut self, ctx: &mut ModelContext<Self>) {
        log::info!(
            "Terminating all LSP servers for {} workspaces",
            self.servers.len()
        );
        let workspace_roots: Vec<_> = self.workspace_roots().cloned().collect();
        for root in workspace_roots {
            log::debug!(
                "Shutting down LSP servers for workspace: {}",
                root.display()
            );
            self.stop_all(root, ctx);
        }
    }

    /// Terminate all LSP servers for app exit, blocking the calling thread until every
    /// shutdown has finished or `grace` has elapsed, whichever comes first.
    ///
    /// [`Self::terminate`] only *spawns* the shutdowns, and the desktop app calls
    /// `std::process::exit` as soon as `on_will_terminate` returns, so without a wait the
    /// LSP `shutdown` request may never even be written. `grace` is a hard upper bound: a
    /// wedged server cannot hang quit, it merely loses the rest of its graceful shutdown and
    /// is left to notice stdin EOF when this process exits. The shutdowns run on the
    /// background executor, so blocking the main thread here cannot deadlock them.
    ///
    /// Returns how many servers finished shutting down within `grace`. This is
    /// [`Self::begin_terminate_for_app_exit`] followed by [`LspAppExitShutdown::wait`].
    pub fn terminate_for_app_exit(
        &mut self,
        grace: Duration,
        ctx: &mut ModelContext<Self>,
    ) -> usize {
        self.begin_terminate_for_app_exit(ctx).wait(grace)
    }

    /// Starts shutting down every LSP server for app exit, without waiting; wait with
    /// [`LspAppExitShutdown::wait`]. Lets the app start these shutdowns early and wait
    /// for them later, after its other teardown, against one shared deadline.
    pub fn begin_terminate_for_app_exit(
        &mut self,
        ctx: &mut ModelContext<Self>,
    ) -> LspAppExitShutdown {
        self.terminated_for_app_exit = true;

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let mut pending = 0usize;
        for servers in self.servers.values() {
            for server in servers {
                let done = done_tx.clone();
                if server.update(ctx, |server, _| {
                    server.terminate_with_completion(Some(done))
                }) {
                    pending += 1;
                }
            }
        }
        drop(done_tx);

        if pending > 0 {
            log::info!("Terminating {pending} running LSP server(s) for app exit");
        }
        LspAppExitShutdown {
            done: done_rx,
            pending,
        }
    }

    /// Whether [`Self::terminate_for_app_exit`] has run.
    pub fn terminated_for_app_exit(&self) -> bool {
        self.terminated_for_app_exit
    }

    /// Given a path, return the path of the registered LSP workspace for that path, if any
    pub fn lsp_model_for_path(&self, path: &Path) -> Option<&[ModelHandle<LspServerModel>]> {
        for ancestor in path.ancestors() {
            if let Some(servers) = self.servers.get(ancestor) {
                return Some(servers);
            }
        }
        None
    }

    #[cfg(target_arch = "wasm32")]
    pub fn repo_path_for_path(_path: &Path, _ctx: &AppContext) -> Option<PathBuf> {
        None
    }
}

/// LSP shutdowns started by [`LspManagerModel::begin_terminate_for_app_exit`].
#[must_use = "call `wait` to give the servers time to shut down"]
pub struct LspAppExitShutdown {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    done: std::sync::mpsc::Receiver<()>,
    pending: usize,
}

impl LspAppExitShutdown {
    /// How many shutdowns were started.
    pub fn pending(&self) -> usize {
        self.pending
    }

    /// Blocks until every shutdown has finished or `grace` has elapsed, whichever is
    /// first, and returns how many finished.
    pub fn wait(self, grace: Duration) -> usize {
        if self.pending == 0 {
            return 0;
        }
        // wasm has no LSP processes (starting one fails there) and cannot block its
        // main thread, so it never waits.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let finished = wait_for_shutdowns(&self.done, self.pending, grace);
            if finished < self.pending {
                log::warn!(
                    "{} of {} LSP server(s) did not finish shutting down within {grace:?}; \
                     exiting anyway",
                    self.pending - finished,
                    self.pending
                );
            }
            finished
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = grace;
            0
        }
    }
}

/// Blocks until `pending` completions arrive on `done` or `grace` elapses, whichever is
/// first, and returns how many arrived. Also returns early if every sender is dropped.
#[cfg(not(target_arch = "wasm32"))]
fn wait_for_shutdowns(
    done: &std::sync::mpsc::Receiver<()>,
    pending: usize,
    grace: Duration,
) -> usize {
    let deadline = std::time::Instant::now() + grace;
    let mut finished = 0usize;
    while finished < pending {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match done.recv_timeout(remaining) {
            Ok(()) => finished += 1,
            Err(_) => break,
        }
    }
    finished
}

impl Entity for LspManagerModel {
    type Event = LspManagerModelEvent;
}

impl SingletonEntity for LspManagerModel {}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
