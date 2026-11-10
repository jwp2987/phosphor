//! The members of a pty's foreground process group on Linux, without reading the `stat` of every
//! process on the machine (#1958).
//!
//! Linux has no call that lists a process group. Until #1958 the answer was a scan: list `/proc`
//! and read `/proc/<pid>/stat` of EVERY process (about 10 us each, more for a process with many
//! threads), 4-16 ms on the main thread per sample. Three ways to the same answer live here:
//!
//! * [`walk_group_members`] (the default): every process of a session descends from the session's
//!   leader (the pty's shell) unless a parent died, so the leader's subtree is walked through
//!   `/proc/<pid>/task/<tid>/children`. The cost follows the size of that one tab's tree (a shell
//!   running `cat`: four small reads), not the machine's process count. It answers only when it
//!   can be trusted; every doubt (see the function) hands over to the list.
//! * [`list_group_members`]: list `/proc`, ask `getpgid(2)` for each pid (one cheap syscall, no
//!   file), read `stat` only of the members. Exactly the scan's membership, about a tenth of its
//!   cost; still proportional to the machine's process count.
//! * [`scan_group_members`]: the scan itself, kept as the reference the tests compare against and
//!   for an A/B run ([`LOOKUP_ENV`]).
//!
//! The one case where the walk's answer can differ from the scan's and nothing flags it: a member
//! whose parent died (so it now hangs under init or a subreaper, outside the shell's subtree)
//! while the group's leader and at least one other member are still inside it. See
//! `the_walk_does_not_see_an_orphan_beside_a_living_leader` and [`LOOKUP_ENV`].

use std::collections::HashSet;
use std::sync::OnceLock;

use super::terminal_attributes::{is_shell_or_login_hop, parse_proc_stat, parse_proc_stat_state};

pub(crate) type Pid = libc::pid_t;

/// Picks how the foreground group's members are found: unset or `walk` (the default: the walk,
/// the list where the walk cannot be trusted), `list` (always the exact list: no walk, so no
/// orphan is ever missed), `scan` (the pre-#1958 scan of every `stat`, for an A/B measurement).
pub(crate) const LOOKUP_ENV: &str = "PHOSPHOR_FG_GROUP_LOOKUP";

/// The most `/proc` reads (a `stat`, a `children` file, a `task` listing) the walk makes before
/// it gives up and the list answers. A shell running one program is four reads; a pipeline of
/// three with a few background jobs about twenty. A tree of thread-heavy programs (one `children`
/// file per thread) goes over, and there the list is the cheaper way anyway.
pub(crate) const MAX_WALK_READS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LookupMode {
    Walk,
    List,
    Scan,
}

/// The mode [`LOOKUP_ENV`] asks for, read once.
pub(crate) fn lookup_mode() -> LookupMode {
    static MODE: OnceLock<LookupMode> = OnceLock::new();
    *MODE.get_or_init(|| lookup_mode_from(std::env::var(LOOKUP_ENV).ok().as_deref()))
}

pub(crate) fn lookup_mode_from(value: Option<&str>) -> LookupMode {
    match value.map(str::trim) {
        Some("list") => LookupMode::List,
        Some("scan") => LookupMode::Scan,
        _ => LookupMode::Walk,
    }
}

/// Which way answered, for the `fg_group_lookup` hop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LookupPath {
    /// The group leader's own `stat` decided (a shell or login hop leads the group).
    Leader,
    Walk,
    /// The list, because the mode asked for it.
    List,
    /// The list, because the walk could not be trusted.
    ListAfterWalk,
    Scan,
}

impl LookupPath {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LookupPath::Leader => "leader",
            LookupPath::Walk => "walk",
            LookupPath::List => "list",
            LookupPath::ListAfterWalk => "list_after_walk",
            LookupPath::Scan => "scan",
        }
    }
}

/// What `getpgid` said about one pid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PgidAnswer {
    Is(Pid),
    /// No such process (it exited after the listing).
    Gone,
    /// The call failed for another reason (a security module): the `stat` is read instead.
    Unknown,
}

/// The few things read from `/proc` and the kernel, so the tests can put a made-up process tree
/// behind the same code.
pub(crate) trait ProcSource {
    /// `/proc/<pid>/stat`.
    fn stat(&self, pid: Pid) -> Option<String>;
    /// The thread ids of `pid` (the names in `/proc/<pid>/task`).
    fn thread_ids(&self, pid: Pid) -> Option<Vec<Pid>>;
    /// `/proc/<pid>/task/<tid>/children`: the pids forked by that thread, space separated.
    fn children(&self, pid: Pid, tid: Pid) -> Option<String>;
    /// Every pid in `/proc`.
    fn pids(&self) -> Option<Vec<Pid>>;
    /// `getpgid(pid)`.
    fn pgid_of(&self, pid: Pid) -> PgidAnswer;
}

/// The real `/proc` and `getpgid`.
pub(crate) struct RealProc;

