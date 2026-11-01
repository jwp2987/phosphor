//! Tests for PTY event loop setup (#882).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use super::*;
use crate::terminal::SizeInfo;
use crate::terminal::local_tty::{ChildEvent, EventedReadWrite, mio_channel};
use crate::terminal::model::reply_ledger::ReplyLedger;

/// A PTY stand-in whose registration can be made to fail, recording whether
/// the event loop killed it.
struct FakePty {
    register_error: Option<i32>,
    killed: Arc<AtomicBool>,
    reader: FakeReader,
    writer: CaptureWriter,
    /// What `is_canonical` reports: 0 = cannot tell, 1 = raw, 2 = canonical.
    mode: Arc<AtomicU8>,
    /// Every `on_resize` call, as (rows, columns).
    resizes: Arc<Mutex<Vec<(usize, usize)>>>,
}

/// A pty reader that reports end-of-file, or fails with a fixed errno on every read.
#[derive(Default)]
struct FakeReader {
    errno: Option<i32>,
    /// When set, the reader hands out `flood.chunk` bytes per read for `flood.reads` reads without
    /// ever reporting `WouldBlock`, like a `yes` that outpaces the parser, then `WouldBlock`.
    flood: Option<Flood>,
    /// How many reads have been made.
    reads: Arc<AtomicUsize>,
}

struct Flood {
    chunk: usize,
    reads: usize,
}

impl io::Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(flood) = &mut self.flood {
            if flood.reads == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            flood.reads -= 1;
            let n = flood.chunk.min(buf.len());
            buf[..n].fill(b'x');
            return Ok(n);
        }
        match self.errno {
            Some(errno) => Err(io::Error::from_raw_os_error(errno)),
            None => Ok(0),
        }
    }
}

/// A pty writer that records what the event loop writes.
#[derive(Clone, Default)]
struct CaptureWriter(
    Arc<Mutex<Vec<u8>>>,
    Option<WriteProbe>,
    /// When set, how many more bytes the "tty" takes before it answers `WouldBlock` (a full input
    /// queue); a test refills it to model the shell reading.
    Option<Arc<Mutex<usize>>>,
    /// An overlapped pipe (the Windows ConPTY): when set, a write is taken whole and sets this
    /// flag (the write is in flight); every write while it is set answers `WouldBlock`, until a
    /// test clears it (the completion event).
    Option<Arc<AtomicBool>>,
);

/// Records how many reads had been made when the first write happened.
#[derive(Clone)]
struct WriteProbe {
    reads: Arc<AtomicUsize>,
    reads_at_first_write: Arc<AtomicUsize>,
}

impl io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(probe) = &self.1 {
            let _ = probe.reads_at_first_write.compare_exchange(
                usize::MAX,
                probe.reads.load(Ordering::SeqCst),
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
        let mut take = buf.len();
        if let Some(in_flight) = &self.3 {
            if in_flight.swap(true, Ordering::SeqCst) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
        }
        if let Some(budget) = &self.2 {
            let mut budget = budget.lock().unwrap();
            if *budget == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            take = take.min(*budget);
            *budget -= take;
        }
        self.0.lock().unwrap().extend_from_slice(&buf[..take]);
        Ok(take)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const MODE_UNKNOWN: u8 = 0;
const MODE_RAW: u8 = 1;
const MODE_CANONICAL: u8 = 2;

impl FakePty {
    fn new(register_error: Option<i32>) -> (Self, Arc<AtomicBool>) {
        let killed = Arc::new(AtomicBool::new(false));
        (
            Self {
                register_error,
                killed: killed.clone(),
                reader: FakeReader::default(),
                writer: CaptureWriter::default(),
                mode: Arc::new(AtomicU8::new(MODE_UNKNOWN)),
                resizes: Arc::default(),
            },
            killed,
        )
    }
}

impl EventedReadWrite for FakePty {
    type Reader = FakeReader;
    type Writer = CaptureWriter;

    fn register(&mut self, _: &mio::Poll, _: mio::Interest) -> io::Result<()> {
        match self.register_error {
            Some(errno) => Err(io::Error::from_raw_os_error(errno)),
            None => Ok(()),
        }
    }

    fn reregister(&mut self, _: &mio::Poll, _: mio::Interest) -> io::Result<()> {
        Ok(())
    }

    fn deregister(&mut self, _: &mio::Poll) -> io::Result<()> {
        Ok(())
    }

    fn reader(&mut self) -> &mut FakeReader {
        &mut self.reader
    }

    fn read_token(&self) -> mio::Token {
        PTY_TOKEN
    }

    fn writer(&mut self) -> &mut CaptureWriter {
        &mut self.writer
    }

    fn write_token(&self) -> mio::Token {
        PTY_TOKEN
    }
}

impl local_tty::EventedPty for FakePty {
    fn is_canonical(&self) -> Option<bool> {
        match self.mode.load(Ordering::SeqCst) {
            MODE_RAW => Some(false),
            MODE_CANONICAL => Some(true),
            _ => None,
        }
    }

    fn child_event_token(&self) -> mio::Token {
        SIGNALS_TOKEN
    }

    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }

    fn on_resize(&mut self, size: &SizeInfo) {
        self.resizes
            .lock()
            .unwrap()
            .push((size.rows(), size.columns()));
    }

    fn kill(self) -> anyhow::Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn new_event_loop(pty: FakePty) -> io::Result<EventLoop<FakePty>> {
    let (_tx, rx) = mio_channel::channel::<Message>();
    EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
}

/// #882: running out of fds while registering the PTY used to panic the
/// "PTY reader" thread on an `unwrap`, and the tab blamed the bootstrap
/// script. Setup now returns the error -- with its "Too many open files"
/// cause intact for the tab -- and kills the shell it will never serve.
#[cfg(unix)]
#[test]
fn emfile_during_setup_is_returned_and_kills_the_shell() {
    let (pty, killed) = FakePty::new(Some(libc::EMFILE));

    let err = match new_event_loop(pty) {
        Ok(_) => panic!("setup must fail when PTY registration fails"),
        Err(err) => err,
    };

    assert_eq!(err.raw_os_error(), Some(libc::EMFILE));
    assert!(
        killed.load(Ordering::SeqCst),
        "the spawned shell must be killed when its event loop cannot start"
    );

    // What the tab shows: `on_pty_spawn_failed` renders the error with `{:#}`
    // after `start_pty_event_loop` adds its context.
    let shown = format!(
        "{:#}",
        anyhow::Error::from(err).context("error preparing terminal I/O")
    );
    assert!(
        shown.contains("Too many open files"),
        "tab message should name the cause, got: {shown}"
    );
}

#[test]
fn successful_setup_leaves_the_shell_running() {
    let (pty, killed) = FakePty::new(None);

    assert!(new_event_loop(pty).is_ok());
    assert!(!killed.load(Ordering::SeqCst));
}

fn block(id: &str) -> BlockId {
    BlockId::from(id.to_owned())
}

fn drain(state: &mut State) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        state.ensure_next();
        match state.take_current() {
            Some(writing) => out.push(writing.remaining_bytes().to_vec()),
            None => return out,
        }
    }
}

/// #829: a query reply and the command's end in the same read. The reply was tagged
/// with the block executing before the parse; the block has since changed, so drop it.
#[test]
fn reply_in_the_same_parse_as_the_command_end_is_dropped() {
    let mut state = State::default();
    state.queue_response(b"\x1b[?62;c".to_vec(), Some(block("cmd")));

    // After the parse the active block is the next (not executing) one.
    state.drop_stale_responses(&block("next"), false);

    assert!(!state.needs_write());
}

/// A reply produced at the prompt (not executing) is always kept.
#[test]
fn reply_produced_at_the_prompt_is_kept() {
    let mut state = State::default();
    state.queue_response(b"\x1b[1;1R".to_vec(), None);

    state.drop_stale_responses(&block("next"), false);

    assert_eq!(drain(&mut state), vec![b"\x1b[1;1R".to_vec()]);
}

/// A reply produced while a command ran and which is still running is kept.
#[test]
fn reply_is_kept_while_the_same_command_is_still_running() {
    let mut state = State::default();
    state.queue_response(b"\x1b[?62;c".to_vec(), Some(block("cmd")));

    state.drop_stale_responses(&block("cmd"), true);

    assert_eq!(drain(&mut state), vec![b"\x1b[?62;c".to_vec()]);
}

/// Same block id but no longer executing (finished) also drops the reply.
#[test]
fn reply_is_dropped_when_its_block_stopped_executing() {
    let mut state = State::default();
    state.queue_response(b"\x1b[?62;c".to_vec(), Some(block("cmd")));

    state.drop_stale_responses(&block("cmd"), false);

    assert!(!state.needs_write());
}

/// Input queued after a reply goes out after it; input is never dropped.
#[test]
fn input_queued_after_a_reply_is_written_after_it() {
    let mut state = State::default();
    state.queue_response(b"\x1b[?62;c".to_vec(), Some(block("cmd")));
    state.queue_input(Cow::Borrowed(b"ls\n"));

    assert_eq!(
        drain(&mut state),
        vec![b"\x1b[?62;c".to_vec(), b"ls\n".to_vec()]
    );
}

