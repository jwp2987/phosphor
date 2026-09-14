//! The session-creation path: mint a [`RemotePtySessionId`], ask a host's
//! daemon to spawn a pty session under it, and -- only once the daemon has
//! answered -- hand back the [`ConnectedRemotePtySession`] witness that
//! [`super::TerminalManager::create_model`] demands.
//!
//! **Nothing calls this yet, and that is the increment boundary, not an
//! oversight.** This module takes a host and hands back a witness; turning that
//! witness into a visible terminal needs the pane layer -- `TerminalViewResources`,
//! a pane size, a `WindowId`, a `model_event_sender` -- which lives in
//! `pane_group`. A `WorkspaceAction` for it was written alongside this and then
//! removed: `Workspace::handle_action` matches that enum with no wildcard, so the
//! variant would not compile without a dispatch arm, and the arm had nothing to
//! call. A user-facing action that does nothing is worse than no action.
//!
//! What the next increment needs, in order:
//!
//! 1. A `PaneGroup` path that calls [`super::TerminalManager::create_model`] with
//!    the witness and pushes the result, alongside `create_session`'s `cfg_if`.
//! 2. A `WorkspaceAction` and its dispatch arm, reinstating what was removed.
//! 3. `SpawnSession.bootstrap_session_id` in the proto -- necessary but not
//!    sufficient. `resolve_shell_starter` must also use the supplied id for
//!    *argv*, not only for the out-of-band script, or bash and fish keep emitting
//!    hooks against an id the client never registered.
//!
//! This is item 6's frontier in `docs/design/moth-parliament.md` ("Scoping
//! session ownership"): everything downstream of here was built and latent
//! because nothing constructed a daemon-owned session. This module is the thing
//! that constructs one.
//!
//! **It deliberately stops short of the pane.** [`spawn_remote_session`] hands
//! its caller a witness and nothing else: `create_model` needs
//! `TerminalViewResources`, a pane size, a `WindowId` and a
//! `model_event_sender`, all of which only the pane layer has. The witness *is*
//! the seam -- it means "the daemon has this session, you may now build a
//! terminal on it" -- and keeping the pane construction on the caller's side is
//! what lets this module stay testable without a window, a font cache or a live
//! view.
//!
//! # Why the witness is built in the response callback and not before
//!
//! `RemoteServerClient::spawn_session` is an `async` RPC; `create_model` needs a
//! `&mut AppContext`, which exists only on the main thread. The two are joined
//! by `ViewContext::spawn(future, callback)`: the future runs on the background
//! executor and the callback runs on the main thread with a live context. The
//! witness is constructed *inside that callback*, from the response, so the
//! obligation [`ConnectedRemotePtySession`]'s doc comment records -- "the
//! session has already been spawned" -- is satisfied by construction rather
//! than by a caller remembering to.
//!
//! `terminal/view/docker_sandbox/mod.rs` already uses exactly this shape
//! (`resolve_sbx_path_from_user_shell` on the executor, manager creation and
//! pane push in the callback), so this is the established sequencing in this
//! codebase rather than a new one.
//!
//! # The bootstrap question, not papered over
//!
//! Every session this path creates is spawned with `no_bootstrap = true`: a
//! plain shell with no Warp shell integration. That is not a preference, it is
//! the only honest value available today, and `NO_BOOTSTRAP` carries the
//! argument in full. The one wire field that would change it is named there
//! too.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use remote_server::RemotePtySessionId;
use remote_server::client::{ClientError, RemoteServerClient};
use remote_server::proto::RemoteSessionSignal;
use warp_core::HostId;
use warpui::{AppContext, Entity, SingletonEntity, ViewContext};

use crate::remote_server::manager::RemoteServerManager;

use super::ConnectedRemotePtySession;

