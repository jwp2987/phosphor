use super::{
    HANGUP_GRACE, groups_to_kill, parse_proc_stat_foreground_group,
    parse_proc_stat_group_and_session, target_groups, wait_for_groups_to_exit,
};
use std::cell::Cell;
use std::time::{Duration, Instant};

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
        .write_all(b"nohup sleep 7771 >/dev/null 2>&1 &\nsleep 7772 & disown\nsleep 7773\n")
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

/// #1376: `terminate_and_reap` returns at once and the reaper still kills and reaps the shell.
#[test]
fn terminate_and_reap_returns_at_once_and_reaps_the_shell() {
    let child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id() as i32;
    super::terminate_and_reap(child);
    // Once reaped the pid no longer exists (ESRCH); a zombie would still answer signal 0.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // SAFETY: signal 0 only probes existence.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if !alive {
            break;
        }
        assert!(Instant::now() < deadline, "shell was not killed and reaped");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Runs [`wait_for_groups_to_exit`] on a fake clock: `exits_at(group)` is when that group exits
/// (`None`: it ignores SIGHUP and never does). Returns the survivors and the time spent waiting.
fn fake_wait(
    alive: Vec<i32>,
    grace: Duration,
    exits_at: impl Fn(i32) -> Option<Duration>,
) -> (Vec<i32>, Duration) {
    let start = Instant::now();
    let clock = Cell::new(start);
    let survivors = wait_for_groups_to_exit(
        alive,
        grace,
        Duration::from_millis(10),
        |g| exits_at(g).is_none_or(|at| clock.get() - start < at),
        || clock.get(),
        |d| clock.set(clock.get() + d),
    );
    (survivors, clock.get() - start)
}

/// #1685: the grace outlasts a cleanup on a loaded machine (it was 300 ms), and stays a bounded
/// give-up.
#[test]
fn hangup_grace_survives_a_loaded_machine() {
    assert!(HANGUP_GRACE >= Duration::from_secs(3));
    assert!(HANGUP_GRACE <= Duration::from_secs(5));
}

/// #1685: a program that exits within the grace (an editor saving for 2 s after SIGHUP) is never
/// killed, and the wait ends when it exits rather than at the grace.
#[test]
fn program_that_exits_within_the_grace_is_never_killed() {
    let (survivors, waited) = fake_wait(vec![700], HANGUP_GRACE, |_| Some(Duration::from_secs(2)));
    assert!(survivors.is_empty());
    assert!(waited >= Duration::from_secs(2));
    assert!(waited < HANGUP_GRACE);
    assert_eq!(groups_to_kill(&[500, 700], 500, &survivors), vec![500]);
}

/// #1685: a program that ignores SIGHUP is killed once the grace is over, and not before.
#[test]
fn program_that_ignores_sighup_is_killed_after_the_grace() {
    let (survivors, waited) = fake_wait(vec![700], HANGUP_GRACE, |_| None);
    assert_eq!(survivors, vec![700]);
    assert!(waited >= HANGUP_GRACE);
    assert!(waited < HANGUP_GRACE + Duration::from_millis(100));
    assert_eq!(groups_to_kill(&[500, 700], 500, &survivors), vec![500, 700]);
}

/// #1685: with several groups, only the one still alive at the grace is killed.
#[test]
fn only_the_groups_alive_at_the_grace_are_killed() {
    let (survivors, _) = fake_wait(vec![700, 800], HANGUP_GRACE, |g| {
        (g == 700).then_some(Duration::from_millis(500))
    });
    assert_eq!(survivors, vec![800]);
    assert_eq!(
        groups_to_kill(&[500, 700, 800], 500, &survivors),
        vec![500, 800]
    );
}

/// Nothing to wait for (no foreground group besides the shell's own): no wait at all.
#[test]
fn no_foreground_group_means_no_wait() {
    let (survivors, waited) = fake_wait(vec![], HANGUP_GRACE, |_| None);
    assert!(survivors.is_empty());
    assert_eq!(waited, Duration::ZERO);
    assert_eq!(groups_to_kill(&[500], 500, &survivors), vec![500]);
}

