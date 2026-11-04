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
        assert!(
            wrapped.contains("(builtin exit $__phosphor_ap_rc)"),
            "{wrapped}"
        );
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
fn guard_save_forks_only_when_the_user_has_an_exit_or_logout_function() {
    // #1283: every command substitution is a fork before the agent's command starts; the saved
    // text is only needed when there is a function to save, which `typeset -f` answers in-process.
    for shell in POSIX {
        let wrapped = wrap_command(Some(shell), "echo hi");
        for (name, var) in [("exit", "__phosphor_ap_gx"), ("logout", "__phosphor_ap_gl")] {
            let subst = format!("$(typeset -f {name} 2>/dev/null)");
            let at = wrapped
                .find(&subst)
                .unwrap_or_else(|| panic!("{shell:?}: no save of {name}: {wrapped}"));
            let test = format!("if typeset -f {name} >/dev/null 2>&1; then {var}=");
            assert!(
                wrapped[..at].ends_with(&test),
                "{shell:?}: the {name} save is not behind its test: {wrapped}"
            );
        }
        assert_eq!(wrapped.matches("$(").count(), 2, "{shell:?}: {wrapped}");
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
        // The command ends its own line; the guard restore and `end` follow on the next one.
        assert!(
            fish.contains(&format!("\n{command}\nset -l __phosphor_ap_st")),
            "{fish}"
        );
        assert!(fish.ends_with("; end"), "{fish}");
        let pwsh = wrap_command(Some(ShellType::PowerShell), command);
        assert!(pwsh.contains("\n}\n$global:LASTEXITCODE"), "{pwsh}");
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
        // pwsh's last line is the status re-raise, which comes after the closing `}`.
        if shell == ShellType::Fish {
            // The command's last line is followed by a newline before anything of ours.
            assert!(
                wrapped.contains("\nls # list\nset -l __phosphor_ap_st"),
                "{wrapped}"
            );
            continue;
        }
        let closer = if shell == ShellType::PowerShell {
            let lines: Vec<&str> = wrapped.lines().collect();
            lines[lines.len() - 2]
        } else {
            wrapped.lines().last().unwrap()
        };
        assert!(matches!(closer, "}" | "end"), "{shell:?}: {wrapped}");
    }
}

#[test]
fn fish_uses_block_scoped_exports_and_keeps_status() {
    let wrapped = wrap_command(Some(ShellType::Fish), "cd /tmp");
    assert_eq!(wrapped.lines().count(), 1);
    assert!(
        wrapped.starts_with(" begin; set -lx PAGER 'cat'"),
        "{wrapped}"
    );
    assert!(wrapped.contains("set -lx AWS_PAGER ''"), "{wrapped}");
    assert!(
        wrapped.contains("; cd /tmp; set -l __phosphor_ap_st $status;"),
        "{wrapped}"
    );
    assert!(
        wrapped.ends_with("command sh -c \"exit $__phosphor_ap_st\"; end"),
        "{wrapped}"
    );
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
    assert!(exits.starts_with(". { "), "{exits}");
    assert!(exits.contains("-EncodedCommand "), "{exits}");
}

#[test]
fn pwsh_commands_that_end_the_host_run_in_a_child_pwsh() {
    use base64::Engine as _;
    for command in [
        "exit",
        "exit 3",
        "[Environment]::Exit(5)",
        "[System.Environment]::Exit(2)",
        "$host.SetShouldExit(4)",
        "Stop-Process -Id $PID",
        "Set-Location /tmp; exit 1",
        "if ($x) { exit 2 }",
        "Invoke-Expression 'exit 3'",
        "iex 'exit'",
    ] {
        assert!(needs_subshell(ShellType::PowerShell, command), "{command}");
        let wrapped = wrap_command(Some(ShellType::PowerShell), command);
        assert!(wrapped.starts_with(". { "), "{wrapped}");
        assert!(
            wrapped.contains("-NoProfile -NoLogo -NonInteractive -EncodedCommand "),
            "{wrapped}"
        );
        assert!(wrapped.contains("__phosphor_ap"), "{wrapped}");
        // The command itself is carried intact, after a Set-Location to the caller's directory.
        let encoded = wrapped
            .split("-EncodedCommand ")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .unwrap()
            .trim();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let decoded = String::from_utf16(&units).unwrap();
        assert!(decoded.ends_with(command), "{decoded}");
        assert!(
            decoded.contains("Set-Location -LiteralPath $env:__PHOSPHOR_AP_CWD"),
            "{decoded}"
        );
    }
    for command in [
        "Set-Location /tmp",
        "$env:FOO = 'bar'",
        "Write-Output exit",
        "ls",
    ] {
        assert!(!needs_subshell(ShellType::PowerShell, command), "{command}");
        assert!(!wrap_command(Some(ShellType::PowerShell), command).contains("EncodedCommand"));
    }
}

#[test]
fn pwsh_nested_interactive_shells_are_refused() {
    for command in [
        "pwsh",
        "& pwsh",
        "pwsh -NoProfile",
        "powershell.exe -NoLogo -NoExit",
        "& 'pwsh' -NoExit",
    ] {
        assert!(
            exec_into_shell(Some(ShellType::PowerShell), command).is_some(),
            "{command}"
        );
    }
    for command in [
        "pwsh -Command 'echo hi'",
        "pwsh script.ps1",
        "ls",
        "echo pwsh",
    ] {
        assert_eq!(
            exec_into_shell(Some(ShellType::PowerShell), command),
            None,
            "{command}"
        );
    }
}

#[test]
fn pwsh_wrapper_calls_are_module_qualified_in_the_bootstrap() {
    // #944: a user function or alias named like a cmdlet must not be reached.
    for file in [
        include_str!("../../../../../assets/bundled/bootstrap/pwsh.ps1"),
        include_str!("../../../../../assets/bundled/bootstrap/pwsh_init_shell.ps1"),
        include_str!("../../../../../assets/bundled/bootstrap/pwsh_init_subshell.ps1"),
    ] {
        for line in file.lines().filter(|l| !l.trim_start().starts_with('#')) {
            for bare in [
                "Write-Output",
                "Write-Host",
                "Set-Location",
                "Get-Location",
                "Get-Command",
                "Remove-Module",
            ] {
                let mut rest = line;
                while let Some(i) = rest.find(bare) {
                    let before = &rest[..i];
                    let after = &rest[i + bare.len()..];
                    let qualified = before.ends_with('\\');
                    let in_name = before.ends_with(|c: char| c.is_alphanumeric() || c == '-')
                        || after.starts_with(|c: char| c.is_alphanumeric() || c == '-');
                    let in_string = before.matches('\'').count() % 2 == 1;
                    assert!(qualified || in_name || in_string, "bare {bare}: {line}");
                    rest = after;
                }
            }
        }
    }
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
            assert!(
                wrapped.starts_with(": __phosphor_ap_subshell; (unset PAGER"),
                "{command}: {wrapped}"
            );
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
    assert!(needs_subshell(ShellType::Fish, "exit 1"));
    assert!(wrap_command(Some(ShellType::Fish), "exit 1").starts_with(" begin;"));
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
            let reraise = wrapped.find("(builtin exit $__phosphor_ap_rc)").unwrap();
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
        // At the top level of the line, after the block: `$?` after a dot-sourced block is
        // always true.
        assert!(wrapped.ends_with(PWSH_RERAISE), "{wrapped}");
        assert!(
            wrapped[..reraise]
                .trim_end()
                .trim_end_matches(';')
                .ends_with('}'),
            "{wrapped}"
        );
    }
    // `$?` first, then `$LASTEXITCODE` only through `Test-Path`: a bare read throws under
    // `Set-StrictMode` when no native command has run yet.
    assert!(PWSH_CAPTURE.starts_with("$__phosphor_ap_ok = $?;"));
    assert!(
        PWSH_CAPTURE
            .contains("Microsoft.PowerShell.Management\\Test-Path variable:global:LASTEXITCODE")
    );
    assert!(
        !PWSH_CAPTURE
            .replace("$global:LASTEXITCODE", "")
            .contains("$LASTEXITCODE")
    );
    assert!(PWSH_RERAISE.contains("$global:LASTEXITCODE = if (Microsoft.PowerShell.Management\\Test-Path variable:__phosphor_ap_ec)"));
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
    assert!(
        fish.contains("ls /x \\\n\nset -l __phosphor_ap_st"),
        "{fish}"
    );
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

