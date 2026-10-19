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

    /// The executable names of every process in the pty's foreground process group, or
    /// `None` when they cannot be determined (callers must treat that as "unknown", never as
    /// a match).
    pub fn foreground_process_names(&self) -> Option<Vec<String>> {
        foreground_process_names(self.fd)
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

/// The executable names of every process in the foreground process group of the pty whose
/// master is `fd`. A pager is almost never the group leader: `git log` and `man` are, and
/// `less` is a member of their group, so the leader alone cannot identify it.
///
/// Linux scans `/proc/*/stat` for processes whose group (field 5) is `tcgetpgrp`'s answer.
/// macOS has no cheap equivalent here and reports the leader only (`proc_pidpath`); callers
/// then see no pager and hand the command to the user. `None` on any failure -- a pty with no
/// foreground group, an unsupported platform, no readable process.
pub fn foreground_process_names(fd: RawFd) -> Option<Vec<String>> {
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 {
        return None;
    }
    process_group_names(pgid)
}

/// Parses the `comm` and process group out of `/proc/<pid>/stat`: `pid (comm) S ppid pgrp
/// ...`. `comm` may itself contain spaces and parentheses, so it is delimited by the first
/// `(` and the LAST `)`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_proc_stat(stat: &str) -> Option<(String, i32)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = &stat[open + 1..close];
    let mut rest = stat[close + 1..].split_whitespace();
    let _state = rest.next()?;
    let _ppid = rest.next()?;
    let pgrp: i32 = rest.next()?.parse().ok()?;
    Some((comm.to_owned(), pgrp))
}

#[cfg(target_os = "linux")]
fn process_group_names(pgid: libc::pid_t) -> Option<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir("/proc").ok()? {
        let Ok(entry) = entry else { continue };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.is_empty() || !file_name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        // The process may exit between the listing and the read; that is not a failure.
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        if let Some((comm, pgrp)) = parse_proc_stat(&stat)
            && pgrp == pgid
            && !comm.is_empty()
        {
            names.push(comm);
        }
    }
    (!names.is_empty()).then_some(names)
}

#[cfg(target_os = "macos")]
fn process_group_names(pgid: libc::pid_t) -> Option<Vec<String>> {
    // The group leader only; see `foreground_process_names`.
    process_name(pgid).map(|name| vec![name])
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
fn process_group_names(_pgid: libc::pid_t) -> Option<Vec<String>> {
    None
}

#[cfg(test)]
#[path = "terminal_attributes_tests.rs"]
mod tests;
