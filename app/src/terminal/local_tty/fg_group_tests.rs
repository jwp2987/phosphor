//! #1958: the walk and the list must give the answer the scan of every `stat` gave. A made-up
//! process table stands behind [`ProcSource`], so each test names the tree it is about.

use std::cell::Cell;
use std::collections::BTreeMap;

use super::{
    GroupLookup, LookupMode, LookupPath, MAX_WALK_READS, PgidAnswer, Pid, ProcSource, StatFacts,
    group_is_plain_program, group_members, list_group_members, lookup_mode_from, parse_stat_facts,
    scan_group_members, walk_group_members,
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

fn names(lookup: &GroupLookup) -> Vec<&str> {
    lookup
        .members
        .as_ref()
        .expect("members known")
        .iter()
        .map(|(_, comm)| comm.as_str())
        .collect()
}

/// The walk, the list and the scan agree on the members, and the plain-program verdict is the
/// one the scan's names give in every mode. Returns the walk-mode lookup for the caller to look
/// at the path taken.
#[track_caller]
fn assert_all_ways_agree(proc: &FakeProc, pgid: Pid) -> GroupLookup {
    let scanned = scan(proc, pgid);
    let walked = group_members(proc, LookupMode::Walk, Some(SHELL), pgid);
    let listed = group_members(proc, LookupMode::List, Some(SHELL), pgid);
    let no_session = group_members(proc, LookupMode::Walk, None, pgid);
    assert_eq!(walked.members, scanned, "walk mode against the scan");
    assert_eq!(listed.members, scanned, "list mode against the scan");
    assert_eq!(no_session.members, scanned, "no session leader known");
    assert_eq!(no_session.path, LookupPath::List);
    let scan_verdict = group_is_plain_program(proc, LookupMode::Scan, Some(SHELL), pgid);
    assert_eq!(scan_verdict.path, LookupPath::Scan);
    for mode in [LookupMode::Walk, LookupMode::List] {
        assert_eq!(
            group_is_plain_program(proc, mode, Some(SHELL), pgid).plain,
            scan_verdict.plain,
            "the plain-program verdict in {mode:?} mode against the scan's"
        );
    }
    walked
}

#[test]
fn a_program_run_by_the_shell_is_found_in_four_reads() {
    let mut proc = machine();
    proc.add(200, "cat", SHELL, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["cat"]);
    // The shell's stat and children, the program's stat and children: nothing else on the
    // machine is read, however many processes it runs.
    assert_eq!(walked.reads, 4);
    let verdict = group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200);
    assert!(verdict.plain);
    assert_eq!(verdict.path, LookupPath::Walk);
    assert_eq!(verdict.members, Some(1));
}

#[test]
fn the_walk_reads_no_stat_outside_the_shells_tree() {
    let mut proc = machine();
    for pid in 1000..3000 {
        proc.add(pid, "worker", 1, pid);
    }
    proc.add(200, "cat", SHELL, 200);
    proc.stat_reads.set(0);
    let walked = group_members(&proc, LookupMode::Walk, Some(SHELL), 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(proc.stat_reads.get(), 2, "the shell's and the program's");
    // The list reads the members' stat only; the scan read all two thousand.
    proc.stat_reads.set(0);
    let listed = group_members(&proc, LookupMode::List, Some(SHELL), 200);
    assert_eq!(proc.stat_reads.get(), 1);
    assert_eq!(listed.members, walked.members);
    proc.stat_reads.set(0);
    assert_eq!(scan(&proc, 200), walked.members);
    assert!(proc.stat_reads.get() > 2000);
}

#[test]
fn every_stage_of_a_pipeline_is_a_member() {
    let mut proc = machine();
    proc.add(200, "git", SHELL, 200);
    proc.add(201, "grep", SHELL, 200);
    proc.add(202, "less", SHELL, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["git", "grep", "less"]);
}

/// `echo x | less`: the group is named after `echo`, which is gone for as long as `less` runs.
#[test]
fn a_pipeline_whose_first_stage_exited_is_still_walked() {
    let mut proc = machine();
    proc.add(201, "less", SHELL, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["less"]);
}

/// The reaped-late first stage: a zombie is a member in the scan, so it is one in the walk.
#[test]
fn a_zombie_member_counts_as_it_does_in_the_scan() {
    let mut proc = machine();
    proc.add(200, "echo", SHELL, 200).state = 'Z';
    proc.add(201, "less", SHELL, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["echo", "less"]);
}

#[test]
fn grandchildren_of_the_leader_are_members() {
    let mut proc = machine();
    proc.add(200, "make", SHELL, 200);
    proc.add(201, "sh", 200, 200);
    proc.add(202, "cc1", 201, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["make", "sh", "cc1"]);
    // `make` leads, but a shell is a member: not a plain program, in every mode.
    assert!(!group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200).plain);
}

