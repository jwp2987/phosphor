//! TUI-facing MCP aggregate for the headless `warp_tui` front-end (Zap / BYOP).
//!
//! Ported from upstream Warp's `tui/mcp.rs`, adapted to Zap's MCP stack. There
//! is one [`TuiMcpManager`] singleton for the TUI process (not one per server);
//! its [`TuiMcpSnapshot`] joins file-config health with per-server runtime
//! state, and per-server actions are routed by [`TuiMcpServerId`] (an
//! installation hash).
//!
//! BYOP / Zap adaptations vs. upstream:
//! - `global_warp_servers()` → Zap's [`FileBasedMCPManager::file_based_servers`];
//!   `global_warp_installation_by_hash` → an inline hash lookup over them.
//! - `active_mcp_config_file_path()` (Warp's managed-paths watcher, absent in
//!   Zap) → the Zap home MCP config path.
//! - Config diagnostics: Zap's `FileBasedMCPManager` has no per-path diagnostic
//!   store, so `config_state` is `Missing`/`Ready` by file existence. The
//!   [`TuiMcpConfigState::Invalid`] variant is retained for source
//!   compatibility with the front-end but is never produced here.
//! - OAuth reopen: Zap does not retain a reopenable per-server authorization
//!   URL, so `authorization_url` is always `None` (the `Authenticating` row
//!   shows status without a reopen action). Log-out availability uses Zap's
//!   hash-keyed [`TemplatableMCPServerManager::can_log_out`], since file-based
//!   OAuth credentials are stored per installation hash here, not per template
//!   UUID as upstream.
//! - `resource_count` is `0`: Zap's runtime manager exposes tools but not a
//!   per-server resource count (the field is unused by the front-end).
//! - The `ReloadConfig` action is dropped (the front-end never issues it and
//!   Zap's file watcher exposes no manual reload).

use std::path::PathBuf;

