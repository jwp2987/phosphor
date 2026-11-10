use std::{
    os::fd::{BorrowedFd, RawFd},
    time::Duration,
};

use nix::Result;
use nix::sys::termios::{self, Termios};
use warpui::{Entity, ModelContext};

use crate::terminal::model::block::ForegroundTtyRead;

/// The default amount of time we wait before polling the terminal attributes again.
// sampling: 1 Hz termios sample; the sample feeds the prompt detector, which hands over only on a password termios state or a Reading probe
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

    /// Whether ANY process of the pty's foreground process group is blocked reading the tty
    /// right now (#1796), for a prompt-shaped line that only ends in `?`/`:`. See
    /// [`foreground_group_tty_read`]. Asked only once such a line has been quiet for its whole
    /// window, then at most once a second.
    pub fn foreground_tty_read(&self) -> ForegroundTtyRead {
        foreground_group_tty_read(self.fd)
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
    tcgetattr_raw(fd)
}

/// `tcgetattr` on a raw fd. nix >= 0.27 takes an `AsFd` (I/O safety), so the raw descriptor is
/// borrowed for the one call. A negative fd is `EBADF` (`BorrowedFd::borrow_raw(-1)` is not
/// allowed), matching what the kernel answers for it.
pub(super) fn tcgetattr_raw(fd: RawFd) -> Result<Termios> {
    if fd < 0 {
        return Err(nix::errno::Errno::EBADF);
    }
    // SAFETY: the borrow does not outlive this call, and the caller owns `fd` for its duration.
    let fd = unsafe { BorrowedFd::borrow_raw(fd) };
    termios::tcgetattr(fd)
}

/// The executable names of every process in the foreground process group of the pty whose
/// master is `fd`. A pager is almost never the group leader: `git log` and `man` are, and
/// `less` is a member of their group, so the leader alone cannot identify it.
///
/// Linux finds the processes whose group (field 5 of `/proc/<pid>/stat`) is `tcgetpgrp`'s answer
/// by walking the shell's own process tree (#1958, `fg_group`; never a read of every process's
/// `stat`). macOS has no cheap equivalent here and reports the leader only (`proc_pidpath`);
/// callers then see no pager and hand the command to the user. `None` on any failure -- a pty
/// with no foreground group, an unsupported platform, no readable process.
pub fn foreground_process_names(fd: RawFd) -> Option<Vec<String>> {
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        let names: Vec<String> = process_group_members(fd, pgid)?
            .into_iter()
            .map(|(_pid, comm)| comm)
            .collect();
        (!names.is_empty()).then_some(names)
    }
    #[cfg(not(target_os = "linux"))]
    {
        process_group_names(pgid)
    }
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
    #[cfg(target_os = "linux")]
    {
        // #1958: the same verdict as "no member's name is a shell or login hop", found without
        // reading every process's `stat`: the leader's name first (one read decides for `ssh`,
        // an inner shell, `sudo -i`), then the members from the shell's own process tree.
        let started = warpui::input_order::enabled().then(std::time::Instant::now);
        let lookup = super::fg_group::group_is_plain_program(
            &super::fg_group::RealProc,
            super::fg_group::lookup_mode(),
            session_leader(fd),
            pgid,
        );
        log_group_lookup(
            started,
            "plain_program",
            lookup.path,
            lookup.reads,
            lookup.members,
        );
        lookup.plain
    }
    #[cfg(not(target_os = "linux"))]
    {
        process_group_names(pgid)
            .is_some_and(|names| names.iter().all(|name| !is_shell_or_login_hop(name)))
    }
}

/// The pty's session leader (the shell the pty was opened for): the root of the #1958 walk for the
/// foreground group's members. `tcgetsid` answers on the master as `tcgetpgrp` does; `None` when it
/// does not (the members are then listed, not walked).
#[cfg(target_os = "linux")]
fn session_leader(fd: RawFd) -> Option<libc::pid_t> {
    // SAFETY: `tcgetsid` only reads the terminal's session id.
    let session = unsafe { libc::tcgetsid(fd) };
    (session > 0).then_some(session)
}