fn numeric_names(dir: &str) -> Option<Vec<Pid>> {
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(dir).ok()? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(id) = name.parse::<Pid>() {
            ids.push(id);
        }
    }
    Some(ids)
}

impl ProcSource for RealProc {
    fn stat(&self, pid: Pid) -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()
    }

    fn thread_ids(&self, pid: Pid) -> Option<Vec<Pid>> {
        numeric_names(&format!("/proc/{pid}/task"))
    }

    fn children(&self, pid: Pid, tid: Pid) -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/task/{tid}/children")).ok()
    }

    fn pids(&self) -> Option<Vec<Pid>> {
        numeric_names("/proc")
    }

    fn pgid_of(&self, pid: Pid) -> PgidAnswer {
        // SAFETY: `getpgid` only reads the process group of `pid`.
        let pgid = unsafe { libc::getpgid(pid) };
        if pgid >= 0 {
            PgidAnswer::Is(pgid)
        } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            PgidAnswer::Gone
        } else {
            PgidAnswer::Unknown
        }
    }
}

/// What the walk needs from one `stat` line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StatFacts {
    pub comm: String,
    pub pgrp: Pid,
    pub state: Option<char>,
    /// `num_threads` (field 20), or `None` when the line is too short to hold it.
    pub threads: Option<u32>,
}

/// Parses `pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt
/// utime stime cutime cstime priority nice num_threads ...`.
pub(crate) fn parse_stat_facts(stat: &str) -> Option<StatFacts> {
    let (comm, pgrp) = parse_proc_stat(stat)?;
    let close = stat.rfind(')')?;
    // After the `)`: state is field 0, so `num_threads` (the 20th of the line) is field 17.
    let threads = stat[close + 1..]
        .split_whitespace()
        .nth(17)
        .and_then(|field| field.parse().ok());
    Some(StatFacts {
        comm,
        pgrp,
        state: parse_proc_stat_state(stat),
        threads,
    })
}

/// The pre-#1958 scan: the `stat` of every process, those whose group is `pgid`, ordered by pid.
/// `None` when `/proc` cannot be listed. Processes with an empty `comm` are skipped.
pub(crate) fn scan_group_members(
    src: &impl ProcSource,
    pgid: Pid,
    reads: &mut usize,
) -> Option<Vec<(Pid, String)>> {
    let mut members = Vec::new();
    *reads += 1;
    for pid in src.pids()? {
        *reads += 1;
        // The process may exit between the listing and the read; that is not a failure.
        let Some(stat) = src.stat(pid) else { continue };
        if let Some((comm, pgrp)) = parse_proc_stat(&stat)
            && pgrp == pgid
            && !comm.is_empty()
        {
            members.push((pid, comm));
        }
    }
    members.sort_by_key(|(pid, _)| *pid);
    Some(members)
}

/// The scan's answer without the scan's cost: `getpgid` picks the members out of the `/proc`
/// listing, and only their `stat` is read (its group must agree, as in the scan). `None` when
/// `/proc` cannot be listed.
pub(crate) fn list_group_members(
    src: &impl ProcSource,
    pgid: Pid,
    reads: &mut usize,
) -> Option<Vec<(Pid, String)>> {
    let mut members = Vec::new();
    *reads += 1;
    for pid in src.pids()? {
        match src.pgid_of(pid) {
            PgidAnswer::Is(group) if group != pgid => continue,
            PgidAnswer::Gone => continue,
            PgidAnswer::Is(_) | PgidAnswer::Unknown => {}
        }
        *reads += 1;
        let Some(stat) = src.stat(pid) else { continue };
        if let Some((comm, pgrp)) = parse_proc_stat(&stat)
            && pgrp == pgid
            && !comm.is_empty()
        {
            members.push((pid, comm));
        }
    }
    members.sort_by_key(|(pid, _)| *pid);
    Some(members)
}

/// Counts one read against [`MAX_WALK_READS`]; `None` when the budget is spent.
fn spend(reads: &mut usize) -> Option<()> {
    *reads += 1;
    (*reads <= MAX_WALK_READS).then_some(())
}