/// `SpawnSession.no_bootstrap`, and it is `true` for **every** session this path
/// creates. A daemon-owned session opened from this client is therefore a plain
/// shell: the user's own `~/.zshrc` or `~/.bashrc`, no injected rcfile, no
/// InitShell handshake, and so no block detection, no prompt chips and no
/// command boundaries. It renders and it accepts input; it is not a warpified
/// session.
///
/// **`false` is not the better option today -- it is the broken one**, and this
/// is the part the design doc's gap-4 entry does not say in so many words,
/// because gap 4 was about zsh and this is about every other shell.
///
/// The daemon resolves its shell through
/// `DirectShellStarter::for_explicit_shell`, which **mints its own
/// `SessionId`** and bakes it into argv -- bash via `--rcfile <(echo ...)`,
/// fish via `--init-command`, PowerShell via `-EncodedCommand`
/// (`local_tty/shell.rs::arguments_for_session_spawning_command`). That id is
/// generated on the far host and nothing carries it back:
/// `SpawnSessionSuccess` is an empty message. So the client never registers it,
/// and `Processor::validate_hook_session_id` (`terminal/model/ansi/mod.rs`)
/// rejects every DCS hook the bootstrap emits -- `Bootstrapped`, `Precmd`,
/// `CommandFinished`, `InputBuffer` -- as an unrecognized session id, logging a
/// warning for each. The result is a shell that was told to suppress its own
/// startup files in favour of an integration whose every message this client
/// throws away.
///
/// zsh is the one shell already protected from that, and only by accident of
/// which half it needs: `remote_pty_thread::resolve_shell_starter` sees
/// `needs_out_of_band_init_script() && bootstrap_session_id.is_none()` and
/// strips the bootstrap itself. bash and fish carry theirs in argv, so that
/// guard never fires for them.
///
/// # What would make `false` correct, precisely
///
/// Two changes, neither of which is in this module's reach:
///
/// 1. **A `bootstrap_session_id` field on the `SpawnSession` message**
///    (`crates/remote_server/proto/remote_server.proto`), plumbed to
///    `PtySpawnSpec::bootstrap_session_id`, whose doc comment already describes
///    this exact field as "deliberately not yet a `SpawnSession` field" because
///    the client that would fill it did not exist. It does now. The client half
///    is: mint with `terminal::bootstrap::generate_session_id`, register with
///    `TerminalModel::register_session_id` before the terminal can receive a
///    byte, and send it -- the same order `local_tty::terminal_manager.rs`
///    already uses, and for the same reason ("the shell must never be able to
///    write anything back before its session ID is registered").
/// 2. **`resolve_shell_starter` must use that id for argv too**, not only for
///    the out-of-band stdin writes. Today `bootstrap_session_id` reaches
///    `session_init_script_writes` and the `cannot_bootstrap` gate, while
///    `DirectShellStarter::for_explicit_shell` still mints its own id for
///    bash/fish/PowerShell. Landing (1) alone would fix zsh and leave bash and
///    fish exactly as broken as they are now, which is the failure mode worth
///    naming: the proto field is necessary and not sufficient.
///
/// Until both land, `true` is the value that produces a working shell. It is
/// recorded as a constant rather than a literal so that flipping it is a
/// deliberate edit against this comment.
const NO_BOOTSTRAP: bool = true;

/// The geometry a session is *spawned* at, before the pane it will live in
/// exists.
///
/// Provisional by design, and already corrected: `TerminalManager::create_model`
/// queues a `Message::Resize` carrying the real `SizeInfo` as the first thing on
/// the outbound channel, before anything is rendered -- see its comment on why
/// the view's own resize path cannot cover the attach case. So these two numbers
/// decide only what the shell's very first `winsize` is, for the fraction of a
/// second before the resize lands.
///
/// 24x80 rather than something larger because a too-small initial size is the
/// benign direction: a shell that prints its prompt at 80 columns and is then
/// widened re-wraps correctly, where one that assumes 200 columns and is then
/// narrowed leaves a mangled first line.
pub const DEFAULT_SPAWN_ROWS: u32 = 24;
/// See [`DEFAULT_SPAWN_ROWS`].
pub const DEFAULT_SPAWN_COLS: u32 = 80;