#[test]
fn exec_into_shell_is_detected_at_the_top_level() {
    for (command, expected) in [
        ("exec bash", Some("bash")),
        ("exec zsh -l", Some("zsh")),
        ("exec /usr/bin/fish", Some("fish")),
        ("exec -l bash", Some("bash")),
        ("exec -a login bash", Some("bash")),
        ("exec env FOO=1 bash", Some("bash")),
        ("cd /tmp && exec bash", Some("bash")),
        ("echo hi\nexec sh\n", Some("sh")),
        ("(exec bash)", Some("bash")),
        ("exec \"$SHELL\"", Some("$SHELL")),
        ("FOO=1 exec bash", Some("bash")),
        ("exec bash -l", Some("bash")),
        ("exec bash --login -i", Some("bash")),
        ("exec \"${SHELL:-/bin/sh}\"", Some("$SHELL")),
        ("exec ${SHELL}", Some("$SHELL")),
        ("exec $(command -v zsh)", Some("$SHELL")),
        ("exec \"$(which fish)\"", Some("$SHELL")),
        ("exec `which fish`", Some("$SHELL")),
        ("eval 'exec bash'", Some("bash")),
        ("eval exec zsh", Some("zsh")),
        ("exec nice bash", Some("bash")),
        ("exec sudo -i", Some("sudo")),
        ("exec doas sh", Some("sh")),
        ("exec su", Some("su")),
        // Runs something and ends.
        ("exec bash -c 'echo hi'", None),
        ("exec sh script.sh", None),
        ("exec env X=1 sh run.sh", None),
        ("exec zsh -lc 'make'", None),
        ("exec sudo make install", None),
        ("exec $EDITOR file.txt", None),
        ("eval 'echo hi'", None),
        // Not a shell, or no program at all: these end on their own.
        ("exec ls -la", None),
        ("exec python3 script.py", None),
        ("exec 3<file", None),
        ("exec >log.txt", None),
        ("exec", None),
        ("echo exec bash", None),
        ("bash -c 'exec bash'", None),
        ("# exec bash", None),
    ] {
        for shell in [
            Some(ShellType::Bash),
            Some(ShellType::Zsh),
            Some(ShellType::Fish),
            None,
        ] {
            assert_eq!(
                exec_into_shell(shell, command).as_deref(),
                expected,
                "{command:?} under {shell:?}"
            );
        }
    }
    // PowerShell has no `exec`; its nested-shell trap is covered by
    // `pwsh_nested_interactive_shells_are_refused`.
    assert_eq!(
        exec_into_shell(Some(ShellType::PowerShell), "exec bash"),
        None
    );
}

#[test]
fn every_bash_zsh_wrapper_form_carries_the_history_marker() {
    for shell in POSIX {
        for command in [
            "ls",
            "ls # c",
            "echo a\necho b",
            "exit 1",
            "exec ls",
            "set -e",
        ] {
            let wrapped = wrap_command(Some(shell), command);
            assert!(wrapped.contains(HISTORY_MARKER), "{command}: {wrapped}");
        }
    }
}

#[test]
fn fish_wrapper_starts_with_a_space_so_fish_keeps_it_out_of_history() {
    for command in ["ls", "echo a\necho b"] {
        assert!(wrap_command(Some(ShellType::Fish), command).starts_with(" begin;"));
    }
}

#[test]
fn fish_commands_that_end_the_shell_run_in_a_child_fish() {
    for command in [
        "exit 3",
        "exec ls /tmp",
        "return 2",
        "false; and exit",
        "true; or exit 1",
        "echo a | exit 2",
        "eval 'exit 3'",
        "exec fish -c 'echo hi'",
    ] {
        assert!(needs_subshell(ShellType::Fish, command), "{command}");
        let wrapped = wrap_command(Some(ShellType::Fish), command);
        assert!(
            wrapped.starts_with(" begin; set -lx PAGER 'cat'"),
            "{wrapped}"
        );
        assert!(
            wrapped.contains("(status fish-path); $__phosphor_ap_fish -c '"),
            "{wrapped}"
        );
        assert!(wrapped.ends_with("'; end"), "{wrapped}");
    }
    // Quotes and backslashes survive the single-quoted -c argument.
    let wrapped = wrap_command(Some(ShellType::Fish), "exit 3 # it's \\ ok");
    assert!(
        wrapped.contains("-c 'exit 3 # it\\'s \\\\ ok'"),
        "{wrapped}"
    );
    // Everything else keeps running in the user's own fish.
    for command in ["cd /tmp", "set -gx FOO bar", "echo exit", "git log"] {
        assert!(!needs_subshell(ShellType::Fish, command), "{command}");
        assert!(!wrap_command(Some(ShellType::Fish), command).contains("fish-path"));
    }
}

// ---- #941 conservative host-ending detection (refuter's 160-case battery, condensed) ----

