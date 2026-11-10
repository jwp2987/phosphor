//! #1958: whichever way the foreground group is looked at, the answer must be the one the scan of
//! every `stat` gave. A made-up process table stands behind [`ProcSource`], so each test names
//! the tree it is about.

use std::cell::Cell;
use std::collections::BTreeMap;

use super::super::terminal_attributes::is_shell_or_login_hop;
use super::{
    LookupMode, LookupPath, MAX_WALK_READS, PgidAnswer, Pid, PlainProgramLookup, ProcSource,
    StatFacts, group_is_plain_program, group_members, list_group_members, lookup_mode_from,
    parse_stat_facts, scan_group_members, walk_sees_shell_or_hop,
};

#[derive(Clone)]
struct FakeProcess {
    comm: String,
    ppid: Pid,
    pgrp: Pid,
    state: char,
    /// Threads besides the main one; their ids are `pid * 1000 + n`.
    extra_threads: u32,
    /// Which of the PARENT's threads forked this process, so which `children` file of the parent
    /// lists it (`None`: the parent's main thread).
    forked_by: Option<Pid>,
    /// `/proc/<pid>` cannot be read (another user's process under `hidepid`).
    hidden: bool,
}

/// A process table. `children` files are derived from `ppid`, as the kernel derives them.
#[derive(Default)]
struct FakeProc {
    procs: BTreeMap<Pid, FakeProcess>,
    /// A kernel without `CONFIG_PROC_CHILDREN`: no `children` file anywhere.
    no_children_file: bool,
    /// `getpgid` refuses (a security module): the list must fall back to the `stat`.
    getpgid_refuses: bool,
    stat_reads: Cell<usize>,
    /// How often `/proc` itself was listed (the list and the scan do; the walk never does).
    listings: Cell<usize>,
}

impl FakeProc {
    fn add(&mut self, pid: Pid, comm: &str, ppid: Pid, pgrp: Pid) -> &mut FakeProcess {
        self.procs.insert(
            pid,
            FakeProcess {
                comm: comm.to_owned(),
                ppid,
                pgrp,
                state: 'S',
                extra_threads: 0,
                forked_by: None,
                hidden: false,
            },
        );
        self.procs.get_mut(&pid).expect("just inserted")
    }

    fn thread_ids_of(&self, pid: Pid) -> Option<Vec<Pid>> {
        let process = self.procs.get(&pid)?;
        let mut ids = vec![pid];
        ids.extend((1..=process.extra_threads as Pid).map(|n| pid * 1000 + n));
        Some(ids)
    }

    fn reset_counts(&self) {
        self.stat_reads.set(0);
        self.listings.set(0);
    }
}

impl ProcSource for FakeProc {
    fn stat(&self, pid: Pid) -> Option<String> {
        self.stat_reads.set(self.stat_reads.get() + 1);
        let process = self.procs.get(&pid).filter(|process| !process.hidden)?;
        // The layout of a real line: 52 fields, `num_threads` the 20th.
        Some(format!(
            "{pid} ({}) {} {} {} 100 34816 {} 4194304 120 0 0 0 1 2 0 0 20 0 {} 0 5000 \
             10000000 900 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0 0 0 0 0 0 0 \
             0 0",
            process.comm,
            process.state,
            process.ppid,
            process.pgrp,
            process.pgrp,
            process.extra_threads + 1,
        ))
    }

    fn thread_ids(&self, pid: Pid) -> Option<Vec<Pid>> {
        self.procs.get(&pid).filter(|process| !process.hidden)?;
        self.thread_ids_of(pid)
    }

    fn children(&self, pid: Pid, tid: Pid) -> Option<String> {
        if self.no_children_file {
            return None;
        }
        self.procs.get(&pid).filter(|process| !process.hidden)?;
        if !self.thread_ids_of(pid)?.contains(&tid) {
            return None;
        }
        let mut line = String::new();
        for (child, process) in &self.procs {
            if process.ppid == pid && process.forked_by.unwrap_or(pid) == tid {
                line.push_str(&format!("{child} "));
            }
        }
        Some(line)
    }