/// What to ask a host's daemon for.
///
/// Mirrors `RemoteServerClient::spawn_session`'s parameters minus the session
/// id, which this module mints, and minus `no_bootstrap`, which is not the
/// caller's to choose (`NO_BOOTSTRAP`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSessionSpawnRequest {
    /// The host whose daemon will own the pty.
    pub host_id: HostId,

    /// The directory the session should start in, or empty for the remote
    /// user's home.
    ///
    /// **Advisory today, and reaching nothing.** The daemon does not `chdir` to
    /// this: `local_tty::unix::build_host_shell_command` starts the child in the
    /// user's home directory and exports this as `WARP_INITIAL_WORKING_DIR` for
    /// the *bootstrap script* to `cd` to, precisely so a nonexistent directory
    /// cannot fail the spawn. With `NO_BOOTSTRAP` there is no script, so
    /// nothing reads the variable and every session starts in `$HOME`.
    ///
    /// It is still sent, because it is also what `ListSessions` reports back as
    /// the session's `cwd` (`SessionSpawnMetadata`), and because it starts
    /// working the moment the bootstrap does.
    pub cwd: String,

    /// An absolute path to a shell on the remote host, or `None` to let the
    /// daemon pick (`ShellStarter::compute_fallback_shell`: the user's passwd
    /// entry, then `/bin/zsh`, `/bin/bash`, `/bin/fish`).
    ///
    /// The daemon only accepts a path whose file name is a shell it knows, and
    /// answers anything else with `unsupported shell: ...` -- a refusal, not a
    /// fallback.
    pub shell: Option<String>,

    /// Extra environment for the spawned shell, merged over the daemon's own.
    ///
    /// Empty is the right default: `TERM`, `TERM_PROGRAM` and `COLORTERM` are
    /// set by the daemon's own spawn path (`build_host_shell_command`), so a
    /// client that sent them would only be restating what is already true.
    pub environment_variables: HashMap<String, String>,

    /// See [`DEFAULT_SPAWN_ROWS`].
    pub rows: u32,
    /// See [`DEFAULT_SPAWN_COLS`].
    pub cols: u32,
}

impl RemoteSessionSpawnRequest {
    /// A session on `host_id` with every other choice left to the far side: the
    /// remote user's home directory, the remote user's shell, no extra
    /// environment, and the provisional geometry of [`DEFAULT_SPAWN_ROWS`].
    pub fn for_host(host_id: HostId) -> Self {
        Self {
            host_id,
            cwd: String::new(),
            shell: None,
            environment_variables: HashMap::new(),
            rows: DEFAULT_SPAWN_ROWS,
            cols: DEFAULT_SPAWN_COLS,
        }
    }
}

/// Why no terminal was opened, and -- the part that matters -- whether a pty may
/// still be running on the host with nothing pointing at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteSessionSpawnFailure {
    /// No connected client for the host, so nothing was ever sent. No session
    /// exists and no id was used.
    NoClientForHost { host_id: HostId },

    /// The daemon answered `SpawnSessionError`. **Nothing is left on the host**:
    /// `ServerModel::handle_spawn_session` removes its `SessionStore`
    /// registration on a failed spawn, and `LiveSession::spawn`'s
    /// `KillPtyOnDrop` kills and reaps a pty that was created before a later
    /// step failed.
    Refused {
        session: RemotePtySessionId,
        detail: String,
    },

    /// The RPC did not complete cleanly and the daemon's answer is unknown, so
    /// a pty **may** be running under `session`. [`OrphanCleanup`] says what was
    /// done about it.
    Indeterminate {
        session: RemotePtySessionId,
        detail: String,
        cleanup: OrphanCleanup,
    },

    /// The daemon spawned the session and then the host's client went away
    /// before a terminal could be built on it, so
    /// [`ConnectedRemotePtySession::for_spawned_session`] refused.
    ///
    /// The session is running, this client cannot reach it, and it cannot be
    /// killed from here either -- the kill is itself an RPC, and there is no
    /// client to carry it. It stays discoverable by `ListSessions` under this
    /// id, which is why the id is carried here and logged rather than dropped.
    ///
    /// Re-adopting it when the host reconnects is the reattach increment's job,
    /// not this one's: `RemoteServerManagerEvent::SessionReconnected` is the
    /// event that would trigger it, and nothing in this module is subscribed.
    SpawnedButUnreachable { session: RemotePtySessionId },
}

