use super::{parse_proc_stat_foreground_group, parse_proc_stat_group_and_session, target_groups};

#[test]
fn parses_group_and_session() {
    let stat = "1234 (ssh) S 1200 1300 1200 34816 1300 4194304 100 0 0 0 0 0 0 0 20 0 1 0 5 1 2";
    assert_eq!(parse_proc_stat_group_and_session(stat), Some((1300, 1200)));
}

#[test]
fn comm_may_contain_spaces_and_parentheses() {
    let stat = "42 (my (odd) prog) R 7 99 55 0 -1";
    assert_eq!(parse_proc_stat_group_and_session(stat), Some((99, 55)));
}

#[test]
fn malformed_stat_is_none() {
    assert_eq!(parse_proc_stat_group_and_session(""), None);
    assert_eq!(parse_proc_stat_group_and_session("1 (x"), None);
    assert_eq!(parse_proc_stat_group_and_session("1 (x) S 1 2"), None);
}

#[test]
fn parses_foreground_group() {
    let stat = "1234 (bash) S 1200 1300 1200 34816 4242 4194304 100";
    assert_eq!(parse_proc_stat_foreground_group(stat), Some(4242));
    let no_tty = "1234 (bash) S 1200 1300 1200 0 -1 4194304 100";
    assert_eq!(parse_proc_stat_foreground_group(no_tty), None);
    assert_eq!(parse_proc_stat_foreground_group("1 (x) S 1 2"), None);
}

#[test]
fn targets_shell_and_foreground_groups_only() {
    assert_eq!(target_groups(500, Some(700), 9999), vec![500, 700]);
    assert_eq!(target_groups(500, Some(500), 9999), vec![500]);
}

#[test]
fn never_targets_own_group_or_init() {
    assert_eq!(target_groups(500, Some(42), 42), vec![500]);
    assert_eq!(target_groups(500, Some(1), 9999), vec![500]);
    assert_eq!(target_groups(500, Some(0), 9999), vec![500]);
}

#[test]
fn shell_group_is_targeted_without_a_foreground_group() {
    assert_eq!(target_groups(500, None, 9999), vec![500]);
}

/// The real path: an interactive bash on a pty with a `nohup` job, a `disown`ed job and a
/// foreground program. Closing the pane must end the foreground program only.
#[cfg(target_os = "linux")]
#[test]
fn pane_kill_ends_foreground_but_spares_nohup_and_disowned_jobs() {
    use std::io::Write;
    use std::os::fd::FromRawFd;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    if !std::path::Path::new("/bin/bash").exists() {
        return;
    }
    let (mut master, mut slave) = (0, 0);
    // SAFETY: out parameters are valid; null name/termios/winsize are allowed.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(rc, 0, "openpty failed");
    // SAFETY: freshly opened, owned descriptors.
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    let mut cmd = Command::new("/bin/bash");
    cmd.args(["--norc", "--noprofile", "-i"])
        .env("TERM", "dumb")
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::from(slave.try_clone().unwrap()));
    // SAFETY: only async-signal-safe calls between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut shell = cmd.spawn().expect("spawn bash");
    drop(slave);
    let shell_pid = shell.id();
    master
        .write_all(
            b"nohup sleep 7771 >/dev/null 2>&1 &\nsleep 7772 & disown\nsleep 7773\n",
        )
        .unwrap();

    // (pid, state) of the processes in the shell's session whose command line is `sleep <arg>`.
    let find = |arg: &str| -> Option<char> {
        for entry in std::fs::read_dir("/proc").ok()?.flatten() {
            let path = entry.path();
            let Ok(cmdline) = std::fs::read(path.join("cmdline")) else {
                continue;
            };
            if cmdline != format!("sleep\0{arg}\0").as_bytes() {
                continue;
            }
            let Ok(stat) = std::fs::read_to_string(path.join("stat")) else {
                continue;
            };
            let close = stat.rfind(')')?;
            let mut rest = stat[close + 1..].split_whitespace();
            let state = rest.next()?.chars().next()?;
            let _ppid = rest.next()?;
            let _pgrp = rest.next()?;
            let session: u32 = rest.next()?.parse().ok()?;
            if session == shell_pid {
                return Some(state);
            }
        }
        None
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while ["7771", "7772", "7773"].iter().any(|a| find(a).is_none()) {
        assert!(Instant::now() < deadline, "jobs did not start");
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(matches!(shell.try_wait(), Ok(None)));
    super::terminate_shell_tree(shell_pid);
    let _ = shell.kill();
    let _ = shell.wait();

    let dead = |s: Option<char>| s.is_none() || s == Some('Z');
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dead(find("7773")) {
        assert!(
            Instant::now() < deadline,
            "foreground program survived the pane kill"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let nohup = find("7771");
    let disowned = find("7772");
    // Clean up before asserting.
    for arg in ["7771", "7772"] {
        let _ = Command::new("pkill")
            .args(["-f", &format!("sleep {arg}")])
            .status();
    }
    assert!(!dead(nohup), "nohup job was killed with the pane");
    assert!(!dead(disowned), "disowned job was killed with the pane");
}
