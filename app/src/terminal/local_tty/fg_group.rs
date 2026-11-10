//! The members of a pty's foreground process group on Linux, without reading the `stat` of every
//! process on the machine (#1958).
//!
//! Linux has no call that lists a process group, and the tty reports the group's id, never its
//! size. Until #1958 the answer was a scan: list `/proc` and read `/proc/<pid>/stat` of EVERY
//! process (about 10 us each, more for a process with many threads), 4-16 ms on the main thread
//! per sample. What lives here:
//!
//! * [`list_group_members`]: list `/proc`, ask `getpgid(2)` for each pid (one cheap syscall, no
//!   file), read `stat` only of the members. Exactly the scan's membership, about a tenth of its
//!   cost; still proportional to the machine's process count. It is the answer wherever the
//!   members themselves are asked for ([`group_members`]).
//! * [`walk_sees_shell_or_hop`]: every process of a session descends from the session's leader
//!   (the pty's shell) unless a parent died, so the leader's subtree is walked through
//!   `/proc/<pid>/task/<tid>/children`, looking for ONE thing: a member that is a shell or a
//!   login hop. The cost follows the size of that one tab's tree, not the machine's process count.
//! * [`scan_group_members`]: the scan itself, kept as the reference the tests compare against and
//!   for an A/B run ([`LOOKUP_ENV`]).
//!
//! # Which answer the walk may give (owner decision 2026-11-09)
//!
//! The walk sees a SUBSET of the group: a member whose parent died hangs under init or a
//! subreaper, outside the shell's subtree, and nothing in the tree (or on the tty) says it
//! exists. So the walk is trusted only for the answer a missing member cannot change:
//!
//! * "a shell or login hop is a member" (so: NOT a plain program, the answer that keeps a key
//!   held): a member the walk saw is a member in the scan too, whatever else the group holds.
//! * "no shell or hop seen" would be "plain program", the answer that DELIVERS a key instead of
//!   holding it, and one unseen member could turn it around. The walk never gives it: the list
//!   answers ([`LookupPath::ListConfirm`]).
//!
//! [`group_is_plain_program`] therefore equals the scan's verdict for every process table, the
//! orphan included (`an_orphan_beside_a_living_leader_gets_the_scans_answer`), and the list's cost
//! is paid only where the verdict is, or would have been, "plain program".

use std::collections::HashSet;
use std::sync::OnceLock;

use super::terminal_attributes::{is_shell_or_login_hop, parse_proc_stat, parse_proc_stat_state};

pub(crate) type Pid = libc::pid_t;

/// Picks how the foreground group is looked at: unset or `walk` (the default: for the
/// plain-program question the leader's name, then the walk for a shell or hop, then the list for
/// every "plain" verdict; the list for the members), `list` (no walk at all: the leader's name,
/// then the list), `scan` (the pre-#1958 scan of every `stat`, for an A/B measurement). All three
/// give the same verdicts and members; they differ in cost only.
pub(crate) const LOOKUP_ENV: &str = "PHOSPHOR_FG_GROUP_LOOKUP";

/// The most `/proc` reads (a `stat`, a `children` file, a `task` listing) the walk makes before
/// it stops looking and the list answers. A shell running one program is four reads; a pipeline
/// of three with a few background jobs about twenty. A tree of thread-heavy programs (one
/// `children` file per thread) goes over, and the list costs less than walking it.
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
    /// The walk saw a shell or login hop among the members (not a plain program).
    Walk,
    /// The list alone: the members were asked for, the mode is `list`, or the pty has no
    /// session leader to walk from.
    List,
    /// The list, after a walk that saw no shell or hop: the verdict "plain program" is the
    /// list's to give, never the walk's.
    ListConfirm,
    Scan,
}

impl LookupPath {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LookupPath::Leader => "leader",
            LookupPath::Walk => "walk",
            LookupPath::List => "list",
            LookupPath::ListConfirm => "list_confirm",
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

/// Counts one read against [`MAX_WALK_READS`]; `false` when the budget is spent.
fn spend(reads: &mut usize) -> bool {
    *reads += 1;
    *reads <= MAX_WALK_READS
}

/// Whether a shell or login hop of group `pgid` is seen among the session leader and everything
/// that descends from it. `true` is a fact about the group (that process is a member in the scan
/// too). `false` is NOT "the group has none": a member whose parent died is outside the subtree,
/// a process may be unreadable, a kernel may have no `children` files
/// (`CONFIG_PROC_CHILDREN`), and the walk stops after [`MAX_WALK_READS`] reads. The caller lists
/// on `false`.
///
/// A process forks its children from any of its threads, and the kernel lists them per thread,
/// so a process with more than one thread costs a `task` listing and one `children` read per
/// thread.
pub(crate) fn walk_sees_shell_or_hop(
    src: &impl ProcSource,
    session_leader: Pid,
    pgid: Pid,
    reads: &mut usize,
) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![session_leader];
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if !spend(reads) {
            return false;
        }
        // Unreadable (another user's process under `hidepid`, or it just exited): nothing is
        // known about it or below it. The rest of the tree can still show a hop.
        let Some(facts) = src.stat(pid).and_then(|stat| parse_stat_facts(&stat)) else {
            continue;
        };
        if facts.pgrp == pgid && is_shell_or_login_hop(&facts.comm) {
            return true;
        }
        // One thread, and it is the one named by the pid (a `Z` here can be a process whose main
        // thread exited while others live): its `children` file is the whole list.
        let tids = if facts.threads == Some(1) && facts.state != Some('Z') {
            vec![pid]
        } else {
            if !spend(reads) {
                return false;
            }
            let Some(tids) = src.thread_ids(pid) else {
                continue;
            };
            tids
        };
        for tid in tids {
            if !spend(reads) {
                return false;
            }
            let Some(children) = src.children(pid, tid) else {
                continue;
            };
            stack.extend(
                children
                    .split_whitespace()
                    .filter_map(|child| child.parse::<Pid>().ok()),
            );
        }
    }
    false
}

