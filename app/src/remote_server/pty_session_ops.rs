//! The pty seam the daemon's five session RPCs (`SpawnSession`,
//! `WriteSessionStdin`, `ResizeSession`, `SignalSession`, `ListSessions`,
//! `crates/remote_server/proto/remote_server.proto`) dispatch through in
//! `server_model.rs`.
//!
//! Every pty operation is behind [`PtySessionOperations`] so the dispatch,
//! the `remote_server::session_store::SessionStore` integration, the
//! response shapes and the error paths in `server_model.rs` can all be
//! proven by unit tests that spawn no process -- see
//! [`FakePtySessionOperations`]. The trait exists precisely to keep that
//! dispatch logic testable without a real pty. The real backend itself is a
//! narrow, deliberate exception: `pty_session_ops_tests.rs` has exactly one
//! test that does spawn a process, because there is no other way to prove
//! `LocalTtyPtySessionOperations` actually owns one.
//!
//! Mostly ctx-free: only `spawn` takes a `ModelContext` / `AppContext`, since
//! starting a real pty needs one (`local_tty::Pty::new` looks up the
//! `PtySpawner` singleton through it) and nothing else does.
//! `write_stdin`/`resize`/`signal`/`kill` all act on an already-spawned pty
//! and stay ctx-free, so most of `server_model_tests.rs`'s dispatch tests
//! stay plain unit tests with no `App` behind them (see its comment on
//! `subscribe_git_status_records_subscriber_and_current_repo` for this
//! fork's established convention there); the tests that exercise `spawn`
//! wrap in `warpui::App::test` instead, the same way other ctx-taking
//! handlers in that file already do.

use std::collections::HashMap;
use std::sync::Mutex;

use remote_server::RemotePtySessionId;
use remote_server::session_store::SessionExitStatus;
use warpui::AppContext;

use super::proto::RemoteSessionSignal;

#[cfg(all(feature = "local_tty", unix))]
mod remote_pty_thread;

/// What a session is spawned with -- the daemon-side counterpart of
/// `SpawnSession`'s fields. Kept as its own type, rather than passing the
/// proto message straight through, so this trait doesn't carry wire/proto
/// concerns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtySpawnSpec {
    pub cwd: String,
    pub shell: Option<String>,
    pub environment_variables: HashMap<String, String>,
    pub rows: u32,
    pub cols: u32,
}

/// A pty operation failed. Carries only a message because every one of the
/// five RPCs' wire error variants (`SpawnSessionError`, `WriteSessionStdinError`,
/// `ResizeSessionError`, `SignalSessionError`, `ListSessionsError`) is itself
/// just `{ message: String }` -- see `remote_server.proto` -- so this maps
/// onto any of them without needing a per-operation error enum.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PtySessionOpError(pub String);

impl PtySessionOpError {
    /// The error every backend returns for an id it has no live session
    /// under. Extracted so [`FakePtySessionOperations`] and
    /// [`LocalTtyPtySessionOperations`] produce identically-shaped messages.
    pub fn unknown_session(id: &RemotePtySessionId) -> Self {
        Self(format!("no session registered under id {id}"))
    }

