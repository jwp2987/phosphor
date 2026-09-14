use std::any::Any;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use async_channel::{Receiver, Sender};
use parking_lot::FairMutex;
use pathfinder_geometry::vector::Vector2F;
use remote_server::RemotePtySessionId;
use warp_core::HostId;
use warpui::{AppContext, ModelHandle, SingletonEntity, ViewHandle, WindowId};

use crate::ai::blocklist::InputConfig;
use crate::context_chips::prompt_type::PromptType;
use crate::pane_group::TerminalViewResources;
use crate::persistence::ModelEvent;
use crate::remote_server::manager::RemoteServerManager;
use crate::terminal::ShellLaunchState;
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::session::Sessions;
use crate::terminal::model_events::ModelEventDispatcher;
use crate::terminal::remote_server_tty::event_loop::EventLoop;
use crate::terminal::shell::{ShellName, ShellType};
use crate::terminal::writeable_pty::terminal_manager_util::{
    init_pty_controller_model, wire_up_pty_controller_with_surface,
};
use crate::terminal::writeable_pty::{self, Message};
use crate::terminal::{TerminalModel, TerminalView, terminal_manager};

/// `async_channel::Sender<Message>` is the `EventLoopSender` here, reusing the
/// `impl EventLoopSender for Sender<Message>` that `remote_tty::terminal_manager`
/// already declares. Trait impls are crate-global, not module-scoped, and
/// `remote_tty` is compiled unconditionally (`terminal/mod.rs` gates it on
/// nothing), so this is reuse with no edit to that file and no second impl of
/// the same trait for the same type -- which would not compile anyway.
type PtyController = writeable_pty::PtyController<Sender<Message>>;

/// Proof that a daemon-owned pty session may have an [`EventLoop`] built for it.
///
/// This type exists for one reason: [`EventLoop::start`] records an invariant it
/// cannot check itself, and a plain pair of ids would let a caller violate it
/// silently. The loop looks its client up once, at construction, and the only
/// manager event that can ever hand it another one is `SessionReconnected` --
/// `HostConnected` and `SessionConnected` carry no client. So an `EventLoop`
/// built for a host that has never connected starts with an empty client slot
/// and has nothing that will ever fill it: a terminal that accepts keystrokes
/// and drops every one of them, looking healthy throughout.
///
/// Two obligations stand behind the invariant, and only one of them is
/// checkable from this layer:
///
/// - **The host has a connected client.** Checked here, by asking
///   `RemoteServerManager` for one. A `None` is a refusal to construct, not a
///   degraded mode.
/// - **The session has already been spawned** on that host. *Not* checkable
///   here -- the daemon owns that fact, and `SpawnSession`'s response is the
///   only place it is observable. The constructor is named for the obligation
///   (`for_spawned_session`) so the caller cannot claim it did not know, and the
///   two are not independent in practice: `SpawnSession` is itself an RPC, so it
///   cannot have succeeded without the connected client this type does check.
///
/// The window between this check and [`TerminalManager::create_model`] is the
/// ordinary reconnect window, which the event loop already handles: if the host
/// drops in between, the slot starts empty and `SessionReconnected` refills it.
/// What the check rules out is the categorically different case -- construction
/// for a host that was never connected at all.
#[derive(Debug)]
pub struct ConnectedRemotePtySession {
    host_id: HostId,
    session: RemotePtySessionId,
}