    fn pids(&self) -> Option<Vec<Pid>> {
        self.listings.set(self.listings.get() + 1);
        Some(
            self.procs
                .iter()
                .filter(|(_, process)| !process.hidden)
                .map(|(pid, _)| *pid)
                .collect(),
        )
    }

    fn pgid_of(&self, pid: Pid) -> PgidAnswer {
        if self.getpgid_refuses {
            return PgidAnswer::Unknown;
        }
        match self.procs.get(&pid) {
            Some(process) => PgidAnswer::Is(process.pgrp),
            None => PgidAnswer::Gone,
        }
    }
}

/// The app (pid 50) under a user manager (pid 20) under init (pid 1), other sessions' processes
/// beside it, and the tab's shell: pid 100, a session leader in its own group.
fn machine() -> FakeProc {
    let mut proc = FakeProc::default();
    proc.add(1, "systemd", 0, 1);
    proc.add(20, "systemd", 1, 20);
    proc.add(30, "pipewire", 20, 30).extra_threads = 3;
    proc.add(40, "bash", 20, 40);
    proc.add(41, "ssh", 40, 41);
    proc.add(50, "phosphor", 20, 50).extra_threads = 12;
    proc.add(100, "bash", 50, 100);
    proc
}

const SHELL: Pid = 100;

fn scan(proc: &FakeProc, pgid: Pid) -> Option<Vec<(Pid, String)>> {
    scan_group_members(proc, pgid, &mut 0)
}

fn walk_mode(proc: &FakeProc, pgid: Pid) -> PlainProgramLookup {
    group_is_plain_program(proc, LookupMode::Walk, Some(SHELL), pgid)
}

/// For one group of one process table, everything the module promises:
///
/// * the members asked for in `walk` and in `list` mode are exactly the scan's;
/// * the plain-program verdict is the scan's in `walk` mode, in `walk` mode with no session
///   leader and in `list` mode;
/// * the walk alone says "a shell or hop is a member" only when the scan has one;
/// * in `walk` mode "plain program" is said by the list and by nothing else.
///
/// Returns the `walk` mode verdict for the caller to look at the path taken.
#[track_caller]
fn assert_all_ways_agree(proc: &FakeProc, pgid: Pid) -> PlainProgramLookup {
    let scanned = scan(proc, pgid);
    for mode in [LookupMode::Walk, LookupMode::List] {
        let lookup = group_members(proc, mode, pgid);
        assert_eq!(lookup.members, scanned, "members in {mode:?} mode");
        assert_eq!(lookup.path, LookupPath::List, "members in {mode:?} mode");
    }
    assert_eq!(group_members(proc, LookupMode::Scan, pgid).members, scanned);

    let scan_verdict = group_is_plain_program(proc, LookupMode::Scan, Some(SHELL), pgid);
    assert_eq!(scan_verdict.path, LookupPath::Scan);
    let walked = walk_mode(proc, pgid);
    let no_session = group_is_plain_program(proc, LookupMode::Walk, None, pgid);
    let listed = group_is_plain_program(proc, LookupMode::List, Some(SHELL), pgid);
    assert_eq!(
        walked.plain, scan_verdict.plain,
        "walk mode against the scan"
    );
    assert_eq!(
        no_session.plain, scan_verdict.plain,
        "walk mode, no session leader, against the scan"
    );
    assert_eq!(
        listed.plain, scan_verdict.plain,
        "list mode against the scan"
    );
    for lookup in [&no_session, &listed] {
        assert!(
            matches!(lookup.path, LookupPath::Leader | LookupPath::List),
            "no walk without a session leader or in list mode: {:?}",
            lookup.path
        );
    }

    let scan_has_hop = scanned
        .as_ref()
        .is_some_and(|members| members.iter().any(|(_, comm)| is_shell_or_login_hop(comm)));
    if walk_sees_shell_or_hop(proc, SHELL, pgid, &mut 0) {
        assert!(scan_has_hop, "the walk saw a hop the scan does not have");
    }
    match walked.path {
        LookupPath::Leader | LookupPath::Walk => {
            assert!(!walked.plain);
            assert!(scan_has_hop);
            assert_eq!(walked.members, None);
        }
        LookupPath::ListConfirm => {
            assert_eq!(walked.members, scanned.as_ref().map(Vec::len));
        }
        other => panic!("walk mode with a session leader answered by {other:?}"),
    }
    walked
}

