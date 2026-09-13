//! The pty seam the daemon's five session RPCs (`SpawnSession`,
//! `WriteSessionStdin`, `ResizeSession`, `SignalSession`, `ListSessions`,
//! `crates/remote_server/proto/remote_server.proto`) dispatch through in
//! `server_model.rs`.
//!
//! Every pty operation is behind [`PtySessionOperations`] so the dispatch,
//! the `remote_server::session_store::SessionStore` integration, the
//! response shapes and the error paths in `server_model.rs` can all be
//! proven by unit tests that spawn no process -- see
//! [`FakePtySessionOperations`]. A test that needs a real pty is a test
//! that will not run (`docs/design/moth-parliament.md`, "Working practice on
//! this branch"), so the trait exists precisely to keep the dispatch logic
//! testable without one.
//!
//! Deliberately ctx-free: no method here takes a `ModelContext` /
//! `AppContext`. `server_model_tests.rs` has an established convention of
//! never constructing one for a plain unit test (see its comment on
//! `subscribe_git_status_records_subscriber_and_current_repo`, which tests a
//! ctx-free helper for exactly this reason) -- keeping this seam ctx-free
//! keeps the session RPCs testable the same way. See
//! [`LocalTtyPtySessionOperations`]'s doc comment for what that costs the
//! real backend.

use std::collections::HashMap;
use std::sync::Mutex;

use remote_server::RemotePtySessionId;

use super::proto::RemoteSessionSignal;

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
    fn spawn(&self, id: &RemotePtySessionId, spec: &PtySpawnSpec) -> Result<(), PtySessionOpError>;

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

/// Real backend for [`PtySessionOperations`], backed by
/// `app::terminal::local_tty`.
///
/// **Not wired to a live OS pty.** `local_tty::Pty::new(options, _, ctx: &mut
/// AppContext)` is the only constructor (`app/src/terminal/local_tty/unix.rs`),
/// and it looks up the `PtySpawner` singleton through `ctx`. Two things stand
/// between this backend and that call, and both are decisions about daemon
/// startup and dispatch-context threading, not pty mechanism -- fixing either
/// alone would be papering over the other:
///
/// 1. `PtySpawner` is never registered as a singleton for the daemon binary.
///    `app/src/remote_server/mod.rs::run_daemon_app` registers
///    `CodebaseIndexManager`, `GlobalBufferModel`, `WarpManagedPathsWatcher`
///    and others, each with a comment explaining why it must happen before
///    `ServerModel` -- `PtySpawner` needs the same treatment, and
///    `PtySpawner::new`'s own doc comment ("should be called extremely early
///    in the application startup process ... to minimize the number of
///    already-obtained resources that could leak into forked subprocesses")
///    means where in that order it goes is itself a decision, not a
///    mechanical addition.
/// 2. Even with that registered, `Pty::new` needs `ctx`, and this trait is
///    deliberately ctx-free (see the module doc comment) so the session RPCs
///    stay testable the way `server_model_tests.rs` already tests everything
///    else. Threading `ctx` through `spawn`/`write_stdin`/`resize`/`signal`
///    would mean either breaking that convention or duplicating every
///    dispatch method into a ctx-free bookkeeping half and a ctx-taking
///    production wrapper.
///
/// `docs/design/moth-parliament.md`'s dependency-ordered work list names this
/// exact gap as its own, later item -- "3. Remote-side ownership: the daemon
/// holds the pty" -- distinct from "2. Session operations" (the dispatch this
/// file implements). Closing it is that item, not a bookkeeping fix here.
///
/// Until then, every operation below behaves *correctly* for what this
/// backend actually has: no session was ever spawned, so every id genuinely
/// is unknown to it.
#[cfg(feature = "local_tty")]
pub struct LocalTtyPtySessionOperations;

#[cfg(feature = "local_tty")]
impl LocalTtyPtySessionOperations {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "local_tty")]
impl Default for LocalTtyPtySessionOperations {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "local_tty")]
impl PtySessionOperations for LocalTtyPtySessionOperations {
    fn spawn(
        &self,
        id: &RemotePtySessionId,
        _spec: &PtySpawnSpec,
    ) -> Result<(), PtySessionOpError> {
        Err(PtySessionOpError(format!(
            "remote pty spawning is not wired up yet for session {id} -- see the doc comment on \
             LocalTtyPtySessionOperations"
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
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(feature = "local_tty"))]
impl Default for LocalTtyPtySessionOperations {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(feature = "local_tty"))]
impl PtySessionOperations for LocalTtyPtySessionOperations {
    fn spawn(
        &self,
        _id: &RemotePtySessionId,
        _spec: &PtySpawnSpec,
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
}

impl FakePtySessionOperations {
    pub fn new() -> Self {
        Self::default()
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
    fn spawn(&self, id: &RemotePtySessionId, spec: &PtySpawnSpec) -> Result<(), PtySessionOpError> {
        let mut state = self.state.lock().unwrap();
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

#[cfg(test)]
#[path = "pty_session_ops_tests.rs"]
mod tests;
