// The code in this file is adapted from the alacritty_terminal crate under the
// Apache license; see: crates/warp_terminal/src/model/LICENSE-ALACRITTY.

//! The main event loop which performs I/O on the pseudoterminal.

use std::{
    borrow::Cow,
    collections::VecDeque,
    io::{self, ErrorKind, Read, Write},
    marker::Send,
    ops::DerefMut,
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use log::error;
use mio::{self, Events, Interest};
use parking_lot::{FairMutex, FairMutexGuard};

use crate::terminal::writeable_pty::write_queue::{
    AbortScope, JobClass, JobId, Pacing, SetupJob, is_signal_byte,
};
use crate::terminal::{
    TerminalModel,
    event_listener::ChannelEventListener,
    local_tty,
    model::{BlockId, ansi, reply_ledger::SharedReplyLedger},
};
use crate::terminal::{model::terminal_model::ExitReason, writeable_pty::Message};

use warp_terminal::model::escape_sequences::{BRACKETED_PASTE_END, BRACKETED_PASTE_START};

use super::mio_channel::Receiver;

/// Slow-operation thresholds for the pty loop (#1146). The parse holds the terminal model lock
/// the UI thread renders from, so it is held to the UI-thread budget.
const PTY_PARSE_SLOW: Duration = Duration::from_millis(16);
/// User input waiting in the writer (owner budget: Enter→shell ≤2 ms, so 16 ms is a real stall).
const PTY_WRITE_SLOW: Duration = Duration::from_millis(16);
/// Write -> first output read: includes the shell's and, over ssh, the network's round trip.
const PTY_ECHO_SLOW: Duration = Duration::from_millis(50);

/// How long after user input reaches the pty its output still counts as the keystroke's echo and
/// is redrawn without waiting for the wakeup throttle (#1138). Long enough to cover a LAN ssh
/// round trip and a TUI's multi-write response; a flood that starts within the window (Enter on
/// `cat bigfile`) is only capped by the throttle's floor until the window closes.
const USER_WRITE_ECHO_WINDOW: Duration = Duration::from_millis(300);
static SLOW_PTY_PARSE: warp_util::slow_log::SlowLog = warp_util::slow_log::SlowLog::new(
    "pty parse (terminal model lock held)",
    PTY_PARSE_SLOW,
    warp_util::slow_log::DEFAULT_WINDOW,
);
static SLOW_PTY_WRITE: warp_util::slow_log::SlowLog = warp_util::slow_log::SlowLog::new(
    "pty write (input received -> written)",
    PTY_WRITE_SLOW,
    warp_util::slow_log::DEFAULT_WINDOW,
);
static SLOW_PTY_ECHO: warp_util::slow_log::SlowLog = warp_util::slow_log::SlowLog::new(
    "pty echo (write -> first output read)",
    PTY_ECHO_SLOW,
    warp_util::slow_log::DEFAULT_WINDOW,
);

/// The size of the buffer to read data into from the PTY.
const READ_BUFFER_SIZE: usize = 0x4_0000;

/// Max bytes to process from the PTY while holding the lock before giving
/// someone else an opportunity to lock it.
const MAX_LOCKED_READ: usize = 0x1_0000;

/// Most bytes parsed in one hold of the terminal model lock. The reader parses what it has read
/// in slices of this size and lets a waiting thread (the UI) in between them, so one hold is
/// bounded by a slice rather than by everything buffered (#1135).
const PARSE_SLICE: usize = 0x4000;

/// Most bytes buffered while the terminal model is locked elsewhere (the UI is laying out or
/// painting) before the reader blocks for the lock instead of reading on. Without a cap the
/// reader buffered the whole read buffer and parsed it in one hold right after the UI let go,
/// just before the UI wanted the lock again (#1135).
const MAX_UNLOCKED_ACCUMULATION: usize = 0x8000;

/// A hold of the terminal model lock longer than this is logged at debug level.
const SLOW_PARSE_HOLD: Duration = Duration::from_millis(8);

pub const CHANNEL_TOKEN: mio::Token = mio::Token(0);
pub const PTY_TOKEN: mio::Token = mio::Token(1);
pub const SIGNALS_TOKEN: mio::Token = mio::Token(2);

/// The main event!.. loop.
/// Handles all the PTY I/O and runs the PTY parser which updates terminal
/// state.
pub struct EventLoop<T: local_tty::EventedPty> {
    poll: mio::Poll,
    pty: T,
    rx: Receiver<Message>,
    terminal: Arc<FairMutex<TerminalModel>>,
    /// Query replies and every pty write, so a reply that leaks into the shell's line can be
    /// told from typed text (#921).
    reply_ledger: SharedReplyLedger,

    /// The event listener is available to the PTY event loop
    /// to emit relevant events to subscribers. The ansi handler
    /// also has a handle to the event listener, so events may also
    /// be emitted at a later stage (i.e. when we have a better idea
    /// of what the bytes from the PTY actually meant).
    event_listener: ChannelEventListener,
}

/// Helper type which tracks how much of a buffer has been written.
struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
    /// What this write is; see `QueuedWrite::class`. A `SetupChunked` writing is one chunk of
    /// `State::active_job`.
    class: JobClass,
}

/// All of the mutable state needed to run the event loop.
/// Contains list of items to write, current write state, etc. Anything that
/// would otherwise be mutated on the `EventLoop` goes here.
pub struct State {
    /// One FIFO of everything bound for the pty, so user input can never overtake an
    /// earlier terminal-query reply.
    write_list: VecDeque<QueuedWrite>,
    writing: Option<Writing>,
    /// The `SetupChunked` job being written (#1384). Strict FIFO: it was taken off the front of
    /// `write_list`, and until it is finished nothing from the list is started except a reply at
    /// the head, and only between two of its chunks. A user key behind it waits for all of it.
    active_job: Option<SetupJob>,
    /// Whether the pty was canonical when the loop last looked (`None`: not looked at, or cannot
    /// be told). Read on demand, only while a setup job whose pacing depends on it is waiting to
    /// start; see [`Pacing::GapIfCanonical`].
    pty_canonical: Option<bool>,
    /// The id the next submitted job gets (monotonic, in every log line about it).
    next_job_id: JobId,
    parser: ansi::Processor,
    /// Whether replies generated while a command runs may be written now. They may not while the
    /// pty is in canonical mode: nobody can read a reply there (it has no line terminator), so
    /// it would sit in the tty until the shell's line editor takes it for typed text (#937).
    reply_gate_open: bool,
    /// How many entries of `write_list` are gated replies, so the questions asked on every
    /// loop iteration are O(1) even under a flood of unread queries.
    gated_count: usize,
    /// When replies started being withheld, so the re-check can back off for a reply that stays
    /// held.
    withheld_since: Option<Instant>,
    /// The pty reported a hang-up (`EIO` on the master) but the child's exit has not been seen
    /// yet. Nothing more can be read or written, so the loop must wait for the exit event
    /// instead of retrying the dead fd (#953).
    hung_up: bool,
    /// When the hang-up was seen, so the exit check backs off and the wait is logged (#1375).
    hung_up_since: Option<Instant>,
    /// Whether the "child still has not exited" warning was logged for this hang-up.
    hung_up_warned: bool,
    /// When the last user input (not a query reply) finished reaching the pty and no output has
    /// been read since. Times the keystroke round trip for the input-order log (#1092).
    user_write_finished_at: Option<Instant>,
    /// When the last user input finished reaching the pty, kept across reads: every read within
    /// [`USER_WRITE_ECHO_WINDOW`] of it is part of the keystroke's echo (#1138).
    last_user_write_at: Option<Instant>,
    /// When the oldest user input not yet fully written was received by this loop. Times the
    /// receive -> pty write leg for the slow-operation log (#1146).
    user_input_received_at: Option<Instant>,
    /// Whether consecutive plain-text key writes are merged into one write (see
    /// [`State::coalesce_keys_into`]). On for a pty whose writes complete asynchronously (the
    /// Windows ConPTY pipe), where each write must wait for the previous one's completion event.
    coalesce_keys: bool,
}

