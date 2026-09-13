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

/// A session's currently retained output bytes, together with both
/// dropped-byte figures. Produced by [`SessionStore::peek_output`], which
/// does not discard anything -- see that method for the two-phase
/// read/acknowledge contract this store uses.
///
/// The two dropped-byte fields are named so a use site cannot confuse a
/// lifetime figure for a per-gap one: `dropped_bytes_total` is the wrong
/// number to show next to a specific reattach, and `dropped_bytes_since_ack`
/// is the wrong number to log as a session-lifetime statistic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeekedOutput {
    /// Stream offset of `bytes[0]` -- the count of bytes appended to this
    /// session before it.
    pub first_offset: u64,
    /// Stream offset just past the last byte in `bytes`. Pass this to
    /// [`SessionStore::acknowledge_output`] once these bytes are delivered.
    ///
    /// Acknowledgement is by offset rather than by a count of delivered bytes
    /// because a count is not stable across eviction: if an `append_output`
    /// between the peek and the acknowledgement overflows the bound, bytes
    /// leave the oldest end, and "discard the first N" would then discard N
    /// bytes starting from a different byte than the one that was peeked --
    /// silently destroying output that was never delivered. An offset names a
    /// byte in the stream rather than a position in the buffer, so a stale
    /// acknowledgement discards less than it asked for instead of the wrong
    /// bytes.
    pub next_offset: u64,
    /// The retained bytes, oldest first.
    pub bytes: Vec<u8>,
    /// Total bytes evicted from this session's buffer, because its bound
    /// was exceeded, since the session was registered. Cumulative and never
    /// reset by anything, including [`SessionStore::acknowledge_output`].
    pub dropped_bytes_total: u64,
    /// Bytes evicted from this session's buffer since the last
    /// [`SessionStore::acknowledge_output`] call (or since registration, if
    /// there has been none) -- the "N KiB dropped while detached" figure a
    /// reattach wants to show. Derived as `dropped_bytes_total` minus a
    /// watermark advanced by acknowledgement, not tracked as an
    /// independent counter, so the two figures cannot drift apart.
    pub dropped_bytes_since_ack: u64,
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
    /// Total bytes ever appended, counting those since evicted or
    /// acknowledged. Combined with `bytes.len()` this yields the stream
    /// offset of the oldest retained byte, which is what makes
    /// acknowledgement safe against eviction -- see
    /// [`Self::first_retained_offset`].
    total_appended: u64,
    dropped_bytes: u64,
    /// The value of `dropped_bytes` as of the last [`Self::acknowledge`]
    /// call (or `0` if there has been none). `dropped_bytes -
    /// acknowledged_dropped_bytes` is the "since acknowledged" figure --
    /// see [`PeekedOutput::dropped_bytes_since_ack`].
    acknowledged_dropped_bytes: u64,
}

impl OutputRingBuffer {
    fn new(bound_bytes: usize) -> Self {
        Self {
            bound_bytes,
            bytes: VecDeque::new(),
            total_appended: 0,
            dropped_bytes: 0,
            acknowledged_dropped_bytes: 0,
        }
    }

