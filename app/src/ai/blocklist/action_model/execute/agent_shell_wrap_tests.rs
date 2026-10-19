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
        // The command is the left side of an `&&` / `||` list (errexit-exempt); its status is
        // captured immediately, the pagers restored, then the status re-raised -- but only
        // when no errexit option is on, since re-raising would exit the user's shell.
        assert!(
            wrapped.contains(
                "; { cd /tmp && make; } && __phosphor_ap_rc=0 || __phosphor_ap_rc=$?; for "
            ),
            "{wrapped}"
        );
        assert!(wrapped.contains("(exit $__phosphor_ap_rc)"), "{wrapped}");
        assert!(wrapped.contains("case $- in *e*) true;;"), "{wrapped}");
        assert_eq!(
            wrapped.contains("[[ -o errreturn ]]"),
            shell == ShellType::Zsh,
            "{wrapped}"
        );
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
        assert!(
            lines[lines.len() - 2]
                .starts_with("} && __phosphor_ap_rc=0 || __phosphor_ap_rc=$?; for "),
            "{wrapped}"
        );
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
        wrapped.contains(&format!(
            "try {{ Set-Location /tmp; {PWSH_CAPTURE} }} finally {{"
        )),
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

#[test]
fn errexit_cannot_see_the_agent_command_or_the_reraise() {
    // The command must sit in an `&&` / `||` list so `set -e` / `ERR_EXIT` / `ERR_RETURN`
    // are suppressed for it, and the failing re-raise must be guarded by an errexit test.
    for shell in POSIX {
        for command in ["false", "true && false", "a\nb"] {
            let wrapped = wrap_command(Some(shell), command);
            let status = "} && __phosphor_ap_rc=0 || __phosphor_ap_rc=$?";
            assert!(wrapped.contains(status), "{wrapped}");
            let reraise = wrapped.find("(exit $__phosphor_ap_rc)").unwrap();
            let guard = wrapped.find("case $- in *e*) true;;").unwrap();
            assert!(guard < reraise, "{wrapped}");
            assert!(wrapped.find(status).unwrap() < guard, "{wrapped}");
            // The pagers are restored before anything that could return early.
            assert!(
                wrapped
                    .find("unset __phosphor_ap_v __phosphor_ap_active")
                    .unwrap()
                    < guard,
                "{wrapped}"
            );
        }
    }
}

#[test]
fn pwsh_captures_status_inside_try_and_restores_it_last() {
    for command in ["Get-Item /nope", "Get-Item /nope\nls", "ls # x"] {
        let wrapped = wrap_command(Some(ShellType::PowerShell), command);
        let capture = wrapped.find(PWSH_CAPTURE).expect("status is captured");
        let finally = wrapped.find("} finally {").unwrap();
        assert!(capture < finally, "captured inside the try: {wrapped}");
        let reraise = wrapped.rfind(PWSH_RERAISE).expect("status is put back");
        assert!(reraise > finally, "re-raised after the finally: {wrapped}");
        assert!(wrapped.ends_with("}"), "{wrapped}");
        assert!(
            wrapped[reraise + PWSH_RERAISE.len()..]
                .trim_start()
                .starts_with('}')
        );
    }
    // `$?` first, then `$LASTEXITCODE` only through `Test-Path`: a bare read throws under
    // `Set-StrictMode` when no native command has run yet.
    assert!(PWSH_CAPTURE.starts_with("$__phosphor_ap_ok = $?;"));
    assert!(PWSH_CAPTURE.contains("Test-Path variable:global:LASTEXITCODE"));
    assert!(!PWSH_CAPTURE.replace("$global:LASTEXITCODE", "").contains("$LASTEXITCODE"));
    assert!(PWSH_RERAISE.contains("$global:LASTEXITCODE = if (Test-Path variable:__phosphor_ap_ec)"));
}

#[test]
fn trailing_backslash_cannot_swallow_the_closer() {
    for shell in POSIX {
        let wrapped = wrap_command(Some(shell), "ls /x \\");
        // The continuation joins the next (blank) line, so the closer is not consumed.
        assert!(wrapped.contains("\nls /x \\\n\n} && "), "{wrapped}");
        assert!(wrapped.ends_with("\n}"), "{wrapped}");
    }
    let fish = wrap_command(Some(ShellType::Fish), "ls /x \\");
    assert!(fish.contains("ls /x \\\n\nend"), "{fish}");
}

#[test]
fn eval_and_noglob_forward_to_the_command_they_run() {
    for command in [
        "eval exit",
        "eval exit 3",
        "eval \"set -e\"",
        "eval 'trap true EXIT'",
        "noglob exit",
    ] {
        for shell in POSIX {
            assert!(needs_subshell(shell, command), "{command}");
        }
    }
    for command in ["eval echo hi", "eval \"cd /tmp\"", "noglob ls *", "eval"] {
        for shell in POSIX {
            assert!(!needs_subshell(shell, command), "{command}");
        }
    }
}

#[test]
fn arithmetic_shifts_are_not_heredocs() {
    // If `<<` here opened a heredoc, the `exit` on the next line would be swallowed as body.
    for command in [
        "echo $((1<<2))\nexit 1",
        "(( x << 2 ))\nexit 1",
        "x=$(( (1<<2) + (3<<1) ))\nexit 1",
        "(( x <<= 2 ))\nexit 1",
    ] {
        assert!(needs_subshell(ShellType::Bash, command), "{command}");
    }
    // A real heredoc still hides its body.
    assert!(!needs_subshell(
        ShellType::Bash,
        "cat <<EOF\nexit 1\nEOF\necho done"
    ));
}

#[test]
fn ansi_c_quoting_keeps_escaped_quotes_inside_the_word() {
    // `\'` does not end the quote, so `; exit` below is inside the string.
    assert!(!needs_subshell(ShellType::Bash, "echo $'it\\'s; exit 1'"));
    assert!(needs_subshell(ShellType::Bash, "echo $'it\\'s'; exit 1"));
    assert!(needs_subshell(ShellType::Bash, "echo $'a'\nexit 1"));
}