const PWSH_ENDS_HOST: &[&str] = &[
    "exit",
    "exit;",
    "Exit 3",
    "EXIT",
    "if ($true) { exit 2 }",
    "if ($true) {exit 2}",
    "& { exit }",
    ". { exit }",
    "function f { exit 9 }; f",
    "$Host.SetShouldExit(1)",
    "$host.SetShouldExit (1)",
    "[Environment]::Exit(1)",
    "[System.Environment]::Exit (1)",
    "[System.Environment]::Exit( 1 )",
    "[Environment]::Exit(0); echo hi",
    "Stop-Process $PID",
    "Stop-Process -Id $PID -Force",
    "Stop-Process -Id:$PID",
    "spps $PID",
    "kill $PID",
    "kill -Id $PID",
    "Stop-Process -Id ([Diagnostics.Process]::GetCurrentProcess().Id)",
    "[Diagnostics.Process]::GetCurrentProcess().Kill()",
    "(Get-Process -Id $PID).Kill()",
    "$PID | Stop-Process",
    "iex \"exit 3\"",
    "Invoke-Expression 'exit'",
    "iex 'ex'+'it'",
    "$c='exit'; iex $c",
    "try { Get-Item /nope -EA Stop } catch { exit 1 }",
    "try{Get-Item /nope -EA Stop}catch{exit 1}",
    "try { exit 1 } finally { }",
    "try { [Environment]::Exit(1) } catch {}",
    "foreach ($i in 1..3) { if ($i -eq 2) { exit 5 } }",
    "switch (1) { 1 { exit 4 } }",
    "switch (1) { default { exit 4 } }",
    "$x = 1; exit $x",
    "Get-Item /nope; if (-not $?) { exit 1 }",
    "ls || exit 1",
    "ls && exit 0",
    "Get-Process | ForEach-Object { exit }",
    "echo hi | % { exit 2 }",
    "Invoke-Command { exit 3 }",
    "Invoke-Command -ScriptBlock { exit 3 }",
    "icm { exit }",
    "Start-Job { exit }",
    "[Environment]::FailFast(\"x\")",
    "[scriptblock]::Create('exit').Invoke()",
    "$sb = { exit }; $sb.Invoke()",
    "$f = 'exit'; & $f",
    "& (Get-Command exit)",
    ". ./exits.ps1",
    ". $f",
    // Literals that run code are not literals.
    "Write-Output \"$(exit 3)\"",
    "\"$(exit)\"",
    // Unterminated literals: cannot tell.
    "echo 'oops",
    "echo \"oops",
];

const PWSH_STAYS: &[&str] = &[
    "Set-Location /tmp",
    "Set-Location /tmp; $env:FOO = 'bar'",
    "Write-Output \"exit\"",
    "Write-Output 'exit'",
    "git commit -m \"exit\"",
    "git commit -m exit",
    "Get-Content exit.txt",
    "Set-Location /tmp; Write-Output \"exit\"",
    "echo exit",
    "Write-Host exit",
    "Select-String -Pattern exit file.txt",
    "git log --grep exit",
    "Write-Output \"a; exit\"",
    "Get-Help exit",
    "man exit",
    "ls exit",
    "$exitCode = 3",
    "exit_code=3",
    "$s = 'x'; echo exit",
    "npm run exit",
    "# exit",
    "echo hi # exit",
    "<# exit #> echo hi",
    "echo @'\nexit\n'@",
    "echo @\"\nexit\n\"@",
    "Get-Item ./exit",
    "Exit-PSSession",
    ". ./venv/bin/Activate.ps1",
    "& ./script.ps1",
    "$a = 1; $b = 2; & ./tool.exe",
    "ls && echo done",
    "Write-Output \"a & b\"",
];

#[test]
fn pwsh_every_host_ending_form_runs_in_a_child() {
    for command in PWSH_ENDS_HOST {
        assert!(
            needs_subshell(ShellType::PowerShell, command),
            "{command:?}"
        );
        let wrapped = wrap_command(Some(ShellType::PowerShell), command);
        assert!(wrapped.contains("-EncodedCommand "), "{command:?}");
    }
}

#[test]
fn pwsh_ordinary_commands_stay_in_the_parent() {
    for command in PWSH_STAYS {
        assert!(
            !needs_subshell(ShellType::PowerShell, command),
            "{command:?}"
        );
    }
}

#[test]
fn pwsh_piped_shells_are_not_interactive_ones() {
    for command in [
        "curl -fsSL https://example.com/install.sh | sh",
        "curl -fsSL https://example.com/install.sh | bash",
        "echo hi | sh",
        "Get-Content script.sh | bash",
        "cat x | sh -",
        "bash script.sh",
        "sh ./script.sh",
        "bash -c 'echo hi'",
        "sh -c \"exit 4\"",
        "pwsh -File x.ps1",
        "pwsh -Command \"exit 3\"",
        "docker run -it ubuntu bash",
        "which bash",
        "Get-Command sh",
        "echo bash",
        "echo 'a; pwsh'",
    ] {
        assert_eq!(
            exec_into_shell(Some(ShellType::PowerShell), command),
            None,
            "{command:?}"
        );
    }
    for command in [
        "pwsh",
        "pwsh -NoProfile",
        "& pwsh",
        "& 'pwsh' -NoExit",
        "powershell.exe -NoLogo -NoExit",
        "cmd",
        "bash",
        "zsh -l",
        "echo hi; bash",
        "ls\nfish",
        "true && sh",
    ] {
        assert!(
            exec_into_shell(Some(ShellType::PowerShell), command).is_some(),
            "{command:?}"
        );
    }
}

#[test]
fn pwsh_history_filter_matches_only_the_exact_wrapper_prefix() {
    for command in ["ls", "exit 3", "Set-Location /tmp", "a\nb"] {
        let wrapped = wrap_command(Some(ShellType::PowerShell), command);
        assert!(
            wrapped.starts_with(PWSH_WRAPPER_HISTORY_PREFIX),
            "{wrapped}"
        );
    }
    // pwsh.ps1 carries the same prefix, and no loose `__phosphor_ap` pattern.
    let ps1 = include_str!("../../../../../assets/bundled/bootstrap/pwsh.ps1");
    assert!(ps1.contains(PWSH_WRAPPER_HISTORY_PREFIX));
    assert!(!ps1.contains("-match '__phosphor_ap'"));
    assert!(!ps1.contains("'*__phosphor_ap*'"));
    // A user's own command that mentions the marker does not look like the wrapper.
    assert!(!"echo __phosphor_ap".starts_with(PWSH_WRAPPER_HISTORY_PREFIX));
}

#[test]
fn pwsh_wrapper_keeps_the_users_lastexitcode_and_records_no_errors() {
    let wrapped = wrap_command(Some(ShellType::PowerShell), "ls");
    // Saved, then reset, before the command runs.
    let save = wrapped.find("$global:__phosphor_ap_lecset").unwrap();
    let reset = wrapped.find("$global:LASTEXITCODE = 0").unwrap();
    let cmd = wrapped.find("try { ls;").unwrap();
    assert!(save < reset && reset < cmd, "{wrapped}");
    assert!(
        PWSH_RERAISE.contains("-ErrorAction Ignore"),
        "{PWSH_RERAISE}"
    );
    let ps1 = include_str!("../../../../../assets/bundled/bootstrap/pwsh.ps1");
    assert!(ps1.contains("$global:LASTEXITCODE = $global:__phosphor_ap_lec"));
}

// ---- bash / zsh / fish attack list (#855, #932) ----