/// #1958: one `input_order hop=fg_group_lookup` line per look at the foreground group's members
/// (only with input-order logging on, which is where `started` comes from): who asked, which way
/// answered (`leader`, `walk`, `list`, `list_after_walk`, `scan`), how many `/proc` reads it took,
/// how many members it found and how long it took, so a latency run shows what a sample costs.
/// Counts and times only, never a process name.
#[cfg(target_os = "linux")]
fn log_group_lookup(
    started: Option<std::time::Instant>,
    asked_by: &str,
    path: super::fg_group::LookupPath,
    reads: usize,
    members: Option<usize>,
) {
    let Some(started) = started else {
        return;
    };
    let micros = started.elapsed().as_micros();
    warpui::input_order::log_hop("fg_group_lookup", || {
        format!(
            "for={asked_by} path={} reads={reads} members={} us={micros}",
            path.as_str(),
            members.map_or_else(|| "none".to_string(), |count| count.to_string()),
        )
    });
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

/// #1641: whether the pty's foreground process group is blocked READING THE TTY right now, an OS
/// fact and never screen text. Under the ssh wrapper's hold this is ssh asking something that is
/// not a secret (a host-key yes/no, a Duo "Passcode or option", a PAM question, an OTP: canonical
/// with echo on), so the answer is passed to it like a password. The group leader is the process
/// asked: the wrapper runs `command ssh` as its own job, so ssh is the leader. Linux reads
/// `/proc/<pid>/wchan` and, where that says nothing conclusive (`wait_woken`, `0`, unreadable),
/// the process state plus `/proc/<pid>/syscall` (a `read` on an fd that is a tty). Other
/// platforms (macOS has no wait message it reports to user space through `libc`; ConPTY) answer
/// `false`: no detection, the hold is unchanged. Cheap: three small reads of the leader's files.
pub fn foreground_reads_tty(fd: RawFd) -> bool {
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 {
        return false;
    }
    process_reads_tty(pgid)
}

/// #1641: whether [`foreground_reads_tty`] can answer here: Linux with a readable `/proc` and a
/// known `read` syscall number. Elsewhere it is not installed, and the managed-ssh hold keeps its
/// silence ceiling.
pub fn tty_read_probe_works() -> bool {
    #[cfg(target_os = "linux")]
    {
        READ_SYSCALL.is_some() && std::fs::metadata("/proc/self/stat").is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// #1796: whether any process -- any THREAD of any process -- in the pty's foreground process
/// group is blocked reading the tty. The leader alone is not enough: under `make`, `npm run`, a
/// script that runs a child, or `( ... )` the process at the prompt is a member, not the leader,
/// and a Go or Java program reads stdin on a thread other than its main one. `Reading` as soon as
/// one task is; `Unknown` when the platform has no probe ([`tty_read_probe_works`]), `/proc` cannot
/// be listed, a sleeping task's `syscall` or fd link cannot be read (ptrace restrictions such as
/// Yama `ptrace_scope` >= 2, a member owned by another user, e.g. root under `sudo`), or the walk
/// hit its task cap; otherwise `NotReading`. The group's members as [`foreground_process_names`]
/// finds them (#1958: the shell's process tree, not every process on the machine), plus three
/// small reads per sleeping task of the group; the
/// #1641 per-key ssh probe ([`foreground_reads_tty`]) stays on the leader alone because it runs
/// on the key path.
pub fn foreground_group_tty_read(fd: RawFd) -> ForegroundTtyRead {
    if !tty_read_probe_works() {
        return ForegroundTtyRead::Unknown;
    }
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 {
        return ForegroundTtyRead::Unknown;
    }
    group_tty_read(fd, pgid)
}

/// The most tasks (threads, across the whole group) [`foreground_group_tty_read`] inspects; past
/// it the answer is `Unknown` unless a reading task was already found.
#[cfg(target_os = "linux")]
const MAX_GROUP_TASKS: usize = 512;

#[cfg(target_os = "linux")]
fn group_tty_read(fd: RawFd, pgid: libc::pid_t) -> ForegroundTtyRead {
    let Some(members) = process_group_members(fd, pgid) else {
        return ForegroundTtyRead::Unknown;
    };
    let mut tasks = 0usize;
    let mut verdicts = Vec::new();
    for (pid, _comm) in members {
        let base = format!("/proc/{pid}");
        let task_dirs: Vec<String> = match std::fs::read_dir(format!("{base}/task")) {
            Ok(dir) => dir
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
                .filter(|tid| !tid.is_empty() && tid.bytes().all(|b| b.is_ascii_digit()))
                .map(|tid| format!("{base}/task/{tid}"))
                .collect(),
            // No task directory (the process just exited, or an odd /proc): ask the process.
            Err(_) => vec![base],
        };
        for task in task_dirs {
            if tasks >= MAX_GROUP_TASKS {
                verdicts.push(ForegroundTtyRead::Unknown);
                return combine_tty_reads(verdicts);
            }
            tasks += 1;
            let verdict = task_tty_read(&task);
            if verdict == ForegroundTtyRead::Reading {
                return verdict;
            }
            verdicts.push(verdict);
        }
    }
    combine_tty_reads(verdicts)
}

#[cfg(not(target_os = "linux"))]
fn group_tty_read(_fd: RawFd, _pgid: libc::pid_t) -> ForegroundTtyRead {
    ForegroundTtyRead::Unknown
}

/// One task's answer, from its `/proc/<pid>[/task/<tid>]` directory.
#[cfg(target_os = "linux")]
fn task_tty_read(base: &str) -> ForegroundTtyRead {
    let Some(read_nr) = READ_SYSCALL else {
        return ForegroundTtyRead::Unknown;
    };
    let wchan = std::fs::read_to_string(format!("{base}/wchan")).ok();
    let state = std::fs::read_to_string(format!("{base}/stat"))
        .ok()
        .and_then(|stat| parse_proc_stat_state(&stat));
    let asks_syscall = state == Some('S') && !wchan.as_deref().is_some_and(wchan_is_tty_read);
    let syscall = asks_syscall
        .then(|| std::fs::read_to_string(format!("{base}/syscall")).ok())
        .flatten();
    task_tty_read_verdict(
        wchan.as_deref(),
        state,
        syscall.as_deref(),
        read_nr,
        |read_fd| {
            std::fs::read_link(format!("{base}/fd/{read_fd}"))
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        },
    )
}

/// The #1796 decision for one task from what could be read. `Reading`: the line discipline's
/// read in `wchan`, or asleep (`S`) in a `read` whose fd is a terminal. `NotReading`: gone (no
/// state), not asleep (running, stopped, a zombie, uninterruptible I/O), asleep in another
/// syscall, or reading something that is not a terminal. `Unknown`: asleep, but the `syscall` line
/// (or the fd link of its `read`) could not be read -- the kernel refuses both without ptrace
/// access, which is exactly a root-owned member or a Yama-restricted system.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn task_tty_read_verdict(
    wchan: Option<&str>,
    state: Option<char>,
    syscall: Option<&str>,
    read_nr: u64,
    read_fd_target: impl FnOnce(i64) -> Option<String>,
) -> ForegroundTtyRead {
    if wchan.is_some_and(wchan_is_tty_read) {
        return ForegroundTtyRead::Reading;
    }
    if state != Some('S') {
        return ForegroundTtyRead::NotReading;
    }
    let Some(syscall) = syscall.map(str::trim).filter(|line| !line.is_empty()) else {
        return ForegroundTtyRead::Unknown;
    };
    let Some(read_fd) = syscall_read_fd(syscall, read_nr) else {
        return ForegroundTtyRead::NotReading;
    };
    match read_fd_target(read_fd) {
        Some(target) if path_is_tty(&target) => ForegroundTtyRead::Reading,
        Some(_) => ForegroundTtyRead::NotReading,
        None => ForegroundTtyRead::Unknown,
    }
}

/// Several tasks' answers: `Reading` if any is, else `Unknown` if any is, else `NotReading` (also
/// for no tasks at all: an exited group reads nothing).
#[cfg(any(target_os = "linux", test))]
pub(crate) fn combine_tty_reads(
    verdicts: impl IntoIterator<Item = ForegroundTtyRead>,
) -> ForegroundTtyRead {
    let mut combined = ForegroundTtyRead::NotReading;
    for verdict in verdicts {
        match verdict {
            ForegroundTtyRead::Reading => return ForegroundTtyRead::Reading,
            ForegroundTtyRead::Unknown => combined = ForegroundTtyRead::Unknown,
            ForegroundTtyRead::NotReading => {}
        }
    }
    combined
}

#[cfg(target_os = "linux")]
fn process_reads_tty(pid: libc::pid_t) -> bool {
    let base = format!("/proc/{pid}");
    let wchan = std::fs::read_to_string(format!("{base}/wchan")).ok();
    if wchan.as_deref().is_some_and(wchan_is_tty_read) {
        return true;
    }
    let state = std::fs::read_to_string(format!("{base}/stat"))
        .ok()
        .and_then(|stat| parse_proc_stat_state(&stat));
    if state != Some('S') {
        return false;
    }
    let Some(read_nr) = READ_SYSCALL else {
        return false;
    };
    let Some(read_fd) = std::fs::read_to_string(format!("{base}/syscall"))
        .ok()
        .and_then(|line| syscall_read_fd(&line, read_nr))
    else {
        return false;
    };
    let target = std::fs::read_link(format!("{base}/fd/{read_fd}")).ok();
    reads_tty_decision(
        wchan.as_deref(),
        state,
        target
            .as_ref()
            .map(|path| path.to_string_lossy())
            .as_deref(),
    )
}

#[cfg(not(target_os = "linux"))]
fn process_reads_tty(_pid: libc::pid_t) -> bool {
    false
}

/// The `read` syscall number of this architecture, as `/proc/<pid>/syscall` reports it.
#[cfg(target_os = "linux")]
const READ_SYSCALL: Option<u64> = if cfg!(target_arch = "x86_64") {
    Some(0)
} else if cfg!(any(
    target_arch = "aarch64",
    target_arch = "riscv64",
    target_arch = "loongarch64"
)) {
    Some(63)
} else if cfg!(any(target_arch = "x86", target_arch = "arm")) {
    Some(3)
} else {
    None
};

/// Whether a `/proc/<pid>/wchan` answer is the tty line discipline's read. `wait_woken` (where
/// newer kernels report a blocked `n_tty_read`) is NOT enough alone: sockets and pipes wait there
/// too, so it falls through to the syscall check.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn wchan_is_tty_read(wchan: &str) -> bool {
    let wchan = wchan.trim();
    wchan.starts_with("n_tty_read") || wchan == "tty_read"
}

/// The fd of a blocked `read` from a `/proc/<pid>/syscall` line (`<nr> <arg0 hex> ...`), or
/// `None` for another syscall, `running`, or a malformed line.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn syscall_read_fd(line: &str, read_nr: u64) -> Option<i64> {
    let mut fields = line.split_whitespace();
    let nr: u64 = fields.next()?.parse().ok()?;
    if nr != read_nr {
        return None;
    }
    let fd = fields.next()?;
    i64::from_str_radix(fd.strip_prefix("0x").unwrap_or(fd), 16).ok()
}

