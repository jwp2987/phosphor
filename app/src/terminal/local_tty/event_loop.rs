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

use crate::terminal::{
    TerminalModel,
    event_listener::ChannelEventListener,
    local_tty,
    model::{BlockId, ansi, reply_ledger::SharedReplyLedger},
};
use crate::terminal::{model::terminal_model::ExitReason, writeable_pty::Message};

use super::mio_channel::Receiver;

/// The size of the buffer to read data into from the PTY.
const READ_BUFFER_SIZE: usize = 0x4_0000;

/// Max bytes to process from the PTY while holding the lock before giving
/// someone else an opportunity to lock it.
const MAX_LOCKED_READ: usize = 0x1_0000;

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
}

/// All of the mutable state needed to run the event loop.
/// Contains list of items to write, current write state, etc. Anything that
/// would otherwise be mutated on the `EventLoop` goes here.
pub struct State {
    /// One FIFO of everything bound for the pty, so user input can never overtake an
    /// earlier terminal-query reply.
    write_list: VecDeque<QueuedWrite>,
    writing: Option<Writing>,
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
}

/// How often a withheld reply re-checks whether the pty has left canonical mode: promptly for the
/// first second (a program that queries and then switches to raw mode does so within
/// milliseconds), then less often for a reply nobody is waiting for.
const REPLY_GATE_POLL: Duration = Duration::from_millis(10);
const REPLY_GATE_POLL_SLOW: Duration = Duration::from_millis(100);
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
    bytes: Cow<'static, [u8]>,
    produced_while_executing: Option<BlockId>,
    /// A reply to a terminal query, as opposed to user input.
    is_reply: bool,
}

impl QueuedWrite {
    /// Replies generated while a command runs wait for the pty to leave canonical mode.
    fn is_gated(&self) -> bool {
        self.is_reply && self.produced_while_executing.is_some()
    }
}

impl Default for State {
    fn default() -> State {
        State {
            write_list: VecDeque::new(),
            parser: ansi::Processor::new(),
            writing: None,
            reply_gate_open: true,
            gated_count: 0,
            withheld_since: None,
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
        // The first entry that may be written now: a withheld reply is skipped, not waited for,
        // so user input behind it is never delayed.
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
        self.writing = next.map(|entry| Writing::new(entry.bytes));
    }

    /// Whether any queued write is a reply that the gate may be withholding.
    #[inline]
    fn has_gated(&self) -> bool {
        self.gated_count > 0
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
        match (sync_timeout, self.gate_timeout()) {
            (Some(sync), Some(gate)) => Some(sync.min(gate)),
            (sync, gate) => sync.or(gate),
        }
    }

    /// Queues user input; never dropped.
    fn queue_input(&mut self, bytes: Cow<'static, [u8]>) {
        self.write_list.push_back(QueuedWrite {
            bytes,
            produced_while_executing: None,
            is_reply: false,
        });
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
            bytes: Cow::Owned(bytes),
            produced_while_executing: executing_block,
            is_reply: true,
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

    #[inline]
    fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    #[inline]
    fn needs_write(&self) -> bool {
        self.writing.is_some()
            || (self.reply_gate_open && !self.write_list.is_empty())
            || self.write_list.len() > self.gated_count
    }

    #[inline]
    fn set_current(&mut self, new: Option<Writing>) {
        self.writing = new;
    }
}

/// The id of the active block if it is currently executing a command.
fn executing_block_id(terminal: &TerminalModel) -> Option<BlockId> {
    let block = terminal.block_list().active_block();
    block.is_executing().then(|| block.id().clone())
}

impl Writing {
    #[inline]
    fn new(c: Cow<'static, [u8]>) -> Writing {
        Writing {
            source: c,
            written: 0,
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
    }

    /// Drain the channel.
    /// Returns `false` when a shutdown message was received.
    fn drain_recv_channel(&mut self, state: &mut State) -> ChannelResult {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Message::Input(input) => {
                    // The single choke point for everything written to the pty.
                    self.reply_ledger.lock().note_input(&input);
                    state.queue_input(input)
                }
                Message::Shutdown => {
                    return ChannelResult::TerminateLoop {
                        child_exited: false,
                    };
                }
                Message::Resize(size) => self.pty.on_resize(&size),
                Message::ChildExited => return ChannelResult::TerminateLoop { child_exited: true },
            }
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
                    // If we've filled up the buffer, block on locking the terminal.
                    None if bytes_in_buffer >= READ_BUFFER_SIZE => self.terminal.lock(),
                    // Otherwise, if we failed to acquire the lock, try to read more
                    // data into the buffer.
                    None => continue,
                    // Finally, if we acquired the lock, make use of it.
                    Some(terminal) => terminal,
                }),
            };

