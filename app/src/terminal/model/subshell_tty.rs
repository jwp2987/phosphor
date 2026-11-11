//! What the pty says about who reads it, for the subshell setup's readiness judge (#1679,
//! #1711 step 3; the Opus review of the redesign, R1-R5). The OS probe that fills these in lives
//! in `local_tty::terminal_attributes::subshell_tty_fact` (unix local ptys); this module is the
//! platform-free part: the types, the leader lists, the comm classification, and the probe's
//! signature, so the view's pure judge (`view/subshell_ready.rs`) and the model compile without
//! the `local_tty` feature and a test installs a sample of its own.
//!
//! Precedence of the sample, as the probe builds it: Secret > Relay / Unknown > LineReader >
//! Cooked. The leader is the pty's foreground process-group LEADER only (`/proc/<pgid>/stat` on
//! Linux, `proc_pidpath` on macOS): one read, never a `/proc` scan (R10).

use std::sync::Arc;

/// The shells the setup can be written into once their reader proves ready: the ones the
/// bootstrap has a script for (bash, zsh, fish, nu, pwsh) and the ones the `sh` probe runs in
/// and then reports as unsupported by the event (#1710: dash, ash, busybox, ksh).
pub const SHELL_LEADERS: &[&str] = &[
    "bash",
    "zsh",
    "fish",
    "dash",
    "sh",
    "ash",
    "busybox",
    "ksh",
    "mksh",
    "nu",
    "nushell",
    "pwsh",
    "powershell",
];

/// Programs that put a shell behind the pty without being one: a privilege hop, a login hop to
/// another machine, a container, a multiplexer, a recorder. While one of these is the foreground
/// leader the OS facts describe IT, not the shell behind it, so only the prompt-shape rule applies
/// (review R4). `sudo` with `use_pty`, `ssh` and `docker` stay the leader for the session. `su`
/// (classic, pty shared) and `sudo` without `use_pty` are the leader only while they run
/// themselves (reading the password, exec-ing the shell): the shell they start takes the
/// foreground as its own group, and from then on the sample names THAT shell, with its OS facts
/// (the reconciliation R5 asked for: the list says what the leader is now, not what the hop is).
pub const RELAY_LEADERS: &[&str] = &[
    "sudo",
    "su",
    "doas",
    "ksu",
    "ssh",
    "mosh",
    "mosh-client",
    "et",
    "sshpass",
    "telnet",
    "docker",
    "podman",
    "kubectl",
    "nsenter",
    "chroot",
    "machinectl",
    "lxc",
    "incus",
    "script",
    "expect",
    "screen",
    "tmux",
    "wsl",
    "login",
];

/// Shells the bootstrap has no script for AND the `sh` probe cannot run in (its `[ -z $VAR ] &&
/// printf ...` is not their syntax): an attempt is given up at once as unsupported (review R5)
/// instead of writing a probe that would print errors into the user's shell.
pub const UNSUPPORTED_LEADERS: &[&str] = &[
    "csh", "tcsh", "xonsh", "elvish", "rc", "es", "ion", "oil", "osh", "ysh",
];

/// Shell leaders with no line editor: at their prompt the tty is COOKED (canonical, echo on), the
/// state a `read` builtin of bash/zsh/fish also leaves it in. Only for these may a cooked prompt
/// shape be written into (review R3); a cooked bash/zsh/fish/nu/pwsh is a `read` and waits.
pub const EDITOR_LESS_SHELL_LEADERS: &[&str] = &["dash", "sh", "ash", "busybox", "ksh", "mksh"];

/// What the pty's foreground process-group leader is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaderKind {
    /// One of [`SHELL_LEADERS`].
    Shell,
    /// One of [`RELAY_LEADERS`].
    Relay,
    /// One of [`UNSUPPORTED_LEADERS`].
    Unsupported,
    /// Any other program (vim, fzf, less, a REPL, a compiler).
    Other,
    /// The parent shell's own group: the shell that ran this block is at its prompt (or runs a
    /// builtin); no subshell is in front. Not when the pid's comm changed since the shell reported
    /// it (`exec zsh`, #1937, [`parent_was_replaced`]): that leader is classified by its comm.
    Parent,
    /// No foreground group, an unreadable leader, an unsupported platform.
    Unreadable,
}

