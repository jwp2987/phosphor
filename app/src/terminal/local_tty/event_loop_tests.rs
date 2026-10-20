//! Tests for PTY event loop setup (#882).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use super::*;
use crate::terminal::SizeInfo;
use crate::terminal::local_tty::{ChildEvent, EventedReadWrite, mio_channel};
use crate::terminal::model::reply_ledger::ReplyLedger;

/// A PTY stand-in whose registration can be made to fail, recording whether
/// the event loop killed it.
struct FakePty {
    register_error: Option<i32>,
    killed: Arc<AtomicBool>,
    reader: io::Empty,
    writer: CaptureWriter,
    /// What `is_canonical` reports: 0 = cannot tell, 1 = raw, 2 = canonical.
    mode: Arc<AtomicU8>,
}

/// A pty writer that records what the event loop writes.
#[derive(Clone, Default)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
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
                reader: io::empty(),
                writer: CaptureWriter::default(),
                mode: Arc::new(AtomicU8::new(MODE_UNKNOWN)),
            },
            killed,
        )
    }
}

impl EventedReadWrite for FakePty {
    type Reader = io::Empty;
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

    fn reader(&mut self) -> &mut io::Empty {
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

    fn on_resize(&mut self, _: &SizeInfo) {}

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
            bytes: Cow::Borrowed(b"R"),
            produced_while_executing: Some(block("cmd")),
            is_reply: true,
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
