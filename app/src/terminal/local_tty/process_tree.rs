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

/// How long the foreground program gets to react to SIGHUP before it is SIGKILLed (#1685).
///
/// SIGHUP is the program's cue to clean up: an editor writes its swap or recovery file, git
/// finishes or rolls back its index write, a database flushes. On a loaded machine that can take
/// seconds, and a SIGKILL in the middle of it loses or corrupts the data (300 ms used to). The
/// grace costs nothing visible: it runs on the reaper thread (see [`terminate_and_reap`]), so
/// the pane closes at once, and it ends as soon as the foreground group has exited -- only a
/// program that ignores SIGHUP waits it out.
const HANGUP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the foreground groups are probed for exit during [`HANGUP_GRACE`]. A process group
/// that is not our child has no exit event, so it can only be probed.
const HANGUP_PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);

/// Waits until every group in `alive` has exited (`probe` returns false for it) or `grace` has
/// passed, sleeping `interval` between probes. Returns the groups still alive at the end: the
/// only ones that may be SIGKILLed. A group that exits within the grace is dropped from the list
/// and never killed. The clock and sleep are injected so the policy is testable without real
/// processes or real time.
fn wait_for_groups_to_exit(
    mut alive: Vec<i32>,
    grace: std::time::Duration,
    interval: std::time::Duration,
    mut probe: impl FnMut(i32) -> bool,
    mut now: impl FnMut() -> std::time::Instant,
    mut sleep: impl FnMut(std::time::Duration),
) -> Vec<i32> {
    let deadline = now() + grace;
    while !alive.is_empty() && now() < deadline {
        sleep(interval);
        alive.retain(|&g| probe(g));
    }
    alive
}

/// The groups to SIGKILL once the grace is over: the shell's own group (its unreaped shell is
/// killed directly by the caller anyway, and anything else in that group goes with it) and the
/// groups that outlived the grace. A group that already exited is not signalled again, so a
/// recycled group id is never hit.
fn groups_to_kill(targets: &[i32], shell_pid: i32, survivors: &[i32]) -> Vec<i32> {
    targets
        .iter()
        .copied()
        .filter(|g| *g == shell_pid || survivors.contains(g))
        .collect()
}

/// Ends the pty shell's own group and the pty's foreground group: SIGHUP first (so programs can
/// clean up), then SIGKILL after [`HANGUP_GRACE`] if the foreground group is still alive. Same
/// reaping precondition as [`signal_shell_tree`]. Blocks for up to the grace: callers run it on
/// the reaper thread ([`terminate_and_reap`]), never on a loop or the UI thread.
pub(crate) fn terminate_shell_tree(shell_pid: u32) {
    terminate_shell_tree_with_grace(shell_pid, HANGUP_GRACE);
}

fn terminate_shell_tree_with_grace(shell_pid: u32, grace: std::time::Duration) {
    let Ok(shell_pid) = i32::try_from(shell_pid) else {
        return;
    };
    let targets = target_groups(shell_pid, foreground_group(shell_pid), own_pgrp());
    let mut alive: Vec<i32> = targets
        .iter()
        .copied()
        .filter(|&g| killpg(g, libc::SIGHUP))
        .collect();
    // A stopped foreground job would never act on SIGHUP.
    for &pgrp in &alive {
        killpg(pgrp, libc::SIGCONT);
    }
    // The shell's own group contains the (unreaped) shell, which the caller kills directly; only
    // wait on other groups.
    alive.retain(|&g| g != shell_pid);
    // Signal 0 only probes existence. Zombies still count, but their parent is the shell or init
    // and they are reaped promptly; the grace bounds the wait regardless.
    let survivors = wait_for_groups_to_exit(
        alive,
        grace,
        HANGUP_PROBE_INTERVAL,
        |g| killpg(g, 0),
        std::time::Instant::now,
        std::thread::sleep,
    );
    if !survivors.is_empty() {
        log::info!(
            "Foreground process group(s) {survivors:?} ignored SIGHUP for {grace:?}; sending SIGKILL"
        );
    }
    for pgrp in groups_to_kill(&targets, shell_pid, &survivors) {
        killpg(pgrp, libc::SIGKILL);
    }
}

/// Ends `child`'s process tree and reaps the shell on a short-lived "PTY child reaper" thread, so
/// no caller (the pty event loop, the terminal server's single loop, the UI thread) ever sits in
/// the hang-up grace period or in `wait`. The thread owns the child, so the reaping precondition
/// of [`terminate_shell_tree`] holds by construction.
///
/// The exit itself is waited for with a blocking `waitpid` (woken by the kernel, no polling);
/// the only polling left is the existence probe of non-child foreground groups inside
/// [`terminate_shell_tree`], which has no event source (a process group can only be probed, and
/// those processes are not our children), bounded by [`HANGUP_GRACE`] and now off every loop.
///
/// At app quit (#1685) nothing waits for this thread: it is detached, and the host's and the
/// terminal server's quit budgets do not include it. If the process exits while a reaper is
/// still inside the grace, the thread dies with it and the SIGKILL is never sent. That is the
/// deliberate, safe outcome: the foreground program has already had its SIGHUP and finishes its
/// cleanup; one that ignores SIGHUP keeps running, exactly as after closing a plain terminal
/// emulator, which hangs up and never SIGKILLs. Quit never waits on a grace and never kills a
/// program mid-save.
pub(crate) fn terminate_and_reap(mut child: std::process::Child) {
    let work = move || {
        if matches!(child.try_wait(), Ok(None)) {
            terminate_shell_tree(child.id());
        }
        // Direct kill so the shell dies even if the group signals failed.
        let _ = child.kill();
        if let Err(err) = child.wait() {
            log::warn!("Failed to reap pty shell {}: {err}", child.id());
        }
    };
    let work = std::sync::Arc::new(std::sync::Mutex::new(Some(work)));
    let thread_work = work.clone();
    let spawned = std::thread::Builder::new()
        .name("PTY child reaper".into())
        .spawn(move || {
            if let Some(mut work) = thread_work.lock().ok().and_then(|mut w| w.take()) {
                work();
            }
        });
    if let Err(err) = spawned {
        // Out of threads: do it here rather than leak a zombie and a live process tree.
        log::error!("Failed to spawn the child reaper thread ({err}); reaping inline");
        if let Some(mut work) = work.lock().ok().and_then(|mut w| w.take()) {
            work();
        }
    }
}

#[cfg(test)]
#[path = "process_tree_tests.rs"]
mod tests;