    fn append(&mut self, data: &[u8]) {
        self.total_appended += data.len() as u64;
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

    /// Returns a copy of all retained bytes, along with both dropped-byte
    /// figures. Mutates nothing: this is the non-destructive half of the
    /// read/acknowledge pair, kept non-destructive for exactly the reason
    /// [`Self::acknowledge`]'s doc comment explains.
    /// Stream offset of the oldest retained byte.
    ///
    /// Derived from `total_appended - bytes.len()` rather than accumulated as
    /// bytes leave the front, so it is automatically correct however they
    /// left -- evicted by the bound, in either append branch, or discarded by
    /// an acknowledgement. There is no per-branch bookkeeping to get wrong.
    fn first_retained_offset(&self) -> u64 {
        self.total_appended - self.bytes.len() as u64
    }

    fn peek(&self) -> PeekedOutput {
        PeekedOutput {
            first_offset: self.first_retained_offset(),
            next_offset: self.total_appended,
            bytes: self.bytes.iter().copied().collect(),
            dropped_bytes_total: self.dropped_bytes,
            dropped_bytes_since_ack: self.dropped_bytes - self.acknowledged_dropped_bytes,
        }
    }

    /// Discards every retained byte before stream offset `through_offset`
    /// and advances the acknowledged-dropped watermark to the current
    /// dropped-byte total, so a subsequent [`Self::peek`] reports
    /// `dropped_bytes_since_ack` as `0` until more is dropped.
    ///
    /// This is the only place that discards buffered output. Reading
    /// (`peek`) never does, precisely so that a caller who peeks, builds a
    /// reattach payload, and then fails to deliver it (a send error, a
    /// crash before flush) has lost nothing -- the next peek sees the same
    /// bytes. Only a confirmed delivery should call this.
    ///
    /// Addressed by stream offset, not by a count of delivered bytes,
    /// because `peek` and `acknowledge` are two separate calls and an
    /// `append` that overflows the bound in between evicts from the oldest
    /// end. "Discard the first N" would then discard N bytes starting from a
    /// different byte than the one that was peeked, silently destroying
    /// output that was never delivered -- the precise failure this two-phase
    /// design exists to prevent. An offset names a byte in the stream, so
    /// the arithmetic below self-corrects: a `through_offset` already passed
    /// by eviction saturates to zero and discards nothing, and one beyond
    /// what is buffered clamps to the buffer length. Neither can discard a
    /// byte the caller did not name, and neither panics.
    ///
    /// The watermark advances even when nothing is discarded and even
    /// if the buffer is already empty. Both are real, non-error calls: a
    /// reattach whose entire gap was retained-bytes-free (everything since
    /// the last acknowledgement was evicted, not retained) still delivers
    /// a gap marker with zero bytes behind it, and that delivery must still
    /// retire the gap it reported -- otherwise the same dropped span would
    /// be reported again on the next reattach even though the client has
    /// already been told about it.
    fn acknowledge(&mut self, through_offset: u64) {
        let discard = through_offset
            .saturating_sub(self.first_retained_offset())
            .min(self.bytes.len() as u64) as usize;
        self.bytes.drain(..discard);
        self.acknowledged_dropped_bytes = self.dropped_bytes;
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
    /// exit status -- is left untouched, and `metadata` is discarded.
    /// Client-minted ids exist so a retried spawn after a lost response
    /// does not create a second session (`pty_session_id.rs`); this is
    /// that property applied to registration.
    pub fn register(&mut self, id: RemotePtySessionId, metadata: SessionSpawnMetadata) {
        self.sessions
            .entry(id)
            .or_insert_with(|| Session::new(metadata, self.output_bound_bytes));
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

    fn session(&self, id: &RemotePtySessionId) -> Result<&Session, UnknownSession> {
        self.sessions
            .get(id)
            .ok_or_else(|| UnknownSession(id.clone()))
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

    /// Returns a session's currently retained output, for building a
    /// reattach payload, without discarding anything.
    ///
    /// This is the read half of a two-phase read/acknowledge pair --
    /// see "Output buffering while detached, two-phase delivery" in
    /// `docs/design/moth-parliament.md`. Calling this twice with no
    /// intervening [`Self::acknowledge_output`] returns the same bytes
    /// both times (plus whatever new output has since arrived): nothing
    /// is consumed until the caller confirms delivery by acknowledging
    /// it. A caller that peeks, builds a reattach payload, and then fails
    /// to deliver it (a send error, a crash before flush) has lost
    /// nothing -- the bytes are still here on the next peek.
    pub fn peek_output(&self, id: &RemotePtySessionId) -> Result<PeekedOutput, UnknownSession> {
        let session = self.session(id)?;
        Ok(session.output.peek())
    }

    /// Confirms that a session's peeked output was delivered up to stream
    /// offset `through_offset` -- pass [`PeekedOutput::next_offset`] from the
    /// peek whose bytes were delivered. Discards the acknowledged bytes and
    /// advances the "since acknowledged" dropped-byte watermark so it
    /// reflects only what drops after this call.
    ///
    /// This is the only method that discards buffered output -- see
    /// [`OutputRingBuffer::acknowledge`] for the full contract, including why
    /// it is addressed by offset rather than by a count of delivered bytes,
    /// and why the watermark advances even when nothing is discarded.
    pub fn acknowledge_output(
        &mut self,
        id: &RemotePtySessionId,
        through_offset: u64,
    ) -> Result<(), UnknownSession> {
        let session = self.session_mut(id)?;
        session.output.acknowledge(through_offset);
        Ok(())
    }

    /// Lists every registered session for `ListSessions`: id, whether it
    /// is running or exited, and its buffer's current occupancy.
    ///
    /// Read-only: like [`Self::peek_output`] and unlike
    /// [`Self::acknowledge_output`], this clears nothing. Ordered by id, so
    /// callers get a stable listing.
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
