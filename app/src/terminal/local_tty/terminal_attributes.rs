use std::{os::fd::RawFd, time::Duration};

use nix::Result;
use nix::sys::termios::{self, Termios};
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

    /// The pty's current termios, read now (a cheap ioctl, no timer). Used by event-driven
    /// checks that need the raw/cooked state at the moment output arrived.
    pub fn current_attributes(&self) -> Option<Termios> {
        fetch_termial_attributes(self.fd).ok()
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

/// Who owns the terminal right now, for the interactive latch (#1373). `Input`: the tty is raw
/// (non-canonical) or reading a password (canonical, no echo), so a program is definitely reading
/// keys. `Program`: a process group other than the shell's is the foreground group (an `ssh`, a
/// compiler, `sleep`). `Shell`: neither (a builtin or function, or the shell between commands).
/// Any failure reads as `Shell`: the latch then waits for a better signal rather than guessing.
pub fn terminal_owner(
    fd: RawFd,
    shell_pid: i32,
) -> crate::terminal::model::block::interactive_latch::TerminalOwner {
    use crate::terminal::model::block::interactive_latch::TerminalOwner;
    if let Ok(termios) = fetch_termial_attributes(fd) {
        if !termios.local_flags.contains(termios::LocalFlags::ICANON)
            || is_canonical_without_echo(termios.local_flags)
        {
            return TerminalOwner::Input;
        }
    }
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid > 0 && pgid != shell_pid {
        TerminalOwner::Program
    } else if shell_is_blocked(shell_pid) {
        // The shell is the foreground group yet asleep: a builtin waiting on the tty (`read x`).
        // A shell computing in a function is running, not asleep, and stays `Shell`.
        TerminalOwner::ShellBlocked
    } else {
        TerminalOwner::Shell
    }
}

/// Whether the shell process is asleep in the kernel. Linux: `/proc/<pid>/stat` state `S`.
/// macOS: `proc_pidinfo(PROC_PIDTBSDINFO)` status `SSLEEP`. Any failure or other platform is
/// `false` (the latch then waits for a better signal).
fn shell_is_blocked(shell_pid: i32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string(format!("/proc/{shell_pid}/stat"))
            .ok()
            .and_then(|stat| parse_proc_stat_state(&stat))
            == Some('S')
    }
    #[cfg(target_os = "macos")]
    {
        // `SSLEEP` from <sys/proc.h>.
        const SSLEEP: u32 = 3;
        // SAFETY: zeroed is a valid bit pattern for this plain C struct; `proc_pidinfo` writes at
        // most `size_of` bytes into it.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let written = unsafe {
            libc::proc_pidinfo(
                shell_pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast::<libc::c_void>(),
                size,
            )
        };
        written == size && info.pbi_status == SSLEEP
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = shell_pid;
        false
    }
}

/// The process state letter from `/proc/<pid>/stat` (`pid (comm) S ...`).
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_proc_stat_state(stat: &str) -> Option<char> {
    let close = stat.rfind(')')?;
    stat[close + 1..].split_whitespace().next()?.chars().next()
}

/// Whether the pty's foreground process group provably cannot hold a shell at its prompt, local
/// or remote: it is not the shell's own group (`shell_pid`, a session leader) and none of its
/// processes is a shell or a program that hands the terminal to one (a login hop such as `ssh`,
/// `sudo -i`, `su`, `ksu`, a container or multiplexer). Over ssh the foreground is `ssh` while the
/// REMOTE shell may be at its prompt, so a name check, not just "not the shell", decides. Any
/// failure or unsupported platform is `false`: callers keep the safe behaviour.
pub fn foreground_is_plain_program(fd: RawFd, shell_pid: i32) -> bool {
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 || pgid == shell_pid {
        return false;
    }
    process_group_names(pgid)
        .is_some_and(|names| names.iter().all(|name| !is_shell_or_login_hop(name)))
}

/// Whether the shell is provably not about to read a line at a prompt, so a young Enter is not
/// the #933 race (a line typed at a prompt the app has not noticed yet): either a plain program
/// owns the foreground (see [`foreground_is_plain_program`]), or the tty is canonical with echo
/// off, which is a program reading a password (`sudo`, `su`, `ksu`, `ssh`, `read -s`, in the
/// top-level shell or a Phosphorized subshell). A shell at its prompt is never in that state:
/// readline/zle run the tty non-canonical, and a shell between commands has restored echo.
/// Over ssh the local tty is raw (non-canonical), so a remote prompt is never mistaken for one.
pub fn shell_provably_not_at_prompt(fd: RawFd, shell_pid: i32) -> bool {
    foreground_is_plain_program(fd, shell_pid)
        || fetch_termial_attributes(fd)
            .is_ok_and(|termios| is_canonical_without_echo(termios.local_flags))
}

