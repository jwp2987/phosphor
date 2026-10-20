//! Tests for PTY event loop setup (#882).

use std::sync::atomic::{AtomicBool, Ordering};

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
    writer: io::Sink,
}

impl FakePty {
    fn new(register_error: Option<i32>) -> (Self, Arc<AtomicBool>) {
        let killed = Arc::new(AtomicBool::new(false));
        (
            Self {
                register_error,
                killed: killed.clone(),
                reader: io::empty(),
                writer: io::sink(),
            },
            killed,
        )
    }
}

impl EventedReadWrite for FakePty {
    type Reader = io::Empty;
    type Writer = io::Sink;

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

    fn writer(&mut self) -> &mut io::Sink {
        &mut self.writer
    }

    fn write_token(&self) -> mio::Token {
        PTY_TOKEN
    }
}

impl local_tty::EventedPty for FakePty {
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
