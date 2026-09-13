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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionExitStatus {
    pub code: i32,
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
    /// Bytes evicted from the buffer, because its bound was exceeded,
    /// since the last drain (or since the session was registered, if it
    /// was never drained before).
    pub dropped_bytes: u64,
}

/// A session referenced by an id the store has no session registered for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("no session is registered under this id")]
pub struct UnknownSession;

/// A summary of one session's state, as reported to a `ListSessions`
/// caller: id, whether it is running or exited, and its buffer's current
/// occupancy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: RemotePtySessionId,
    pub state: SessionState,
    pub buffered_bytes: usize,
    pub dropped_bytes: u64,
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

    /// Removes and returns all retained bytes, along with the number of
    /// bytes dropped since the last drain.
    ///
    /// Draining clears both the retained bytes and the dropped count: the
    /// returned [`DrainedOutput`] is a complete report of everything
    /// buffered since the last drain, and the next detached period starts
    /// its own count from zero rather than inheriting this one.
    fn drain(&mut self) -> DrainedOutput {
        let bytes = self.bytes.drain(..).collect();
        let dropped_bytes = self.dropped_bytes;
        self.dropped_bytes = 0;
        DrainedOutput {
            bytes,
            dropped_bytes,
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
    output: OutputRingBuffer,
    exit_status: Option<SessionExitStatus>,
}

impl Session {
    fn new(output_bound_bytes: usize) -> Self {
        Self {
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
    /// existing session -- its buffered output, drop count and exit
    /// status -- is left untouched. Client-minted ids exist so a retried
    /// spawn after a lost response does not create a second session
    /// (`pty_session_id.rs`); this is that property applied to
    /// registration.
    pub fn register(&mut self, id: RemotePtySessionId) {
        self.sessions
            .entry(id)
            .or_insert_with(|| Session::new(self.output_bound_bytes));
    }

    /// Removes a session entirely. Returns whether one was present.
    pub fn remove(&mut self, id: &RemotePtySessionId) -> bool {
        self.sessions.remove(id).is_some()
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
        let session = self.sessions.get_mut(id).ok_or(UnknownSession)?;
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
        let session = self.sessions.get_mut(id).ok_or(UnknownSession)?;
        session.exit_status = Some(status);
        Ok(())
    }

    /// Removes and returns a session's buffered output, for reattach.
    ///
    /// Draining clears both the retained bytes and the dropped-byte count:
    /// the returned [`DrainedOutput`] is a complete report of everything
    /// buffered since the last drain, and the next detached period starts
    /// its own count from zero rather than inheriting this one.
    pub fn drain_output(
        &mut self,
        id: &RemotePtySessionId,
    ) -> Result<DrainedOutput, UnknownSession> {
        let session = self.sessions.get_mut(id).ok_or(UnknownSession)?;
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
                state: match session.exit_status {
                    Some(status) => SessionState::Exited(status),
                    None => SessionState::Running,
                },
                buffered_bytes: session.output.len(),
                dropped_bytes: session.output.dropped_bytes,
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