/// `PHOSPHOR_PTY_COALESCE=0` turns key-write coalescing off.
fn coalesce_keys_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| !std::env::var("PHOSPHOR_PTY_COALESCE").is_ok_and(|v| v.trim() == "0"))
}

/// Most bytes one coalesced key write carries.
const MAX_COALESCED_KEY_BYTES: usize = 4096;

/// Whether `bytes` is plain typed text that may share a write with other plain typed text:
/// nothing below 0x20 (no ESC, whose meaning depends on what follows it in the same read, no ^C
/// or other signal byte, no CR) and non-empty.
fn is_plain_key_text(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|&b| b >= 0x20)
}

/// How often a withheld reply re-checks whether the pty has left canonical mode: promptly for the
/// first second (a program that queries and then switches to raw mode does so within
/// milliseconds), then less often for a reply nobody is waiting for.
const REPLY_GATE_POLL: Duration = Duration::from_millis(10);
const REPLY_GATE_POLL_SLOW: Duration = Duration::from_millis(100);
/// How many consecutive non-EINTR `poll` failures the reader retries before ending the session.
const MAX_CONSECUTIVE_POLL_ERRORS: u32 = 3;

/// What the reader does about a failed `poll` (#1376): it used to panic the reader thread, which
/// took the tab's output down with it.
#[derive(Debug, PartialEq, Eq)]
enum PollErrorAction {
    /// Poll again.
    Retry,
    /// Give up on this session cleanly (it is torn down like a disconnect).
    EndSession,
}

/// Decides how to react to `err` given how many polls in a row have now failed (EINTR excluded).
fn poll_error_action(err: &io::Error, consecutive_failures: u32) -> PollErrorAction {
    if err.kind() == ErrorKind::Interrupted {
        return PollErrorAction::Retry;
    }
    if consecutive_failures > MAX_CONSECUTIVE_POLL_ERRORS {
        PollErrorAction::EndSession
    } else {
        PollErrorAction::Retry
    }
}

/// While the pty has hung up, how often to check whether the child has exited, in case its
/// `SIGCHLD` was consumed before the hang-up was seen.
///
/// The exit normally arrives as an event (`SIGCHLD` / the pty's exit notification); this check is
/// only a give-up safety net for the case where that signal was consumed before the hang-up was
/// seen, an OS-level race with no event of its own. It backs off to [`HUNG_UP_POLL_SLOW`] and
/// logs once at [`HUNG_UP_WARN_AFTER`], so a child that never exits is visible instead of being
/// polled forever in silence.
const HUNG_UP_POLL: Duration = Duration::from_millis(100);
const HUNG_UP_POLL_SLOW: Duration = Duration::from_secs(1);
const HUNG_UP_WARN_AFTER: Duration = Duration::from_secs(5);
const REPLY_GATE_SLOW_AFTER: Duration = Duration::from_secs(1);
/// Most gated replies kept. A program that queries in a loop without reading gets its oldest
/// replies dropped; nobody can read them from a cooked tty anyway.
const MAX_GATED_REPLIES: usize = 1024;

/// A queued pty write. `produced_while_executing` is `Some(block id)` for a reply to a
/// terminal query (DA, OSC colors, ...) that was generated while that block was
/// executing a command; such a reply still unwritten after the command finished is
/// dropped, as the shell would otherwise read it back as typed text (#829). User input
/// and replies produced at the prompt carry `None` and are never dropped.
struct QueuedWrite {
    id: JobId,
    bytes: Cow<'static, [u8]>,
    produced_while_executing: Option<BlockId>,
    /// What this is; decides what may drop it (a submitted user command is the only thing
    /// `DiscardQueuedInput` drops, #1046) and what may interleave with it.
    class: JobClass,
    /// The chunks of a `SetupChunked` job (`bytes` is empty then).
    setup: Option<SetupJob>,
}

impl QueuedWrite {
    /// Replies generated while a command runs wait for the pty to leave canonical mode.
    fn is_gated(&self) -> bool {
        self.class.is_reply() && self.produced_while_executing.is_some()
    }
}

impl Default for State {
    fn default() -> State {
        State {
            write_list: VecDeque::new(),
            parser: ansi::Processor::new(),
            writing: None,
            active_job: None,
            pty_canonical: None,
            next_job_id: 1,
            reply_gate_open: true,
            gated_count: 0,
            withheld_since: None,
            hung_up: false,
            hung_up_since: None,
            hung_up_warned: false,
            user_write_finished_at: None,
            last_user_write_at: None,
            user_input_received_at: None,
            coalesce_keys: false,
        }
    }
}

impl State {
    #[inline]
    fn ensure_next(&mut self) {
        if self.writing.is_none() {
            self.goto_next();
        }
    }

    #[inline]
    fn goto_next(&mut self) {
        self.goto_next_at(Instant::now());
    }

    /// Picks what is written next. While a `SetupChunked` job is active it is the only thing
    /// written, except a reply at the head of the list (a chunk boundary is the one place a reply
    /// may land without corrupting the job); its next chunk is picked once its gap has passed.
    fn goto_next_at(&mut self, now: Instant) {
        loop {
            match self
                .active_job
                .as_ref()
                .map(|job| (job.is_exhausted(), job.is_due(now)))
            {
                Some((true, _)) => {
                    self.active_job = None;
                    continue;
                }
                Some((false, due)) => {
                    self.writing = if let Some(reply) = self.take_leading_reply() {
                        Some(Writing::new(reply.bytes, JobClass::Reply))
                    } else if due {
                        self.active_job
                            .as_mut()
                            .and_then(SetupJob::take_chunk)
                            .map(|chunk| Writing::new(chunk, JobClass::SetupChunked))
                    } else {
                        None
                    };
                    return;
                }
                None => {}
            }
            // The first entry that may be written now: a withheld reply is skipped, not waited
            // for, so user input behind it is never delayed.
            let open = self.reply_gate_open;
            let next = if open || self.gated_count == 0 {
                self.write_list.pop_front()
            } else if self.write_list.len() == self.gated_count {
                None
            } else {
                self.write_list
                    .iter()
                    .position(|entry| !entry.is_gated())
                    .and_then(|index| self.write_list.remove(index))
            };
            if next.as_ref().is_some_and(QueuedWrite::is_gated) {
                self.gated_count -= 1;
            }
            match next {
                Some(QueuedWrite {
                    id,
                    setup: Some(job),
                    ..
                }) => {
                    let mut job = job;
                    let canonical = self.pty_canonical;
                    job.resolve_pacing(canonical);
                    match job.pacing() {
                        Pacing::Gap { dur, why } => log::info!(
                            "pty write job {id}: setup, paced {dur:?} between chunks ({why}; tty canonical: \
                             {canonical:?})"
                        ),
                        Pacing::None => log::info!(
                            "pty write job {id}: setup, unpaced (tty canonical: {canonical:?})"
                        ),
                        Pacing::GapIfCanonical { .. } => {
                            log::warn!("pty write job {id}: setup, pacing left unresolved")
                        }
                    }
                    self.active_job = Some(job);
                }
                Some(mut entry) => {
                    if self.coalesce_keys {
                        self.coalesce_keys_into(&mut entry);
                    }
                    self.writing = Some(Writing::new(entry.bytes, entry.class));
                    return;
                }
                None => {
                    self.writing = None;
                    return;
                }
            }
        }
    }

