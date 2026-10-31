use super::{is_canonical_without_echo, is_shell_or_login_hop, parse_proc_stat};
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
