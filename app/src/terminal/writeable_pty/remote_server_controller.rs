use crate::auth::AuthStateProvider;
use crate::remote_server::auth_context::server_api_auth_context;
use instant::Instant;
use remote_server::auth::RemoteServerAuthContext;
use settings::Setting;
use std::path::PathBuf;
use std::sync::Arc;
use warp_core::SessionId;
use warpui::{Entity, ModelContext, ModelHandle, SingletonEntity, WeakModelHandle};

use crate::terminal::warpify::settings::SshExtensionInstallMode;

use crate::remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use crate::remote_server::ssh_transport::SshTransport;
// Zap Wave 3-1: `ServerApiProvider` is no longer used by this file — the
// `auth_client` call sites were physically removed along with AuthClient.
use crate::terminal::model::session::{IsLegacySSHSession, SessionInfo};
use crate::terminal::model_events::{ModelEvent, ModelEventDispatcher};
use crate::terminal::warpify::settings::WarpifySettings;
use crate::{send_telemetry_from_ctx, TelemetryEvent};
use remote_server::setup::{
    PreinstallCheckResult, PreinstallStatus, RemoteLibc, RemotePlatform, UnsupportedReason,
};

use super::pty_controller::{EventLoopSender, PtyController};

/// Per-SSH-init state machine. Encoding the state as an enum makes invalid
/// transitions unrepresentable and ensures the `SessionInfo` stash cannot be
/// accessed after it has been consumed.
///
/// Every active state carries `setup_start` so that the total setup duration
/// can be measured when the flow reaches `SessionConnected`.
enum SshInitState {
    Idle,
    /// Stash held, `check_binary` in flight (and, with #1266, a speculative connect).
    AwaitingCheck {
        session_info: SessionInfo,
        transport: SshTransport,
        setup_start: Instant,
        speculative: SpeculativeConnect,
    },
    /// Stash held, choice block showing.
    AwaitingUserChoice {
        session_info: SessionInfo,
        transport: SshTransport,
        setup_start: Instant,
    },
    /// Stash held, `install_binary` in flight.
    AwaitingInstall {
        session_id: SessionId,
        session_info: SessionInfo,
        transport: SshTransport,
        setup_start: Instant,
    },
    /// `connect_session` in flight. The bootstrap is flushed when the binary check passes
    /// (#1172, `bootstrap_flushed`), or otherwise once `SessionConnected` arrives (or on
    /// connection failure).
    AwaitingConnect {
        session_id: SessionId,
        session_info: SessionInfo,
        setup_start: Instant,
        /// The stashed bootstrap was already written to the PTY (see
        /// `early_bootstrap_flush_enabled`), so no later path may write it again.
        bootstrap_flushed: bool,
        /// The connect being waited for is the speculative one (#1266): the binary check passed
        /// while it was still in flight, so no second connect was started. If it fails, a
        /// probe-driven connect runs then.
        speculative: bool,
    },
}

/// Where a session's speculative connect (#1266) stands while its binary check runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpeculativeConnect {
    /// Not started (switched off).
    Off,
    InFlight,
    /// It did not succeed; the probe-driven setup proceeds as if it had never run.
    Failed,
}

/// What `on_binary_check_complete` does with a passed check (#1266): start the probe-driven
/// connect, or keep waiting for the speculative one that is still in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckPassedAction {
    StartConnect,
    WaitForSpeculativeConnect,
}

fn check_passed_action(speculative: SpeculativeConnect) -> CheckPassedAction {
    match speculative {
        SpeculativeConnect::InFlight => CheckPassedAction::WaitForSpeculativeConnect,
        SpeculativeConnect::Off | SpeculativeConnect::Failed => CheckPassedAction::StartConnect,
    }
}

/// Per-pane orchestrator that defers the bootstrap script write for SSH sessions,
/// checks for the remote-server binary, and presents a two-option choice block when the binary is missing.
///
/// Uses a [`WeakModelHandle`] back to [`PtyController`] to avoid preventing
/// `PtyController` from being deallocated.
pub struct RemoteServerController<T: EventLoopSender> {
    pty_controller: WeakModelHandle<PtyController<T>>,
    model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
    auth_context: Arc<RemoteServerAuthContext>,
    state: SshInitState,
    /// Whether the binary was installed during this setup flow.
    did_install: bool,
    /// Detected remote platform from the binary check phase, used for telemetry.
    remote_platform: Option<RemotePlatform>,
    /// Outcome of the preinstall check from the binary check phase,
    /// used for telemetry on the supported path.
    preinstall_check: Option<PreinstallCheckResult>,
    /// The current ssh init's bootstrap has been written to the PTY (#1172). Makes a second
    /// write impossible by construction: every path that releases the stash goes through
    /// `flush_stashed_bootstrap`, which is a no-op once this is set. Reset per ssh init.
    bootstrap_written: bool,
}