/// `timeout 60 bash`: the leader is plain and its child is a shell at a prompt.
#[test]
fn a_shell_below_a_plain_leader_makes_the_group_not_plain() {
    let mut proc = machine();
    proc.add(200, "timeout", SHELL, 200);
    proc.add(201, "bash", 200, 200);
    assert_all_ways_agree(&proc, 200);
    let verdict = group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200);
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
        proc.stat_reads.set(0);
        let verdict = group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200);
        assert!(!verdict.plain, "{hop}");
        assert_eq!(verdict.path, LookupPath::Leader, "{hop}");
        assert_eq!(verdict.reads, 1, "{hop}");
        assert_eq!(proc.stat_reads.get(), 1, "{hop}");
    }
}

/// A pid that was reused by a process of another group is not this group's leader.
#[test]
fn a_shell_with_the_leaders_pid_but_another_group_does_not_decide() {
    let mut proc = machine();
    proc.add(200, "bash", 1, 777);
    proc.add(201, "cat", SHELL, 200);
    assert_all_ways_agree(&proc, 200);
    let verdict = group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200);
    assert!(verdict.plain);
}

/// Background jobs and an inner interactive shell's own job: only the group asked for.
#[test]
fn other_groups_of_the_session_are_not_members_and_do_not_hide_members_below_them() {
    let mut proc = machine();
    proc.add(150, "sleep", SHELL, 150); // a background job
    proc.add(160, "bash", SHELL, 160); // an inner interactive shell, its own group
    proc.add(161, "vim", 160, 161); // the inner shell's foreground job
    proc.add(162, "watch", 160, 162); // the inner shell's background job
    let walked = assert_all_ways_agree(&proc, 161);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["vim"]);
    assert!(group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 161).plain);
}

/// The shell's own group in front (a builtin `read`, a `( ... )` without job control).
#[test]
fn the_shells_own_group_is_the_shell_and_its_groupless_children() {
    let mut proc = machine();
    proc.add(150, "sleep", SHELL, 150);
    proc.add(170, "bash", SHELL, SHELL);
    let walked = assert_all_ways_agree(&proc, SHELL);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["bash", "bash"]);
}

/// The kernel lists children under the thread that forked them: a Go or Node program's child
/// is in a worker thread's file, not the main thread's.
#[test]
fn a_child_forked_by_a_worker_thread_is_found() {
    let mut proc = machine();
    proc.add(200, "node", SHELL, 200).extra_threads = 4;
    proc.add(201, "sh", 200, 200).forked_by = Some(200 * 1000 + 3);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["node", "sh"]);
    assert!(!group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200).plain);
}

/// A multi-threaded shell (fish, nu, pwsh) as the session leader.
#[test]
fn a_threaded_shell_is_walked_through_all_its_threads() {
    let mut proc = machine();
    proc.procs.get_mut(&SHELL).expect("shell").extra_threads = 2;
    proc.add(200, "cat", SHELL, 200).forked_by = Some(SHELL * 1000 + 2);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["cat"]);
}

#[test]
fn a_tree_of_thread_heavy_programs_goes_to_the_list() {
    let mut proc = machine();
    proc.add(200, "java", SHELL, 200).extra_threads = MAX_WALK_READS as u32;
    proc.add(201, "sh", 200, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::ListAfterWalk);
    assert_eq!(names(&walked), ["java", "sh"]);
    // The walk stopped at its budget; the list added its own reads on top.
    assert!(walked.reads > MAX_WALK_READS);
    assert!(walked.reads <= MAX_WALK_READS + 1 + 1 + 2);
}

#[test]
fn a_kernel_without_children_files_goes_to_the_list() {
    let mut proc = machine();
    proc.no_children_file = true;
    proc.add(200, "make", SHELL, 200);
    proc.add(201, "sh", 200, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::ListAfterWalk);
    assert_eq!(names(&walked), ["make", "sh"]);
}

/// Every member's parent died: they hang under the user manager now. The walk finds nothing in
/// the shell's tree, and nothing is no answer for a group `tcgetpgrp` just named.
#[test]
fn a_group_wholly_outside_the_shells_tree_goes_to_the_list() {
    let mut proc = machine();
    proc.add(201, "sh", 20, 200);
    proc.add(202, "sleep", 201, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::ListAfterWalk);
    assert_eq!(names(&walked), ["sh", "sleep"]);
}

/// The leader is alive but not below the shell (its parent died), while a member still is.
#[test]
fn a_living_leader_outside_the_shells_tree_goes_to_the_list() {
    let mut proc = machine();
    proc.add(200, "sh", 20, 200);
    proc.add(201, "tee", SHELL, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::ListAfterWalk);
    assert_eq!(names(&walked), ["sh", "tee"]);
}

