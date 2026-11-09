use super::{
    combine_tty_reads, is_canonical_without_echo, is_shell_or_login_hop, parse_proc_stat,
    parse_proc_stat_state, path_is_tty, reads_tty_decision, syscall_read_fd, task_tty_read_verdict,
    wchan_is_tty_read,
};
use crate::terminal::model::block::ForegroundTtyRead;
use nix::sys::termios::LocalFlags;

#[test]
fn only_a_canonical_tty_with_echo_off_is_a_password_read() {
    let cooked = LocalFlags::ICANON | LocalFlags::ECHO;
    assert!(is_canonical_without_echo(LocalFlags::ICANON));
    assert!(is_canonical_without_echo(
        LocalFlags::ICANON | LocalFlags::ISIG
    ));
    // A running program with echo on, readline at a prompt (raw, echo off), and ssh (raw).
    assert!(!is_canonical_without_echo(cooked));
    assert!(!is_canonical_without_echo(LocalFlags::ISIG));
    assert!(!is_canonical_without_echo(LocalFlags::empty()));
}

#[test]
fn shells_and_login_hops_are_not_plain_programs() {
    for name in ["bash", "-zsh", "fish", "ssh", "sudo", "ksu", "tmux"] {
        assert!(is_shell_or_login_hop(name), "{name}");
    }
    for name in ["cat", "less", "python3", "vim", "sleep", "ncat"] {
        assert!(!is_shell_or_login_hop(name), "{name}");
    }
}

#[test]
fn parses_comm_and_process_group() {
    let stat = "1234 (less) S 1200 1200 1200 34816 1200 4194304 100 0 0 0 0 0 0 0 20 0 1 0 5 1 2";
    assert_eq!(parse_proc_stat(stat), Some(("less".to_owned(), 1200)));
}

#[test]
fn comm_may_contain_spaces_and_parentheses() {
    let stat = "42 (my (odd) prog) R 7 99 99 0 -1";
    assert_eq!(
        parse_proc_stat(stat),
        Some(("my (odd) prog".to_owned(), 99))
    );
}

#[test]
fn malformed_stat_is_none() {
    assert_eq!(parse_proc_stat(""), None);
    assert_eq!(parse_proc_stat("1 (x"), None);
    assert_eq!(parse_proc_stat("1 (x) S"), None);
}

#[test]
fn proc_stat_state_is_read_after_the_last_paren() {
    assert_eq!(parse_proc_stat_state("12 (bash) S 1 12 12 0"), Some('S'));
    assert_eq!(parse_proc_stat_state("12 (we (ird) R 1 12 12 0"), Some('R'));
    assert_eq!(parse_proc_stat_state("garbage"), None);
}

/// #1641: the tty-read detection's pure parts. `n_tty_read` is the line discipline's read;
/// `wait_woken` and `0` say nothing alone (sockets and pipes wait there too).
#[test]
fn wchan_names_only_the_tty_read() {
    assert!(wchan_is_tty_read("n_tty_read"));
    assert!(wchan_is_tty_read("n_tty_read\n"));
    assert!(wchan_is_tty_read("n_tty_read.isra.0"));
    assert!(wchan_is_tty_read("tty_read"));
    assert!(!wchan_is_tty_read("wait_woken"));
    assert!(!wchan_is_tty_read("0"));
    assert!(!wchan_is_tty_read("do_sys_poll"));
    assert!(!wchan_is_tty_read(""));
}

#[test]
fn a_blocked_read_fd_is_parsed_from_the_syscall_line() {
    assert_eq!(
        syscall_read_fd("0 0x5 0x7ffd5e1c 0x1 0x0 0x0 0x0 0x7ffd 0x7f12\n", 0),
        Some(5)
    );
    assert_eq!(
        syscall_read_fd("63 0xa 0x1 0x1 0x0 0x0 0x0 0x0 0x0", 63),
        Some(10)
    );
    // poll, running, a non-read syscall, garbage.
    assert_eq!(
        syscall_read_fd("7 0x7ffd 0x2 0xffffffff 0 0 0 0 0", 0),
        None
    );
    assert_eq!(syscall_read_fd("running", 0), None);
    assert_eq!(syscall_read_fd("", 0), None);
    assert_eq!(syscall_read_fd("0 zz", 0), None);
}