    /// An error with an arbitrary message, for backends reporting something
    /// other than an unknown id.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Events the real pty backend forwards toward `ServerModel`
/// (`handle_pty_session_output` / `handle_pty_session_exit`) as they occur.
///
/// Defined here, rather than inside the unix-only `remote_pty_thread`
/// submodule that produces them, so `ServerModel::new` can build the channel
/// and every `LocalTtyPtySessionOperations` variant -- including the two that
/// never actually spawn anything -- can share one `new` signature regardless
/// of platform or the `local_tty` feature.
pub enum PtySessionEvent {
    Output(Vec<u8>),
    Exited(SessionExitStatus),
}

/// The pty operations the daemon's five session RPCs dispatch through.
///
/// Five methods, not four: `signal` is how the wire asks a *running*
/// process to react (interrupt, terminate, its own kill signal, hang up --
/// `SignalSession` carries `RemoteSessionSignal::Kill` for the client-visible
/// "kill this session" action, matching the proto doc comment "there is no
/// separate kill/terminate-session request"). `kill` is a different,
/// daemon-internal operation: it releases the OS resources behind an id once
/// its process is already gone (mirrors `local_tty::EventedPty::kill`, which
/// drops the pty fd and reaps the child). Nothing on the wire calls `kill`
/// directly -- `ServerModel` calls it once it has recorded an exit.
pub trait PtySessionOperations: Send + Sync {
    /// Starts a new pty-backed process for `id`. Called at most once per id:
    /// `ServerModel::handle_spawn_session` gates a retried `SpawnSession` on
    /// `SessionStore` registration before ever reaching here, so this trait
    /// does not need its own idempotency.
    ///
    /// Takes `ctx` -- unlike the four operations below, which all act on an
    /// already-spawned pty and need nothing from the entity system -- because
    /// starting a real one requires it: `local_tty::Pty::new` looks up the
    /// `PtySpawner` singleton through `ctx`. It is available at every
    /// dispatch site in `server_model.rs` already (`ModelContext` derefs to
    /// `AppContext`), so threading it through costs nothing at the call site.
    fn spawn(
        &self,
        id: &RemotePtySessionId,
        spec: &PtySpawnSpec,
        ctx: &mut AppContext,
    ) -> Result<(), PtySessionOpError>;

    /// Writes `data` to `id`'s stdin. Errors (rather than silently dropping
    /// the bytes) if `id` names no live session.
    fn write_stdin(&self, id: &RemotePtySessionId, data: &[u8]) -> Result<(), PtySessionOpError>;

    /// Resizes `id`'s pty. Errors if `id` names no live session.
    fn resize(
        &self,
        id: &RemotePtySessionId,
        rows: u32,
        cols: u32,
    ) -> Result<(), PtySessionOpError>;

    /// Sends `signal` to `id`'s process (group). Errors if `id` names no
    /// live session.
    fn signal(
        &self,
        id: &RemotePtySessionId,
        signal: RemoteSessionSignal,
    ) -> Result<(), PtySessionOpError>;

    /// Releases the OS resources behind `id`, once its process has already
    /// exited. Not an error to call on an id this backend never spawned or
    /// has already released: exit is recorded by `SessionStore` regardless
    /// of whether this cleanup step finds anything to do, so this must not
    /// be a second source of failure for the same exit.
    fn kill(&self, id: &RemotePtySessionId);
}

/// Converts a process's real exit outcome into the store's
/// [`remote_server::session_store::SessionExitStatus`].
///
/// A process terminated by a signal has no exit code -- `ExitStatus::code`
/// documents exactly this: `None` iff the process was terminated by a
/// signal. This must not fabricate a code for that case; that is what
/// `SessionExitStatus::signalled()` (no code) exists to distinguish from
/// `SessionExitStatus::exited(code)`.
pub fn session_exit_status_from_process(
    exit_status: std::process::ExitStatus,
) -> remote_server::session_store::SessionExitStatus {
    match exit_status.code() {
        Some(code) => remote_server::session_store::SessionExitStatus::exited(code),
        None => remote_server::session_store::SessionExitStatus::signalled(),
    }
}

/// Real backend for [`PtySessionOperations`] on unix, backed by
/// `app::terminal::local_tty`. See `remote_pty_thread` (this module's
/// unix-only submodule) for the dedicated-OS-thread-per-session mechanism
/// that owns each spawned pty; this type is just the map from id to a live
/// session plus the channel new sessions forward events through.
///
/// `docs/design/moth-parliament.md`'s dependency-ordered work list names
/// this as its own item -- "3. Remote-side ownership: the daemon holds the
/// pty" -- distinct from "2. Session operations" (the dispatch
/// `server_model.rs` already does). This is that item.
#[cfg(all(feature = "local_tty", unix))]
pub struct LocalTtyPtySessionOperations {
    sessions: Mutex<HashMap<RemotePtySessionId, remote_pty_thread::LiveSession>>,
    events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>,
}

#[cfg(all(feature = "local_tty", unix))]
impl LocalTtyPtySessionOperations {
    pub fn new(events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            events_tx,
        }
    }
}