impl ConnectedRemotePtySession {
    /// Returns a witness for `session` on `host_id`, or `None` when there is no
    /// connected client for that host to carry its RPCs.
    ///
    /// `session` must already have been spawned on `host_id` -- see the type's
    /// doc comment for why that half cannot be checked here. This function never
    /// spawns anything: it takes an id the caller already owns.
    ///
    /// [`super::spawn_remote_session`] is the caller that discharges the
    /// obligation properly -- it calls this only from `SpawnSession`'s response
    /// callback, so the "already spawned" half is true by construction rather
    /// than by promise. It stays public for the reattach increment, whose ids
    /// come from `ListSessions` and are equally already-spawned.
    pub fn for_spawned_session(
        host_id: HostId,
        session: RemotePtySessionId,
        ctx: &AppContext,
    ) -> Option<Self> {
        // No manager at all means no transport -- a test, or a build where the
        // remote-server feature never registered one. `RemoteServerManager::
        // as_ref` panics rather than returning an `Option` in that case, so the
        // membership check has to come first.
        if !ctx.has_singleton_model::<RemoteServerManager>() {
            log::warn!(
                "no RemoteServerManager registered; refusing a terminal for remote session \
                 {session}"
            );
            return None;
        }

        if RemoteServerManager::as_ref(ctx)
            .client_for_host(&host_id)
            .is_none()
        {
            log::warn!(
                "no connected client for host {}; refusing a terminal for remote session {session}",
                host_id.as_str()
            );
            return None;
        }

        Some(Self { host_id, session })
    }

    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }

    pub fn session(&self) -> &RemotePtySessionId {
        &self.session
    }
}

/// Owns one daemon-owned remote pty session: the third sibling of
/// `local_tty::TerminalManager` and `remote_tty::TerminalManager`.
///
/// Structurally this is `remote_tty::TerminalManager` with a different event
/// loop behind it -- the same `writeable_pty` machinery, the same channels, the
/// same `Sender<Message>` as `EventLoopSender`. What differs is the session's
/// lifetime: the pty lives on the far side, owned by `crates/remote_server`'s
/// daemon, and survives this manager being dropped -- but **closing a tab does
/// not always leave it running**, and an earlier version of this comment said it
/// did.
///
/// Dropping this manager detaches: the `PtyController` holding the only
/// `Sender<Message>` goes with it, `run_outbound` falls out of its loop, and the
/// daemon keeps the pty. But `workspace/view.rs`'s tab close first calls
/// `TerminalView::shutdown_pty` on every pane whose active block
/// `is_active_and_long_running()`, and that routes to `Message::Kill` ->
/// `SignalSession`/`Kill`. So closing an *idle* remote tab detaches, and closing
/// one with a command still running **destroys the session on the host**.
///
/// That guard was written for local shells, where "still running" means "you
/// would orphan a process in the window you are closing". For a daemon-owned
/// session it selects precisely the sessions worth keeping -- a two-hour remote
/// build is exactly what `is_active_and_long_running()` matches. Recorded here
/// rather than in the design doc alone because this type is where someone will
/// look to decide whether closing a tab is safe.
///
/// **The resolution: the close asks.** `workspace/view.rs` now scans the tab's
/// panes before closing one, and a pane whose manager downcasts to *this type*
/// and whose active block `is_active_and_long_running()` makes the close open
/// the three-answer close-session dialog: cancel, close and stop the session, or
/// close and leave it running. "Leave running" is carried by
/// `RemotePtyDisposition::LeaveRunning`, which means exactly one thing in
/// `remove_tab_with_disposition`: that pane is skipped by the `shutdown_pty`
/// loop, so no `Message::Kill` is ever queued and the detach-by-drop described
/// above is what happens. A tab of purely local panes never reaches the dialog.
/// See "Shutdown is two intents wearing one name" in
/// `docs/design/moth-parliament.md` for the argument.
///
/// This type's own concrete identity is therefore load-bearing: it is the
/// discriminator that close path downcasts to. A manager that wraps or replaces
/// it without preserving `as_any`'s answer would silently turn the prompt off
/// and put the kill back.
///
/// None of that is reachable end to end yet, and the remaining gap is now the
/// pane rather than the session: [`super::spawn_remote_session`] constructs
/// daemon-owned sessions, but nothing calls it, because opening a tab on one
/// needs `pane_group`/`workspace` to hold the returned pair. Until then no tab
/// has ever held one of these.
///
/// **This manager does not create sessions.** It takes a
/// [`ConnectedRemotePtySession`], which can only be obtained by passing the
/// connected-client check, and attaches to the session it names. Creating one is
/// [`super::spawn_remote_session`]'s job, and it is a separate function rather
/// than a step inside this one for a reason: `create_model` is synchronous and
/// `SpawnSession` is an RPC, so a `create_model` that spawned would have to
/// build its entity graph before the response arrived -- exactly the eager
/// construction [`EventLoop::start`]'s invariant forbids.
pub struct TerminalManager {
    model: Arc<FairMutex<TerminalModel>>,