impl fmt::Display for RemoteSessionSpawnFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoClientForHost { host_id } => {
                write!(f, "no connected client for host {host_id}")
            }
            Self::Refused { session, detail } => {
                write!(f, "host refused to spawn session {session}: {detail}")
            }
            Self::Indeterminate {
                session,
                detail,
                cleanup,
            } => write!(
                f,
                "spawning session {session} did not complete ({detail}); {cleanup}"
            ),
            Self::SpawnedButUnreachable { session } => write!(
                f,
                "session {session} was spawned but its host became unreachable before a \
                 terminal could attach; it is still running"
            ),
        }
    }
}

/// What became of a session that may have been spawned by an RPC that did not
/// complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrphanCleanup {
    /// The daemon accepted `SignalSession`/`Kill`, which means it had a session
    /// under this id and has signalled it. Whatever the failed spawn left
    /// behind is gone.
    Killed,

    /// The daemon did not accept the kill, and its reason is carried verbatim.
    ///
    /// **This is not by itself evidence of an orphan, and reading it as one is
    /// the mistake worth naming.** An id the daemon holds no session under is
    /// answered with an *error*, not with success:
    /// `ServerModel::handle_signal_session` returns whatever
    /// `PtySessionOperations::signal` gives it, and every backend gives
    /// `PtySessionOpError::unknown_session` -- "no session registered under id
    /// ...". So that particular message here is the *good* outcome: the spawn
    /// never took effect and there was nothing to clean up.
    ///
    /// What this side cannot do is tell that apart from a kill that never
    /// reached the host at all, and it deliberately does not try: the
    /// distinguishing evidence is a human-readable message on the wire, and
    /// matching on its text would make this classification depend on a string
    /// the daemon is free to reword. The message is carried instead of
    /// summarised so whoever reads the report has what this code cannot use.
    NotKilled(String),
}

impl fmt::Display for OrphanCleanup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Killed => f.write_str("a session it had started was killed"),
            Self::NotKilled(detail) => write!(
                f,
                "the host did not accept a kill for it ({detail}) -- which is also what it \
                 answers for an id it never spawned, so this is only a possible orphan"
            ),
        }
    }
}

/// Whether a failed `SpawnSession` can have left a pty running on the host.
///
/// The distinction is the whole reason this classification exists: one of these
/// is a plain error to report, and the other is a resource on someone else's
/// machine that nothing points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpawnAftermath {
    /// Nothing is running under this id.
    NothingSpawned,
    /// A pty may be running under this id.
    PossiblyOrphaned,
}