            // Process the bytes read into the buffer. Any terminal response
            // sequences the parser generates (e.g. cursor position / device
            // status reports) are collected into a buffer and queued onto
            // `write_list` below, rather than written directly to the PTY
            // here: a direct write can hit `WouldBlock` if the PTY's write
            // buffer is full, and `ansi::Processor` does not retry a failed
            // write, so a direct write silently drops the response.
            // `write_list`/`pty_write` already retry across `WouldBlock`
            // (see below), so routing through it is what actually
            // guarantees delivery.
            // Upstream: e59c7a491489 "Fix dropped terminal response
            // sequences when PTY writes block" (#11906).
            let mut terminal_response_sequences = Vec::new();
            // Read the active block BEFORE parsing: replies are generated, and the
            // command may finish, synchronously inside `parse_bytes`.
            let executing_block = executing_block_id(terminal);
            state.parser.parse_bytes(
                terminal.deref_mut(),
                &buf[..bytes_in_buffer],
                &mut ReplyWriter {
                    replies: &mut terminal_response_sequences,
                    ledger: &self.reply_ledger,
                },
            );
            let evicted = state.queue_response(terminal_response_sequences, executing_block);
            if !evicted.is_empty() {
                let mut ledger = self.reply_ledger.lock();
                for bytes in &evicted {
                    ledger.forget_replies(bytes);
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
        }

        // Queue a terminal redraw if we processed some number
        // of non-(synchronized output) bytes.
        if bytes_processed > state.parser.sync_output_buffer_len().unwrap_or(0) {
            self.event_listener.send_wakeup_event();
        }

        Ok(())
    }

    #[inline]
    fn pty_write(&mut self, state: &mut State, can_write: &mut bool) -> io::Result<()> {
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
                        current.advance(n);
                        if current.finished() {
                            state.goto_next();
                            break 'write_one;
                        }
                    }
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                                if err.kind() == ErrorKind::WouldBlock {
                                    *can_write = false;
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

                let mut state = State::default();
                let mut buf = [0u8; READ_BUFFER_SIZE];

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
                    let poll_timeout = state.poll_timeout(sync_state_timeout);
                    if let Err(err) = self.poll.poll(&mut events, poll_timeout) {
                        match err.kind() {
                            ErrorKind::Interrupted => continue,
                            _ => panic!("EventLoop polling error: {err:?}"),
                        }
                    }

                    // If there were no events but `poll` returned, that means we hit the timeout.
                    // (A wake-up for the reply gate alone is not the synchronized-output timeout.)
                    let woke_for_gate = gate_poll
                        .is_some_and(|gate| sync_state_timeout.is_none_or(|sync| gate < sync));
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
                                        if exited {
                                            self.terminal
                                                .lock()
                                                .exit(ExitReason::ShellProcessExited);
                                            child_exited = true;
                                            self.event_listener.send_wakeup_event();
                                        }
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
                    while can_read || (state.needs_write() && can_write) {
                        if can_read {
                            match self.pty_read(&mut state, &mut buf, &mut can_read) {
                                Ok(_) => {}
                                Err(err) => {
                                    // On Linux, a `read` on the master side of a PTY can fail
                                    // with `EIO` if the client side hangs up.  In that case,
                                    // just loop back round for the inevitable `Exited` event.
                                    // This sucks, but checking the process is either racy or
                                    // blocking.
                                    //
                                    // `io::Error::kind()` cannot be used here: EIO maps to
                                    // `ErrorKind::Uncategorized`, which is unstable and
                                    // deliberately unmatchable from outside std. Compare the
                                    // raw errno instead.
                                    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                                    if err.raw_os_error() == Some(libc::EIO) {
                                        continue;
                                    }

                                    error!("Error reading from PTY in event loop: {err}");
                                    break 'event_loop;
                                }
                            }
                        }

                        self.refresh_reply_gate(&mut state);
                        if state.needs_write() && can_write {
                            if let Err(err) = self.pty_write(&mut state, &mut can_write) {
                                error!("Error writing to PTY in event loop: {err}");
                                break 'event_loop;
                            }
                        }
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