    // Store a reference to the PTYController and EventLoop so the UI framework doesn't end up
    // deallocating them because there are no strong references to the models.
    _pty_controller: ModelHandle<PtyController>,

    _event_loop: ModelHandle<EventLoop>,

    /// Retained to keep the terminal surface alive for the manager's lifetime.
    #[allow(dead_code)]
    view: ViewHandle<TerminalView>,
}

impl TerminalManager {
    /// Creates a terminal manager model that feeds bytes to/from an
    /// already-spawned, daemon-owned pty session.
    ///
    /// Infallible by construction: every way this could fail to have a transport
    /// is settled by [`ConnectedRemotePtySession::for_spawned_session`] before a
    /// single model or view is added. Returning `None` from here instead would
    /// mean abandoning a half-built entity graph on the failure path.
    pub fn create_model(
        session: ConnectedRemotePtySession,
        resources: TerminalViewResources,
        initial_size: Vector2F,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        window_id: WindowId,
        initial_input_config: Option<InputConfig>,
        ctx: &mut AppContext,
    ) -> (
        ViewHandle<TerminalView>,
        ModelHandle<Box<dyn crate::terminal::TerminalManager>>,
    ) {
        // Create all the necessary channels we need for communication.
        let (wakeups_tx, wakeups_rx) = async_channel::unbounded();
        let (events_tx, events_rx) = async_channel::unbounded();
        let (executor_command_tx, executor_command_rx) = async_channel::unbounded();

        // Use an empty pty reads broadcaster since we don't need to broadcast any PTY bytes for the
        // network-backed PTY. We use 1 instead of 0 here because `async_broadcast` internally
        // asserts that the capacity is at least 1.
        let (pty_reads_tx, _pty_reads_rx) = async_broadcast::broadcast(1);

        let channel_event_proxy = ChannelEventListener::new(wakeups_tx, events_tx, pty_reads_tx);

        // Initialize the sessions model.
        let sessions: ModelHandle<Sessions> =
            ctx.add_model(|ctx| Sessions::new(executor_command_tx, ctx));

        let model_events =
            ctx.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));

        // Create the terminal model.
        let model = terminal_manager::create_terminal_model(
            None, /* startup_directory */
            None, /* restored_blocks */
            initial_size,
            channel_event_proxy.clone(),
            // The shell is a placeholder, exactly as it is in `remote_tty`, and
            // it stays one even now that `session_spawn` exists -- because the
            // resolved shell is not a fact this client has. A `SpawnSession`
            // request may carry `shell: None`, in which case the daemon picks
            // with `ShellStarter::compute_fallback_shell` (the remote user's
            // passwd entry, then `/bin/zsh`, `/bin/bash`, `/bin/fish`) and
            // `SpawnSessionSuccess` -- an empty message -- never says which. So
            // the only honest values here are the blank ones, and they cost
            // little: every session this path creates is spawned with
            // `no_bootstrap = true` (see `session_spawn::NO_BOOTSTRAP`), so
            // there is no shell integration for a shell *type* to configure.
            // Reporting it properly needs a resolved-shell field on
            // `SpawnSessionSuccess`, which is the same proto increment that
            // `no_bootstrap` is waiting on.
            ShellLaunchState::ShellSpawned {
                available_shell: None,
                display_name: ShellName::blank(),
                shell_type: ShellType::Zsh,
            },
            ctx,
        );

        let size_info = *model.block_list().size();
        let colors = model.colors();
        let model = Arc::new(FairMutex::new(model));

        let (event_loop_tx, event_loop_rx) = async_channel::unbounded();

        let event_loop = Self::create_and_start_event_loop(
            session,
            model.clone(),
            channel_event_proxy.clone(),
            event_loop_rx,
            ctx,
        );

        // Tell the daemon the geometry we are attaching at, before anything is
        // rendered. Without this a session keeps whatever size it was *spawned*
        // with: attach a session spawned at 80x24 into a 200x50 pane and every
        // line wraps at column 80, and `vim`/`htop`/`less` draw at the wrong size
        // until the user happens to resize the pane.
        //
        // The view's own resize path cannot cover this. It emits `Event::Resize`
        // only when `size_update.anything_changed()`, and attaching at exactly
        // the size the `TerminalModel` was just built with changes nothing, so it
        // stays silent precisely in the case that needs a message. Queued rather
        // than sent: it goes through `run_outbound` like any other resize, so the
        // ordering guarantee that keeps stdin in order covers it too.
        if let Err(err) = event_loop_tx.try_send(Message::Resize(size_info)) {
            // The loop was only just created and the channel is unbounded, so
            // this is not reachable in practice; logged rather than ignored
            // because the failure it would represent -- a terminal permanently
            // at the wrong geometry -- is silent and confusing to diagnose.
            log::warn!("could not send the initial resize for a remote session: {err}");
        }

        // Initialize the PtyController.
        let pty_controller = init_pty_controller_model(
            event_loop_tx.clone(),
            executor_command_rx,
            model_events.clone(),
            sessions.clone(),
            model.clone(),
            ctx,
        );

        let cloned_model = model.clone();
        let prompt_type =
            ctx.add_model(|ctx| PromptType::new_dynamic_from_sessions(sessions.clone(), ctx));
        let view = ctx.add_typed_action_view(window_id, |ctx| {
            TerminalView::new(
                resources,
                wakeups_rx,
                model_events.clone(),
                cloned_model,
                sessions.clone(),
                size_info,
                colors,
                model_event_sender.clone(),
                prompt_type,
                initial_input_config,
                None, // conversation_restoration - not used for a daemon-owned session
                None, // inactive_pty_reads_rx
                ctx,
            )
        });

        wire_up_pty_controller_with_surface(
            &pty_controller,
            &view,
            model.clone(),
            sessions,
            model_event_sender,
            ctx,
        );

        let terminal_view = view.clone();

        // Create the terminal manager itself.
        let terminal_manager = Self {
            model,
            view,
            _pty_controller: pty_controller,
            _event_loop: event_loop,
        };

        let manager_model = ctx.add_model(|_ctx| {
            let manager: Box<dyn crate::terminal::TerminalManager> = Box::new(terminal_manager);
            manager
        });

        (terminal_view, manager_model)
    }

    /// Consumes the witness: the host and session ids reach the event loop only
    /// by taking it apart here, so there is no path to `EventLoop::start` that
    /// skipped the check.
    fn create_and_start_event_loop(
        session: ConnectedRemotePtySession,
        terminal_model: Arc<FairMutex<TerminalModel>>,
        channel_event_listener: ChannelEventListener,
        message_receiver: Receiver<Message>,
        ctx: &mut AppContext,
    ) -> ModelHandle<EventLoop> {
        let ConnectedRemotePtySession { host_id, session } = session;
        ctx.add_model(|ctx| {
            EventLoop::start(
                host_id,
                session,
                terminal_model,
                message_receiver,
                channel_event_listener,
                ctx,
            )
        })
    }
}

impl crate::terminal::TerminalManager for TerminalManager {
    fn model(&self) -> Arc<FairMutex<TerminalModel>> {
        self.model.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
#[path = "terminal_manager_tests.rs"]
mod tests;
