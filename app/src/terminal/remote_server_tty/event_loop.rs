use std::sync::Arc;

use async_channel::Receiver;
use parking_lot::{FairMutex, Mutex};
use remote_server::RemotePtySessionId;
use remote_server::client::RemoteServerClient;
use warp_core::HostId;
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use crate::terminal::TerminalModel;
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::ansi::Processor;
use crate::terminal::model::terminal_model::ExitReason;
use crate::terminal::writeable_pty::Message;

/// What one [`Message`] from the `PtyController` becomes on the wire.
///
/// Extracted from the send loop so the four decisions below are a pure function
/// of the message -- they are product decisions, not plumbing, and one of them
/// (`Shutdown`) is genuinely contested. A test can pin each without a client, a
/// daemon, or a pty.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum OutboundRpc {
    /// `WriteSessionStdin`.
    WriteStdin(Vec<u8>),
    /// `ResizeSession`.
    Resize { rows: u32, cols: u32 },
    /// Stop consuming messages and leave the session running on the daemon.
    Detach,
    /// Nothing goes on the wire.
    Ignore,
}

/// Maps a `PtyController` message onto the session RPC that carries it.
///
/// **`Shutdown` detaches; it does not kill.** This is the one decision here that
/// is not forced, so it is stated rather than buried. A daemon-owned session
/// survives its client by design -- requirement 4 of "What is actually needed"
/// in `docs/design/moth-parliament.md` -- and `Message::Shutdown` reaches this
/// point from `TerminalManager::drop`, which fires on tab close, window close
/// and app quit alike. Killing here would mean quitting the app kills every
/// remote session, which is precisely the failure reattach exists to prevent.
/// The neighbouring `remote_tty` event loop already ignores `Shutdown` for its
/// own transport, so this is also the established shape for a non-local pty.
///
/// **Known cost, recorded because it is real and not yet paid.**
/// `Message::Shutdown` is overloaded: `terminal/view.rs`'s autoupdate path sends
/// it meaning *kill this shell so it does not come back after the relaunch*, and
/// under this mapping a remote session would survive that relaunch and return.
/// The fix is not to flip this arm -- that would break every other caller -- but
/// to give the kill intent its own route (`SignalSession` with
/// `RemoteSessionSignal::Kill`, which the protocol already carries for exactly
/// the client-visible "kill this session" action). Until that exists, closing a
/// tab leaves a remote session running, discoverable only from the hosts
/// dashboard. See "Shutdown is two intents wearing one name" in the design doc.
///
/// `ChildExited` is ignored rather than forwarded: it is a Windows-only device
/// for telling the *local* event loop that its own child is gone, and a daemon
/// session learns about exits the other way round, from `SessionExitedPush`.
pub(super) fn outbound_rpc_for(message: Message) -> OutboundRpc {
    match message {
        Message::Input(bytes) => OutboundRpc::WriteStdin(bytes.into_owned()),
        Message::Resize(size_info) => OutboundRpc::Resize {
            rows: size_info.rows as u32,
            cols: size_info.columns as u32,
        },
        Message::Shutdown => OutboundRpc::Detach,
        Message::ChildExited => OutboundRpc::Ignore,
    }
}

/// True when a manager event addressed to `(event_host, event_session)` belongs
/// to the session identified by `(this_host, this_session)`.
///
/// Both halves are required. A `RemotePtySessionId` is minted by the client, so
/// nothing stops two hosts holding sessions under the same id -- matching on the
/// id alone would splice one host's output into another host's terminal. And a
/// host holds many sessions at once, which is the whole reason
/// `RemoteServerManagerEvent::SessionOutputChunk` carries a session id alongside
/// `host_id` in the first place, so matching on the host alone would broadcast
/// every session's output into every terminal on that host.
fn is_for_session(
    event_host: &HostId,
    event_session: &RemotePtySessionId,
    this_host: &HostId,
    this_session: &RemotePtySessionId,
) -> bool {
    event_host == this_host && event_session == this_session
}

/// The client this session's RPCs travel through, shared between the
/// entity-system subscription that refreshes it and the background task that
/// uses it.
///
/// A slot rather than a captured `Arc<RemoteServerClient>` because the client is
/// replaced, not mutated, when a host reconnects: `SessionReconnected` carries a
/// *new* `Arc`, and a task holding the old one would keep writing into a dead
/// connection forever while the terminal looked healthy.
#[derive(Clone, Default)]
pub(super) struct ClientSlot(Arc<Mutex<Option<Arc<RemoteServerClient>>>>);

impl ClientSlot {
    pub(super) fn new(client: Option<Arc<RemoteServerClient>>) -> Self {
        Self(Arc::new(Mutex::new(client)))
    }

    fn get(&self) -> Option<Arc<RemoteServerClient>> {
        self.0.lock().clone()
    }

    fn set(&self, client: Option<Arc<RemoteServerClient>>) {
        *self.0.lock() = client;
    }
}