const POSIX_ENDS_SHELL: &[&str] = &[
    "f() { exit 9; }; f",
    "{ exit 2; }",
    "if true; then exit 2; fi",
    "if true;then exit 2;fi",
    "trap 'exit' USR1",
    "trap exit EXIT",
    "eval \"$v\"",
    "eval exit",
    "eval 'exit 3'",
    "while true; do exit 3; done",
    "case x in x) exit 4;; esac",
    "for i in 1; do exit; done",
    "true && exit 5",
    "false || exit 6",
    "exec ls",
    "exec > /tmp/log",
    "logout",
    "set -e; false",
    "set -e\nfalse",
    "set -o errexit; false",
    "return",
    "return 3",
    "exit",
    "exit 3",
    "(exit 3)",
    "ulimit -n 5",
];

const POSIX_STAYS: &[&str] = &[
    "cd /tmp",
    "cd /tmp && echo \"exit\"",
    "echo 'exit'",
    "echo \"exit\"; cd /tmp",
    "exit_code=3",
    "git commit -m exit",
    "git commit -m \"exit\"",
    "grep exit file",
    "# exit",
    "echo hi # exit",
    "cat exit.txt",
    "export FOO=bar",
    "source venv/bin/activate",
    ". venv/bin/activate",
    "source ~/.bashrc",
    "source ./env.sh",
    ". ./env.sh",
    "source /tmp/not-readable-here.sh",
    "eval echo hi",
    "eval \"cd /tmp\"",
    "echo $((1<<2))",
];

const POSIX_KILLS_SHELL: &[&str] = &[
    "kill $$",
    "kill -9 $$",
    "kill -KILL $$",
    "kill \"$$\"",
    "kill -- $$",
    "kill $PPID",
    "kill ${PPID}",
    "kill $BASHPID",
    "kill 0",
    "kill -9 -1",
    "kill -- -$$",
    "builtin kill $$",
    "/bin/kill -9 $$",
    "trap 'kill $$' EXIT",
    "eval \"kill $$\"",
    "true; kill -TERM $$",
];

#[test]
fn bash_and_zsh_every_shell_ending_form_runs_in_a_subshell() {
    for shell in POSIX {
        for command in POSIX_ENDS_SHELL {
            assert!(needs_subshell(shell, command), "{shell:?} {command:?}");
            let wrapped = wrap_command(Some(shell), command);
            assert!(
                wrapped.starts_with(": __phosphor_ap_subshell; ("),
                "{shell:?} {wrapped}"
            );
        }
    }
}

#[test]
fn bash_and_zsh_ordinary_commands_stay_in_the_user_shell() {
    for shell in POSIX {
        for command in POSIX_STAYS {
            assert!(!needs_subshell(shell, command), "{shell:?} {command:?}");
            assert!(
                wrap_command(Some(shell), command).starts_with("{ "),
                "{command:?}"
            );
        }
    }
}

#[test]
fn bash_and_zsh_self_kills_are_refused_because_a_subshell_cannot_contain_them() {
    // `$$` and `$PPID` name the user's shell even inside `( ... )`.
    for shell in [Some(ShellType::Bash), Some(ShellType::Zsh), None] {
        for command in POSIX_KILLS_SHELL {
            assert!(
                kills_own_shell(shell, command).is_some(),
                "{shell:?} {command:?}"
            );
        }
        for command in [
            "kill 1234",
            "kill -9 4321",
            "kill %1",
            "kill -0 4321",
            "git log --grep kill",
            "echo kill $HOME",
            "pkill -f server",
            "sleep 1 &\nkill $!",
            "echo \"kill $$ is bad\"",
        ] {
            assert_eq!(
                kills_own_shell(shell, command),
                None,
                "{shell:?} {command:?}"
            );
        }
    }
    // fish and pwsh run such a command in a child shell, where the pid is the child's.
    assert_eq!(
        kills_own_shell(Some(ShellType::Fish), "kill $fish_pid"),
        None
    );
    assert_eq!(
        kills_own_shell(Some(ShellType::PowerShell), "kill $PID"),
        None
    );
}

#[test]
fn fish_every_shell_ending_form_runs_in_a_child_fish() {
    for command in [
        "function f; exit 9; end; f",
        "begin; exit 2; end",
        "if true; exit 2; end",
        "while true; exit 3; end",
        "switch x; case x; exit 4; end",
        "true; and exit 5",
        "false; or exit 6",
        "for i in 1; exit; end",
        "kill $fish_pid",
        "kill %self",
        "kill -9 $fish_pid",
        "kill (echo $fish_pid)",
        "eval $v",
        "eval \"$v\"",
        "eval exit",
        "return",
        "return 2",
        "exit",
        "exit 3",
        "exec ls",
    ] {
        assert!(needs_subshell(ShellType::Fish, command), "{command:?}");
        let wrapped = wrap_command(Some(ShellType::Fish), command);
        assert!(wrapped.contains("fish-path"), "{command:?}");
    }
    for command in [
        "cd /tmp",
        "set -gx FOO bar",
        "echo \"exit\"",
        "echo 'exit'",
        "echo exit",
        "git commit -m exit",
        "set exit_code 3",
        "# exit",
        "source ~/.config/fish/config.fish",
        "source venv/bin/activate.fish",
        "source ./env.fish",
        ". ./env.fish",
        "eval echo hi",
        "cat exit.txt",
    ] {
        assert!(!needs_subshell(ShellType::Fish, command), "{command:?}");
    }
}

#[test]
fn bootstrap_and_builtin_pwsh_commands_are_module_qualified() {
    // #944: what runs in the user's pwsh must not be reachable by a user function or alias.
    let builtins = include_str!("../../../../context_chips/builtins.rs");
    for bare in [
        "| Split-Path",
        "| Measure-Object",
        "| where {",
        "| foreach {",
    ] {
        assert!(
            !builtins.contains(bare),
            "unqualified `{bare}` in context_chips/builtins.rs"
        );
    }
    let block = include_str!(
        "../../../../../assets/bundled/bootstrap/pwsh_subshell_bootstrap_block_output.txt"
    );
    assert!(block.starts_with("Microsoft.PowerShell.Management\\Add-Content"));
    assert!(block.contains("{ Microsoft.PowerShell.Utility\\Write-Host "));
}

// ---- the wrappers run in REAL shells: nothing in the attack lists may end the user's shell, the
// ---- in-shell `exit` guard exists only while the command runs, and the user's own `exit` (and its
// ---- function, byte-identical) is back afterwards. Skipped where a shell is not installed.

mod real_shells {
    use super::*;
    use std::process::{Command, ExitStatus, Stdio};
    use std::sync::{Arc, Mutex, OnceLock, mpsc};
    use std::time::{Duration, Instant};

    /// How long one shell invocation may run before it is killed and reported as a TIMEOUT.
    /// Generous because a cold pwsh (the host plus the child the wrapper starts) can take many
    /// seconds on a loaded box; still short enough that nextest's own kill (60 s by default,
    /// see `.config/nextest.toml`) does not swallow the report of a test's first hang.
    const HANG_GUARD: Duration = Duration::from_secs(45);
    /// After the shell exits, how long a descendant that inherited the output pipes may keep
    /// them open before the output read so far is taken as final.
    const PIPE_GRACE: Duration = Duration::from_secs(10);

