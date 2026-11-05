//! Signalling the whole process tree behind a pty shell (#975).
//!
//! Each pty shell is a session leader (`setsid()` in the spawner), so its pid is also its process
//! group id. But an interactive shell with job control puts every foreground job -- `ssh`,
//! `sudo bash --rcfile ...` -- into a process group of its own, so signalling only the shell's
//! group misses exactly the processes that matter here. So we also signal the pty's foreground
//! process group. Other background groups (`nohup`/`disown`ed jobs, `app &`) are left alone.

use std::collections::HashSet;

/// Parses `(process group, session id)` out of `/proc/<pid>/stat`: `pid (comm) S ppid pgrp
/// session ...`. `comm` may itself contain spaces and parentheses, so it is delimited by the
/// first `(` and the LAST `)`.
#[cfg(any(target_os = "linux", test))]
#[allow(dead_code)] // used only by tests
pub(crate) fn parse_proc_stat_group_and_session(stat: &str) -> Option<(i32, i32)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if close < open {
        return None;
    }
    let mut rest = stat[close + 1..].split_whitespace();
    let _state = rest.next()?;
    let _ppid = rest.next()?;
    let pgrp: i32 = rest.next()?.parse().ok()?;
    let session: i32 = rest.next()?.parse().ok()?;
    Some((pgrp, session))
}

/// Parses the terminal's foreground process group (`tpgid`) out of `/proc/<pid>/stat`: the field
/// after `pgrp session tty_nr`. `-1` (no controlling terminal) yields `None`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_proc_stat_foreground_group(stat: &str) -> Option<i32> {
    let close = stat.rfind(')')?;
    let mut rest = stat[close + 1..].split_whitespace();
    // state ppid pgrp session tty_nr tpgid
    let tpgid: i32 = rest.nth(5)?.parse().ok()?;
    (tpgid > 0).then_some(tpgid)
}

/// The foreground process group of the pty controlled by the session leader `shell_pid`. This is
/// what `tcgetpgrp` on the pty master would return; the terminal server no longer holds the
/// master, so it is read from the shell's own controlling-terminal state instead.
#[cfg(target_os = "linux")]
fn foreground_group(shell_pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{shell_pid}/stat")).ok()?;
    parse_proc_stat_foreground_group(&stat)
}