impl<T: EventLoopSender> Entity for RemoteServerController<T> {
    type Event = ();
}

impl<T: EventLoopSender> RemoteServerController<T> {
    pub fn new(
        pty_controller: WeakModelHandle<PtyController<T>>,
        model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let auth_context = Arc::new(server_api_auth_context(
            AuthStateProvider::as_ref(ctx).get().clone(),
        ));
        ctx.subscribe_to_model(&model_event_dispatcher, |me, event, ctx| {
            if let ModelEvent::SshInitShell {
                pending_session_info,
            } = event
            {
                me.on_ssh_init_shell_requested(pending_session_info.as_ref().clone(), ctx);
            }
        });

        let mgr = RemoteServerManager::handle(ctx);
        ctx.subscribe_to_model(&mgr, |me, event, ctx| match event {
            RemoteServerManagerEvent::BinaryCheckComplete {
                session_id,
                result,
                remote_platform,
                preinstall_check,
                has_old_binary,
            } => {
                me.remote_platform = remote_platform.clone();
                me.preinstall_check = preinstall_check.clone();
                me.on_binary_check_complete(
                    *session_id,
                    result.clone(),
                    preinstall_check.clone(),
                    *has_old_binary,
                    ctx,
                );
            }
            RemoteServerManagerEvent::BinaryInstallComplete { session_id, result } => {
                me.on_binary_install_complete(*session_id, result.clone(), ctx);
            }
            RemoteServerManagerEvent::SessionConnected { session_id, .. } => {
                me.on_session_connected(*session_id, ctx);
            }
            RemoteServerManagerEvent::SessionConnectionFailed { session_id, .. } => {
                me.on_session_connection_failed(*session_id, ctx);
            }
            RemoteServerManagerEvent::SessionSpeculativeConnectFailed { session_id } => {
                me.on_speculative_connect_failed(*session_id, ctx);
            }
            RemoteServerManagerEvent::SessionConnecting { .. }
            | RemoteServerManagerEvent::SessionDisconnected { .. }
            | RemoteServerManagerEvent::SessionConnectionLost { .. }
            | RemoteServerManagerEvent::SessionReconnected { .. }
            | RemoteServerManagerEvent::SessionDeregistered { .. }
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
            | RemoteServerManagerEvent::SetupStateChanged { .. }
            | RemoteServerManagerEvent::ClientRequestFailed { .. }
            | RemoteServerManagerEvent::CodebaseIndexStatusesSnapshot { .. }
            | RemoteServerManagerEvent::CodebaseIndexStatusUpdated { .. }
            | RemoteServerManagerEvent::CodebaseIndexMutationFailed { .. }
            | RemoteServerManagerEvent::ServerMessageDecodingError { .. } => {}
        });

        Self {
            pty_controller,
            model_event_dispatcher,
            auth_context,
            state: SshInitState::Idle,
            did_install: false,
            remote_platform: None,
            preinstall_check: None,
            bootstrap_written: false,
        }
    }

    /// Extracts the `SessionInfo` from the stash and writes the bootstrap
    /// script to the PTY via `PtyController::initialize_shell`.
    fn flush_stashed_bootstrap(&mut self, session_info: SessionInfo, ctx: &mut ModelContext<Self>) {
        if self.bootstrap_written {
            return;
        }
        self.bootstrap_written = true;
        match self.pty_controller.upgrade(ctx) { Some(pty) => {
            pty.update(ctx, |pty, ctx| {
                pty.initialize_shell(&session_info, ctx);
            });
        } _ => {
            log::warn!("PtyController dropped before bootstrap could be flushed");
        }}
    }