/// The members found, and how.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GroupLookup {
    /// `None` when `/proc` cannot be listed (the caller's "unknown").
    pub members: Option<Vec<(Pid, String)>>,
    pub path: LookupPath,
    /// `/proc` reads made (files and listings).
    pub reads: usize,
}

/// Every process (pid, `comm`) of group `pgid`, ordered by pid: the list (the scan in `scan`
/// mode). Never the walk: a caller that wants the members wants all of them, and the walk cannot
/// know it has seen them all (module docs).
pub(crate) fn group_members(src: &impl ProcSource, mode: LookupMode, pgid: Pid) -> GroupLookup {
    let mut reads = 0;
    match mode {
        LookupMode::Scan => GroupLookup {
            members: scan_group_members(src, pgid, &mut reads),
            path: LookupPath::Scan,
            reads,
        },
        LookupMode::Walk | LookupMode::List => GroupLookup {
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
    /// `/proc` reads made (files and listings), all steps together.
    pub reads: usize,
    /// How many members the list (or the scan) found; `None` when the leader or the walk decided
    /// without counting them, or `/proc` cannot be listed.
    pub members: Option<usize>,
}

/// Whether group `pgid` has members and none of them is a shell or a login hop
/// (`terminal_attributes::foreground_is_plain_program` for a group that is not the shell's own).
/// The scan's verdict in every mode, for every process table.
///
/// 1. The leader's own `stat`: a shell or login hop there (`ssh`, an inner `bash`, `sudo -i`)
///    makes the answer `false` whatever the other members are, with one read.
/// 2. `walk` mode with a session leader (`tcgetsid` of the pty): the walk of the shell's subtree.
///    A shell or hop seen there (`make` running `sh`, `timeout 60 bash`) is `false` too.
/// 3. Otherwise the list decides. "Plain program" is only ever said here, with every member of
///    the group read (module docs: the walk cannot see a member whose parent died).
///
/// The `scan` mode skips all of that and does exactly what the code before #1958 did.
pub(crate) fn group_is_plain_program(
    src: &impl ProcSource,
    mode: LookupMode,
    session_leader: Option<Pid>,
    pgid: Pid,
) -> PlainProgramLookup {
    let mut reads = 0;
    let (members, path) = if mode == LookupMode::Scan {
        (scan_group_members(src, pgid, &mut reads), LookupPath::Scan)
    } else {
        reads += 1;
        let leader_is_hop = src
            .stat(pgid)
            .and_then(|stat| parse_proc_stat(&stat))
            .is_some_and(|(comm, pgrp)| pgrp == pgid && is_shell_or_login_hop(&comm));
        if leader_is_hop {
            return PlainProgramLookup {
                plain: false,
                path: LookupPath::Leader,
                reads,
                members: None,
            };
        }
        let mut path = LookupPath::List;
        if mode == LookupMode::Walk
            && let Some(session_leader) = session_leader
        {
            // The walk has its own budget; the leader's read is not counted against it.
            let mut walk_reads = 0;
            let hop_seen = walk_sees_shell_or_hop(src, session_leader, pgid, &mut walk_reads);
            reads += walk_reads;
            if hop_seen {
                return PlainProgramLookup {
                    plain: false,
                    path: LookupPath::Walk,
                    reads,
                    members: None,
                };
            }
            path = LookupPath::ListConfirm;
        }
        (list_group_members(src, pgid, &mut reads), path)
    };
    let plain = members.as_ref().is_some_and(|members| {
        !members.is_empty() && members.iter().all(|(_, comm)| !is_shell_or_login_hop(comm))
    });
    PlainProgramLookup {
        plain,
        path,
        reads,
        members: members.as_ref().map(Vec::len),
    }
}

#[cfg(test)]
#[path = "fg_group_tests.rs"]
mod tests;
