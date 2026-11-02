pub mod active_session;
pub mod command_executor;
mod git_branch_cache;

use async_channel::Sender;
pub use command_executor::*;

use anyhow::Result;
use futures::future::{BoxFuture, Shared};
use futures::FutureExt;
use instant::Instant;
use once_cell::sync::OnceCell;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
#[cfg(windows)]
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use typed_path::{TypedPath, TypedPathBuf, WindowsPath};
use warp_util::path::{
    convert_msys2_to_windows_native_path, convert_wsl_to_windows_host_path, msys2_exe_to_root,
    ShellDialect, ShellFamily,
};

use version_compare::Version;
use warp_completer::completer::{
    CommandExitStatus, CommandOutput, PathSeparators, TopLevelCommandCaseSensitivity,
};
use warpui::{platform::OperatingSystem, Entity, ModelContext, SingletonEntity};

#[cfg(feature = "local_tty")]
use crate::features::FeatureFlag;
#[cfg(feature = "local_tty")]
use crate::remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use crate::server::telemetry::{BootstrappingInfo, TelemetryEvent};
use crate::terminal::event::ExecutedExecutorCommandEvent;
use crate::terminal::ShellHost;
use crate::terminal::ShellLaunchData;
#[cfg(feature = "local_tty")]
use crate::remote_server::client::RemoteServerClient;
#[cfg(feature = "local_tty")]
use command_executor::remote_server_executor::RemoteServerCommandExecutor;
use parking_lot::{Mutex, RwLock};

use crate::terminal::shell::{Shell, ShellType};
use crate::terminal::warpify::SubshellSource;
use crate::terminal::History;

use super::ansi::{BootstrappedValue, InitShellValue, SSHValue};
use super::terminal_model::{HistoryEntry, SubshellInitializationInfo};
use crate::terminal::event::RemoteServerSetupState;

#[derive(thiserror::Error, Debug)]
pub enum ReadHistoryContentsError {
    #[cfg(windows)]
    #[error("Error running PowerShell commands to read history file: {0}")]
    PowerShellError(anyhow::Error),

    #[cfg(windows)]
    #[error("Error running PowerShell commands and reading from filesystem to read history file. PowerShell error: {powershell_error}, filesystem error: {async_fs_error}")]
    PowerShellAndAsyncFsError {
        powershell_error: anyhow::Error,
        async_fs_error: std::io::Error,
    },

    #[error("Error reading history file from filesystem: {0}")]
    AsyncFsError(std::io::Error),
}

/// Builds the PowerShell `ReadAllText` invocation for `path`, escaping the path so it cannot
/// terminate the single-quoted PowerShell string literal it is embedded in.
#[cfg(windows)]
fn powershell_read_all_text_command(path: &OsStr) -> OsString {
    let mut command = OsString::from("[System.IO.File]::ReadAllText('");
    command.push(escape_powershell_single_quotes(path));
    command.push("')");
    command
}

/// Doubles every single quote in `path` so it is safe to embed inside a
/// PowerShell single-quoted string literal.
#[cfg(windows)]
fn escape_powershell_single_quotes(path: &OsStr) -> OsString {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    const SINGLE_QUOTE: u16 = b'\'' as u16;
    let mut escaped = Vec::new();
    for unit in path.encode_wide() {
        escaped.push(unit);
        if unit == SINGLE_QUOTE {
            escaped.push(SINGLE_QUOTE);
        }
    }
    OsString::from_wide(&escaped)
}

// SessionId is defined in warp_core and re-exported here for backward compatibility.
pub use warp_core::SessionId;

/// Information about the sessions within a given terminal pane/top-level
/// shell.
///
/// This stores multiple sessions as each bootstrapped subshell is a separate
/// session (whether it's a true subshell or an SSH session).
#[derive(Debug)]
pub struct Sessions {
    /// The start time for pending sessions, keyed by the session's
    /// unique ID.
    pending_session_start_times: HashMap<SessionId, Instant>,

    /// The set of known sessions, keyed by the session's unique ID.
    sessions: HashMap<SessionId, Arc<Session>>,

    /// The sending side of a channel used by in-band command executors.
    executor_command_tx: Sender<ExecutorCommandEvent>,

    /// The sending side of channels used to distribute the results of
    /// in-band command execution, keyed by session ID.
    in_band_command_output_tx_map: HashMap<SessionId, Sender<ExecutedExecutorCommandEvent>>,

    /// An executor to use for all spawned sessions.
    ///
    /// This is only intended to be used in tests, which may want to use
    /// various mock executor types in order to test and assert on behaviors.
    executor_for_all_sessions: Option<Arc<dyn CommandExecutor>>,

    /// Select environment variables and their values.
    env_vars: HashMap<SessionId, HashMap<String, String>>,

    /// Tracks the remote server setup state for SSH sessions that have the
    /// `SshRemoteServer` feature flag enabled. Keyed by the pending session ID.
    remote_server_setup_states: HashMap<SessionId, RemoteServerSetupState>,
}

#[derive(Clone, Debug)]
pub struct SessionBootstrappedEvent {
    pub session_id: SessionId,
    pub spawning_command: String,
    pub shell: Shell,
    pub subshell_info: Option<SubshellInitializationInfo>,
    pub session_type: BootstrapSessionType,
}

/// Why a session settled into the legacy (ControlMaster-based) SSH executor
/// without the command-executor construction path ever reaching an explicit
/// [`crate::terminal::event::RemoteServerSetupState::Failed`] or `Unsupported`
/// terminal state -- see [`SessionsEvent::LegacySshFallback`]. Both of these
/// were previously `log::info!`-only (TODO.md "Remote-session setup degrades
/// silently", item 1's "default fully-silent case").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacySshFallbackReason {
    /// The `SshRemoteServer` feature flag is off entirely, so no attempt at
    /// the remote-server extension was ever made for this session.
    FeatureDisabled,
    /// The flag is on, but there is no connected remote-server client for
    /// this session -- e.g. the connection is still pending, or setup
    /// resolved to "no client" without recording a `Failed`/`Unsupported`
    /// state the UI can otherwise key on.
    NoConnectedClient,
}

/// Set of events produced the [`Sessions`] model.
#[derive(Clone, Debug)]
pub enum SessionsEvent {
    /// The session was initialized. This does not indicate that the session has bootstrapped, but
    /// only that we're aware of the beginning of a session that we will attempt to bootstrap.
    SessionInitialized { session_id: SessionId },
    /// A new session was successfully bootstrapped.
    SessionBootstrapped(Box<SessionBootstrappedEvent>),
    /// The environment variables were updated.
    EnvironmentVariablesUpdated { session_id: SessionId },
    /// A legacy SSH session's command executor was just constructed without
    /// ever reaching a connected remote-server client, and without an
    /// explicit `RemoteServerSetupState::Failed`/`Unsupported` state existing
    /// for it -- emitted from `new_command_executor_for_local_tty_session`,
    /// the exact site that used to only `log::info!` this (TODO.md
    /// "Remote-session setup degrades silently", item 1).
    LegacySshFallback {
        session_id: SessionId,
        reason: LegacySshFallbackReason,
    },
}

impl Entity for Sessions {
    type Event = SessionsEvent;
}

/// Waits between attempts to enumerate deferred function/builtin names after a transport error.
const DEFERRED_NAME_RETRY_DELAYS: [std::time::Duration; 2] = [
    std::time::Duration::from_millis(500),
    std::time::Duration::from_secs(3),
];

