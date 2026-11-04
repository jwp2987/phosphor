use super::{
    is_canonical_without_echo, is_shell_or_login_hop, parse_proc_stat, parse_proc_stat_state,
    path_is_tty, reads_tty_decision, syscall_read_fd, wchan_is_tty_read,
};
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