    /// The path of `shell`'s binary, looked up once per test process (each lookup of pwsh is a
    /// full .NET start of its own).
    fn program(shell: ShellType) -> Option<String> {
        static FOUND: [OnceLock<Option<String>>; 5] = [const { OnceLock::new() }; 5];
        let (slot, name) = match shell {
            ShellType::Bash => (0, "bash"),
            ShellType::Zsh => (1, "zsh"),
            ShellType::Fish => (2, "fish"),
            ShellType::PowerShell => (3, "pwsh"),
            ShellType::Nu => (4, "nu"),
        };
        FOUND[slot]
            .get_or_init(|| {
                let mut candidates = vec![name.to_owned()];
                if let Ok(home) = std::env::var("HOME") {
                    candidates.push(format!("{home}/.local/bin/{name}"));
                }
                candidates.into_iter().find(|c| {
                    Command::new(c)
                        .arg("--version")
                        .output()
                        .is_ok_and(|o| o.status.success())
                })
            })
            .clone()
    }

    /// Everything one real-shell run produced, so that a failing assertion can say *how* the
    /// shell ended (exit code, signal, or killed by the hang guard) and what it wrote where.
    pub struct Ran {
        pub stdout: String,
        pub stderr: String,
        /// `None` only if the child could not be reaped after a kill.
        pub status: Option<ExitStatus>,
        pub elapsed: Duration,
        /// Killed by [`HANG_GUARD`]: the output is partial and must not count as a result.
        pub timed_out: bool,
        /// A descendant still held stdout/stderr open [`PIPE_GRACE`] after the shell ended.
        pub pipes_held_open: bool,
    }

    impl Ran {
        /// stdout followed by stderr: what the assertions search.
        pub fn out(&self) -> String {
            format!("{}{}", self.stdout, self.stderr)
        }

        pub fn code(&self) -> Option<i32> {
            self.status.and_then(|status| status.code())
        }
    }