/// Classifies a `spawn_session` error by what it implies about the far side.
///
/// The match is exhaustive with no wildcard, for the same reason the manager
/// subscription in `event_loop.rs` is: a new `ClientError` variant must force a
/// decision here rather than inherit whichever answer a `_` arm happened to
/// give. Defaulting a new variant to [`SpawnAftermath::NothingSpawned`] would
/// silently stop reporting orphans.
///
/// Two arms are load-bearing and both were read rather than assumed:
///
/// - **`SessionOperationFailed` is the daemon's own answer.**
///   `RemoteServerClient::spawn_session` produces it only from
///   `spawn_session_response::Result::Error`, which
///   `ServerModel::handle_spawn_session` returns *after* calling
///   `self.session_store.remove(&id)` on a failed `pty_ops.spawn` -- and
///   `LiveSession::spawn` guards every fallible step after the pty exists with
///   `KillPtyOnDrop`. So this is genuinely "nothing was left behind", not an
///   optimistic reading of an error string.
/// - **`Disconnected` means the request was never sent.**
///   `RemoteServerClient::send_request` returns it in exactly two places, both
///   *before* the message reaches the wire: the `self.disconnected` check, and
///   `outbound_tx.send(msg).await.is_err()`, which fails because the writer is
///   gone. A connection that drops after a successful enqueue surfaces as
///   `ResponseChannelClosed` or `Timeout` instead, and both of those are in the
///   orphan arm. **If `send_request` ever enqueues before checking, or grows a
///   retry, this arm has to move.** It is worth the precision because
///   "disconnected host" is the common failure and warning about a possible
///   orphan on every one of them is how a real orphan warning gets ignored.
pub(super) fn spawn_aftermath_for(error: &ClientError) -> SpawnAftermath {
    match error {
        ClientError::SessionOperationFailed(_) | ClientError::Disconnected => {
            SpawnAftermath::NothingSpawned
        }
        // Everything below reaches the daemon, or may have.
        //
        // `ServerError` is a generic `ErrorResponse` in place of a
        // `SpawnSessionResponse`, which is not something `handle_spawn_session`
        // can produce -- so it came from a layer this side cannot reason about,
        // and "the handler never ran" is an assumption, not a fact.
        //
        // `FileOperationFailed` is unreachable for this RPC (it is the
        // file-operation arm of the same enum) and is grouped here rather than
        // given its own arm, so that if it ever does become reachable it
        // defaults to the conservative answer instead of hiding an orphan.
        ClientError::Protocol(_)
        | ClientError::ResponseChannelClosed
        | ClientError::UnexpectedResponse
        | ClientError::ServerError { .. }
        | ClientError::Timeout(_)
        | ClientError::FileOperationFailed(_) => SpawnAftermath::PossiblyOrphaned,
    }
}

/// What the background half of the spawn came back with.
///
/// `ClientError` itself never crosses this boundary: `ViewContext::spawn`
/// requires a `Send` output, and `remote_sessions_model.rs` already flattens
/// `ClientError` to a `String` at the same seam for the same reason. Flattening
/// here also means the *classification* happens while the client is still in
/// hand, which is what lets the orphan cleanup run in the same future rather
/// than needing a second round trip through the main thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SpawnRpcOutcome {
    /// There was no client to send on; nothing was attempted.
    NoClient,
    /// The daemon answered `SpawnSessionSuccess`.
    Spawned,
    /// The daemon refused. Nothing is running under the id.
    Refused(String),
    /// The RPC did not complete; a pty may be running under the id.
    Indeterminate {
        detail: String,
        cleanup: OrphanCleanup,
    },
}

/// Maps a completed RPC onto the failure to report, or `None` when the caller
/// should go on to build the witness.
///
/// Split out from [`finish_spawn`] because it is the whole decision minus the
/// one step that needs an `AppContext`, and so is the part a test can pin.
/// `None` is not "success": it is "the daemon says this session exists", which
/// is necessary and not sufficient -- the witness still has to be obtainable.
pub(super) fn failure_for(
    session: RemotePtySessionId,
    outcome: SpawnRpcOutcome,
    host_id: &HostId,
) -> Option<RemoteSessionSpawnFailure> {
    match outcome {
        SpawnRpcOutcome::Spawned => None,
        SpawnRpcOutcome::NoClient => Some(RemoteSessionSpawnFailure::NoClientForHost {
            host_id: host_id.clone(),
        }),
        SpawnRpcOutcome::Refused(detail) => {
            Some(RemoteSessionSpawnFailure::Refused { session, detail })
        }
        SpawnRpcOutcome::Indeterminate { detail, cleanup } => {
            Some(RemoteSessionSpawnFailure::Indeterminate {
                session,
                detail,
                cleanup,
            })
        }
    }
}