/// Which of the lists `comm` is on. A login shell's leading `-` (`-bash`) and case are ignored.
pub fn classify_leader(comm: &str) -> LeaderKind {
    let name = normalize_comm(comm);
    if SHELL_LEADERS.contains(&name.as_str()) {
        LeaderKind::Shell
    } else if RELAY_LEADERS.contains(&name.as_str()) {
        LeaderKind::Relay
    } else if UNSUPPORTED_LEADERS.contains(&name.as_str()) {
        LeaderKind::Unsupported
    } else {
        LeaderKind::Other
    }
}

/// Whether a shell leader reads its prompt through a line editor (raw tty). `false` for the
/// editor-less shells ([`EDITOR_LESS_SHELL_LEADERS`]); `true` for every other name, so an unknown
/// name is never written into at a cooked prompt.
pub fn leader_has_line_editor(comm: &str) -> bool {
    !EDITOR_LESS_SHELL_LEADERS.contains(&normalize_comm(comm).as_str())
}

/// The shell type the setup would be written for, from the leader's comm: what `exec zsh` or
/// `exec fish` in a `.bashrc` leaves in front (review R5). `None` for the probe path (an
/// editor-less shell, a relay, anything else): the probe names the shell by the event.
pub fn shell_type_of_leader(comm: &str) -> Option<crate::terminal::shell::ShellType> {
    use crate::terminal::shell::ShellType;
    match normalize_comm(comm).as_str() {
        "bash" => Some(ShellType::Bash),
        "zsh" => Some(ShellType::Zsh),
        "fish" => Some(ShellType::Fish),
        "nu" | "nushell" => Some(ShellType::Nu),
        "pwsh" | "powershell" => Some(ShellType::PowerShell),
        _ => None,
    }
}

/// The comm as the lists spell it: no login-shell `-`, no `.exe`, lower case.
fn normalize_comm(comm: &str) -> String {
    let name = comm.trim().trim_start_matches('-');
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name.to_ascii_lowercase()
}

/// The leader's comm from a `/proc/<pid>/stat` line (`pid (comm) S ppid pgrp ...`). `comm` may
/// itself contain spaces and parentheses (`(my (odd) prog)`), so it is everything between the
/// FIRST `(` and the LAST `)`; the kernel truncates it to 15 bytes, which the lists allow for.
pub fn comm_from_proc_stat(stat: &str) -> Option<String> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if close <= open {
        return None;
    }
    let comm = &stat[open + 1..close];
    (!comm.is_empty()).then(|| comm.to_owned())
}

/// What the tty is doing, in the order the judge needs: a secret read beats everything (writing
/// into a password is the one unrecoverable mistake), a relay or an unknown leader has no OS fact
/// about the shell behind it, a shell leader's tty is either raw (its line editor reads) or
/// cooked (a `read`, or between a command and the editor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubshellTtyKind {
    /// Canonical with echo off: a program reads a secret (`sudo`, `su`, `read -s`, ssh's
    /// password). Whoever the leader is.
    Secret,
    /// The leader is a [`RELAY_LEADERS`] program: the shell is behind it, out of reach of the OS
    /// facts; only the prompt shape says anything, and only while the local tty is raw (R4).
    Relay,
    /// A shell leader with a non-canonical tty: its line editor is reading.
    LineReader,
    /// A shell leader with a canonical, echoing tty: a `read` builtin, or the gap between a
    /// command and the line editor; for an editor-less shell, its prompt.
    Cooked,
    /// No fact: the parent's group, another program (vim, fzf, a REPL), an unsupported leader,
    /// no foreground group, or no probe.
    Unknown,
}