#[test]
fn a_plain_program_is_said_by_the_list_after_a_four_read_walk() {
    let mut proc = machine();
    proc.add(200, "cat", SHELL, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(1));
    // The leader's stat; the walk (the shell's stat and children, the program's stat and
    // children); the list (the listing of /proc and the one member's stat).
    assert_eq!(verdict.reads, 1 + 4 + 2);
    let mut walk_reads = 0;
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 200, &mut walk_reads));
    assert_eq!(walk_reads, 4);
}

/// `make` running `sh`: the walk answers, and nothing outside the shell's tree is read, however
/// many processes the machine runs.
#[test]
fn a_shell_among_the_members_is_decided_inside_the_shells_tree() {
    let mut proc = machine();
    for pid in 1000..3000 {
        proc.add(pid, "worker", 1, pid);
    }
    proc.add(200, "make", SHELL, 200);
    proc.add(201, "sh", 200, 200);
    assert_all_ways_agree(&proc, 200);
    proc.reset_counts();
    let verdict = walk_mode(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Walk);
    // The leader's stat, then the shell's stat and children, `make`'s stat and children, and
    // the stat of `sh`, where the walk stops.
    assert_eq!(verdict.reads, 1 + 5);
    assert_eq!(proc.stat_reads.get(), 4);
    assert_eq!(proc.listings.get(), 0, "/proc is not listed");
    // The list reads the leader's stat and the members' only; the scan reads all two thousand.
    proc.reset_counts();
    let listed = group_is_plain_program(&proc, LookupMode::List, Some(SHELL), 200);
    assert!(!listed.plain);
    assert_eq!(proc.stat_reads.get(), 3);
    assert_eq!(proc.listings.get(), 1);
    proc.reset_counts();
    assert!(!group_is_plain_program(&proc, LookupMode::Scan, Some(SHELL), 200).plain);
    assert!(proc.stat_reads.get() > 2000);
}

#[test]
fn every_stage_of_a_pipeline_is_a_member() {
    let mut proc = machine();
    proc.add(200, "git", SHELL, 200);
    proc.add(201, "grep", SHELL, 200);
    proc.add(202, "less", SHELL, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(3));
    let names: Vec<String> = group_members(&proc, LookupMode::Walk, 200)
        .members
        .expect("listed")
        .into_iter()
        .map(|(_, comm)| comm)
        .collect();
    assert_eq!(names, ["git", "grep", "less"]);
}

/// `echo x | less`: the group is named after `echo`, which is gone for as long as `less` runs.
#[test]
fn a_pipeline_whose_first_stage_exited_is_still_a_plain_program() {
    let mut proc = machine();
    proc.add(201, "less", SHELL, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.members, Some(1));
}

/// The reaped-late first stage: a zombie is a member in the scan, so it is one in the list.
#[test]
fn a_zombie_member_counts_as_it_does_in_the_scan() {
    let mut proc = machine();
    proc.add(200, "echo", SHELL, 200).state = 'Z';
    proc.add(201, "less", SHELL, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.members, Some(2));
    // A zombie shell is a member too, and the walk reads its name like any other.
    proc.add(202, "sh", SHELL, 200).state = 'Z';
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Walk);
}

#[test]
fn a_shell_below_the_leaders_child_makes_the_group_not_plain() {
    let mut proc = machine();
    proc.add(200, "make", SHELL, 200);
    proc.add(201, "cc", 200, 200);
    proc.add(202, "sh", 201, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Walk);
}

/// `timeout 60 bash`: the leader is plain and its child is a shell at a prompt.
#[test]
fn a_shell_below_a_plain_leader_makes_the_group_not_plain() {
    let mut proc = machine();
    proc.add(200, "timeout", SHELL, 200);
    proc.add(201, "bash", 200, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Walk);
}