#[cfg(all(feature = "local_tty", unix))]
impl PtySessionOperations for LocalTtyPtySessionOperations {
    fn spawn(
        &self,
        id: &RemotePtySessionId,
        spec: &PtySpawnSpec,
        ctx: &mut AppContext,
    ) -> Result<(), PtySessionOpError> {
        let session =
            remote_pty_thread::LiveSession::spawn(id.clone(), spec, ctx, self.events_tx.clone())?;
        self.sessions.lock().unwrap().insert(id.clone(), session);
        Ok(())
    }

    fn write_stdin(&self, id: &RemotePtySessionId, data: &[u8]) -> Result<(), PtySessionOpError> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(id)
            .ok_or_else(|| PtySessionOpError::unknown_session(id))?;
        session.write_stdin(data.to_vec());
        Ok(())
    }

    fn resize(
        &self,
        id: &RemotePtySessionId,
        rows: u32,
        cols: u32,
    ) -> Result<(), PtySessionOpError> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(id)
            .ok_or_else(|| PtySessionOpError::unknown_session(id))?;
        session.resize(rows, cols);
        Ok(())
    }

    fn signal(
        &self,
        id: &RemotePtySessionId,
        signal: RemoteSessionSignal,
    ) -> Result<(), PtySessionOpError> {
        let pid = {
            let sessions = self.sessions.lock().unwrap();
            let session = sessions
                .get(id)
                .ok_or_else(|| PtySessionOpError::unknown_session(id))?;
            session.pid()
        };
        // A running terminal delivers Ctrl-C/Ctrl-\ through the tty driver to
        // whichever process group currently owns the foreground; sending an
        // explicit signal to the shell's own process group (see
        // `remote_pty_thread::send_signal_to_process_group`) is the closest
        // equivalent available over this wire protocol.
        let raw_signal = match signal {
            RemoteSessionSignal::Interrupt => libc::SIGINT,
            RemoteSessionSignal::Terminate => libc::SIGTERM,
            RemoteSessionSignal::Kill => libc::SIGKILL,
            RemoteSessionSignal::Hangup => libc::SIGHUP,
            // `handle_signal_session` already rejects this before ever
            // reaching a backend; handled here only to keep the match
            // exhaustive.
            RemoteSessionSignal::Unspecified => {
                return Err(PtySessionOpError::new(
                    "cannot send an unspecified signal to a pty",
                ));
            }
        };
        remote_pty_thread::send_signal_to_process_group(pid, raw_signal)
            .map_err(|err| PtySessionOpError::new(format!("failed to send signal: {err}")))
    }

    fn kill(&self, id: &RemotePtySessionId) {
        // `remove` makes this safe to call twice: the first call takes the
        // only `LiveSession` for `id` out of the map, so a second call (or a
        // call for an id this backend never spawned) finds nothing and is a
        // no-op, matching the trait's contract.
        if let Some(session) = self.sessions.lock().unwrap().remove(id) {
            session.join();
        }
    }
}

/// Stub backend for `local_tty` builds on a non-unix target. The real
/// backend (`remote_pty_thread`) registers the pty's raw fd with mio through
/// `mio::unix::SourceFd`, which has no Windows equivalent here, so this type
/// exists only so the crate still compiles off unix; it never actually spawns
/// anything.
#[cfg(all(feature = "local_tty", not(unix)))]
pub struct LocalTtyPtySessionOperations;

#[cfg(all(feature = "local_tty", not(unix)))]
impl LocalTtyPtySessionOperations {
    pub fn new(_events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>) -> Self {
        Self
    }
}