/// Drives one daemon-owned pty session: this app's writes out to it, and its
/// output back into a `TerminalModel`.
///
/// Mirrors `local_tty::event_loop::EventLoop` and `remote_tty::event_loop::
/// EventLoop` in role, and neither in mechanism -- there is no pty fd to poll
/// and no socket to own here. Both directions are someone else's channel: writes
/// go out as RPCs on the `RemoteServerClient` the manager holds, and output
/// arrives as `RemoteServerManagerEvent`s the manager has already demultiplexed
/// by host.
///
/// **Where this increment stops, stated so the gap is not mistaken for a bug.**
/// Nothing constructs this yet. The `TerminalManager` that would own it (the
/// third sibling of `local_tty::TerminalManager` and `remote_tty::
/// TerminalManager`) and the path that spawns the session in the first place are
/// separate increments, in that order. The same rhythm as the daemon half, where
/// `SessionStore` landed before the RPCs that drive it.
///
/// **Input during a disconnect is dropped, deliberately and visibly.** With no
/// client in the slot there is nowhere for keystrokes to go, and they are logged
/// and discarded rather than queued. Queuing them would mean replaying a user's
/// typing into a shell whose state has moved on since -- the reconnect equivalent
/// of the in-band injection decision B rules out. What should happen instead
/// (refuse input while detached, visibly, rather than accepting it into a void)
/// is a UI decision that belongs with the `TerminalManager` increment.
pub struct EventLoop {
    host_id: HostId,
    session: RemotePtySessionId,
    terminal_model: Arc<FairMutex<TerminalModel>>,
    parser: Processor,
    channel_event_listener: ChannelEventListener,
}

impl EventLoop {
    /// Starts driving `session` on `host_id`: subscribes to the manager for its
    /// output and exit, and spawns the task that carries `messages` out as RPCs.
    pub fn start(
        host_id: HostId,
        session: RemotePtySessionId,
        terminal_model: Arc<FairMutex<TerminalModel>>,
        messages: Receiver<Message>,
        channel_event_listener: ChannelEventListener,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let event_loop = Self {
            host_id: host_id.clone(),
            session: session.clone(),
            terminal_model,
            parser: Processor::default(),
            channel_event_listener,
        };

        // The initial lookup is the only one that happens outside an event, and
        // it is sufficient because of an invariant the caller owes this loop:
        // **an `EventLoop` is constructed for a session that has already been
        // spawned**, and `SpawnSession` cannot have succeeded without a
        // connected client. So `client_for_host` returns `Some` here for any
        // real session, and the empty slot covers the *reconnect window* (the
        // host dropped, `SessionReconnected` will hand back a new client), not a
        // startup state.
        //
        // That invariant is load-bearing, so it is written down rather than
        // assumed: of the manager's events, only `SessionReconnected` carries a
        // client. `HostConnected` and `SessionConnected` do not, and re-reading
        // the manager from inside its own event stream is not something any
        // existing subscriber does -- so a caller that constructs an `EventLoop`
        // eagerly, for a host that has not connected yet, would leave this slot
        // empty with nothing to fill it. The session-creation increment must not
        // do that; if it ever needs to, initial acquisition is its problem to
        // solve, not something this loop can paper over.
        //
        // No manager at all means no transport: a test, or a build where the
        // remote-server feature never registered one.
        let client_slot = if ctx.has_singleton_model::<RemoteServerManager>() {
            let manager = RemoteServerManager::handle(ctx);
            let initial =
                manager.read(ctx, |manager, _| manager.client_for_host(&host_id).cloned());
            let slot = ClientSlot::new(initial);
            event_loop.subscribe_to_manager(&manager, slot.clone(), ctx);
            slot
        } else {
            log::warn!(
                "no RemoteServerManager registered; remote session {session} will have no \
                 transport"
            );
            ClientSlot::default()
        };

        ctx.background_executor()
            .spawn(Self::run_outbound(session, client_slot, messages))
            .detach();

        event_loop
    }