#[test]
fn only_terminal_paths_are_ttys() {
    assert!(path_is_tty("/dev/pts/3"));
    assert!(path_is_tty("/dev/tty"));
    assert!(path_is_tty("/dev/ttyS0"));
    assert!(!path_is_tty("socket:[12345]"));
    assert!(!path_is_tty("pipe:[999]"));
    assert!(!path_is_tty("/home/u/.ssh/known_hosts"));
}

#[test]
fn the_tty_read_decision_needs_the_line_discipline_or_a_sleeping_tty_read() {
    assert!(reads_tty_decision(Some("n_tty_read"), None, None));
    assert!(reads_tty_decision(
        Some("wait_woken"),
        Some('S'),
        Some("/dev/pts/4")
    ));
    assert!(reads_tty_decision(Some("0"), Some('S'), Some("/dev/tty")));
    assert!(reads_tty_decision(None, Some('S'), Some("/dev/pts/4")));
    // ssh connecting (poll), a ProxyCommand child reading a socket, a running process.
    assert!(!reads_tty_decision(Some("do_sys_poll"), Some('S'), None));
    assert!(!reads_tty_decision(
        Some("wait_woken"),
        Some('S'),
        Some("socket:[1]")
    ));
    assert!(!reads_tty_decision(
        Some("0"),
        Some('R'),
        Some("/dev/pts/4")
    ));
    assert!(!reads_tty_decision(None, None, None));
}

/// #1796 review: one task's tri-state answer. An unreadable `syscall` (or fd link) of a sleeping
/// task -- ptrace restrictions, a root-owned member under `sudo` -- is `Unknown`, never
/// `NotReading`, so the detector falls back to its long-silence window instead of never.
#[test]
fn a_task_tty_read_is_unknown_when_its_syscall_cannot_be_read() {
    use ForegroundTtyRead::{NotReading, Reading, Unknown};
    let never = |_fd: i64| -> Option<String> { panic!("no fd link is needed here") };
    let pts = |_fd: i64| Some("/dev/pts/4".to_owned());
    let pipe = |_fd: i64| Some("pipe:[9]".to_owned());
    let unreadable = |_fd: i64| -> Option<String> { None };
    let read_on_fd0 = "0 0x0 0x7ffd 0x1 0 0 0 0 0";

    assert_eq!(
        task_tty_read_verdict(Some("n_tty_read"), None, None, 0, never),
        Reading
    );
    assert_eq!(
        task_tty_read_verdict(Some("wait_woken"), Some('S'), Some(read_on_fd0), 0, pts),
        Reading
    );
    // Running, a zombie, gone: not reading.
    assert_eq!(
        task_tty_read_verdict(Some("0"), Some('R'), None, 0, never),
        NotReading
    );
    assert_eq!(
        task_tty_read_verdict(None, Some('Z'), None, 0, never),
        NotReading
    );
    assert_eq!(
        task_tty_read_verdict(None, None, None, 0, never),
        NotReading
    );
    // `sleep` (nanosleep, syscall 230 on x86_64) and a read of a pipe.
    assert_eq!(
        task_tty_read_verdict(
            Some("hrtimer_nanosleep"),
            Some('S'),
            Some("230 0x0 0x0 0x7ffd 0 0 0 0 0"),
            0,
            never
        ),
        NotReading
    );
    assert_eq!(
        task_tty_read_verdict(Some("0"), Some('S'), Some(read_on_fd0), 0, pipe),
        NotReading
    );
    // Asleep, but the kernel refused the syscall line or the fd link.
    assert_eq!(
        task_tty_read_verdict(Some("0"), Some('S'), None, 0, never),
        Unknown
    );
    assert_eq!(
        task_tty_read_verdict(Some("0"), Some('S'), Some(""), 0, never),
        Unknown
    );
    assert_eq!(
        task_tty_read_verdict(Some("0"), Some('S'), Some(read_on_fd0), 0, unreadable),
        Unknown
    );
}