/// What the pty says about the shell and its prompt right now (#1353 phase 6), the sample the
/// typeahead hand-off decides a young Enter on instead of an elapsed-time window.
///
/// `NotAtPrompt` is [`shell_provably_not_at_prompt`]. `LineEditorReading`: the shell's own process
/// group owns the foreground and the tty is raw (non-canonical), which is the state readline, zle,
/// fish and reedline put it in while they read a line. A shell running a command has restored the
/// cooked tty and a program in the foreground is not the shell's group, so neither looks like
/// this. Under bash and zsh a line editor that reads has already had its prompt's Precmd written,
/// so that Precmd is in the pty stream and will be parsed; fish sets raw BEFORE it fires
/// `fish_prompt`, so there it may not be (#1392 installs its drained-stream veto for bash and zsh
/// only). `ShellCooked` (#1614): the shell's own group owns the foreground with a canonical tty:
/// between a command's end and its line editor (`PROMPT_COMMAND`, PS1 substitutions), or a cooked
/// builtin such as `read x`. Anything else, including a failed query and a local tty in front of
/// an `ssh` or an inner shell (another process group), is `Unknown`.
pub fn shell_prompt_fact(
    fd: RawFd,
    shell_pid: i32,
) -> crate::terminal::model::typeahead_handoff::ShellPromptFact {
    use crate::terminal::model::typeahead_handoff::ShellPromptFact;
    if shell_provably_not_at_prompt(fd, shell_pid) {
        return ShellPromptFact::NotAtPrompt;
    }
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 || pgid != shell_pid {
        return ShellPromptFact::Unknown;
    }
    match fetch_termial_attributes(fd) {
        Ok(termios) if !termios.local_flags.contains(termios::LocalFlags::ICANON) => {
            ShellPromptFact::LineEditorReading
        }
        Ok(_) => ShellPromptFact::ShellCooked,
        Err(_) => ShellPromptFact::Unknown,
    }
}

/// Whether the pty master `fd` holds output the reader has not taken yet (#1392), for the stream
/// barrier ([`crate::terminal::model::pty_stream_barrier::PtyStreamBarrier::drained_with`]).
/// `Some(false)` only when the platform proves it empty; `None` when it cannot tell (an error, a
/// hang-up, an unsupported platform), which the barrier treats as "not drained".
///
/// A zero-timeout `poll` first: it waits for nothing (no timer), and on Linux `n_tty_poll` flushes
/// the pty's pending flip-buffer work into the line discipline before it answers, so bytes a slave
/// `write` has already returned for are seen even if the kernel has not pushed them yet. Then the
/// byte count, read in the direction the master reads:
/// - Linux: `FIONREAD` (`TIOCINQ`), the master's line-discipline read count.
/// - macOS: `TIOCOUTQ`; master and slave share one tty there and the master reads the slave's
///   output queue (`FIONREAD` on the master counts the other direction). Unlike `poll`, it also
///   counts output held back by `^S` flow control.
/// - Anything else: `None`, so the barrier never says drained and today's behaviour stays.
pub fn pty_master_has_unread(fd: RawFd) -> Option<bool> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid `pollfd`, a zero timeout; `poll` only writes `revents`.
        let polled = unsafe { libc::poll(&mut pollfd, 1, 0) };
        if polled < 0 || pollfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return None;
        }
        if pollfd.revents & libc::POLLIN != 0 {
            return Some(true);
        }
        #[cfg(target_os = "linux")]
        let request = libc::FIONREAD;
        #[cfg(target_os = "macos")]
        let request = libc::TIOCOUTQ;
        let mut pending: libc::c_int = 0;
        // SAFETY: both requests write one `int` through the pointer, which points at `pending`.
        let result = unsafe { libc::ioctl(fd, request, &mut pending as *mut libc::c_int) };
        if result != 0 {
            return None;
        }
        Some(pending != 0)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = fd;
        None
    }
}

pub(crate) fn is_canonical_without_echo(flags: termios::LocalFlags) -> bool {
    flags.contains(termios::LocalFlags::ICANON) && !flags.contains(termios::LocalFlags::ECHO)
}

/// Whether the tty is canonical with echo off right now: a program is reading a secret (`ssh`'s
/// password prompt, `sudo`, `read -s`). The termios guard of the ssh typeahead release (#1611,
/// #1612, #1621): the keys held before ssh owned the tty are discarded then, as the kernel's
/// `TCSAFLUSH` discards a plain terminal's typeahead, instead of being written into the password.
/// A failed query reads as `false` (the keys are delivered; nothing is guessed from text).
pub fn tty_is_reading_secret(fd: RawFd) -> bool {
    fetch_termial_attributes(fd).is_ok_and(|termios| is_canonical_without_echo(termios.local_flags))
}

/// Shells, and programs that put a shell (possibly on another machine) behind the pty.
pub(crate) fn is_shell_or_login_hop(comm: &str) -> bool {
    const NAMES: &[&str] = &[
        "bash",
        "zsh",
        "fish",
        "sh",
        "dash",
        "ash",
        "ksh",
        "mksh",
        "pdksh",
        "tcsh",
        "csh",
        "nu",
        "nushell",
        "pwsh",
        "powershell",
        "elvish",
        "xonsh",
        "ssh",
        "mosh",
        "mosh-client",
        "et",
        "sshpass",
        "telnet",
        "sudo",
        "su",
        "doas",
        "ksu",
        "login",
        "docker",
        "podman",
        "kubectl",
        "nsenter",
        "chroot",
        "screen",
        "tmux",
        "script",
        "expect",
        "wsl",
        "machinectl",
        "lxc",
        "incus",
    ];
    let name = comm.trim_start_matches('-').to_ascii_lowercase();
    NAMES.contains(&name.as_str())
}

/// Delivers SIGINT to the pty's foreground process group (what the tty driver does for a ^C
/// byte in cooked mode), regardless of the line discipline. A client that put the tty in raw
/// mode (ssh, once connected) receives a ^C byte as DATA and forwards it to the far side, but
/// still exits on the signal. Returns whether a signal was sent; never signals our own group.
pub fn sigint_foreground_group(fd: RawFd) -> bool {
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    // SAFETY: `getpgrp` has no preconditions.
    if pgid <= 1 || pgid == unsafe { libc::getpgrp() } {
        return false;
    }
    // SAFETY: plain signal delivery to a foreground group that is not ours.
    unsafe { libc::killpg(pgid, libc::SIGINT) == 0 }
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