#[cfg(all(feature = "local_tty", not(unix)))]
impl PtySessionOperations for LocalTtyPtySessionOperations {
    fn spawn(
        &self,
        id: &RemotePtySessionId,
        _spec: &PtySpawnSpec,
        _ctx: &mut AppContext,
    ) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError(format!(
            "remote pty spawning is not implemented on this platform yet for session {id}"
        )))
    }

    fn write_stdin(&self, id: &RemotePtySessionId, _data: &[u8]) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError::unknown_session(id))
    }

    fn resize(
        &self,
        id: &RemotePtySessionId,
        _rows: u32,
        _cols: u32,
    ) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError::unknown_session(id))
    }

    fn signal(
        &self,
        id: &RemotePtySessionId,
        _signal: RemoteSessionSignal,
    ) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError::unknown_session(id))
    }

    fn kill(&self, _id: &RemotePtySessionId) {}
}

/// Fallback backend when the `local_tty` feature is off. Behaves exactly
/// like [`LocalTtyPtySessionOperations`] (nothing is ever spawned, so every
/// id is unknown), but says why in its error rather than pointing at a doc
/// comment on a type that isn't compiled in.
#[cfg(not(feature = "local_tty"))]
pub struct LocalTtyPtySessionOperations;

#[cfg(not(feature = "local_tty"))]
impl LocalTtyPtySessionOperations {
    pub fn new(_events_tx: async_channel::Sender<(RemotePtySessionId, PtySessionEvent)>) -> Self {
        Self
    }
}

#[cfg(not(feature = "local_tty"))]
impl PtySessionOperations for LocalTtyPtySessionOperations {
    fn spawn(
        &self,
        _id: &RemotePtySessionId,
        _spec: &PtySpawnSpec,
        _ctx: &mut AppContext,
    ) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError(
            "remote pty sessions require the local_tty feature".to_string(),
        ))
    }

    fn write_stdin(&self, id: &RemotePtySessionId, _data: &[u8]) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError::unknown_session(id))
    }

    fn resize(
        &self,
        id: &RemotePtySessionId,
        _rows: u32,
        _cols: u32,
    ) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError::unknown_session(id))
    }

    fn signal(
        &self,
        id: &RemotePtySessionId,
        _signal: RemoteSessionSignal,
    ) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError::unknown_session(id))
    }

    fn kill(&self, _id: &RemotePtySessionId) {}
}

/// Test double for [`PtySessionOperations`]. Records every call instead of
/// touching an OS process, so `server_model_tests.rs` can drive
/// `server_model.rs`'s dispatch deterministically. Not `#[cfg(test)]`-gated
/// (mirrors `app::terminal::mock_terminal_manager::MockTerminalManager`),
/// so it is available to any test module that needs it.
#[derive(Default)]
pub struct FakePtySessionOperations {
    state: Mutex<FakeState>,
}

#[derive(Default)]
struct FakeState {
    spawned: HashMap<RemotePtySessionId, PtySpawnSpec>,
    spawn_calls: Vec<RemotePtySessionId>,
    stdin_writes: Vec<(RemotePtySessionId, Vec<u8>)>,
    resizes: Vec<(RemotePtySessionId, u32, u32)>,
    signals: Vec<(RemotePtySessionId, RemoteSessionSignal)>,
    killed: Vec<RemotePtySessionId>,
    /// When set, `spawn` fails with this message instead of succeeding.
    ///
    /// Exists because a double that can only succeed cannot exercise a
    /// handler's failure path: the store cleanup that runs when a spawn fails
    /// was correct but unreachable from any test until this was added.
    spawn_failure: Option<String>,
}