/// `ssh host`, an inner `bash`, `sudo -i`: the leader's name decides, with one read.
#[test]
fn a_login_hop_leading_the_group_is_decided_by_one_read() {
    for hop in ["ssh", "bash", "sudo", "tmux"] {
        let mut proc = machine();
        proc.add(200, hop, SHELL, 200);
        proc.add(201, "helper", 200, 200);
        assert_all_ways_agree(&proc, 200);
        proc.reset_counts();
        let verdict = walk_mode(&proc, 200);
        assert!(!verdict.plain, "{hop}");
        assert_eq!(verdict.path, LookupPath::Leader, "{hop}");
        assert_eq!(verdict.reads, 1, "{hop}");
        assert_eq!(proc.stat_reads.get(), 1, "{hop}");
        assert_eq!(proc.listings.get(), 0, "{hop}");
    }
}

/// A pid that was reused by a process of another group is not this group's leader.
#[test]
fn a_shell_with_the_leaders_pid_but_another_group_does_not_decide() {
    let mut proc = machine();
    proc.add(200, "bash", 1, 777);
    proc.add(201, "cat", SHELL, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
}

/// Background jobs and an inner interactive shell's own job: only the group asked for. The
/// inner shell is on the way to the job and is not a member of its group.
#[test]
fn other_groups_of_the_session_are_not_members() {
    let mut proc = machine();
    proc.add(150, "sleep", SHELL, 150); // a background job
    proc.add(160, "bash", SHELL, 160); // an inner interactive shell, its own group
    proc.add(161, "vim", 160, 161); // the inner shell's foreground job
    proc.add(162, "watch", 160, 162); // the inner shell's background job
    let verdict = assert_all_ways_agree(&proc, 161);
    assert!(verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(1));
}

/// The shell's own group in front (a builtin `read`, a `( ... )` without job control).
#[test]
fn the_shells_own_group_is_the_shell_and_its_groupless_children() {
    let mut proc = machine();
    proc.add(150, "sleep", SHELL, 150);
    proc.add(170, "bash", SHELL, SHELL);
    let verdict = assert_all_ways_agree(&proc, SHELL);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Leader);
    assert_eq!(
        group_members(&proc, LookupMode::Walk, SHELL)
            .members
            .map(|members| members.len()),
        Some(2)
    );
}

/// The kernel lists children under the thread that forked them: a Go or Node program's child
/// is in a worker thread's file, not the main thread's.
#[test]
fn a_shell_forked_by_a_worker_thread_is_seen_by_the_walk() {
    let mut proc = machine();
    proc.add(200, "node", SHELL, 200).extra_threads = 4;
    proc.add(201, "sh", 200, 200).forked_by = Some(200 * 1000 + 3);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Walk);
}

/// A multi-threaded shell (fish, nu, pwsh) as the session leader.
#[test]
fn a_threaded_shell_is_walked_through_all_its_threads() {
    let mut proc = machine();
    proc.procs.get_mut(&SHELL).expect("shell").extra_threads = 2;
    proc.add(200, "timeout", SHELL, 200).forked_by = Some(SHELL * 1000 + 2);
    proc.add(201, "bash", 200, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Walk);
}

/// More `children` files than the walk's budget: it stops looking, and the list finds the shell.
#[test]
fn a_tree_of_thread_heavy_programs_is_decided_by_the_list() {
    let mut proc = machine();
    proc.add(200, "java", SHELL, 200).extra_threads = MAX_WALK_READS as u32;
    proc.add(201, "sh", 200, 200);
    let mut walk_reads = 0;
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 200, &mut walk_reads));
    assert_eq!(walk_reads, MAX_WALK_READS + 1, "stopped at the budget");
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(2));
    // The leader's stat, the walk up to its budget, the listing and the two members' stat.
    assert_eq!(verdict.reads, 1 + (MAX_WALK_READS + 1) + 3);
}

#[test]
fn a_kernel_without_children_files_is_decided_by_the_list() {
    let mut proc = machine();
    proc.no_children_file = true;
    proc.add(200, "make", SHELL, 200);
    proc.add(201, "sh", 200, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(2));
}