#[test]
fn input_survives_stale_reply_dropping_in_order() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"a"));
    state.queue_response(b"R".to_vec(), Some(block("cmd")));
    state.queue_input(Cow::Borrowed(b"b"));

    state.drop_stale_responses(&block("next"), false);

    assert_eq!(drain(&mut state), vec![b"a".to_vec(), b"b".to_vec()]);
}

/// #921: every message the channel carries is recorded in the ledger as it is drained, so no
/// write to the pty (typed keys, a released hold, a command, an agent's input) goes unrecorded.
#[test]
fn every_input_message_is_recorded_in_the_reply_ledger() {
    let (tx, rx) = mio_channel::channel::<Message>();
    let mut event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        FakePty::new(None).0,
        rx,
    )
    .expect("event loop");
    event_loop.reply_ledger.lock().note_replies(b"\x1b[24;1R");
    tx.send(Message::Input(Cow::Borrowed(b"R"))).unwrap();
    let mut state = State::default();
    event_loop.drain_recv_channel(&mut state);
    // The "R" the channel carried was written, so the report "R" is that text, not residue.
    assert_eq!(
        event_loop.reply_ledger.lock().filter_report("R".into()),
        "R"
    );
    assert_eq!(drain(&mut state), vec![b"R".to_vec()]);
}

#[test]
fn replies_are_recorded_as_the_parser_writes_them() {
    use std::io::Write as _;
    let ledger = SharedReplyLedger::default();
    let mut replies = Vec::new();
    let mut writer = ReplyWriter {
        replies: &mut replies,
        ledger: &ledger,
    };
    writer.write_all(b"\x1b[24;1R").unwrap();
    assert_eq!(ledger.lock().filter_report("R".into()), "");
    assert_eq!(replies, b"\x1b[24;1R");
}

/// #921: a stale reply the event loop drops (#829) never reaches the pty, so the ledger must
/// not keep it as something that could explain text in the shell's line.
#[test]
fn a_dropped_stale_reply_is_returned_so_the_ledger_can_forget_it() {
    let mut state = State::default();
    state.queue_response(b"\x1b[24;1R".to_vec(), Some(block("cmd")));
    state.queue_response(b"\x1b[?62;c".to_vec(), None);
    let dropped = state.drop_stale_responses(&block("next"), false);
    assert_eq!(dropped, vec![Cow::<[u8]>::Owned(b"\x1b[24;1R".to_vec())]);

    let mut ledger = ReplyLedger::default();
    ledger.note_replies(b"\x1b[24;1R");
    ledger.note_replies(b"\x1b[?62;c");
    ledger.forget_replies(&dropped[0]);
    // The CPR was never written; only the device attributes reply could have left residue.
    assert_eq!(ledger.filter_report("R".into()), "R");
}

fn closed_gate_state() -> State {
    let mut state = State::default();
    state.reply_gate_open = false;
    state
}

/// #937: a reply generated while a command runs is not written while the pty is canonical
/// (nobody can read it there), but user input behind it is never held up.
#[test]
fn a_withheld_reply_does_not_delay_user_input() {
    let mut state = closed_gate_state();
    state.queue_response(b"\x1b[24;1R".to_vec(), Some(block("cmd")));
    state.queue_input(Cow::Borrowed(b"ls"));
    assert!(state.is_withholding());
    assert!(state.needs_write());
    assert_eq!(drain(&mut state), vec![b"ls".to_vec()]);
    assert!(!state.needs_write(), "only the withheld reply is left");
    assert!(state.is_withholding());
}

#[test]
fn a_withheld_reply_is_written_once_the_gate_opens() {
    let mut state = closed_gate_state();
    state.queue_input(Cow::Borrowed(b"a"));
    state.queue_response(b"\x1b[24;1R".to_vec(), Some(block("cmd")));
    state.queue_input(Cow::Borrowed(b"b"));
    assert_eq!(drain(&mut state), vec![b"a".to_vec(), b"b".to_vec()]);
    state.reply_gate_open = true;
    assert!(state.needs_write());
    assert_eq!(drain(&mut state), vec![b"\x1b[24;1R".to_vec()]);
}

#[test]
fn replies_produced_at_the_prompt_are_never_withheld() {
    let mut state = closed_gate_state();
    state.queue_response(b"\x1b[?62;c".to_vec(), None);
    assert!(!state.is_withholding());
    assert_eq!(drain(&mut state), vec![b"\x1b[?62;c".to_vec()]);
}

#[test]
fn an_open_gate_keeps_the_original_order() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"a"));
    state.queue_response(b"R".to_vec(), Some(block("cmd")));
    state.queue_input(Cow::Borrowed(b"b"));
    assert_eq!(
        drain(&mut state),
        vec![b"a".to_vec(), b"R".to_vec(), b"b".to_vec()]
    );
}

#[test]
fn a_withheld_reply_is_dropped_when_its_command_finishes() {
    let mut state = closed_gate_state();
    state.queue_response(b"\x1b[24;1R".to_vec(), Some(block("cmd")));
    let dropped = state.drop_stale_responses(&block("next"), false);
    assert_eq!(dropped, vec![Cow::<[u8]>::Owned(b"\x1b[24;1R".to_vec())]);
    assert!(!state.is_withholding());
    state.reply_gate_open = true;
    assert!(!state.needs_write());
}

#[test]
fn a_pty_that_cannot_tell_its_mode_never_withholds() {
    let (pty, _) = FakePty::new(None);
    assert_eq!(local_tty::EventedPty::is_canonical(&pty), None);
}

/// An event loop over a pty whose mode and written bytes the test controls.
struct GateHarness {
    event_loop: EventLoop<FakePty>,
    mode: Arc<AtomicU8>,
    written: Arc<Mutex<Vec<u8>>>,
}

fn gate_harness() -> GateHarness {
    let (pty, _) = FakePty::new(None);
    let mode = pty.mode.clone();
    let written = pty.writer.0.clone();
    GateHarness {
        event_loop: new_event_loop(pty).expect("event loop"),
        mode,
        written,
    }
}

impl GateHarness {
    /// What one pass of the event loop does before writing: recompute the gate, then write.
    fn pass(&mut self, state: &mut State) {
        self.event_loop.refresh_reply_gate(state);
        let mut can_write = true;
        if state.needs_write() {
            self.event_loop.pty_write(state, &mut can_write).unwrap();
        }
    }

    /// One write pass with the caller's `can_write` flag (a `WouldBlock` clears it).
    fn pass_with(&mut self, state: &mut State, can_write: &mut bool) {
        self.event_loop.refresh_reply_gate(state);
        if state.needs_write() && *can_write {
            self.event_loop.pty_write(state, can_write).unwrap();
        }
    }

    fn take_written(&self) -> Vec<u8> {
        std::mem::take(&mut *self.written.lock().unwrap())
    }
}

/// `[R(gated), K]` writes `K` while the pty is canonical, and `R` once it leaves canonical mode.
#[test]
fn the_loop_writes_a_withheld_reply_only_after_the_pty_leaves_canonical_mode() {
    let mut h = gate_harness();
    let mut state = State::default();
    state.queue_response(b"\x1b[24;1R".to_vec(), Some(block("cmd")));
    state.queue_input(Cow::Borrowed(b"K"));

    h.mode.store(MODE_CANONICAL, Ordering::SeqCst);
    h.pass(&mut state);
    assert_eq!(h.take_written(), b"K");
    assert!(state.is_withholding());
    h.pass(&mut state);
    assert_eq!(h.take_written(), b"", "still canonical: still withheld");

    h.mode.store(MODE_RAW, Ordering::SeqCst);
    h.pass(&mut state);
    assert_eq!(h.take_written(), b"\x1b[24;1R");
    assert!(!state.is_withholding());
}

#[test]
fn the_loop_recomputes_the_gate_when_the_pty_returns_to_canonical_mode() {
    let mut h = gate_harness();
    let mut state = State::default();
    h.mode.store(MODE_RAW, Ordering::SeqCst);
    state.queue_response(b"A".to_vec(), Some(block("cmd")));
    h.pass(&mut state);
    assert_eq!(h.take_written(), b"A", "raw: written at once");

    h.mode.store(MODE_CANONICAL, Ordering::SeqCst);
    state.queue_response(b"B".to_vec(), Some(block("cmd")));
    h.pass(&mut state);
    assert_eq!(h.take_written(), b"", "canonical again: withheld");
}

#[test]
fn a_pty_that_cannot_tell_its_mode_writes_replies_at_once() {
    let mut h = gate_harness();
    let mut state = State::default();
    state.queue_response(b"A".to_vec(), Some(block("cmd")));
    h.pass(&mut state);
    assert_eq!(h.take_written(), b"A");
}