impl FakePtySessionOperations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every subsequent `spawn` fail with `message`, so a test can drive
    /// the handler's spawn-failure path.
    pub fn fail_spawns_with(&self, message: impl Into<String>) {
        self.state.lock().unwrap().spawn_failure = Some(message.into());
    }

    /// How many times `spawn` actually started a session for `id` -- used to
    /// prove a retried `SpawnSession` starts exactly one pty.
    pub fn spawn_count(&self, id: &RemotePtySessionId) -> usize {
        self.state
            .lock()
            .unwrap()
            .spawn_calls
            .iter()
            .filter(|spawned| *spawned == id)
            .count()
    }

    /// Every `spec` a `spawn` call for `id` was given, in call order.
    pub fn spawn_specs_for(&self, id: &RemotePtySessionId) -> Vec<PtySpawnSpec> {
        let state = self.state.lock().unwrap();
        state
            .spawn_calls
            .iter()
            .filter(|spawned| *spawned == id)
            .filter_map(|spawned| state.spawned.get(spawned).cloned())
            .collect()
    }

    pub fn stdin_writes_for(&self, id: &RemotePtySessionId) -> Vec<Vec<u8>> {
        self.state
            .lock()
            .unwrap()
            .stdin_writes
            .iter()
            .filter(|(written, _)| written == id)
            .map(|(_, data)| data.clone())
            .collect()
    }

    pub fn last_resize(&self, id: &RemotePtySessionId) -> Option<(u32, u32)> {
        self.state
            .lock()
            .unwrap()
            .resizes
            .iter()
            .rev()
            .find(|(resized, _, _)| resized == id)
            .map(|(_, rows, cols)| (*rows, *cols))
    }

    pub fn signals_for(&self, id: &RemotePtySessionId) -> Vec<RemoteSessionSignal> {
        self.state
            .lock()
            .unwrap()
            .signals
            .iter()
            .filter(|(signalled, _)| signalled == id)
            .map(|(_, signal)| *signal)
            .collect()
    }

    pub fn was_killed(&self, id: &RemotePtySessionId) -> bool {
        self.state
            .lock()
            .unwrap()
            .killed
            .iter()
            .any(|killed| killed == id)
    }
}

impl PtySessionOperations for FakePtySessionOperations {
    fn spawn(
        &self,
        id: &RemotePtySessionId,
        spec: &PtySpawnSpec,
        _ctx: &mut AppContext,
    ) -> Result<(), PtySessionOpError> {
        let mut state = self.state.lock().unwrap();
        if let Some(message) = state.spawn_failure.clone() {
            // Recorded as an attempt even though it failed: a test asserting a
            // retry starts exactly one pty needs to see attempts, not successes.
            state.spawn_calls.push(id.clone());
            return Err(PtySessionOpError::new(message));
        }
        state.spawned.insert(id.clone(), spec.clone());
        state.spawn_calls.push(id.clone());
        Ok(())
    }

    fn write_stdin(&self, id: &RemotePtySessionId, data: &[u8]) -> Result<(), PtySessionOpError> {
        let mut state = self.state.lock().unwrap();
        if !state.spawned.contains_key(id) {
            return Err(PtySessionOpError::unknown_session(id));
        }
        state.stdin_writes.push((id.clone(), data.to_vec()));
        Ok(())
    }

    fn resize(
        &self,
        id: &RemotePtySessionId,
        rows: u32,
        cols: u32,
    ) -> Result<(), PtySessionOpError> {
        let mut state = self.state.lock().unwrap();
        if !state.spawned.contains_key(id) {
            return Err(PtySessionOpError::unknown_session(id));
        }
        state.resizes.push((id.clone(), rows, cols));
        Ok(())
    }

    fn signal(
        &self,
        id: &RemotePtySessionId,
        signal: RemoteSessionSignal,
    ) -> Result<(), PtySessionOpError> {
        let mut state = self.state.lock().unwrap();
        if !state.spawned.contains_key(id) {
            return Err(PtySessionOpError::unknown_session(id));
        }
        state.signals.push((id.clone(), signal));
        Ok(())
    }

    fn kill(&self, id: &RemotePtySessionId) {
        self.state.lock().unwrap().killed.push(id.clone());
    }
}

// Unix-gated, not merely `cfg(test)`: these tests build `ExitStatus` values with
// `std::os::unix::process::ExitStatusExt::from_raw`, which is the only way to
// exercise the signalled branch without spawning a process, and that trait does
// not exist off Unix. Without the gate the `check-windows` CI job fails, since it
// type-checks test code (`cargo check -p warp --features gui --lib --tests`).
//
// `session_exit_status_from_process` itself is cross-platform -- it reads only
// `ExitStatus::code()` -- so what is gated here is the test's construction
// technique, not the behaviour under test.
#[cfg(all(test, unix))]
#[path = "pty_session_ops_tests.rs"]
mod tests;