    /// Merges the typed-text jobs queued directly behind `entry` into it, so a burst of keys that
    /// piled up while the previous write was in flight goes out as ONE write instead of one write
    /// (and one wait for its completion event) per key. Order is exactly the queue order: only
    /// jobs adjacent to `entry` are taken, only of class [`JobClass::UserKey`] holding plain text
    /// (see [`is_plain_key_text`]), so no reply, command, paste, agent write, signal byte or
    /// escape sequence is ever moved, merged or split from what follows it.
    fn coalesce_keys_into(&mut self, entry: &mut QueuedWrite) {
        if entry.class != JobClass::UserKey
            || entry.setup.is_some()
            || entry.is_gated()
            || !is_plain_key_text(&entry.bytes)
        {
            return;
        }
        let mut merged = 0usize;
        while let Some(next) = self.write_list.front() {
            if next.class != JobClass::UserKey
                || next.setup.is_some()
                || next.is_gated()
                || !is_plain_key_text(&next.bytes)
                || entry.bytes.len() + next.bytes.len() > MAX_COALESCED_KEY_BYTES
            {
                break;
            }
            let Some(next) = self.write_list.pop_front() else {
                break;
            };
            entry.bytes.to_mut().extend_from_slice(&next.bytes);
            merged += 1;
        }
        if merged > 0 {
            warpui::input_order::log_hop("pty_write_coalesced", || {
                format!("jobs={} bytes={}", merged + 1, entry.bytes.len())
            });
        }
    }

    /// Whether the head of the list is a reply that may be written now.
    fn has_leading_reply(&self) -> bool {
        self.write_list.front().is_some_and(|entry| {
            entry.class.is_reply() && (self.reply_gate_open || !entry.is_gated())
        })
    }

    fn take_leading_reply(&mut self) -> Option<QueuedWrite> {
        if !self.has_leading_reply() {
            return None;
        }
        let entry = self.write_list.pop_front()?;
        if entry.is_gated() {
            self.gated_count -= 1;
        }
        Some(entry)
    }

    /// How long until the active job's next chunk is due, if the writer is waiting for that.
    fn job_timeout(&self) -> Option<Duration> {
        if self.writing.is_some() {
            return None;
        }
        self.active_job.as_ref()?.wait(Instant::now())
    }

    /// Called when a write finished: a chunk of the active job starts the gap before the next
    /// one, and the last chunk (or the last of an aborted job) ends the job.
    fn note_job_chunk_finished(&mut self, finished: &Writing, at: Instant) {
        if finished.class != JobClass::SetupChunked {
            return;
        }
        let Some(job) = self.active_job.as_mut() else {
            return;
        };
        job.chunk_written(&finished.source, at);
        if job.is_exhausted() {
            let close_paste = job.is_aborted() && job.is_paste_open();
            self.active_job = None;
            if close_paste {
                self.push_paste_close();
            }
        }
    }

    fn push_paste_close(&mut self) {
        let id = self.alloc_job_id();
        self.write_list.push_front(QueuedWrite {
            id,
            bytes: Cow::Borrowed(BRACKETED_PASTE_END),
            produced_while_executing: None,
            class: JobClass::UserKey,
            setup: None,
        });
    }

    fn alloc_job_id(&mut self) -> JobId {
        let id = self.next_job_id;
        self.next_job_id = self.next_job_id.wrapping_add(1);
        id
    }

    /// Whether any queued write is a reply that the gate may be withholding.
    #[inline]
    fn has_gated(&self) -> bool {
        self.gated_count > 0
    }

    /// Whether a setup job that has not started yet is paced by the tty mode, so the loop must
    /// read it before the job is picked.
    fn needs_canonical_probe(&self) -> bool {
        self.write_list.iter().any(|entry| {
            entry
                .setup
                .as_ref()
                .is_some_and(|job| matches!(job.pacing(), Pacing::GapIfCanonical { .. }))
        })
    }

    /// Whether some replies are being withheld right now.
    #[inline]
    fn is_withholding(&self) -> bool {
        !self.reply_gate_open && self.has_gated()
    }

    /// Opens or closes the gate for the pty's current mode, and notes when withholding began.
    fn set_reply_gate(&mut self, pty_is_canonical: Option<bool>) {
        self.reply_gate_open = pty_is_canonical != Some(true);
        if self.is_withholding() {
            self.withheld_since.get_or_insert_with(Instant::now);
        } else {
            self.withheld_since = None;
        }
    }

    /// How soon to re-check the pty's mode, while a reply is withheld.
    fn gate_timeout(&self) -> Option<Duration> {
        self.is_withholding().then(|| match self.withheld_since {
            Some(since) if since.elapsed() >= REPLY_GATE_SLOW_AFTER => REPLY_GATE_POLL_SLOW,
            _ => REPLY_GATE_POLL,
        })
    }

    /// How long the event loop may sleep, given the synchronized-output timeout and any withheld
    /// reply waiting for the pty to leave canonical mode.
    fn poll_timeout(&self, sync_timeout: Option<Duration>) -> Option<Duration> {
        let timeout = [sync_timeout, self.gate_timeout(), self.job_timeout()]
            .into_iter()
            .flatten()
            .min();
        if self.hung_up {
            let poll = self.hung_up_poll();
            Some(timeout.map_or(poll, |t| t.min(poll)))
        } else {
            timeout
        }
    }

    /// How often to look for the child's exit while the pty is hung up: promptly at first, then
    /// slowly once the exit is clearly not coming straight away.
    fn hung_up_poll(&self) -> Duration {
        match self.hung_up_since {
            Some(since) if since.elapsed() >= HUNG_UP_WARN_AFTER => HUNG_UP_POLL_SLOW,
            _ => HUNG_UP_POLL,
        }
    }

    /// Records the hang-up (once).
    fn note_hung_up(&mut self) {
        if !self.hung_up {
            log::info!("pty hung up (EIO on the master); waiting for the child's exit");
        }
        self.hung_up = true;
        self.hung_up_since.get_or_insert_with(Instant::now);
    }

    /// Logs once when the child has still not exited long after the hang-up.
    fn warn_if_hung_up_too_long(&mut self) {
        if !self.hung_up_warned
            && self
                .hung_up_since
                .is_some_and(|since| since.elapsed() >= HUNG_UP_WARN_AFTER)
        {
            self.hung_up_warned = true;
            log::warn!(
                "pty has been hung up for {:?} and the child has not exited; still checking every {:?}",
                HUNG_UP_WARN_AFTER,
                HUNG_UP_POLL_SLOW
            );
        }
    }

