//! TUI-facing MCP catalog for the headless `warp_tui` front-end (Zap / BYOP).
//!
//! Ported from upstream Warp's `tui/mcp.rs`, adapted to Zap's MCP stack. There
//! is one [`TuiMcpManager`] singleton for the TUI process (not one per server);
//! its [`TuiMcpSnapshot`] joins every locally-known MCP definition with its
//! runtime state, and per-server actions are routed by [`TuiMcpServerId`].
//!
//! The snapshot draws on three local sources:
//! - **installations** — templates installed on this device
//!   ([`TemplatableMCPServerManager::get_installed_templatable_servers`]);
//! - **saved templates** — locally stored templatable definitions with no
//!   installation, surfaced as [`TuiMcpServerStatus::Available`] and installable
//!   through [`TuiMcpManager::install_and_enable`];
//! - **file configs** — every provider Zap watches (Zap, Claude, Codex, other
//!   agents), at both global and project scope, each row labelled with the
//!   config files that define it.
//!
//! Refreshing is a pure read: nothing here installs, starts, or begins OAuth.
//! An available entry becomes runnable only through `install_and_enable`, after
//! the frontend has collected any required template variables.
//!
//! BYOP / Zap adaptations vs. upstream:
//! - Upstream's fourth source, the Warp-hosted **MCP gallery**, is absent.
//!   `MCPGalleryManager` is a deliberately gutted stub in this fork (localization
//!   phase 2d-2: it keeps no items and has no cloud fetch), so a gallery branch
//!   here would be dead code and gallery-derived tests would assert nothing.
//!   See `DECLINED.md`.
//! - Upstream distinguishes a synced template that arrived *from another device*
//!   from one *shared by a team member*, via `is_server_template_shared` and
//!   `get_creator`. Both resolve through cloud object spaces and account user
//!   profiles, so in BYOP the first is always false and the second always `None`.
//!   The provenance enum collapses to a single local [`TuiMcpServerSource::Template`].
//! - `active_mcp_config_file_path()` (Warp's managed-paths watcher, absent in
//!   Zap) is not needed: the snapshot reports one diagnostic per unhealthy
//!   config file rather than the health of a single config path.
//! - OAuth reopen: Zap does not retain a reopenable per-server authorization
//!   URL, so `authorization_url` is always `None` (the `Authenticating` row
//!   shows status without a reopen action). Log-out availability uses Zap's
//!   [`TemplatableMCPServerManager::can_log_out`], since file-based OAuth
//!   credentials are stored per installation hash here, not per template UUID.
//! - `resource_count` is `0`: Zap's runtime manager exposes tools but not a
//!   per-server resource count (the field is unused by the front-end).
//! - The `ReloadConfig` action is dropped (the front-end never issues it and
//!   Zap's file watcher exposes no manual reload).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;

use uuid::Uuid;
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::ai::mcp::parsing::resolve_json;
use crate::ai::mcp::templatable_manager::TemplatableMCPServerManagerEvent;
use crate::ai::mcp::{
    FileBasedMCPManager, FileBasedMCPServerScope, MCPServer, MCPServerState, TemplatableMCPServer,
    TemplatableMCPServerInstallation, TemplatableMCPServerManager, TransportType, VariableType,
    VariableValue,
};

/// Identifies a catalog row across refreshes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TuiMcpServerId {
    /// Stable content hash. File-based installation UUIDs are regenerated on
    /// every config parse, so they cannot preserve selection across reloads.
    FileBased(u64),
    Installation(Uuid),
    /// A locally saved templatable definition that is not installed here.
    Template(Uuid),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiMcpTransport {
    Stdio,
    HttpOrSse,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TuiMcpFileScope {
    Global,
    Project,
}

/// One config file that defines a file-based server.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TuiMcpFileSource {
    pub provider: String,
    pub root_path: PathBuf,
    pub scope: TuiMcpFileScope,
}

/// Where a catalog row came from, used for its source label and for search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiMcpServerSource {
    FileBased { sources: Vec<TuiMcpFileSource> },
    Installation,
    Template,
}