/// Whether an fd's `/proc/<pid>/fd` target is a terminal (`/dev/pts/N`, `/dev/tty`, `/dev/ttyX`).
#[cfg(any(target_os = "linux", test))]
pub(crate) fn path_is_tty(path: &str) -> bool {
    path.starts_with("/dev/pts/") || path.starts_with("/dev/tty")
}

/// The decision of [`foreground_reads_tty`] on Linux from what was read: the line discipline's
/// read in `wchan`, or an asleep process (`S`) in a `read` whose fd is a terminal.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn reads_tty_decision(
    wchan: Option<&str>,
    state: Option<char>,
    read_fd_target: Option<&str>,
) -> bool {
    wchan.is_some_and(wchan_is_tty_read)
        || (state == Some('S') && read_fd_target.is_some_and(path_is_tty))
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

/// Every process (pid, `comm`) whose process group (field 5 of `/proc/<pid>/stat`) is `pgid`,
/// ordered by pid, or `None` when `/proc` cannot be listed. Processes with an empty `comm` are
/// skipped.
///
/// #1958: this was a read of EVERY process's `stat` (4-16 ms on the main thread per call). It is
/// now the walk of the pty's own session tree, or the `getpgid` list where the walk cannot vouch
/// for its answer; see `fg_group`. `fd` is the pty master, asked for its session leader.
#[cfg(target_os = "linux")]
fn process_group_members(fd: RawFd, pgid: libc::pid_t) -> Option<Vec<(libc::pid_t, String)>> {
    let started = warpui::input_order::enabled().then(std::time::Instant::now);
    let lookup = super::fg_group::group_members(
        &super::fg_group::RealProc,
        super::fg_group::lookup_mode(),
        session_leader(fd),
        pgid,
    );
    log_group_lookup(
        started,
        "members",
        lookup.path,
        lookup.reads,
        lookup.members.as_ref().map(Vec::len),
    );
    lookup.members
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

// ---- subshell readiness (#1679 / #1711 step 3) ------------------------------------------------

use crate::terminal::model::subshell_tty::{
    LeaderKind, SubshellTtyKind, SubshellTtySample, classify_leader,
};

/// What the pty says about who reads it, for the subshell setup's readiness judge
/// (`view/subshell_ready.rs`). `parent_pid` is the shell that ran the block (the session's
/// reported pid; the spawn pid for the top-level shell): its own group in front means no
/// subshell is, so the sample is `Unknown` with `LeaderKind::Parent`.
///
/// Order, and cost (review R10: ioctls first, `/proc` only for a candidate, and this is called at
/// most once per output wakeup): `tcgetattr` (Secret ends it here, nothing else is read),
/// `tcgetpgrp` (no group or the parent's ends it), ONE read of the leader's comm
/// (`/proc/<pgid>/stat` on Linux, `proc_pidpath` on macOS; never a `/proc` scan), then, for a
/// shell or relay leader only, the slave input-queue probe ([`slave_input_queue_empty`], R1).
/// Precedence: Secret > Relay / Unknown > LineReader > Cooked.
pub fn subshell_tty_fact(fd: RawFd, parent_pid: i32) -> SubshellTtySample {
    let termios = fetch_termial_attributes(fd).ok();
    let canonical = termios
        .as_ref()
        .map(|t| t.local_flags.contains(termios::LocalFlags::ICANON));
    if termios
        .as_ref()
        .is_some_and(|t| is_canonical_without_echo(t.local_flags))
    {
        return SubshellTtySample {
            kind: SubshellTtyKind::Secret,
            leader: LeaderKind::Unreadable,
            leader_comm: None,
            canonical,
            queue_empty: None,
        };
    }
    // SAFETY: `tcgetpgrp` only reads the terminal's foreground group id.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 {
        return SubshellTtySample {
            canonical,
            ..SubshellTtySample::unknown()
        };
    }
    if pgid == parent_pid {
        return SubshellTtySample {
            kind: SubshellTtyKind::Unknown,
            leader: LeaderKind::Parent,
            leader_comm: None,
            canonical,
            queue_empty: None,
        };
    }
    let Some(comm) = leader_comm(pgid) else {
        return SubshellTtySample {
            canonical,
            ..SubshellTtySample::unknown()
        };
    };
    let leader = classify_leader(&comm);
    let kind = match (leader, canonical) {
        (LeaderKind::Relay, _) => SubshellTtyKind::Relay,
        (LeaderKind::Shell, Some(false)) => SubshellTtyKind::LineReader,
        (LeaderKind::Shell, Some(true)) => SubshellTtyKind::Cooked,
        _ => SubshellTtyKind::Unknown,
    };
    let queue_empty = matches!(leader, LeaderKind::Shell | LeaderKind::Relay)
        .then(|| slave_input_queue_empty(fd))
        .flatten();
    SubshellTtySample {
        kind,
        leader,
        leader_comm: Some(comm),
        canonical,
        queue_empty,
    }
}

/// The comm of the process `pid` (the foreground group's leader), or `None` when it cannot be
/// read: one `/proc/<pid>/stat` read on Linux, `proc_pidpath` on macOS, nothing elsewhere.
fn leader_comm(pid: libc::pid_t) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        crate::terminal::model::subshell_tty::comm_from_proc_stat(&stat)
    }
    #[cfg(target_os = "macos")]
    {
        process_name(pid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

/// `TIOCGPTPEER` (Linux 4.13+): an fd for the master's own slave, with no path lookup and no
/// `devpts` namespace question. `_IO('T', 0x41)` on every architecture this app builds for
/// (x86-64, aarch64); spelled here rather than taken from `libc` so an older crate does not
/// decide whether this compiles.
#[cfg(target_os = "linux")]
const TIOCGPTPEER: libc::c_ulong = 0x5441;

/// Opens the slave side of the pty whose master is `fd`, read-only, `O_NOCTTY | O_NONBLOCK |
/// O_CLOEXEC` (never our controlling tty, never a blocking open); `TIOCGPTPEER` first, `ptsname_r`
/// + `open` as the fallback for an older kernel. The caller closes it. Opening another slave fd
/// changes nothing for the shell that holds the slave: hang-up happens when the LAST slave fd
/// closes, and ours is never the last while the shell lives.
#[cfg(target_os = "linux")]
fn open_slave_peer(fd: RawFd) -> Option<RawFd> {
    let flags = libc::O_RDONLY | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC;
    // SAFETY: `TIOCGPTPEER` takes the open flags as its argument and returns a new fd or -1.
    let peer = unsafe { libc::ioctl(fd, TIOCGPTPEER as _, flags) };
    if peer >= 0 {
        return Some(peer);
    }
    let mut name = [0 as libc::c_char; 64];
    // SAFETY: `ptsname_r` writes at most `name.len()` bytes into `name`.
    if unsafe { libc::ptsname_r(fd, name.as_mut_ptr(), name.len()) } != 0 {
        return None;
    }
    // SAFETY: `name` is NUL-terminated by `ptsname_r`.
    let peer = unsafe { libc::open(name.as_ptr(), flags) };
    (peer >= 0).then_some(peer)
}

/// The consumption proof of the subshell readiness judge (review R1): whether the SLAVE's input
/// queue is empty, i.e. everything written to the master (the user's pass-through bytes) has been
/// read by whoever reads the tty. `Some(false)` when bytes wait, `None` when the platform cannot
/// tell (then nothing is proven, and the judge waits).
///
/// - Linux: a slave peer is opened (`O_NOCTTY | O_NONBLOCK`), a zero-timeout `poll` first (it
///   flushes the line discipline's pending work, as `pty_master_has_unread` relies on), then
///   `FIONREAD` on the slave (`n_tty`: the bytes a `read` would get, which is the same queue the
///   shell reads), then it is closed.
/// - macOS: `FIONREAD` on the MASTER. Master and slave share one tty there and the master's
///   `FIONREAD` counts the input queue, the direction the slave reads (`pty_master_has_unread`
///   uses `TIOCOUTQ` for the other one). To be verified on the Mac box with the lab cases.
/// - Elsewhere: `None`.
pub fn slave_input_queue_empty(fd: RawFd) -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let peer = open_slave_peer(fd)?;
        let answer = (|| {
            let mut pollfd = libc::pollfd {
                fd: peer,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one valid `pollfd`, a zero timeout; `poll` only writes `revents`.
            let polled = unsafe { libc::poll(&mut pollfd, 1, 0) };
            if polled < 0 || pollfd.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
                return None;
            }
            if pollfd.revents & libc::POLLIN != 0 {
                return Some(false);
            }
            let mut pending: libc::c_int = 0;
            // SAFETY: `FIONREAD` writes one `int` through the pointer, which points at `pending`.
            let result =
                unsafe { libc::ioctl(peer, libc::FIONREAD, &mut pending as *mut libc::c_int) };
            (result == 0).then(|| pending == 0)
        })();
        // SAFETY: `peer` is an fd this function opened and nothing else holds.
        unsafe { libc::close(peer) };
        answer
    }
    #[cfg(target_os = "macos")]
    {
        let mut pending: libc::c_int = 0;
        // SAFETY: `FIONREAD` writes one `int` through the pointer, which points at `pending`.
        let result = unsafe { libc::ioctl(fd, libc::FIONREAD, &mut pending as *mut libc::c_int) };
        (result == 0).then(|| pending == 0)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = fd;
        None
    }
}

/// Discards the slave's unread input (review R9): a setup cancelled while its bytes provably sit
/// unread in the tty (`slave_input_queue_empty` said `Some(false)` and the shell echoed nothing of
/// them) must not have them read later by whatever reads next -- a `Password:` prompt, the user's
/// own shell. `TCIFLUSH` on the slave peer on Linux (the master's `TCIFLUSH` flushes the other
/// direction there); on macOS on the master, where the queues are one tty's (to be verified on the
/// Mac box). Returns whether a flush was issued.
pub fn flush_slave_input(fd: RawFd) -> bool {
    #[cfg(target_os = "linux")]
    {
        let Some(peer) = open_slave_peer(fd) else {
            return false;
        };
        // SAFETY: `tcflush` on an fd this function opened.
        let flushed = unsafe { libc::tcflush(peer, libc::TCIFLUSH) } == 0;
        // SAFETY: `peer` is an fd this function opened and nothing else holds.
        unsafe { libc::close(peer) };
        flushed
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY: `tcflush` only discards queued data of the tty behind `fd`.
        unsafe { libc::tcflush(fd, libc::TCIFLUSH) == 0 }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = fd;
        false
    }
}

#[cfg(test)]
#[path = "terminal_attributes_tests.rs"]
mod tests;
