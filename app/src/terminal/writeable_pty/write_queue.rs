//! The typed jobs of the pty writer's queue (#1384).
//!
//! Everything bound for the pty is a job in ONE strict FIFO owned by the thread that owns the fd
//! (`local_tty::event_loop::State`). A job has an id (in every log line), a class (what it is, and
//! so what may drop it), and an abort scope. The types here are pure data: no fd, no clock except
//! the `Instant` a paced job compares against.
//!
//! The pty fd is non-blocking and the loop resumes on poll-writable, so a plain job needs no
//! pacing: the kernel's backpressure (`EAGAIN`) decides how fast bytes go. Only a
//! [`JobClass::SetupChunked`] job is paced, and only because the consumer behind the pty (a
//! user-space pty relay such as `docker exec -it`, `sudo -i`, `script`) gives no observable
//! signal of its own (#960). Its pacing is a deadline the writer's `poll` honours, so a busy UI
//! thread can no longer stretch it.

use std::{
    borrow::Cow,
    collections::VecDeque,
    time::{Duration, Instant},
};

use crate::terminal::model::escape_sequences::{BRACKETED_PASTE_END, BRACKETED_PASTE_START};

/// Monotonic per-pty job id.
pub type JobId = u64;

/// What a queued write is. Decides what may drop it and where it may interleave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobClass {
    /// Typed bytes. Never dropped.
    UserKey,
    /// The submitted buffer of a user command; a ^C drops it while unwritten (#1046).
    UserCommand,
    /// A paste, streamed a tty-buffer at a time; a ^C drops what is unwritten (#1255). Today a
    /// paste travels as `Message::CommandInput` and so is classed `UserCommand`; the two are
    /// handled identically.
    #[allow(dead_code)]
    Paste,
    /// A reply to a terminal query. Never dropped by an abort; may interleave at chunk boundaries
    /// of a [`JobClass::SetupChunked`] job.
    Reply,
    /// An agent's input. Not distinguishable on the wire yet; ordered like `UserKey`.
    #[allow(dead_code)]
    Agent,
    /// A bootstrap script written in paced chunks. One unit: nothing but a reply lands inside it.
    SetupChunked,
    /// A window size change. Applied immediately and coalesced (#1376), never queued as bytes.
    #[allow(dead_code)]
    Winsize,
}

impl JobClass {
    #[inline]
    pub fn is_reply(self) -> bool {
        self == JobClass::Reply
    }

    /// Whether `Message::DiscardQueuedInput` / a ^C drops this class while it is unwritten.
    #[inline]
    pub fn is_discardable(self) -> bool {
        matches!(self, JobClass::UserCommand | JobClass::Paste)
    }
}

/// Which jobs a [`Message::Abort`](super::Message::Abort) removes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbortScope {
    /// Submitted user commands and pastes (what `DiscardQueuedInput` always did).
    Discardable,
    /// Every `SetupChunked` job, queued or in flight: a newer bootstrap, the shell exiting, the
    /// pty shutting down.
    Setup,
}

impl AbortScope {
    pub fn covers(self, class: JobClass) -> bool {
        match self {
            AbortScope::Discardable => class.is_discardable(),
            AbortScope::Setup => class == JobClass::SetupChunked,
        }
    }
}

/// How a job's chunks are spaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    /// Back to back, as fast as the pty takes them.
    None,
    /// `dur` between the end of one chunk and the start of the next. `why` names the missing
    /// signal and is logged once per job.
    Gap { dur: Duration, why: &'static str },
}

/// A `SetupChunked` job: its chunks, and where the writer is in them.
pub struct SetupJob {
    pub id: JobId,
    chunks: VecDeque<Cow<'static, [u8]>>,
    pacing: Pacing,
    /// Whether the chunks written so far opened a bracketed paste that is not closed.
    in_paste: bool,
    /// When the next chunk may start (`None` before the first chunk and with `Pacing::None`).
    next_at: Option<Instant>,
    /// Cut by an abort: nothing more is started, and a bracketed paste left open is closed once
    /// the chunk in flight (if any) has been written.
    aborted: bool,
}

impl SetupJob {
    pub fn new(id: JobId, chunks: Vec<Vec<u8>>, pacing: Pacing) -> SetupJob {
        SetupJob {
            id,
            chunks: chunks
                .into_iter()
                .filter(|chunk| !chunk.is_empty())
                .map(Cow::Owned)
                .collect(),
            pacing,
            in_paste: false,
            next_at: None,
            aborted: false,
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn pacing(&self) -> Pacing {
        self.pacing
    }

    /// Whether the next chunk may start now.
    pub fn is_due(&self, now: Instant) -> bool {
        self.next_at.is_none_or(|at| now >= at)
    }

    /// How long until the next chunk may start (`None` when it may start now or nothing is left).
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        if self.chunks.is_empty() {
            return None;
        }
        self.next_at
            .and_then(|at| at.checked_duration_since(now))
            .filter(|d| !d.is_zero())
    }

    pub fn take_chunk(&mut self) -> Option<Cow<'static, [u8]>> {
        self.chunks.pop_front()
    }

    /// Records that `chunk` was written in full at `now`: tracks the bracketed paste state and
    /// starts the gap before the next chunk.
    pub fn chunk_written(&mut self, chunk: &[u8], now: Instant) {
        if let Some(open) = paste_state_after(chunk) {
            self.in_paste = open;
        }
        self.next_at = match self.pacing {
            Pacing::Gap { dur, .. } if !self.chunks.is_empty() => Some(now + dur),
            _ => None,
        };
    }

    /// Drops the chunks not yet started (an abort).
    pub fn cut(&mut self) {
        self.chunks.clear();
        self.aborted = true;
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted
    }

    /// Whether the chunks written so far left a bracketed paste open.
    pub fn is_paste_open(&self) -> bool {
        self.in_paste
    }
}

/// `Some(true)` when `bytes` leave a bracketed paste open, `Some(false)` when they close one, and
/// `None` when they hold no marker.
pub fn paste_state_after(bytes: &[u8]) -> Option<bool> {
    let last = |needle: &[u8]| bytes.windows(needle.len()).rposition(|w| w == needle);
    match (last(BRACKETED_PASTE_START), last(BRACKETED_PASTE_END)) {
        (Some(start), Some(end)) => Some(start > end),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

/// Whether `bytes` is exactly one signal-generating control byte: ^C, ^D, ^Z or ^\. A signal
/// preempts and aborts a setup job instead of queueing behind it.
pub fn is_signal_byte(bytes: &[u8]) -> bool {
    matches!(bytes, [0x03] | [0x04] | [0x1a] | [0x1c])
}