    /// Routes this session's output, exit and client replacement from the
    /// manager's event stream.
    ///
    /// The match is exhaustive on purpose, and the "not ours" arm is spelled out
    /// rather than left to a wildcard, for the reason the `Sessions` subscription
    /// in `terminal/model/session.rs` gives: a wildcard here would silently
    /// swallow a future session-addressed event that this loop ought to handle.
    fn subscribe_to_manager(
        &self,
        manager: &warpui::ModelHandle<RemoteServerManager>,
        client_slot: ClientSlot,
        ctx: &mut ModelContext<Self>,
    ) {
        let host_id = self.host_id.clone();
        let session = self.session.clone();
        ctx.subscribe_to_model(manager, move |event_loop, event, _ctx| match event {
            RemoteServerManagerEvent::SessionOutputChunk {
                host_id: event_host,
                remote_pty_session_id: event_session,
                data,
            } => {
                if is_for_session(event_host, event_session, &host_id, &session) {
                    event_loop.process_pty_bytes(data);
                }
            }
            RemoteServerManagerEvent::SessionExited {
                host_id: event_host,
                remote_pty_session_id: event_session,
                ..
            } => {
                if is_for_session(event_host, event_session, &host_id, &session) {
                    // `ExitReason::ShellProcessExited` regardless of code or
                    // signal, matching `local_tty::event_loop`, whose own exit
                    // branch reports the same unit variant and discards the
                    // status. The exit *code* has a home already -- the daemon
                    // records it in `SessionStore` and `ListSessions` reports it
                    // -- so nothing is lost here that is not lost locally too.
                    event_loop
                        .terminal_model
                        .lock()
                        .exit(ExitReason::ShellProcessExited);
                    event_loop.channel_event_listener.send_wakeup_event();
                }
            }
            // Keyed by `session_id` (this app's own `SessionId`), not by
            // `RemotePtySessionId`, so the host is all there is to match on --
            // which is correct here and only here: a replaced client belongs to
            // the whole connection, and every session on that host must swap to
            // it.
            RemoteServerManagerEvent::SessionReconnected {
                host_id: event_host,
                client,
                ..
            } => {
                if *event_host == host_id {
                    client_slot.set(Some(client.clone()));
                }
            }
            RemoteServerManagerEvent::HostDisconnected {
                host_id: event_host,
                ..
            } => {
                if *event_host == host_id {
                    client_slot.set(None);
                }
            }
            RemoteServerManagerEvent::HostConnected { .. }
            | RemoteServerManagerEvent::SessionConnected { .. }
            | RemoteServerManagerEvent::SessionConnecting { .. }
            | RemoteServerManagerEvent::SessionDisconnected { .. }
            | RemoteServerManagerEvent::SessionDeregistered { .. }
            | RemoteServerManagerEvent::SessionConnectionFailed { .. }
            | RemoteServerManagerEvent::SetupStateChanged { .. }
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
        });
    }

    /// Carries `messages` out to the daemon until the controller detaches or the
    /// channel closes.
    ///
    /// Sequential, one RPC at a time, and that ordering is load-bearing rather
    /// than incidental: stdin is a byte stream, and two writes racing would
    /// reorder a user's keystrokes. `async_channel` preserves send order and this
    /// loop preserves it the rest of the way by awaiting each RPC before taking
    /// the next message.
    ///
    /// The client is looked up per message rather than once, which is what makes
    /// a reconnect transparent: `ClientSlot` is refreshed by the subscription
    /// while this loop is parked on `recv`, so the next write goes out on the new
    /// connection with nothing here needing to know it changed.
    async fn run_outbound(
        session: RemotePtySessionId,
        client_slot: ClientSlot,
        messages: Receiver<Message>,
    ) {
        while let Ok(message) = messages.recv().await {
            match outbound_rpc_for(message) {
                OutboundRpc::Detach => {
                    log::info!("detaching from remote session {session}; the daemon keeps its pty");
                    return;
                }
                OutboundRpc::Ignore => {}
                OutboundRpc::WriteStdin(bytes) => {
                    // The byte *count*, never the bytes. This is a user's
                    // keystrokes: passwords typed at a prompt, tokens pasted
                    // into a command. A dropped write is worth a line in the
                    // log; its contents never are.
                    let Some(client) = client_slot.get() else {
                        log::warn!(
                            "no client for remote session {session}; dropping {} stdin byte(s)",
                            bytes.len()
                        );
                        continue;
                    };
                    if let Err(error) = client.write_session_stdin(session.clone(), bytes).await {
                        log::warn!("remote session {session} stdin write failed: {error:?}");
                    }
                }
                OutboundRpc::Resize { rows, cols } => {
                    let Some(client) = client_slot.get() else {
                        log::warn!(
                            "no client for remote session {session}; dropping resize to \
                             {rows}x{cols}"
                        );
                        continue;
                    };
                    if let Err(error) = client.resize_session(session.clone(), rows, cols).await {
                        log::warn!("remote session {session} resize failed: {error:?}");
                    }
                }
            }
        }
    }

    /// Feeds daemon output through the ANSI parser into the terminal model, the
    /// same way `remote_tty`'s loop does for its own transport.
    fn process_pty_bytes(&mut self, bytes: &[u8]) {
        let mut terminal_model = self.terminal_model.lock();
        self.parser
            .parse_bytes(&mut *terminal_model, bytes, &mut std::io::sink());
        drop(terminal_model);
        // Only the wakeup. The pty-read broadcast is NOT sent here: parsing
        // above already reached `TerminalModel`, which fans the same bytes out
        // through its own `event_proxy` (`terminal_model.rs`), so sending one
        // from this side too would deliver every byte twice to every subscriber
        // -- the session recorder and any shared-session viewer among them.
        // Neither `local_tty`'s nor `remote_tty`'s loop sends one either.
        self.channel_event_listener.send_wakeup_event();
    }
}

impl Entity for EventLoop {
    type Event = ();
}

#[cfg(test)]
#[path = "event_loop_tests.rs"]
mod tests;