/// The wake-up that re-checks the pty is 10 ms for the first second, then 100 ms; the
/// synchronized-output timeout still wins when it is sooner; nothing withheld, no gate wake-up.
#[test]
fn the_poll_timeout_follows_the_gate() {
    let mut state = closed_gate_state();
    assert_eq!(state.poll_timeout(None), None, "nothing withheld");
    assert_eq!(
        state.poll_timeout(Some(Duration::from_millis(7))),
        Some(Duration::from_millis(7))
    );

    state.queue_response(b"R".to_vec(), Some(block("cmd")));
    state.set_reply_gate(Some(true));
    assert_eq!(state.poll_timeout(None), Some(REPLY_GATE_POLL));
    assert_eq!(
        state.poll_timeout(Some(Duration::from_millis(3))),
        Some(Duration::from_millis(3))
    );
    assert_eq!(
        state.poll_timeout(Some(Duration::from_secs(5))),
        Some(REPLY_GATE_POLL)
    );

    // After a second of being held, the re-check backs off.
    state.withheld_since = Some(Instant::now() - Duration::from_secs(2));
    assert_eq!(state.poll_timeout(None), Some(REPLY_GATE_POLL_SLOW));

    // Released or dropped: no gate wake-up and the clock restarts.
    state.set_reply_gate(Some(false));
    assert_eq!(state.poll_timeout(None), None);
    assert_eq!(state.withheld_since, None);
}

/// A flood of unread queries stays bounded and every question asked per iteration is O(1).
#[test]
fn a_flood_of_gated_replies_is_bounded_and_cheap() {
    let mut state = closed_gate_state();
    let mut evicted = 0;
    for _ in 0..100_000 {
        evicted += state
            .queue_response(b"\x1b[1;1R".to_vec(), Some(block("cmd")))
            .len();
    }
    assert_eq!(state.gated_count, MAX_GATED_REPLIES);
    assert_eq!(evicted, 100_000 - MAX_GATED_REPLIES);
    assert_eq!(state.write_list.len(), MAX_GATED_REPLIES);

    let started = Instant::now();
    for _ in 0..100_000 {
        assert!(!state.needs_write());
        assert!(state.is_withholding());
        assert!(state.has_gated());
        assert!(state.has_tagged());
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

/// The same questions with 100k entries really queued (bypassing the cap): still O(1).
#[test]
fn the_per_iteration_questions_are_constant_time_with_a_hundred_thousand_gated_entries() {
    let mut state = closed_gate_state();
    for _ in 0..100_000 {
        state.write_list.push_back(QueuedWrite {
            id: 0,
            bytes: Cow::Borrowed(b"R"),
            produced_while_executing: Some(block("cmd")),
            class: JobClass::Reply,
            setup: None,
        });
    }
    state.gated_count = 100_000;
    let started = Instant::now();
    for _ in 0..100_000 {
        assert!(!state.needs_write());
        assert!(state.is_withholding());
        assert!(state.has_gated());
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    // Dropping them keeps the count right.
    let dropped = state.drop_stale_responses(&block("next"), false);
    assert_eq!(dropped.len(), 100_000);
    assert_eq!(state.gated_count, 0);
    assert!(!state.has_gated());
}

#[test]
fn the_gated_count_follows_writes_and_drops() {
    let mut state = closed_gate_state();
    state.queue_response(b"A".to_vec(), Some(block("cmd")));
    state.queue_response(b"B".to_vec(), Some(block("cmd")));
    state.queue_input(Cow::Borrowed(b"K"));
    assert_eq!(state.gated_count, 2);
    assert_eq!(drain(&mut state), vec![b"K".to_vec()]);
    assert_eq!(state.gated_count, 2);
    state.reply_gate_open = true;
    assert_eq!(drain(&mut state), vec![b"A".to_vec(), b"B".to_vec()]);
    assert_eq!(state.gated_count, 0);
}

/// #953: a hang-up (`EIO` on the master) with a gated reply queued must end the I/O pass
/// instead of re-reading the dead fd forever. Run on a thread so a regression fails the test
/// rather than hanging it.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn a_hung_up_pty_ends_the_io_pass_and_drops_queued_writes() {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut pty, _) = FakePty::new(None);
        pty.reader.errno = Some(libc::EIO);
        pty.mode.store(MODE_CANONICAL, Ordering::SeqCst);
        let mut event_loop = new_event_loop(pty).expect("event loop");
        let mut state = State::default();
        // A gated reply (held because the pty is canonical) and user input behind it.
        state.queue_response(b"reply".to_vec(), Some(block("b")));
        state.queue_input(Cow::Borrowed(b"typed"));
        let mut buf = [0u8; 64];
        let (mut can_read, mut can_write) = (true, true);
        let mut passes = 0;
        // The caller returns to `poll` after each pass; a pass that leaves `can_read` set
        // would be re-run immediately.
        while can_read && passes < 100 {
            event_loop
                .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
                .expect("EIO is a hang-up, not an error");
            passes += 1;
        }
        let _ = done_tx.send((
            passes,
            can_read,
            can_write,
            state.hung_up,
            state.needs_write(),
            state.has_gated(),
        ));
    });
    let (passes, can_read, can_write, hung_up, needs_write, has_gated) = done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("service_io spun on a hung-up pty");
    assert_eq!(passes, 1);
    assert!(!can_read && !can_write);
    assert!(hung_up);
    assert!(!needs_write && !has_gated, "queued writes are dropped");
}

#[test]
fn a_hung_up_pty_wakes_the_loop_to_look_for_the_exit() {
    let mut state = State::default();
    assert_eq!(state.poll_timeout(None), None);
    state.hung_up = true;
    assert_eq!(state.poll_timeout(None), Some(HUNG_UP_POLL));
    assert_eq!(
        state.poll_timeout(Some(Duration::from_millis(5))),
        Some(Duration::from_millis(5))
    );
}

#[test]
fn a_long_hung_up_pty_backs_off_and_warns_once() {
    let mut state = State::default();
    state.note_hung_up();
    assert_eq!(state.hung_up_poll(), HUNG_UP_POLL);
    state.warn_if_hung_up_too_long();
    assert!(!state.hung_up_warned, "no warning right after the hang-up");
    state.hung_up_since = Some(Instant::now() - HUNG_UP_WARN_AFTER - Duration::from_millis(1));
    assert_eq!(state.hung_up_poll(), HUNG_UP_POLL_SLOW);
    assert_eq!(state.poll_timeout(None), Some(HUNG_UP_POLL_SLOW));
    state.warn_if_hung_up_too_long();
    assert!(state.hung_up_warned);
}

/// Any other read error still stops the loop.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn a_fatal_read_error_still_stops_the_loop() {
    let (mut pty, _) = FakePty::new(None);
    pty.reader.errno = Some(libc::EBADF);
    let mut event_loop = new_event_loop(pty).expect("event loop");
    let mut state = State::default();
    let mut buf = [0u8; 64];
    let (mut can_read, mut can_write) = (true, true);
    assert!(
        event_loop
            .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
            .is_err()
    );
}

/// A multi-line command as `bytes_to_execute_command` writes it: one bracketed paste of `n`
/// lines plus the submit CR.
fn big_paste(n: usize) -> Vec<u8> {
    let mut bytes = BRACKETED_PASTE_START.to_vec();
    for i in 0..n {
        bytes.extend_from_slice(format!("# comment line {i}\n").as_bytes());
    }
    bytes.extend_from_slice(BRACKETED_PASTE_END);
    bytes.push(b'\r');
    bytes
}

/// #1046: a big multi-line submission is written a tty-buffer at a time, and a ^C queued behind
/// it waited for all of it. Discarding drops the unwritten lines of the submitted command, closes
/// its half-written bracketed paste, and keeps everything else queued, in order.
#[test]
fn discard_queued_input_drops_only_the_submitted_command() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"bootstrap script\r"));
    state.queue_command_input(Cow::Owned(big_paste(2500)));
    state.queue_input(Cow::Borrowed(b"typed ahead"));
    state.queue_response(b"\x1b[?62c".to_vec(), None);
    state.queue_command_input(Cow::Borrowed(b"echo later\r"));

    // The bootstrap goes out, then the tty accepts only 4 KiB of the paste.
    state.ensure_next();
    let first = state.take_current().unwrap();
    assert_eq!(first.remaining_bytes(), b"bootstrap script\r");
    state.goto_next();
    let mut current = state.take_current().expect("the paste is being written");
    assert!(current.class.is_discardable());
    current.advance(4096);
    state.set_current(Some(current));

    state.discard_queued_input();
    state.queue_input(Cow::Borrowed(&[0x03]));

    assert_eq!(
        drain(&mut state),
        vec![
            BRACKETED_PASTE_END.to_vec(),
            b"typed ahead".to_vec(),
            b"\x1b[?62c".to_vec(),
            vec![0x03]
        ]
    );
}

/// A ^C with no submitted command in flight changes nothing in the queue.
#[test]
fn discard_queued_input_without_a_command_changes_nothing() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"bootstrap\r"));
    state.queue_input(Cow::Borrowed(b"typed ahead"));
    state.queue_response(b"\x1b[?62c".to_vec(), None);
    state.ensure_next();
    let mut current = state.take_current().unwrap();
    current.advance(3);
    state.set_current(Some(current));

    state.discard_queued_input();

    assert_eq!(
        drain(&mut state),
        vec![
            // "boo" of "bootstrap\r" was already written.
            b"tstrap\r".to_vec(),
            b"typed ahead".to_vec(),
            b"\x1b[?62c".to_vec()
        ]
    );
}