    /// Idle -> AwaitingCheck
    fn on_ssh_init_shell_requested(&mut self, info: SessionInfo, ctx: &mut ModelContext<Self>) {
        let IsLegacySSHSession::Yes {
            socket_path,
            external_control_master,
            ..
        } = &info.is_legacy_ssh_session
        else {
            return;
        };
        let session_id = info.session_id;
        let socket_path = socket_path.clone();
        let warp_owns_control_master = !external_control_master;
        debug_assert!(matches!(self.state, SshInitState::Idle));
        match std::mem::replace(&mut self.state, SshInitState::Idle) {
            SshInitState::Idle => {}
            SshInitState::AwaitingCheck {
                session_info: old_info,
                ..
            }
            | SshInitState::AwaitingUserChoice {
                session_info: old_info,
                ..
            }
            | SshInitState::AwaitingInstall {
                session_info: old_info,
                ..
            } => {
                self.flush_stashed_bootstrap(old_info, ctx);
            }
            SshInitState::AwaitingConnect {
                session_info: old_info,
                bootstrap_flushed,
                ..
            } => {
                if !bootstrap_flushed {
                    self.flush_stashed_bootstrap(old_info, ctx);
                }
            }
        }
        // A new ssh init: the previous one's stash was released above, so its write flag ends.
        self.bootstrap_written = false;
        // ControlMaster ownership is driven by the bootstrap SSH hook's
        // `external_control_master` flag (parsed from the DCS hook into
        // `IsLegacySSHSession::Yes`). Warp owns the master only when the
        // wrapper created it, i.e. when the flag is `false`.
        let transport = SshTransport::new(
            socket_path.clone(),
            self.auth_context.clone(),
            warp_owns_control_master,
        );
        self.did_install = false;
        self.remote_platform = None;
        self.preinstall_check = None;
        // #1266: on a host that has the extension, the connect is what the session is waiting
        // for, and the probes only confirm what the connect proves by succeeding. Start both;
        // whichever lands first flushes the bootstrap. A failed speculative connect costs one
        // ssh exec and changes nothing: the probes decide as before.
        let speculative = if speculative_connect_enabled() {
            let connection_label = connection_label_for_session_info(&info);
            let speculative_transport = SshTransport::new(
                socket_path,
                self.auth_context.clone(),
                warp_owns_control_master,
            );
            let auth_context = self.auth_context.clone();
            RemoteServerManager::handle(ctx).update(ctx, |mgr, ctx| {
                mgr.connect_session_speculative(
                    session_id,
                    speculative_transport,
                    auth_context,
                    Some(connection_label),
                    ctx,
                );
            });
            SpeculativeConnect::InFlight
        } else {
            SpeculativeConnect::Off
        };
        // #1172 (proposal 1): a host that already proved to have the extension this app session
        // gets its bootstrap NOW; the binary check runs behind it instead of holding the remote
        // shell idle for a mux round trip. The stash is still kept: a check that then says
        // absent / old / unsupported / failed runs the same install, prompt and fallback paths
        // as ever, and `bootstrap_written` stops any of them writing the script a second time.
        let host_key = connection_label_for_session_info(&info);
        if early_flush_known_hosts_enabled()
            && !info.hostname.is_empty()
            && known_extension_hosts().contains(&host_key)
        {
            self.flush_stashed_bootstrap(info.clone(), ctx);
            log::info!(
                "remote-server bootstrap flushed immediately for {host_key} (extension known \
                 present); binary check runs behind it for session {session_id:?}"
            );
        }
        self.state = SshInitState::AwaitingCheck {
            session_info: info,
            transport: transport.clone(),
            setup_start: Instant::now(),
            speculative,
        };
        RemoteServerManager::handle(ctx).update(ctx, |mgr, ctx| {
            mgr.check_binary(session_id, transport, ctx);
        });
    }