/// Spawns a daemon-owned pty session on `request.host_id` and calls `on_outcome`
/// on the main thread with the witness that lets a terminal be built on it.
///
/// The witness is the only thing handed back, and building the pane is the
/// caller's: see this module's doc comment for why the seam is here.
///
/// **Every path reaches `on_outcome` exactly once, including the ones that fail
/// before any RPC is sent** -- the "no client for this host" case goes through
/// the same future so a caller has one place to handle failure rather than two.
/// The one exception is not this function's to fix: `ViewContext::spawn`'s
/// callback needs a live `&mut V`, so if the calling view is dropped while the
/// RPC is in flight the callback is dropped with it. That window is why a
/// successful spawn is logged from inside the future, before the callback is
/// ever reached: the id of a session that nothing ends up pointing at must be
/// recoverable from the log.
pub fn spawn_remote_session<V, F>(
    request: RemoteSessionSpawnRequest,
    ctx: &mut ViewContext<V>,
    on_outcome: F,
) where
    V: Entity,
    F: 'static
        + FnOnce(
            &mut V,
            Result<ConnectedRemotePtySession, RemoteSessionSpawnFailure>,
            &mut ViewContext<V>,
        ),
{
    // Minted here, on the main thread, before anything is sent. The id is the
    // client's to choose (`RemotePtySessionId`'s doc comment: a retried
    // `SpawnSession` under the same id is idempotent rather than spawning a
    // second pty), and having it in hand before the RPC is what makes an
    // orphaned session nameable in the failure paths below.
    let session = RemotePtySessionId::new();
    let host_id = request.host_id.clone();

    // The client lookup is a main-thread read of the manager singleton, so it
    // happens here rather than inside the future.
    let client = connected_client(&host_id, ctx);

    let session_for_rpc = session.clone();
    let host_for_rpc = host_id.clone();
    ctx.spawn(
        async move {
            let Some(client) = client else {
                return SpawnRpcOutcome::NoClient;
            };
            let outcome = spawn_and_settle(client, session_for_rpc.clone(), request).await;
            if matches!(outcome, SpawnRpcOutcome::Spawned) {
                // Logged from the future, not the callback: see this function's
                // doc comment on the dropped-view window.
                log::info!("spawned remote session {session_for_rpc} on host {host_for_rpc}");
            }
            outcome
        },
        move |view, outcome, ctx| {
            let result = finish_spawn(host_id, session, outcome, ctx);
            on_outcome(view, result, ctx);
        },
    );
}

/// The connected client for `host_id`, or `None` with a reason in the log.
///
/// Mirrors the check in [`ConnectedRemotePtySession::for_spawned_session`]
/// deliberately rather than calling it: that one is a *post-spawn* witness and
/// must stay so, while this is the pre-flight question "is there anything to
/// send on at all". The two are asked at different times and a host can change
/// state in between, which is exactly the
/// [`RemoteSessionSpawnFailure::SpawnedButUnreachable`] case.
fn connected_client(host_id: &HostId, ctx: &AppContext) -> Option<Arc<RemoteServerClient>> {
    // `RemoteServerManager::as_ref` panics rather than returning an `Option`
    // when no manager is registered (a test, or a build where the remote-server
    // feature never registered one), so the membership check has to come first.
    if !ctx.has_singleton_model::<RemoteServerManager>() {
        log::warn!("no RemoteServerManager registered; cannot spawn a session on host {host_id}");
        return None;
    }

    let client = RemoteServerManager::as_ref(ctx)
        .client_for_host(host_id)
        .cloned();
    if client.is_none() {
        log::warn!("no connected client for host {host_id}; cannot spawn a session on it");
    }
    client
}