/// A command cut outside a paste (a raw multi-line write to a shell without bracketed paste) is
/// dropped without a stray paste-close.
#[test]
fn discard_queued_input_adds_no_paste_close_outside_a_paste() {
    let mut state = State::default();
    let raw: Vec<u8> = (0..2500)
        .flat_map(|i| format!("# comment line {i}\r").into_bytes())
        .collect();
    state.queue_command_input(Cow::Owned(raw));
    state.ensure_next();
    let mut current = state.take_current().unwrap();
    current.advance(4096);
    state.set_current(Some(current));

    state.discard_queued_input();

    assert!(drain(&mut state).is_empty());
}

/// A paste that was fully written before the discard is not closed a second time.
#[test]
fn discard_queued_input_does_not_double_close_a_finished_paste() {
    let mut state = State::default();
    let mut writing = Writing::new(Cow::Owned(big_paste(3)), JobClass::UserCommand);
    let end_of_paste = writing.source.len() - 1;
    writing.advance(end_of_paste);
    assert!(!writing.cut_inside_bracketed_paste());
    state.set_current(Some(writing));
    state.discard_queued_input();
    assert!(drain(&mut state).is_empty());
}

/// #1092: the keystroke round-trip clock starts when user input reaches the pty, is read once by
/// the first output after it, and is never started by a query reply.
#[test]
fn user_write_age_is_taken_once_and_replies_do_not_start_it() {
    let mut state = State::default();
    let start = Instant::now();
    assert_eq!(state.take_user_write_age(start), None);

    state.note_write_finished(true, start);
    assert_eq!(state.take_user_write_age(start), None);

    state.note_write_finished(false, start);
    let later = start + Duration::from_millis(30);
    assert_eq!(
        state.take_user_write_age(later),
        Some(Duration::from_millis(30))
    );
    assert_eq!(state.take_user_write_age(later), None);
}

/// #1138: every read within the echo window of a user write is the keystroke's echo, not only the
/// first one (a TUI answers a key with the character and then the cursor position), and a query
/// reply opens no window.
#[test]
fn reads_within_the_echo_window_all_follow_the_user_write() {
    let mut state = State::default();
    let start = Instant::now();
    assert!(!state.read_follows_user_write(start));

    state.note_write_finished(true, start);
    assert!(!state.read_follows_user_write(start));

    state.note_write_finished(false, start);
    assert!(state.read_follows_user_write(start + Duration::from_millis(2)));
    // The first read took the round-trip age; the second read is still the echo.
    assert!(state.take_user_write_age(start + Duration::from_millis(2)).is_some());
    assert!(state.read_follows_user_write(start + Duration::from_millis(5)));
    assert!(state.read_follows_user_write(start + USER_WRITE_ECHO_WINDOW));
    assert!(!state.read_follows_user_write(start + USER_WRITE_ECHO_WINDOW + Duration::from_millis(1)));
}

/// An event loop over a flooding pty, with the sending half of its channel, the number of reads
/// made, the number of reads made when the first byte was written to the pty, and the bytes
/// written.
fn flood_harness(
    chunk: usize,
    reads: usize,
) -> (
    EventLoop<FakePty>,
    mio_channel::Sender<Message>,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<u8>>>,
) {
    let (mut pty, _) = FakePty::new(None);
    pty.reader.flood = Some(Flood { chunk, reads });
    let read_count = pty.reader.reads.clone();
    let at_first_write = Arc::new(AtomicUsize::new(usize::MAX));
    pty.writer.1 = Some(WriteProbe {
        reads: read_count.clone(),
        reads_at_first_write: at_first_write.clone(),
    });
    let written = pty.writer.0.clone();
    let (tx, rx) = mio_channel::channel::<Message>();
    let event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");
    (event_loop, tx, read_count, at_first_write, written)
}

/// #1132: typed bytes queued while the pty floods output are written between read passes, not
/// after the flood ends. On the old loop the channel was drained only after `poll` reported it,
/// which a never-ending flood never returns to, so the input is not written at all by this
/// `service_io` call and the `written` assertion fails.
#[test]
fn input_queued_during_a_flood_is_written_before_the_flood_ends() {
    const FLOOD_READS: usize = 400;
    let (mut event_loop, tx, reads, at_first_write, written) = flood_harness(0x4000, FLOOD_READS);
    tx.send(Message::Input(Cow::Borrowed(b"q"))).unwrap();

    let mut state = State::default();
    let mut buf = vec![0u8; READ_BUFFER_SIZE];
    let (mut can_read, mut can_write) = (true, true);
    let result = event_loop
        .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
        .expect("service_io");
    assert!(matches!(result, ChannelResult::Continue));

    assert_eq!(&*written.lock().unwrap(), b"q");
    assert!(
        at_first_write.load(Ordering::SeqCst) <= 8,
        "the input was written after {} of {FLOOD_READS} reads",
        at_first_write.load(Ordering::SeqCst)
    );
    // Reading carried on to the end of the flood: nothing broke out early.
    assert!(reads.load(Ordering::SeqCst) > FLOOD_READS);
    assert!(!can_read);
}

/// Several inputs queued during a flood are written in the order they were sent.
#[test]
fn inputs_during_a_flood_keep_their_order() {
    let (mut event_loop, tx, _, _, written) = flood_harness(0x4000, 200);
    for part in [&b"a"[..], b"b", b"c"] {
        tx.send(Message::Input(Cow::Owned(part.to_vec()))).unwrap();
    }
    let mut state = State::default();
    let mut buf = vec![0u8; READ_BUFFER_SIZE];
    let (mut can_read, mut can_write) = (true, true);
    event_loop
        .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
        .expect("service_io");
    assert_eq!(&*written.lock().unwrap(), b"abc");
}

/// #1132/#1135: a shutdown message queued during a flood stops the pass at once, instead of
/// after the flood, and tells the caller to terminate. (A flood of a million reads would take
/// minutes if the pass did not stop.)
#[test]
fn shutdown_during_a_flood_ends_the_pass_promptly() {
    const FLOOD_READS: usize = 1_000_000;
    let (mut event_loop, tx, reads, _, _) = flood_harness(0x4000, FLOOD_READS);
    tx.send(Message::Shutdown).unwrap();

    let mut state = State::default();
    let mut buf = vec![0u8; READ_BUFFER_SIZE];
    let (mut can_read, mut can_write) = (true, true);
    let result = event_loop
        .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
        .expect("service_io");
    assert!(matches!(
        result,
        ChannelResult::TerminateLoop {
            child_exited: false
        }
    ));
    assert!(reads.load(Ordering::SeqCst) < 100);
}

/// Parses `input` in slices of `slice_len` and returns what the active block shows and the
/// replies queued for the pty.
fn parse_in_slices(input: &[u8], slice_len: usize) -> (String, Vec<u8>) {
    let mut model = TerminalModel::mock(None, None);
    let ledger = model.reply_ledger();
    let mut state = State::default();
    for slice in input.chunks(slice_len) {
        parse_slice(&mut state, &mut model, &ledger, slice);
    }
    (
        model.block_list().active_block().output_to_string(),
        drain(&mut state).concat(),
    )
}

/// Output is identical whether the parser sees the bytes in one call or in slices, including
/// slices that cut escape sequences and multi-byte characters, and the replies it generates are
/// the same bytes (#1135).
#[test]
fn slice_parsing_matches_one_call() {
    let mut input = Vec::new();
    for i in 0..300 {
        input.extend_from_slice(
            format!("line {i} \u{e9}\u{4e16}\u{754c} \x1b[31mred\x1b[0m \x1b[c\r\n").as_bytes(),
        );
    }
    let whole = parse_in_slices(&input, input.len());
    assert!(
        !whole.1.is_empty(),
        "the device-attributes queries are answered"
    );
    for slice_len in [1usize, 7, 64, PARSE_SLICE] {
        assert_eq!(
            parse_in_slices(&input, slice_len),
            whole,
            "slice length {slice_len}"
        );
    }
}

/// A Ctrl-C typed during a flood reaches the pty as an ETX byte through the same queue as any
/// other key (only a pending ssh connect uses a direct SIGINT), so it must be written within one
/// read pass, along with the discard-queued-input message sent before it.
#[test]
fn etx_during_a_flood_is_written_within_one_read_pass() {
    let (mut event_loop, tx, _, at_first_write, written) = flood_harness(0x4000, 1_000);
    tx.send(Message::DiscardQueuedInput).unwrap();
    tx.send(Message::Input(Cow::Borrowed(b"\x03"))).unwrap();

    let mut state = State::default();
    let mut buf = vec![0u8; READ_BUFFER_SIZE];
    let (mut can_read, mut can_write) = (true, true);
    event_loop
        .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
        .expect("service_io");

    assert_eq!(&*written.lock().unwrap(), b"\x03");
    // One pass reads at most MAX_LOCKED_READ bytes: four 16 KB reads.
    assert!(at_first_write.load(Ordering::SeqCst) <= 4);
}

/// A keystroke waiting in the channel ends the read pass after the read in progress, not after
/// MAX_LOCKED_READ bytes more: the pty reader parses ~16 KB in a few ms, so a whole 64 KB pass
/// was the longest a Ctrl-C could wait behind a flood. On the old loop the ETX is written after
/// four 16 KB reads (a full pass), so `at_first_write == 1` fails.
#[test]
fn input_waiting_during_a_flood_ends_the_pass_after_one_read() {
    let (mut event_loop, tx, reads, at_first_write, written) = flood_harness(0x4000, 1_000);
    tx.send(Message::Input(Cow::Borrowed(b"\x03"))).unwrap();

    let mut state = State::default();
    let mut buf = vec![0u8; READ_BUFFER_SIZE];
    let (mut can_read, mut can_write) = (true, true);
    event_loop
        .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
        .expect("service_io");

    assert_eq!(&*written.lock().unwrap(), b"\x03");
    assert_eq!(at_first_write.load(Ordering::SeqCst), 1);
    // The flood was still read to its end afterwards: ending the pass early drops no output.
    assert!(reads.load(Ordering::SeqCst) > 1_000);
    assert!(!can_read);
}