    /// The speculative connect did not succeed (#1266). While the check is still running, the
    /// passed-check path later starts a probe-driven connect as if nothing had been tried;
    /// if the check already passed and was waiting for this connect, that connect starts now.
    fn on_speculative_connect_failed(
        &mut self,
        session_id: SessionId,
        ctx: &mut ModelContext<Self>,
    ) {
        match &mut self.state {
            SshInitState::AwaitingCheck {
                session_info,
                speculative,
                ..
            } if session_info.session_id == session_id => {
                *speculative = SpeculativeConnect::Failed;
            }
            SshInitState::AwaitingConnect {
                session_id: expected,
                speculative: true,
                ..
            } if *expected == session_id => {
                let SshInitState::AwaitingConnect {
                    session_id,
                    session_info,
                    setup_start,
                    bootstrap_flushed,
                    ..
                } = std::mem::replace(&mut self.state, SshInitState::Idle)
                else {
                    unreachable!("just matched AwaitingConnect above");
                };
                let IsLegacySSHSession::Yes {
                    socket_path,
                    external_control_master,
                    ..
                } = &session_info.is_legacy_ssh_session
                else {
                    return;
                };
                let socket_path = socket_path.clone();
                let owns_control_master = !external_control_master;
                let connection_label = connection_label_for_session_info(&session_info);
                self.state = SshInitState::AwaitingConnect {
                    session_id,
                    session_info,
                    setup_start,
                    bootstrap_flushed,
                    speculative: false,
                };
                self.connect_session_for_current_identity(
                    session_id,
                    socket_path,
                    owns_control_master,
                    connection_label,
                    ctx,
                );
            }
            SshInitState::Idle
            | SshInitState::AwaitingCheck { .. }
            | SshInitState::AwaitingUserChoice { .. }
            | SshInitState::AwaitingInstall { .. }
            | SshInitState::AwaitingConnect { .. } => {}
        }
    }

    /// Abandons the speculative connect (#1266) before a path that will install, skip or fail:
    /// a connect landing later must not install a client over that decision.
    fn cancel_speculative_connect(
        session_id: SessionId,
        speculative: SpeculativeConnect,
        ctx: &mut ModelContext<Self>,
    ) {
        if speculative == SpeculativeConnect::InFlight {
            RemoteServerManager::handle(ctx).update(ctx, |mgr, _ctx| {
                mgr.cancel_speculative_connect(session_id);
            });
        }
    }

