//! Tests for PTY event loop setup (#882).

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::terminal::SizeInfo;
use crate::terminal::local_tty::{ChildEvent, EventedReadWrite, mio_channel};

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

/// #829: a reply to a terminal query that was produced while a command ran must
/// not be written after that command finished; the shell would read it back as
/// typed text (`62c`, `10;rgb:...`).
#[test]
fn stale_query_responses_are_dropped_once_the_command_finishes() {
    let mut state = State::default();
    state.queue_response(b"\x1b[?62;c".to_vec(), true);
    state.queue_response(b"\x1b[1;1R".to_vec(), false);

    state.drop_stale_responses(false);

    state.ensure_next();
    let writing = state.take_current().expect("prompt-time reply is kept");
    assert_eq!(writing.remaining_bytes(), b"\x1b[1;1R");
    assert!(!state.needs_write());
}

#[test]
fn query_responses_are_kept_while_the_command_is_still_running() {
    let mut state = State::default();
    state.queue_response(b"\x1b[?62;c".to_vec(), true);

    state.drop_stale_responses(true);

    assert!(state.needs_write());
}