/// Output already read while the UI holds the terminal model lock (a paint) is parsed once the
/// lock is free; the reader must wait for it, not spin on `read` + `try_lock` until then. On the
/// old loop a `WouldBlock` with bytes buffered `continue`d straight back to `read`, so a 100 ms
/// hold cost thousands of reads (a whole core) and `reads < 10` fails.
#[test]
fn a_reader_with_buffered_output_waits_for_the_model_lock_instead_of_spinning() {
    let (mut pty, _) = FakePty::new(None);
    // One three-byte echo, then `WouldBlock`.
    pty.reader.flood = Some(Flood { chunk: 3, reads: 1 });
    let reads = pty.reader.reads.clone();
    let (_tx, rx) = mio_channel::channel::<Message>();
    let terminal = Arc::new(FairMutex::new(TerminalModel::mock(None, None)));
    let mut event_loop = EventLoop::new(
        terminal.clone(),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");

    // The "UI thread" takes the lock and holds it for a while.
    let (locked_tx, locked_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = terminal.lock();
        locked_tx.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(100));
    });
    locked_rx.recv().unwrap();

    let mut state = State::default();
    let mut buf = vec![0u8; READ_BUFFER_SIZE];
    let (mut can_read, mut can_write) = (true, true);
    event_loop
        .service_io(&mut state, &mut buf, &mut can_read, &mut can_write)
        .expect("service_io");
    holder.join().unwrap();

    let reads = reads.load(Ordering::SeqCst);
    assert!(reads < 10, "the reader made {reads} reads while it waited");
    assert!(!can_read);
}

/// #1376: a window drag sends a Resize per pixel; only the last size reaches the pty (one
/// ioctl), and it is applied before input sent after it.
#[test]
fn a_resize_burst_applies_only_the_last_size_and_keeps_write_order() {
    let (tx, rx) = mio_channel::channel::<Message>();
    let (pty, _) = FakePty::new(None);
    let resizes = pty.resizes.clone();
    let mut event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");
    for n in 10..60 {
        tx.send(Message::Resize(SizeInfo::new_without_font_metrics(n, n)))
            .unwrap();
    }
    event_loop.drain_recv_channel(&mut State::default());
    assert_eq!(*resizes.lock().unwrap(), vec![(59, 59)]);

    // A resize, then input, then another resize: both applied, each in order around the write.
    resizes.lock().unwrap().clear();
    let mut state = State::default();
    tx.send(Message::Resize(SizeInfo::new_without_font_metrics(20, 20)))
        .unwrap();
    tx.send(Message::Resize(SizeInfo::new_without_font_metrics(21, 21)))
        .unwrap();
    tx.send(Message::Input(Cow::Borrowed(b"x"))).unwrap();
    tx.send(Message::Resize(SizeInfo::new_without_font_metrics(30, 30)))
        .unwrap();
    event_loop.drain_recv_channel(&mut state);
    assert_eq!(*resizes.lock().unwrap(), vec![(21, 21), (30, 30)]);
}

/// #1376: a failed poll no longer panics the reader: it is retried a few times, then the session
/// ends cleanly. EINTR is always just retried.
#[test]
fn a_poll_error_is_recoverable_not_a_panic() {
    let eintr = io::Error::from(ErrorKind::Interrupted);
    let other = io::Error::from_raw_os_error(libc::ENOMEM);
    assert_eq!(poll_error_action(&eintr, 100), PollErrorAction::Retry);
    assert_eq!(poll_error_action(&other, 1), PollErrorAction::Retry);
    assert_eq!(
        poll_error_action(&other, MAX_CONSECUTIVE_POLL_ERRORS + 1),
        PollErrorAction::EndSession
    );
}

// ---- #1384: the typed write queue ------------------------------------------------------------

/// A gap long enough that no test ever reaches it by waiting: tests move the clock themselves.
const TEST_GAP: Duration = Duration::from_secs(3600);

fn gap() -> Pacing {
    Pacing::Gap {
        dur: TEST_GAP,
        why: "test",
    }
}

/// What one pass of the writer does at `now`: pick the next write if none is in flight, write it
/// in full, and note it finished (what `pty_write` does around the fd).
fn write_next(state: &mut State, now: Instant) -> Option<Vec<u8>> {
    if state.writing.is_none() {
        state.goto_next_at(now);
    }
    let writing = state.take_current()?;
    let bytes = writing.remaining_bytes().to_vec();
    state.note_job_chunk_finished(&writing, now);
    Some(bytes)
}

fn setup_chunks(chunks: &[&[u8]]) -> Vec<Vec<u8>> {
    chunks.iter().map(|c| c.to_vec()).collect()
}

/// Strict FIFO across every class: a setup job is one entry in the same line as keys, commands
/// and replies, and everything behind it is written after all of it.
#[test]
fn the_queue_is_one_fifo_across_classes() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"key1"));
    state.queue_command_input(Cow::Borrowed(b"cmd\r"));
    state.queue_response(b"\x1b[?62c".to_vec(), None);
    state.queue_setup(setup_chunks(&[b"S1", b"S2", b"S3"]), Pacing::None);
    state.queue_input(Cow::Borrowed(b"key2"));
    state.queue_response(b"\x1b[0n".to_vec(), None);

    assert_eq!(
        drain(&mut state),
        vec![
            b"key1".to_vec(),
            b"cmd\r".to_vec(),
            b"\x1b[?62c".to_vec(),
            b"S1".to_vec(),
            b"S2".to_vec(),
            b"S3".to_vec(),
            b"key2".to_vec(),
            // A reply behind the job lands after it, like any other write.
            b"\x1b[0n".to_vec(),
        ]
    );
}

/// A key typed while a setup job is between chunks is written after the WHOLE job, never inside
/// it, and the writer does not spin on it while the gap runs.
#[test]
fn a_key_typed_during_a_setup_job_is_written_after_the_whole_job() {
    let mut state = State::default();
    let t0 = Instant::now();
    state.queue_setup(setup_chunks(&[b"A", b"B", b"C"]), gap());
    assert_eq!(write_next(&mut state, t0), Some(b"A".to_vec()));

    state.queue_input(Cow::Borrowed(b"k"));
    // Mid-gap: the key is queued, nothing is due, so there is nothing to write and no busy loop.
    assert_eq!(write_next(&mut state, t0), None);
    assert!(!state.needs_write());
    assert!(state.job_timeout().is_some());
    assert!(state.poll_timeout(None).is_some());

    let t1 = t0 + TEST_GAP;
    assert_eq!(write_next(&mut state, t1), Some(b"B".to_vec()));
    assert_eq!(write_next(&mut state, t1), None);
    let t2 = t1 + TEST_GAP;
    assert_eq!(write_next(&mut state, t2), Some(b"C".to_vec()));
    // The job is over: the key goes straight out, with no further gap.
    assert_eq!(write_next(&mut state, t2), Some(b"k".to_vec()));
    assert!(state.active_job.is_none());
    assert!(state.job_timeout().is_none());
}

/// A reply interleaves only at a chunk boundary: never inside a chunk being written, and not
/// when a user key is ahead of it in the line.
#[test]
fn replies_interleave_only_at_chunk_boundaries() {
    let mut state = State::default();
    let t0 = Instant::now();
    state.queue_setup(setup_chunks(&[b"AAAA", b"BBBB"]), gap());
    state.goto_next_at(t0);
    let mut chunk = state.take_current().unwrap();
    chunk.advance(2);
    state.set_current(Some(chunk));

    // A reply arrives while the chunk is half written: the chunk stays the write in flight.
    state.queue_response(b"R".to_vec(), None);
    assert_eq!(
        state.writing.as_ref().unwrap().class,
        JobClass::SetupChunked
    );
    let chunk = state.take_current().unwrap();
    assert_eq!(chunk.remaining_bytes(), b"AA");
    state.note_job_chunk_finished(&chunk, t0);

    // At the boundary the reply goes, then the gap holds the next chunk back.
    assert_eq!(write_next(&mut state, t0), Some(b"R".to_vec()));
    assert_eq!(write_next(&mut state, t0), None);
    assert_eq!(
        write_next(&mut state, t0 + TEST_GAP),
        Some(b"BBBB".to_vec())
    );

    // A reply behind a key does not jump the key, and neither goes inside a job.
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"A", b"B"]), gap());
    assert_eq!(write_next(&mut state, t0), Some(b"A".to_vec()));
    state.queue_input(Cow::Borrowed(b"k"));
    state.queue_response(b"R".to_vec(), None);
    assert_eq!(write_next(&mut state, t0), None);
    assert_eq!(write_next(&mut state, t0 + TEST_GAP), Some(b"B".to_vec()));
    assert_eq!(write_next(&mut state, t0 + TEST_GAP), Some(b"k".to_vec()));
    assert_eq!(write_next(&mut state, t0 + TEST_GAP), Some(b"R".to_vec()));
}