    impl std::fmt::Display for Ran {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            if self.timed_out {
                write!(
                    f,
                    "TIMEOUT: killed by the {HANG_GUARD:?} hang guard after {:.1?}",
                    self.elapsed
                )?;
            } else {
                match self.status {
                    Some(status) => write!(f, "{status} after {:.1?}", self.elapsed)?,
                    None => write!(f, "no exit status after {:.1?}", self.elapsed)?,
                }
            }
            if self.pipes_held_open {
                write!(
                    f,
                    " (a descendant still held the output pipes open {PIPE_GRACE:?} later)"
                )?;
            }
            write!(
                f,
                "\n--- stdout ({} bytes) ---\n{}\n--- stderr ({} bytes) ---\n{}",
                self.stdout.len(),
                self.stdout,
                self.stderr.len(),
                self.stderr
            )
        }
    }

    /// Reads `pipe` to EOF on its own thread, so a chatty child can never block on a full pipe
    /// while we wait for it. What has been read so far stays readable even if EOF never comes
    /// (a descendant that inherited the pipe outlives the shell).
    fn drain(
        mut pipe: impl std::io::Read + Send + 'static,
    ) -> (Arc<Mutex<Vec<u8>>>, mpsc::Receiver<()>) {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let (done, finished) = mpsc::channel();
        let sink = Arc::clone(&bytes);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => sink
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = done.send(());
        });
        (bytes, finished)
    }

    /// Runs `script` in a fresh non-interactive `shell` (no rc files) and reports everything about
    /// how it went; `None` only if the shell is not installed. A shell that is installed but
    /// cannot be started or waited on is a panic, never a silent skip.
    ///
    /// The hang guard lives here rather than in an external `timeout`: on Windows that name
    /// resolves to `TIMEOUT.EXE` (a sleep command), so pwsh never ran and every pwsh case failed
    /// with "Default option is not allowed more than '1' time(s)".
    ///
    /// For pwsh, `cache_dir` (when given) becomes `XDG_CACHE_HOME`, so concurrent pwsh processes
    /// do not share one `~/.cache/powershell` (startup JIT profile, module analysis cache); the
    /// update check and telemetry, background work that touches the same directory, are off.
    pub fn run_detailed(
        shell: ShellType,
        script: &str,
        cache_dir: Option<&std::path::Path>,
    ) -> Option<Ran> {
        let prog = program(shell)?;
        let mut cmd = Command::new(&prog);
        match shell {
            ShellType::Bash => cmd.args(["--norc", "--noprofile", "-c", script]),
            ShellType::Zsh => cmd.args(["-f", "-c", script]),
            ShellType::Fish => cmd.args(["--no-config", "-c", script]),
            ShellType::PowerShell => cmd.args([
                "-NoProfile",
                "-NoLogo",
                "-NonInteractive",
                "-Command",
                script,
            ]),
            ShellType::Nu => cmd.args(["--no-config-file", "-c", script]),
        };
        if shell == ShellType::PowerShell {
            cmd.env("POWERSHELL_TELEMETRY_OPTOUT", "1")
                .env("POWERSHELL_UPDATECHECK", "Off");
            if let Some(dir) = cache_dir {
                cmd.env("XDG_CACHE_HOME", dir);
            }
        }
        let started = Instant::now();
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{prog} is installed but could not be started: {e}"));
        let (stdout, stdout_done) = drain(child.stdout.take().expect("stdout is piped"));
        let (stderr, stderr_done) = drain(child.stderr.take().expect("stderr is piped"));
        let deadline = started + HANG_GUARD;
        let (status, timed_out) = loop {
            match child.try_wait() {
                Ok(Some(status)) => break (Some(status), false),
                Ok(None) => {}
                Err(e) => panic!("waiting for {prog}: {e}"),
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                break (child.wait().ok(), true);
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let elapsed = started.elapsed();
        // Exit is not EOF: wait for both readers to see EOF (a disconnected sender means the
        // reader is gone, which is EOF too), so no output still in flight is lost.
        let grace_ends = Instant::now() + PIPE_GRACE;
        let mut pipes_held_open = false;
        for finished in [&stdout_done, &stderr_done] {
            let left = grace_ends.saturating_duration_since(Instant::now());
            if let Err(mpsc::RecvTimeoutError::Timeout) = finished.recv_timeout(left) {
                pipes_held_open = true;
            }
        }
        let text = |bytes: &Arc<Mutex<Vec<u8>>>| {
            String::from_utf8_lossy(&bytes.lock().unwrap_or_else(|e| e.into_inner())).into_owned()
        };
        Some(Ran {
            stdout: text(&stdout),
            stderr: text(&stderr),
            status,
            elapsed,
            timed_out,
            pipes_held_open,
        })
    }

    /// [`run_detailed`] for the many assertions that only need the output (stdout then stderr)
    /// and the exit code; `None` if the shell is not installed. A run killed by the hang guard
    /// panics with a TIMEOUT report instead of passing its partial output off as a result.
    pub fn run(shell: ShellType, script: &str) -> Option<(String, Option<i32>)> {
        let ran = run_detailed(shell, script, None)?;
        assert!(!ran.timed_out, "{shell:?} {ran}\n--- script ---\n{script}");
        Some((ran.out(), ran.code()))
    }

    fn alive_probe(shell: ShellType) -> &'static str {
        match shell {
            ShellType::Fish => "echo ALIVE rc=$status pwd=$PWD",
            ShellType::PowerShell => "Write-Output \"ALIVE pwd=$PWD\"",
            _ => "printf 'ALIVE rc=%s pwd=%s\\n' \"$?\" \"$PWD\"",
        }
    }

    fn wrapped_then_probe(shell: ShellType, command: &str) -> String {
        format!(
            "{}\n{}",
            wrap_command(Some(shell), command),
            alive_probe(shell)
        )
    }

    #[test]
    fn bash_and_zsh_survive_every_form_of_the_attack_list() {
        for shell in POSIX {
            for command in POSIX_ENDS_SHELL
                .iter()
                .filter(|c| !c.starts_with("source /tmp"))
            {
                if kills_own_shell(Some(shell), command).is_some() {
                    continue;
                }
                let Some((out, _)) = run(shell, &wrapped_then_probe(shell, command)) else {
                    return;
                };
                assert!(out.contains("ALIVE"), "{shell:?} {command:?}: {out}");
            }
            // Plain `exit N` reports N and the shell goes on.
            let Some((out, _)) = run(shell, &wrapped_then_probe(shell, "exit 3")) else {
                return;
            };
            assert!(out.contains("ALIVE rc=3"), "{shell:?}: {out}");
        }
    }

    #[test]
    fn fish_survives_every_form_of_the_attack_list() {
        for command in [
            "function f; exit 9; end; f",
            "begin; exit 2; end",
            "if true; exit 2; end",
            "while true; exit 3; end",
            "switch x; case x; exit 4; end",
            "true; and exit 5",
            "false; or exit 6",
            "kill $fish_pid",
            "kill %self",
            "kill -9 $fish_pid",
            "eval exit",
            "set v exit; eval $v",
            "return 2",
            "exit 3",
            "exec ls /",
        ] {
            let Some((out, _)) = run(
                ShellType::Fish,
                &wrapped_then_probe(ShellType::Fish, command),
            ) else {
                return;
            };
            assert!(out.contains("ALIVE"), "{command:?}: {out}");
        }
        let Some((out, _)) = run(
            ShellType::Fish,
            &wrapped_then_probe(ShellType::Fish, "exit 3"),
        ) else {
            return;
        };
        assert!(out.contains("ALIVE rc=3"), "{out}");
    }

    #[test]
    fn pwsh_survives_every_form_of_the_attack_list() {
        if program(ShellType::PowerShell).is_none() {
            return;
        }
        // pwsh starts slowly, so the forms run on a few workers pulling from one queue. Every
        // form is two live pwsh processes (the host and the child the wrapper starts), so
        // `WORKERS` bounds the .NET runtimes this test has alive at once; nextest reserves CPUs
        // for them (`threads-required` in `.config/nextest.toml`, #1050). Each worker has its
        // own pwsh cache directory, so no two concurrent pwsh share one.
        const WORKERS: usize = 4;
        let next = std::sync::atomic::AtomicUsize::new(0);
        let next = &next;
        let failures: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..WORKERS)
                .map(|_| {
                    scope.spawn(move || {
                        let cache = tempfile::tempdir().expect("a pwsh cache directory");
                        let mut failures = Vec::new();
                        loop {
                            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(command) = PWSH_ENDS_HOST.get(i) else {
                                break;
                            };
                            let ran = run_detailed(
                                ShellType::PowerShell,
                                &wrapped_then_probe(ShellType::PowerShell, command),
                                Some(cache.path()),
                            )
                            .expect("pwsh was found above");
                            if ran.timed_out || !ran.out().contains("ALIVE") {
                                failures.push(format!("{command:?}: {ran}"));
                            }
                        }
                        failures
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect()
        });
        assert!(
            failures.is_empty(),
            "{} of {} forms did not leave pwsh ALIVE:\n\n{}",
            failures.len(),
            PWSH_ENDS_HOST.len(),
            failures.join("\n\n")
        );
    }

    #[test]
    fn pwsh_wrapper_leaves_no_error_records_and_reports_a_native_exit_code() {
        let wrapped = wrap_command(Some(ShellType::PowerShell), "Write-Output hi");
        let Some((out, _)) = run(
            ShellType::PowerShell,
            &format!("$Error.Clear()\n{wrapped}\n\"errors=$($Error.Count)\""),
        ) else {
            return;
        };
        assert!(out.contains("hi") && out.contains("errors=0"), "{out}");
        // A failing native command: `$?` is false and `$LASTEXITCODE` is its code, as `Warp-Precmd` reads them.
        let wrapped = wrap_command(Some(ShellType::PowerShell), "sh -c 'exit 4'");
        let (out, _) = run(
            ShellType::PowerShell,
            &format!("{wrapped}\n\"q=$? ec=$LASTEXITCODE\""),
        )
        .unwrap();
        assert!(out.contains("q=False ec=4"), "{out}");
        // A stale code of the user's is not what a later failing cmdlet reports.
        let wrapped = wrap_command(
            Some(ShellType::PowerShell),
            "Get-Item /nope-zz -ErrorAction Ignore",
        );
        let (out, _) = run(
            ShellType::PowerShell,
            &format!("sh -c 'exit 9'\n{wrapped}\n\"ec=$LASTEXITCODE\""),
        )
        .unwrap();
        assert!(out.contains("ec=0"), "{out}");
    }

    /// #950: the real bootstrap and prompt function leave `$Error` alone, also after agent
    /// commands (the wrapper's leftovers are removed by `Warp-Precmd`).
    #[test]
    fn pwsh_bootstrap_and_prompts_add_no_error_records() {
        let ps1 = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/bundled/bootstrap/pwsh.ps1"
        );
        let wrapper = wrap_command(Some(ShellType::PowerShell), "Write-Output hi");
        let script = format!(
            "$global:_warpSessionId = [uint64]123; $env:WARP_HONOR_PS1 = '0'\n\
             . '{ps1}' *> $null\n\
             $Error.Clear()\n\
             for ($i = 0; $i -lt 3; $i++) {{ $null = prompt 2>&1; \"after-prompt-$i errors=$($Error.Count)\" }}\n\
             {wrapper} *> $null\n\
             $null = prompt 2>&1\n\
             \"after-agent errors=$($Error.Count)\"\n\
             $Error | Select-Object -First 3 | ForEach-Object {{ \"ERR: $_\" }}"
        );
        let Some((out, _)) = run(ShellType::PowerShell, &script) else {
            return;
        };
        assert!(out.contains("after-prompt-2 errors=0"), "{out}");
        assert!(out.contains("after-agent errors=0"), "{out}");
        assert!(!out.contains("ERR:"), "{out}");
    }

    #[test]
    fn false_positives_stay_in_the_users_shell_and_cd_persists() {
        for shell in [ShellType::Bash, ShellType::Zsh, ShellType::Fish] {
            for command in [
                "cd /tmp && echo \"exit\"",
                "cd /tmp; echo 'exit'",
                "cd /tmp # exit",
                "cd /tmp; git commit -m exit 2>/dev/null; true",
            ] {
                let command = if shell == ShellType::Fish {
                    command.replace("&&", "; and")
                } else {
                    command.to_owned()
                };
                let Some((out, _)) = run(shell, &wrapped_then_probe(shell, &command)) else {
                    continue;
                };
                assert!(out.contains("pwd=/tmp"), "{shell:?} {command:?}: {out}");
                assert!(!wrap_command(Some(shell), &command).contains("fish-path"));
                assert!(!needs_subshell(shell, &command), "{shell:?} {command:?}");
            }
        }
    }

    #[test]
    fn in_shell_commands_keep_running_in_the_users_shell() {
        // cd, export / set -gx and a function the agent defines persist: no child involved.
        for (shell, cmd) in [
            (
                ShellType::Bash,
                "cd /tmp && export FOO=bar && agent_fn() { echo fn; }",
            ),
            (
                ShellType::Zsh,
                "cd /tmp && export FOO=bar && agent_fn() { echo fn; }",
            ),
            (
                ShellType::Fish,
                "cd /tmp; set -gx FOO bar; function agent_fn; echo fn; end",
            ),
        ] {
            let probe = match shell {
                ShellType::Fish => "echo pwd=$PWD foo=$FOO; agent_fn",
                _ => "echo pwd=$PWD foo=$FOO; agent_fn",
            };
            let Some((out, _)) = run(
                shell,
                &format!("{}\n{probe}", wrap_command(Some(shell), cmd)),
            ) else {
                continue;
            };
            assert!(
                out.contains("pwd=/tmp foo=bar") && out.contains("fn"),
                "{shell:?}: {out}"
            );
        }
        let Some((out, _)) = run(
            ShellType::PowerShell,
            &format!(
                "{}\n$env:FOO",
                wrap_command(
                    Some(ShellType::PowerShell),
                    "Set-Location /tmp; $env:FOO = 'bar'"
                )
            ),
        ) else {
            return;
        };
        assert!(out.contains("bar"), "{out}");
    }

    #[test]
    fn the_exit_guard_is_active_only_while_the_agent_command_runs() {
        for shell in POSIX {
            // During: `exit` is a function. After: the builtin is back.
            let script = format!(
                "{}\n{}\ntype exit",
                wrap_command(Some(shell), "type exit"),
                "echo ---"
            );
            let Some((out, _)) = run(shell, &script) else {
                continue;
            };
            let (during, after) = out.split_once("---").unwrap();
            assert!(during.contains("function"), "{shell:?} during: {out}");
            assert!(after.contains("builtin"), "{shell:?} after: {out}");

            // A failing command: the user's own `exit` still exits, with its code.
            let (_, code) = run(
                shell,
                &format!("{}\nexit 7", wrap_command(Some(shell), "false")),
            )
            .unwrap();
            assert_eq!(code, Some(7), "{shell:?}");

            // The user's own `exit` function comes back byte-identical, and still runs.
            let script = format!(
                "exit() {{ echo 'user exit'; builtin exit 43; }}\nbefore=$(typeset -f exit)\n{}\nafter=$(typeset -f exit)\n[ \"$before\" = \"$after\" ] && echo SAME\nexit",
                wrap_command(Some(shell), "echo hi")
            );
            let (out, code) = run(shell, &script).unwrap();
            assert!(
                out.contains("SAME") && out.contains("user exit"),
                "{shell:?}: {out}"
            );
            assert_eq!(code, Some(43), "{shell:?}: {out}");
            // ... also when the agent command itself redefines `exit`.
            let script = format!(
                "exit() {{ echo 'user exit'; builtin exit 44; }}\nbefore=$(typeset -f exit)\n{}\n[ \"$before\" = \"$(typeset -f exit)\" ] && echo SAME\nexit",
                wrap_command(
                    Some(shell),
                    "exit() { echo agent; }; logout() { echo agent; }"
                )
            );
            let (out, code) = run(shell, &script).unwrap();
            assert_eq!(code, Some(44), "{shell:?}: {out}");
        }
        // fish
        let wrapped = wrap_command(Some(ShellType::Fish), "functions -q exit; and echo GUARDED");
        let Some((out, _)) = run(
            ShellType::Fish,
            &format!("{wrapped}\nfunctions -q exit; or echo CLEAN"),
        ) else {
            return;
        };
        assert!(out.contains("GUARDED") && out.contains("CLEAN"), "{out}");
        let (_, code) = run(
            ShellType::Fish,
            &format!("{}\nexit 7", wrap_command(Some(ShellType::Fish), "false")),
        )
        .unwrap();
        assert_eq!(code, Some(7));
        let script = format!(
            "function exit; echo 'user exit'; builtin exit 43; end\nset before (functions exit | string match -rv '^# Defined' | string collect)\n{}\nset after (functions exit | string match -rv '^# Defined' | string collect)\ntest \"$before\" = \"$after\"; and echo SAME\nexit",
            wrap_command(Some(ShellType::Fish), "echo hi")
        );
        let (out, code) = run(ShellType::Fish, &script).unwrap();
        assert!(out.contains("SAME") && out.contains("user exit"), "{out}");
        assert_eq!(code, Some(43), "{out}");
    }

    #[test]
    fn the_interrupted_command_backstops_restore_the_exit_guard() {
        let bash = include_str!("../../../../../assets/bundled/bootstrap/bash_body.sh");
        let zsh = include_str!("../../../../../assets/bundled/bootstrap/zsh_body.sh");
        let fish = include_str!("../../../../../assets/bundled/bootstrap/fish.sh");
        assert!(bash.contains(POSIX_GUARD_RESTORE));
        assert!(zsh.contains(POSIX_GUARD_RESTORE));
        assert!(fish.contains("function __phosphor_ap_fish_restore --on-event fish_prompt"));
    }
}