    /// Queues user input; never dropped.
    fn queue_input(&mut self, bytes: Cow<'static, [u8]>) {
        let id = self.alloc_job_id();
        self.write_list.push_back(QueuedWrite {
            id,
            bytes,
            produced_while_executing: None,
            class: JobClass::UserKey,
            setup: None,
        });
    }

    /// Queues a bootstrap script as one paced job (#1384). A job still queued or in flight is
    /// replaced: the newer bootstrap supersedes the older, as `cancel_paced_writes` did.
    fn queue_setup(&mut self, chunks: Vec<Vec<u8>>, pacing: Pacing) {
        self.abort(AbortScope::Setup);
        let id = self.alloc_job_id();
        let job = SetupJob::new(id, chunks, pacing);
        if job.is_exhausted() {
            return;
        }
        self.write_list.push_back(QueuedWrite {
            id,
            bytes: Cow::Borrowed(&[]),
            produced_while_executing: None,
            class: JobClass::SetupChunked,
            setup: Some(job),
        });
    }

    /// Queues the submitted buffer of a user command, which a ^C may cancel while unwritten.
    fn queue_command_input(&mut self, bytes: Cow<'static, [u8]>) {
        self.queue_classed(bytes, JobClass::UserCommand);
    }

    /// Queues a paste (#1384 P4): cancelled by a ^C like a command, and by `AbortScope::Paste`.
    fn queue_paste_input(&mut self, bytes: Cow<'static, [u8]>) {
        self.queue_classed(bytes, JobClass::Paste);
    }

    /// Queues an agent's input (#1384 P4): kept by a ^C, removed by `AbortScope::Agent`.
    fn queue_agent_input(&mut self, bytes: Cow<'static, [u8]>) {
        self.queue_classed(bytes, JobClass::Agent);
    }

    fn queue_classed(&mut self, bytes: Cow<'static, [u8]>, class: JobClass) {
        let id = self.alloc_job_id();
        self.write_list.push_back(QueuedWrite {
            id,
            bytes,
            produced_while_executing: None,
            class,
            setup: None,
        });
    }

    /// Removes the unwritten jobs of `class`, and cuts the write in progress if it is one. A
    /// write cut inside a bracketed paste is followed by the paste close, so the shell does not
    /// swallow what comes next as pasted text.
    fn abort_class(&mut self, class: JobClass) {
        self.write_list.retain(|entry| entry.class != class);
        if self.writing.as_ref().is_some_and(|w| w.class == class) {
            let cut_paste = self
                .writing
                .take()
                .is_some_and(|w| w.cut_inside_bracketed_paste());
            if cut_paste {
                self.push_paste_close();
            }
        }
    }

    /// Queues a reply to a terminal query. `executing_block` is the id of the active
    /// block if it was executing a command when the bytes were handed to the parser
    /// (read BEFORE parsing: block completion happens synchronously inside the parse).
    ///
    /// Returns the bytes of gated replies evicted to stay within `MAX_GATED_REPLIES`.
    fn queue_response(
        &mut self,
        bytes: Vec<u8>,
        executing_block: Option<BlockId>,
    ) -> Vec<Cow<'static, [u8]>> {
        let mut evicted = Vec::new();
        if bytes.is_empty() {
            return evicted;
        }
        let entry = QueuedWrite {
            id: self.alloc_job_id(),
            bytes: Cow::Owned(bytes),
            produced_while_executing: executing_block,
            class: JobClass::Reply,
            setup: None,
        };
        if entry.is_gated() {
            self.gated_count += 1;
            while self.gated_count > MAX_GATED_REPLIES {
                if let Some(index) = self.write_list.iter().position(QueuedWrite::is_gated)
                    && let Some(old) = self.write_list.remove(index)
                {
                    self.gated_count -= 1;
                    evicted.push(old.bytes);
                } else {
                    break;
                }
            }
        }
        self.write_list.push_back(entry);
        evicted
    }

    /// Drops queued replies produced while a command ran if that command has since
    /// finished, i.e. the active block changed or is no longer executing. Returns the dropped
    /// bytes, so the reply ledger can forget replies that never reached the pty.
    fn drop_stale_responses(
        &mut self,
        active_block: &BlockId,
        executing_now: bool,
    ) -> Vec<Cow<'static, [u8]>> {
        let mut dropped = Vec::new();
        self.write_list = std::mem::take(&mut self.write_list)
            .into_iter()
            .filter_map(|entry| match &entry.produced_while_executing {
                Some(id) if !(executing_now && id == active_block) => {
                    dropped.push(entry.bytes);
                    None
                }
                _ => Some(entry),
            })
            .collect();
        // Everything dropped was a tagged reply, i.e. gated.
        self.gated_count -= dropped.len();
        dropped
    }

    #[inline]
    fn has_tagged(&self) -> bool {
        // Only replies carry a tag, and those are the gated ones.
        self.gated_count > 0
    }

    /// Drops every queued and partially written byte, for a pty that can no longer be written.
    fn discard_pending_writes(&mut self) {
        self.write_list.clear();
        self.writing = None;
        self.active_job = None;
        self.gated_count = 0;
        self.withheld_since = None;
    }

    /// Drops the submitted user commands that have not reached the pty yet: queued writes tagged
    /// `discardable`, and what is left of the write in progress if it is one (#1046). A big
    /// multi-line paste is written a tty-buffer at a time, so a ^C queued behind it would
    /// otherwise wait for all of it. Everything else (bootstrap scripts, typeahead, replies,
    /// agent writes) stays queued in order. Edge: a command submitted after the interrupted work
    /// and not yet started is dropped too. If the write in progress was cut inside a bracketed
    /// paste, the paste is closed so the shell does not swallow what follows as pasted text.
    fn discard_queued_input(&mut self) {
        self.write_list
            .retain(|entry| !entry.class.is_discardable());
        if self
            .writing
            .as_ref()
            .is_some_and(|w| w.class.is_discardable())
        {
            let cut_paste = self
                .writing
                .take()
                .is_some_and(|w| w.cut_inside_bracketed_paste());
            if cut_paste {
                self.push_paste_close();
            }
        }
    }

    /// Removes the queued and in-flight jobs `scope` covers (#1384), at the next chunk boundary.
    ///
    /// A discardable write is cut where it stands (see [`Self::discard_queued_input`]). A setup
    /// job is cut before its next chunk; a chunk already partly written is finished first (a
    /// chunk is never split), and a bracketed paste the job left open is closed after it.
    fn abort(&mut self, scope: AbortScope) {
        if scope.covers(JobClass::UserCommand) {
            self.discard_queued_input();
        } else {
            for class in [JobClass::Paste, JobClass::Agent] {
                if scope.covers(class) {
                    self.abort_class(class);
                }
            }
        }
        if !scope.covers(JobClass::SetupChunked) {
            return;
        }
        self.write_list
            .retain(|entry| entry.class != JobClass::SetupChunked);
        let Some(job) = self.active_job.as_mut() else {
            return;
        };
        log::info!("pty write job {}: aborted", job.id);
        job.cut();
        let mid_chunk = self
            .writing
            .as_ref()
            .is_some_and(|w| w.class == JobClass::SetupChunked && w.written > 0);
        if mid_chunk {
            // `note_job_chunk_finished` ends the job and closes the paste.
            return;
        }
        if self
            .writing
            .as_ref()
            .is_some_and(|w| w.class == JobClass::SetupChunked)
        {
            self.writing = None;
        }
        let close_paste = job.is_paste_open();
        self.active_job = None;
        if close_paste {
            self.push_paste_close();
        }
    }

    #[inline]
    fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    #[inline]
    fn needs_write(&self) -> bool {
        if self.writing.is_some() {
            return true;
        }
        // Inside a setup job the list is not written from: only a reply at its head may go, and
        // the job's own next chunk once its gap has passed. Anything else waits for the whole job.
        if let Some(job) = &self.active_job {
            return job.is_exhausted() || job.is_due(Instant::now()) || self.has_leading_reply();
        }
        (self.reply_gate_open && !self.write_list.is_empty())
            || self.write_list.len() > self.gated_count
    }

    #[inline]
    fn set_current(&mut self, new: Option<Writing>) {
        self.writing = new;
    }

    /// Records that a queued write finished reaching the pty. Only user input starts the
    /// round-trip clock; a query reply is not something the user waits to see echoed (#1092).
    fn note_write_finished(&mut self, is_reply: bool, at: Instant) {
        if !is_reply {
            self.user_write_finished_at = Some(at);
            self.last_user_write_at = Some(at);
            if let Some(received) = self.user_input_received_at.take() {
                SLOW_PTY_WRITE.report(
                    at.saturating_duration_since(received),
                    at,
                    log::Level::Warn,
                    || "user input waited in the pty writer".to_string(),
                );
            }
        }
    }

    /// Notes that user input reached this loop, keeping the oldest unwritten one (#1146).
    fn note_user_input_received(&mut self, at: Instant) {
        self.user_input_received_at.get_or_insert(at);
    }

    /// How long ago user input finished reaching the pty, for the first output read after it;
    /// `None` once that read has been counted, so a flood logs one line per keystroke (#1092).
    fn take_user_write_age(&mut self, now: Instant) -> Option<Duration> {
        self.user_write_finished_at
            .take()
            .map(|at| now.saturating_duration_since(at))
    }

    /// Whether output read at `now` belongs to the echo of a keystroke: user input reached the
    /// pty within [`USER_WRITE_ECHO_WINDOW`]. Unlike [`Self::take_user_write_age`] this holds for
    /// every read in the window, not only the first, because a program answers a key in several
    /// writes (the character, then the cursor position; the echo, then the prompt redraw).
    fn read_follows_user_write(&self, now: Instant) -> bool {
        self.last_user_write_at
            .is_some_and(|at| now.saturating_duration_since(at) <= USER_WRITE_ECHO_WINDOW)
    }
}