/// An abort between chunks cuts the job before its next chunk; queued replies and keys survive.
#[test]
fn abort_setup_cuts_the_job_at_a_chunk_boundary_and_keeps_everything_else() {
    let t0 = Instant::now();
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"A", b"B", b"C"]), gap());
    assert_eq!(write_next(&mut state, t0), Some(b"A".to_vec()));
    state.queue_input(Cow::Borrowed(b"k"));
    state.queue_response(b"R".to_vec(), None);
    state.abort(AbortScope::Setup);

    assert!(state.active_job.is_none());
    assert_eq!(drain(&mut state), vec![b"k".to_vec(), b"R".to_vec()]);
}

/// A queued (not yet started) setup job is dropped by an abort too.
#[test]
fn abort_setup_drops_a_queued_job_and_leaves_user_commands_alone() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"first"));
    state.queue_setup(setup_chunks(&[b"S"]), Pacing::None);
    state.queue_command_input(Cow::Borrowed(b"cmd\r"));
    state.abort(AbortScope::Setup);
    assert_eq!(
        drain(&mut state),
        vec![b"first".to_vec(), b"cmd\r".to_vec()]
    );
}

/// Cutting a job inside a bracketed paste closes the paste, once, ahead of what follows.
#[test]
fn aborting_a_job_inside_a_bracketed_paste_appends_the_paste_close() {
    let mut open = BRACKETED_PASTE_START.to_vec();
    open.extend_from_slice(b"script part 1");
    let mut close = b"script part 2".to_vec();
    close.extend_from_slice(BRACKETED_PASTE_END);
    close.push(b'\r');

    let t0 = Instant::now();
    let mut state = State::default();
    state.queue_setup(vec![open.clone(), close.clone()], gap());
    assert_eq!(write_next(&mut state, t0), Some(open.clone()));
    state.abort(AbortScope::Setup);
    state.queue_input(Cow::Borrowed(&[0x03]));
    assert_eq!(
        drain(&mut state),
        vec![BRACKETED_PASTE_END.to_vec(), vec![0x03]]
    );

    // After the whole job the paste is already closed: an abort then has nothing to close.
    let mut state = State::default();
    state.queue_setup(vec![open, close.clone()], gap());
    assert!(write_next(&mut state, t0).is_some());
    assert_eq!(write_next(&mut state, t0 + TEST_GAP), Some(close));
    state.abort(AbortScope::Setup);
    assert!(drain(&mut state).is_empty());
}

/// A chunk already partly in the pty is finished (a chunk is never split), then the job ends and
/// the paste it left open is closed.
#[test]
fn aborting_mid_chunk_finishes_the_chunk_then_closes_the_paste() {
    let mut open = BRACKETED_PASTE_START.to_vec();
    open.extend_from_slice(b"script part 1");
    let mut state = State::default();
    state.queue_setup(vec![open.clone(), b"never written".to_vec()], gap());
    state.goto_next_at(Instant::now());
    let mut chunk = state.take_current().unwrap();
    chunk.advance(3);
    state.set_current(Some(chunk));

    state.abort(AbortScope::Setup);
    state.queue_input(Cow::Borrowed(b"k"));

    let rest = state
        .take_current()
        .expect("the chunk in flight is finished");
    assert_eq!(rest.remaining_bytes(), &open[3..]);
    state.note_job_chunk_finished(&rest, Instant::now());
    assert!(state.active_job.is_none());
    assert_eq!(
        drain(&mut state),
        vec![BRACKETED_PASTE_END.to_vec(), b"k".to_vec()]
    );
}

/// An abort that finds a chunk picked but with no byte written drops it whole.
#[test]
fn aborting_before_a_chunk_is_started_drops_it() {
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"A", b"B"]), Pacing::None);
    state.goto_next_at(Instant::now());
    assert!(state.writing.is_some());
    state.abort(AbortScope::Setup);
    assert!(state.writing.is_none());
    assert!(drain(&mut state).is_empty());
}

/// A newer setup job replaces the unwritten rest of an older one.
#[test]
fn a_newer_setup_job_replaces_the_older_one() {
    let t0 = Instant::now();
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"a1", b"a2", b"a3"]), gap());
    assert_eq!(write_next(&mut state, t0), Some(b"a1".to_vec()));
    state.queue_setup(setup_chunks(&[b"b1"]), gap());
    assert_eq!(write_next(&mut state, t0), Some(b"b1".to_vec()));
    assert_eq!(write_next(&mut state, t0 + TEST_GAP), None);
}

/// The gap is the writer's own deadline: the poll timeout follows it, and it is gone with the job.
#[test]
fn the_poll_timeout_follows_the_setup_gap() {
    let mut state = State::default();
    assert_eq!(state.poll_timeout(None), None);
    state.queue_setup(
        setup_chunks(&[b"A", b"B"]),
        Pacing::Gap {
            dur: Duration::from_millis(25),
            why: "test",
        },
    );
    assert_eq!(
        state.poll_timeout(None),
        None,
        "the first chunk is due at once"
    );
    assert_eq!(write_next(&mut state, Instant::now()), Some(b"A".to_vec()));
    let wait = state.poll_timeout(None).expect("waiting for the gap");
    assert!(wait <= Duration::from_millis(25), "{wait:?}");
    // The synchronized-output timeout still wins when it is sooner.
    assert_eq!(
        state.poll_timeout(Some(Duration::from_millis(1))),
        Some(Duration::from_millis(1))
    );
    state.abort(AbortScope::Setup);
    assert_eq!(state.poll_timeout(None), None);
}

/// A dead pty drops the job with the rest of the queue.
#[test]
fn discarding_pending_writes_drops_the_setup_job() {
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"A", b"B"]), gap());
    assert!(write_next(&mut state, Instant::now()).is_some());
    state.discard_pending_writes();
    assert!(state.active_job.is_none());
    assert!(!state.needs_write());
}

/// Job ids are monotonic and unique across classes.
#[test]
fn job_ids_are_monotonic() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"a"));
    state.queue_command_input(Cow::Borrowed(b"b"));
    state.queue_response(b"c".to_vec(), None);
    state.queue_setup(setup_chunks(&[b"d"]), Pacing::None);
    let ids: Vec<JobId> = state.write_list.iter().map(|entry| entry.id).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
}

/// The loop end to end over the message channel: a setup job is written chunk by chunk, a ^C
/// byte preempts it (no more chunks, the open paste closed) and itself goes out.
#[test]
fn ctrl_c_preempts_a_setup_job_through_the_event_loop() {
    let (tx, rx) = mio_channel::channel::<Message>();
    let (pty, _) = FakePty::new(None);
    let written = pty.writer.0.clone();
    let mut event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");

    let mut open = BRACKETED_PASTE_START.to_vec();
    open.extend_from_slice(b"part 1");
    let mut state = State::default();
    tx.send(Message::SetupChunked {
        chunks: vec![open.clone(), b"part 2".to_vec(), b"part 3".to_vec()],
        pacing: gap(),
    })
    .unwrap();
    event_loop.drain_recv_channel(&mut state);
    let mut can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    assert_eq!(
        *written.lock().unwrap(),
        open,
        "the first chunk goes at once"
    );

    // A key and the ^C arrive during the gap. The ^C aborts the job and goes out; the key typed
    // before it keeps its place ahead of it.
    tx.send(Message::Input(Cow::Borrowed(b"k"))).unwrap();
    tx.send(Message::Input(Cow::Borrowed(&[0x03]))).unwrap();
    event_loop.drain_recv_channel(&mut state);
    event_loop.pty_write(&mut state, &mut can_write).unwrap();

    let mut expected = open;
    expected.extend_from_slice(BRACKETED_PASTE_END);
    expected.extend_from_slice(b"k");
    expected.push(0x03);
    assert_eq!(*written.lock().unwrap(), expected);
    assert!(state.active_job.is_none());
}

/// `Message::Abort` cuts a job the same way, and `Message::DiscardQueuedInput` still only drops
/// submitted commands (a setup job survives it).
#[test]
fn abort_message_cuts_a_job_and_discard_leaves_it() {
    let (tx, rx) = mio_channel::channel::<Message>();
    let (pty, _) = FakePty::new(None);
    let written = pty.writer.0.clone();
    let mut event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");
    let mut state = State::default();
    tx.send(Message::SetupChunked {
        chunks: setup_chunks(&[b"one", b"two"]),
        pacing: gap(),
    })
    .unwrap();
    tx.send(Message::DiscardQueuedInput).unwrap();
    event_loop.drain_recv_channel(&mut state);
    let mut can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    assert_eq!(*written.lock().unwrap(), b"one");
    assert!(
        state.active_job.is_some(),
        "a discard does not touch a setup job"
    );

    tx.send(Message::Abort(AbortScope::Setup)).unwrap();
    event_loop.drain_recv_channel(&mut state);
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    assert!(state.active_job.is_none());
    assert_eq!(
        *written.lock().unwrap(),
        b"one",
        "no further chunk after the abort"
    );
}

// ---- #1384 P3: the macOS line-clear payload as one job, paced by the tty mode ----