    fn on_binary_check_complete(
        &mut self,
        session_id: SessionId,
        result: Result<bool, String>,
        preinstall_check: Option<PreinstallCheckResult>,
        has_old_binary: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let SshInitState::AwaitingCheck {
            ref session_info, ..
        } = self.state
        else {
            return;
        };
        if session_info.session_id != session_id {
            return;
        }

        let SshInitState::AwaitingCheck {
            session_info,
            transport,
            setup_start,
            speculative,
        } = std::mem::replace(&mut self.state, SshInitState::Idle)
        else {
            unreachable!("just matched AwaitingCheck above");
        };

        // Preinstall gate. Runs **before** any user-visible install
        // affordance: if the script positively classified the host as
        // unsupported, skip the install/prompt entirely and fall back to
        // the legacy ControlMaster-backed SSH flow.
        let unsupported = preinstall_check
            .as_ref()
            .and_then(|check| match &check.status {
                PreinstallStatus::Unsupported { reason } => Some((check, reason.clone())),
                PreinstallStatus::Supported | PreinstallStatus::Unknown => None,
            });
        if let Some((check, reason)) = unsupported {
            log::info!(
                "Preinstall check classified {session_id:?} as unsupported \
                 ({:?}); falling back to legacy SSH",
                check.status
            );
            known_extension_hosts().forget(&connection_label_for_session_info(&session_info));
            send_unsupported_telemetry(self.remote_platform.as_ref(), check, ctx);
            Self::cancel_speculative_connect(session_id, speculative, ctx);
            RemoteServerManager::handle(ctx).update(ctx, |mgr, ctx| {
                mgr.mark_setup_unsupported(session_id, reason, ctx);
            });
            self.flush_stashed_bootstrap(session_info, ctx);
            return;
        }

        match result {
            Ok(true) => {
                let socket_path = transport.socket_path().clone();
                let owns_control_master = transport.owns_control_master();
                let connection_label = connection_label_for_session_info(&session_info);
                // #1172: the binary is there, so the connect below is what the bootstrap was
                // waiting for -- and the remote shell needs hundreds of ms to read and run the
                // bootstrap, far longer than the connect + handshake (tens of ms). Write it now
                // so the two overlap. If the connect somehow lands after the session was
                // initialized, `Sessions` swaps the remote-server executor in (the same swap a
                // reconnect does); a failed connect just leaves the legacy executor.
                known_extension_hosts().remember(&connection_label);
                let bootstrap_flushed = self.bootstrap_written || early_bootstrap_flush_enabled();
                if bootstrap_flushed && !self.bootstrap_written {
                    self.flush_stashed_bootstrap(session_info.clone(), ctx);
                    log::info!(
                        "remote-server bootstrap flushed {} ms after ssh init, \
                         overlapping the connect for session {session_id:?}",
                        setup_start.elapsed().as_millis()
                    );
                }
                // #1266: a speculative connect still in flight IS the connect; a second one
                // would race it for the same session. Wait for it instead.
                let action = check_passed_action(speculative);
                self.state = SshInitState::AwaitingConnect {
                    session_id,
                    session_info,
                    setup_start,
                    bootstrap_flushed,
                    speculative: action == CheckPassedAction::WaitForSpeculativeConnect,
                };
                if action == CheckPassedAction::StartConnect {
                    self.connect_session_for_current_identity(
                        session_id,
                        socket_path,
                        owns_control_master,
                        connection_label,
                        ctx,
                    );
                }
            }
            Ok(false) if has_old_binary => {
                known_extension_hosts().forget(&connection_label_for_session_info(&session_info));
                Self::cancel_speculative_connect(session_id, speculative, ctx);
                // Auto-update: a prior install exists, so skip the modal
                // and reinstall.
                self.did_install = true;
                self.state = SshInitState::AwaitingInstall {
                    session_id,
                    session_info,
                    transport: transport.clone(),
                    setup_start,
                };
                RemoteServerManager::handle(ctx).update(ctx, |mgr, ctx| {
                    mgr.install_binary(session_id, transport, true, ctx);
                });
            }
            Ok(false) => {
                known_extension_hosts().forget(&connection_label_for_session_info(&session_info));
                Self::cancel_speculative_connect(session_id, speculative, ctx);
                let install_mode = *WarpifySettings::as_ref(ctx)
                    .ssh_extension_install_mode
                    .value();
                match install_mode {
                    SshExtensionInstallMode::AlwaysAsk => {
                        self.state = SshInitState::AwaitingUserChoice {
                            session_info,
                            transport,
                            setup_start,
                        };
                        self.model_event_dispatcher.update(ctx, |d, ctx| {
                            d.request_remote_server_block(session_id, ctx);
                        });
                    }
                    SshExtensionInstallMode::AlwaysInstall => {
                        self.did_install = true;
                        self.state = SshInitState::AwaitingInstall {
                            session_id,
                            session_info,
                            transport: transport.clone(),
                            setup_start,
                        };
                        RemoteServerManager::handle(ctx).update(ctx, |mgr, ctx| {
                            mgr.install_binary(session_id, transport, false, ctx);
                        });
                    }
                    SshExtensionInstallMode::NeverInstall => {
                        self.flush_stashed_bootstrap(session_info, ctx);
                    }
                }
            }
            Err(err) => {
                // #1266: a speculative connect still in flight is left to run: if it succeeds
                // where the probe failed, `Sessions` adopts its client (the #1172 swap); if not,
                // nothing changes. The session goes on with the bootstrap either way, as before.
                log::error!("Binary check failed for {session_id:?}: {err}");
                known_extension_hosts().forget(&connection_label_for_session_info(&session_info));
                self.flush_stashed_bootstrap(session_info, ctx);
            }
        }
    }

    pub fn handle_ssh_remote_server_install(
        &mut self,
        session_id: SessionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let SshInitState::AwaitingUserChoice { .. } = self.state else {
            log::warn!("Install clicked but state is not AwaitingUserChoice for {session_id:?}");
            return;
        };

        let SshInitState::AwaitingUserChoice {
            session_info,
            transport,
            setup_start,
        } = std::mem::replace(&mut self.state, SshInitState::Idle)
        else {
            unreachable!("just matched AwaitingUserChoice above");
        };

        // Reaching this path implies the user explicitly confirmed a
        // fresh install from the modal. Auto-update flows (with an old
        // binary detected) skip the modal entirely and go through
        // `on_binary_check_complete` with `is_update: true`.
        self.did_install = true;
        self.state = SshInitState::AwaitingInstall {
            session_id,
            session_info,
            transport: transport.clone(),
            setup_start,
        };
        RemoteServerManager::handle(ctx).update(ctx, |mgr, ctx| {
            mgr.install_binary(session_id, transport, false, ctx);
        });
    }

