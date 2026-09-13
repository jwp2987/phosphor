//! In-memory bookkeeping for pty sessions a daemon owns across client
//! disconnections.
//!
//! This is pure bookkeeping only: no pty is spawned, no process runs, and
//! nothing here touches the wire protocol or `server_model.rs`. It
//! implements the session identity, output-buffer and exit-status
//! decisions from `docs/design/moth-parliament.md`, "Scoping session
//! ownership" and "Output buffering while detached" (both 2026-09-12).
//! Streaming push variants, session lifecycle requests (spawn, write
//! stdin, resize, signal) and reattach-over-the-wire are later increments
//! described in the same document.

use std::collections::{HashMap, VecDeque};

use thiserror::Error;

use crate::pty_session_id::RemotePtySessionId;

/// Default bound, in bytes, for a session's output ring buffer.
///
/// Bytes, not lines: pty output is a byte stream, and a session emitting a
/// progress bar with no newline would otherwise register as one unbounded
/// line. 256 KiB holds the tail of a build comfortably, and twenty
/// detached sessions cost 5 MiB total -- see "Output buffering while
/// detached", `docs/design/moth-parliament.md`.
pub const DEFAULT_OUTPUT_BUFFER_BYTES: usize = 256 * 1024;

/// The terminal outcome of a pty session.
///
/// Recorded once, outside the output ring buffer, and never evicted: if
/// exit lived in the byte stream, a chatty process that exited and then
/// flooded its buffer past the bound could evict its own exit status, and
/// the session would reattach as "still running" forever.
///
/// Shaped to match this crate's existing `RemoteServerExitStatus`
/// (`manager.rs`) and the wire's `SessionExitedPush.exit_code`, which is an
/// `optional int32` for the same reason: a process killed by a signal has no
/// exit code, and `WIFEXITED`/`WIFSIGNALED` are distinct outcomes. A bare
/// `i32` would force a caller to invent a sentinel and report a fabricated
/// code for every signalled session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionExitStatus {
    /// Process exit code, if the process exited normally. `None` when it
    /// was killed by a signal, which has no exit code of its own.
    pub code: Option<i32>,
    /// True if the process was killed by a signal (Unix only).
    pub signal_killed: bool,
}

impl SessionExitStatus {
    /// A session whose process exited normally with `code`.
    pub fn exited(code: i32) -> Self {
        Self {
            code: Some(code),
            signal_killed: false,
        }
    }

    /// A session whose process was killed by a signal, and so has no exit
    /// code. `SignalSession` is one of the session RPCs this store serves,
    /// so this is a state the store must be able to represent rather than
    /// flatten into a sentinel code.
    pub fn signalled() -> Self {
        Self {
            code: None,
            signal_killed: true,
        }
    }
}

/// A session's lifecycle state, as reported to a `ListSessions` caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Exited(SessionExitStatus),
}

/// Bytes retrieved from a session's output buffer, and how many earlier
/// bytes were dropped before they could be retrieved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrainedOutput {
    /// The retained bytes, oldest first.
    pub bytes: Vec<u8>,
    /// Total bytes evicted from this session's buffer, because its bound
    /// was exceeded, since the session was registered.
    ///
    /// Cumulative and never reset -- see [`SessionStore::drain_output`] for
    /// why draining does not clear it. A caller that wants "dropped during
    /// this detached period" keeps its own watermark and subtracts.
    pub dropped_bytes_total: u64,
}

/// A session referenced by an id the store has no session registered for.
///
/// Carries the id: the immediate caller already knows it, but this error is
/// meant to be logged and propagated away from the call site, where "no session
/// is registered" on its own says nothing about which one.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("no session is registered under id {0}")]
pub struct UnknownSession(pub RemotePtySessionId);