/// #1796 review: the group's answer is `Reading` if ANY task reads the tty (a `make` child, a Go
/// program's reader thread), else `Unknown` if any task could not be judged, else `NotReading`.
#[test]
fn any_reading_task_makes_the_group_read_the_tty() {
    use ForegroundTtyRead::{NotReading, Reading, Unknown};
    assert_eq!(combine_tty_reads([NotReading, Reading, Unknown]), Reading);
    assert_eq!(combine_tty_reads([Unknown, NotReading, Reading]), Reading);
    assert_eq!(combine_tty_reads([NotReading, Unknown]), Unknown);
    assert_eq!(combine_tty_reads([NotReading, NotReading]), NotReading);
    assert_eq!(
        combine_tty_reads(Vec::new()),
        NotReading,
        "an exited group reads nothing"
    );
}

/// #1679 step 3: the subshell readiness sample on a real pty. The probe is exercised against a pty
/// pair this test owns: no shell behind it, so the leader is unreadable and the kind `Unknown`, but
/// the consumption proof (R1) reads the real slave input queue -- empty, then holding a byte the
/// master wrote, then empty again once the slave read it; the flush (R9) empties it without a
/// read.
#[cfg(target_os = "linux")]
#[test]
fn the_slave_input_queue_proof_and_the_flush_read_a_real_pty() {
    use super::{flush_slave_input, slave_input_queue_empty, subshell_tty_fact};
    use crate::terminal::model::subshell_tty::{LeaderKind, SubshellTtyKind};
    // SAFETY: a fresh pty pair, both fds closed at the end.
    let (master, slave) = unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        assert!(master >= 0, "posix_openpt");
        assert_eq!(libc::grantpt(master), 0);
        assert_eq!(libc::unlockpt(master), 0);
        let mut name = [0 as libc::c_char; 64];
        assert_eq!(libc::ptsname_r(master, name.as_mut_ptr(), name.len()), 0);
        let slave = libc::open(name.as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
        assert!(slave >= 0, "open slave");
        (master, slave)
    };
    assert_eq!(slave_input_queue_empty(master), Some(true), "fresh pty");
    // The pty's foreground group is nobody's: no leader, the parent's pid is irrelevant.
    let sample = subshell_tty_fact(master, std::process::id() as i32);
    assert_eq!(sample.kind, SubshellTtyKind::Unknown, "{sample:?}");
    assert_ne!(sample.leader, LeaderKind::Shell, "{sample:?}");
    // SAFETY: a write of one byte from a valid buffer to an fd this test owns.
    assert_eq!(unsafe { libc::write(master, b"x".as_ptr().cast(), 1) }, 1);
    // The byte sits in the slave's input queue until something reads it: the proof says so.
    let mut waited = 0;
    while slave_input_queue_empty(master) != Some(false) && waited < 200 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        waited += 1;
    }
    assert_eq!(slave_input_queue_empty(master), Some(false), "a byte waits");
    assert!(flush_slave_input(master), "the flush is issued");
    assert_eq!(
        slave_input_queue_empty(master),
        Some(true),
        "flushed, unread"
    );
    // Written again and read by the slave: consumed.
    // SAFETY: as above; the read goes into a one-byte buffer this test owns.
    unsafe {
        assert_eq!(libc::write(master, b"y".as_ptr().cast(), 1), 1);
        let mut byte = 0u8;
        let mut waited = 0;
        while slave_input_queue_empty(master) != Some(false) && waited < 200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            waited += 1;
        }
        assert_eq!(libc::read(slave, (&mut byte as *mut u8).cast(), 1), 1);
        assert_eq!(byte, b'y');
    }
    assert_eq!(slave_input_queue_empty(master), Some(true), "consumed");
    // SAFETY: fds this test opened.
    unsafe {
        libc::close(slave);
        libc::close(master);
    }
}
