use super::{
    EDITOR_LESS_SHELL_LEADERS, LeaderKind, RELAY_LEADERS, SHELL_LEADERS, UNSUPPORTED_LEADERS,
    classify_leader, comm_from_proc_stat, leader_has_line_editor, shell_type_of_leader,
};
use crate::terminal::shell::ShellType;

#[test]
fn every_listed_leader_classifies_as_its_list_and_the_lists_do_not_overlap() {
    for name in SHELL_LEADERS {
        assert_eq!(classify_leader(name), LeaderKind::Shell, "{name}");
        assert!(!RELAY_LEADERS.contains(name) && !UNSUPPORTED_LEADERS.contains(name));
    }
    for name in RELAY_LEADERS {
        assert_eq!(classify_leader(name), LeaderKind::Relay, "{name}");
        assert!(!UNSUPPORTED_LEADERS.contains(name));
    }
    for name in UNSUPPORTED_LEADERS {
        assert_eq!(classify_leader(name), LeaderKind::Unsupported, "{name}");
    }
    for name in EDITOR_LESS_SHELL_LEADERS {
        assert!(SHELL_LEADERS.contains(name), "{name} is a shell");
    }
}

#[test]
fn login_shells_case_and_exe_suffixes_are_normalized() {
    assert_eq!(classify_leader("-bash"), LeaderKind::Shell);
    assert_eq!(classify_leader("-zsh"), LeaderKind::Shell);
    assert_eq!(classify_leader("Pwsh.exe"), LeaderKind::Shell);
    assert_eq!(classify_leader("SUDO"), LeaderKind::Relay);
    assert_eq!(classify_leader("-tcsh"), LeaderKind::Unsupported);
}

#[test]
fn programs_that_are_neither_are_other() {
    for name in [
        "vim", "fzf", "less", "python3", "node", "cargo", "sleep", "cat", "irb", "psql",
    ] {
        assert_eq!(classify_leader(name), LeaderKind::Other, "{name}");
    }
}

#[test]
fn only_editor_less_shells_may_be_written_at_a_cooked_prompt() {
    for name in ["dash", "sh", "-sh", "ash", "busybox", "ksh", "mksh"] {
        assert!(!leader_has_line_editor(name), "{name}");
    }
    // A cooked bash/zsh/fish/nu/pwsh is a `read` builtin (R3); an unknown name is never written.
    for name in ["bash", "-zsh", "fish", "nu", "pwsh", "vim", "unknown-shell"] {
        assert!(leader_has_line_editor(name), "{name}");
    }
}

#[test]
fn the_leaders_shell_type_names_the_scripted_shells_only() {
    assert_eq!(shell_type_of_leader("-bash"), Some(ShellType::Bash));
    assert_eq!(shell_type_of_leader("zsh"), Some(ShellType::Zsh));
    assert_eq!(shell_type_of_leader("fish"), Some(ShellType::Fish));
    assert_eq!(shell_type_of_leader("nu"), Some(ShellType::Nu));
    assert_eq!(shell_type_of_leader("nushell"), Some(ShellType::Nu));
    assert_eq!(shell_type_of_leader("pwsh"), Some(ShellType::PowerShell));
    assert_eq!(
        shell_type_of_leader("powershell.exe"),
        Some(ShellType::PowerShell)
    );
    for probe_path in ["dash", "sh", "ash", "busybox", "ksh", "sudo", "ssh", "vim"] {
        assert_eq!(shell_type_of_leader(probe_path), None, "{probe_path}");
    }
}

/// #1937 (review r8 D6): the parent's pid in front under ANOTHER comm than the one it had when the
/// shell reported it is an `exec` (`exec zsh` from bash): the leader is then classified by its comm
/// (a shell, a relay, a program) instead of reading as the parent. The same comm, or a comm that
/// was never read / cannot be read now, is the parent as before.
#[test]
fn a_parent_pid_under_another_comm_was_replaced_by_an_exec() {
    use super::parent_was_replaced;
    for (at_report, now, replaced) in [
        ("bash", "zsh", true),
        ("bash", "fish", true),
        ("zsh", "bash", true),
        ("bash", "dash", true),
        ("bash", "ssh", true),
        ("bash", "vim", true),
        // `exec bash` from bash: not told apart (stays the parent).
        ("bash", "bash", false),
        ("zsh", "zsh", false),
        ("bash", " bash\n", false),
        // Nothing to compare.
        ("", "zsh", false),
        ("bash", "", false),
        ("", "", false),
    ] {
        assert_eq!(
            parent_was_replaced(at_report, now),
            replaced,
            "{at_report:?} -> {now:?}"
        );
    }
    // What the replaced leader is, by its comm: the sample the judge then gets.
    assert_eq!(classify_leader("zsh"), LeaderKind::Shell);
    assert_eq!(shell_type_of_leader("zsh"), Some(ShellType::Zsh));
    assert_eq!(classify_leader("ssh"), LeaderKind::Relay);
    assert_eq!(classify_leader("vim"), LeaderKind::Other);
}

#[test]
fn the_comm_is_between_the_first_open_and_the_last_close_parenthesis() {
    assert_eq!(
        comm_from_proc_stat("1234 (bash) S 1200 1234 1200 34816 1234 4194304 100"),
        Some("bash".to_owned())
    );
    assert_eq!(
        comm_from_proc_stat("42 (my (odd) prog) R 7 99 99 0 -1"),
        Some("my (odd) prog".to_owned())
    );
    assert_eq!(
        comm_from_proc_stat("9 (a) b) S 1 9"),
        Some("a) b".to_owned())
    );
    assert_eq!(comm_from_proc_stat("77 () S 1 77"), None);
    assert_eq!(comm_from_proc_stat("no parens here"), None);
    assert_eq!(comm_from_proc_stat(") 5 ("), None);
}