/// What a session was spawned with, retained so `ListSessions` can answer
/// for a session whose client has since disconnected.
///
/// These are the fields the wire's `RemoteSessionSummary` requires (`cwd`,
/// `shell`, `rows`, `cols`); without retaining them the daemon could enumerate
/// only ids, and a reattaching client could not tell two sessions apart. They
/// come from `SpawnSession`, which carries exactly these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSpawnMetadata {
    /// Absolute working directory. Empty means the daemon's default, matching
    /// `SpawnSession.cwd`.
    pub cwd: String,
    /// Shell/command to run. `None` means the daemon's default login shell.
    pub shell: Option<String>,
    /// Terminal size, kept current by [`SessionStore::record_resize`] rather
    /// than frozen at spawn -- a reattaching client needs the size the pty has
    /// now, not the size it started with.
    pub rows: u32,
    pub cols: u32,
}

/// A summary of one session's state, as reported to a `ListSessions`
/// caller: id, what it was spawned with, whether it is running or exited,
/// and its buffer's current occupancy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: RemotePtySessionId,
    pub metadata: SessionSpawnMetadata,
    pub state: SessionState,
    pub buffered_bytes: usize,
    pub dropped_bytes_total: u64,
}

/// A bounded, byte-oriented ring buffer for one session's pty output.
///
/// Appends beyond `bound_bytes` drop from the oldest end and are counted
/// in `dropped_bytes`, never silently discarded.
#[derive(Debug)]
struct OutputRingBuffer {
    bound_bytes: usize,
    bytes: VecDeque<u8>,
    dropped_bytes: u64,
}

impl OutputRingBuffer {
    fn new(bound_bytes: usize) -> Self {
        Self {
            bound_bytes,
            bytes: VecDeque::new(),
            dropped_bytes: 0,
        }
    }

    fn append(&mut self, data: &[u8]) {
        if data.len() >= self.bound_bytes {
            // `data` alone fills or exceeds the bound: everything
            // currently buffered is superseded, and only the newest
            // `bound_bytes` of `data` itself survive. Handled as its own
            // branch because the general drop-oldest path below assumes
            // the incoming slice is appended whole after trimming the
            // existing buffer, which does not hold when the slice itself
            // is bigger than the bound.
            self.dropped_bytes += self.bytes.len() as u64;
            self.bytes.clear();
            let keep_from = data.len() - self.bound_bytes;
            self.dropped_bytes += keep_from as u64;
            self.bytes.extend(&data[keep_from..]);
            return;
        }
        let overflow = (self.bytes.len() + data.len()).saturating_sub(self.bound_bytes);
        if overflow > 0 {
            self.dropped_bytes += overflow as u64;
            self.bytes.drain(..overflow);
        }
        self.bytes.extend(data);
    }

    /// Removes and returns all retained bytes, along with the cumulative
    /// count of bytes this buffer has ever dropped.
    ///
    /// Clears the retained bytes but NOT the dropped count. Draining is
    /// "remove for use", which is not the same as "delivered": a caller
    /// that drains, builds a reattach payload, and then fails to deliver it
    /// would otherwise destroy the only record that a gap existed, and no
    /// later call could recover it. A monotonic count cannot be lost that
    /// way, and a caller needing a per-period delta can subtract its own
    /// watermark.
    fn drain(&mut self) -> DrainedOutput {
        DrainedOutput {
            bytes: self.bytes.drain(..).collect(),
            dropped_bytes_total: self.dropped_bytes,
        }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }
}

/// One session's bookkeeping: its output buffer and, separately, whether
/// and how it exited.
#[derive(Debug)]
struct Session {
    metadata: SessionSpawnMetadata,
    output: OutputRingBuffer,
    exit_status: Option<SessionExitStatus>,
}

impl Session {
    fn new(metadata: SessionSpawnMetadata, output_bound_bytes: usize) -> Self {
        Self {
            metadata,
            output: OutputRingBuffer::new(output_bound_bytes),
            exit_status: None,
        }
    }
}

/// In-memory registry of pty sessions a daemon owns across client
/// disconnections, keyed by client-minted [`RemotePtySessionId`].
#[derive(Debug)]
pub struct SessionStore {
    sessions: HashMap<RemotePtySessionId, Session>,
    output_bound_bytes: usize,
}

impl SessionStore {
    /// Creates a store whose sessions bound their output to
    /// [`DEFAULT_OUTPUT_BUFFER_BYTES`].
    pub fn new() -> Self {
        Self::with_output_bound(DEFAULT_OUTPUT_BUFFER_BYTES)
    }