    /// Called when the remote server session is connected. Flushes the
    /// stashed bootstrap (so the session initializes with a live client)
    /// and emits the `RemoteServerSetupDuration` telemetry event.
    fn on_session_connected(&mut self, session_id: SessionId, ctx: &mut ModelContext<Self>) {
        let (session_info, setup_start, bootstrap_flushed, speculative) = match &self.state {
            SshInitState::AwaitingConnect {
                session_id: expected,
                ..
            } if *expected == session_id => {
                let SshInitState::AwaitingConnect {
                    session_info,
                    setup_start,
                    bootstrap_flushed,
                    speculative,
                    ..
                } = std::mem::replace(&mut self.state, SshInitState::Idle)
                else {
                    unreachable!("just matched AwaitingConnect above");
                };
                (session_info, setup_start, bootstrap_flushed, speculative)
            }
            // #1266: the speculative connect landed before the binary check did. The check's
            // result is moot now (its `BinaryCheckComplete` finds the state Idle and is ignored).
            SshInitState::AwaitingCheck {
                session_info,
                speculative: SpeculativeConnect::InFlight,
                ..
            } if session_info.session_id == session_id => {
                let SshInitState::AwaitingCheck {
                    session_info,
                    setup_start,
                    ..
                } = std::mem::replace(&mut self.state, SshInitState::Idle)
                else {
                    unreachable!("just matched AwaitingCheck above");
                };
                (session_info, setup_start, false, true)
            }
            SshInitState::Idle
            | SshInitState::AwaitingCheck { .. }
            | SshInitState::AwaitingUserChoice { .. }
            | SshInitState::AwaitingInstall { .. }
            | SshInitState::AwaitingConnect { .. } => return,
        };

        known_extension_hosts().remember(&connection_label_for_session_info(&session_info));
        // Flush the stashed bootstrap now that the server is connected (unless the binary check
        // already did, #1172). `client_for_session` will return `Some` when the session
        // subsequently initializes, so it picks `RemoteServerCommandExecutor`.
        if !bootstrap_flushed {
            self.flush_stashed_bootstrap(session_info, ctx);
        }
        let bootstrap_flushed = bootstrap_flushed || self.bootstrap_written;

        let duration_ms = Instant::now()
            .duration_since(setup_start)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        // #1146: the end of the per-connect stage timings (see the manager's setup, install and
        // connect lines): the shell bootstrap was held this long behind the ssh init.
        log::info!(
            "remote-server setup done for session {session_id:?}: connected \
             {duration_ms} ms after ssh init (early_flush={bootstrap_flushed}, \
             speculative={speculative}, installed_binary={})",
            self.did_install
        );
        let (remote_os, remote_arch) = self
            .remote_platform
            .as_ref()
            .map(|p| {
                (
                    Some(p.os.as_str().to_owned()),
                    Some(p.arch.as_str().to_owned()),
                )
            })
            .unwrap_or((None, None));
        let remote_libc = self
            .preinstall_check
            .as_ref()
            .map(|check| describe_libc(&check.libc));
        send_telemetry_from_ctx!(
            TelemetryEvent::RemoteServerSetupDuration {
                duration_ms,
                installed_binary: self.did_install,
                remote_os,
                remote_arch,
                remote_libc,
            },
            ctx
        );
    }

    /// Called when the remote server connection failed. Flushes the stashed
    /// bootstrap so the SSH session is not permanently blocked.
    fn on_session_connection_failed(
        &mut self,
        session_id: SessionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let SshInitState::AwaitingConnect {
            session_id: expected,
            ..
        } = &self.state
        else {
            return;
        };
        if *expected != session_id {
            return;
        }

        let SshInitState::AwaitingConnect {
            session_info,
            bootstrap_flushed,
            ..
        } = std::mem::replace(&mut self.state, SshInitState::Idle)
        else {
            unreachable!("just matched AwaitingConnect above");
        };
        if bootstrap_flushed {
            log::warn!(
                "Remote server connection failed for session {session_id:?}; the bootstrap \
                 was already flushed, so the session continues on the ControlMaster executor"
            );
            return;
        }
        log::warn!(
            "Remote server connection failed for session {session_id:?}; \
             flushing bootstrap to unblock SSH session"
        );
        self.flush_stashed_bootstrap(session_info, ctx);
    }