/// One sample of the pty, taken at an output wakeup, the hand-off's change or a banner click;
/// never on a timer. One `tcgetattr`, one `tcgetpgrp`, at most one `/proc` read and the queue
/// probe's ioctls (R10: the ioctls first, `/proc` only for a candidate); for an editor-less cooked
/// shell with an empty queue, on Linux, the leader's blocked-read fact on top (`reader_blocked`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubshellTtySample {
    pub kind: SubshellTtyKind,
    pub leader: LeaderKind,
    /// The leader's comm, when it was read (a shell, relay, unsupported or other program).
    pub leader_comm: Option<String>,
    /// Whether the LOCAL tty is canonical (`ICANON`); `None` when the query failed. A relay
    /// counts only while this is `Some(false)` (R4: `docker exec -t` without `-i` leaves the
    /// local tty cooked and the keys go to the parent shell).
    pub canonical: Option<bool>,
    /// The consumption proof (R1): whether the SLAVE's input queue is empty, i.e. everything the
    /// app wrote (the user's pass-through bytes) has been read by whoever reads the tty. `None`
    /// when the platform cannot tell. macOS: `FIONREAD` on the master; Linux: a slave peer opened
    /// `O_NOCTTY|O_NONBLOCK`, `poll(0)`, `FIONREAD`, closed.
    pub queue_empty: Option<bool>,
    /// Whether the leader is BLOCKED IN `read(2)` ON A TTY at this sample (owner decision
    /// 2026-11-09, #1935): the shell itself sits in its read, waiting for the next line. Linux
    /// only (`/proc/<pgid>/syscall`: the `read` syscall number of the architecture and an fd that
    /// is a terminal), read once per look, after the queue probe, and only for the one candidate
    /// it decides: an editor-less shell leader on a cooked tty with an empty queue. `None`
    /// everywhere else: another platform, another kind of sample, or a leader whose `/proc` files
    /// the kernel will not show (another user's shell, Yama `ptrace_scope` >= 2).
    pub reader_blocked: Option<bool>,
}

impl SubshellTtySample {
    /// Nothing is known (no probe installed, a failed query, a platform without one).
    pub fn unknown() -> Self {
        Self {
            kind: SubshellTtyKind::Unknown,
            leader: LeaderKind::Unreadable,
            leader_comm: None,
            canonical: None,
            queue_empty: None,
            reader_blocked: None,
        }
    }

    /// The sample as a shell leader at a raw prompt, for tests and the judge's tables.
    #[cfg(test)]
    pub fn line_reader(comm: &str) -> Self {
        Self {
            kind: SubshellTtyKind::LineReader,
            leader: LeaderKind::Shell,
            leader_comm: Some(comm.to_owned()),
            canonical: Some(false),
            queue_empty: Some(true),
            reader_blocked: None,
        }
    }
}

/// Builds a [`SubshellTtySample`] for the pty, given the parent shell's pid (the shell that ran
/// the block; its own group in front is [`LeaderKind::Parent`]) and the comm that pid had when the
/// shell reported it (#1937; `None` when it was never read). Installed by the local tty
/// (`terminal_attributes::subshell_tty_fact`); a test installs a closure of its own.
pub type SubshellTtyProbe = Arc<dyn Fn(i32, Option<String>) -> SubshellTtySample + Send + Sync>;

/// Reads the comm of a process (#1937): `/proc/<pid>/stat` on Linux, `proc_pidpath` on macOS.
/// Installed by the local tty; called once per `InitShell` that carries a pid.
pub type ProcessCommProbe = Arc<dyn Fn(i32) -> Option<String> + Send + Sync>;

/// #1937 (review r8 D6): `exec zsh` typed in the parent shell keeps the parent's pid, so the
/// foreground group is still "the parent's" and every sample read `LeaderKind::Parent` (no fact)
/// until the 60 s ceiling. A pid whose comm is no longer the one the shell had when it reported
/// that pid is a different program: an `exec` replaced the shell. `comm_at_report` is what the
/// pid's comm was at the shell's `InitShell`, `comm_now` what it is at this sample. Then the
/// leader is classified by `comm_now` like any other (a shell, a relay for `exec ssh host`, a
/// program). The same comm (`exec bash` from bash) cannot be told from the parent at its prompt:
/// it stays `Parent`, as before.
pub fn parent_was_replaced(comm_at_report: &str, comm_now: &str) -> bool {
    let (was, now) = (comm_at_report.trim(), comm_now.trim());
    !was.is_empty() && !now.is_empty() && was != now
}

/// Flushes the slave's unread input (R9: a cancelled setup whose bytes the shell provably never
/// read must not be read later by whatever reads next). Returns whether anything was flushed.
pub type SubshellTtyFlush = Arc<dyn Fn() -> bool + Send + Sync>;

/// #1991: how many bytes wait unread in the slave's input queue (`None`: it cannot be told). For
/// the R9 flush of a setup line written to a line editor, which is allowed only while the queue
/// holds exactly that write. Sampled at a cancel, right before the flush it decides.
pub type SubshellTtyInputBytes = Arc<dyn Fn() -> Option<usize> + Send + Sync>;

#[cfg(test)]
#[path = "subshell_tty_tests.rs"]
mod tests;