/// The pacing the line-clear payload asks for.
fn mac_pacing() -> Pacing {
    Pacing::GapIfCanonical {
        dur: Duration::from_millis(10),
        why: "test: canonical tty drops silently",
    }
}

/// A harness with a "tty" that takes `budget` bytes and then answers `WouldBlock`.
fn throttled_harness(mode: u8, budget: usize) -> (GateHarness, Arc<Mutex<usize>>) {
    let (mut pty, _) = FakePty::new(None);
    let budget = Arc::new(Mutex::new(budget));
    pty.writer.2 = Some(budget.clone());
    pty.mode.store(mode, Ordering::SeqCst);
    let mode = pty.mode.clone();
    let written = pty.writer.0.clone();
    (
        GateHarness {
            event_loop: new_event_loop(pty).expect("event loop"),
            mode,
            written,
        },
        budget,
    )
}

/// Raw / line-editor tty: no gap. Every chunk goes back to back, a full tty answers `WouldBlock`
/// and the rest is delivered, in order and in full, as the tty drains (POLLOUT), with no timer.
#[test]
fn line_clear_payload_in_raw_mode_has_no_gap_and_survives_wouldblock() {
    let payload: Vec<u8> = (0..4200u32).map(|i| (i % 251) as u8).collect();
    let chunks: Vec<Vec<u8>> = payload.chunks(1000).map(<[u8]>::to_vec).collect();
    let (mut h, budget) = throttled_harness(MODE_RAW, 1024);
    let mut state = State::default();
    state.queue_setup(chunks, mac_pacing());
    h.event_loop.refresh_reply_gate(&mut state);
    assert_eq!(state.pty_canonical, Some(false));

    let mut can_write = true;
    let mut rounds = 0;
    while state.needs_write() {
        rounds += 1;
        assert!(rounds < 50, "never finished");
        h.pass_with(&mut state, &mut can_write);
        // No gap is ever armed: the poll deadline stays empty, only POLLOUT wakes the loop.
        assert_eq!(state.job_timeout(), None);
        if !can_write {
            // The tty drained 1 KB (the shell read): writable again.
            *budget.lock().unwrap() = 1024;
            can_write = true;
        }
    }
    assert_eq!(h.take_written(), payload, "every byte, in order, none lost");
    assert!(rounds > 1, "the fake tty did push back");
}

/// Canonical tty (cooked, no newline): the named gap stays, because the tty drops silently.
#[test]
fn line_clear_payload_in_canonical_mode_keeps_the_named_gap() {
    let (mut h, _) = throttled_harness(MODE_CANONICAL, usize::MAX);
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"A", b"B"]), mac_pacing());
    h.event_loop.refresh_reply_gate(&mut state);
    let mut can_write = true;
    h.pass_with(&mut state, &mut can_write);
    assert_eq!(h.take_written(), b"A", "the second chunk waits for the gap");
    let wait = state.job_timeout().expect("a gap is armed");
    assert!(wait <= Duration::from_millis(10), "{wait:?}");
    assert!(matches!(
        state.active_job.as_ref().unwrap().pacing(),
        Pacing::Gap { dur, .. } if dur == Duration::from_millis(10)
    ));
}

/// A tty whose mode cannot be read is treated as canonical: the safe side.
#[test]
fn line_clear_payload_with_unknown_mode_keeps_the_gap() {
    let (mut h, _) = throttled_harness(MODE_UNKNOWN, usize::MAX);
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"A", b"B"]), mac_pacing());
    h.event_loop.refresh_reply_gate(&mut state);
    let mut can_write = true;
    h.pass_with(&mut state, &mut can_write);
    assert_eq!(h.take_written(), b"A");
    assert!(state.job_timeout().is_some());
}

/// ^C aborts the line-clear job at a chunk boundary, in raw mode too (chunks are the boundaries).
#[test]
fn ctrl_c_aborts_the_line_clear_job_at_a_chunk_boundary() {
    let (tx, rx) = mio_channel::channel::<Message>();
    let (mut pty, _) = FakePty::new(None);
    pty.mode.store(MODE_RAW, Ordering::SeqCst);
    let budget = Arc::new(Mutex::new(1000usize));
    pty.writer.2 = Some(budget.clone());
    let written = pty.writer.0.clone();
    let mut event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");
    let mut state = State::default();
    let chunks: Vec<Vec<u8>> = (0..4).map(|i| vec![b'a' + i; 1000]).collect();
    tx.send(Message::SetupChunked {
        chunks,
        pacing: mac_pacing(),
    })
    .unwrap();
    event_loop.drain_recv_channel(&mut state);
    event_loop.refresh_reply_gate(&mut state);
    let mut can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    assert_eq!(written.lock().unwrap().len(), 1000, "first chunk in full");
    assert!(!can_write, "then the tty is full");

    tx.send(Message::Input(Cow::Borrowed(&[0x03]))).unwrap();
    event_loop.drain_recv_channel(&mut state);
    *budget.lock().unwrap() = usize::MAX;
    can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    let out = written.lock().unwrap().clone();
    assert_eq!(
        out.len(),
        1001,
        "chunk 1 and the ^C, nothing of 2..4: {}",
        out.len()
    );
    assert_eq!(out[1000], 0x03);
    assert!(state.active_job.is_none());
}

/// A user key typed during the line-clear job waits for the whole job.
#[test]
fn a_key_during_the_line_clear_job_waits_behind_all_of_it() {
    let (mut h, _) = throttled_harness(MODE_RAW, usize::MAX);
    let mut state = State::default();
    state.queue_setup(setup_chunks(&[b"A", b"B", b"C"]), mac_pacing());
    state.queue_input(Cow::Borrowed(b"k"));
    h.event_loop.refresh_reply_gate(&mut state);
    let mut can_write = true;
    h.pass_with(&mut state, &mut can_write);
    assert_eq!(h.take_written(), b"ABCk");
}

// ---- #1384 P4: paste and agent writes are their own job classes ----

/// `Abort(Agent)` removes only the agent's unwritten input, and the agent write in flight is cut
/// where it stands; typed keys, pastes, commands and replies keep their order. A ^C's discard
/// (`Discardable`) never takes agent input.
#[test]
fn agent_abort_cuts_only_agent_jobs() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"key1"));
    state.queue_agent_input(Cow::Borrowed(b"agent1"));
    state.queue_paste_input(Cow::Owned(b"paste".to_vec()));
    state.queue_command_input(Cow::Borrowed(b"cmd\r"));
    state.queue_agent_input(Cow::Borrowed(b"agent2"));
    state.queue_response(b"\x1b[?62c".to_vec(), None);
    state.queue_input(Cow::Borrowed(b"key2"));

    // key1 is written; the agent's first write is in progress and half written.
    state.ensure_next();
    assert_eq!(state.take_current().unwrap().remaining_bytes(), b"key1");
    state.goto_next();
    let mut current = state.take_current().expect("agent1 is being written");
    assert_eq!(current.class, JobClass::Agent);
    current.advance(3);
    state.set_current(Some(current));

    state.abort(AbortScope::Agent);

    assert_eq!(
        drain(&mut state),
        vec![
            b"paste".to_vec(),
            b"cmd\r".to_vec(),
            b"\x1b[?62c".to_vec(),
            b"key2".to_vec()
        ]
    );
}

/// A ^C's discard drops the paste and the command but leaves the agent's input queued, as it did
/// when agent input travelled as ordinary input.
#[test]
fn discard_keeps_agent_jobs_and_takes_pastes() {
    let mut state = State::default();
    state.queue_agent_input(Cow::Borrowed(b"agent"));
    state.queue_paste_input(Cow::Owned(b"paste".to_vec()));
    state.queue_command_input(Cow::Borrowed(b"cmd\r"));
    state.abort(AbortScope::Discardable);
    assert_eq!(drain(&mut state), vec![b"agent".to_vec()]);
}

/// `Abort(Paste)` takes pastes only, not commands.
#[test]
fn paste_abort_takes_only_pastes() {
    let mut state = State::default();
    state.queue_paste_input(Cow::Owned(b"paste".to_vec()));
    state.queue_command_input(Cow::Borrowed(b"cmd\r"));
    state.queue_input(Cow::Borrowed(b"k"));
    state.abort(AbortScope::Paste);
    assert_eq!(drain(&mut state), vec![b"cmd\r".to_vec(), b"k".to_vec()]);
}

/// A paste cut inside its bracketed paste is closed with ESC[201~ before anything queued behind
/// it, whichever scope cut it.
#[test]
fn a_cut_paste_is_closed_with_the_bracketed_paste_end() {
    for scope in [AbortScope::Paste, AbortScope::Discardable] {
        let mut state = State::default();
        state.queue_paste_input(Cow::Owned(big_paste(2500)));
        state.queue_input(Cow::Borrowed(b"k"));
        state.ensure_next();
        let mut current = state.take_current().unwrap();
        assert_eq!(current.class, JobClass::Paste);
        current.advance(4096);
        state.set_current(Some(current));
        state.abort(scope);
        assert_eq!(
            drain(&mut state),
            vec![BRACKETED_PASTE_END.to_vec(), b"k".to_vec()],
            "{scope:?}"
        );
    }
    // Agent input that opens a bracketed paste (the agent's paste mode) is closed the same way.
    let mut state = State::default();
    state.queue_agent_input(Cow::Owned(big_paste(2500)));
    state.ensure_next();
    let mut current = state.take_current().unwrap();
    current.advance(4096);
    state.set_current(Some(current));
    state.abort(AbortScope::Agent);
    assert_eq!(drain(&mut state), vec![BRACKETED_PASTE_END.to_vec()]);
}