    /// Creates a store whose sessions each bound their output to
    /// `output_bound_bytes`.
    ///
    /// The bound is per session, not shared across the store: one
    /// runaway session must not evict a quiet neighbour's buffered
    /// output.
    pub fn with_output_bound(output_bound_bytes: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            output_bound_bytes,
        }
    }

    /// Registers `id` as an active session.
    ///
    /// Idempotent: if `id` is already registered, this is a no-op and the
    /// existing session -- its metadata, buffered output, drop count and
    /// exit status -- is left untouched, and `metadata` is discarded. Client-minted ids exist so a retried
    /// spawn after a lost response does not create a second session
    /// (`pty_session_id.rs`); this is that property applied to
    /// registration.
    pub fn register(&mut self, id: RemotePtySessionId, metadata: SessionSpawnMetadata) {
        let output_bound_bytes = self.output_bound_bytes;
        self.sessions
            .entry(id)
            .or_insert_with(|| Session::new(metadata, output_bound_bytes));
    }

    /// Records a session's new terminal size, so `ListSessions` reports the
    /// size the pty has now rather than the one it was spawned with.
    pub fn record_resize(
        &mut self,
        id: &RemotePtySessionId,
        rows: u32,
        cols: u32,
    ) -> Result<(), UnknownSession> {
        let session = self.session_mut(id)?;
        session.metadata.rows = rows;
        session.metadata.cols = cols;
        Ok(())
    }

    /// Removes a session entirely. Returns whether one was present.
    pub fn remove(&mut self, id: &RemotePtySessionId) -> bool {
        self.sessions.remove(id).is_some()
    }

    fn session_mut(&mut self, id: &RemotePtySessionId) -> Result<&mut Session, UnknownSession> {
        self.sessions
            .get_mut(id)
            .ok_or_else(|| UnknownSession(id.clone()))
    }

    /// Appends output bytes to a session's ring buffer.
    ///
    /// Accepted regardless of whether the session has already exited: a
    /// process's final output can still arrive after its exit is
    /// recorded, and the buffer's job is to hold output, not to police
    /// lifecycle ordering.
    pub fn append_output(
        &mut self,
        id: &RemotePtySessionId,
        data: &[u8],
    ) -> Result<(), UnknownSession> {
        let session = self.session_mut(id)?;
        session.output.append(data);
        Ok(())
    }

    /// Records that a session has exited, storing the status outside the
    /// ring buffer so no subsequent amount of buffered output can evict
    /// it.
    pub fn record_exit(
        &mut self,
        id: &RemotePtySessionId,
        status: SessionExitStatus,
    ) -> Result<(), UnknownSession> {
        let session = self.session_mut(id)?;
        session.exit_status = Some(status);
        Ok(())
    }

    /// Removes and returns a session's buffered output, for reattach.
    ///
    /// Clears the retained bytes but NOT the dropped-byte count, which is
    /// cumulative for the life of the session. Draining is "remove for use",
    /// not "delivered": zeroing the count here would mean a caller that
    /// drains and then fails to deliver the payload destroys the only record
    /// that output was ever dropped, leaving a gap nothing can report.
    pub fn drain_output(
        &mut self,
        id: &RemotePtySessionId,
    ) -> Result<DrainedOutput, UnknownSession> {
        let session = self.session_mut(id)?;
        Ok(session.output.drain())
    }

    /// Lists every registered session for `ListSessions`: id, whether it
    /// is running or exited, and its buffer's current occupancy.
    ///
    /// Read-only: unlike [`Self::drain_output`], this clears nothing.
    /// Ordered by id, so callers get a stable listing.
    pub fn list(&self) -> Vec<SessionSummary> {
        let mut summaries: Vec<SessionSummary> = self
            .sessions
            .iter()
            .map(|(id, session)| SessionSummary {
                id: id.clone(),
                metadata: session.metadata.clone(),
                state: match session.exit_status {
                    Some(status) => SessionState::Exited(status),
                    None => SessionState::Running,
                },
                buffered_bytes: session.output.len(),
                dropped_bytes_total: session.output.dropped_bytes,
            })
            .collect();
        summaries.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        summaries
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "session_store_tests.rs"]
mod tests;