impl Sessions {
    pub fn new(
        executor_command_tx: Sender<ExecutorCommandEvent>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        // Track the connected host_id on the `Session` type so downstream
        // code can distinguish hosts. The `RemoteServerCommandExecutor`
        // client itself is baked in at session construction time
        // (see `new_command_executor_for_local_tty_session`) so we no
        // longer need to wire it here on connect/disconnect.
        #[cfg(feature = "local_tty")]
        if FeatureFlag::SshRemoteServer.is_enabled() {
            let mgr = RemoteServerManager::handle(ctx);
            ctx.subscribe_to_model(&mgr, |sessions, event, ctx| match event {
                RemoteServerManagerEvent::SessionConnected {
                    session_id: sid,
                    host_id,
                } => {
                    if let Some(session) = sessions.sessions.get(sid) {
                        session.set_remote_host_id(Some(host_id.clone()));
                        // #1172: the connect landed after the session was initialized (the
                        // bootstrap no longer waits for it): move it off the ControlMaster.
                        let client = RemoteServerManager::handle(ctx)
                            .read(ctx, |mgr, _| mgr.client_for_session(*sid).cloned());
                        if let Some(client) = client {
                            session.adopt_remote_server_client(client);
                        }
                    }
                    sessions.set_remote_host_id_for_descendants(*sid, Some(host_id.clone()));
                }
                RemoteServerManagerEvent::SessionDisconnected {
                    session_id: sid, ..
                } => {
                    if let Some(session) = sessions.sessions.get(sid) {
                        session.set_remote_host_id(None);
                        // The client is gone for good (or the ssh session is
                        // ending): never keep handing completions to it.
                        session.fall_back_to_control_master_executor();
                    }
                    sessions.set_remote_host_id_for_descendants(*sid, None);
                }
                RemoteServerManagerEvent::SessionConnectionLost {
                    session_id: sid, ..
                } => {
                    // The proxy died but the ssh session lives: keep
                    // completions/suggestions working over the ControlMaster
                    // while the manager reconnects in the background; the
                    // `SessionReconnected` arm below swaps back.
                    if let Some(session) = sessions.sessions.get(sid) {
                        session.set_remote_host_id(None);
                        session.fall_back_to_control_master_executor();
                    }
                    sessions.set_remote_host_id_for_descendants(*sid, None);
                }
                RemoteServerManagerEvent::SetupStateChanged { session_id, state } => {
                    sessions.set_remote_server_setup_state(*session_id, state.clone());
                    ctx.notify();
                }
                RemoteServerManagerEvent::SessionConnecting { .. }
                | RemoteServerManagerEvent::SessionSpeculativeConnectFailed { .. }
                | RemoteServerManagerEvent::SessionDeregistered { .. }
                | RemoteServerManagerEvent::SessionConnectionFailed { .. }
                | RemoteServerManagerEvent::HostConnected { .. }
                | RemoteServerManagerEvent::HostDisconnected { .. }
                | RemoteServerManagerEvent::RemoteAgentContextSnapshot { .. }
                | RemoteServerManagerEvent::NavigatedToDirectory { .. }
                | RemoteServerManagerEvent::RepoMetadataSnapshot { .. }
                | RemoteServerManagerEvent::RepoMetadataUpdated { .. }
                | RemoteServerManagerEvent::RepoMetadataDirectoryLoaded { .. }
                | RemoteServerManagerEvent::BufferUpdated { .. }
                | RemoteServerManagerEvent::BufferConflictDetected { .. }
                | RemoteServerManagerEvent::DiffStateSnapshotReceived { .. }
                | RemoteServerManagerEvent::DiffStateMetadataUpdateReceived { .. }
                | RemoteServerManagerEvent::DiffStateFileDeltaReceived { .. }
                | RemoteServerManagerEvent::GitStatusPushReceived { .. }
                | RemoteServerManagerEvent::GitHubPrInfoPushReceived { .. }
                | RemoteServerManagerEvent::GitHubRepositoryInfoPushReceived { .. }
                | RemoteServerManagerEvent::BinaryCheckComplete { .. }
                | RemoteServerManagerEvent::BinaryInstallComplete { .. }
                | RemoteServerManagerEvent::ClientRequestFailed { .. }
                | RemoteServerManagerEvent::CodebaseIndexStatusesSnapshot { .. }
                | RemoteServerManagerEvent::CodebaseIndexStatusUpdated { .. }
                | RemoteServerManagerEvent::CodebaseIndexMutationFailed { .. }
                | RemoteServerManagerEvent::ServerMessageDecodingError { .. } => {}
                RemoteServerManagerEvent::SessionReconnected {
                    session_id: sid,
                    client,
                    ..
                } => {
                    if let Some(session) = sessions.sessions.get(sid) {
                        let new_executor =
                            Arc::new(RemoteServerCommandExecutor::new(*sid, client.clone()));
                        session.set_command_executor(new_executor);
                        log::info!("Swapped command executor for session {sid:?} after reconnect");
                    }
                }
            });
        }
        #[cfg(not(feature = "local_tty"))]
        let _ = ctx;

        Self {
            pending_session_start_times: Default::default(),
            sessions: Default::default(),
            executor_command_tx,
            in_band_command_output_tx_map: Default::default(),
            executor_for_all_sessions: None,
            env_vars: Default::default(),
            remote_server_setup_states: Default::default(),
        }
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn new_for_test() -> Self {
        let (executor_command_tx, _executor_command_rx) = async_channel::unbounded();
        Self {
            pending_session_start_times: Default::default(),
            sessions: Default::default(),
            executor_command_tx,
            in_band_command_output_tx_map: Default::default(),
            executor_for_all_sessions: None,
            env_vars: Default::default(),
            remote_server_setup_states: Default::default(),
        }
    }

    #[cfg(test)]
    pub fn with_command_executor(mut self, executor: Arc<dyn CommandExecutor>) -> Self {
        self.executor_for_all_sessions = Some(executor);
        self
    }

    pub fn set_env_vars_for_session(
        &mut self,
        session_id: SessionId,
        env_vars: HashMap<String, String>,
        ctx: &mut ModelContext<Self>,
    ) {
        let old_vars = self.env_vars.insert(session_id, env_vars);
        let new_vars = self.env_vars.get(&session_id);

        // We compute a list to see which env vars actually changed. If there were no
        // actual changes in env var, we do not fire an event
        let did_change = match (old_vars, new_vars) {
            (None, None) => false,
            (None, Some(new_vars)) => !new_vars.is_empty(),
            (Some(old_vars), None) => !old_vars.is_empty(),
            (Some(old_vars), Some(new_vars)) => old_vars != *new_vars,
        };
        if did_change {
            ctx.emit(SessionsEvent::EnvironmentVariablesUpdated { session_id })
        }
    }

    pub fn get_env_vars_for_session(
        &self,
        session_id: SessionId,
    ) -> Option<HashMap<String, String>> {
        self.env_vars.get(&session_id).cloned()
    }

    /// Updates the remote server setup state for the given session.
    pub fn set_remote_server_setup_state(
        &mut self,
        session_id: SessionId,
        state: RemoteServerSetupState,
    ) {
        self.remote_server_setup_states.insert(session_id, state);
    }

    /// Returns the current remote server setup state for the given session, if any.
    pub fn remote_server_setup_state(
        &self,
        session_id: SessionId,
    ) -> Option<&RemoteServerSetupState> {
        self.remote_server_setup_states.get(&session_id)
    }

    pub fn register_pending_session(
        &mut self,
        session_info: &SessionInfo,
        ctx: &mut ModelContext<Self>,
    ) {
        self.pending_session_start_times
            .insert(session_info.session_id, Instant::now());
        ctx.emit(SessionsEvent::SessionInitialized {
            session_id: session_info.session_id,
        })
    }

    pub fn initialize_bootstrapped_session(
        &mut self,
        session_info: SessionInfo,
        spawning_command: String,
        restored_block_commands: Vec<HistoryEntry>,
        rcfiles_duration_seconds: Option<f64>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Remove the session from the list of pending sessions.
        let pending_session_start_time = self
            .pending_session_start_times
            .remove(&session_info.session_id);

        let session_id = session_info.session_id;

        let (in_band_command_output_tx, in_band_command_output_rx) = async_channel::unbounded();
        let command_executor = if let Some(executor) = &self.executor_for_all_sessions {
            // Explicitly drop the receiver to match the other branch, where
            // it gets consumed through the function call.
            let _ = in_band_command_output_rx;
            executor.clone()
        } else {
            let parent_session_info = session_info
                .spawning_session_id
                .as_ref()
                .and_then(|session_id| self.sessions.get(session_id))
                .map(|session| &session.info);
            command_executor::new_command_executor_for_session(
                &session_info,
                &self.executor_command_tx,
                in_band_command_output_rx,
                parent_session_info,
                ctx,
            )
        };
        if !in_band_command_output_tx.is_closed() {
            // If the receiving side of the channel was stored somewhere (and
            // not just dropped), store the sending side of the channel so we
            // can proxy output to it.
            self.in_band_command_output_tx_map
                .insert(session_info.session_id, in_band_command_output_tx);
        }

        let session = Session::new(session_info.clone(), command_executor);

        log::info!("Shell is bootstrapped with session_id {:?}", session.id());
        log::debug!("Session details: {session:?}");

        let session = Arc::new(session);
        self.sessions.insert(session.id(), session.clone());

        // For warpified-remote sessions, pick up the current host_id from
        // the manager so session.remote_host_id() is populated without
        // waiting for the next SessionConnected event. The
        // RemoteServerCommandExecutor already has its client baked in, so
        // nothing else needs to be wired here.
        #[cfg(feature = "local_tty")]
        if FeatureFlag::SshRemoteServer.is_enabled()
            && matches!(
                session_info.session_type,
                BootstrapSessionType::WarpifiedRemote
            )
        {
            // #1239: the manager only knows the ssh session itself. A subshell bootstrapped
            // inside it (`sudo -s`, `ksu`, then Phosphorize) runs on the same host, so it takes
            // the host id of the nearest ancestor that has one; without it the agent's file
            // tools refuse ("extension not installed") and path completions are withheld.
            let host_id = RemoteServerManager::as_ref(ctx)
                .host_id_for_session(session_id)
                .cloned()
                .or_else(|| self.inherited_remote_host_id(session.spawning_session_id()));
            if let Some(host_id) = host_id {
                session.set_remote_host_id(Some(host_id));
            }
        }

        let bootstrap_duration_seconds =
            pending_session_start_time.map(|start| start.elapsed().as_secs_f64());
        let warp_attributed_bootstrap_duration_seconds =
            match (bootstrap_duration_seconds, rcfiles_duration_seconds) {
                (Some(total), Some(rcfiles)) => Some(total - rcfiles),
                _ => None,
            };
        let was_triggered_by_rc_file = session
            .subshell_info()
            .clone()
            .map(|info| info.was_triggered_by_rc_file_snippet)
            .unwrap_or(false);

        crate::send_telemetry_from_ctx!(
            TelemetryEvent::BootstrappingSucceeded(BootstrappingInfo {
                shell: session.shell().shell_type().name(),
                shell_version: session.shell().version().clone(),
                is_ssh: session.is_legacy_ssh_session(),
                was_triggered_by_rc_file,
                is_subshell: session.subshell_info().is_some(),
                is_wsl: session.is_wsl(),
                bootstrap_duration_seconds,
                rcfiles_duration_seconds,
                warp_attributed_bootstrap_duration_seconds,
                is_msys2: session.is_msys2(),
                terminal_session_id: Some(session.id()),
            }),
            ctx
        );

        History::handle(ctx).update(ctx, |history, ctx| {
            let session_id = session.id();
            let shell_host = ShellHost::from_session(session.as_ref());

            history.init_session(session, ctx);

            let this_host_commands: Vec<_> = restored_block_commands
                .iter()
                .filter(|item| {
                    item.shell_host
                        .as_ref()
                        .is_none_or(|host| *host == shell_host)
                })
                .cloned()
                .collect();

            // Append the restored block commands at the end of history.
            history.append_restored_commands(session_id, this_host_commands);
        });

        ctx.emit(SessionsEvent::SessionBootstrapped(Box::new(
            SessionBootstrappedEvent {
                session_id,
                spawning_command,
                shell: session_info.shell,
                subshell_info: session_info.subshell_info,
                session_type: session_info.session_type,
            },
        )))
    }

    pub fn get(&self, session_id: SessionId) -> Option<Arc<Session>> {
        self.sessions.get(&session_id).map(Clone::clone)
    }

    /// The longest spawning chain followed when resolving a subshell's host (#1239). Real chains
    /// are two or three deep (ssh -> sudo -> su); the bound only guards against a cycle in
    /// `spawning_session_id`, which nothing prevents.
    const MAX_SPAWNING_CHAIN: usize = 8;

    /// The remote host id of the nearest ancestor of a session spawned from `spawning_session_id`
    /// that has one (#1239): a subshell inside an ssh session is on that session's host.
    pub fn inherited_remote_host_id(
        &self,
        spawning_session_id: Option<SessionId>,
    ) -> Option<warp_core::HostId> {
        let mut next = spawning_session_id;
        for _ in 0..Self::MAX_SPAWNING_CHAIN {
            let ancestor = self.sessions.get(&next?)?;
            if let Some(host_id) = ancestor.remote_host_id() {
                return Some(host_id);
            }
            next = ancestor.spawning_session_id();
        }
        None
    }

    /// Sets the remote host id of every session spawned, directly or through other subshells,
    /// from `root` (#1239). `root` itself is left alone; the caller handles it.
    pub fn set_remote_host_id_for_descendants(
        &self,
        root: SessionId,
        host_id: Option<warp_core::HostId>,
    ) {
        for session in self.sessions.values() {
            if session.id() != root && self.is_spawned_from(session, root) {
                session.set_remote_host_id(host_id.clone());
            }
        }
    }

    /// Whether `session`'s spawning chain reaches `root` within [`Self::MAX_SPAWNING_CHAIN`] hops.
    fn is_spawned_from(&self, session: &Session, root: SessionId) -> bool {
        let mut next = session.spawning_session_id();
        for _ in 0..Self::MAX_SPAWNING_CHAIN {
            let Some(id) = next else {
                return false;
            };
            if id == root {
                return true;
            }
            next = self.sessions.get(&id).and_then(|s| s.spawning_session_id());
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Returns whether we're aware of the existence of any sessions, whether
    /// they are pending or fully bootstrapped.
    pub fn has_pending_or_bootstrapped_session(&self) -> bool {
        !self.pending_session_start_times.is_empty() || !self.sessions.is_empty()
    }

    /// Returns whether the given `session_id` is tracked by this [`Sessions`]
    /// model, either as a pending session (registered via [`Self::register_pending_session`])
    /// or a fully bootstrapped one.
    pub fn tracks_session(&self, session_id: SessionId) -> bool {
        self.sessions.contains_key(&session_id)
            || self.pending_session_start_times.contains_key(&session_id)
    }

    /// Returns a map of the spawning commands for all subshell sessions, keyed the session's `SessionId`.
    pub fn spawning_command_for_subshell_sessions(&self) -> HashMap<SessionId, SubshellSource> {
        self.sessions
            .iter()
            .filter_map(|(id, session)| {
                session.subshell_info().as_ref().map(|info| {
                    (
                        *id,
                        if let Some(env_var_collection_name) = &info.env_var_collection_name {
                            SubshellSource::EnvVarCollection(env_var_collection_name.clone())
                        } else {
                            SubshellSource::Command(info.spawning_command.clone())
                        },
                    )
                })
            })
            .collect()
    }

    /// Handles an [`ExecutedExecutorCommandEvent`] by forwarding the event to
    /// the session's [`InBandCommandExecutor`].
    pub fn handle_executed_command_event(
        &mut self,
        session_id: SessionId,
        event: ExecutedExecutorCommandEvent,
    ) {
        if let Some(in_band_command_output_tx) = self.in_band_command_output_tx_map.get(&session_id)
        {
            if let Err(e) = in_band_command_output_tx.try_send(event) {
                log::error!(
                    "Failed to send ExecutedExecutorCommandEvent to InBandCommandExecutor: {e:?}"
                );
            }
        }
    }

    /// Registers a session in the map for the purpose of testing.
    ///
    /// Prefer using [`Self::initialize_bootstrapped_session`] to properly register
    /// a session (e.g. emit the appropriate events).
    #[cfg(test)]
    pub fn register_session_for_test(&mut self, session: SessionInfo) {
        use command_executor::testing::TestCommandExecutor;
        self.sessions.insert(
            session.session_id,
            Arc::new(Session::new(
                session,
                Arc::new(TestCommandExecutor::default()),
            )),
        );
    }
}

/// How many of the newest history lines a remote session reads at start (#1242). The whole file
/// used to be `cat` through the shared ssh connection: a multi-MB history (unlimited
/// `HISTFILESIZE`, zsh `SAVEHIST=100000`) was one bulk response queued ahead of the first
/// completions, and on a slow link ahead of keystroke echo (#985). Large enough that an ordinary
/// history is read whole; `PHOSPHOR_SSH_HISTORY_MAX_LINES` overrides it (0 = the whole file).
const DEFAULT_REMOTE_HISTORY_MAX_LINES: usize = 50_000;

/// `PHOSPHOR_SSH_HISTORY_MAX_LINES`: unset or unparsable is the default, `0` is unbounded.
fn remote_history_max_lines_from_env(value: Option<&str>) -> Option<usize> {
    match value.map(str::trim).and_then(|v| v.parse::<usize>().ok()) {
        Some(0) => None,
        Some(n) => Some(n),
        None => Some(DEFAULT_REMOTE_HISTORY_MAX_LINES),
    }
}

/// The remote command that reads a (shell-quoted) history file: its newest `max_lines` lines, or
/// all of it. `tail -n` is POSIX and line-based, so an entry is never cut mid-line; the parsers
/// drop a leading partial multi-line entry's continuation on their own.
fn remote_history_read_command(quoted_history_file: &str, max_lines: Option<usize>) -> String {
    match max_lines {
        Some(max_lines) => format!("tail -n {max_lines} {quoted_history_file}"),
        None => format!("cat {quoted_history_file}"),
    }
}

impl From<SessionType> for command_corrections::SessionType {
    fn from(session_type: SessionType) -> Self {
        match session_type {
            SessionType::WarpifiedRemote { .. } => command_corrections::SessionType::Remote,
            SessionType::Local => command_corrections::SessionType::Local,
        }
    }
}

impl From<&SessionType> for command_corrections::SessionType {
    fn from(session_type: &SessionType) -> Self {
        match session_type {
            SessionType::WarpifiedRemote { .. } => command_corrections::SessionType::Remote,
            SessionType::Local => command_corrections::SessionType::Local,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsLegacySSHSession {
    Yes {
        socket_path: PathBuf,
        /// `true` when `socket_path` points at a ControlMaster the user
        /// already had running (the SSH wrapper attached to it instead of
        /// creating a Warp-owned one). Warp must not tear down such a
        /// master on session exit.
        external_control_master: bool,
        /// The remote reported it can take the bootstrap as a counted raw read (#1172); see
        /// `SSHValue::raw_bootstrap`.
        raw_bootstrap: bool,
    },
    No,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostInfo {
    // TODO(CORE-2219): This should be an enum instead of a string
    pub os_category: Option<String>,
    pub linux_distribution: Option<String>,
}

impl HostInfo {
    // TODO(CORE-2219): Once we have a struct instead of a string type,
    // we should instead implement this as either
    //   From<StructName> for command_corrections::PlatformType
    //   TryFrom<StructName for command_corrections::PlatformType
    pub fn platform_type(&self) -> command_corrections::PlatformType {
        use command_corrections::PlatformType::*;

        let Some(category) = &self.os_category else {
            return Posix;
        };
        if category == "Windows" {
            Windows
        } else {
            Posix
        }
    }
}

/// Session information sent from the shell to the Rust app after bootstrap.
///
/// This is an intermediate abstraction between the [`BootstrappedValue`] read from the pty at
/// session bootstrap and the [`Session`] model object managed by the Rust app. At session bootstrap
/// time, the `TerminalModel` constructs this from the session `BootstrappedValue` and emits it as
/// part of the `BootstrappedEvent` payload.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub shell: Shell,
    pub launch_data: Option<ShellLaunchData>,
    pub histfile: Option<String>,
    /// Nushell's sqlite history file, when its history is not a plaintext file (#1203).
    pub history_sqlite: Option<String>,
    pub user: String,
    pub hostname: String,
    pub subshell_info: Option<SubshellInitializationInfo>,
    pub path: Option<String>,
    pub environment_variable_names: HashSet<SmolStr>,
    pub aliases: HashMap<SmolStr, String>,
    pub abbreviations: HashMap<SmolStr, String>,
    // A Vec is sufficient here because function_names are guaranteed to be unique.
    pub function_names: HashSet<SmolStr>,
    pub builtins: HashSet<SmolStr>,
    pub keywords: Vec<SmolStr>,
    pub is_legacy_ssh_session: IsLegacySSHSession,
    pub home_dir: Option<String>,
    pub editor: Option<String>,
    pub session_type: BootstrapSessionType,
    pub host_info: HostInfo,
    pub tmux_control_mode: bool,
    pub wsl_name: Option<String>,
    /// If this is a subshell or remote session, e.g. ssh, store the parent session ID here.
    pub spawning_session_id: Option<SessionId>,
    /// For a session started by the ssh wrapper, the command the user typed in the spawning
    /// (local) session. Such sessions have no `subshell_info`, so it is kept separately; a split
    /// from the session replays it (#1069, `pane_group::ssh_split`).
    pub legacy_ssh_spawning_command: Option<String>,
    /// For an ssh session (wrapper or tmux-warpified), the working directory of the spawning
    /// local session when the `ssh` was typed, as that session reported it. A split replays the
    /// ssh command there (#1069).
    pub ssh_spawning_local_pwd: Option<String>,
}

impl SessionInfo {
    /// Returns a partially populated `SessionInfo` constructed from data contained in the given
    /// arguments. Most notably, this is created after the InitShell DCS hook is received, which
    /// contains the SessionId.
    ///
    /// This should be called to cache data from the InitShell value after the InitShell DCS for a
    /// newly spawned session is received.
    pub fn create_pending(
        shell_type: ShellType,
        init_shell_value: InitShellValue,
        subshell_info: Option<SubshellInitializationInfo>,
        launch_data: Option<ShellLaunchData>,
        legacy_ssh_session: Option<SSHValue>,
        is_warpified_ssh_session: bool,
        active_block_session_id: Option<SessionId>,
    ) -> Self {
        let is_legacy_ssh_session = match legacy_ssh_session {
            Some(ssh_value) => IsLegacySSHSession::Yes {
                socket_path: ssh_value.socket_path,
                external_control_master: ssh_value.external_control_master,
                raw_bootstrap: ssh_value.raw_bootstrap,
            },
            None => IsLegacySSHSession::No,
        };

        if launch_data.is_none() && is_legacy_ssh_session == IsLegacySSHSession::No {
            log::warn!("pending_local_shell_path was None for a local session");
        }

        // Compare the hostname of the session bootstrap payload with the hostname of the machine
        // to determine if this is a local or remote session.
        let session_type = Self::determine_session_type(
            &init_shell_value,
            is_warpified_ssh_session
                || matches!(&is_legacy_ssh_session, IsLegacySSHSession::Yes { .. }),
        );

        let spawning_session_id = if matches!(session_type, BootstrapSessionType::WarpifiedRemote)
            || subshell_info.is_some()
        {
            active_block_session_id
        } else {
            None
        };

        SessionInfo {
            session_id: init_shell_value.session_id,
            shell: Shell::new(shell_type, None, None, Default::default(), None),
            launch_data,
            user: init_shell_value.user,
            hostname: init_shell_value.hostname,
            session_type,
            subshell_info,
            is_legacy_ssh_session,
            environment_variable_names: Default::default(),
            path: None,
            home_dir: None,
            editor: None,
            histfile: None,
            history_sqlite: None,
            aliases: Default::default(),
            abbreviations: Default::default(),
            function_names: Default::default(),
            builtins: Default::default(),
            keywords: Default::default(),
            host_info: Default::default(),
            tmux_control_mode: false,
            wsl_name: init_shell_value.wsl_name,
            spawning_session_id,
            legacy_ssh_spawning_command: None,
            ssh_spawning_local_pwd: None,
        }
    }

    #[cfg(not(feature = "remote_tty"))]
    fn determine_session_type(
        init_shell_value: &InitShellValue,
        is_warpified_ssh_session: bool,
    ) -> BootstrapSessionType {
        match get_local_hostname() {
            Ok(local_hostname) => {
                // Ensures subshells are treated as local
                if local_hostname == init_shell_value.hostname &&
                // Ensures `ssh localhost` is treated as remote
                !is_warpified_ssh_session
                {
                    BootstrapSessionType::Local
                } else {
                    BootstrapSessionType::WarpifiedRemote
                }
            }
            Err(e) => {
                crate::report_error!(e);
                BootstrapSessionType::Local
            }
        }
    }

    #[cfg(feature = "remote_tty")]
    fn determine_session_type(
        _init_shell_value: &InitShellValue,
        _is_warpified_ssh_session: bool,
    ) -> BootstrapSessionType {
        // When the `remote_tty` feature is enabled--the session is always considered remote.
        BootstrapSessionType::WarpifiedRemote
    }

    /// Returns a fully populated [`SessionInfo`] containing data derived from the given
    /// `bootstrapped_value`. SessionId, user, and hostname are carried over from `self` since
    /// these are populated in [`Self::create_pending()`].
    ///
    /// This should be called on the pending `SessionInfo` after the session is bootstrapped and
    /// used to create the canonical `Session` object for the newly bootstrapped session.
    pub fn merge_from_bootstrapped_value(
        mut self,
        bootstrapped_value: BootstrappedValue,
        tmux_control_mode: bool,
    ) -> Self {
        // Determine the value from the bootstrap message, falling back to the cached shell type
        // (from the `InitShell` payload) if unable to parse.
        let shell_type = match ShellType::from_name(bootstrapped_value.shell.as_str()) {
            Some(value) => {
                if value != self.shell.shell_type() {
                    log::error!("Received ShellType {:?} in BootstrappedValue that conflicts with pending ShellType {:?}", value, self.shell.shell_type());
                }
                value
            }
            None => self.shell.shell_type(),
        };

        let home_dir = bootstrapped_value.home_dir;

        let aliases = bootstrapped_value
            .aliases
            .map(|alias_output| shell_type.aliases(alias_output.as_str()));

        let abbreviations = bootstrapped_value
            .abbreviations
            .map(|abbr_output| shell_type.abbreviations(abbr_output.as_str()));

        let function_names = bootstrapped_value
            .function_names
            .map(|function_names_output| function_names_output.lines().map(Into::into).collect());

        let builtins = bootstrapped_value
            .builtins
            .map(|builtins_output| builtins_output.lines().map(Into::into).collect());

        let keywords = bootstrapped_value
            .keywords
            .map(|keywords_output| keywords_output.lines().map(Into::into).collect());

        let env_var_names = bootstrapped_value.env_var_names.map(|names| {
            // In zsh the output of `echo ${(k)parameters[(R)*export*]}` is a single line separated
            // by spaces, whereas `compgen -e` in Bash and `set --names` in Fish put each env var on
            // a separate line.
            let split = match &self.shell.shell_type() {
                ShellType::Zsh | ShellType::PowerShell => names.split(' '),
                // `nu.nu` reports the names one per line.
                ShellType::Bash | ShellType::Fish | ShellType::Nu => names.split('\n'),
            };
            split.map(Into::into).collect::<HashSet<_>>()
        });

        let options = if bootstrapped_value
            .vi_mode_enabled
            .is_some_and(|vi_mode| vi_mode.eq("1"))
        {
            let mut opts = bootstrapped_value.shell_options.unwrap_or_default();
            opts.insert("vi_mode".into());
            Some(opts)
        } else {
            bootstrapped_value.shell_options
        };

        SessionInfo {
            session_id: self.session_id,
            shell: Shell::new(
                shell_type,
                bootstrapped_value.shell_version,
                options,
                bootstrapped_value.shell_plugins.unwrap_or_default(),
                bootstrapped_value.shell_path,
            ),
            launch_data: self.launch_data.take(),
            histfile: bootstrapped_value.histfile,
            history_sqlite: bootstrapped_value.history_sqlite,
            user: self.user,
            hostname: self.hostname,
            session_type: self.session_type,
            path: bootstrapped_value.path,
            environment_variable_names: env_var_names.unwrap_or_default(),
            aliases: aliases.unwrap_or_default(),
            abbreviations: abbreviations.unwrap_or_default(),
            function_names: function_names.unwrap_or_default(),
            builtins: builtins.unwrap_or_default(),
            keywords: keywords.unwrap_or_default(),
            home_dir,
            editor: bootstrapped_value.editor,
            is_legacy_ssh_session: self.is_legacy_ssh_session,
            subshell_info: self.subshell_info.take(),
            host_info: HostInfo {
                os_category: bootstrapped_value.os_category,
                linux_distribution: bootstrapped_value.linux_distribution,
            },
            tmux_control_mode,
            wsl_name: bootstrapped_value.wsl_name,
            spawning_session_id: self.spawning_session_id,
            legacy_ssh_spawning_command: self.legacy_ssh_spawning_command.take(),
            ssh_spawning_local_pwd: self.ssh_spawning_local_pwd.take(),
        }
    }

    /// Returns the name of the WSL distribution, or `None` if this session is not a WSL session.
    fn wsl_name(&self) -> Option<&str> {
        self.wsl_name
            .as_deref()
            .or(self
                .launch_data
                .as_ref()
                .and_then(|launch_data| match launch_data {
                    ShellLaunchData::WSL { distro } => Some(distro.as_str()),
                    _ => None,
                }))
    }

    /// If the path is for a session inside some emulation layer, like a VM for WSL, convert a
    /// paths from inside the session into something the native host can use. Otherwise, leave the
    /// path as-is.
    pub fn maybe_convert_to_native_path(&self, path: &TypedPath) -> anyhow::Result<PathBuf> {
        if let Some(distro) = self.wsl_name() {
            return Ok(convert_wsl_to_windows_host_path(path, distro)?);
        }
        if let Some(ShellLaunchData::MSYS2 {
            executable_path, ..
        }) = &self.launch_data
        {
            return Ok(convert_msys2_to_windows_native_path(
                path,
                &msys2_exe_to_root(WindowsPath::new(
                    executable_path.as_os_str().as_encoded_bytes(),
                )),
            )?);
        }
        PathBuf::try_from(path.to_path_buf())
            .map_err(|path| anyhow::anyhow!("Unable to convert path: {path:?}"))
    }
}

/// The session type determined at bootstrap time.
///
/// Unlike [`SessionType`], this does not carry a `host_id` because that
/// information is not available until the remote-server handshake completes,
/// which happens *after* the session is bootstrapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapSessionType {
    /// The session host is the same host where Zap is running.
    Local,

    /// The session host is a different host from where Zap is running.
    WarpifiedRemote,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionType {
    /// The session host is the same host where Zap is running.
    Local,

    /// The session host is a different host from where Zap is running.
    /// Note that we only know this for sure when we Warpify a block.
    ///
    /// `host_id` is `Some` when the remote server feature flag is enabled and
    /// `RemoteServerManager` has completed the connection handshake. It is
    /// `None` when the feature flag is off or the connection hasn't been
    /// established yet.
    WarpifiedRemote { host_id: Option<warp_core::HostId> },
}

impl From<BootstrapSessionType> for SessionType {
    fn from(bst: BootstrapSessionType) -> Self {
        match bst {
            BootstrapSessionType::Local => SessionType::Local,
            BootstrapSessionType::WarpifiedRemote => SessionType::WarpifiedRemote { host_id: None },
        }
    }
}

/// Represents session state and context, mostly populated and constructed at session bootstrap
/// time.
///
/// This object exposes methods for querying the session context (e.g. history, aliases, executable
/// names) as well as command execution, in which an arbitrary shell command may be executed in a
/// context similar to that of the session (e.g. with the same environment variable definitions,
/// $PATH, working directory, etc).
#[derive(Debug)]
pub struct Session {
    info: SessionInfo,
    external_commands: Arc<OnceCell<HashSet<SmolStr>>>,
    /// Function names collected asynchronously after bootstrap via an in-band command.
    additional_function_names: OnceCell<HashSet<SmolStr>>,
    /// builtin/cmdlet names collected asynchronously after bootstrap via an in-band command.
    additional_builtin_names: OnceCell<HashSet<SmolStr>>,
    /// The command executor for this session. Behind a `RwLock` so it can be
    /// swapped after a remote server reconnect (via `set_command_executor`).
    command_executor: RwLock<Arc<dyn CommandExecutor>>,
    load_external_commands_future: OnceCell<Shared<BoxFuture<'static, ()>>>,
    /// Whether the external-command probe has FINISHED, irrespective of whether it
    /// produced anything. Distinct from `has_loaded_external_commands`, which reports
    /// success: since #616 a failed probe deliberately leaves `external_commands`
    /// unset, so success alone can never be waited on -- doing that would hang forever
    /// on exactly the failure it is meant to detect.
    external_commands_load_finished: Arc<AtomicBool>,
    /// Set once a command that may have changed the shell's command set (install, `source`,
    /// `export PATH`, `alias`, `cd`...) has completed since the bootstrap snapshot and `$PATH`
    /// scan, which are never refreshed (#833). See [`Self::knows_command_word`].
    command_data_stale: AtomicBool,
    /// Executables found on `$PATH` after the bootstrap scan, for words the user typed (#1019).
    late_commands: LateCommands,
    load_all_function_names_future: OnceCell<Shared<BoxFuture<'static, ()>>>,
    load_all_builtins_future: OnceCell<Shared<BoxFuture<'static, ()>>>,
    command_case_sensitivity: TopLevelCommandCaseSensitivity,
    /// The authoritative session type, initially derived from the
    /// [`BootstrapSessionType`] in `SessionInfo` and updated by [`Sessions`]
    /// when `RemoteServerManager` reports a connected session (to fill in the
    /// `host_id`). Interior mutability allows updating through `Arc<Session>`.
    session_type: Mutex<SessionType>,
}

impl Session {
    pub fn new(session_info: SessionInfo, command_executor: Arc<dyn CommandExecutor>) -> Self {
        if let Some(version) = &session_info.shell.version() {
            log::info!("Parsed shell version string: {:?}", Version::from(version));
        }
        let command_case_sensitivity = session_info
            .host_info
            .os_category
            .as_deref()
            .map(TopLevelCommandCaseSensitivity::from_os_category)
            .unwrap_or_else(|| OperatingSystem::get().into());

        let session_type = SessionType::from(session_info.session_type.clone());
        Self {
            info: session_info,
            external_commands: Arc::new(OnceCell::new()),
            additional_function_names: OnceCell::new(),
            additional_builtin_names: OnceCell::new(),
            command_executor: RwLock::new(command_executor),
            load_external_commands_future: Default::default(),
            external_commands_load_finished: Arc::new(AtomicBool::new(false)),
            command_data_stale: AtomicBool::new(false),
            late_commands: Default::default(),
            load_all_function_names_future: Default::default(),
            load_all_builtins_future: Default::default(),
            command_case_sensitivity,
            session_type: Mutex::new(session_type),
        }
    }

    pub fn id(&self) -> SessionId {
        self.info.session_id
    }

    pub fn user(&self) -> &str {
        self.info.user.as_str()
    }

    pub fn hostname(&self) -> &str {
        self.info.hostname.as_str()
    }

    pub fn session_type(&self) -> SessionType {
        self.session_type.lock().clone()
    }

    /// The session this one was launched from, if it is a subshell or remote session.
    pub fn spawning_session_id(&self) -> Option<SessionId> {
        self.info.spawning_session_id
    }

    /// See [`SessionInfo::legacy_ssh_spawning_command`].
    pub fn legacy_ssh_spawning_command(&self) -> Option<&str> {
        self.info.legacy_ssh_spawning_command.as_deref()
    }

    /// See [`SessionInfo::ssh_spawning_local_pwd`].
    pub fn ssh_spawning_local_pwd(&self) -> Option<&str> {
        self.info.ssh_spawning_local_pwd.as_deref()
    }

    /// The connected remote-server host of a `WarpifiedRemote` session, once the handshake has
    /// completed (or an ancestor's has, #1239); `None` for a local session.
    pub fn remote_host_id(&self) -> Option<warp_core::HostId> {
        match &*self.session_type.lock() {
            SessionType::WarpifiedRemote { host_id } => host_id.clone(),
            SessionType::Local => None,
        }
    }

    /// Updates the `host_id` on a `WarpifiedRemote` session type after the
    /// remote server handshake completes (or clears it on disconnect).
    pub fn set_remote_host_id(&self, host_id: Option<warp_core::HostId>) {
        let mut st = self.session_type.lock();
        if let SessionType::WarpifiedRemote { host_id: ref mut h } = *st {
            *h = host_id;
        }
    }

    pub fn shell_family(&self) -> ShellFamily {
        self.shell().shell_type().into()
    }

    /// The exact quoting syntax of this session's shell; see [`ShellDialect`].
    pub fn shell_dialect(&self) -> ShellDialect {
        self.shell().shell_type().into()
    }

    pub fn launch_data(&self) -> Option<&ShellLaunchData> {
        self.info.launch_data.as_ref()
    }

    pub fn maybe_convert_to_native_path(&self, path: &TypedPath) -> anyhow::Result<PathBuf> {
        self.info.maybe_convert_to_native_path(path)
    }

    pub fn path_separators(&self) -> PathSeparators {
        match self.shell().shell_type() {
            ShellType::Zsh | ShellType::Bash | ShellType::Fish => PathSeparators::for_unix(),
            ShellType::PowerShell | ShellType::Nu => PathSeparators::for_os(),
        }
    }

    pub fn home_dir(&self) -> Option<&str> {
        if cfg!(test) {
            return warp_util::path::TEST_SESSION_HOME_DIR.as_deref();
        }

        self.info.home_dir.as_deref()
    }

    pub fn editor(&self) -> Option<&str> {
        self.info.editor.as_deref()
    }

    pub fn host_info(&self) -> HostInfo {
        self.info.host_info.clone()
    }

    pub fn is_legacy_ssh_session(&self) -> bool {
        matches!(
            self.info.is_legacy_ssh_session,
            IsLegacySSHSession::Yes { .. }
        )
    }

    pub fn is_subshell_or_ssh(&self) -> bool {
        matches!(self.session_type(), SessionType::WarpifiedRemote { .. })
            || self.is_legacy_ssh_session()
            || self.subshell_info().is_some()
    }

    pub fn is_wsl(&self) -> bool {
        self.info.wsl_name().is_some()
    }

    pub fn wsl_distro_name(&self) -> Option<&str> {
        self.info.wsl_name()
    }

    pub fn is_msys2(&self) -> bool {
        matches!(self.launch_data(), Some(ShellLaunchData::MSYS2 { .. }))
    }

    /// Returns the function that converts a Windows-native path into this session's native
    /// representation, or `None` when no conversion is appropriate.
    pub fn windows_path_converter(&self) -> Option<fn(&str) -> String> {
        if self.is_wsl() {
            Some(warp_util::path::convert_windows_path_to_wsl)
        } else if self.is_msys2() {
            Some(warp_util::path::convert_windows_path_to_msys2)
        } else {
            None
        }
    }

    pub fn alias_names(&self) -> impl Iterator<Item = &str> {
        self.info.aliases.keys().map(Deref::deref)
    }

    pub fn builtin_names(&self) -> impl Iterator<Item = &str> {
        self.info
            .builtins
            .iter()
            .chain(self.additional_builtin_names.get().into_iter().flatten())
            .map(Deref::deref)
    }

    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.info
            .function_names
            .iter()
            .chain(self.additional_function_names.get().into_iter().flatten())
            .map(Deref::deref)
    }

    pub fn executable_names(&self) -> impl Iterator<Item = &str> {
        self.external_commands
            .get()
            .into_iter()
            .flatten()
            .map(Deref::deref)
    }

    pub fn aliases(&self) -> &HashMap<SmolStr, String> {
        &self.info.aliases
    }

    pub fn alias_value(&self, name: &str) -> Option<&str> {
        self.info.aliases.get(name).map(Deref::deref)
    }

    pub fn abbreviations(&self) -> &HashMap<SmolStr, String> {
        &self.info.abbreviations
    }

    pub fn abbreviation_value(&self, name: &str) -> Option<&str> {
        self.info.abbreviations.get(name).map(Deref::deref)
    }

    pub fn functions(&self) -> &HashSet<SmolStr> {
        &self.info.function_names
    }

    pub fn builtins(&self) -> &HashSet<SmolStr> {
        &self.info.builtins
    }

    pub fn subshell_info(&self) -> &Option<SubshellInitializationInfo> {
        &self.info.subshell_info
    }

    /// Replaces the command executor for this session. Used after a remote
    /// server reconnect to swap in a new `RemoteServerCommandExecutor`
    /// backed by the reconnected client.
    pub fn set_command_executor(&self, executor: Arc<dyn CommandExecutor>) {
        *self.command_executor.write() = executor;
    }

    /// Swaps this legacy-ssh session's command executor back to the
    /// ControlMaster-based [`RemoteCommandExecutor`], e.g. after the
    /// remote-server proxy died (#1171). Returns `true` if the executor was
    /// replaced; `false` for a non-ssh session or when the current executor is
    /// not the remote-server one (so repeated events log and swap only once).
    #[cfg(feature = "local_tty")]
    pub fn fall_back_to_control_master_executor(&self) -> bool {
        let IsLegacySSHSession::Yes { socket_path, .. } = &self.info.is_legacy_ssh_session else {
            return false;
        };
        let mut executor = self.command_executor.write();
        // Only the remote-server (helper) executor is swapped; any other executor (the
        // ControlMaster one already, tmux, test doubles, ...) is left alone (#1253).
        if !executor.as_ref().is_remote_server_executor() {
            return false;
        }
        let wsl_distro = self.info.wsl_name().map(ToOwned::to_owned);
        *executor = Arc::new(RemoteCommandExecutor::new(socket_path.clone(), wsl_distro));
        log::info!(
            "Remote-server client lost for ssh session {:?}; \
             command executor fell back to the ControlMaster",
            self.info.session_id
        );
        true
    }

    /// #1172: the shell bootstrap of a legacy ssh session is written as soon as the remote-server
    /// binary check passes, overlapping the connect and handshake, so on a slow link the session
    /// can be initialized (and given the ControlMaster executor) before the connect lands. When it
    /// does, this swaps the ControlMaster executor for one backed by `client`, as a reconnect
    /// does. Returns `true` if the executor was replaced; `false` for a non-ssh session or one
    /// whose executor is not the ControlMaster one (already remote-server backed, tmux, ...).
    #[cfg(feature = "local_tty")]
    pub fn adopt_remote_server_client(&self, client: Arc<RemoteServerClient>) -> bool {
        if !matches!(
            self.info.is_legacy_ssh_session,
            IsLegacySSHSession::Yes { .. }
        ) {
            return false;
        }
        let mut executor = self.command_executor.write();
        if executor
            .as_ref()
            .as_any()
            .downcast_ref::<RemoteCommandExecutor>()
            .is_none()
        {
            return false;
        }
        *executor = Arc::new(RemoteServerCommandExecutor::new(
            self.info.session_id,
            client,
        ));
        log::info!(
            "Remote-server client connected after session init for ssh session {:?}; \
             command executor swapped from the ControlMaster",
            self.info.session_id
        );
        true
    }

    /// Returns `true` if already attempted to load external commands for the `Session`.
    pub fn has_attempted_to_load_external_commands(&self) -> bool {
        self.load_external_commands_future.get().is_some()
    }

    /// Returns `true` if external commands have finished loading for this `Session`.
    ///
    /// This reports SUCCESS: a probe that was killed or returned nothing leaves the
    /// cell unset (#616), so this stays false. Callers that need "the probe is no
    /// longer in flight" want [`Self::has_finished_loading_external_commands`].
    pub fn has_loaded_external_commands(&self) -> bool {
        self.external_commands.get().is_some()
    }

    /// Returns `true` once the external-command probe has finished, whether or not it
    /// produced anything. Safe to wait on; `has_loaded_external_commands` is not.
    pub fn has_finished_loading_external_commands(&self) -> bool {
        self.external_commands_load_finished.load(Ordering::Acquire)
    }

    /// Whether the post-bootstrap function and builtin enumeration
    /// ([`Self::load_all_function_names`], [`Self::load_all_builtins`]) can be skipped because it
    /// would only repeat what the Bootstrapped hook already carried (#1230).
    ///
    /// A local PowerShell session runs those commands in two fresh `pwsh -NoProfile` processes,
    /// each a 140-250 ms (Linux) to multi-second (Windows) start with a cold module scan, started
    /// together with the chip commands right after bootstrap. What they could add is names
    /// the bootstrapped shell did not report, and the bootstrap's own `Get-Command` listings in
    /// the live session see everything a `-NoProfile` process does and the profile's additions.
    pub fn deferred_name_enumeration_is_redundant(&self) -> bool {
        self.info.shell.shell_type() == ShellType::PowerShell
            && self
                .command_executor
                .read()
                .as_ref()
                .as_any()
                .downcast_ref::<command_executor::LocalCommandExecutor>()
                .is_some()
    }

    /// Asynchronously collects all function names via an in-band shell command.
    ///
    /// A no-op for every shell whose bootstrap already reports its complete
    /// function list — see [`ShellType::shell_command_to_get_all_functions`].
    ///
    /// **What this currently adds, and what it is waiting on (#586).** At the
    /// pin this loader is one half of a pair: `pwsh.ps1` enumerates only the
    /// modules that ship with PowerShell (`Get-Command -CommandType Function
    /// -Module $corePsModules`) so that bootstrap stays fast, and everything
    /// else arrives here afterwards. This fork has **not** taken that bootstrap
    /// narrowing, deliberately: `Session::execute_command` reaches a local
    /// PowerShell through `LocalCommandExecutor`, which passes `-NoProfile`, so
    /// a deferred pass cannot see functions the user defined in their profile —
    /// narrowing the bootstrap would drop those from completion for good. Until
    /// that is resolved, this loader is additive-only and will usually find
    /// nothing new on a local session, because the wide bootstrap already
    /// carries the same names. Do not narrow `pwsh.ps1` without first making
    /// profile-defined functions recoverable here.
    pub async fn load_all_function_names(&self) {
        let Some(command) = self
            .info
            .shell
            .shell_type()
            .shell_command_to_get_all_functions()
        else {
            return;
        };
        self.load_deferred_name_set(
            command,
            &self.info.function_names,
            &self.additional_function_names,
            &self.load_all_function_names_future,
            "function",
        )
        .await;
    }

    /// Asynchronously collects all builtin (cmdlet) names via an in-band shell command.
    ///
    /// A no-op for every shell whose bootstrap already reports its complete
    /// builtin list — see [`ShellType::shell_command_to_get_all_builtins`], and
    /// subject to the same bootstrap coupling described on
    /// [`Self::load_all_function_names`].
    pub async fn load_all_builtins(&self) {
        let Some(command) = self
            .info
            .shell
            .shell_type()
            .shell_command_to_get_all_builtins()
        else {
            return;
        };
        self.load_deferred_name_set(
            command,
            &self.info.builtins,
            &self.additional_builtin_names,
            &self.load_all_builtins_future,
            "builtin",
        )
        .await;
    }

    /// Shared helper for [`Self::load_all_function_names`] and [`Self::load_all_builtins`].
    ///
    /// Runs `command` in-band, keeps only the names the bootstrap snapshot in
    /// `existing` did not already carry, and stores them in `storage`. As with
    /// `load_external_commands`, concurrent callers share a single execution via
    /// `future_cell`, so calling either loader more than once per session costs
    /// at most one in-band command.
    async fn load_deferred_name_set(
        &self,
        command: &'static str,
        existing: &HashSet<SmolStr>,
        storage: &OnceCell<HashSet<SmolStr>>,
        future_cell: &OnceCell<Shared<BoxFuture<'static, ()>>>,
        label: &'static str,
    ) {
        let (load_future, receiver) = (async {
            // A transport error (the remote connection dropped under the command) says nothing
            // about the names, so it is retried after a short backoff and, if it never
            // succeeds, nothing is stored; storing an empty set made the failure permanent for
            // the session (#1289).
            let mut result = self
                .execute_command(command, None, None, ExecuteCommandOptions::default())
                .await;
            for delay in DEFERRED_NAME_RETRY_DELAYS {
                if result.is_ok() {
                    break;
                }
                warpui::r#async::Timer::after(delay).await;
                result = self
                    .execute_command(command, None, None, ExecuteCommandOptions::default())
                    .await;
            }
            if let Err(e) = &result {
                log::warn!("Failed to load {label} names, will not store an empty set: {e:#}");
                return;
            }

            let new_names: HashSet<SmolStr> = match result {
                Ok(output) if output.status == CommandExitStatus::Success => {
                    match output.to_string() {
                        Ok(output_string) => output_string
                            .lines()
                            .filter(|name| !name.is_empty() && !existing.contains(*name))
                            .map(Into::into)
                            .collect(),
                        Err(e) => {
                            log::warn!("Failed to decode {label} names output: {e:#}");
                            HashSet::new()
                        }
                    }
                }
                Ok(_) => {
                    log::warn!("In-band command for {label} names returned non-success status");
                    HashSet::new()
                }
                Err(_) => return,
            };

            if storage.set(new_names).is_err() {
                log::warn!("Additional {label} names were already set for this session.");
            }
        })
        .remote_handle();

        match future_cell.try_insert(receiver.boxed().shared()) {
            Ok(_) => load_future.await,
            Err((existing_receiver, _)) => existing_receiver.clone().await,
        };
    }

    /// The output `shell_command_to_get_executables` gives for a local PowerShell session,
    /// produced by scanning the session's `PATH` directories in this process (see
    /// [`warp_util::path::executable_file_names_in_path`]); `None` for any other session, which
    /// runs the shell command.
    async fn native_powershell_executables(&self) -> Option<CommandOutput> {
        let is_msys2 = self
            .info
            .launch_data
            .as_ref()
            .is_some_and(|launch_data| matches!(launch_data, ShellLaunchData::MSYS2 { .. }));
        if is_msys2 || !self.deferred_name_enumeration_is_redundant() {
            // Not a local PowerShell session.
            return None;
        }
        let path_env = self
            .info
            .path
            .as_deref()
            .map(std::ffi::OsString::from)
            .or_else(|| std::env::var_os("PATH"))?;
        let pathext = std::env::var_os("PATHEXT");
        let names = blocking::unblock(move || {
            warp_util::path::executable_file_names_in_path(&path_env, pathext.as_deref())
        })
        .await;
        Some(CommandOutput {
            stdout: names.join("\n").into_bytes(),
            stderr: Vec::new(),
            status: CommandExitStatus::Success,
            exit_code: None,
        })
    }

    /// Asynchronously loads the external commands.
    ///
    /// If this is called while a previous call to `load_external_commands` is
    /// still running, this will not resolve until the previous call does.  This
    /// means it is safe to call `load_external_commands` multiple times, and
    /// that when a call resolves, there is a guarantee that external commands
    /// have been loaded.
    ///
    /// Note that we load executables post-bootstrap because we don't need an interactive, login shell
    /// to get them (unlike aliases, functions and env-vars). All we need is the user's $PATH var,
    /// which we have access to at this point.
    pub async fn load_external_commands(&self) {
        let (load_future, receiver) = (async {
            let shell = self.info.shell.clone();
            let external_commands = self.external_commands.clone();
            let external_commands_load_finished = self.external_commands_load_finished.clone();

            // #978: on a warpified remote fish session this probe (`complete -C --escape ''`)
            // is the in-band command that was observed leaking into fish's command line and
            // history, corrupting every user command typed after it. Skip it here rather than
            // swapping the whole session executor for a no-op: that also broke history loading
            // and every other in-band read. Leaving the cell unset reports `Unknown`, not
            // 'no executables' (see #616 below), so executable-gated chips stay enabled.
            if shell.shell_type() == ShellType::Fish
                && matches!(self.session_type(), SessionType::WarpifiedRemote { .. })
            {
                log::info!("Skipping external command enumeration for remote fish session (#978)");
                external_commands_load_finished.store(true, Ordering::Release);
                return;
            }

            let shell_command_to_get_executables =
                shell.shell_type().shell_command_to_get_executables();
            let env_vars = self
                .info
                .path
                .as_deref()
                .map(|path| HashMap::from_iter([("PATH".to_string(), path.to_string())]));

            // A local PowerShell session lists the executables itself (#1230) instead of
            // starting `pwsh -NoProfile` for `Get-Command -CommandType Application`.
            let native_result = self.native_powershell_executables().await;
            let result = match native_result {
                Some(output) => Ok(output),
                None => {
                    self.execute_command(
                        shell_command_to_get_executables,
                        None,
                        env_vars,
                        ExecuteCommandOptions::default(),
                    )
                    .await
                }
            };

            let is_msys2 =
                self.info.launch_data.as_ref().is_some_and(|launch_data| {
                    matches!(launch_data, ShellLaunchData::MSYS2 { .. })
                });
            // We gather the external Windows-specific commands by using PowerShell because
            // Git Bash's `compgen` is slow at gathering these. The Git Bash-specific commands
            // like ls.exe are retrieved above.
            let mut new_commands = if is_msys2 {
                let env_vars = self
                    .info
                    .path
                    .as_deref()
                    .map(|path| HashMap::from_iter([("PATH".to_string(), path.to_string())]));
                let executor = self.command_executor.read().clone();
                let windows_results = executor
                    .execute_command(
                        ShellType::PowerShell.shell_command_to_get_executables(),
                        &Shell::new(ShellType::PowerShell, None, None, Default::default(), None),
                        None,
                        env_vars,
                        ExecuteCommandOptions::default(),
                    )
                    .await;
                HashSet::from_iter(
                    ShellType::PowerShell
                        .executables_from_shell_command_output(
                            windows_results,
                            false, /* is_msys2 */
                        )
                        .into_iter(),
                )
            } else {
                HashSet::new()
            };
            new_commands.extend(
                shell
                    .shell_type()
                    .executables_from_shell_command_output(result, is_msys2)
                    .into_iter(),
            );
            // #616: do NOT cache an empty result. This probe is a shell command, and
            // `cancel_active_commands()` is session-global, so submitting a command
            // while it is still in flight SIGKILLs it -- `Generator for executable
            // names failed`, empty output, `Vec::new()`. Caching that into this
            // `OnceCell` made the failure permanent: every executable-gated chip
            // (git, gh, kubectl, svn) reported its command absent for the rest of the
            // session, on a machine that plainly has it, and nothing ever re-probed
            // because `has_attempted_to_load_external_commands` gates the only retry.
            //
            // Leaving the cell unset keeps `has_loaded_external_commands()` false, so
            // capabilities stay `ExternalCommandsAvailability::Unknown`, whose
            // `contains()` returns `None` rather than `Some(false)` -- and
            // `ChipRuntimePolicy::availability` only disables on `Some(false)`. So the
            // chips stay enabled instead of being disabled on bad evidence.
            //
            // A genuinely empty PATH is indistinguishable from a killed probe here,
            // and this deliberately treats both as "unknown". That errs toward showing
            // a chip that may fail over permanently hiding one that works.
            // Mark the probe finished before the outcome branch, so a failed probe is
            // still observably "done" -- see `has_finished_loading_external_commands`.
            external_commands_load_finished.store(true, Ordering::Release);
            if new_commands.is_empty() {
                log::warn!(
                    "External command enumeration returned nothing; not caching, so the \
                     session reports Unknown rather than 'no executables' (#616)."
                );
            } else if external_commands.set(new_commands).is_err() {
                log::warn!("External commands should only be loaded once per session.");
            }
        })
        .remote_handle();

        match self
            .load_external_commands_future
            .try_insert(receiver.boxed().shared())
        {
            Ok(_) => load_future.await,
            Err((existing_receiver, _)) => existing_receiver.clone().await,
        };
    }

    /// All of the top-level commands within this session. This includes executables on the
    /// user's PATH and aliases.
    pub fn top_level_commands(&self) -> impl Iterator<Item = &str> {
        self.external_commands
            .get()
            .into_iter()
            .flatten()
            .chain(&self.info.function_names)
            .chain(self.additional_function_names.get().into_iter().flatten())
            .chain(self.info.aliases.keys())
            .chain(self.info.abbreviations.keys())
            .chain(&self.info.builtins)
            .chain(self.additional_builtin_names.get().into_iter().flatten())
            .chain(&self.info.keywords)
            .map(Deref::deref)
            .chain(self.late_commands.found.lock().clone())
    }

    /// #1019: the bootstrap `$PATH` scan is a one-time snapshot, so a program installed after it
    /// (or put in a new `$PATH` directory) was never a known command and a line led by it, such as
    /// `mytool is great`, kept routing as prose. Looks `word` up on the session's `$PATH` and, if
    /// it is an executable there, adds it to [`Self::top_level_commands`] for the rest of the
    /// session.
    ///
    /// Cheap and bounded: only local Unix sessions (a remote or WSL `$PATH` is not ours to stat),
    /// only a bare command-name word (never a path or arbitrary typed text, #1007), only once the
    /// bootstrap scan has landed and only if the word is not already known. The probe is a
    /// `stat` per `$PATH` directory, and a miss is remembered until the next command completes
    /// ([`Self::note_user_command`]), so re-typing the same unknown word costs nothing.
    pub fn discover_command_word(&self, word: &str) {
        if self.command_word_needs_probe(word) {
            self.probe_command_word(word);
        }
    }

    /// Whether [`Self::discover_command_word`] would stat `$PATH` for `word` right now: the word
    /// is a candidate, is not already known, and has no fresh negative-cache entry. Does no
    /// filesystem access, so the Enter path can ask this on the main thread and push the stat to a
    /// background task ([`Self::probe_command_word`]) instead of waiting on it.
    pub fn command_word_needs_probe(&self, word: &str) -> bool {
        if !cfg!(unix)
            || !self.is_local()
            || self.is_wsl()
            || !self.has_loaded_external_commands()
            || !is_bare_command_name(word)
        {
            return false;
        }
        let case_insensitive = matches!(
            self.command_case_sensitivity,
            TopLevelCommandCaseSensitivity::CaseInsensitive
        );
        if self.top_level_commands().any(|name| {
            if case_insensitive {
                name.eq_ignore_ascii_case(word)
            } else {
                name == word
            }
        }) {
            return false;
        }
        !self
            .late_commands
            .missing
            .lock()
            .contains(word)
    }

    /// The `stat`-per-`$PATH`-directory half of [`Self::discover_command_word`]; records a miss
    /// in the negative cache. Blocks on the filesystem: call off the main thread.
    pub fn probe_command_word(&self, word: &str) {
        {
            let mut missing = self.late_commands.missing.lock();
            if missing.contains(word) {
                return;
            }
            if missing.len() >= MAX_MISSING_COMMAND_WORDS {
                missing.clear();
            }
            missing.insert(word.to_owned());
        }
        let mut dirs: Vec<PathBuf> = self
            .info
            .path
            .as_deref()
            .map(|path| std::env::split_paths(path).collect())
            .unwrap_or_default();
        dirs.extend(self.late_commands.extra_dirs.lock().iter().cloned());
        if !dirs.iter().any(|dir| is_executable_file(&dir.join(word))) {
            return;
        }
        let mut found = self.late_commands.found.lock();
        if found.len() < MAX_LATE_COMMANDS && !found.iter().any(|known| *known == word) {
            // Leaked so `top_level_commands` can hand out `&str`s without holding a lock. Bounded
            // by `MAX_LATE_COMMANDS` entries per session, each a short command name.
            found.push(Box::leak(word.to_owned().into_boxed_str()));
        }
        self.late_commands.missing.lock().remove(word);
    }

    /// [`Self::discover_command_word`] for the first word of a buffer the user is typing. Only a
    /// word that has been finished (followed by whitespace) is looked up, so typing `zorkle` does
    /// not stat once per prefix.
    pub fn discover_command_word_in_buffer(&self, buffer: &str) {
        let trimmed = buffer.trim_start();
        if let Some(end) = trimmed.find(char::is_whitespace) {
            self.discover_command_word(&trimmed[..end]);
        }
    }

    /// Whether this session's shell would recognise `word` as the head of a command: an alias,
    /// function, builtin, keyword or abbreviation reported by this session's bootstrap, or an
    /// executable on its `$PATH` (#833). `None` when the session cannot say -- the `$PATH` scan
    /// has not succeeded yet, or the bootstrap carried no builtin list (every real shell has
    /// some, so an empty set means the data never arrived). A caller must treat `None` as
    /// "do not guess". The bootstrap lists and the `$PATH` scan are one-time snapshots: once a
    /// command that could have changed them has run ([`Self::note_user_command`]) the answer for
    /// a name the snapshot lacks is `None` for the rest of the session, because a tool installed
    /// or a function defined since would otherwise read as unknown. A name the snapshot has is
    /// still `Some(true)`: stale data never un-knows an installed command (#933). Each subshell and ssh session is its own `Session`, so this is always
    /// the answer for the shell the user is typing into.
    pub fn knows_command_word(&self, word: &str) -> Option<bool> {
        if !self.has_loaded_external_commands() || self.info.builtins.is_empty() {
            return None;
        }
        let case_insensitive = matches!(
            self.command_case_sensitivity,
            TopLevelCommandCaseSensitivity::CaseInsensitive
        );
        let known = self.top_level_commands().any(|name| {
            if case_insensitive {
                name.eq_ignore_ascii_case(word)
            } else {
                name == word
            }
        });
        // A name the snapshot has stays known after the snapshot goes stale (a command that
        // could change the command set only ever adds names the snapshot lacks); stale data only
        // stops a name it lacks from being called unknown (#933).
        if known {
            return Some(true);
        }
        if self.command_data_stale.load(Ordering::Acquire) {
            return None;
        }
        Some(false)
    }

    /// False once a command that could have changed what the shell recognises has run
    /// ([`Self::note_user_command`]); the snapshots behind [`Self::knows_command_word`] are then
    /// stale (#833, #935).
    pub fn command_data_is_current(&self) -> bool {
        !self.command_data_stale.load(Ordering::Acquire)
    }

    /// Records a command the user has run in this session. If it could have changed what the
    /// shell recognises (a package install, `source`/`.`, `export`, `alias`, `eval`, `cd` into a
    /// directory a hook such as direnv/nvm reacts to, an environment activation, a build that
    /// installs), the snapshot behind [`Self::knows_command_word`] is stale from here on (#833).
    /// Matching is by shell word anywhere in the command, so `sudo apt install x` and
    /// `a && pip install y` both count. There is no re-scan: the simplest correct choice is to
    /// stop answering, which keeps the line in the shell.
    pub fn note_user_command(&self, command: &str) {
        // #1019/#1381: a word found missing on `$PATH` is missing until something runs that could
        // have created it, and any command can (a build, `./install.sh`, `cp`). The completed
        // command is the event that ends the miss, not a clock; there is no event for installs
        // done outside this session, which are picked up when the next command here completes.
        self.late_commands.missing.lock().clear();
        if command_may_change_known_commands(command) {
            self.command_data_stale.store(true, Ordering::Release);
        }
        // #1019: remember directories the user added to `$PATH`, so a tool put there is found.
        if self.is_local() && command.contains("PATH=") {
            let home = std::env::var("HOME").ok();
            let mut extra = self.late_commands.extra_dirs.lock();
            for dir in path_dirs_assigned_in(command, home.as_deref()) {
                if extra.len() < MAX_EXTRA_PATH_DIRS && !extra.contains(&dir) {
                    extra.push(dir);
                }
            }
            // A new directory may now hold a word recently reported missing.
            self.late_commands.missing.lock().clear();
        }
    }

    pub fn path(&self) -> &Option<String> {
        &self.info.path
    }

    pub fn histfile(&self) -> &Option<String> {
        &self.info.histfile
    }

    pub fn shell(&self) -> &Shell {
        &self.info.shell
    }

    pub fn is_local(&self) -> bool {
        self.session_type() == SessionType::Local
    }

    async fn read_history_for_local_session(&self, is_kaspersky_running: bool) -> Vec<String> {
        let histfile = &self.info.histfile;
        let shell_type = &self.info.shell.shell_type();
        if *shell_type == ShellType::Nu
            && let Some(database) = self.info.history_sqlite.as_deref()
            && let Some(history) = read_nu_sqlite_history(database)
        {
            return history;
        }
        let history_files = histfile.as_ref().map_or_else(
            || shell_type.history_files(),
            |histfile| vec![histfile.to_string()],
        );

        for history_file in history_files {
            let typed_path = TypedPath::from(history_file.as_str());
            let Ok(history_file) = self.maybe_convert_to_native_path(&typed_path) else {
                continue;
            };
            if history_file.exists() {
                log::info!(
                    "Loading history from file {} for shell {}",
                    history_file.display(),
                    shell_type.name()
                );

                let contents = match Self::read_history_contents(
                    history_file.as_path(),
                    *shell_type,
                    is_kaspersky_running,
                )
                .await
                {
                    Ok(contents) => contents,
                    Err(e) => {
                        log::error!("Failed to read history contents for file: {e:?}");
                        continue;
                    }
                };

                let history = shell_type.parse_history(&contents);
                return history;
            }
        }
        log::info!(
            "No history file found for shell {}, starting with empty history",
            shell_type.name()
        );
        Vec::new()
    }

    #[cfg_attr(not(windows), allow(unused_variables))]
    async fn read_history_contents(
        history_file: &Path,
        shell_type: ShellType,
        is_kaspersky_running: bool,
    ) -> Result<Vec<u8>, ReadHistoryContentsError> {
        #[cfg(windows)]
        if shell_type == ShellType::PowerShell {
            return Self::read_powershell_history_contents(history_file, is_kaspersky_running)
                .await;
        }

        async_fs::read(history_file)
            .await
            .map_err(ReadHistoryContentsError::AsyncFsError)
    }

    /// Read the PowerShell history contents by running a PowerShell command and reading the output.
    ///
    /// This is a workaround as reading the history file using [`async_fs::read`] on Windows is a
    /// trigger for certain antivirus software (Kaspersky).
    #[cfg(windows)]
    async fn read_powershell_history_contents(
        history_file: &Path,
        is_kaspersky_running: bool,
    ) -> Result<Vec<u8>, ReadHistoryContentsError> {
        // Try reading the history file using PowerShell commands first.
        let powershell_error =
            match Self::read_history_via_powershell(history_file.as_os_str()).await {
                Ok(result) => return Ok(result),
                Err(e) => e,
            };

        // If Kaspersky is running, early return since we can't use [`async_fs`] to read the history
        // file.
        if is_kaspersky_running {
            return Err(ReadHistoryContentsError::PowerShellError(powershell_error));
        }

        // Otherwise, fall back to using [`async_fs`] to read the history file.
        match async_fs::read(history_file).await {
            Ok(contents) => {
                // Report this error so we have some data on whether this method of running
                // PowerShell commands is reliable. If this turns out to be noisy, we can remove
                // this log line.
                log::warn!(
                    "Failed to read history using PowerShell commands: {powershell_error:?}"
                );
                log::error!(
                    "Failed to read history using PowerShell commands: {powershell_error:?}"
                );
                Ok(contents)
            }
            Err(e) => Err(ReadHistoryContentsError::PowerShellAndAsyncFsError {
                powershell_error,
                async_fs_error: e,
            }),
        }
    }

    #[cfg(windows)]
    async fn read_history_via_powershell(history_file_path: &OsStr) -> Result<Vec<u8>> {
        let Some(powershell_command) = warp_util::path::windows::any_powershell_path() else {
            return Err(anyhow::anyhow!(
                "Failed to find powershell executable to read history"
            ));
        };

        let read_result = command::r#async::Command::new(powershell_command)
            .arg("-NoProfile")
            .arg("-NoLogo")
            .arg("-Command")
            .arg(powershell_read_all_text_command(history_file_path))
            .output()
            .await;
        match read_result {
            Ok(output) if output.status.success() => Ok(output.stdout),
            Ok(output) => Err(anyhow::anyhow!(
                "Command to read history file failed with stderr: {:#}",
                String::from_utf8_lossy(&output.stderr)
            )),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to execute command to read history file: {:#}",
                e
            )),
        }
    }

    async fn read_history_for_remote_session(&self) -> Vec<String> {
        let histfile = &self.info.histfile;
        let shell_type = self.info.shell.shell_type();

        let history_files = histfile.as_ref().map_or_else(
            || shell_type.history_files(),
            |histfile| vec![histfile.to_string()],
        );

        for history_file in history_files {
            if let Some(command_history) = self.read_history_from_file(history_file.as_str()).await
            {
                return command_history;
            }
        }

        log::info!(
            "No history file found for shell {}, starting with empty history",
            shell_type.name()
        );
        Vec::new()
    }

    async fn read_history_from_file(&self, history_file: &str) -> Option<Vec<String>> {
        let env_vars = self
            .info
            .path
            .as_deref()
            .map(|path| HashMap::from_iter([("PATH".to_string(), path.to_string())]));

        // `shell_quote_arg`, not bare single quotes: a control character in the path must not
        // be typed into the PTY raw (#867).
        let quoted_history_file =
            shell_quote_arg(history_file, self.info.shell.shell_type().executor_syntax());
        let command = remote_history_read_command(
            &quoted_history_file,
            remote_history_max_lines_from_env(
                std::env::var("PHOSPHOR_SSH_HISTORY_MAX_LINES")
                    .ok()
                    .as_deref(),
            ),
        );
        let output_in_bytes = self
            .execute_command(&command, None, env_vars, ExecuteCommandOptions::default())
            .await
            .ok()?;

        match output_in_bytes.status {
            CommandExitStatus::Success => {
                log::info!("Successfully parsed history file");
                Some(
                    self.info
                        .shell
                        .shell_type()
                        .parse_history(output_in_bytes.output()),
                )
            }
            CommandExitStatus::Failure => {
                log::warn!("Failed to read history file from remote session");
                None
            }
        }
    }

    pub async fn read_history(&self, is_kaspersky_running: bool) -> Vec<String> {
        match self.info.session_type {
            BootstrapSessionType::Local => {
                self.read_history_for_local_session(is_kaspersky_running)
                    .await
            }
            BootstrapSessionType::WarpifiedRemote => self.read_history_for_remote_session().await,
        }
    }

    pub fn environment_variable_names(&self) -> &HashSet<SmolStr> {
        &self.info.environment_variable_names
    }

    #[cfg(feature = "integration_tests")]
    pub fn external_commands(&self) -> Arc<OnceCell<HashSet<SmolStr>>> {
        self.external_commands.clone()
    }

    pub async fn execute_command(
        &self,
        command: &str,
        current_dir_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
        execute_command_options: ExecuteCommandOptions,
    ) -> Result<CommandOutput> {
        // Clone the Arc out of the lock so we don't hold the read guard
        // across the await point.
        let executor = self.command_executor.read().clone();
        // Kept for one retry (#1289); only a command that can be caught by an executor swap needs
        // them, which is a remote session's.
        let retry_args = (!self.is_local()).then(|| environment_variables.clone());
        let result = executor
            .execute_command(
                command,
                &self.info.shell,
                current_dir_path,
                environment_variables,
                execute_command_options,
            )
            .await;
        if result.is_ok() {
            return result;
        }
        let Some(environment_variables) = retry_args else {
            return result;
        };

        // The remote-server connection can drop under a request in flight (the proxy was killed,
        // #1171) and the session swaps its executor for the ControlMaster one, and back after the
        // reconnect. A request caught by that fails once, and every caller (completions, history,
        // chips, command validity) used to read the error as an empty answer. Move off a dead
        // connection right away rather than waiting for the manager's event, and run the
        // command once on the executor the session has now.
        #[cfg(feature = "local_tty")]
        if executor.connection_lost() {
            self.fall_back_to_control_master_executor();
        }
        let current = self.command_executor.read().clone();
        if Arc::ptr_eq(&executor, &current) {
            return result;
        }
        log::info!("Retrying a failed session command on the session's new command executor");
        current
            .execute_command(
                command,
                &self.info.shell,
                current_dir_path,
                environment_variables,
                execute_command_options,
            )
            .await
    }

    /// Whether the backing executor for the session supports execution of commands in parallel.
    pub fn supports_parallel_command_execution(&self) -> bool {
        self.command_executor
            .read()
            .supports_parallel_command_execution()
    }

    pub fn cancel_active_commands(&self) {
        self.command_executor.read().cancel_active_commands();
    }

    pub async fn git_branches_for_command_corrections(&self, working_dir: &str) -> Vec<String> {
        // Reuse the last list while the repository's HEAD and branch refs are unchanged (#1342).
        let stamp = (self.is_local() && std::env::var_os(git_branch_cache::DISABLE_ENV).is_none())
            .then(|| git_branch_cache::branch_stamp(working_dir))
            .flatten();
        if let Some(stamp) = &stamp {
            if let Some(branches) = git_branch_cache::lookup(self.id(), working_dir, stamp) {
                return branches;
            }
        }
        // Running `git branch` outside a repository always fails; skip the process spawn
        // (and the log noise) when we can cheaply tell the local directory isn't in a repo.
        if self.is_local() && local_dir_is_outside_git_repo(Path::new(working_dir)) {
            return vec![];
        }
        let env_vars = self
            .info
            .path
            .as_deref()
            .map(|path| HashMap::from_iter([("PATH".to_string(), path.to_string())]));

        let output = self
            .execute_command(
                crate::context_chips::builtins::GIT_BRANCHES_FOR_CORRECTIONS_COMMAND,
                Some(working_dir),
                env_vars,
                ExecuteCommandOptions::default(),
            )
            .await;

        match output {
            Ok(command_output) if command_output.status == CommandExitStatus::Success => {
                let Ok(output_string) = command_output.to_string() else {
                    log::warn!(
                        "the output for git_branches_for_command_corrections was unparseable"
                    );
                    return vec![];
                };
                let res: Vec<String> = output_string
                    .lines()
                    .map(|s| s.trim().to_string())
                    .collect();
                if let Some(stamp) = stamp {
                    git_branch_cache::store(self.id(), working_dir, stamp, res.clone());
                }
                res
            }
            _ => {
                // Expected whenever the directory isn't a git repository (or git is missing),
                // so this must not WARN once per command.
                log::debug!("failed to get git_branches_for_command_corrections");
                vec![]
            }
        }
    }

    pub fn command_case_sensitivity(&self) -> TopLevelCommandCaseSensitivity {
        self.command_case_sensitivity
    }

    /// Converts the given directory into a [`typed_path::TypedPathBuf`].
    pub fn convert_directory_to_typed_path_buf(&self, pwd: String) -> TypedPathBuf {
        // We need to determine whether this session requires windows file paths
        // or unix file paths. This needs to be resilient to warpified ssh. Some examples:
        // - bash on mac ---> unix
        // - powershell on linux ---> unix
        // - powershell on windows ---> windows
        // - wsl on windows ---> unix
        // - warpified zsh --> unix

        // If the host architecture is unix, we can infer unix file paths. This would break
        // if we supported warpifying a powershell-on-windows SSH session.
        if cfg!(unix) {
            return TypedPathBuf::from_unix(pwd);
        }

        // We assume that we're on Windows.
        match self.shell_family() {
            // Cases: WSL, MSYS2, warpified bash
            ShellFamily::Posix => TypedPathBuf::from_unix(pwd),
            // Cases: powershell sessions
            ShellFamily::PowerShell => TypedPathBuf::from_windows(pwd),
        }
    }

    /// Returns whether `cwd` (a working directory reported for this session)
    /// can be resolved to a usable native path.
    pub fn can_resolve_cwd_to_native_path(&self, cwd: &str) -> bool {
        let typed_path = self.convert_directory_to_typed_path_buf(cwd.to_string());
        self.maybe_convert_to_native_path(&typed_path.to_path())
            .is_ok()
    }
}

impl Display for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "shell: {}, histfile: {:?}, user: {}, host_machine: {:?}",
            self.info.shell.shell_type().name(),
            self.info.histfile,
            self.info.user,
            self.info.hostname,
        )
    }
}

/// Returns the hostname for the local machine where Zap is running.
pub fn get_local_hostname() -> Result<String> {
    cfg_if::cfg_if! {
        if #[cfg(not(target_family = "wasm"))] {
            use gethostname::gethostname;

            gethostname()
                .into_string()
                .map_err(|os_string| {
                    anyhow::anyhow!("Failed to convert local hostname OsString {os_string:?} into String.")
                })
        } else {
            anyhow::bail!("Cannot get machine hostname from wasm")
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
pub mod testing {
    use super::{command_executor::testing::TestCommandExecutor, *};

    /// Builder methods for constructing `SessionInfo` in tests.
    impl SessionInfo {
        pub fn new_for_test() -> Self {
            let path = std::env::var_os("PATH").unwrap().into_string().ok();

            #[cfg(unix)]
            let shell_type = ShellType::Bash;
            #[cfg(windows)]
            let shell_type = ShellType::PowerShell;

            Self {
                session_id: SessionId::from(0),
                shell: Shell::new(shell_type, None, None, Default::default(), None),
                launch_data: None,
                histfile: None,
                history_sqlite: None,
                user: "local:user".to_owned(),
                hostname: "local:host".to_owned(),
                session_type: BootstrapSessionType::Local,
                subshell_info: None,
                path,
                editor: None,
                environment_variable_names: HashSet::new(),
                aliases: HashMap::new(),
                abbreviations: HashMap::new(),
                function_names: HashSet::new(),
                builtins: HashSet::new(),
                keywords: Vec::new(),
                is_legacy_ssh_session: IsLegacySSHSession::No,
                home_dir: None,
                host_info: Default::default(),
                tmux_control_mode: false,
                wsl_name: None,
                spawning_session_id: None,
                legacy_ssh_spawning_command: None,
                ssh_spawning_local_pwd: None,
            }
        }

        pub fn with_aliases(mut self, aliases: HashMap<SmolStr, String>) -> Self {
            self.aliases = aliases;
            self
        }

        pub fn with_abbreviations(mut self, abbreviations: HashMap<SmolStr, String>) -> Self {
            self.abbreviations = abbreviations;
            self
        }

        pub fn with_builtins(mut self, builtins: HashSet<SmolStr>) -> Self {
            self.builtins = builtins;
            self
        }

        pub fn with_function_names(mut self, function_names: HashSet<SmolStr>) -> Self {
            self.function_names = function_names;
            self
        }

        pub fn with_histfile(mut self, histfile: Option<String>) -> Self {
            self.histfile = histfile;
            self
        }

        pub fn with_history_sqlite(mut self, history_sqlite: Option<String>) -> Self {
            self.history_sqlite = history_sqlite;
            self
        }

        pub fn with_user(mut self, user: String) -> Self {
            self.user = user;
            self
        }

        pub fn with_session_type(mut self, session_type: BootstrapSessionType) -> Self {
            self.session_type = session_type;
            self
        }

        pub fn with_spawning_session_id(mut self, spawning_session_id: SessionId) -> Self {
            self.spawning_session_id = Some(spawning_session_id);
            self
        }

        /// A subshell bootstrapped from `spawning_command` (`sudo -s`, `ksu`, ...).
        pub fn with_subshell_spawned_by(mut self, spawning_command: &str) -> Self {
            self.subshell_info = Some(SubshellInitializationInfo {
                spawning_command: spawning_command.to_owned(),
                was_triggered_by_rc_file_snippet: false,
                env_var_collection_name: None,
                ssh_connection_info: None,
            });
            self
        }

        pub fn with_hostname(mut self, hostname: String) -> Self {
            self.hostname = hostname;
            self
        }

        pub fn with_home_dir(mut self, home_dir: String) -> Self {
            self.home_dir = Some(home_dir);
            self
        }

        pub fn with_id(mut self, id: impl Into<SessionId>) -> Self {
            self.session_id = id.into();
            self
        }

        pub fn with_ssh_socket_path(mut self, socket_path: PathBuf) -> Self {
            if let BootstrapSessionType::Local = self.session_type {
                self.session_type = BootstrapSessionType::WarpifiedRemote;
            }
            self.is_legacy_ssh_session = IsLegacySSHSession::Yes {
                socket_path,
                external_control_master: false,
                raw_bootstrap: false,
            };
            self
        }

        /// Marks an ssh session (see [`Self::with_ssh_socket_path`]) as one whose remote reported
        /// it can take the bootstrap as a counted raw read (#1172).
        pub fn with_raw_bootstrap(mut self, raw_bootstrap_value: bool) -> Self {
            if let IsLegacySSHSession::Yes {
                ref mut raw_bootstrap,
                ..
            } = self.is_legacy_ssh_session
            {
                *raw_bootstrap = raw_bootstrap_value;
            }
            self
        }

        pub fn with_keywords(mut self, keywords: Vec<SmolStr>) -> Self {
            self.keywords = keywords;
            self
        }

        pub fn with_path(mut self, path: Option<String>) -> Self {
            self.path = path;
            self
        }

        pub fn with_environment_variable_names(
            mut self,
            environment_variable_names: HashSet<SmolStr>,
        ) -> Self {
            self.environment_variable_names = environment_variable_names;
            self
        }

        pub fn with_shell_type(mut self, shell_type: ShellType) -> Self {
            self.shell = Shell::new(
                shell_type,
                self.shell.version().clone(),
                self.shell.options().clone(),
                self.shell.plugins().clone(),
                self.shell.shell_path().clone(),
            );
            self
        }

        pub fn with_shell_options(mut self, shell_options: HashSet<String>) -> Self {
            self.shell = Shell::new(
                self.shell.shell_type(),
                self.shell.version().clone(),
                Some(shell_options),
                self.shell.plugins().clone(),
                self.shell.shell_path().clone(),
            );
            self
        }
    }

    impl Session {
        pub fn test() -> Self {
            let info = SessionInfo::new_for_test();
            let session_type = SessionType::from(info.session_type.clone());
            Self {
                info,
                external_commands: Default::default(),
                command_executor: RwLock::new(Arc::new(TestCommandExecutor::default())),
                load_external_commands_future: Default::default(),
                external_commands_load_finished: Arc::new(AtomicBool::new(false)),
            command_data_stale: AtomicBool::new(false),
                late_commands: Default::default(),
                command_case_sensitivity: TopLevelCommandCaseSensitivity::CaseSensitive,
                session_type: Mutex::new(session_type),
                additional_function_names: Default::default(),
                load_all_function_names_future: Default::default(),
                additional_builtin_names: Default::default(),
                load_all_builtins_future: Default::default(),
            }
        }

        pub fn test_remote() -> Self {
            let info = SessionInfo::new_for_test()
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_shell_type(ShellType::Bash); // We only support UNIX-based remote sessions.
            let session_type = SessionType::from(info.session_type.clone());
            Self {
                info,
                external_commands: Default::default(),
                command_executor: RwLock::new(Arc::new(TestCommandExecutor::default())),
                load_external_commands_future: Default::default(),
                external_commands_load_finished: Arc::new(AtomicBool::new(false)),
            command_data_stale: AtomicBool::new(false),
                late_commands: Default::default(),
                command_case_sensitivity: TopLevelCommandCaseSensitivity::CaseSensitive,
                session_type: Mutex::new(session_type),
                additional_function_names: Default::default(),
                load_all_function_names_future: Default::default(),
                additional_builtin_names: Default::default(),
                load_all_builtins_future: Default::default(),
            }
        }

        pub fn set_shell_options(&mut self, options: Option<HashSet<String>>) {
            self.info.shell = Shell::new(
                self.info.shell.shell_type(),
                self.info.shell.version().clone(),
                options,
                self.info.shell.plugins().clone(),
                self.info.shell.shell_path().clone(),
            );
        }

        pub fn set_external_commands(&self, commands: impl IntoIterator<Item = impl AsRef<str>>) {
            if self
                .external_commands
                .set(external_commands_with_values(commands))
                .is_err()
            {
                log::warn!("Ignored call to set_external_commands, as external commands had already been set!");
            };
        }

        pub fn with_shell_launch_data(mut self, launch_data: ShellLaunchData) -> Self {
            self.info.launch_data = Some(launch_data);
            self
        }
    }

    fn external_commands_with_values(
        values: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> HashSet<SmolStr> {
        values
            .into_iter()
            .map(|item| item.as_ref().into())
            .collect()
    }
}

/// Words whose appearance in a command line means the shell's set of known commands may have
/// changed (#833).
#[rustfmt::skip]
const COMMAND_SET_CHANGING_WORDS: &[&str] = &[
    "source", ".", "export", "alias", "unalias", "function", "eval", "hash", "rehash", "set",
    "unset", "cd", "pushd", "popd", "z", "direnv", "nvm", "fnm", "asdf", "mise", "pyenv", "rbenv",
    "rustup", "conda", "mamba", "activate", "deactivate", "pip", "pip3", "pipx", "uv", "poetry",
    "npm", "npx", "yarn", "pnpm", "bun", "cargo", "go", "gem", "bundle", "brew", "apt", "apt-get",
    "dpkg", "dnf", "yum", "rpm", "pacman", "zypper", "apk", "snap", "flatpak", "nix", "nix-env",
    "nix-shell", "make", "install", "curl", "wget", "tar", "unzip", "ln", "cp", "mv", "chmod",
    "docker", "podman", "sudo", "su", "bash", "zsh", "fish", "sh", "exec",
];

const MAX_LATE_COMMANDS: usize = 64;
const MAX_MISSING_COMMAND_WORDS: usize = 128;
const MAX_EXTRA_PATH_DIRS: usize = 16;

/// Commands found on `$PATH` after the bootstrap scan, and the bookkeeping to find them cheaply
/// (#1019). See [`Session::discover_command_word`].
#[derive(Debug, Default)]
struct LateCommands {
    found: Mutex<Vec<&'static str>>,
    /// Words not found on `$PATH`; cleared when a command completes.
    missing: Mutex<HashSet<String>>,
    /// Directories the user has put on `$PATH` since the bootstrap, parsed from `PATH=` commands.
    extra_dirs: Mutex<Vec<PathBuf>>,
}

/// True iff `word` is only a command name: no path separator, quoting, expansion or leading dash.
/// Only such a word is ever joined to a `$PATH` directory and stat'd (#1007).
fn is_bare_command_name(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= 64
        && !word.starts_with(['-', '.'])
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-'))
}

fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// Absolute directories named by every `PATH=` assignment in `command` (`export PATH=$HOME/bin:$PATH`,
/// `PATH="/opt/x:$PATH" tool`). Elements that are not literal after expanding a leading `~` /
/// `$HOME` / `${HOME}` (a `$PATH` self-reference, other variables, command substitution) are
/// skipped.
fn path_dirs_assigned_in(command: &str, home: Option<&str>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (index, _) in command.match_indices("PATH=") {
        // `MANPATH=`, `GOPATH=` and the like are different variables.
        if command[..index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        let value = &command[index + "PATH=".len()..];
        let value = value
            .split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|'))
            .next()
            .unwrap_or("");
        let value = value.trim_matches(|c| c == '"' || c == '\'');
        for element in value.split(':') {
            let expanded = if let Some(rest) = element
                .strip_prefix("$HOME")
                .or_else(|| element.strip_prefix("${HOME}"))
                .or_else(|| element.strip_prefix('~'))
            {
                if !(rest.is_empty() || rest.starts_with('/')) {
                    continue;
                }
                match home {
                    Some(home) => format!("{home}{rest}"),
                    None => continue,
                }
            } else {
                element.to_owned()
            };
            if expanded.starts_with('/') && !expanded.contains(['$', '`', '(', '*']) {
                dirs.push(PathBuf::from(expanded));
            }
        }
    }
    dirs
}

fn command_may_change_known_commands(command: &str) -> bool {
    command.contains("PATH=")
        || command
            .split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')' | '`'))
            .map(|word| word.trim_matches(|c| c == '"' || c == '\''))
            .any(|word| {
                COMMAND_SET_CHANGING_WORDS.contains(&word)
                    || word.rsplit('/').next().is_some_and(|base| {
                        COMMAND_SET_CHANGING_WORDS.contains(&base) || base.starts_with("activate")
                    })
            })
}

/// Whether `dir` is an existing local directory with no `.git` (directory or file) in it or any
/// ancestor, i.e. `git branch` there is certain to fail. Anything uncertain -- a missing path, or
/// `GIT_DIR` set in our environment -- returns `false` so git is still consulted.
fn local_dir_is_outside_git_repo(dir: &Path) -> bool {
    if std::env::var_os("GIT_DIR").is_some() || !dir.is_dir() {
        return false;
    }
    !dir.ancestors().any(|ancestor| ancestor.join(".git").exists())
}

/// The commands in nushell's sqlite history (`history.file_format = sqlite`, its default), oldest
/// first (#1203): the `history` table's `command_line`, read read-only. `None` when the file
/// cannot be opened or read (locked, an unknown schema), so the session starts from Phosphor's own
/// records instead. A plain `SELECT` over a few thousand rows, on the thread that reads the
/// history file.
pub(crate) fn read_nu_sqlite_history(database: &str) -> Option<Vec<String>> {
    #[derive(diesel::QueryableByName)]
    struct Row {
        #[diesel(sql_type = diesel::sql_types::Text)]
        command_line: String,
    }
    let url = format!("file:{database}?mode=ro");
    let mut connection =
        match <diesel::sqlite::SqliteConnection as diesel::Connection>::establish(&url) {
            Ok(connection) => connection,
            Err(error) => {
                log::warn!("Could not open nushell's sqlite history {database}: {error:#}");
                return None;
            }
        };
    let rows: Vec<Row> = match diesel::RunQueryDsl::load(
        diesel::sql_query("SELECT command_line FROM history ORDER BY id"),
        &mut connection,
    ) {
        Ok(rows) => rows,
        Err(error) => {
            log::warn!("Could not read nushell's sqlite history {database}: {error:#}");
            return None;
        }
    };
    Some(rows.into_iter().map(|row| row.command_line).collect())
}

#[cfg(test)]
#[path = "session_test.rs"]
mod test;