use uuid::Uuid;
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::ai::mcp::parsing::resolve_json;
use crate::ai::mcp::templatable_manager::TemplatableMCPServerManagerEvent;
use crate::ai::mcp::{
    FileBasedMCPManager, MCPServer, MCPServerState, TemplatableMCPServerManager, TransportType,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TuiMcpServerId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiMcpTransport {
    Stdio,
    HttpOrSse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiMcpServerStatus {
    Offline,
    Starting,
    Authenticating,
    Running,
    Stopping,
    Failed { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiMcpServerSnapshot {
    pub id: TuiMcpServerId,
    pub installation_uuid: Uuid,
    pub name: String,
    pub transport: TuiMcpTransport,
    pub status: TuiMcpServerStatus,
    pub tool_count: usize,
    pub resource_count: usize,
    /// Whether the `/mcp` menu should offer the Ctrl+R "log out & remove
    /// credentials" secondary action for this row.
    pub can_log_out: bool,
    pub authorization_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiMcpConfigState {
    Missing,
    Ready,
    /// Retained for front-end source compatibility. Zap's `FileBasedMCPManager`
    /// has no per-path config diagnostics, so this is never produced.
    Invalid {
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiMcpSnapshot {
    pub config_path: PathBuf,
    pub config_state: TuiMcpConfigState,
    pub servers: Vec<TuiMcpServerSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiMcpAction {
    Start(TuiMcpServerId),
    Stop(TuiMcpServerId),
    Retry(TuiMcpServerId),
    LogOut(TuiMcpServerId),
    ReopenAuthorization(TuiMcpServerId),
}

#[derive(Clone, Copy, Debug)]
pub enum TuiMcpManagerEvent {
    Updated,
}

/// TUI-facing aggregate over every file-based MCP server.
///
/// One singleton model for the TUI process; its snapshot joins file-config
/// health with runtime state for all configured servers, and per-server
/// actions are routed by [`TuiMcpServerId`].
pub struct TuiMcpManager {
    snapshot: TuiMcpSnapshot,
}

impl TuiMcpManager {
    /// Creates an empty MCP aggregate for frontend tests.
    #[cfg(any(test, all(feature = "tui", feature = "test-util")))]
    pub(crate) fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            snapshot: TuiMcpSnapshot {
                config_path: PathBuf::new(),
                config_state: TuiMcpConfigState::Missing,
                servers: Vec::new(),
            },
        }
    }

    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // The file-based manager's event type differs between the real
        // (`local_fs`) and dummy builds, so ignore the payload and just refresh.
        ctx.subscribe_to_model(&FileBasedMCPManager::handle(ctx), |me, _, ctx| {
            me.refresh(ctx);
        });
        ctx.subscribe_to_model(
            &TemplatableMCPServerManager::handle(ctx),
            |me, event, ctx| match event {
                TemplatableMCPServerManagerEvent::StateChanged { .. }
                | TemplatableMCPServerManagerEvent::TemplatableMCPServersUpdated => me.refresh(ctx),
                TemplatableMCPServerManagerEvent::ServerInstallationAdded(_)
                | TemplatableMCPServerManagerEvent::ServerInstallationDeleted(_)
                | TemplatableMCPServerManagerEvent::LegacyServerConverted => {}
            },
        );

        let mut model = Self {
            snapshot: TuiMcpSnapshot {
                config_path: config_file_path(),
                config_state: TuiMcpConfigState::Missing,
                servers: Vec::new(),
            },
        };
        model.refresh(ctx);
        model
    }

    pub fn snapshot(&self) -> &TuiMcpSnapshot {
        &self.snapshot
    }

    pub fn apply_action(&mut self, action: TuiMcpAction, ctx: &mut ModelContext<Self>) {
        match action {
            TuiMcpAction::ReopenAuthorization(id) => {
                if let Some(url) = self
                    .snapshot
                    .servers
                    .iter()
                    .find(|server| server.id == id)
                    .and_then(|server| server.authorization_url.as_deref())
                {
                    ctx.open_url(url);
                }
            }
            TuiMcpAction::Start(id) | TuiMcpAction::Retry(id) => {
                let installation = installation_by_hash(id.0, ctx);
                if let Some(installation) = installation {
                    TemplatableMCPServerManager::handle(ctx).update(ctx, |manager, ctx| {
                        if !manager.is_server_active_or_pending(installation.uuid()) {
                            manager.spawn_ephemeral_server(installation, ctx);
                        }
                    });
                }
            }
            TuiMcpAction::Stop(id) | TuiMcpAction::LogOut(id) => {
                let installation_uuid =
                    installation_by_hash(id.0, ctx).map(|installation| installation.uuid());
                if let Some(installation_uuid) = installation_uuid {
                    TemplatableMCPServerManager::handle(ctx).update(ctx, |manager, ctx| {
                        manager.shutdown_server(installation_uuid, ctx);
                        if matches!(action, TuiMcpAction::LogOut(_)) {
                            manager.delete_credentials_from_secure_storage(installation_uuid, ctx);
                        }
                    });
                }
            }
        }
    }

    fn refresh(&mut self, ctx: &mut ModelContext<Self>) {
        let config_path = config_file_path();
        let file_manager = FileBasedMCPManager::as_ref(ctx);
        let runtime_manager = TemplatableMCPServerManager::as_ref(ctx);
        let config_state = if config_path.exists() {
            TuiMcpConfigState::Ready
        } else {
            TuiMcpConfigState::Missing
        };

        let mut servers = file_manager
            .file_based_servers()
            .into_iter()
            .filter_map(|installation| {
                let hash = installation.hash()?;
                let uuid = installation.uuid();
                let transport = MCPServer::from_user_json(&resolve_json(installation))
                    .ok()?
                    .pop()
                    .map(|server| match server.transport_type {
                        TransportType::CLIServer(_) => TuiMcpTransport::Stdio,
                        TransportType::ServerSentEvents(_) => TuiMcpTransport::HttpOrSse,
                    })?;
                let status = match runtime_manager.get_server_state(uuid) {
                    None | Some(MCPServerState::NotRunning) => TuiMcpServerStatus::Offline,
                    Some(MCPServerState::Starting) => TuiMcpServerStatus::Starting,
                    Some(MCPServerState::Authenticating) => TuiMcpServerStatus::Authenticating,
                    Some(MCPServerState::Running) => TuiMcpServerStatus::Running,
                    Some(MCPServerState::ShuttingDown) => TuiMcpServerStatus::Stopping,
                    Some(MCPServerState::FailedToStart) => TuiMcpServerStatus::Failed {
                        message: runtime_manager
                            .get_server_error_message(uuid)
                            .unwrap_or("Failed to start")
                            .to_string(),
                    },
                };
                Some(TuiMcpServerSnapshot {
                    id: TuiMcpServerId(hash),
                    installation_uuid: uuid,
                    name: installation.templatable_mcp_server().name.clone(),
                    transport,
                    status,
                    tool_count: runtime_manager.tools_for_server(uuid).len(),
                    // Zap's runtime manager exposes tools but not a per-server
                    // resource count; the front-end does not display it.
                    resource_count: 0,
                    can_log_out: runtime_manager.can_log_out(uuid, hash),
                    // Zap retains no reopenable per-server authorization URL.
                    authorization_url: None,
                })
            })
            .collect::<Vec<_>>();
        servers.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then(left.id.cmp(&right.id))
        });

        let snapshot = TuiMcpSnapshot {
            config_path,
            config_state,
            servers,
        };
        if self.snapshot != snapshot {
            self.snapshot = snapshot;
            ctx.emit(TuiMcpManagerEvent::Updated);
            ctx.notify();
        }
    }
}

impl Entity for TuiMcpManager {
    type Event = TuiMcpManagerEvent;
}

impl SingletonEntity for TuiMcpManager {}

/// The Zap home MCP config path (Warp's `active_mcp_config_file_path` analogue).
fn config_file_path() -> PathBuf {
    warp_core::paths::warp_home_mcp_config_file_path().unwrap_or_default()
}

/// Finds and clones the file-based installation whose hash matches `hash`
/// (Zap's stand-in for Warp's `global_warp_installation_by_hash`).
fn installation_by_hash(
    hash: u64,
    ctx: &warpui::AppContext,
) -> Option<crate::ai::mcp::TemplatableMCPServerInstallation> {
    FileBasedMCPManager::as_ref(ctx)
        .file_based_servers()
        .into_iter()
        .find(|installation| installation.hash() == Some(hash))
        .cloned()
}