/// The owner decision of 2026-11-09, and the case it is about: a member whose parent died (it
/// hangs under the user manager now) beside a leader and another member that are still below the
/// shell. Nothing in the tree says it exists, so the walk alone sees a plain program; the answer
/// given is the list's, which is the scan's.
#[test]
fn an_orphan_beside_a_living_leader_gets_the_scans_answer() {
    let mut proc = machine();
    proc.add(200, "tee", SHELL, 200);
    proc.add(202, "sh", 20, 200); // its parent, a stage of the pipeline, died
    assert_eq!(scan(&proc, 200).expect("listed").len(), 2);
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 200, &mut 0));
    assert!(!group_is_plain_program(&proc, LookupMode::Scan, Some(SHELL), 200).plain);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain, "the orphaned shell is a member");
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(2));
    assert!(!group_is_plain_program(&proc, LookupMode::List, Some(SHELL), 200).plain);
}

/// The same orphan under init, and an orphan that is itself plain: counted, and still plain.
#[test]
fn orphans_are_members_wherever_they_hang() {
    let mut proc = machine();
    proc.add(200, "tee", SHELL, 200);
    proc.add(202, "ssh", 1, 200);
    assert!(!assert_all_ways_agree(&proc, 200).plain);

    let mut proc = machine();
    proc.add(200, "tee", SHELL, 200);
    proc.add(202, "sleep", 20, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.members, Some(2));
}

/// Every member's parent died: they hang under the user manager now, and the shell's tree holds
/// none of them.
#[test]
fn a_group_wholly_outside_the_shells_tree_is_decided_by_the_list() {
    let mut proc = machine();
    proc.add(201, "sh", 20, 200);
    proc.add(202, "sleep", 201, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(2));
}

/// The leader is alive but not below the shell (its parent died), while a member still is.
#[test]
fn a_living_leader_outside_the_shells_tree() {
    let mut proc = machine();
    proc.add(200, "tee", 20, 200);
    proc.add(201, "cat", SHELL, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.members, Some(2));
    // A shell leading from out there is the leader's read.
    proc.procs.get_mut(&200).expect("leader").comm = "sh".to_owned();
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::Leader);
}

/// A process of the tree that cannot be read (`hidepid`, or it exited mid-walk): what is below
/// it is not known to the walk. The scan cannot read it either, and still sees the readable
/// member below it; so does the list.
#[test]
fn an_unreadable_process_in_the_tree() {
    let mut proc = machine();
    proc.add(200, "sudo", SHELL, 200).hidden = true;
    proc.add(201, "tail", 200, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(
        verdict.plain,
        "the scan reads `tail` only, and so does the list"
    );
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    assert_eq!(verdict.members, Some(1));
    // A shell below the unreadable process: the walk cannot get there, the list does.
    proc.add(202, "sh", 200, 200);
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 200, &mut 0));
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
    // A shell beside it: the walk goes on past what it cannot read.
    proc.add(203, "sh", SHELL, 200);
    assert_eq!(assert_all_ways_agree(&proc, 200).path, LookupPath::Walk);
}

#[test]
fn a_shell_that_is_gone_is_no_obstacle() {
    let mut proc = machine();
    proc.procs.remove(&SHELL);
    proc.add(200, "cat", 20, 200);
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.path, LookupPath::ListConfirm);
}

#[test]
fn a_group_with_no_process_left_is_empty_not_plain() {
    let proc = machine();
    let verdict = assert_all_ways_agree(&proc, 200);
    assert!(!verdict.plain);
    assert_eq!(verdict.members, Some(0));
    assert_eq!(
        group_members(&proc, LookupMode::Walk, 200).members,
        Some(Vec::new())
    );
}

/// No session leader known (`tcgetsid` failed): no walk, the list alone.
#[test]
fn without_a_session_leader_the_list_answers() {
    let mut proc = machine();
    proc.add(200, "cat", SHELL, 200);
    let verdict = group_is_plain_program(&proc, LookupMode::Walk, None, 200);
    assert!(verdict.plain);
    assert_eq!(verdict.path, LookupPath::List);
    assert_eq!(verdict.reads, 1 + 2);
}