/// End to end over the channel with a tty that takes 4 KiB and then answers `WouldBlock`: a key
/// sent while a paste (then an agent write) is half written lands after the whole of each, never
/// inside, and the bytes arrive in submission order.
#[test]
fn user_keys_never_land_inside_a_paste_or_an_agent_write() {
    let (tx, rx) = mio_channel::channel::<Message>();
    let (mut pty, _) = FakePty::new(None);
    let budget = Arc::new(Mutex::new(4096usize));
    pty.writer.2 = Some(budget.clone());
    let written = pty.writer.0.clone();
    let mut event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");
    let mut state = State::default();
    let paste = big_paste(1500);
    let agent = big_paste(1400);
    tx.send(Message::PasteInput(Cow::Owned(paste.clone())))
        .unwrap();
    tx.send(Message::AgentInput(Cow::Owned(agent.clone())))
        .unwrap();
    event_loop.drain_recv_channel(&mut state);
    let mut can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    assert!(!can_write, "the tty is full");
    assert_eq!(written.lock().unwrap().len(), 4096);

    tx.send(Message::Input(Cow::Borrowed(b"k"))).unwrap();
    event_loop.drain_recv_channel(&mut state);
    *budget.lock().unwrap() = usize::MAX;
    can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();

    let mut expected = paste;
    expected.extend_from_slice(&agent);
    expected.push(b'k');
    assert_eq!(*written.lock().unwrap(), expected);
}

/// `Message::Abort(Agent)` through the loop: the agent input still waiting behind a half-written
/// paste is dropped; the paste and the key finish.
#[test]
fn agent_abort_message_through_the_loop() {
    let (tx, rx) = mio_channel::channel::<Message>();
    let (mut pty, _) = FakePty::new(None);
    let budget = Arc::new(Mutex::new(4096usize));
    pty.writer.2 = Some(budget.clone());
    let written = pty.writer.0.clone();
    let mut event_loop = EventLoop::new(
        Arc::new(FairMutex::new(TerminalModel::mock(None, None))),
        ChannelEventListener::new_for_test(),
        pty,
        rx,
    )
    .expect("event loop");
    let mut state = State::default();
    let paste = big_paste(1500);
    tx.send(Message::PasteInput(Cow::Owned(paste.clone())))
        .unwrap();
    tx.send(Message::AgentInput(Cow::Borrowed(b"agent")))
        .unwrap();
    tx.send(Message::Input(Cow::Borrowed(b"k"))).unwrap();
    event_loop.drain_recv_channel(&mut state);
    let mut can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    tx.send(Message::Abort(AbortScope::Agent)).unwrap();
    event_loop.drain_recv_channel(&mut state);
    *budget.lock().unwrap() = usize::MAX;
    can_write = true;
    event_loop.pty_write(&mut state, &mut can_write).unwrap();
    let mut expected = paste;
    expected.push(b'k');
    assert_eq!(*written.lock().unwrap(), expected);
}

fn coalescing_state() -> State {
    State {
        coalesce_keys: true,
        ..State::default()
    }
}

/// Keys that queued up behind a write in flight go out as ONE write, in typed order.
#[test]
fn adjacent_plain_keys_are_merged_into_one_write_in_order() {
    let mut state = coalescing_state();
    for k in [&b"e"[..], b"c", b"h", b"o", b" ", b"h", b"i"] {
        state.queue_input(Cow::Owned(k.to_vec()));
    }
    assert_eq!(drain(&mut state), vec![b"echo hi".to_vec()]);
}

/// Without the flag (every unix pty) nothing is merged: one write per job, as before.
#[test]
fn keys_are_not_merged_when_the_pty_does_not_coalesce() {
    let mut state = State::default();
    state.queue_input(Cow::Borrowed(b"a"));
    state.queue_input(Cow::Borrowed(b"b"));
    assert_eq!(drain(&mut state), vec![b"a".to_vec(), b"b".to_vec()]);
}

/// A merge never crosses a reply, a command, a paste, an agent write, an escape sequence, a
/// signal byte or a control byte: the concatenation of the writes is exactly the queue order, and
/// each of those stays its own write.
#[test]
fn coalescing_never_moves_or_merges_anything_but_adjacent_plain_text() {
    let mut state = coalescing_state();
    state.queue_input(Cow::Borrowed(b"ab"));
    state.queue_input(Cow::Borrowed(b"cd"));
    state.queue_input(Cow::Borrowed(b"\x1b[A"));
    state.queue_input(Cow::Borrowed(b"e"));
    state.queue_input(Cow::Borrowed(b"\x1b"));
    state.queue_input(Cow::Borrowed(b"f"));
    state.queue_command_input(Cow::Borrowed(b"g\r"));
    state.queue_input(Cow::Borrowed(b"h"));
    state.queue_response(b"\x1b[0n".to_vec(), None);
    state.queue_input(Cow::Borrowed(b"i"));
    state.queue_input(Cow::Borrowed(b"\x03"));
    state.queue_input(Cow::Borrowed(b"j"));
    state.queue_agent_input(Cow::Borrowed(b"k"));
    state.queue_input(Cow::Borrowed(b"l"));
    state.queue_paste_input(Cow::Borrowed(b"m"));
    state.queue_input(Cow::Borrowed(b"\r"));
    let writes = drain(&mut state);
    assert_eq!(
        writes,
        vec![
            b"abcd".to_vec(),
            b"\x1b[A".to_vec(),
            b"e".to_vec(),
            b"\x1b".to_vec(),
            b"f".to_vec(),
            b"g\r".to_vec(),
            b"h".to_vec(),
            b"\x1b[0n".to_vec(),
            b"i".to_vec(),
            b"\x03".to_vec(),
            b"j".to_vec(),
            b"k".to_vec(),
            b"l".to_vec(),
            b"m".to_vec(),
            b"\r".to_vec(),
        ]
    );
}

/// A merged write is capped, and the cap never splits or reorders: the rest follows in order.
#[test]
fn a_coalesced_write_is_capped_and_the_rest_follows_in_order() {
    let mut state = coalescing_state();
    let key = vec![b'x'; 1000];
    for _ in 0..6 {
        state.queue_input(Cow::Owned(key.clone()));
    }
    let writes = drain(&mut state);
    assert_eq!(
        writes.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![4000, 2000]
    );
}

/// An abort still takes a queued command, and the keys around it still merge in order.
#[test]
fn an_abort_still_drops_a_queued_command_after_coalescing() {
    let mut state = coalescing_state();
    state.queue_input(Cow::Borrowed(b"a"));
    state.queue_command_input(Cow::Borrowed(b"cmd\r"));
    state.abort(AbortScope::Discardable);
    state.queue_input(Cow::Borrowed(b"b"));
    assert_eq!(drain(&mut state), vec![b"ab".to_vec()]);
}

/// The overlapped-pipe fake through the loop: while a write is in flight every later key is
/// `WouldBlock`; on the completion event all keys that piled up go out as ONE write, nothing is
/// lost, duplicated or reordered, and the kill-buffer chord stays its own write.
#[test]
fn keys_piled_up_behind_an_in_flight_write_go_out_as_one_write_on_completion() {
    let (mut pty, _) = FakePty::new(None);
    let in_flight = Arc::new(AtomicBool::new(false));
    pty.writer.3 = Some(in_flight.clone());
    let writes = Arc::new(Mutex::new(Vec::<usize>::new()));
    let written = pty.writer.0.clone();
    let mode = pty.mode.clone();
    let mut h = GateHarness {
        event_loop: new_event_loop(pty).expect("event loop"),
        mode,
        written: written.clone(),
    };
    let mut state = coalescing_state();
    let mut can_write = true;

    state.queue_input(Cow::Borrowed(b"a"));
    h.pass_with(&mut state, &mut can_write);
    assert_eq!(h.take_written(), b"a".to_vec());
    assert!(in_flight.load(Ordering::SeqCst));

    // Ten keys arrive while "a" is in flight.
    for k in b"bcdefghijk" {
        state.queue_input(Cow::Owned(vec![*k]));
    }
    state.queue_input(Cow::Borrowed(b"\x1b2"));
    state.queue_command_input(Cow::Borrowed(b"echo\r"));
    h.pass_with(&mut state, &mut can_write);
    assert!(!can_write, "the in-flight write pushed back");
    assert!(h.take_written().is_empty());

    // Completion event: the pipe is writable again. One write for the keys, then the chord, then
    // the command, each after the previous one's completion.
    for expected in [&b"bcdefghijk"[..], b"\x1b2", b"echo\r"] {
        in_flight.store(false, Ordering::SeqCst);
        can_write = true;
        h.pass_with(&mut state, &mut can_write);
        writes.lock().unwrap().push(h.take_written().len());
        assert_eq!(writes.lock().unwrap().last().copied(), Some(expected.len()));
    }
    assert_eq!(*writes.lock().unwrap(), vec![10, 2, 5]);
}
