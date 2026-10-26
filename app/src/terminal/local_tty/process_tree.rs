//! Signalling the whole process tree behind a pty shell (#975).
//!
//! Each pty shell is a session leader (`setsid()` in the spawner), so its pid is also its process
//! group id. But an interactive shell with job control puts every foreground job -- `ssh`,
//! `sudo bash --rcfile ...` -- into a process group of its own, so signalling only the shell's
//! group misses exactly the processes that matter here. Every process the shell starts stays in
//! the shell's *session*, so on Linux we also signal each process group in that session.

use std::collections::HashSet;

/// Parses `(process group, session id)` out of `/proc/<pid>/stat`: `pid (comm) S ppid pgrp
/// session ...`. `comm` may itself contain spaces and parentheses, so it is delimited by the
/// first `(` and the LAST `)`.
#[cfg(any(target_os = "linux", test))]
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

/// The process groups of every process in session `session_id`, found by scanning `/proc`.
#[cfg(target_os = "linux")]
fn process_groups_in_session(session_id: i32) -> HashSet<i32> {
    let mut groups = HashSet::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return groups;
    };
    for entry in entries.flatten() {
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
        if let Some((pgrp, session)) = parse_proc_stat_group_and_session(&stat)
            && session == session_id
        {
            groups.insert(pgrp);
        }
    }
    groups
}

/// Without `/proc` (macOS) only the shell's own group can be found.
#[cfg(not(target_os = "linux"))]
fn process_groups_in_session(_session_id: i32) -> HashSet<i32> {
    HashSet::new()
}

/// The process groups to signal for the shell `shell_pid`: its own group plus every group in its
/// session, never group 0/1 and never `own_pgrp` (this process's group, which would take down
/// the app or terminal server along with the shell).
fn target_groups(shell_pid: i32, session_groups: HashSet<i32>, own_pgrp: i32) -> Vec<i32> {
    let mut groups: Vec<i32> = std::iter::once(shell_pid)
        .chain(session_groups)
        .filter(|&pgrp| pgrp > 1 && pgrp != own_pgrp)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    groups.sort_unstable();
    groups
}

/// Sends `signal` to the pty shell `shell_pid` and everything it started.
///
/// The caller must make sure the shell has not been reaped yet (`Child::try_wait` returned
/// `Ok(None)`): once it is, its pid may be recycled and the group signal could land on an
/// unrelated process. Failures are logged, not returned; callers also signal the shell directly
/// (`Child::kill`) so the shell itself never depends on this succeeding.
pub(crate) fn signal_shell_tree(shell_pid: u32, signal: libc::c_int) {
    let Ok(shell_pid) = i32::try_from(shell_pid) else {
        return;
    };
    // SAFETY: `getpgrp` has no preconditions and cannot fail.
    let own_pgrp = unsafe { libc::getpgrp() };
    for pgrp in target_groups(shell_pid, process_groups_in_session(shell_pid), own_pgrp) {
        // SAFETY: plain signal delivery to a process group id that is not ours.
        if unsafe { libc::killpg(pgrp, signal) } != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::ESRCH) {
                log::warn!("Failed to signal process group {pgrp} (signal {signal}): {err}");
            }
        }
    }
}

#[cfg(test)]
#[path = "process_tree_tests.rs"]
mod tests;