/// Sends the `SpawnSession` RPC and, when its outcome is ambiguous, tries to
/// clean up after itself.
///
/// The cleanup is best-effort and deliberate: an RPC that neither succeeded nor
/// was refused may have left a pty running that nothing will ever attach to, and
/// leaving it there is how a remote host accumulates shells nobody remembers
/// asking for. `SignalSession`/`Kill` is the protocol's own client-visible kill,
/// and sending it for an id the daemon never spawned changes nothing on the host
/// -- it is answered with "no session registered under id ...", which
/// [`OrphanCleanup::NotKilled`] is careful not to report as an orphan.
///
/// **The race it does not close, recorded rather than rounded off.** On
/// `ClientError::Timeout` the original request may still be in flight, so the
/// kill can arrive *first*, find nothing, and be followed by the spawn it was
/// meant to undo. That leaves exactly the orphan this is trying to prevent. The
/// id is still the one in the returned failure and in the log, so the session is
/// nameable; closing the race properly needs the daemon to refuse a spawn for an
/// id it has already been asked to kill, which is a daemon-side change.
async fn spawn_and_settle(
    client: Arc<RemoteServerClient>,
    session: RemotePtySessionId,
    request: RemoteSessionSpawnRequest,
) -> SpawnRpcOutcome {
    // Destructured by name so the two adjacent `u32`s cannot be swapped
    // silently at this call site; `spawn_session` takes them positionally.
    let RemoteSessionSpawnRequest {
        host_id: _,
        cwd,
        shell,
        environment_variables,
        rows,
        cols,
    } = request;

    let error = match client
        .spawn_session(
            session.clone(),
            cwd,
            shell,
            environment_variables,
            rows,
            cols,
            NO_BOOTSTRAP,
            // `None` until this path registers an id with the `TerminalModel`
            // that will render the session. The wire can carry one now
            // (`SpawnSession.bootstrap_session_id`) and the daemon binds both
            // argv and the init script to it, but registering has to happen on
            // the model `create_model` builds -- which does not exist until
            // after this response lands. Supplying an unregistered id would be
            // worse than none: every hook the bootstrap emits would be rejected.
            // Closing this is what flips `NO_BOOTSTRAP` to `false`.
            None,
        )
        .await
    {
        Ok(()) => return SpawnRpcOutcome::Spawned,
        Err(error) => error,
    };

    let detail = error.to_string();
    match spawn_aftermath_for(&error) {
        SpawnAftermath::NothingSpawned => SpawnRpcOutcome::Refused(detail),
        SpawnAftermath::PossiblyOrphaned => {
            log::warn!(
                "spawning remote session {session} did not complete ({detail}); killing it in \
                 case the host started it anyway"
            );
            let cleanup = match client
                .signal_session(session.clone(), RemoteSessionSignal::Kill)
                .await
            {
                Ok(()) => OrphanCleanup::Killed,
                Err(kill_error) => {
                    // `warn!` rather than `error!`: the most likely reason is
                    // that the spawn never took effect and the daemon has no
                    // such session, which is the outcome we wanted.
                    log::warn!(
                        "kill not accepted for possibly-orphaned remote session {session}: \
                         {kill_error:?}"
                    );
                    OrphanCleanup::NotKilled(kill_error.to_string())
                }
            };
            SpawnRpcOutcome::Indeterminate { detail, cleanup }
        }
    }
}

/// Turns a completed RPC into either a witness or a failure, on the main thread.
///
/// The witness is built here and nowhere earlier. That ordering is the
/// constraint [`ConnectedRemotePtySession`] exists to express, and it is what
/// makes `TerminalManager::create_model` unreachable for a session the daemon
/// has not confirmed.
fn finish_spawn<V>(
    host_id: HostId,
    session: RemotePtySessionId,
    outcome: SpawnRpcOutcome,
    ctx: &mut ViewContext<V>,
) -> Result<ConnectedRemotePtySession, RemoteSessionSpawnFailure> {
    if let Some(failure) = failure_for(session.clone(), outcome, &host_id) {
        log::warn!("no remote terminal opened on host {host_id}: {failure}");
        return Err(failure);
    }

    ConnectedRemotePtySession::for_spawned_session(host_id, session.clone(), ctx).ok_or_else(|| {
        let failure = RemoteSessionSpawnFailure::SpawnedButUnreachable { session };
        // `error!`, not `warn!`: every other failure here leaves the host as it
        // was, and this one leaves a process running on someone else's machine
        // with no handle on it anywhere in this app.
        log::error!("{failure}");
        failure
    })
}

#[cfg(test)]
#[path = "session_spawn_tests.rs"]
mod tests;