/// `getpgid` refused for every pid: the list reads each `stat`, as the scan does, and agrees.
#[test]
fn the_list_falls_back_to_the_stat_where_getpgid_refuses() {
    let mut proc = machine();
    proc.getpgid_refuses = true;
    proc.add(200, "git", SHELL, 200);
    proc.add(201, "less", SHELL, 200);
    assert_eq!(list_group_members(&proc, 200, &mut 0), scan(&proc, 200));
    assert_all_ways_agree(&proc, 200);
}

/// Many made-up process tables: pipelines, nested jobs, threads, zombies, groups of every shape,
/// and what the walk cannot see or read: members re-parented to the user manager or to init,
/// unreadable processes, kernels without `children` files. For every group of every table the
/// members and the verdict are the scan's in every mode (`assert_all_ways_agree`).
#[test]
fn made_up_trees_give_the_scans_answer_in_every_mode() {
    const COMMS: [&str; 8] = [
        "cat", "make", "sh", "node", "ssh", "less", "python3", "bash",
    ];
    let mut seed: u64 = 0x1958;
    let mut next = move |bound: u64| {
        // A small linear congruential generator: the trees only need to differ, repeatably.
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % bound
    };
    let mut by_leader = 0;
    let mut by_walk = 0;
    let mut by_list = 0;
    let mut plain = 0;
    // "Not plain" said by the list for a group with members, after a walk that saw no shell or
    // hop: the shell or hop was an orphan, below an unreadable process, past the walk's budget
    // or behind a missing `children` file. What a walk trusted for "plain" would have got wrong.
    let mut list_found_the_hop = 0;
    for _ in 0..400 {
        let mut proc = machine();
        proc.no_children_file = next(20) == 0;
        let count = 1 + next(9) as Pid;
        let mut pids = vec![SHELL];
        let mut groups = vec![SHELL];
        for index in 0..count {
            let pid = 200 + index;
            let parent = pids[next(pids.len() as u64) as usize];
            // A new group of its own, or one that exists already.
            let pgrp = if next(3) == 0 {
                pid
            } else {
                groups[next(groups.len() as u64) as usize]
            };
            let comm = COMMS[next(COMMS.len() as u64) as usize];
            let parent_threads = proc.thread_ids_of(parent).expect("parent exists");
            let forked_by = parent_threads[next(parent_threads.len() as u64) as usize];
            let process = proc.add(pid, comm, parent, pgrp);
            process.forked_by = Some(forked_by);
            process.extra_threads = if next(4) == 0 { next(24) as u32 } else { 0 };
            if next(12) == 0 {
                process.state = 'Z';
                process.extra_threads = 0;
            }
            process.hidden = next(25) == 0;
            pids.push(pid);
            if !groups.contains(&pgrp) {
                groups.push(pgrp);
            }
        }
        // A zombie has no children: give any it was dealt to the shell.
        let zombies: Vec<Pid> = proc
            .procs
            .iter()
            .filter(|(_, process)| process.state == 'Z')
            .map(|(pid, _)| *pid)
            .collect();
        for process in proc.procs.values_mut() {
            if zombies.contains(&process.ppid) {
                process.ppid = SHELL;
                process.forked_by = None;
            }
        }
        // Orphans: about one process in five lost its parent and hangs under the user manager
        // or init, its own children still below it.
        for (pid, process) in proc.procs.iter_mut() {
            if *pid >= 200 && next(5) == 0 {
                process.ppid = if next(2) == 0 { 20 } else { 1 };
                process.forked_by = None;
            }
        }
        for pgid in groups {
            let verdict = assert_all_ways_agree(&proc, pgid);
            match verdict.path {
                LookupPath::Leader => by_leader += 1,
                LookupPath::Walk => by_walk += 1,
                LookupPath::ListConfirm => by_list += 1,
                other => panic!("walk mode answered by {other:?}"),
            }
            if verdict.plain {
                plain += 1;
            }
            if verdict.path == LookupPath::ListConfirm
                && !verdict.plain
                && verdict.members.is_some_and(|count| count > 0)
            {
                list_found_the_hop += 1;
            }
        }
    }
    // Every way was exercised. The generator is fixed, so these counts never change; a Python
    // port of it gave leader 625, walk 52, list 380, plain 330, list-found-the-hop 33.
    assert!(by_leader > 100, "the leader answered {by_leader} times");
    assert!(by_walk > 30, "the walk answered {by_walk} times");
    assert!(by_list > 100, "the list answered {by_list} times");
    assert!(plain > 50, "{plain} plain verdicts");
    assert!(
        list_found_the_hop > 20,
        "the list found a shell or hop the walk had not seen {list_found_the_hop} times"
    );
}