#[cfg(target_os = "macos")]
fn foreground_group(shell_pid: i32) -> Option<i32> {
    // SAFETY: `info` is a plain-old-data out parameter of exactly the size passed.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let written = unsafe {
        libc::proc_pidinfo(
            shell_pid,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };
    (written == size && info.e_tpgid > 0).then_some(info.e_tpgid as i32)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn foreground_group(_shell_pid: i32) -> Option<i32> {
    None
}

/// The process groups to signal for the shell `shell_pid`: its own group plus the pty's
/// foreground group, never group 0/1 and never `own_pgrp` (this process's group, which would
/// take down the app or terminal server along with the shell).
///
/// Other background groups in the session (`nohup cmd &`, `cmd & disown`, GUI apps started with
/// `&`) are deliberately NOT targeted: closing a pane should end the shell and what is running in
/// the foreground, while background jobs are left to the kernel's normal hangup semantics (#975).
fn target_groups(shell_pid: i32, foreground: Option<i32>, own_pgrp: i32) -> Vec<i32> {
    let mut groups: Vec<i32> = std::iter::once(shell_pid)
        .chain(foreground)
        .filter(|&pgrp| pgrp > 1 && pgrp != own_pgrp)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    groups.sort_unstable();
    groups
}

fn own_pgrp() -> i32 {
    // SAFETY: `getpgrp` has no preconditions and cannot fail.
    unsafe { libc::getpgrp() }
}

fn killpg(pgrp: i32, signal: libc::c_int) -> bool {
    // SAFETY: plain signal delivery to a process group id that is not ours.
    if unsafe { libc::killpg(pgrp, signal) } == 0 {
        return true;
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() != Some(libc::ESRCH) {
        log::warn!("Failed to signal process group {pgrp} (signal {signal}): {err}");
    }
    false
}

/// Sends `signal` to the pty shell `shell_pid`'s own process group and the pty's foreground
/// process group.
///
/// The caller must make sure the shell has not been reaped yet (`Child::try_wait` returned
/// `Ok(None)`): once it is, its pid may be recycled and the group signal could land on an
/// unrelated process. Failures are logged, not returned; callers also signal the shell directly
/// (`Child::kill`) so the shell itself never depends on this succeeding.
pub(crate) fn signal_shell_tree(shell_pid: u32, signal: libc::c_int) {
    let Ok(shell_pid) = i32::try_from(shell_pid) else {
        return;
    };
    for pgrp in target_groups(shell_pid, foreground_group(shell_pid), own_pgrp()) {
        killpg(pgrp, signal);
    }
}

/// How long the shell and the foreground program get to react to SIGHUP before they are
/// SIGKILLed (#1685).
///
/// SIGHUP is the program's cue to clean up: an editor writes its swap or recovery file, git
/// finishes or rolls back its index write, a database flushes, zsh saves its HISTFILE. On a loaded
/// machine that can take seconds, and a SIGKILL in the middle of it loses or corrupts the data
/// (300 ms used to). The grace costs nothing visible: it runs on the reaper thread (see
/// [`terminate_and_reap`]), so the pane closes at once, and it ends as soon as everything hung up
/// has exited (an idle shell: a few ms) -- only a program that ignores SIGHUP waits it out.
const HANGUP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the tree is probed for exit during [`HANGUP_GRACE`]. A process group that is not our
/// child has no exit event, so it can only be probed.
const HANGUP_PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);

/// What is still alive when the hang-up wait ends: only these may be SIGKILLed.
#[derive(Debug, PartialEq, Eq)]
struct HangupSurvivors {
    /// The shell itself has not exited (it is still our unreaped child).
    shell_running: bool,
    /// The shell's own process group still has members: the running shell, or, once the shell
    /// is gone, what it ran in its own group (`exec`-less programs of a shell without job
    /// control, `$(...)` children).
    shell_group: bool,
    /// The other hung-up groups (the pty's foreground job) still alive.
    others: Vec<i32>,
}

/// Waits until the shell has exited, its own group `shell_group` has no members left, and every
/// group in `others` has exited, or until `grace` has passed, sleeping `interval` between probes.
///
/// `shell_running` answers for the shell itself and reaps it once it has exited (`try_wait`), so
/// that the zombie no longer counts as a member of its group and `group_alive(shell_group)` then
/// answers for the rest of the group only. A reaped shell's pid cannot be recycled while its
/// group still has members (the kernel keeps a pid in use as a group id), and once the group is
/// seen empty it is not probed or signalled again. When `shell_leads_group` is false (the shell
/// is not a session leader), `shell_group` is never probed: only the shell itself is waited for.
/// `group_alive` is `killpg(g, 0)`. The clock and
/// sleep are injected so the policy is testable without real processes or real time.
#[allow(clippy::too_many_arguments)]
fn wait_for_tree_to_exit(
    shell_group: i32,
    shell_leads_group: bool,
    mut others: Vec<i32>,
    grace: std::time::Duration,
    interval: std::time::Duration,
    mut shell_running: impl FnMut() -> bool,
    mut group_alive: impl FnMut(i32) -> bool,
    mut now: impl FnMut() -> std::time::Instant,
    mut sleep: impl FnMut(std::time::Duration),
) -> HangupSurvivors {
    let deadline = now() + grace;
    let mut shell = true;
    // Unknown (so assumed) while the shell runs: its own group contains it. A shell that leads no
    // group has no group of that id to probe (the id may even be someone else's group).
    let mut shell_group_members = shell_leads_group;
    loop {
        if shell {
            shell = shell_running();
        }
        if !shell && shell_group_members {
            shell_group_members = group_alive(shell_group);
        }
        others.retain(|&g| group_alive(g));
        let all_gone = !shell && !shell_group_members && others.is_empty();
        if all_gone || now() >= deadline {
            break;
        }
        sleep(interval);
    }
    HangupSurvivors {
        shell_running: shell,
        shell_group: shell || shell_group_members,
        others,
    }
}

/// The groups to SIGKILL once the grace is over: those among `targets` still alive at its end.
/// A group is not signalled again once seen gone.
fn groups_to_kill(targets: &[i32], shell_pid: i32, survivors: &HangupSurvivors) -> Vec<i32> {
    targets
        .iter()
        .copied()
        .filter(|g| {
            if *g == shell_pid {
                survivors.shell_group
            } else {
                survivors.others.contains(g)
            }
        })
        .collect()
}

/// Ends `child` (a pty shell, the session leader) and its tree, and reaps it: SIGHUP to the
/// shell's own group and the pty's foreground group (so programs can clean up), then, after at
/// most `grace`, SIGKILL to whichever of them is still alive, and a direct kill of a shell that
/// still runs. A shell, or a program in the shell's own group (`exec vim`, a launch command run
/// as the pty child, a shell without job control), gets the same grace as the foreground job.
/// Blocks for up to `grace` plus the reap: callers run it on the reaper thread
/// ([`terminate_and_reap`]), never on a loop or the UI thread.
fn terminate_tree_with_grace(child: &mut std::process::Child, grace: std::time::Duration) {
    let still_running = matches!(child.try_wait(), Ok(None));
    match i32::try_from(child.id()) {
        Ok(shell_pid) if still_running => hang_up_tree(child, shell_pid, grace),
        _ => {}
    }
    // Direct kill so the shell dies even if the group signals failed (a no-op once reaped).
    let _ = child.kill();
    if let Err(err) = child.wait() {
        log::warn!("Failed to reap pty shell {}: {err}", child.id());
    }
}

fn hang_up_tree(child: &mut std::process::Child, shell_pid: i32, grace: std::time::Duration) {
    // SAFETY: `getsid` has no preconditions; `shell_pid` is our unreaped child.
    let leads_session = unsafe { libc::getsid(shell_pid) } == shell_pid;
    if !leads_session {
        // Not a session leader (the pty spawner always calls `setsid`, so only an unusual
        // caller): it leads no process group of its id, and `/proc` tpgid would be the
        // INHERITED terminal's foreground group -- possibly our own launcher's. Hang up the
        // shell alone, give it the grace, and leave every group alone.
        // SAFETY: plain signal delivery to our own unreaped child.
        unsafe { libc::kill(shell_pid, libc::SIGHUP) };
        let survivors = wait_for_tree_to_exit(
            shell_pid,
            false,
            Vec::new(),
            grace,
            HANGUP_PROBE_INTERVAL,
            || matches!(child.try_wait(), Ok(None)),
            |g| killpg(g, 0),
            std::time::Instant::now,
            std::thread::sleep,
        );
        if survivors.shell_running {
            log::info!(
                "Pty shell {shell_pid} was still running {grace:?} after SIGHUP; killing it"
            );
        }
        // The caller kills a shell that still runs, directly.
        return;
    }
    let targets = target_groups(shell_pid, foreground_group(shell_pid), own_pgrp());
    let alive: Vec<i32> = targets
        .iter()
        .copied()
        .filter(|&g| killpg(g, libc::SIGHUP))
        .collect();
    // A stopped job would never act on SIGHUP.
    for &pgrp in &alive {
        killpg(pgrp, libc::SIGCONT);
    }
    let others: Vec<i32> = alive.into_iter().filter(|&g| g != shell_pid).collect();
    // Signal 0 only probes existence. Zombies of other groups still count, but their parent is
    // the shell or init and they are reaped promptly; the grace bounds the wait regardless.
    let survivors = wait_for_tree_to_exit(
        shell_pid,
        true,
        others,
        grace,
        HANGUP_PROBE_INTERVAL,
        || matches!(child.try_wait(), Ok(None)),
        |g| killpg(g, 0),
        std::time::Instant::now,
        std::thread::sleep,
    );
    if survivors.shell_group || !survivors.others.is_empty() {
        log::info!(
            "Pty shell {shell_pid}'s tree was still running {grace:?} after SIGHUP \
             ({survivors:?}); sending SIGKILL"
        );
    }
    for pgrp in groups_to_kill(&targets, shell_pid, &survivors) {
        killpg(pgrp, libc::SIGKILL);
    }
}

/// Ends `child`'s process tree and reaps the shell on a short-lived "PTY child reaper" thread, so
/// no caller (the pty event loop, the terminal server's single loop, the UI thread) ever sits in
/// the hang-up grace period or in `wait`. The thread owns the child, so no one else can reap it
/// while its pid is being signalled.
///
/// The shell's exit is probed with `try_wait` and finally waited for with a blocking `waitpid`;
/// non-child foreground groups can only be probed (no event source), bounded by
/// [`HANGUP_GRACE`] and off every loop.
///
/// At app quit (#1685) nothing waits for this thread: it is detached, and the host's and the
/// terminal server's quit budgets do not include it. If the process exits while a reaper is
/// still inside the grace, the thread dies with it and the SIGKILL is never sent. That is the
/// deliberate, safe outcome: the shell and the foreground program have already had their SIGHUP
/// and finish their cleanup; one that ignores SIGHUP keeps running, exactly as after closing a
/// plain terminal emulator, which hangs up and never SIGKILLs. Quit never waits on a grace and
/// never kills a program mid-save.
pub(crate) fn terminate_and_reap(child: std::process::Child) {
    terminate_and_reap_with_grace(child, HANGUP_GRACE);
}

fn terminate_and_reap_with_grace(mut child: std::process::Child, grace: std::time::Duration) {
    let work = move |grace: std::time::Duration| terminate_tree_with_grace(&mut child, grace);
    let work = std::sync::Arc::new(std::sync::Mutex::new(Some(work)));
    let thread_work = work.clone();
    let spawned = std::thread::Builder::new()
        .name("PTY child reaper".into())
        .spawn(move || {
            if let Some(mut work) = thread_work.lock().ok().and_then(|mut w| w.take()) {
                work(grace);
            }
        });
    if let Err(err) = spawned {
        // Out of threads: do it here rather than leak a zombie and a live process tree, but with
        // no grace: this is the caller's thread (a pty loop or the server's single loop), which
        // must not sit in a grace period per pane.
        log::error!("Failed to spawn the child reaper thread ({err}); reaping inline");
        if let Some(mut work) = work.lock().ok().and_then(|mut w| w.take()) {
            work(std::time::Duration::ZERO);
        }
    }
}

#[cfg(test)]
#[path = "process_tree_tests.rs"]
mod tests;