/// The id of the active block if it is currently executing a command.
fn executing_block_id(terminal: &TerminalModel) -> Option<BlockId> {
    let block = terminal.block_list().active_block();
    block.is_executing().then(|| block.id().clone())
}

/// Parses one slice of pty output into the terminal model, queueing the replies the parser
/// generates. The executing block is read for each slice: the command may finish, and the next
/// one start, inside the parse of an earlier slice. The parser is incremental (it advances a
/// byte at a time and already sees arbitrary read boundaries), so slicing does not change the
/// result.
fn parse_slice(
    state: &mut State,
    terminal: &mut TerminalModel,
    reply_ledger: &SharedReplyLedger,
    bytes: &[u8],
) {
    // Any terminal response sequences the parser generates (e.g. cursor position / device
    // status reports) are collected into a buffer and queued onto `write_list` below, rather
    // than written directly to the PTY here: a direct write can hit `WouldBlock` if the PTY's
    // write buffer is full, and `ansi::Processor` does not retry a failed write, so a direct
    // write silently drops the response. `write_list`/`pty_write` already retry across
    // `WouldBlock`, so routing through it is what actually guarantees delivery.
    // Upstream: e59c7a491489 "Fix dropped terminal response sequences when PTY writes block"
    // (#11906).
    let mut terminal_response_sequences = Vec::new();
    // Read the active block BEFORE parsing: replies are generated, and the command may finish,
    // synchronously inside `parse_bytes`.
    let executing_block = executing_block_id(terminal);
    state.parser.parse_bytes(
        terminal,
        bytes,
        &mut ReplyWriter {
            replies: &mut terminal_response_sequences,
            ledger: reply_ledger,
        },
    );
    let evicted = state.queue_response(terminal_response_sequences, executing_block);
    if !evicted.is_empty() {
        let mut ledger = reply_ledger.lock();
        for bytes in &evicted {
            ledger.forget_replies(bytes);
        }
    }
}

impl Writing {
    #[inline]
    fn new(c: Cow<'static, [u8]>, class: JobClass) -> Writing {
        Writing {
            source: c,
            written: 0,
            class,
        }
    }

    /// A reply to a terminal query rather than user input.
    #[inline]
    fn is_reply(&self) -> bool {
        self.class.is_reply()
    }

    /// Whether the bytes written so far opened a bracketed paste that was not closed. Cutting the
    /// write off there would leave the shell reading everything after as pasted text.
    fn cut_inside_bracketed_paste(&self) -> bool {
        let written = &self.source[..self.written.min(self.source.len())];
        let last = |needle: &[u8]| written.windows(needle.len()).rposition(|w| w == needle);
        match (last(BRACKETED_PASTE_START), last(BRACKETED_PASTE_END)) {
            (Some(start), Some(end)) => start > end,
            (Some(_), None) => true,
            _ => false,
        }
    }

    #[inline]
    fn advance(&mut self, n: usize) {
        self.written += n;
    }

    #[inline]
    fn remaining_bytes(&self) -> &[u8] {
        &self.source[self.written..]
    }

    #[inline]
    fn finished(&self) -> bool {
        self.written >= self.source.len()
    }
}

/// Collects the replies the parser generates, and records each in the ledger as it is
/// generated, so a report parsed later in the same chunk sees them.
struct ReplyWriter<'a> {
    replies: &'a mut Vec<u8>,
    ledger: &'a SharedReplyLedger,
}

impl io::Write for ReplyWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.ledger.lock().note_replies(buf);
        self.replies.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

enum ChannelResult {
    Continue,
    TerminateLoop { child_exited: bool },
}