// ---- sourced files run in the user's shell (#941 follow-up) ----

mod sourcing {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("phosphor_ap_{tag}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_sourced_file_runs_in_the_shell_unless_it_visibly_holds_a_hazard() {
        let dir = temp_dir("src");
        let cwd = dir.to_str().unwrap();
        fs::write(dir.join("env.sh"), "export FOO=1\ncd /tmp\n").unwrap();
        fs::write(dir.join("exits.sh"), "echo before\nexit 3\n").unwrap();
        fs::write(dir.join("hazard1.sh"), "builtin exit 3\n").unwrap();
        fs::write(dir.join("hazard2.sh"), "command exit 3\n").unwrap();
        fs::write(dir.join("hazard3.sh"), "kill -9 $$\n").unwrap();
        fs::write(dir.join("hazard4.sh"), "exec ls\n").unwrap();
        for shell in [
            Some(ShellType::Bash),
            Some(ShellType::Zsh),
            Some(ShellType::Fish),
        ] {
            for ok in [
                "source env.sh",
                ". ./env.sh",
                "source exits.sh",
                "source ./missing.sh",
            ] {
                let wrapped = wrap_command_in(shell, ok, Some(cwd));
                assert!(
                    wrapped.starts_with(" begin;") || wrapped.starts_with("{ "),
                    "{shell:?} {ok}: {wrapped}"
                );
                assert!(
                    !wrapped.contains("fish-path") && !wrapped.contains("__phosphor_ap_subshell"),
                    "{wrapped}"
                );
            }
            for bad in [
                "source hazard1.sh",
                "source ./hazard2.sh",
                ". hazard3.sh",
                "source hazard4.sh",
            ] {
                let wrapped = wrap_command_in(shell, bad, Some(cwd));
                assert!(
                    wrapped.contains("fish-path") || wrapped.contains("__phosphor_ap_subshell"),
                    "{shell:?} {bad}: {wrapped}"
                );
            }
        }
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pwsh_dot_sources_a_file_with_no_exit_like_text_in_the_shell() {
        let dir = temp_dir("pwsrc");
        let cwd = dir.to_str().unwrap();
        fs::write(dir.join("env.ps1"), "$env:FOO = '1'\nSet-Location /tmp\n").unwrap();
        fs::write(dir.join("exits.ps1"), "Write-Output hi\nexit 3\n").unwrap();
        fs::write(dir.join("pid.ps1"), "Stop-Process -Id $PID\n").unwrap();
        fs::write(dir.join("nested.ps1"), ". ./env.ps1\n").unwrap();
        let in_shell = |c: &str| {
            !wrap_command_in(Some(ShellType::PowerShell), c, Some(cwd)).contains("-EncodedCommand ")
        };
        assert!(in_shell(". ./env.ps1"));
        assert!(in_shell(". 'env.ps1'"));
        assert!(in_shell(". \"env.ps1\"; Set-Location /tmp"));
        assert!(!in_shell(". ./exits.ps1"));
        assert!(!in_shell(". ./pid.ps1"));
        assert!(!in_shell(". ./nested.ps1"));
        assert!(!in_shell(". ./missing.ps1"));
        assert!(!in_shell(". $f"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn real_shells_persist_a_sourced_file_and_survive_one_that_exits() {
        use super::real_shells::run;
        let dir = temp_dir("realsrc");
        let cwd = dir.to_str().unwrap();
        fs::write(dir.join("env.sh"), "export FOO=1\ncd /tmp\n").unwrap();
        fs::write(dir.join("exits.sh"), "echo before\nexit 3\n").unwrap();
        fs::write(dir.join("env.fish"), "set -gx FOO 1\ncd /tmp\n").unwrap();
        fs::write(dir.join("exits.fish"), "echo before\nexit 3\n").unwrap();
        for (shell, env, exits, probe, alive) in [
            (
                ShellType::Bash,
                "source env.sh",
                "source exits.sh",
                "echo pwd=$PWD foo=$FOO",
                "printf 'ALIVE rc=%s\\n' \"$?\"",
            ),
            (
                ShellType::Zsh,
                "source env.sh",
                "source exits.sh",
                "echo pwd=$PWD foo=$FOO",
                "printf 'ALIVE rc=%s\\n' \"$?\"",
            ),
            (
                ShellType::Fish,
                "source env.fish",
                "source exits.fish",
                "echo pwd=$PWD foo=$FOO",
                "echo ALIVE rc=$status",
            ),
        ] {
            let wrapped = wrap_command_in(Some(shell), env, Some(cwd));
            // Quoted: on Windows the temp dir is `C:\Users\...`, whose backslashes an
            // unquoted word loses in Git Bash.
            let script = format!("cd '{cwd}'\n{wrapped}\n{probe}");
            let Some((out, _)) = run(shell, &script) else {
                continue;
            };
            assert!(out.contains("pwd=/tmp foo=1"), "{shell:?}: {out}");
            let wrapped = wrap_command_in(Some(shell), exits, Some(cwd));
            let script = format!("cd '{cwd}'\n{wrapped}\n{alive}");
            let (out, _) = run(shell, &script).unwrap();
            assert!(
                out.contains("before") && out.contains("ALIVE rc=3"),
                "{shell:?}: {out}"
            );
        }
        fs::remove_dir_all(dir).ok();
    }
}

// ---- the exit guard inside subshells (#941 follow-up) ----

#[test]
fn the_exit_guard_exits_for_real_inside_subshells() {
    use real_shells::run;
    // `$e` hides the word `exit` from the detection, so the command runs in the guarded group.
    for shell in POSIX {
        let wrapped = wrap_command(
            Some(shell),
            "e=exit; ( $e 4 ); echo sub=$?; x=$($e 5; echo no); echo \"x=[$x] st=$?\"; $e 9; echo guarded-returns",
        );
        assert!(wrapped.starts_with("{ "), "{wrapped}");
        let Some((out, _)) = run(shell, &wrapped) else {
            continue;
        };
        assert!(out.contains("sub=4"), "{shell:?}: {out}");
        assert!(out.contains("x=[] st=5"), "{shell:?}: {out}");
        // ... while the user's own shell is still protected.
        assert!(out.contains("guarded-returns"), "{shell:?}: {out}");
        // A subshell function defined by the agent that calls exit leaves only its subshell too.
        let wrapped = wrap_command(Some(shell), "e=exit; f() { $e 6; }; ( f ); echo f=$?");
        let (out, _) = run(shell, &wrapped).unwrap();
        assert!(out.contains("f=6"), "{shell:?}: {out}");
    }
}

/// nushell scopes `cd` and `$env` changes to a block, so the agent's command is NOT wrapped in
/// one (a `cd` would silently not take effect). It goes through as typed, with a leading space
/// that keeps it out of the history (`history.ignore_space_prefixed`).
#[test]
fn nushell_commands_are_not_wrapped_in_a_block() {
    assert_eq!(wrap_command(Some(ShellType::Nu), "cd /tmp"), " cd /tmp");
    assert_eq!(
        wrap_command(Some(ShellType::Nu), "ls | where size > 1mb\n\n"),
        " ls | where size > 1mb"
    );
    let multi = wrap_command(Some(ShellType::Nu), "let x = 1\n$x + 1");
    assert_eq!(multi, " let x = 1\n$x + 1");
    for wrapped in [&multi, &wrap_command(Some(ShellType::Nu), "exit 3")] {
        assert!(!wrapped.contains("{ "), "{wrapped}");
        assert!(!wrapped.contains("__phosphor_ap"), "{wrapped}");
    }
    // `$$` / `$PPID` do not exist in nushell, so the POSIX self-kill scan does not apply.
    assert_eq!(kills_own_shell(Some(ShellType::Nu), "kill $$"), None);
}
