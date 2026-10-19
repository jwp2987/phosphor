use std::{os::fd::RawFd, time::Duration};

use nix::sys::termios::{self, Termios};
use nix::Result;
use warpui::{Entity, ModelContext};

/// The default amount of time we wait before polling the terminal attributes again.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// A model that polls the terminal attributes of a given tty via the termios interface
/// and produces an event whenever a query successfully completes.
/// The poller can be start and stopped.
pub struct TerminalAttributesPoller {
    /// The file descriptor identifying the terminal to query.
    fd: RawFd,
    /// Simple counter to ensure there is exactly one poll running at a time, eventually.
    poll_epoch: usize,
}

pub enum Event {
    TermiosQueryFinished { termios: Termios },
}

impl Entity for TerminalAttributesPoller {
    type Event = Event;
}

impl TerminalAttributesPoller {
    pub fn new(fd: RawFd) -> Self {
        Self { fd, poll_epoch: 0 }
    }

    /// If there is already a poll running, calling `start_polling`
    /// will effectively restart it, ultimately resulting in one single poll.
    pub fn start_polling(&mut self, ctx: &mut ModelContext<Self>) {
        self.poll_epoch += 1;
        self.poll_terminal_attributes(self.poll_epoch, ctx);
    }

    pub fn stop_polling(&mut self) {
        self.poll_epoch += 1;
    }

    /// The executable name of the pty's foreground process group leader, or `None` when it
    /// cannot be determined (callers must treat that as "unknown", never as a match).
    pub fn foreground_process_name(&self) -> Option<String> {
        foreground_process_name(self.fd)
    }

    fn poll_terminal_attributes(&mut self, poll_epoch: usize, ctx: &mut ModelContext<Self>) {
        if poll_epoch != self.poll_epoch {
            return;
        }

        // TODO: consider using Timer::interval() here to implement the interval-based polling.
        let fd = self.fd;
        ctx.spawn(
            async move {
                warpui::r#async::Timer::after(POLL_INTERVAL).await;
                fetch_termial_attributes(fd)
            },
            move |me, termios, ctx| {
                if let Ok(termios) = termios {
                    ctx.emit(Event::TermiosQueryFinished { termios });
                }
                me.poll_terminal_attributes(poll_epoch, ctx);
            },
        );
    }
}

/// Queries the terminal attributes via the `termios` API.
fn fetch_termial_attributes(fd: RawFd) -> Result<Termios> {
    termios::tcgetattr(fd)
}

/// The executable name of the foreground process group leader of the pty whose master is
/// `fd`: `tcgetpgrp` on the master, then `/proc/<pid>/comm` on Linux or `proc_pidpath` on
/// macOS. `None` on any failure -- a pty with no foreground group, a process that exited
/// between the two calls, an unsupported platform.
pub fn foreground_process_name(fd: RawFd) -> Option<String> {
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 {
        return None;
    }
    process_name(pgid)
}

#[cfg(target_os = "linux")]
fn process_name(pid: libc::pid_t) -> Option<String> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let comm = comm.trim_end_matches('\n');
    (!comm.is_empty()).then(|| comm.to_owned())
}

#[cfg(target_os = "macos")]
fn process_name(pid: libc::pid_t) -> Option<String> {
    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `buffer` is writable for `buffer.len()` bytes, which is what is passed.
    let written = unsafe {
        libc::proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast::<libc::c_void>(),
            buffer.len() as u32,
        )
    };
    if written <= 0 {
        return None;
    }
    let path = std::str::from_utf8(&buffer[..written as usize]).ok()?;
    path.rsplit('/').next().map(str::to_owned)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_name(_pid: libc::pid_t) -> Option<String> {
    None
}
