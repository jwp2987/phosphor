use super::*;

const POSIX: [ShellType; 2] = [ShellType::Bash, ShellType::Zsh];
const ALL: [ShellType; 4] = [
    ShellType::Bash,
    ShellType::Zsh,
    ShellType::Fish,
    ShellType::PowerShell,
];

#[test]
fn posix_group_runs_in_the_current_shell_and_restores() {
    for shell in POSIX {
        let wrapped = wrap_command(Some(shell), "cd /tmp && make");
        assert!(wrapped.starts_with("{ "), "{wrapped}");
        assert!(wrapped.ends_with("; }"), "{wrapped}");
        assert!(!wrapped.starts_with('('), "{wrapped}");
        assert!(
            wrapped.contains("export PAGER=cat GIT_PAGER=cat"),
            "{wrapped}"
        );
        assert!(wrapped.contains("AWS_PAGER= "), "{wrapped}");
        assert!(wrapped.contains("; cd /tmp && make; "), "{wrapped}");
        // Status is captured immediately after the command, restored, then re-raised.
        assert!(wrapped.contains("__phosphor_ap_rc=$?; for "), "{wrapped}");
        assert!(wrapped.contains("(exit $__phosphor_ap_rc)"), "{wrapped}");
        assert!(wrapped.contains("unset __phosphor_ap_v __phosphor_ap_active"));
        assert!(
            !wrapped.contains('\n'),
            "single line stays single line: {wrapped}"
        );
    }
}

#[test]
fn every_pager_variable_is_saved_and_restored() {
    for var in PAGER_VARS {
        assert!(posix_restore().contains(var), "{var}");
        assert!(
            wrap_command(Some(ShellType::Bash), "true").contains(var),
            "{var}"
        );
    }
}

#[test]
fn multiline_heredoc_closer_is_alone_on_its_line() {
    let command = "python3 - <<'PY'\nprint('ok')\nPY";
    for shell in POSIX {
        let wrapped = wrap_command(Some(shell), command);
        let lines: Vec<&str> = wrapped.lines().collect();
        assert_eq!(lines[lines.len() - 1], "}", "{wrapped}");
        // Heredoc terminator keeps its own line, followed by our status capture line.
        assert_eq!(lines[lines.len() - 3], "PY", "{wrapped}");
        assert!(lines[lines.len() - 2].starts_with("__phosphor_ap_rc=$?;"));
    }
}

#[test]
fn trailing_background_and_semicolon_use_the_own_line_form() {
    for command in ["sleep 5 &", "echo hi;", "make 2>&1 &"] {
        for shell in POSIX {
            let wrapped = wrap_command(Some(shell), command);
            assert!(wrapped.contains(&format!("\n{command}\n")), "{wrapped}");
            assert!(wrapped.ends_with("\n}"), "{wrapped}");
        }
        let fish = wrap_command(Some(ShellType::Fish), command);
        assert!(fish.ends_with("\nend"), "{fish}");
        let pwsh = wrap_command(Some(ShellType::PowerShell), command);
        assert!(pwsh.ends_with("\n}"), "{pwsh}");
    }
}

#[test]
fn commands_containing_braces_are_wrapped_intact() {
    let command = "echo ${HOME} && if true; then echo }; fi";
    for shell in POSIX {
        let wrapped = wrap_command(Some(shell), command);
        assert!(wrapped.contains(command), "{wrapped}");
        assert!(wrapped.ends_with("; }"), "{wrapped}");
    }
    let multi = "awk '{ print $1 }' f\necho }";
    let wrapped = wrap_command(Some(ShellType::Bash), multi);
    assert!(wrapped.contains(multi));
    assert!(wrapped.ends_with("\n}"));
}

#[test]
fn trailing_comment_cannot_swallow_the_closer() {
    let command = "ls # list";
    for shell in ALL {
        let wrapped = wrap_command(Some(shell), command);
        let last = wrapped.lines().last().unwrap();
        assert!(matches!(last, "}" | "end"), "{shell:?}: {wrapped}");
    }
}