#[test]
fn the_walk_alone_says_only_what_it_saw() {
    let mut proc = machine();
    proc.add(200, "cat", SHELL, 200);
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 200, &mut 0));
    proc.add(201, "sh", 200, 200);
    assert!(walk_sees_shell_or_hop(&proc, SHELL, 200, &mut 0));
    // A shell of another group is not this group's.
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 999, &mut 0));
    // The budget is already spent.
    let mut spent = MAX_WALK_READS;
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 200, &mut spent));
    // No `children` file to read.
    proc.no_children_file = true;
    assert!(!walk_sees_shell_or_hop(&proc, SHELL, 200, &mut 0));
}

#[test]
fn the_stat_line_gives_comm_group_state_and_thread_count() {
    let proc = {
        let mut proc = machine();
        proc.add(200, "we (ird) name", SHELL, 77).extra_threads = 6;
        proc
    };
    assert_eq!(
        parse_stat_facts(&proc.stat(200).expect("readable")),
        Some(StatFacts {
            comm: "we (ird) name".to_owned(),
            pgrp: 77,
            state: Some('S'),
            threads: Some(7),
        })
    );
    // A line cut short still gives the group; the thread count is unknown (all threads are read).
    assert_eq!(
        parse_stat_facts("12 (bash) S 1 12 12 0"),
        Some(StatFacts {
            comm: "bash".to_owned(),
            pgrp: 12,
            state: Some('S'),
            threads: None,
        })
    );
    assert_eq!(parse_stat_facts("garbage"), None);
}

#[test]
fn the_lookup_mode_is_the_walk_unless_asked_otherwise() {
    assert_eq!(lookup_mode_from(None), LookupMode::Walk);
    assert_eq!(lookup_mode_from(Some("")), LookupMode::Walk);
    assert_eq!(lookup_mode_from(Some("walk")), LookupMode::Walk);
    assert_eq!(lookup_mode_from(Some(" list ")), LookupMode::List);
    assert_eq!(lookup_mode_from(Some("scan")), LookupMode::Scan);
    assert_eq!(lookup_mode_from(Some("anything else")), LookupMode::Walk);
}

/// The real `/proc`: this test process's own group. The test runner is alive and in its group
/// for the whole test, so it must be in every list of the members; other members may come and
/// go between the looks, so only that much is asserted, and of the verdict only how it was
/// reached.
#[test]
fn the_real_proc_finds_this_process_in_its_own_group() {
    use super::RealProc;
    // SAFETY: the three calls only read this process's ids.
    let (me, pgid, session) = unsafe { (libc::getpid(), libc::getpgrp(), libc::getsid(0)) };
    assert_eq!(RealProc.pgid_of(me), PgidAnswer::Is(pgid));
    let has_me = |members: Option<Vec<(Pid, String)>>| {
        members
            .expect("/proc is listed")
            .iter()
            .any(|(pid, comm)| *pid == me && !comm.is_empty())
    };
    assert!(has_me(scan_group_members(&RealProc, pgid, &mut 0)));
    assert!(has_me(list_group_members(&RealProc, pgid, &mut 0)));
    let listed = group_members(&RealProc, LookupMode::Walk, pgid);
    assert_eq!(listed.path, LookupPath::List);
    assert!(has_me(listed.members));
    let verdict = group_is_plain_program(&RealProc, LookupMode::Walk, Some(session), pgid);
    match verdict.path {
        LookupPath::Leader | LookupPath::Walk => assert!(!verdict.plain),
        LookupPath::ListConfirm => {
            assert!(verdict.members.is_some_and(|count| count >= 1));
        }
        other => panic!("walk mode with a session leader answered by {other:?}"),
    }
}