/// #1685: closing a pane never waits on the grace. `terminate_and_reap` hands everything to the
/// reaper thread and returns at once, even for a shell whose foreground program will take the
/// whole grace.
#[cfg(target_os = "linux")]
#[test]
fn close_returns_at_once_while_the_reaper_sits_in_the_grace() {
    let mut pty = InteractiveBash::spawn();
    let dir = TestDir::new("close_returns_at_once");
    let ready = dir.path("ready");
    pty.run(&format!(
        "sh -c 'trap \"\" HUP; echo ready > {ready}; exec sleep 7781'\n",
        ready = ready.display()
    ));
    wait_for_file(&ready);
    let shell_pid = pty.shell_pid();
    let sleeper = wait_for_sleep_in_session("7781", shell_pid);

    let start = Instant::now();
    super::terminate_and_reap(pty.take_shell());
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "terminate_and_reap blocked for {:?}",
        start.elapsed()
    );
    // The reaper still kills the SIGHUP-ignoring program once the grace is over.
    let deadline = Instant::now() + HANGUP_GRACE + Duration::from_secs(10);
    while process_is_live(sleeper) {
        assert!(
            Instant::now() < deadline,
            "SIGHUP-ignoring program survived the grace"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// #1685, real processes: a foreground program that spends a second cleaning up after SIGHUP
/// (it would have been SIGKILLed at 300 ms) finishes its cleanup.
#[cfg(target_os = "linux")]
#[test]
fn foreground_cleanup_after_sighup_is_not_killed() {
    let mut pty = InteractiveBash::spawn();
    let dir = TestDir::new("cleanup_not_killed");
    let ready = dir.path("ready");
    let saved = dir.path("saved");
    // On SIGHUP: ignore further hangups (the shell forwards one as it exits), "save" for a
    // second, then write the marker and exit.
    pty.run(&format!(
        r#"sh -c 'trap "trap \"\" HUP; sleep 1; echo saved > {saved}; exit 0" HUP; echo ready > {ready}; while :; do sleep 0.1; done'
"#,
        saved = saved.display(),
        ready = ready.display()
    ));
    wait_for_file(&ready);
    let shell_pid = pty.shell_pid();

    super::terminate_shell_tree(shell_pid);
    pty.kill_and_reap();

    let saved_text = std::fs::read_to_string(&saved).unwrap_or_default();
    assert_eq!(
        saved_text.trim(),
        "saved",
        "the foreground program was killed before it finished its cleanup"
    );
}

/// #1685, real processes: a foreground program that ignores SIGHUP is SIGKILLed after the grace,
/// and not before it.
#[cfg(target_os = "linux")]
#[test]
fn foreground_program_ignoring_sighup_is_killed_after_the_grace() {
    let mut pty = InteractiveBash::spawn();
    let dir = TestDir::new("ignoring_killed");
    let ready = dir.path("ready");
    pty.run(&format!(
        "sh -c 'trap \"\" HUP; echo ready > {ready}; exec sleep 7782'\n",
        ready = ready.display()
    ));
    wait_for_file(&ready);
    let shell_pid = pty.shell_pid();
    let sleeper = wait_for_sleep_in_session("7782", shell_pid);

    // A short grace keeps the test fast; the policy is the same as for `HANGUP_GRACE`.
    let grace = Duration::from_millis(500);
    let start = Instant::now();
    super::terminate_shell_tree_with_grace(shell_pid, grace);
    let waited = start.elapsed();
    pty.kill_and_reap();

    assert!(
        waited >= grace,
        "SIGKILL came after {waited:?}, before the {grace:?} grace"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_live(sleeper) {
        assert!(
            Instant::now() < deadline,
            "SIGHUP-ignoring program survived"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// An interactive bash on its own pty, as a pane runs it (session leader, controlling tty).
#[cfg(target_os = "linux")]
struct InteractiveBash {
    master: std::fs::File,
    shell: Option<std::process::Child>,
}

#[cfg(target_os = "linux")]
impl InteractiveBash {
    fn spawn() -> Self {
        use std::os::fd::FromRawFd;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

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
        let master = unsafe { std::fs::File::from_raw_fd(master) };
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
        let shell = cmd.spawn().expect("spawn bash");
        drop(slave);
        // Drain the pty so bash never blocks on a full output buffer.
        let mut reader = master.try_clone().expect("clone pty master");
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            while matches!(reader.read(&mut buf), Ok(n) if n > 0) {}
        });
        Self {
            master,
            shell: Some(shell),
        }
    }

    fn run(&mut self, line: &str) {
        use std::io::Write;
        self.master.write_all(line.as_bytes()).unwrap();
    }

    fn shell_pid(&self) -> u32 {
        self.shell.as_ref().expect("shell").id()
    }

    fn take_shell(&mut self) -> std::process::Child {
        self.shell.take().expect("shell")
    }

    fn kill_and_reap(&mut self) {
        if let Some(mut shell) = self.shell.take() {
            let _ = shell.kill();
            let _ = shell.wait();
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for InteractiveBash {
    fn drop(&mut self) {
        self.kill_and_reap();
    }
}

/// A scratch directory removed on drop.
#[cfg(target_os = "linux")]
struct TestDir(std::path::PathBuf);

#[cfg(target_os = "linux")]
impl TestDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "phosphor-process-tree-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create test dir");
        Self(dir)
    }

    fn path(&self, file: &str) -> std::path::PathBuf {
        self.0.join(file)
    }
}

#[cfg(target_os = "linux")]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(target_os = "linux")]
fn wait_for_file(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The pid of the `sleep <arg>` process in the session `session`.
#[cfg(target_os = "linux")]
fn wait_for_sleep_in_session(arg: &str, session: u32) -> i32 {
    let find = || -> Option<i32> {
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
            let Some(close) = stat.rfind(')') else {
                continue;
            };
            let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
            // state ppid pgrp session
            if fields.get(3).and_then(|s| s.parse::<u32>().ok()) == Some(session) {
                return entry.file_name().to_str()?.parse().ok();
            }
        }
        None
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pid) = find() {
            return pid;
        }
        assert!(Instant::now() < deadline, "sleep {arg} did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Whether `pid` is still running (a zombie counts as gone).
#[cfg(target_os = "linux")]
fn process_is_live(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rfind(')')
            .and_then(|close| stat[close + 1..].split_whitespace().next())
            .is_some_and(|state| state != "Z" && state != "X"),
        Err(_) => false,
    }
}