impl<T> EventLoop<T>
where
    T: local_tty::EventedPty + Send + 'static,
{
    /// Create a new event loop, registering the PTY and the message channel
    /// with a fresh [`mio::Poll`].
    ///
    /// Every step here can allocate a file descriptor (the epoll instance, the
    /// channel's eventfd waker, the PTY and its signal pipe registrations), so
    /// every step can fail with `EMFILE` when the process is out of fds
    /// (#882). That is done here, on the caller's thread, rather than on the
    /// PTY reader thread, so a failure is returned to the caller and shown on
    /// the tab like any other PTY spawn failure instead of panicking the
    /// reader thread. On failure the already-spawned shell is killed, as the
    /// event loop that would otherwise own its lifetime never runs.
    pub fn new(
        terminal: Arc<FairMutex<TerminalModel>>,
        event_listener: ChannelEventListener,
        mut pty: T,
        mut rx: Receiver<Message>,
    ) -> io::Result<EventLoop<T>> {
        let registered = mio::Poll::new().and_then(|poll| {
            poll.registry()
                .register(&mut rx, CHANNEL_TOKEN, Interest::READABLE)?;
            // Register TTY through EventedRW interface.
            pty.register(&poll, Interest::READABLE | Interest::WRITABLE)?;
            Ok(poll)
        });

        match registered {
            Ok(poll) => {
                let reply_ledger = terminal.lock().reply_ledger();
                Ok(EventLoop {
                    poll,
                    pty,
                    rx,
                    reply_ledger,
                    terminal,
                    event_listener,
                })
            }
            Err(err) => {
                // The `Poll` was dropped when the closure above returned the
                // error, closing the epoll instance and with it any
                // registration that did succeed.
                if let Err(kill_err) = pty.kill() {
                    log::error!(
                        "Failed to kill PTY process after event loop setup failed: {kill_err:?}"
                    );
                }
                Err(err)
            }
        }
    }

    /// Opens the reply gate once the pty has left canonical mode (or if that cannot be told, as
    /// on platforms without termios), and closes it while the pty is canonical.
    fn refresh_reply_gate(&self, state: &mut State) {
        let canonical = if state.has_gated() {
            self.pty.is_canonical()
        } else {
            None
        };
        state.set_reply_gate(canonical);
        // The tty mode a setup job is paced by is read here, on demand, just before the job can be
        // picked (one cheap `tcgetattr`, only while such a job waits).
        state.pty_canonical = if state.needs_canonical_probe() {
            self.pty.is_canonical()
        } else {
            None
        };
    }

    /// Drain the channel.
    /// Returns `false` when a shutdown message was received.
    fn drain_recv_channel(&mut self, state: &mut State) -> ChannelResult {
        // Resizes are coalesced keep-last (#1376): a window drag sends one per pixel, and each
        // would be its own TIOCSWINSZ + SIGWINCH. The last size seen is applied before any later
        // message that is ordered against it (input) and at the end of the drain, so the final
        // size is always applied and never reordered relative to writes.
        let mut pending_resize = None;
        while let Ok(msg) = self.rx.try_recv() {
            if !matches!(msg, Message::Resize(_)) {
                if let Some(size) = pending_resize.take() {
                    self.pty.on_resize(&size);
                }
            }
            match msg {
                Message::Input(input) => {
                    // The single choke point for everything written to the pty.
                    self.reply_ledger.lock().note_input(&input);
                    state.note_user_input_received(Instant::now());
                    // A signal byte (^C ^D ^Z ^\) preempts a setup job: it cuts the job at its
                    // next chunk boundary instead of queueing behind the rest of the script.
                    if is_signal_byte(&input) {
                        state.abort(AbortScope::Setup);
                    }
                    if input.contains(&0x03) {
                        warpui::input_order::log_hop("ctrl_c_received", || {
                            format!("queued={}", state.write_list.len())
                        });
                    }
                    state.queue_input(input)
                }
                Message::AgentInput(input) => {
                    self.reply_ledger.lock().note_input(&input);
                    state.note_user_input_received(Instant::now());
                    // An agent may write a ^C into its command: a signal byte preempts a setup
                    // job exactly as a typed one does.
                    if is_signal_byte(&input) {
                        state.abort(AbortScope::Setup);
                    }
                    state.queue_agent_input(input)
                }
                Message::PasteInput(input) => {
                    self.reply_ledger.lock().note_input(&input);
                    state.note_user_input_received(Instant::now());
                    state.queue_paste_input(input)
                }
                Message::SetupChunked { chunks, pacing } => {
                    // The ledger sees the bytes now, as it saw each chunk as its own `Input`.
                    // Not `note_user_input_received`: the gaps are not a user waiting.
                    {
                        let mut ledger = self.reply_ledger.lock();
                        for chunk in &chunks {
                            ledger.note_input(chunk);
                        }
                    }
                    state.queue_setup(chunks, pacing)
                }
                Message::Abort(scope) => state.abort(scope),
                Message::Shutdown => {
                    return ChannelResult::TerminateLoop {
                        child_exited: false,
                    };
                }
                Message::CommandInput(input) => {
                    self.reply_ledger.lock().note_input(&input);
                    state.note_user_input_received(Instant::now());
                    state.queue_command_input(input)
                }
                Message::DiscardQueuedInput => state.abort(AbortScope::Discardable),
                Message::Resize(size) => pending_resize = Some(size),
                Message::ChildExited => return ChannelResult::TerminateLoop { child_exited: true },
            }
        }
        if let Some(size) = pending_resize {
            self.pty.on_resize(&size);
        }

        ChannelResult::Continue
    }

    /// Returns a `bool` indicating whether or not the event loop should continue running.
    #[inline]
    fn channel_event(&mut self, state: &mut State) -> ChannelResult {
        self.drain_recv_channel(state)
    }

    /// Reads from the pty into the provided buffer, using the provided state
    /// information in order to properly advance the ANSI parser.
    /// If `writer` is `Some`, a copy of all bytes read will be written to that
    /// writer.
    /// Returns the number of bytes read from the PTY.
    #[inline]
    #[allow(clippy::unwrap_in_result)]
    fn pty_read(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        can_read: &mut bool,
    ) -> io::Result<()> {
        warpui::profiling::zone!("pty_read");
        let mut bytes_in_buffer = 0;
        let mut bytes_processed = 0;

        let mut terminal = None;

        // We read up to sizeof(buf) to limit the amount of time spent
        // reading from the PTY for a given event. Currently, the buf
        // has size [`MAX_READ`].
        loop {
            match self.pty.reader().read(&mut buf[bytes_in_buffer..]) {
                Ok(0) if bytes_in_buffer == 0 => {
                    // If we get 0 here with an empty buffer (guaranteed if
                    // bytes_in_buffer == 0), it means the object is unable to
                    // receive reads.
                    *can_read = false;
                    // There is nothing to be processed in the buffer, so return
                    // to the event loop.
                    break;
                }
                // Otherwise, track how many additional bytes we read and move
                // on to byte processing.
                Ok(got) => bytes_in_buffer += got,
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                        if err.kind() == ErrorKind::WouldBlock {
                            *can_read = false;
                        }
                        if bytes_in_buffer == 0 {
                            break;
                        }
                    }
                    _ => return Err(err),
                },
            }

            let terminal = match &mut terminal {
                Some(terminal) => terminal,
                None => terminal.insert(match self.terminal.try_lock() {
                    // If we've buffered a lot while the terminal was locked elsewhere, block
                    // on it rather than read on: the wait is one hold by the other thread
                    // (fair mutex), and the parse that follows stays small (#1135). The same
                    // goes once the pty has been drained (`WouldBlock`, `can_read` cleared):
                    // there is nothing more to accumulate, and reading on would only spin on
                    // `read` + `try_lock` for as long as the UI paints, a whole core burnt
                    // while the echo of a keystroke waits for the lock anyway.
                    None if bytes_in_buffer >= MAX_UNLOCKED_ACCUMULATION || !*can_read => {
                        self.terminal.lock()
                    }
                    // Otherwise, if we failed to acquire the lock, try to read more
                    // data into the buffer.
                    None => continue,
                    // Finally, if we acquired the lock, make use of it.
                    Some(terminal) => terminal,
                }),
            };

            // Parse in bounded slices, handing the lock to a waiting thread between them, so
            // no single hold covers everything read (#1135).
            let mut offset = 0;
            while offset < bytes_in_buffer {
                let end = (offset + PARSE_SLICE).min(bytes_in_buffer);
                warpui::profiling::zone!("parse");
                // What the main thread left for the model (a key's bookkeeping, #1403) is applied
                // before the bytes are parsed: the record of a key is in place before the echo it
                // causes. Then the facts the user-key path reads without this lock are published.
                terminal.apply_inbox();
                let held_since = Instant::now();
                parse_slice(
                    state,
                    terminal.deref_mut(),
                    &self.reply_ledger,
                    &buf[offset..end],
                );
                terminal.publish_key_facts();
                let held = held_since.elapsed();
                SLOW_PTY_PARSE.finish(held_since, || {
                    format!("{} bytes, terminal model locked", end - offset)
                });
                if held >= SLOW_PARSE_HOLD {
                    log::debug!(
                        "pty parse held the model lock for {held:?} ({} bytes)",
                        end - offset
                    );
                }
                offset = end;
                if offset < bytes_in_buffer {
                    FairMutexGuard::bump(terminal);
                }
            }

            bytes_processed += bytes_in_buffer;
            bytes_in_buffer = 0;

            if bytes_processed >= MAX_LOCKED_READ {
                break;
            }

            // Give up the lock to a waiting thread, if any, before reading
            // more bytes from the PTY.
            FairMutexGuard::bump(terminal);

            // Something (a keystroke, a resize, shutdown) is waiting in the channel: end the
            // pass here instead of after up to MAX_LOCKED_READ more bytes. `service_io` takes
            // the message and writes it before it reads on, so under a flood a Ctrl-C waits for
            // one read-and-parse rather than a whole pass.
            if self.rx.has_pending() {
                break;
            }
        }

        // #1092: time the first output after a keystroke reached the pty (counts only).
        let now = Instant::now();
        let follows_user_write = bytes_processed > 0 && state.read_follows_user_write(now);
        if bytes_processed > 0
            && let Some(age) = state.take_user_write_age(now)
        {
            warpui::input_order::log_pty_read_after_write(bytes_processed, age);
            SLOW_PTY_ECHO.report(age, Instant::now(), log::Level::Warn, || {
                format!("{bytes_processed} bytes read after the pty write")
            });
        }

        // Queue a terminal redraw if we processed some number
        // of non-(synchronized output) bytes. The echo of a keystroke asks the receiving
        // throttle to skip its trailing-edge wait (#1138).
        if bytes_processed > state.parser.sync_output_buffer_len().unwrap_or(0) {
            if follows_user_write {
                self.event_listener.send_after_user_input_wakeup_event();
            } else {
                self.event_listener.send_wakeup_event();
            }
        }

        Ok(())
    }

    #[inline]
    fn pty_write(&mut self, state: &mut State, can_write: &mut bool) -> io::Result<()> {
        warpui::profiling::zone!("pty_write");
        state.ensure_next();

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                match self.pty.writer().write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        // We never attempt to write an empty buffer, so if we
                        // get 0 here, it means the object is unable to receive
                        // writes.
                        *can_write = false;
                        break 'write_many;
                    }
                    Ok(n) => {
                        // #1162: a short write (a ConPTY pipe taking part of a buffer), and every
                        // reply written between user bytes, to place a lost or reordered byte.
                        if n < current.remaining_bytes().len() {
                            warpui::input_order::log_hop("pty_write_partial", || {
                                format!("wrote={n} of={}", current.remaining_bytes().len())
                            });
                        }
                        current.advance(n);
                        if current.finished() {
                            if current.is_reply() {
                                warpui::input_order::log_hop("pty_write_reply", || {
                                    format!("bytes={}", current.source.len())
                                });
                            }
                            let finished_at = Instant::now();
                            state.note_write_finished(current.is_reply(), finished_at);
                            if !current.is_reply() {
                                warpui::input_order::log_pty_write(current.source.len());
                                if current.source.contains(&0x03) {
                                    warpui::input_order::log_hop("ctrl_c_pty_write", || {
                                        format!("bytes={}", current.source.len())
                                    });
                                }
                            }
                            state.note_job_chunk_finished(&current, finished_at);
                            state.goto_next_at(finished_at);
                            break 'write_one;
                        }
                    }
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                                if err.kind() == ErrorKind::WouldBlock {
                                    *can_write = false;
                                    warpui::input_order::log_hop("pty_write_wouldblock", || {
                                        format!("queued={}", state.write_list.len())
                                    });
                                }
                                break 'write_many;
                            }
                            _ => return Err(err),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Reads and writes the pty for as long as it is ready, then returns so the caller can wait
    /// for the next readiness event. `Err(())` means the loop must stop.
    ///
    /// A hang-up (`EIO` from the master once the child has closed the slave) ends reading for
    /// good: `can_read` is cleared and the loop goes back to `poll`, where the child's exit
    /// event (or the shutdown message) arrives. It used to `continue` with `can_read` still set,
    /// which re-read the dead fd forever without ever polling: a 100% CPU "PTY reader" thread
    /// that could not be shut down, so closing the tab blocked the UI thread joining it (#953).
    fn service_io(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        can_read: &mut bool,
        can_write: &mut bool,
    ) -> Result<ChannelResult, ()> {
        self.refresh_reply_gate(state);
        while *can_read || (state.needs_write() && *can_write) {
            if *can_read {
                if let Err(err) = self.pty_read(state, buf, can_read) {
                    // On Linux, a `read` on the master side of a PTY can fail with `EIO` if the
                    // client side hangs up. `io::Error::kind()` cannot be used here: EIO maps
                    // to `ErrorKind::Uncategorized`, which is unstable and deliberately
                    // unmatchable from outside std. Compare the raw errno instead.
                    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                    if err.raw_os_error() == Some(libc::EIO) {
                        *can_read = false;
                        *can_write = false;
                        state.note_hung_up();
                        // Nothing queued can reach a dead pty, and a withheld reply would
                        // keep the gate poll timer armed.
                        state.discard_pending_writes();
                        return Ok(ChannelResult::Continue);
                    }

                    error!("Error reading from PTY in event loop: {err}");
                    return Err(());
                }

                // Under a sustained flood the pty never reports `WouldBlock`, so this loop
                // would not return to `poll` (the only other place the channel is drained)
                // until the output stopped: typed keys, resizes and shutdown waited behind
                // it (#1132). Take them between read passes. This must not return early:
                // `mio` is edge-triggered, so leaving with `can_read` still set would wait
                // for an edge that has already been consumed.
                if let ChannelResult::TerminateLoop { child_exited } =
                    self.drain_recv_channel(state)
                {
                    return Ok(ChannelResult::TerminateLoop { child_exited });
                }
            }

            self.refresh_reply_gate(state);
            if state.needs_write() && *can_write {
                if let Err(err) = self.pty_write(state, can_write) {
                    error!("Error writing to PTY in event loop: {err}");
                    return Err(());
                }
            }
        }
        Ok(ChannelResult::Continue)
    }

    /// What the loop does when it is asked to stop. Returns whether the child had exited.
    fn note_terminating(&self, child_exited: bool) -> bool {
        if child_exited {
            self.terminal.lock().exit(ExitReason::ShellProcessExited);
            self.event_listener.send_wakeup_event();
        }
        child_exited
    }

    pub fn spawn(mut self) -> JoinHandle<()> {
        #[cfg(test)]
        let feature_flag_overrides = warp_core::features::get_overrides();

        thread::Builder::new()
            .name("PTY reader".into())
            .spawn(move || {
                // Make sure any overridden feature flags are also overridden
                // in the PTY reader thread.
                #[cfg(test)]
                warp_core::features::set_overrides(feature_flag_overrides);

                let mut state = State {
                    coalesce_keys: self.pty.coalesces_key_writes() && coalesce_keys_enabled(),
                    ..State::default()
                };
                // Heap (calloc) rather than stack: a stack array is memset up front and
                // keeps ~256 KB resident per pane even when the pane is idle.
                let mut buf = vec![0u8; READ_BUFFER_SIZE].into_boxed_slice();

                // Keep track of whether we've "drained" read and write
                // readiness.  Once we receive a read or write readiness event,
                // we won't receive another until the operation would block.
                // These let us know whether we should keep processing reads
                // and writes, even without receiving a new readiness event.
                let mut can_read = false;
                let mut can_write = false;

                // `self.rx` and `self.pty` were registered with `self.poll`
                // in `EventLoop::new`, where a failure can be reported.

                let mut events = Events::with_capacity(1024);

                // True if the child exiting caused the event loop to wind down
                // (e.g. CTRL D or `exit`) rather than the inverse.
                let mut child_exited = false;
                let mut poll_failures = 0u32;

                'event_loop: loop {
                    // Clear the events so that we can reliably equate the absence of events
                    // to the timeout being fired.
                    events.clear();

                    // Wait for events, but only up to the remaining timeout for the synchronous output
                    // update (if any).
                    let sync_state_timeout = state.parser.sync_output_remaining_timeout();
                    // While replies are withheld, wake often enough to notice the pty leaving
                    // canonical mode.
                    let gate_poll = state.gate_timeout();
                    let job_poll = state.job_timeout();
                    let poll_timeout = state.poll_timeout(sync_state_timeout);
                    if let Err(err) = self.poll.poll(&mut events, poll_timeout) {
                        if err.kind() != ErrorKind::Interrupted {
                            poll_failures += 1;
                            log::error!("EventLoop polling error ({poll_failures}): {err:?}");
                        }
                        match poll_error_action(&err, poll_failures) {
                            PollErrorAction::Retry => continue,
                            PollErrorAction::EndSession => break 'event_loop,
                        }
                    }
                    poll_failures = 0;

                    // If there were no events but `poll` returned, that means we hit the timeout.
                    // (A wake-up for the reply gate alone is not the synchronized-output timeout.)
                    let woke_for_gate = gate_poll
                        .is_some_and(|gate| sync_state_timeout.is_none_or(|sync| gate < sync))
                        || job_poll
                            .is_some_and(|job| sync_state_timeout.is_none_or(|sync| job < sync))
                        || (state.hung_up
                            && sync_state_timeout.is_none_or(|sync| state.hung_up_poll() < sync));
                    if events.is_empty() && !woke_for_gate {
                        // Same reasoning as the `parse_bytes` call above: route any
                        // response sequences through `write_list` instead of writing
                        // directly, so a blocked PTY write queues and retries instead
                        // of silently dropping them.
                        let mut terminal_response_sequences = Vec::new();
                        let executing_block = {
                            let mut terminal = self.terminal.lock();
                            let executing_block = executing_block_id(&terminal);
                            state.parser.finish_sync_output(
                                &mut *terminal,
                                &mut ReplyWriter {
                                    replies: &mut terminal_response_sequences,
                                    ledger: &self.reply_ledger,
                                },
                            );
                            executing_block
                        };
                        let evicted =
                            state.queue_response(terminal_response_sequences, executing_block);
                        if !evicted.is_empty() {
                            let mut ledger = self.reply_ledger.lock();
                            for bytes in &evicted {
                                ledger.forget_replies(bytes);
                            }
                        }
                        // The flush may have made buffered content visible; ask for a
                        // redraw rather than waiting for the next pty read (#818).
                        self.event_listener.send_wakeup_event();
                        // Fall through (no events to iterate) so the write loop below
                        // delivers any responses queued by the flush right away.
                    }

                    for event in events.iter() {
                        match event.token() {
                            token if token == CHANNEL_TOKEN => {
                                match self.channel_event(&mut state) {
                                    ChannelResult::Continue => {}
                                    ChannelResult::TerminateLoop {
                                        child_exited: exited,
                                    } => {
                                        child_exited = self.note_terminating(exited);
                                        break 'event_loop;
                                    }
                                }
                            }

                            token if token == self.pty.child_event_token() => {
                                if let Some(local_tty::ChildEvent::Exited) =
                                    self.pty.next_child_event()
                                {
                                    self.terminal.lock().exit(ExitReason::ShellProcessExited);
                                    child_exited = true;
                                    self.event_listener.send_wakeup_event();
                                    break 'event_loop;
                                }
                            }

                            token
                                if token == self.pty.read_token()
                                    || token == self.pty.write_token() =>
                            {
                                #[cfg(unix)]
                                if event.is_read_closed() || event.is_write_closed() {
                                    // Don't try to do I/O on a dead PTY.
                                    continue;
                                }

                                if event.is_readable() {
                                    can_read = true;
                                }
                                if event.is_writable() {
                                    can_write = true;
                                }
                            }
                            _ => (),
                        }
                    }

                    // As long as we have work to do, do it.  Once we need to
                    // wait on some readiness (pty readability, pty writability,
                    // or new data to write), go back to the start of the event
                    // loop.
                    if state.has_tagged() {
                        let terminal = self.terminal.lock();
                        let block = terminal.block_list().active_block();
                        let dropped = state.drop_stale_responses(block.id(), block.is_executing());
                        if !dropped.is_empty() {
                            let mut ledger = self.reply_ledger.lock();
                            for bytes in &dropped {
                                ledger.forget_replies(bytes);
                            }
                        }
                    }
                    self.refresh_reply_gate(&mut state);
                    match self.service_io(&mut state, &mut buf, &mut can_read, &mut can_write) {
                        Ok(ChannelResult::Continue) => {}
                        Ok(ChannelResult::TerminateLoop {
                            child_exited: exited,
                        }) => {
                            child_exited = self.note_terminating(exited);
                            break 'event_loop;
                        }
                        Err(()) => break 'event_loop,
                    }
                    // After a hang-up the exit event may already have been consumed; ask the
                    // child directly rather than waiting on a signal that will not come.
                    state.warn_if_hung_up_too_long();
                    if state.hung_up && self.pty.has_exited() {
                        self.terminal.lock().exit(ExitReason::ShellProcessExited);
                        child_exited = true;
                        self.event_listener.send_wakeup_event();
                        break 'event_loop;
                    }
                }

                // The evented instances are not dropped here so deregister them explicitly.
                let _ = self.poll.registry().deregister(&mut self.rx);
                let _ = self.pty.deregister(&self.poll);

                // Terminate the PTY process, if it's not the initiator of the shutdown.
                if !child_exited {
                    let res = self.pty.kill();
                    if let Err(err) = res {
                        log::error!("Failed to kill PTY process {err:?}");
                    }
                }
                // Notify the terminal model that the PTY process has exited.
                self.terminal.lock().exit(ExitReason::PtyDisconnected);
            })
            .expect("thread spawn works")
    }
}

#[cfg(test)]
#[path = "event_loop_tests.rs"]
mod tests;