#[test]
fn fish_uses_block_scoped_exports_and_keeps_status() {
    let wrapped = wrap_command(Some(ShellType::Fish), "cd /tmp");
    assert_eq!(wrapped.lines().count(), 1);
    assert!(
        wrapped.starts_with("begin; set -lx PAGER 'cat'"),
        "{wrapped}"
    );
    assert!(wrapped.contains("set -lx AWS_PAGER ''"), "{wrapped}");
    assert!(wrapped.ends_with("; cd /tmp; end"), "{wrapped}");
    // Must not erase the user's global variables (the old prelude did).
    assert!(!wrapped.contains("set -e"), "{wrapped}");
}

#[test]
fn pwsh_saves_and_restores_process_environment_in_finally() {
    let wrapped = wrap_command(Some(ShellType::PowerShell), "Set-Location /tmp");
    assert!(wrapped.starts_with(". { $__phosphor_ap = @{}"), "{wrapped}");
    assert!(
        wrapped.contains("try { Set-Location /tmp } finally {"),
        "{wrapped}"
    );
    let exits = wrap_command(Some(ShellType::PowerShell), "exit 3");
    assert!(exits.starts_with("& { "), "{exits}");
}

#[test]
fn unsafe_commands_keep_the_subshell() {
    for command in [
        "exit 1",
        "exec ls",
        "set -e",
        "set -euo pipefail",
        "set -o errexit",
        "set +u",
        "trap 'rm x' EXIT",
        "ulimit -n 10",
        "unset PATH",
        "shopt -s failglob",
        "setopt err_exit",
        "unsetopt unset",
        "make && exit",
        "FOO=1 exec ls",
        "builtin exit",
        "\"exit\"",
        "if true; then exit 2; fi",
        "echo a\nset -e\nmake",
    ] {
        for shell in POSIX {
            assert!(needs_subshell(shell, command), "{command}");
            let wrapped = wrap_command(Some(shell), command);
            assert!(wrapped.starts_with("(unset PAGER"), "{command}: {wrapped}");
            assert!(wrapped.contains("PAGER=cat"), "{wrapped}");
            assert!(wrapped.ends_with(')'), "{wrapped}");
        }
    }
}

#[test]
fn state_changing_commands_run_in_the_group() {
    for command in [
        "cd x && make",
        "export A=1",
        "source venv/bin/activate",
        ". venv/bin/activate",
        "echo \"exit\"",
        "echo 'set -e'",
        "echo exit",
        "grep exit file.txt",
        "set -x",
        "set -- a b",
        "set -o vi",
        "shopt -s globstar",
        "setopt extendedglob",
        "unset FOO",
        "python3 - <<'PY'\nimport sys; sys.exit(1)\nexit\nPY",
        "cat <<-EOF\n\texit\n\tEOF\necho done",
        "echo hi # exit",
        "ls | grep exit",
        "git config user.name exit",
        "cargo test --exit-code",
    ] {
        for shell in POSIX {
            assert!(!needs_subshell(shell, command), "{command}");
            assert!(
                wrap_command(Some(shell), command).starts_with("{ "),
                "{command}"
            );
        }
    }
}

#[test]
fn exit_after_a_heredoc_is_still_seen() {
    let command = "cat <<EOF\nbody\nEOF\nexit 3";
    assert!(needs_subshell(ShellType::Bash, command));
}

#[test]
fn fish_never_needs_a_subshell() {
    assert!(!needs_subshell(ShellType::Fish, "exit 1"));
    assert!(wrap_command(Some(ShellType::Fish), "exit 1").starts_with("begin;"));
}

#[test]
fn unknown_shell_is_passed_through_byte_exact() {
    let command = "echo hi   \n\n";
    assert_eq!(wrap_command(None, command), command);
}