impl TuiMcpServerSource {
    pub fn label(&self) -> String {
        match self {
            Self::Installation => "CLI local".to_owned(),
            Self::Template => "saved template".to_owned(),
            Self::FileBased { sources } => {
                let labels = sources
                    .iter()
                    .map(|source| match source.scope {
                        TuiMcpFileScope::Global => format!("{} global", source.provider),
                        TuiMcpFileScope::Project => {
                            let root = source
                                .root_path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("project");
                            format!("{} · {root}", source.provider)
                        }
                    })
                    .collect::<Vec<_>>();
                if labels.is_empty() {
                    "file config".to_owned()
                } else {
                    labels.join(", ")
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiMcpServerStatus {
    /// Known but not installed. Enabling it installs and starts it.
    Available,
    Offline,
    Starting,
    Authenticating,
    Running,
    Stopping,
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiMcpServerSnapshot {
    pub id: TuiMcpServerId,
    /// `None` for an available entry, which has no installation yet.
    pub installation_uuid: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub source: TuiMcpServerSource,
    /// `None` when the definition's JSON does not parse to exactly one server.
    pub transport: Option<TuiMcpTransport>,
    pub status: TuiMcpServerStatus,
    pub tool_count: usize,
    pub resource_count: usize,
    /// Whether the `/mcp` menu should offer the Ctrl+R "log out & remove
    /// credentials" secondary action for this row.
    pub can_log_out: bool,
    pub authorization_url: Option<String>,
}

/// One config file that failed to read or parse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiMcpConfigDiagnostic {
    pub provider: String,
    pub config_path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TuiMcpSnapshot {
    pub diagnostics: Vec<TuiMcpConfigDiagnostic>,
    pub servers: Vec<TuiMcpServerSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiMcpTemplateVariable {
    pub key: String,
    pub allowed_values: Option<Vec<String>>,
}

/// What the frontend must collect before an available entry can be installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiMcpInstallRequest {
    pub id: TuiMcpServerId,
    pub name: String,
    pub variables: Vec<TuiMcpTemplateVariable>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct TuiMcpVariableValue {
    pub key: String,
    pub value: String,
}

impl fmt::Debug for TuiMcpVariableValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TuiMcpVariableValue")
            .field("key", &self.key)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiMcpAction {
    Enable(TuiMcpServerId),
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

/// TUI-facing aggregate over installed, saved-template, and file-based MCPs.
///
/// Refreshing this model is a pure read. Available catalog items become
/// runnable only through [`Self::install_and_enable`], after the frontend has
/// collected any required values.
pub struct TuiMcpManager {
    snapshot: TuiMcpSnapshot,
}

impl TuiMcpManager {
    /// Creates an empty MCP catalog for frontend tests.
    #[cfg(any(test, all(feature = "tui", feature = "test-util")))]
    pub(crate) fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            snapshot: TuiMcpSnapshot::default(),
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
            |me, event, ctx| {
                // Every runtime event can change a row's status, its install
                // state, or the set of saved templates, so all of them refresh.
                match event {
                    TemplatableMCPServerManagerEvent::StateChanged { .. }
                    | TemplatableMCPServerManagerEvent::ServerInstallationAdded(_)
                    | TemplatableMCPServerManagerEvent::ServerInstallationDeleted(_)
                    | TemplatableMCPServerManagerEvent::TemplatableMCPServersUpdated
                    | TemplatableMCPServerManagerEvent::LegacyServerConverted => {}
                }
                me.refresh(ctx);
            },
        );

        let mut model = Self {
            snapshot: TuiMcpSnapshot::default(),
        };
        model.refresh(ctx);
        model
    }

    pub fn snapshot(&self) -> &TuiMcpSnapshot {
        &self.snapshot
    }

    /// Describes what must be collected before `id` can be installed. Pure: it
    /// neither installs nor starts anything.
    pub fn prepare_install(
        &self,
        id: TuiMcpServerId,
        ctx: &ModelContext<Self>,
    ) -> Result<TuiMcpInstallRequest, String> {
        let server = self.template_to_install(id, ctx)?;
        Ok(TuiMcpInstallRequest {
            id,
            name: server.name,
            variables: server
                .template
                .variables
                .into_iter()
                .map(|variable| TuiMcpTemplateVariable {
                    key: variable.key,
                    allowed_values: variable.allowed_values,
                })
                .collect(),
        })
    }

    /// Installs and starts an available template after the frontend collected
    /// its values. Catalog refresh and selection never call this method.
    pub fn install_and_enable(
        &mut self,
        id: TuiMcpServerId,
        values: Vec<TuiMcpVariableValue>,
        ctx: &mut ModelContext<Self>,
    ) -> Result<Uuid, String> {
        let request = self.prepare_install(id, ctx)?;
        let values = validate_variable_values(&request.variables, values)?;
        let server = self.template_to_install(id, ctx)?;
        let installation = TemplatableMCPServerManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.install_from_template(server, values, true, ctx)
        });
        let installation =
            installation.ok_or_else(|| "Unable to install this MCP server".to_owned())?;
        let uuid = installation.uuid();
        self.refresh(ctx);
        Ok(uuid)
    }

    /// Resolves `id` to the saved template it would install, rejecting ids that
    /// are already installed or no longer available.
    fn template_to_install(
        &self,
        id: TuiMcpServerId,
        ctx: &ModelContext<Self>,
    ) -> Result<TemplatableMCPServer, String> {
        if !self
            .snapshot
            .servers
            .iter()
            .any(|server| server.id == id && matches!(server.status, TuiMcpServerStatus::Available))
        {
            return Err("This MCP is no longer available to enable".to_owned());
        }
        match id {
            TuiMcpServerId::Template(template_uuid) => TemplatableMCPServerManager::as_ref(ctx)
                .get_templatable_mcp_server(template_uuid)
                .cloned()
                .ok_or_else(|| "The saved MCP template is no longer available".to_owned()),
            TuiMcpServerId::FileBased(_) | TuiMcpServerId::Installation(_) => {
                Err("This MCP is already installed".to_owned())
            }
        }
    }

    pub fn apply_action(&mut self, action: TuiMcpAction, ctx: &mut ModelContext<Self>) {
        match action {
            // Enabling is a multi-step frontend flow (collect values, confirm),
            // which drives `prepare_install` / `install_and_enable` directly.
            TuiMcpAction::Enable(_) => {}
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
            TuiMcpAction::Start(id) | TuiMcpAction::Retry(id) => match id {
                TuiMcpServerId::FileBased(hash) => {
                    let installation = FileBasedMCPManager::as_ref(ctx)
                        .installation_by_hash(hash)
                        .cloned();
                    if let Some(installation) = installation {
                        TemplatableMCPServerManager::handle(ctx).update(ctx, |manager, ctx| {
                            if !manager.is_server_active_or_pending(installation.uuid()) {
                                manager.spawn_ephemeral_server(installation, ctx);
                            }
                        });
                    }
                }
                TuiMcpServerId::Installation(uuid) => {
                    TemplatableMCPServerManager::handle(ctx).update(ctx, |manager, ctx| {
                        if !manager.is_server_active_or_pending(uuid) {
                            manager.spawn_server(uuid, ctx);
                        }
                    });
                }
                TuiMcpServerId::Template(_) => {}
            },
            TuiMcpAction::Stop(id) | TuiMcpAction::LogOut(id) => {
                let installation_uuid = match id {
                    TuiMcpServerId::FileBased(hash) => FileBasedMCPManager::as_ref(ctx)
                        .installation_by_hash(hash)
                        .map(TemplatableMCPServerInstallation::uuid),
                    TuiMcpServerId::Installation(uuid) => Some(uuid),
                    TuiMcpServerId::Template(_) => None,
                };
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
        let file_manager = FileBasedMCPManager::as_ref(ctx);
        let runtime_manager = TemplatableMCPServerManager::as_ref(ctx);

        let mut diagnostics = file_manager
            .config_diagnostics()
            .into_iter()
            .map(|diagnostic| TuiMcpConfigDiagnostic {
                provider: diagnostic.provider.display_name().to_owned(),
                config_path: diagnostic.config_path,
                message: diagnostic.message,
            })
            .collect::<Vec<_>>();
        diagnostics.sort_by(|left, right| {
            left.provider
                .cmp(&right.provider)
                .then(left.config_path.cmp(&right.config_path))
                .then(left.message.cmp(&right.message))
        });

        let mut servers = Vec::new();
        let installations = runtime_manager
            .get_installed_templatable_servers()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let installed_template_uuids = installations
            .iter()
            .map(TemplatableMCPServerInstallation::template_uuid)
            .collect::<HashSet<_>>();
        // File configs and saved templates can describe the same server. Skip a
        // template whose JSON is byte-for-byte the same server as one a global
        // Zap config already provides, so it is not listed twice.
        let global_warp_server_identities = file_manager
            .file_based_servers_with_sources()
            .into_iter()
            .filter(|server| {
                server.sources.iter().any(|source| {
                    source.scope == FileBasedMCPServerScope::Global
                        && source.provider == crate::ai::mcp::MCPProvider::Zap
                })
            })
            .filter_map(|server| template_identity(server.installation.templatable_mcp_server()))
            .collect::<HashSet<_>>();

        for installation in installations {
            servers.push(snapshot_for_installation(
                TuiMcpServerId::Installation(installation.uuid()),
                TuiMcpServerSource::Installation,
                &installation,
                None,
                runtime_manager,
            ));
        }

        for template in runtime_manager
            .get_all_templatable_mcp_servers()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
        {
            if installed_template_uuids.contains(&template.uuid) {
                continue;
            }
            if is_represented_by_global_warp_server(&template, &global_warp_server_identities) {
                continue;
            }
            servers.push(snapshot_for_available(
                TuiMcpServerId::Template(template.uuid),
                TuiMcpServerSource::Template,
                &template,
            ));
        }

        for file_server in file_manager.file_based_servers_with_sources() {
            let installation = file_server.installation;
            let Some(hash) = installation.hash() else {
                continue;
            };
            let mut sources = file_server
                .sources
                .into_iter()
                .map(|source| TuiMcpFileSource {
                    provider: source.provider.display_name().to_owned(),
                    root_path: source.root_path,
                    scope: match source.scope {
                        FileBasedMCPServerScope::Global => TuiMcpFileScope::Global,
                        FileBasedMCPServerScope::Project => TuiMcpFileScope::Project,
                    },
                })
                .collect::<Vec<_>>();
            sources.sort();
            sources.dedup();
            servers.push(snapshot_for_installation(
                TuiMcpServerId::FileBased(hash),
                TuiMcpServerSource::FileBased { sources },
                &installation,
                Some(hash),
                runtime_manager,
            ));
        }

        sort_servers(&mut servers);
        let snapshot = TuiMcpSnapshot {
            diagnostics,
            servers,
        };
        if self.snapshot != snapshot {
            self.snapshot = snapshot;
            ctx.emit(TuiMcpManagerEvent::Updated);
            ctx.notify();
        }
    }
}

fn validate_variable_values(
    variables: &[TuiMcpTemplateVariable],
    values: Vec<TuiMcpVariableValue>,
) -> Result<HashMap<String, VariableValue>, String> {
    let expected = variables
        .iter()
        .map(|variable| variable.key.as_str())
        .collect::<HashSet<_>>();
    if values.len() != expected.len() {
        return Err("Every required MCP variable must have a value".to_owned());
    }

    let mut resolved = HashMap::new();
    for value in values {
        if value.value.is_empty() || !expected.contains(value.key.as_str()) {
            return Err("Every required MCP variable must have a value".to_owned());
        }
        let variable = variables
            .iter()
            .find(|variable| variable.key == value.key)
            .expect("expected keys were checked");
        if variable
            .allowed_values
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(&value.value))
        {
            return Err("Select one of the allowed values for this MCP variable".to_owned());
        }
        if resolved
            .insert(
                value.key,
                VariableValue {
                    variable_type: VariableType::Text,
                    value: value.value,
                },
            )
            .is_some()
        {
            return Err("Each MCP variable may only be provided once".to_owned());
        }
    }
    Ok(resolved)
}

fn snapshot_for_available(
    id: TuiMcpServerId,
    source: TuiMcpServerSource,
    template: &TemplatableMCPServer,
) -> TuiMcpServerSnapshot {
    TuiMcpServerSnapshot {
        id,
        installation_uuid: None,
        name: template.name.clone(),
        description: template.description.clone(),
        source,
        transport: transport_from_template(template),
        status: TuiMcpServerStatus::Available,
        tool_count: 0,
        resource_count: 0,
        can_log_out: false,
        authorization_url: None,
    }
}

fn snapshot_for_installation(
    id: TuiMcpServerId,
    source: TuiMcpServerSource,
    installation: &TemplatableMCPServerInstallation,
    hash: Option<u64>,
    runtime_manager: &TemplatableMCPServerManager,
) -> TuiMcpServerSnapshot {
    let uuid = installation.uuid();
    TuiMcpServerSnapshot {
        id,
        installation_uuid: Some(uuid),
        name: installation.templatable_mcp_server().name.clone(),
        description: installation.templatable_mcp_server().description.clone(),
        source,
        transport: transport_from_installation(installation),
        status: runtime_status(uuid, runtime_manager),
        tool_count: runtime_manager.tools_for_server(uuid).len(),
        // Zap's runtime manager exposes tools but not a per-server resource
        // count; the front-end does not display it.
        resource_count: 0,
        can_log_out: runtime_manager.can_log_out(uuid, hash),
        // Zap retains no reopenable per-server authorization URL.
        authorization_url: None,
    }
}

/// The identity of the single MCP server a definition describes, used to
/// recognise the same server arriving from two sources. `None` when the JSON
/// does not describe exactly one server.
#[derive(Debug, Eq, Hash, PartialEq)]
enum TuiMcpServerIdentity {
    Stdio {
        name: String,
        command: String,
        args: Vec<String>,
        working_directory: Option<String>,
    },
    HttpOrSse {
        name: String,
        url: String,
    },
}

fn template_identity(template: &TemplatableMCPServer) -> Option<TuiMcpServerIdentity> {
    let mut servers = MCPServer::from_user_json(&template.template.json).ok()?;
    if servers.len() != 1 {
        return None;
    }
    let server = servers.pop()?;
    let name = server.name.to_ascii_lowercase();
    match server.transport_type {
        TransportType::CLIServer(server) => Some(TuiMcpServerIdentity::Stdio {
            name,
            command: server.command,
            args: server.args,
            working_directory: server.cwd_parameter,
        }),
        TransportType::ServerSentEvents(server) => Some(TuiMcpServerIdentity::HttpOrSse {
            name,
            url: server.url,
        }),
    }
}

fn is_represented_by_global_warp_server(
    template: &TemplatableMCPServer,
    global_warp_server_identities: &HashSet<TuiMcpServerIdentity>,
) -> bool {
    template_identity(template)
        .is_some_and(|identity| global_warp_server_identities.contains(&identity))
}

fn transport_from_template(template: &TemplatableMCPServer) -> Option<TuiMcpTransport> {
    MCPServer::from_user_json(&template.template.json)
        .ok()?
        .pop()
        .map(|server| transport_type(server.transport_type))
}

fn transport_from_installation(
    installation: &TemplatableMCPServerInstallation,
) -> Option<TuiMcpTransport> {
    MCPServer::from_user_json(&resolve_json(installation))
        .ok()?
        .pop()
        .map(|server| transport_type(server.transport_type))
}

fn transport_type(transport: TransportType) -> TuiMcpTransport {
    match transport {
        TransportType::CLIServer(_) => TuiMcpTransport::Stdio,
        TransportType::ServerSentEvents(_) => TuiMcpTransport::HttpOrSse,
    }
}

fn runtime_status(uuid: Uuid, runtime_manager: &TemplatableMCPServerManager) -> TuiMcpServerStatus {
    match runtime_manager.get_server_state(uuid) {
        None | Some(MCPServerState::NotRunning) => TuiMcpServerStatus::Offline,
        Some(MCPServerState::Starting) => TuiMcpServerStatus::Starting,
        Some(MCPServerState::Authenticating) => TuiMcpServerStatus::Authenticating,
        Some(MCPServerState::Running) => TuiMcpServerStatus::Running,
        Some(MCPServerState::ShuttingDown) => TuiMcpServerStatus::Stopping,
        Some(MCPServerState::FailedToStart) => TuiMcpServerStatus::Failed {
            message: runtime_manager
                .get_server_error_message(uuid)
                .unwrap_or("Failed to start")
                .to_owned(),
        },
    }
}

/// Installed servers sort ahead of merely available ones, then by name, then by
/// id so the order is total and stable across refreshes.
fn sort_servers(servers: &mut [TuiMcpServerSnapshot]) {
    servers.sort_by(|left, right| {
        server_priority(left)
            .cmp(&server_priority(right))
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
            .then(left.id.cmp(&right.id))
    });
}

fn server_priority(server: &TuiMcpServerSnapshot) -> u8 {
    match server.status {
        TuiMcpServerStatus::Available => 1,
        TuiMcpServerStatus::Offline
        | TuiMcpServerStatus::Starting
        | TuiMcpServerStatus::Authenticating
        | TuiMcpServerStatus::Running
        | TuiMcpServerStatus::Stopping
        | TuiMcpServerStatus::Failed { .. } => 0,
    }
}

impl Entity for TuiMcpManager {
    type Event = TuiMcpManagerEvent;
}

impl SingletonEntity for TuiMcpManager {}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
