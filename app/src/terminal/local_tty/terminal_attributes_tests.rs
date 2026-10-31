use super::{is_shell_or_login_hop, parse_proc_stat};

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