/// A process of the tree that cannot be read (`hidepid`, or it exited mid-walk and its children
/// moved): what is below it is not known, so the walk does not answer. The scan cannot read it
/// either, and still sees the readable member below it.
#[test]
fn an_unreadable_process_in_the_tree_goes_to_the_list() {
    let mut proc = machine();
    proc.add(200, "sudo", SHELL, 200).hidden = true;
    proc.add(201, "tail", 200, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::ListAfterWalk);
    assert_eq!(names(&walked), ["tail"]);
}

#[test]
fn a_shell_that_is_gone_goes_to_the_list() {
    let mut proc = machine();
    proc.procs.remove(&SHELL);
    proc.add(200, "cat", 20, 200);
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.path, LookupPath::ListAfterWalk);
    assert_eq!(names(&walked), ["cat"]);
}

#[test]
fn a_group_with_no_process_left_is_empty_not_unknown() {
    let proc = machine();
    let walked = assert_all_ways_agree(&proc, 200);
    assert_eq!(walked.members, Some(Vec::new()));
    assert!(!group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200).plain);
}

/// `getpgid` refused for every pid: the list reads each `stat`, as the scan does, and agrees.
#[test]
fn the_list_falls_back_to_the_stat_where_getpgid_refuses() {
    let mut proc = machine();
    proc.getpgid_refuses = true;
    proc.add(200, "git", SHELL, 200);
    proc.add(201, "less", SHELL, 200);
    assert_eq!(list_group_members(&proc, 200, &mut 0), scan(&proc, 200));
}

/// The stated limit of the walk (module docs): a member whose parent died, beside a leader and
/// another member that are still below the shell. Nothing in the tree says it exists. `list` mode
/// (`PHOSPHOR_FG_GROUP_LOOKUP=list`) sees it.
#[test]
fn the_walk_does_not_see_an_orphan_beside_a_living_leader() {
    let mut proc = machine();
    proc.add(200, "tee", SHELL, 200);
    proc.add(202, "sh", 20, 200); // its parent, a stage of the pipeline, died
    let scanned = scan(&proc, 200).expect("listed");
    assert_eq!(scanned.len(), 2);
    let walked = group_members(&proc, LookupMode::Walk, Some(SHELL), 200);
    assert_eq!(walked.path, LookupPath::Walk);
    assert_eq!(names(&walked), ["tee"]);
    let listed = group_members(&proc, LookupMode::List, Some(SHELL), 200);
    assert_eq!(listed.members, Some(scanned));
    assert!(group_is_plain_program(&proc, LookupMode::Walk, Some(SHELL), 200).plain);
    assert!(!group_is_plain_program(&proc, LookupMode::List, Some(SHELL), 200).plain);
}

/// Many made-up trees below the shell (no orphans): pipelines, nested jobs, threads, zombies,
/// groups of every shape. Whatever path the walk mode takes, its members and its verdict are the
/// scan's.
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
    let mut walked_paths = 0;
    let mut listed_paths = 0;
    for _ in 0..300 {
        let mut proc = machine();
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
        for pgid in groups {
            let walked = assert_all_ways_agree(&proc, pgid);
            match walked.path {
                LookupPath::Walk => walked_paths += 1,
                LookupPath::ListAfterWalk => listed_paths += 1,
                other => panic!("walk mode answered by {other:?}"),
            }
        }
    }
    // Both ways were exercised (the generator is fixed, so these counts never change).
    assert!(walked_paths > 100, "the walk answered {walked_paths} times");
    assert!(listed_paths > 10, "the list answered {listed_paths} times");
}

#[test]
fn the_walk_alone_refuses_rather_than_guess() {
    let mut proc = machine();
    proc.add(200, "cat", SHELL, 200);
    assert!(walk_group_members(&proc, SHELL, 200, &mut 0).is_some());
    // No member in the tree.
    assert_eq!(walk_group_members(&proc, SHELL, 999, &mut 0), None);
    // The budget is already spent.
    let mut spent = MAX_WALK_READS;
    assert_eq!(walk_group_members(&proc, SHELL, 200, &mut spent), None);
    // No `children` file to read.
    proc.no_children_file = true;
    assert_eq!(walk_group_members(&proc, SHELL, 200, &mut 0), None);
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

/// The real `/proc`: this test process's own group, found all three ways. The test runner is
/// alive and in its group for the whole test, so it must be in every answer; other members may
/// come and go between the looks, so only that much is asserted.
#[test]
fn the_real_proc_finds_this_process_in_its_own_group() {
    use super::RealProc;
    // SAFETY: both calls only read this process's ids.
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
    let walked = group_members(&RealProc, LookupMode::Walk, Some(session), pgid);
    assert!(
        matches!(walked.path, LookupPath::Walk | LookupPath::ListAfterWalk),
        "{:?}",
        walked.path
    );
    assert!(has_me(walked.members));
}