    pub fn handle_ssh_remote_server_skip(
        &mut self,
        session_id: SessionId,
        ctx: &mut ModelContext<Self>,
    ) {
        let SshInitState::AwaitingUserChoice { session_info, .. } =
            std::mem::replace(&mut self.state, SshInitState::Idle)
        else {
            log::warn!("Skip clicked but state is not AwaitingUserChoice for {session_id:?}");
            return;
        };
        self.flush_stashed_bootstrap(session_info, ctx);
    }

    fn on_binary_install_complete(
        &mut self,
        session_id: SessionId,
        result: Result<(), String>,
        ctx: &mut ModelContext<Self>,
    ) {
        let expected = match &self.state {
            SshInitState::AwaitingInstall { session_id, .. } => *session_id,
            _ => return,
        };
        if expected != session_id {
            return;
        }

        let (session_info, transport, setup_start) =
            match std::mem::replace(&mut self.state, SshInitState::Idle) {
                SshInitState::AwaitingInstall {
                    session_info,
                    transport,
                    setup_start,
                    ..
                } => (session_info, transport, setup_start),
                _ => unreachable!("just matched AwaitingInstall above"),
            };
        match result {
            Ok(()) => {
                let socket_path = transport.socket_path().clone();
                let owns_control_master = transport.owns_control_master();
                let connection_label = connection_label_for_session_info(&session_info);
                self.state = SshInitState::AwaitingConnect {
                    session_id,
                    session_info,
                    setup_start,
                    bootstrap_flushed: false,
                    speculative: false,
                };
                self.connect_session_for_current_identity(
                    session_id,
                    socket_path,
                    owns_control_master,
                    connection_label,
                    ctx,
                );
            }
            Err(err) => {
                log::error!("Binary install failed for {session_id:?}: {err}");
                self.flush_stashed_bootstrap(session_info, ctx);
            }
        }
    }

    fn connect_session_for_current_identity(
        &mut self,
        session_id: SessionId,
        socket_path: PathBuf,
        owns_control_master: bool,
        connection_label: String,
        ctx: &mut ModelContext<Self>,
    ) {
        // ControlMaster ownership was decided once in
        // `on_ssh_init_shell_requested` from the wrapper's
        // `external_control_master` flag; carry it through here so a
        // re-created transport does not tear down an external master.
        let transport =
            SshTransport::new(socket_path, self.auth_context.clone(), owns_control_master);
        let auth_context = self.auth_context.clone();
        RemoteServerManager::handle(ctx).update(ctx, |mgr, ctx| {
            mgr.connect_session(
                session_id,
                transport,
                auth_context,
                Some(connection_label),
                ctx,
            );
        });
    }
}

/// #1266: whether a connect is started alongside the binary probes instead of after them. On by
/// default; `PHOSPHOR_SSH_SPECULATIVE_CONNECT=0` restores the probe-then-connect order.
fn speculative_connect_enabled() -> bool {
    speculative_connect_from_env(
        std::env::var("PHOSPHOR_SSH_SPECULATIVE_CONNECT")
            .ok()
            .as_deref(),
    )
}

fn speculative_connect_from_env(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "false" | "off" | "no"))
}

/// #1172: whether the stashed shell bootstrap is written as soon as the binary check passes
/// instead of after the remote-server connect and handshake. On by default;
/// `PHOSPHOR_SSH_EARLY_BOOTSTRAP=0` restores the old order (bootstrap held until connected).
fn early_bootstrap_flush_enabled() -> bool {
    early_bootstrap_flush_from_env(
        std::env::var("PHOSPHOR_SSH_EARLY_BOOTSTRAP")
            .ok()
            .as_deref(),
    )
}

fn early_bootstrap_flush_from_env(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "false" | "off" | "no"))
}

/// #1172: whether a host already known to have the extension gets its bootstrap written at ssh
/// init, with the binary check running behind it. On by default;
/// `PHOSPHOR_SSH_EARLY_FLUSH_KNOWN=0` restores the stash-until-checked order.
fn early_flush_known_hosts_enabled() -> bool {
    early_flush_known_hosts_from_env(
        std::env::var("PHOSPHOR_SSH_EARLY_FLUSH_KNOWN")
            .ok()
            .as_deref(),
    )
}