/// The members of group `pgid` among the session leader and everything that descends from it,
/// ordered by pid; or `None` when the walk cannot vouch for its answer and the caller must list:
///
/// * a `stat` or a `children` file of the subtree cannot be read (a kernel without
///   `CONFIG_PROC_CHILDREN`, a `/proc` mounted with `hidepid`, a process or thread that exited
///   during the walk: its children moved to another parent that may already have been read);
/// * the walk needs more than [`MAX_WALK_READS`] reads;
/// * no member was found (the group exists, `tcgetpgrp` named it, so its members are elsewhere);
/// * the group's leader is alive but is not in the subtree.
///
/// A leader that has exited is not a doubt: `echo x | less` is that for as long as `less` runs.
///
/// A process forks its children from any of its threads, and the kernel lists them per thread,
/// so a process with more than one thread costs a `task` listing and one `children` read per
/// thread.
pub(crate) fn walk_group_members(
    src: &impl ProcSource,
    session_leader: Pid,
    pgid: Pid,
    reads: &mut usize,
) -> Option<Vec<(Pid, String)>> {
    let mut members = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![session_leader];
    let mut leader_seen = false;
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        spend(reads)?;
        let facts = parse_stat_facts(&src.stat(pid)?)?;
        if pid == pgid {
            leader_seen = true;
        }
        if facts.pgrp == pgid && !facts.comm.is_empty() {
            members.push((pid, facts.comm));
        }
        // One thread, and it is the one named by the pid (a `Z` here can be a process whose main
        // thread exited while others live): its `children` file is the whole list.
        let tids = if facts.threads == Some(1) && facts.state != Some('Z') {
            vec![pid]
        } else {
            spend(reads)?;
            src.thread_ids(pid)?
        };
        for tid in tids {
            spend(reads)?;
            for child in src.children(pid, tid)?.split_whitespace() {
                stack.push(child.parse().ok()?);
            }
        }
    }
    if members.is_empty() {
        return None;
    }
    if !leader_seen {
        spend(reads)?;
        let leader_lives_elsewhere = src
            .stat(pgid)
            .and_then(|stat| parse_proc_stat(&stat))
            .is_some_and(|(_, pgrp)| pgrp == pgid);
        if leader_lives_elsewhere {
            return None;
        }
    }
    members.sort_by_key(|(pid, _)| *pid);
    Some(members)
}

/// The members found, and how.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GroupLookup {
    /// `None` when `/proc` cannot be listed (the caller's "unknown").
    pub members: Option<Vec<(Pid, String)>>,
    pub path: LookupPath,
    /// `/proc` reads made (files and listings), all paths together.
    pub reads: usize,
}

/// Every process (pid, `comm`) of group `pgid`, ordered by pid. `session_leader` is the pty's
/// session leader (`tcgetsid`), the root of the walk; without one the list answers.
pub(crate) fn group_members(
    src: &impl ProcSource,
    mode: LookupMode,
    session_leader: Option<Pid>,
    pgid: Pid,
) -> GroupLookup {
    let mut reads = 0;
    match (mode, session_leader) {
        (LookupMode::Scan, _) => GroupLookup {
            members: scan_group_members(src, pgid, &mut reads),
            path: LookupPath::Scan,
            reads,
        },
        (LookupMode::Walk, Some(leader)) => {
            if let Some(members) = walk_group_members(src, leader, pgid, &mut reads) {
                return GroupLookup {
                    members: Some(members),
                    path: LookupPath::Walk,
                    reads,
                };
            }
            GroupLookup {
                members: list_group_members(src, pgid, &mut reads),
                path: LookupPath::ListAfterWalk,
                reads,
            }
        }
        (LookupMode::List, _) | (LookupMode::Walk, None) => GroupLookup {
            members: list_group_members(src, pgid, &mut reads),
            path: LookupPath::List,
            reads,
        },
    }
}

/// The verdict of a plain-program question, and how it was reached.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PlainProgramLookup {
    pub plain: bool,
    pub path: LookupPath,
    pub reads: usize,
    /// How many members were found, or `None` when the leader decided alone or nothing is known.
    pub members: Option<usize>,
}

/// Whether group `pgid` has members and none of them is a shell or a login hop
/// (`terminal_attributes::foreground_is_plain_program` for a group that is not the shell's own).
///
/// The leader's own `stat` is read first: a shell or login hop there (`ssh`, an inner `bash`,
/// `sudo -i`) makes the answer `false` whatever the other members are, as it does in the scan,
/// with one read. The `scan` mode skips that and does exactly what the code before #1958 did.
pub(crate) fn group_is_plain_program(
    src: &impl ProcSource,
    mode: LookupMode,
    session_leader: Option<Pid>,
    pgid: Pid,
) -> PlainProgramLookup {
    let mut leader_reads = 0;
    if mode != LookupMode::Scan {
        leader_reads = 1;
        let leader_is_hop = src
            .stat(pgid)
            .and_then(|stat| parse_proc_stat(&stat))
            .is_some_and(|(comm, pgrp)| pgrp == pgid && is_shell_or_login_hop(&comm));
        if leader_is_hop {
            return PlainProgramLookup {
                plain: false,
                path: LookupPath::Leader,
                reads: leader_reads,
                members: None,
            };
        }
    }
    let lookup = group_members(src, mode, session_leader, pgid);
    let plain = lookup.members.as_ref().is_some_and(|members| {
        !members.is_empty() && members.iter().all(|(_, comm)| !is_shell_or_login_hop(comm))
    });
    PlainProgramLookup {
        plain,
        path: lookup.path,
        reads: lookup.reads + leader_reads,
        members: lookup.members.as_ref().map(Vec::len),
    }
}

#[cfg(test)]
#[path = "fg_group_tests.rs"]
mod tests;