fn early_flush_known_hosts_from_env(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "false" | "off" | "no"))
}

/// Hosts (by connection label) whose binary check passed, or whose server connected, earlier in
/// this app session. In-memory only: a restart pays one stashed connect, then it is warm.
#[derive(Default)]
struct KnownExtensionHosts(std::sync::Mutex<std::collections::HashSet<String>>);

impl KnownExtensionHosts {
    fn contains(&self, key: &str) -> bool {
        self.0.lock().map(|set| set.contains(key)).unwrap_or(false)
    }
    fn remember(&self, key: &str) {
        if let Ok(mut set) = self.0.lock() {
            set.insert(key.to_owned());
        }
    }
    fn forget(&self, key: &str) {
        if let Ok(mut set) = self.0.lock() {
            set.remove(key);
        }
    }
}

fn known_extension_hosts() -> &'static KnownExtensionHosts {
    static HOSTS: std::sync::OnceLock<KnownExtensionHosts> = std::sync::OnceLock::new();
    HOSTS.get_or_init(KnownExtensionHosts::default)
}

/// Builds the connection label for a session from its [`SessionInfo`],
/// preferring the SSH host parsed from the interactive `ssh` command over
/// the remote-reported hostname. Passed to `RemoteServerManager::connect_session`
/// so it can be surfaced later via `host_label` instead of the raw `HostId`.
fn connection_label_for_session_info(session_info: &SessionInfo) -> String {
    let ssh_host = session_info
        .subshell_info
        .as_ref()
        .and_then(|info| info.ssh_connection_info.as_ref())
        .and_then(|ssh| ssh.host.as_deref());
    connection_label_from_session_hosts(&session_info.user, &session_info.hostname, ssh_host)
}

/// Builds the short connection label shown for an SSH session, preferring the
/// SSH host from the connection info over the remote-reported hostname.
fn connection_label_from_session_hosts(
    user: &str,
    hostname: &str,
    ssh_host: Option<&str>,
) -> String {
    let host = ssh_host
        .filter(|host| !host.is_empty())
        .map(connection_label_from_ssh_host)
        .or_else(|| (!hostname.is_empty()).then(|| hostname.to_string()));

    connection_label_from_user_and_host(user, host.as_deref())
}

/// Formats a connection label from a user and optional host, matching the UDI
/// (user-defined identifier) display format.
fn connection_label_from_user_and_host(user: &str, host: Option<&str>) -> String {
    match (user.is_empty(), host.filter(|host| !host.is_empty())) {
        (false, Some(host)) => format!("{user}@{host}"),
        (false, None) => user.to_string(),
        (true, Some(host)) => host.to_string(),
        (true, None) => "Remote host".to_string(),
    }
}

/// Strips any `user@` prefix from an SSH host string, keeping just the host.
fn connection_label_from_ssh_host(host: &str) -> String {
    host.rsplit_once('@')
        .map_or(host, |(_user, host)| host)
        .to_string()
}

/// Describes a [`RemoteLibc`] as a short string for telemetry.
fn describe_libc(libc: &RemoteLibc) -> String {
    match libc {
        RemoteLibc::Glibc(version) => format!("glibc {version}"),
        RemoteLibc::NonGlibc { name } => name.clone(),
        RemoteLibc::Unknown => "unknown".to_string(),
    }
}

fn send_unsupported_telemetry<T: EventLoopSender>(
    remote_platform: Option<&RemotePlatform>,
    check: &PreinstallCheckResult,
    ctx: &mut ModelContext<RemoteServerController<T>>,
) {
    let (remote_os, remote_arch) = remote_platform
        .map(|p| {
            (
                Some(p.os.as_str().to_owned()),
                Some(p.arch.as_str().to_owned()),
            )
        })
        .unwrap_or((None, None));
    let required_glibc = match &check.status {
        remote_server::setup::PreinstallStatus::Unsupported {
            reason: UnsupportedReason::GlibcTooOld { required, .. },
        } => required.to_string(),
        _ => String::new(),
    };
    send_telemetry_from_ctx!(
        TelemetryEvent::RemoteServerHostUnsupported {
            remote_os,
            remote_arch,
            detected_libc: describe_libc(&check.libc),
            required_glibc,
        },
        ctx
    );
}

#[cfg(test)]
#[path = "remote_server_controller_tests.rs"]
mod tests;
