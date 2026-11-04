use super::*;
use warpui::App;

use crate::test_util::settings::initialize_settings_for_tests;

struct TestAssetProvider;

impl AssetProvider for TestAssetProvider {
    fn get(&self, path: &str) -> anyhow::Result<Cow<'_, [u8]>> {
        let content = match path {
            "bundled/bootstrap/bash.sh" => "#include hello_world",
            "bundled/bootstrap/fish.sh" => "# this is a comment\nthis_is_a_command",
            "bundled/bootstrap/zsh.sh" => {
                "asdf\n#include whitespace\n    prepended whitespace\n\n\n"
            }
            "bundled/bootstrap/pwsh.ps1" => {
                r#"# This is a comment
                Write-Output 'Testing some output'
                function test1 {
                    [Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSAvoidUsingInvokeExpression', '', Justification = 'We actually need it')]
                    param([string]$command)
                    Invoke-Expression $command
                }"#
            }
            "hello_world" => "hello world!",
            "whitespace" => "no whitespace\n\n\n yes whitespace!",
            _ => anyhow::bail!("path not found in assets"),
        };
        Ok(Cow::Borrowed(content.as_bytes()))
    }
}

#[test]
fn test_include_directive() {
    assert_eq!(
        decode_script(&script_for_shell(ShellType::Bash, &TestAssetProvider)),
        "hello world!\n"
    );
}

#[test]
fn test_trims_comments() {
    assert_eq!(
        decode_script(&script_for_shell(ShellType::Fish, &TestAssetProvider)),
        "this_is_a_command\n"
    );
}

#[test]
fn test_trims_whitespace() {
    assert_eq!(
        decode_script(&script_for_shell(ShellType::Zsh, &TestAssetProvider)),
        "asdf\nno whitespace\n yes whitespace!\n prepended whitespace\n"
    );
}

#[test]
fn test_trims_powershell_specifics() {
    assert_eq!(
        decode_script(&script_for_shell(ShellType::PowerShell, &TestAssetProvider)),
        " Write-Output 'Testing some output'\n function test1 {\n param([string]$command)\n Invoke-Expression $command\n }\n"
    );
}

fn decode_script(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("should not fail to decode")
}

#[cfg(unix)]
/// The `warp_escape_json` helper and the sed program it is built with, cut out of a bundled
/// bootstrap script: from `start` through the line `end` that closes the function.
fn warp_escape_json_snippet<'a>(script: &'a str, start: &str, end: &str) -> &'a str {
    let start = script
        .find(start)
        .expect("the bootstrap script builds WARP_ESCAPE_JSON_CTRL_SED");
    let definition = start
        + script[start..]
            .find("warp_escape_json")
            .expect("the bootstrap script defines warp_escape_json");
    let end = definition
        + script[definition..]
            .find(end)
            .expect("warp_escape_json is closed")
        + end.len();
    &script[start..end]
}

/// #866: runs the real `warp_escape_json` from each bundled script in the real shell, and checks
/// that its output is a valid JSON string body that round-trips exactly. Before the fix, ESC, BEL,
/// `\x01` and DEL reached the payload raw, which strict JSON rejects. A value that is not UTF-8
/// passes through byte for byte (the app decodes it lossily). A shell that is not installed is
/// skipped, visibly.
#[cfg(unix)]
#[test]
fn warp_escape_json_output_is_valid_json_in_every_shell() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    use std::process::Command;

    let bash = include_str!("../../assets/bundled/bootstrap/bash_body.sh");
    let zsh = include_str!("../../assets/bundled/bootstrap/zsh_body.sh");
    let fish = include_str!("../../assets/bundled/bootstrap/fish.sh");
    let shells: [(&str, &[&str], &str); 3] = [
        (
            "bash",
            &["--noprofile", "--norc"],
            warp_escape_json_snippet(bash, "WARP_ESCAPE_JSON_CTRL_SED=''", "\n    }\n"),
        ),
        (
            "zsh",
            &["-f"],
            warp_escape_json_snippet(zsh, "WARP_ESCAPE_JSON_CTRL_SED=''", "\n  }\n"),
        ),
        (
            "fish",
            &["--no-config"],
            warp_escape_json_snippet(fish, "set -g WARP_ESCAPE_JSON_CTRL_SED", "\nend\n"),
        ),
    ];
    let round_trips = [
        "/h/esc\u{1b}[31mRED\u{1b}[0m",
        "bell\u{7}ctl\u{1}\u{2}",
        "del\u{7f}",
        "emoji \u{1f600} rtl \u{202e}",
        "quote\" back\\slash",
        "tab\tcr\rff\u{c}bs\u{8}vt\u{b}",
        "line one\nline two",
    ];

    for (shell, flags, snippet) in shells {
        if !Command::new(shell)
            .args(flags)
            .args(["-c", "true"])
            .status()
            .is_ok_and(|status| status.success())
        {
            eprintln!("SKIPPED: {shell} is not installed");
            continue;
        }
        let escape = |value: &[u8]| -> Vec<u8> {
            let output = Command::new(shell)
                .args(flags)
                .arg("-c")
                .arg(format!("{snippet}\nwarp_escape_json \"$WARP_TEST_VALUE\""))
                .env("WARP_TEST_VALUE", OsStr::from_bytes(value))
                .output()
                .unwrap_or_else(|error| panic!("{shell} failed to run: {error}"));
            assert!(
                output.status.success(),
                "{shell}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            output.stdout
        };

        for value in round_trips {
            let escaped = escape(value.as_bytes());
            let json = [&b"\""[..], &escaped, b"\""].concat();
            let decoded: String = serde_json::from_slice(&json).unwrap_or_else(|error| {
                panic!(
                    "{shell}: {:?} escaped to invalid JSON {:?}: {error}",
                    value,
                    String::from_utf8_lossy(&escaped)
                )
            });
            assert_eq!(decoded, value, "{shell} did not round-trip");
        }

        assert_eq!(
            escape(b"bad\xff\xfe"),
            b"bad\xff\xfe",
            "{shell}: bytes that are not UTF-8 pass through unchanged"
        );
    }
}

/// `ShellType::PowerShell` used to `todo!()` in `init_subshell_script_for_shell` (#800,
/// `TODO(PLAT-750)`). This exercises the real bundled `pwsh_init_subshell.ps1` asset (not
/// a stub), so it also covers "the new .ps1 asset loads and contains the session-id
/// placeholder" -- the placeholder must be gone and the real session id substituted in.
#[test]
fn test_init_subshell_script_for_shell_powershell_loads_asset_and_uses_pwsh_syntax() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            let script = init_subshell_script_for_shell(
                ShellType::PowerShell,
                &crate::ASSETS,
                &[],
                SessionId::from(123456789u64),
                ctx,
            );

            // PowerShell needs its own `$env:` assignment syntax, not the POSIX
            // `export NAME=value;` the other shells use.
            assert!(
                script.starts_with("$env:WARP_HONOR_PS1 = '0';"),
                "expected PowerShell-syntax env setup, got: {script}"
            );
            assert!(
                script.contains("[uint64]123456789"),
                "expected the session id to be substituted into the loaded pwsh asset: \
                 {script}"
            );
            assert!(
                !script.contains("@@WARP_SESSION_ID@@"),
                "the session id placeholder should have been substituted: {script}"
            );
            assert!(
                script.contains("InitShell"),
                "expected the pwsh subshell script to emit the InitShell hook: {script}"
            );
            assert!(
                script.contains("is_subshell = $true"),
                "expected the pwsh subshell script to mark is_subshell true: {script}"
            );
        });
    });
}

/// #1116: a subshell started by the user (`powershell` typed in a tab) has no process execution
/// policy the app set, so under a stock Windows install (Restricted) the `. <tmp>.ps1` line that
/// sources the bootstrap is refused and the bootstrap never finishes. A line typed before the
/// script lifts the policy for the process, as `pwsh_init_shell.ps1` does; `pwsh.ps1` restores it
/// from the variable this saves.
#[test]
fn test_init_subshell_lines_powershell_lift_restricted_execution_policy_first() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            let lines = init_subshell_command_lines(
                Some(ShellType::PowerShell),
                &[],
                SessionId::from(123456789u64),
                ctx,
            );
            let lift = lines
                .iter()
                .position(|line| {
                    line.contains(
                        "Set-ExecutionPolicy -Scope Process -ExecutionPolicy RemoteSigned",
                    ) && line.contains("$global:_warp_PSProcessExecPolicy = ")
                })
                .expect("a line must lift a Restricted policy for the process");
            let init = lines
                .iter()
                .position(|line| line.contains("InitShell"))
                .expect("InitShell line");
            assert!(
                lift < init,
                "the policy must be lifted before the script runs"
            );
            assert!(
                lines[lift].starts_with(" if (-not $global:WARP_BOOTSTRAPPED) {"),
                "the history handler only filters lines that start with the guard"
            );
        });
    });
}

/// The generic `[ -z $WARP_BOOTSTRAPPED ] && eval '...'` guard `init_subshell_command` wraps
/// every other shell's subshell script in is not valid PowerShell syntax (#800) -- PowerShell
/// needs its own guard and must not be passed through `eval`.
#[test]
fn test_init_subshell_command_powershell_uses_powershell_guard_not_posix() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            let command =
                init_subshell_command(Some(ShellType::PowerShell), &[], SessionId::from(1u64), ctx);

            assert!(
                command.contains("if (-not $global:WARP_BOOTSTRAPPED)"),
                "expected a PowerShell-syntax bootstrap guard: {command}"
            );
            assert!(
                !command.contains("[ -z $WARP_BOOTSTRAPPED ]")
                    && !command.contains(r#"[ -z "${WARP_BOOTSTRAPPED-}" ]"#),
                "the POSIX guard is not valid PowerShell syntax: {command}"
            );
            assert!(
                !command.contains("eval '"),
                "PowerShell doesn't need the eval-wrapping bash/zsh/fish use: {command}"
            );
        });
    });
}

/// #839: the subshell guard is typed into a shell whose rcfiles already ran. Under bash
/// `set -u` / zsh `setopt nounset` a bare `$WARP_BOOTSTRAPPED` aborts the whole line, so bash
/// and zsh must use the nounset-safe `${WARP_BOOTSTRAPPED-}` -- and fish, which cannot parse
/// `${...}`, must keep the bare form. (#962: the guard is on the last, short line, which evals
/// the script the earlier lines accumulated.)
#[test]
fn test_init_subshell_command_guard_is_nounset_safe_for_bash_and_zsh_only() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            for shell_type in [ShellType::Bash, ShellType::Zsh] {
                let lines =
                    init_subshell_command_lines(Some(shell_type), &[], SessionId::from(1u64), ctx);
                let last = lines.last().unwrap();
                assert!(
                    last.contains(r#"[ -z "${WARP_BOOTSTRAPPED-}" ]"#)
                        && last.contains(r#"eval "$_wbs""#)
                        && last.contains(r#""${#_wbs}" -eq "#),
                    "expected a nounset-safe, length-checked guard for {shell_type:?}: {last}"
                );
                assert!(
                    !lines.iter().any(|l| l.contains("history -d")),
                    "{shell_type:?}: a numbered history deletion can remove the user's entry"
                );
            }

            let lines =
                init_subshell_command_lines(Some(ShellType::Fish), &[], SessionId::from(1u64), ctx);
            let last = lines.last().unwrap();
            assert!(
                last.starts_with(" [ -z $WARP_BOOTSTRAPPED ] && ") && last.contains("eval $_wbs"),
                "fish cannot parse `${{...}}`, so its guard must stay bare: {last}"
            );
            assert!(
                !lines.iter().any(|l| l.contains("${")),
                "fish cannot parse `${{...}}`: {lines:?}"
            );
        });
    });
}

/// Feeds the bash subshell bootstrap lines to a real bash on stdin under `locale`, optionally
/// dropping one accumulated chunk (a paced line lost on the way), and returns its stdout.
/// `extra` is a non-ASCII environment value so that characters and bytes differ.
fn run_bash_subshell_bootstrap(locale: &str, drop_a_chunk: bool) -> Option<String> {
    use crate::env_vars::{EnvVar, EnvVarValue};
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    // The test only needs a bash; a machine without one skips it.
    Command::new("bash").arg("--version").output().ok()?;
    let collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = collected.clone();
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let vars = [EnvVar {
                name: "PHOSPHOR_TEST_NON_ASCII".to_owned(),
                value: EnvVarValue::Constant("caf\u{e9} \u{2603}".to_owned()),
                description: None,
            }];
            *sink.lock().unwrap() = init_subshell_command_lines(
                Some(ShellType::Bash),
                &vars,
                SessionId::from(7u64),
                ctx,
            );
        });
    });
    let mut lines = collected.lock().unwrap().clone();
    if drop_a_chunk {
        // Line 0 is the history prelude, the last is the guarded eval; drop the second chunk.
        lines.remove(2);
    }
    let mut child = Command::new("bash")
        .args(["--norc", "--noprofile"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("LC_ALL", locale)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    for line in &lines {
        writeln!(stdin, "{line}").ok()?;
    }
    // A shell whose eval was skipped must still come back to its prompt and run this.
    writeln!(stdin, "echo PROMPT-REACHED").ok()?;
    drop(stdin);
    let output = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// #960: the length check on the last line compares `${#_wbs}` with the script's character count
/// or byte count, so it must pass whichever one the shell's locale counts: C counts bytes, a
/// UTF-8 locale counts characters. The script here has non-ASCII text so the two differ.
#[test]
fn bash_subshell_bootstrap_runs_under_c_and_utf8_locales() {
    // The hook's envelope: a DCS, or on Windows the OSC 9278 form, because the lines carry
    // `WARP_USING_WINDOWS_CON_PTY=true` there and ConPTY does not pass a DCS through.
    let hook_start = if cfg!(windows) {
        "\u{1b}]9278;d;"
    } else {
        "\u{1b}P$d"
    };
    for locale in ["C", "C.UTF-8", "POSIX"] {
        let Some(out) = run_bash_subshell_bootstrap(locale, false) else {
            return;
        };
        assert!(
            out.contains(hook_start) && out.contains("PROMPT-REACHED"),
            "LC_ALL={locale}: the InitShell hook was not emitted: {out:?}"
        );
    }
}

/// #960: when a line was lost the script is not evaluated, but the shell is not left hanging: it
/// says why and returns to the prompt (the app also stops holding input, see view.rs).
#[test]
fn bash_subshell_bootstrap_with_a_lost_chunk_reports_and_does_not_hang() {
    for locale in ["C", "C.UTF-8"] {
        let Some(out) = run_bash_subshell_bootstrap(locale, true) else {
            return;
        };
        assert!(
            !out.contains("\u{1b}P$d"),
            "LC_ALL={locale}: an incomplete script must not be evaluated: {out:?}"
        );
        assert!(
            out.contains("subshell setup skipped") && out.contains("PROMPT-REACHED"),
            "LC_ALL={locale}: no notice / the shell never returned: {out:?}"
        );
    }
}

/// #962: the subshell bootstrap is written while the new shell's tty may still be canonical,
/// where a line over MAX_CANON (1024 bytes on macOS) is truncated and the shell is left at a
/// continuation prompt. Every line written -- for every shell, with and without a large
/// environment, and for the shell-agnostic probe -- must stay under that limit, and must start
/// with a space so it is kept out of history (#967).
#[test]
fn test_init_subshell_lines_fit_in_a_canonical_tty_line_and_start_with_a_space() {
    use crate::env_vars::{EnvVar, EnvVarValue};
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            let many_vars: Vec<EnvVar> = (0..30)
                .map(|i| EnvVar {
                    name: format!("PHOSPHOR_TEST_VAR_{i}"),
                    value: EnvVarValue::Constant(format!("it's a value with 'quotes' {i}")),
                    description: None,
                })
                .collect();
            let shells = [
                Some(ShellType::Bash),
                Some(ShellType::Zsh),
                Some(ShellType::Fish),
                Some(ShellType::PowerShell),
                None,
            ];
            for shell_type in shells {
                for vars in [&[][..], &many_vars[..]] {
                    // The generated session id is 20 digits at most; use one.
                    let id = SessionId::from(u64::MAX);
                    let lines = init_subshell_command_lines(shell_type, vars, id, ctx);
                    assert!(!lines.is_empty());
                    // PowerShell's PSReadLine line editor is not a canonical tty; it is the
                    // one shell exempt from the line limit when the user adds variables.
                    let limit_applies = shell_type != Some(ShellType::PowerShell) || vars.is_empty();
                    for line in &lines {
                        assert!(
                            line.starts_with(' '),
                            "{shell_type:?}: a line without the leading space lands in history: {line}"
                        );
                        assert!(
                            !line.contains('\n') && !line.contains('\r'),
                            "{shell_type:?}: a line must not embed line breaks: {line}"
                        );
                        if limit_applies {
                            assert!(
                                line.len() < MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES,
                                "{shell_type:?}: {} byte line exceeds the canonical-mode limit: {line}",
                                line.len()
                            );
                        }
                    }
                }
            }
        });
    });
}

/// Splitting must not change the script: concatenating the quoted chunks (as the shell does)
/// and undoing the quoting gives back exactly the raw script, for every character that needs
/// escaping, including a single quote or backslash landing on a chunk boundary.
#[test]
fn test_chunked_eval_lines_round_trip() {
    let mut raw = String::new();
    for i in 0..400 {
        raw.push_str(&format!("echo '{i}' \\ \"q\" é;"));
    }
    for shell_type in [ShellType::Bash, ShellType::Zsh, ShellType::Fish] {
        let lines = chunked_eval_lines(shell_type, &raw);
        let mut rebuilt = String::new();
        for line in &lines {
            let Some(start) = line.find('\'') else {
                continue;
            };
            if !line.contains("_wbs") || line.contains("eval") {
                continue;
            }
            let end = line.len() - 1;
            let quoted = &line[start + 1..end];
            // Undo the shell's quoting: `'"'"'` is a literal quote; for fish `\\` is a backslash.
            let mut un = quoted.replace(r#"'"'"'"#, "'");
            if shell_type == ShellType::Fish {
                un = un.replace(r"\\", r"\");
            }
            rebuilt.push_str(&un);
        }
        assert_eq!(rebuilt, raw, "{shell_type:?}");
        // The first chunk assigns (a stale $_wbs must never be prepended), the others append.
        let chunk_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("_wbs") && !l.contains("eval"))
            .collect();
        assert!(
            chunk_lines[0].starts_with(" _wbs='") || chunk_lines[0].starts_with(" set -g _wbs '"),
            "{shell_type:?}: {}",
            chunk_lines[0]
        );
        assert!(chunk_lines[1..].iter().all(|l| l.contains("$_wbs")));
        let last = lines.last().unwrap();
        assert!(
            last.contains(&raw.len().to_string()),
            "{shell_type:?}: the final line must check the script length: {last}"
        );
        assert!(
            lines
                .iter()
                .all(|l| l.len() < MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES)
        );
    }
}

fn bundled_script(path: &str) -> String {
    let bytes = AssetProvider::get(&crate::ASSETS, path)
        .unwrap_or_else(|_| panic!("{path} should be a bundled asset"));
    String::from_utf8(bytes.to_vec()).expect("bootstrap scripts are UTF-8")
}

/// Assigning the hook list at the prompt (`PROMPT_COMMAND='echo pc'` in bash,
/// `precmd_functions=(myp)` in zsh) used to drop the precmd hook after the command's Preexec
/// had been sent, leaving its block running forever (#807, #828). The repairs live only in the
/// shell scripts, and the pin has neither, so a re-pin that takes upstream's scripts wholesale
/// would silently drop them. This pins the definitions and their registrations; DECLINED.md's
/// IMPROVED row keys its `keep:` markers on these names.
#[test]
fn test_bootstrap_scripts_repair_a_replaced_prompt_hook() {
    let bash = bundled_script("bundled/bootstrap/bash.sh");
    for needle in [
        "__bp_reinstall_prompt_command() {",
        "__bp_reinstall_prompt_command \"$__bp_status_at_trap\"",
    ] {
        assert!(
            bash.contains(needle),
            "bash.sh lost the #807 repair: missing `{needle}`"
        );
    }

    let zsh = bundled_script("bundled/bootstrap/zsh_body.sh");
    for needle in [
        "_warp_repair_hooks() {",
        "_warp_zle_line_init() {",
        "zle -N zle-line-init _warp_zle_line_init",
        "_warp_repair_hooks $? || :",
        "_WARP_PRECMD_PENDING=1",
    ] {
        assert!(
            zsh.contains(needle),
            "zsh_body.sh lost the #828 repair: missing `{needle}`"
        );
    }
}

/// zsh's lifecycle hooks must run with the user's `err_return`/`err_exit`/`nounset` neutralised
/// (#836), and a Preexec whose precmd was abandoned by a failing user precmd function must still
/// be finished from the zle-line-init repair (#844). Behaviour was checked by driving the script
/// in a pty; this pins the lines that do it.
#[test]
fn test_zsh_hooks_survive_strict_options_and_failing_precmd() {
    let zsh = bundled_script("bundled/bootstrap/zsh_body.sh");
    let guard = "setopt localoptions noerrreturn noerrexit unset\n";
    for hook in [
        "warp_preexec () {\n",
        "function warp_update_prompt_vars() {\n",
        "function warp_set_title_idle_on_precmd {\n",
        "function warp_set_title_active_on_preexec {\n",
    ] {
        let start = zsh
            .find(hook)
            .unwrap_or_else(|| panic!("zsh_body.sh lost `{hook}`"));
        let head = &zsh[start..start + 600.min(zsh.len() - start)];
        assert!(
            head.contains(guard.trim_end()),
            "`{hook}` must start with `{guard}` (#836)"
        );
    }
    // warp_title keeps its own `nopromptsubst` in the same setopt.
    let title = zsh
        .find("function warp_title {\n")
        .expect("zsh_body.sh lost `warp_title`");
    assert!(
        zsh[title..(title + 600).min(zsh.len())]
            .contains("setopt localoptions nopromptsubst noerrreturn noerrexit unset"),
        "warp_title must keep the strict-option guard (#836)"
    );
    // warp_precmd takes `$?` first, so its guard is not the first line; check it by
    // position within the function instead.
    let precmd = zsh
        .find("warp_precmd () {\n")
        .expect("zsh_body.sh lost `warp_precmd`");
    let head = &zsh[precmd..precmd + 1500.min(zsh.len() - precmd)];
    let status = head
        .find("local exit_code=$?")
        .expect("warp_precmd must capture `$?` first");
    assert!(
        head[status..].contains(guard.trim_end()),
        "warp_precmd must apply `{guard}` right after capturing `$?` (#836)"
    );
    assert!(
        zsh.contains("# Phosphor (#844)"),
        "zsh_body.sh lost the #844 owed-precmd repair"
    );
}

/// #935: a pasted multi-line command must title the tab with its first line and an ellipsis
/// instead of running the lines together ("gradle onegradle two"). Runs the real `warp_title`
/// of the bash bootstrap with a stub `__warp_emit`; skipped, visibly, when bash is missing.
#[cfg(unix)]
#[test]
fn bash_warp_title_shows_first_line_and_ellipsis_for_multiline_commands() {
    use std::process::Command;

    let bash = include_str!("../../assets/bundled/bootstrap/bash_body.sh");
    let start = bash
        .find("    warp_title () {\n")
        .expect("bash_body.sh lost `warp_title`");
    let end = start
        + bash[start..]
            .find("\n    }\n")
            .expect("warp_title has no end")
        + 7;
    let function = &bash[start..end];
    let script = format!("__warp_emit() {{ printf \"$@\"; }}\n{function}\nwarp_title \"$1\"\n");
    for (input, expected) in [
        ("gradle one\ngradle two", "gradle one…"),
        ("a\r\nb", "a…"),
        ("echo PL1\necho PL2\necho PL3", "echo PL1…"),
        ("ls -la", "ls -la"),
    ] {
        let output = match Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &script, "bash", input])
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                eprintln!("skipping: bash is not installed ({err})");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            stdout,
            format!("\u{1b}]0;{expected}\u{7}"),
            "input {input:?}"
        );
    }
}

/// #1094: with a PROMPT_COMMAND already set before bash-preexec installs (systemd's
/// `__systemd_osc_context_precmdline`, a distro's or the user's), `__bp_install` left its no-op
/// `:` token as the last PROMPT_COMMAND line. `__bp_in_prompt_command` then matched every command
/// whose first simple command is `:`, so `while :; do seq 1 100000; done` never got a Preexec: no
/// running title, and the loop's whole output landed in the block's command grid and became the
/// tab title after Ctrl-C. Runs the real bash-preexec install of `bash.sh`; skipped, visibly,
/// when bash is missing.
#[cfg(unix)]
#[test]
fn bash_preexec_install_leaves_no_colon_token_in_prompt_command() {
    use std::process::Command;

    let bash = include_str!("../../assets/bundled/bootstrap/bash.sh");
    let start = bash
        .find(" function define_bashpreexec_functions() {\n")
        .expect("bash.sh lost `define_bashpreexec_functions`");
    let end = start
        + bash[start..]
            .find("\n }\n")
            .expect("define_bashpreexec_functions has no end")
        + 4;
    let function = &bash[start..end];
    let script = format!(
        "__warp_run_relaxed() {{ \"$@\"; }}\n{function}\ndefine_bashpreexec_functions\n\
         PROMPT_COMMAND=$1\n__bp_install_after_session_init\n__bp_install\n\
         if __bp_in_prompt_command ':'; then echo COLON; else echo CLEAN; fi\n\
         printf '%s|' \"${{PROMPT_COMMAND[@]}}\"\n"
    );
    for existing in ["", "__systemd_osc_context_precmdline", "a; b", "x\ny"] {
        let output = match Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &script, "bash", existing])
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                eprintln!("skipping: bash is not installed ({err})");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("CLEAN\n"),
            "existing PROMPT_COMMAND {existing:?}: `:` is still a prompt command: {stdout:?} {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The user's own commands are kept, between our two hooks.
        assert!(
            stdout.contains("__bp_precmd_invoke_cmd")
                && stdout.contains("__bp_interactive_mode")
                && stdout.contains(existing),
            "existing PROMPT_COMMAND {existing:?} was not kept: {stdout:?}"
        );
    }
}

/// #934: a native command that inherits the terminal's stdin makes .NET reconfigure the tty
/// around the child, which drops the Enter the user typed ahead while the prompt ran (two lines
/// then run as one). Every native command the PowerShell bootstrap runs must get a redirected
/// stdin: pipe an empty string into it. This scans the bootstrap for call-operator invocations
/// and fails on any that is neither piped nor on the list of PowerShell-only callees below.
#[test]
fn powershell_bootstrap_native_calls_redirect_stdin() {
    // Callees that are PowerShell script blocks, not native processes: the user's own prompt
    // the PSConsoleHostReadLine wrapper, and the user's previous AddToHistoryHandler script block
    // (#967). A user's prompt that runs native commands is theirs.
    const NOT_NATIVE: [&str; 3] = [
        "& $global:_warpOriginalPrompt",
        "& $script:oldPSConsoleHostReadLine",
        "& $global:_warpPrevHistoryHandler",
    ];
    // Generator commands arrive as text and run through Invoke-Expression; they are not a
    // fixed call site, so they cannot take a pipe here.
    const NOT_A_CALL_SITE: [&str; 1] = ["Invoke-Expression -Command"];

    let files = [
        (
            "pwsh.ps1",
            include_str!("../../assets/bundled/bootstrap/pwsh.ps1"),
        ),
        (
            "pwsh_init_shell.ps1",
            include_str!("../../assets/bundled/bootstrap/pwsh_init_shell.ps1"),
        ),
        (
            "pwsh_init_subshell.ps1",
            include_str!("../../assets/bundled/bootstrap/pwsh_init_subshell.ps1"),
        ),
    ];
    let mut offenders = Vec::new();
    let mut piped = 0;
    for (name, text) in files {
        for (index, line) in text.lines().enumerate() {
            let code = line.trim();
            if code.starts_with('#') {
                continue;
            }
            let calls = code.contains("& $") || code.contains("&(");
            if !calls || NOT_NATIVE.iter().any(|ok| code.contains(ok)) {
                continue;
            }
            if NOT_A_CALL_SITE.iter().any(|ok| code.contains(ok)) {
                continue;
            }
            if code.contains("'' | &") {
                piped += 1;
            } else {
                offenders.push(format!("{name}:{}: {code}", index + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "native calls in the PowerShell bootstrap without a redirected stdin (pipe '' into them): {offenders:#?}"
    );
    assert!(
        piped >= 3,
        "expected the git, node and apt-config calls to be piped, saw {piped}"
    );
}

/// The zsh first-stage script is typed into the pty as ONE line (`enqueue_init_script`) while
/// the tty is still canonical, before zsh starts its line editor. macOS caps a canonical input
/// line at `MAX_CANON` = 1024 bytes including the newline; a longer line never completes, so zsh
/// never runs it and the pane hangs at "Starting shell" (v0.1.13 on macOS: the #879 escaping
/// loop took the line from 708 to 1054 bytes). Linux allows 4096, so only a Mac ever showed it.
#[test]
fn zsh_init_shell_line_fits_macos_max_canon() {
    const MACOS_MAX_CANON: usize = 1024;
    let script =
        init_shell_script_for_shell(ShellType::Zsh, &crate::ASSETS, SessionId::from(u64::MAX));
    let typed = script.len() + ShellType::Zsh.execute_command_bytes().len();
    assert!(
        typed <= MACOS_MAX_CANON,
        "the zsh init line is {typed} bytes typed; macOS drops a canonical line over \
         {MACOS_MAX_CANON} bytes and the shell never starts. Shorten zsh_init_shell.sh \
         (move work into zsh_body.sh)."
    );
    assert!(!script.contains('\n'), "it must stay a single line");
}

/// Local zsh/bash and every warpified-remote session receive the main bootstrap script as
/// raw typed input (`write_bootstrap_script_to_shell`), read line by line while the tty is
/// canonical. On macOS a canonical line longer than `MAX_CANON` (1024 bytes, newline included)
/// never completes, so the bootstrap stalls and the pane reports "Bootstrapping failed" (v0.1.13
/// on macOS: the ssh wrapper's embedded remote-zsh script grew one line to 1070 bytes in zsh,
/// 1106 in bash and 1105 in fish). Linux allows 4096, so only a Mac could show it.
#[test]
fn typed_bootstrap_lines_fit_macos_max_canon() {
    const MACOS_MAX_CANON: usize = 1024;
    for shell in [ShellType::Zsh, ShellType::Bash, ShellType::Fish] {
        let script = script_for_shell(shell, &crate::ASSETS);
        for (index, line) in script.split(|byte| *byte == b'\n').enumerate() {
            assert!(
                line.len() + 1 <= MACOS_MAX_CANON,
                "{shell:?} bootstrap line {} is {} bytes; macOS drops a canonical input line over \
                 {MACOS_MAX_CANON} bytes, so the bootstrap never finishes. Split it (a trailing \
                 backslash continues the line) or shorten it.",
                index + 1,
                line.len()
            );
        }
    }
}

/// Runs the real lines in a real interactive shell (stdin is a pipe, so no tty, but readline-less
/// `-i` shells still keep history) and checks the InitShell hook arrives and that nothing from the
/// bootstrap reaches the history file (#967). Skipped for a shell that is not installed.
#[test]
#[cfg(unix)]
fn test_init_subshell_lines_run_in_real_shells_and_stay_out_of_history() {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let cases: [(ShellType, &str, &[&str]); 3] = [
                (ShellType::Bash, "bash", &["--norc", "-i"]),
                (ShellType::Zsh, "zsh", &["-f", "-i"]),
                (ShellType::Fish, "fish", &["--no-config", "-i"]),
            ];
            for (shell_type, bin, args) in cases {
                let Ok(path) = Command::new("sh")
                    .args(["-c", &format!("command -v {bin}")])
                    .output()
                else {
                    continue;
                };
                let path = String::from_utf8_lossy(&path.stdout).trim().to_owned();
                if path.is_empty() {
                    eprintln!("skipping {bin}: not installed");
                    continue;
                }
                for hc in [None, Some("ignorespace"), Some("ignoreboth")] {
                    let dir = std::env::temp_dir().join(format!(
                        "phosphor-subshell-{bin}-{}-{}",
                        std::process::id(),
                        hc.unwrap_or("unset")
                    ));
                    std::fs::remove_dir_all(&dir).ok();
                    std::fs::create_dir_all(dir.join("data/fish")).unwrap();
                    let histfile = dir.join("history");
                    // The user's own history must survive byte for byte, whatever HISTCONTROL says.
                    std::fs::write(&histfile, "keep1\nkeep2\n").unwrap();
                    std::fs::write(
                        dir.join("data/fish/fish_history"),
                        "- cmd: keep1\n  when: 1\n- cmd: keep2\n  when: 2\n",
                    )
                    .unwrap();
                    let lines = init_subshell_command_lines(
                        Some(shell_type),
                        &[],
                        SessionId::from(4242u64),
                        ctx,
                    );
                    let mut command = Command::new(&path);
                    command.args(args).env_clear();
                    if let Some(hc) = hc {
                        command.env("HISTCONTROL", hc);
                    }
                    let mut child = command
                        .env("PATH", std::env::var("PATH").unwrap_or_default())
                        .env("HOME", &dir)
                        .env("HISTFILE", &histfile)
                        .env("HISTSIZE", "1000")
                        .env("SAVEHIST", "1000")
                        .env("XDG_DATA_HOME", dir.join("data"))
                        .env("XDG_CONFIG_HOME", dir.join("cfg"))
                        .env("TERM", "dumb")
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                        .unwrap();
                    {
                        let stdin = child.stdin.as_mut().unwrap();
                        for line in &lines {
                            stdin.write_all(line.as_bytes()).unwrap();
                            stdin.write_all(b"\n").unwrap();
                        }
                        stdin.write_all(b" echo done-marker\nexit\n").unwrap();
                    }
                    let out = child.wait_with_output().unwrap();
                    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                    assert!(
                        stdout.contains("\u{1b}P$d"),
                        "{bin}: no DCS hook in the output: {stdout:?}"
                    );
                    let hex: String = stdout
                        .split("\u{1b}P$d")
                        .nth(1)
                        .unwrap()
                        .chars()
                        .take_while(|c| c.is_ascii_hexdigit())
                        .collect();
                    let payload: Vec<u8> = (0..hex.len() / 2)
                        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
                        .collect();
                    let payload = String::from_utf8(payload).unwrap();
                    assert!(
                        payload.contains("InitShell") && payload.contains("\"is_subshell\": true"),
                        "{bin}: {payload}"
                    );
                    assert!(
                        payload.contains("4242"),
                        "{bin}: wrong session id in {payload}"
                    );
                    let mut history = std::fs::read_to_string(&histfile).unwrap_or_default();
                    history.push_str(
                        &std::fs::read_to_string(dir.join("data/fish/fish_history"))
                            .unwrap_or_default(),
                    );
                    assert!(
                        history.contains("keep1") && history.contains("keep2"),
                        "{bin} (HISTCONTROL={hc:?}): the user's history entries were lost: {history:?}"
                    );
                    assert!(
                        !history.contains("_wbs") && !history.contains("WARP_"),
                        "{bin} (HISTCONTROL={hc:?}): bootstrap lines reached the history file: {history:?}"
                    );
                    std::fs::remove_dir_all(&dir).ok();
                }
            }
        });
    });
}

/// #967: the shell-agnostic probe is typed into a shell nobody knows yet. Its first line turns
/// history-ignores-space on in bash and zsh (the one short line a shell can still record), so the
/// long probe line is not recorded, and the probe's tail puts the option back. Nothing in the
/// user's history file is lost, and fish and POSIX sh parse the lines without noise.
#[test]
#[cfg(unix)]
fn test_unknown_shell_probe_stays_out_of_history_and_restores_the_options() {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let cases: [(&str, &[&str]); 4] = [
                ("bash", &["--norc", "-i"]),
                ("zsh", &["-f", "-i"]),
                ("fish", &["--no-config", "-i"]),
                ("dash", &["-i"]),
            ];
            for (bin, args) in cases {
                let Ok(path) = Command::new("sh")
                    .args(["-c", &format!("command -v {bin}")])
                    .output()
                else {
                    continue;
                };
                let path = String::from_utf8_lossy(&path.stdout).trim().to_owned();
                if path.is_empty() {
                    eprintln!("skipping {bin}: not installed");
                    continue;
                }
                let lines = init_subshell_command_lines(None, &[], SessionId::from(4242u64), ctx);
                assert_eq!(lines.len(), 2, "a history prelude, then the probe");
                let dir = std::env::temp_dir()
                    .join(format!("phosphor-probe-{bin}-{}", std::process::id()));
                std::fs::remove_dir_all(&dir).ok();
                std::fs::create_dir_all(dir.join("data/fish")).unwrap();
                let histfile = dir.join("history");
                std::fs::write(&histfile, "keep1\nkeep2\n").unwrap();
                let mut child = Command::new(&path)
                    .args(args)
                    .env_clear()
                    .env("PATH", std::env::var("PATH").unwrap_or_default())
                    .env("HOME", &dir)
                    .env("HISTFILE", &histfile)
                    .env("HISTSIZE", "1000")
                    .env("SAVEHIST", "1000")
                    .env("HISTCONTROL", "ignoredups")
                    .env("XDG_DATA_HOME", dir.join("data"))
                    .env("XDG_CONFIG_HOME", dir.join("cfg"))
                    .env("TERM", "dumb")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                {
                    let stdin = child.stdin.as_mut().unwrap();
                    for line in &lines {
                        stdin.write_all(line.as_bytes()).unwrap();
                        stdin.write_all(b"\n").unwrap();
                    }
                    // A bash or zsh the probe names is followed by the known-shell stage, whose
                    // last line puts the options back (the probe keeps them on until then so that
                    // stage's first line is not recorded either).
                    let known = match bin {
                        "bash" => Some(ShellType::Bash),
                        "zsh" => Some(ShellType::Zsh),
                        _ => None,
                    };
                    if let Some(known) = known {
                        for line in chunked_eval_lines(known, "echo STAGE2-RAN") {
                            stdin.write_all(line.as_bytes()).unwrap();
                            stdin.write_all(b"\n").unwrap();
                        }
                    }
                    // `HISTCONTROL` must be back to the user's value after the probe.
                    stdin
                        .write_all(b" echo \"hc-after=[$HISTCONTROL]\"\n")
                        .unwrap();
                    if bin == "zsh" {
                        stdin
                            .write_all(
                                b" [[ -o hist_ignore_space ]] && echo whi-after=on || echo whi-after=off\n",
                            )
                            .unwrap();
                    }
                    stdin.write_all(b"exit\n").unwrap();
                }
                let out = child.wait_with_output().unwrap();
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                assert!(
                    stdout.contains("\u{1b}]9278;f;") && stdout.contains("InitSubshell"),
                    "{bin}: no probe hook in the output: {stdout:?}"
                );
                if bin == "bash" {
                    assert!(
                        stdout.contains("hc-after=[ignoredups]"),
                        "{bin}: HISTCONTROL was not restored: {stdout:?}"
                    );
                }
                if bin == "zsh" {
                    assert!(
                        stdout.contains("STAGE2-RAN") && stdout.contains("whi-after=off"),
                        "{bin}: hist_ignore_space was not restored: {stdout:?}"
                    );
                }
                for noise in ["not a valid", "nknown command", "syntax error", "unbound"] {
                    assert!(
                        !stderr.contains(noise) && !stdout.contains(noise),
                        "{bin}: the probe printed an error ({noise}): {stderr:?}"
                    );
                }
                let mut history = std::fs::read_to_string(&histfile).unwrap_or_default();
                history.push_str(
                    &std::fs::read_to_string(dir.join("data/fish/fish_history"))
                        .unwrap_or_default(),
                );
                if bin == "bash" {
                    assert!(
                        history.contains("keep1") && history.contains("keep2"),
                        "{bin}: the user's history entries were lost: {history:?}"
                    );
                }
                assert!(
                    !history.contains("WARP_BOOTSTRAPPED") && !history.contains("InitSubshell"),
                    "{bin}: the probe line reached the history file: {history:?}"
                );
                std::fs::remove_dir_all(&dir).ok();
            }
        });
    });
}

/// #967: the real probe-then-known-shell line stream, typed into real interactive bash and zsh
/// under every history setting that matters, leaves at most ONE Phosphor line in the history file
/// (the prelude that switches ignore-space on: a shell records a line before it runs it, so no
/// line can keep itself out of history) and none when the shell already ignored space. The user's
/// own entries survive byte for byte and the settings are exactly as they were afterwards.
#[test]
#[cfg(unix)]
fn test_probe_and_known_stage_leave_at_most_the_prelude_in_history() {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let probe = init_subshell_command_lines(None, &[], SessionId::from(4242u64), ctx);
            // (shell, shell type, env HISTCONTROL / zshrc lines, ignores space already)
            let mut cases: Vec<(&str, ShellType, String, bool)> = Vec::new();
            for hc in ["", "ignoredups", "ignorespace", "ignoreboth", "erasedups"] {
                cases.push((
                    "bash",
                    ShellType::Bash,
                    hc.to_owned(),
                    hc.contains("ignorespace") || hc.contains("ignoreboth"),
                ));
            }
            for (rc, ignores) in [
                ("", false),
                ("setopt hist_ignore_space", true),
                ("setopt inc_append_history", false),
                ("setopt share_history", false),
            ] {
                cases.push(("zsh", ShellType::Zsh, rc.to_owned(), ignores));
            }
            for (bin, shell_type, setting, ignores_space) in cases {
                let Ok(path) = Command::new("sh")
                    .args(["-c", &format!("command -v {bin}")])
                    .output()
                else {
                    continue;
                };
                let path = String::from_utf8_lossy(&path.stdout).trim().to_owned();
                if path.is_empty() {
                    eprintln!("skipping {bin}: not installed");
                    continue;
                }
                let dir = std::env::temp_dir().join(format!(
                    "phosphor-prelude-{bin}-{}-{}",
                    std::process::id(),
                    setting.replace(' ', "_")
                ));
                std::fs::remove_dir_all(&dir).ok();
                std::fs::create_dir_all(&dir).unwrap();
                let histfile = dir.join("history");
                std::fs::write(&histfile, "keep1\nkeep2\n").unwrap();
                // macOS's /etc/zshrc forces `HISTFILE=$ZDOTDIR/.zsh_history` AFTER the environment is
                // read, so the env var alone leaves the history in a file this test never reads
                // ("lost echo user1"). Setting it again in the user's .zshrc (which runs after
                // /etc/zshrc) pins it on every OS.
                std::fs::write(
                    dir.join(".zshrc"),
                    format!(
                        "HISTFILE='{}'\nHISTSIZE=1000\nSAVEHIST=1000\n{setting}\n",
                        histfile.display()
                    ),
                )
                .unwrap();
                let mut command = Command::new(&path);
                command
                    .args(if bin == "bash" {
                        &["--norc", "-i"][..]
                    } else {
                        &["-i"][..]
                    })
                    .env_clear()
                    .env("PATH", std::env::var("PATH").unwrap_or_default())
                    .env("HOME", &dir)
                    .env("ZDOTDIR", &dir)
                    .env("HISTFILE", &histfile)
                    .env("HISTSIZE", "1000")
                    .env("TERM", "dumb");
                if bin == "bash" && !setting.is_empty() {
                    command.env("HISTCONTROL", &setting);
                }
                let mut child = command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                {
                    let stdin = child.stdin.as_mut().unwrap();
                    let mut stream = vec!["echo user1".to_owned()];
                    stream.extend(probe.iter().cloned());
                    stream.extend(chunked_eval_lines(shell_type, "echo STAGE2-RAN"));
                    stream.push("echo user2".to_owned());
                    stream.push(if bin == "bash" {
                        "echo \"hc-after=[$HISTCONTROL] set=${HISTCONTROL+y}\"".to_owned()
                    } else {
                        "[[ -o hist_ignore_space ]] && echo whi-after=on || echo whi-after=off"
                            .to_owned()
                    });
                    stream.push("exit".to_owned());
                    for line in stream {
                        stdin.write_all(line.as_bytes()).unwrap();
                        stdin.write_all(b"\n").unwrap();
                    }
                }
                let out = child.wait_with_output().unwrap();
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                let why = format!("{bin} [{setting}]");
                assert!(
                    stdout.contains("STAGE2-RAN"),
                    "{why}: stage two did not run: {stdout:?}"
                );
                if bin == "bash" {
                    // Unset stays "set but empty" (the stage cannot tell the two apart); a set
                    // value comes back exactly.
                    assert!(
                        stdout.contains(&format!("hc-after=[{setting}]")),
                        "{why}: HISTCONTROL not restored: {stdout:?}"
                    );
                } else {
                    let expect = if ignores_space {
                        "whi-after=on"
                    } else {
                        "whi-after=off"
                    };
                    assert!(
                        stdout.contains(expect),
                        "{why}: option not restored: {stdout:?}"
                    );
                }
                let history = std::fs::read_to_string(&histfile).unwrap_or_default();
                for kept in ["keep1", "keep2", "echo user1", "echo user2"] {
                    assert!(history.contains(kept), "{why}: lost {kept}: {history:?}");
                }
                let bootstrap_lines: Vec<&str> = history
                    .lines()
                    .filter(|l| {
                        l.contains("__hc0")
                            || l.contains("__whi")
                            || l.contains("_wbs")
                            || l.contains("WARP_")
                            || l.contains("InitSubshell")
                            || l.contains("STAGE2")
                    })
                    .collect();
                if ignores_space {
                    assert!(
                        bootstrap_lines.is_empty(),
                        "{why}: bootstrap lines recorded: {bootstrap_lines:?}"
                    );
                } else {
                    assert!(
                        bootstrap_lines.len() <= 1
                            && bootstrap_lines
                                .iter()
                                .all(|l| l.trim_start()
                                    == UNKNOWN_SHELL_HISTORY_PRELUDE.trim_start()),
                        "{why}: more than the one prelude line recorded: {bootstrap_lines:?}"
                    );
                }
                std::fs::remove_dir_all(&dir).ok();
            }
        });
    });
}

/// #967: no bootstrap line deletes a history entry by number (the entry could be the user's own
/// when the shell never recorded our space-led line), and PowerShell keeps the big line out of
/// PSReadLine's history with a chained AddToHistoryHandler instead of rewriting the history file.
#[test]
fn test_init_subshell_never_deletes_or_rewrites_history() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            for shell_type in [
                Some(ShellType::Bash),
                Some(ShellType::Zsh),
                Some(ShellType::Fish),
                Some(ShellType::PowerShell),
                None,
            ] {
                let lines =
                    init_subshell_command_lines(shell_type, &[], SessionId::from(1u64), ctx);
                for line in &lines {
                    assert!(
                        !line.contains("history -d") && !line.contains("HISTCMD"),
                        "{shell_type:?}: {line}"
                    );
                    assert!(
                        !line.contains("BASH_VERSION"),
                        "{shell_type:?}: the unknown-shell probe must not use a bare $BASH_VERSION: {line}"
                    );
                }
            }
            let lines = init_subshell_command_lines(
                Some(ShellType::PowerShell),
                &[],
                SessionId::from(1u64),
                ctx,
            );
            assert!(lines.len() >= 2 && lines[0].contains("-AddToHistoryHandler"));
            assert!(lines[0].contains("_warpPrevHistoryHandler"), "must chain: {}", lines[0]);

            // #967: the handler also refuses the line that later sources the temp bootstrap file,
            // and the line that installs it carries the word PSReadLine's default handler keeps
            // out of the history file.
            assert!(lines[0].contains(r"\.tmp[A-Za-z0-9]{6}\.ps1"), "{}", lines[0]);
            assert!(lines[0].trim_end().ends_with("# apikey"), "{}", lines[0]);
            // #1122: and the line that sources the installed bootstrap file on Windows
            // (`<install dir>\Phosphor\pwsh.ps1`), which was saved to PSReadLine's history file.
            assert!(lines[0].contains(r"Phosphor[\\/]pwsh\.ps1"), "{}", lines[0]);
            // The pattern in the handler matches the line `source_bootstrap_script` types for a
            // `tempfile` path (same regex dialect for this subset), and not an ordinary command.
            let tmp_path = "/var/folders/ab/c_d/T/.tmpAbC123.ps1";
            let typed = format!(
                " . {}",
                warp_util::path::ShellFamily::PowerShell.escape(tmp_path)
            );
            let pattern = lines[0]
                .split("-match '")
                .nth(1)
                .and_then(|rest| rest.split("')").next())
                .expect("the handler has a bootstrap-file pattern");
            let regex = regex::Regex::new(pattern).unwrap();
            assert!(regex.is_match(typed.trim_start()), "{pattern} vs {typed}");
            assert!(!regex.is_match(". ./build.ps1"));
            assert!(!regex.is_match("Get-ChildItem .tmpAbC123.ps1"));
        });
    });
}

/// #995: the ssh wrapper hands the remote bootstrap to ssh through a variable, so the job text a
/// shell prints for a stopped ssh (Ctrl-Z during connect) is a short command line, not the whole
/// embedded script. The remote command itself is unchanged: a stub `ssh` on PATH receives it.
#[test]
#[cfg(unix)]
fn ssh_wrapper_job_text_is_short_and_remote_script_still_arrives() {
    use std::process::Command;

    for (name, source, fish) in [
        (
            "bash",
            include_str!("../../assets/bundled/bootstrap/bash_body.sh"),
            false,
        ),
        (
            "fish",
            include_str!("../../assets/bundled/bootstrap/fish.sh"),
            true,
        ),
    ] {
        // The statement that runs `command ssh ... -t <args>` with the remote script.
        let start = source
            .find(if fish {
                "$warp_ipqos_args -o ControlMaster=$control_master_mode"
            } else {
                "-o ControlMaster=$control_master_mode"
            })
            .unwrap_or_else(|| panic!("{name}: the wrapper's ssh invocation moved"));
        // (#1035: the background-master start before it, `... -N -f "$@"`, is a separate short
        // ssh call with no remote script; this is the session invocation that carries it.)
        let start = source[..start].rfind("command ssh").unwrap();
        let statement: String = source[start..]
            .lines()
            .take(2)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            statement.len() < 400 && !statement.contains("TERM_PROGRAM"),
            "{name}: the ssh invocation must not carry the script inline (#995): {statement}"
        );
        assert!(
            statement.contains("warp_remote_bootstrap"),
            "{name}: {statement}"
        );

        // Run the real helper with a stub ssh: the last argument must still be the whole script.
        let have = Command::new(name).arg("--version").output().is_ok();
        if !have {
            eprintln!("skipping {name}: not installed");
            continue;
        }
        let dir =
            std::env::temp_dir().join(format!("phosphor-ssh-stub-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let stub = dir.join("ssh");
        std::fs::write(
            &stub,
            "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in -G|-O) exit 1;; esac; done\nfor a in \"$@\"; do last=\"$a\"; done\nprintf '%s' \"$last\" > \"$OUT\"\n",
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (open, close) = if fish {
            ("    function warp_ssh_helper", "\n    end\n")
        } else {
            (
                "        function warp_ssh_helper() {",
                "\n        function __warp_ssh_impl",
            )
        };
        let from = source.find(open).unwrap();
        let to = from + source[from..].find(close).unwrap() + close.len();
        let function = if fish {
            source[from..to].to_owned()
        } else {
            source[from..to - "\n        function __warp_ssh_impl".len()].to_owned()
        };
        let out_file = dir.join("out");
        let prelude = if fish {
            "function warp_init_shell; echo init-$argv; end; function warp_send_json_message; end; set -g SSH_SOCKET_DIR /tmp/s; set -g WARP_SESSION_ID 77; set -g WARP_HONOR_PS1 1;"
        } else {
            "init_shell_hook(){ echo hook-$1; }; warp_send_json_message(){ :; }; SSH_SOCKET_DIR=/tmp/s; WARP_SESSION_ID=77; WARP_HONOR_PS1=1;"
        };
        let script = format!("{prelude}\n{function}\nwarp_ssh_helper -p 22 user@host");
        let mut command = Command::new(name);
        if fish {
            command.args(["--no-config", "-c", &script]);
        } else {
            command.args(["-c", &script]);
        }
        let status = command
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    dir.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("OUT", &out_file)
            .status()
            .unwrap();
        assert!(status.success(), "{name}: helper failed");
        let remote = std::fs::read_to_string(&out_file).unwrap_or_default();
        assert!(
            remote.contains("export TERM_PROGRAM='WarpTerminal'")
                && remote.contains("exec -l zsh -g"),
            "{name}: the remote script did not reach ssh intact: {remote:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// #1035 / #1039: the wrapper decides AFTER connecting, with ONE authentication. It starts the
/// Phosphor-owned ControlMaster in the background (`-N -f`), asks the remote what shell family it
/// runs over that master, then runs either the managed POSIX session or a plain interactive
/// session over the same master, and finally takes the master down. A stub `ssh` records every
/// invocation and simulates the master start, the probe answer and failures.
///
/// Every ssh the wrapper runs that CONNECTS (master, probe, session) must carry the user's own
/// arguments (`-p 22 user@host` here): a helper ssh without them would not see the user's
/// known_hosts / jump host / port (#1039). The control commands (`-O check` / `-O exit`) are the
/// opposite (#1083): they must carry the guards that make a fallback login impossible and must
/// NOT carry the user's arguments or config.
///
/// #1084: a dead socket left at the pane's ControlPath (SIGKILLed master) is removed before the
/// master starts, so exactly one live master is created; a LIVE master already there (another ssh
/// of this shell) is never touched and the ssh runs unwrapped. A failing `-O exit` is the last
/// ssh call: nothing retries or falls back.
#[test]
#[cfg(unix)]
fn ssh_wrapper_decides_after_connecting_over_one_master() {
    use std::process::Command;

    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipping: python3 is needed to bind the stub's control socket");
        return;
    }

    // (case, master behaviour, probe output, expected session: "plain" | "managed" | "none",
    //  expected helper exit status, extra ssh args)
    struct Case {
        name: &'static str,
        master: &'static str,
        probe: &'static str,
        session: &'static str,
        status: i32,
        extra: &'static [&'static str],
        /// What is already at the ControlPath: "" (nothing), "dead" (a socket nobody listens
        /// on, as a SIGKILLed master leaves it) or "live" (a listening master).
        pre: &'static str,
        /// `ssh -O exit` fails (the master is already gone / shutting down).
        exit_fails: bool,
    }
    let cases = [
        Case {
            name: "windows-cmd",
            master: "ok",
            probe: "warp-probe-C:\\Windows\\system32\\cmd.exe-$PSVersionTable",
            session: "plain",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        Case {
            name: "windows-powershell",
            master: "ok",
            probe: "warp-probe-%COMSPEC%-System.Management.Automation.PSVersionHashTable",
            session: "plain",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        Case {
            name: "posix",
            master: "ok",
            probe: "warp-probe-%COMSPEC%-",
            session: "managed",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        Case {
            name: "inconclusive",
            master: "ok",
            probe: "",
            session: "managed",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        Case {
            name: "windows-behind-jump",
            master: "ok",
            probe: "warp-probe-C:\\Windows\\system32\\cmd.exe-$PSVersionTable",
            session: "plain",
            status: 0,
            extra: &["-J", "jump"],
            pre: "",
            exit_fails: false,
        },
        Case {
            name: "posix-behind-jump",
            master: "ok",
            probe: "warp-probe-%COMSPEC%-",
            session: "managed",
            status: 0,
            extra: &["-J", "jump"],
            pre: "",
            exit_fails: false,
        },
        // #1626: authentication failed (ssh already printed why). Plain ssh asks for credentials
        // once per invocation, so ssh's status comes back and NOTHING else runs: no second ssh
        // whose password prompt would eat the user's next command. (#1551 expected a plain-ssh
        // retry here; that was the #1626 defect.)
        Case {
            name: "auth-failure",
            master: "fail255",
            probe: "",
            session: "none",
            status: 255,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        // #1551: ssh authenticated (its ControlPath socket exists) but cannot multiplex ("Failed to
        // connect to new control master", 255). The user's own plain ssh runs instead -- once,
        // with exactly the user's arguments -- and its status is the helper's status. Nothing
        // else (no probe, no -O exit) runs.
        Case {
            name: "mux-failure",
            master: "muxfail255",
            probe: "",
            session: "fallback",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        // #1551: Git Bash / MSYS2 (WARP_IN_MSYS2=true): no master at all, just the plain ssh.
        // Bash only (the zsh/fish wrappers do not run in Git Bash).
        Case {
            name: "msys2",
            master: "fail255",
            probe: "",
            session: "plainonly",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        // Ctrl-C while the master authenticates: ssh dies with 130, nothing else may run (no
        // probe, no session) and no master is left to clean up.
        Case {
            name: "interrupted",
            master: "fail130",
            probe: "",
            session: "none",
            status: 130,
            extra: &[],
            pre: "",
            exit_fails: false,
        },
        // #1084: a SIGKILLed master's dead socket is removed first, then ONE fresh master.
        Case {
            name: "dead-socket-at-start",
            master: "ok",
            probe: "warp-probe-%COMSPEC%-",
            session: "managed",
            status: 0,
            extra: &[],
            pre: "dead",
            exit_fails: false,
        },
        Case {
            name: "dead-socket-at-start-windows",
            master: "ok",
            probe: "warp-probe-C:\\Windows\\system32\\cmd.exe-$PSVersionTable",
            session: "plain",
            status: 0,
            extra: &["-J", "jump"],
            pre: "dead",
            exit_fails: false,
        },
        // #1084: a live master at the path is another ssh's: untouched, this ssh runs unwrapped.
        Case {
            name: "live-master-at-start",
            master: "ok",
            probe: "",
            session: "unwrapped",
            status: 0,
            extra: &[],
            pre: "live",
            exit_fails: false,
        },
        // #1083: `-O exit` fails (master already shutting down): no second attempt, no login.
        Case {
            name: "exit-fails",
            master: "ok",
            probe: "warp-probe-%COMSPEC%-",
            session: "managed",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: true,
        },
        Case {
            name: "exit-fails-windows",
            master: "ok",
            probe: "warp-probe-C:\\Windows\\system32\\cmd.exe-$PSVersionTable",
            session: "plain",
            status: 0,
            extra: &[],
            pre: "",
            exit_fails: true,
        },
    ];

    for (name, source, kind) in [
        (
            "bash",
            include_str!("../../assets/bundled/bootstrap/bash_body.sh"),
            0,
        ),
        (
            "zsh",
            include_str!("../../assets/bundled/bootstrap/zsh_body.sh"),
            1,
        ),
        (
            "fish",
            include_str!("../../assets/bundled/bootstrap/fish.sh"),
            2,
        ),
    ] {
        if Command::new(name).arg("--version").output().is_err() {
            eprintln!("skipping {name}: not installed");
            continue;
        }
        // From the helper through the probe / stop-master helpers that follow it.
        let (open, close) = match kind {
            0 => (
                "        function warp_ssh_helper() {",
                "\n        function __warp_ssh_impl",
            ),
            1 => (
                "      function warp_ssh_helper() {",
                "\n      function ssh() {",
            ),
            _ => ("    function warp_ssh_helper", "\n    function ssh\n"),
        };
        let from = source.find(open).unwrap();
        let to = from + source[from..].find(close).unwrap();
        let function = source[from..to].to_owned();

        // Each case runs twice: with an absolute SSH_SOCKET_DIR, and with the literal `~/.ssh`
        // the app really exports (HOME is a temp dir). With the tilde, the wrapper must still
        // find the master's socket (`-S` test), so the probe and the single-connection session
        // happen; it passes the tilde path on to ssh unexpanded, as ssh expands it itself.
        for (case, tilde) in cases.iter().flat_map(|c| [(c, false), (c, true)]) {
            if case.name == "msys2" && kind != 0 {
                continue;
            }
            let dir = std::env::temp_dir().join(format!(
                "p1035-{name}-{}-{}{}",
                cases.iter().position(|c| c.name == case.name).unwrap(),
                std::process::id(),
                if tilde { "t" } else { "" }
            ));
            std::fs::remove_dir_all(&dir).ok();
            let socks = if tilde {
                dir.join("home/.ssh")
            } else {
                dir.join("socks")
            };
            std::fs::create_dir_all(&socks).unwrap();
            let socket_dir_literal = if tilde {
                "~/.ssh".to_owned()
            } else {
                socks.display().to_string()
            };
            let msys2 = if case.name == "msys2" {
                "WARP_IN_MSYS2=true;"
            } else {
                ""
            };
            let prelude = match kind {
                2 => format!(
                    "function warp_init_shell; echo init-$argv; end; function warp_send_json_message; end; set -g SSH_SOCKET_DIR '{}'; set -g WARP_SESSION_ID 77; set -g WARP_HONOR_PS1 1;",
                    socket_dir_literal
                ),
                _ => format!(
                    "init_shell_hook(){{ echo hook-$1; }}; warp_send_json_message(){{ :; }}; SSH_SOCKET_DIR='{}'; WARP_SESSION_ID=77; WARP_HONOR_PS1=1; {msys2}",
                    socket_dir_literal
                ),
            };
            // The stub records every invocation (arguments one per line after a `CALL` marker),
            // answers -G with a minimal resolved config, and otherwise behaves like ssh would:
            //   -O check -> 0 when a master listens on the ControlPath, else 255;
            //   -O exit  -> tells that master to exit (it unlinks its socket), or 255 ($EXIT_FAILS
            //               or no master);
            //   -N       -> master start: fail, or start a listening master at the ControlPath
            //               that serves until told to exit. Like ssh, when the path is taken it
            //               says "already exists, disabling multiplexing" (recorded as MUXFAIL)
            //               and still succeeds -- the #1084 leak;
            //   probe    -> prints $PROBE;
            //   other    -> the session.
            let mux = dir.join("mux.py");
            std::fs::write(
                &mux,
                r#"import os, socket, sys
op, path = sys.argv[1], sys.argv[2]
if op in ("check", "exit"):
    c = socket.socket(socket.AF_UNIX)
    try:
        c.connect(path)
    except OSError:
        sys.exit(255)
    c.sendall(op.encode())
    c.close()
    sys.exit(0)
s = socket.socket(socket.AF_UNIX)
try:
    s.bind(path)
except OSError:
    with open(os.environ["OUT"], "a") as f:
        f.write("MUXFAIL\n")
    sys.exit(0)
s.listen(8)
if os.fork():
    sys.exit(0)
os.setsid()
null = os.open(os.devnull, os.O_RDWR)
for fd in (0, 1, 2):
    os.dup2(null, fd)
s.settimeout(30)
try:
    while True:
        c, _ = s.accept()
        if c.recv(16) == b"exit":
            break
except OSError:
    pass
os.unlink(path)
"#,
            )
            .unwrap();
            let stub = dir.join("ssh");
            std::fs::write(
                &stub,
                r#"#!/bin/sh
printf 'CALL\n' >> "$OUT"
for a in "$@"; do printf '%s\n' "$a" >> "$OUT"; done
p=
for a in "$@"; do case "$a" in ControlPath=*) p="${a#ControlPath=}"; case "$p" in "~"*) p="$HOME${p#\~}";; esac;; esac; done
case " $* " in
  *" -G "*) printf 'user u\nhostname h\nport 22\n'; exit 0;;
  *" -O check "*) python3 "$MUX" check "$p"; exit $?;;
  *" -O exit "*) test -n "$EXIT_FAILS" && exit 255; python3 "$MUX" exit "$p"; exit $?;;
  *" -N "*)
    case "$MASTER" in
      fail255) echo "user@host: Permission denied (publickey,password)." >&2; exit 255;;
      muxfail255) python3 "$MUX" serve "$p"; echo "Failed to connect to new control master" >&2; exit 255;;
      fail130) exit 130;;
    esac
    python3 "$MUX" serve "$p"
    exit 0;;
  *" -T "*) printf '%s\n' "$PROBE"; exit 0;;
esac
exit 0
"#,
            )
            .unwrap();
            // What is already at the pane's ControlPath before the wrapper runs.
            let socket_file = socks.join("77");
            let _live_master = match case.pre {
                "dead" => {
                    drop(std::os::unix::net::UnixListener::bind(&socket_file).unwrap());
                    None
                }
                "live" => Some(std::os::unix::net::UnixListener::bind(&socket_file).unwrap()),
                _ => None,
            };
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            let extra = case.extra.join(" ");
            let exit_status = if kind == 2 { "$status" } else { "$?" };
            let script = format!(
                "{prelude}\n{function}\nwarp_ssh_helper -p 22 {extra} user@host\nexit {exit_status}"
            );
            let mut command = Command::new(name);
            if kind == 2 {
                command.args(["--no-config", "-c", &script]);
            } else {
                command.args(["-c", &script]);
            }
            let out_file = dir.join("out");
            if tilde {
                command.env("HOME", dir.join("home"));
            } else {
                // The app exports an ABSOLUTE SSH_SOCKET_DIR: a $HOME that differs from where the
                // socket lives (isolated HOME, sudo -E, containers) must change nothing, and the
                // master/probe/session still make exactly one connection.
                std::fs::create_dir_all(dir.join("unrelated-home")).unwrap();
                command.env("HOME", dir.join("unrelated-home"));
            }
            let status = command
                .env(
                    "PATH",
                    format!(
                        "{}:{}",
                        dir.display(),
                        std::env::var("PATH").unwrap_or_default()
                    ),
                )
                .env("OUT", &out_file)
                .env("MASTER", case.master)
                .env("PROBE", case.probe)
                .env("MUX", &mux)
                .env("EXIT_FAILS", if case.exit_fails { "1" } else { "" })
                .stdin(std::process::Stdio::null())
                .status()
                .unwrap();
            let recorded = std::fs::read_to_string(&out_file).unwrap_or_default();
            // Stop a stub master the wrapper did not (exit-fails cases), so none outlives the test.
            if case.pre != "live" {
                if let Ok(mut master) = std::os::unix::net::UnixStream::connect(&socket_file) {
                    use std::io::Write as _;
                    let _ = master.write_all(b"exit");
                }
            }
            assert_eq!(
                status.code(),
                Some(case.status),
                "{name}/{}: helper status; calls: {recorded:?}",
                case.name
            );
            let all_calls: Vec<&str> = recorded
                .split("CALL\n")
                .filter(|call| !call.is_empty())
                .collect();
            // `ssh -G` config lookups are local-only; they still must carry the user's arguments.
            let calls: Vec<&str> = all_calls
                .iter()
                .copied()
                .filter(|call| !call.starts_with("-G\n"))
                .collect();
            let mut user_args = String::from("-p\n22\n");
            for extra in case.extra {
                user_args.push_str(extra);
                user_args.push('\n');
            }
            user_args.push_str("user@host\n");
            // #1083: a control command can never become a login: guarded, no user config, no
            // user arguments, a placeholder destination, the ControlPath of the pane.
            let is_control = |call: &&str| call.starts_with("-O\n") || call.contains("\n-O\n");
            for call in all_calls.iter().filter(|call| is_control(*call)) {
                for guard in [
                    "-F\n/dev/null\n",
                    "ControlMaster=no\n",
                    "ProxyCommand=false\n",
                    "BatchMode=yes\n",
                    "\n-n\n",
                ] {
                    assert!(
                        call.contains(guard),
                        "{name}/{}: -O call without {guard:?}: {call:?}",
                        case.name
                    );
                }
                assert!(
                    call.starts_with("-F\n/dev/null\n")
                        && call.ends_with("phosphor-mux-control\n")
                        && !call.contains("user@host")
                        && !call.contains("jump"),
                    "{name}/{}: -O call must not carry the user's arguments: {call:?}",
                    case.name
                );
                assert!(
                    call.contains(&format!("ControlPath={socket_dir_literal}/77\n")),
                    "{name}/{}: {call:?}",
                    case.name
                );
            }
            // #1039: every connecting helper ssh (master, probe, session) carries the user's own
            // arguments, so their known_hosts / port / jump host apply to it.
            for call in all_calls.iter().filter(|call| !is_control(*call)) {
                assert!(
                    call.contains(&user_args),
                    "{name}/{}: an ssh call without the user's arguments: {call:?}",
                    case.name
                );
            }
            // #1084: the master never hit an occupied path.
            assert!(
                !recorded.contains("MUXFAIL"),
                "{name}/{}: the master was started over an existing socket: {recorded:?}",
                case.name
            );
            let checks_before_master = calls
                .iter()
                .take_while(|call| !(call.contains("-N\n") && call.contains("-f\n")))
                .filter(|call| call.contains("-O\ncheck\n"))
                .count();
            assert_eq!(
                checks_before_master,
                usize::from(!case.pre.is_empty()),
                "{name}/{}: an occupied ControlPath is checked (once) before anything else: {recorded:?}",
                case.name
            );
            // #1172: the connect adds as few processes over plain ssh as it can. ONE `ssh -G` per
            // wrapped ssh (none when the wrapper runs the plain ssh before reading any config), and
            // an `-O check` only where the answer is needed: for an occupied ControlPath before the
            // master starts, never again once the master has been started and its socket is there.
            let g_calls = all_calls.len() - calls.len();
            assert_eq!(
                g_calls,
                usize::from(case.name != "msys2"),
                "{name}/{}: ssh -G runs once: {recorded:?}",
                case.name
            );
            let total_checks = calls.iter().filter(|c| c.contains("-O\ncheck\n")).count();
            assert_eq!(
                total_checks, checks_before_master,
                "{name}/{}: no -O check after the master is started: {recorded:?}",
                case.name
            );
            if case.session == "unwrapped" {
                // A live master of another ssh: not checked twice, not stopped, not replaced; this
                // ssh is the user's plain command.
                assert_eq!(calls.len(), 2, "{name}/{}: {recorded:?}", case.name);
                assert_eq!(calls[1], user_args, "{name}/{}: {recorded:?}", case.name);
                assert!(
                    socket_file.exists(),
                    "{name}/{}: live master removed",
                    case.name
                );
                std::fs::remove_dir_all(&dir).ok();
                continue;
            }
            if case.session == "plainonly" {
                // #1551: no master, no probe, no session options: only the user's own ssh.
                assert_eq!(calls.len(), 1, "{name}/{}: {recorded:?}", case.name);
                assert_eq!(calls[0], user_args, "{name}/{}: {recorded:?}", case.name);
                std::fs::remove_dir_all(&dir).ok();
                continue;
            }
            let is_master = |call: &&str| call.contains("-N\n") && call.contains("-f\n");
            let masters = calls.iter().filter(|call| is_master(*call)).count();
            assert_eq!(
                masters, 1,
                "{name}/{}: exactly one master: {recorded:?}",
                case.name
            );
            let master = calls.iter().find(|call| is_master(*call)).unwrap();
            assert!(
                master.contains("ControlMaster=yes") && master.contains("ControlPersist="),
                "{name}/{}: the master is Phosphor-owned with a safety-net persist: {master:?}",
                case.name
            );
            let exits = calls.iter().filter(|c| c.contains("-O\nexit\n")).count();
            let probes = calls.iter().filter(|c| c.contains("-T\n")).count();
            let sessions: Vec<&&str> = calls
                .iter()
                .filter(|c| c.contains("-t\n") && !is_master(c))
                .collect();
            match case.session {
                "fallback" => {
                    // #1551: the failed master, then the user's plain ssh and nothing else.
                    assert_eq!(calls.len(), 2, "{name}/{}: {recorded:?}", case.name);
                    assert_eq!(calls[1], user_args, "{name}/{}: {recorded:?}", case.name);
                    assert_eq!(
                        (probes, exits),
                        (0, 0),
                        "{name}/{}: {recorded:?}",
                        case.name
                    );
                    continue;
                }
                "none" => {
                    assert_eq!(
                        (calls.len(), probes, exits),
                        (1, 0, 0),
                        "{name}/{}: a failed connect runs nothing else: {recorded:?}",
                        case.name
                    );
                    continue;
                }
                kind_of_session => {
                    assert_eq!(probes, 1, "{name}/{}: one probe: {recorded:?}", case.name);
                    assert_eq!(
                        exits, 1,
                        "{name}/{}: the master is taken down at the end, exactly once: {recorded:?}",
                        case.name
                    );
                    if !case.exit_fails {
                        assert!(
                            !socket_file.exists(),
                            "{name}/{}: the master is gone after the session: {recorded:?}",
                            case.name
                        );
                    }
                    assert_eq!(sessions.len(), 1, "{name}/{}: {recorded:?}", case.name);
                    let session = sessions[0];
                    assert!(
                        session.contains("ControlMaster=no")
                            && session.contains(&format!("ControlPath={socket_dir_literal}/77\n")),
                        "{name}/{}: the session runs over the master: {session:?}",
                        case.name
                    );
                    // The exit is the last call, after the session.
                    assert!(
                        calls.last().unwrap().contains("-O\nexit\n"),
                        "{name}/{}: -O exit comes last: {recorded:?}",
                        case.name
                    );
                    if kind_of_session == "plain" {
                        assert!(
                            !session.contains("export TERM_PROGRAM"),
                            "{name}/{}: a non-POSIX remote must not get the POSIX bootstrap: {session:?}",
                            case.name
                        );
                    } else {
                        assert!(
                            session.contains("export TERM_PROGRAM='WarpTerminal'"),
                            "{name}/{}: a POSIX remote keeps the managed session: {session:?}",
                            case.name
                        );
                    }
                }
            }
            std::fs::remove_dir_all(&dir).ok();
        }
    }
}

/// #1039: no ssh the wrapper itself runs may omit the user's arguments. The old pre-connect
/// banner probe (ControlPath=none, its own option set) is gone; every remaining helper takes
/// the original arguments through.
#[test]
fn ssh_wrappers_have_no_pre_connect_banner_probe() {
    for (name, script) in [
        (
            "bash",
            include_str!("../../assets/bundled/bootstrap/bash_body.sh"),
        ),
        (
            "zsh",
            include_str!("../../assets/bundled/bootstrap/zsh_body.sh"),
        ),
        (
            "fish",
            include_str!("../../assets/bundled/bootstrap/fish.sh"),
        ),
    ] {
        assert!(
            !script.contains("remote software version")
                && !script.contains("PreferredAuthentications=none"),
            "{name}: the unauthenticated pre-connect banner probe must stay removed"
        );
    }
}

/// #960: `trap <fn> EXIT HUP` replaced bash's default hang-up handling, so a remote shell
/// neither exited on SIGHUP nor, while a foreground child such as `sudo` ran, ever did --
/// closing the tab left orphaned shells (and root shells under them). EXIT alone already runs
/// on a SIGHUP death.
#[test]
fn bash_body_does_not_trap_sighup() {
    let body = include_str!("../../assets/bundled/bootstrap/bash_body.sh");
    for line in body.lines() {
        let code = line.trim_start();
        if code.starts_with('#') {
            continue;
        }
        assert!(
            !(code.starts_with("trap ")
                && code.split_whitespace().any(|w| w == "HUP" || w == "SIGHUP")),
            "bash_body.sh must not trap HUP: {line}"
        );
    }
}

/// #960: a literal XOFF (0x13) or XON (0x11) in anything typed at a shell would, on a tty with
/// `IXON` set (the default on the pty `sudo -i` allocates), be eaten by the line discipline and
/// stop the shell's output. Nothing the app types may contain either byte, for any shell: the
/// scripts, the init-shell scripts and the paced subshell lines.
#[test]
fn typed_bootstrap_never_contains_xoff_or_xon_bytes() {
    fn assert_clean(what: &str, bytes: &[u8]) {
        for (index, byte) in bytes.iter().enumerate() {
            assert!(
                *byte != 0x13 && *byte != 0x11,
                "{what} contains flow-control byte {byte:#04x} at offset {index}"
            );
        }
    }
    for shell in [
        ShellType::Zsh,
        ShellType::Bash,
        ShellType::Fish,
        ShellType::PowerShell,
    ] {
        assert_clean(
            &format!("{shell:?} bootstrap script"),
            &script_for_shell(shell, &crate::ASSETS),
        );
        assert_clean(
            &format!("{shell:?} init-shell script"),
            init_shell_script_for_shell(shell, &crate::ASSETS, SessionId::from(1u64)).as_bytes(),
        );
    }
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            for shell_type in [
                Some(ShellType::Bash),
                Some(ShellType::Zsh),
                Some(ShellType::Fish),
                Some(ShellType::PowerShell),
                None,
            ] {
                for line in
                    init_subshell_command_lines(shell_type, &[], SessionId::from(u64::MAX), ctx)
                {
                    assert_clean(&format!("{shell_type:?} subshell line"), line.as_bytes());
                }
            }
        });
    });
}

/// #960: the full script reaches a subshell in writes small enough that a blocking user-space
/// pty relay (`sudo -i`) never has more pending than the inner tty can queue, and pacing never
/// reorders or drops a byte.
#[test]
fn subshell_bootstrap_chunks_are_small_and_lossless() {
    for shell in [ShellType::Zsh, ShellType::Bash, ShellType::Fish] {
        let script = script_for_shell(shell, &crate::ASSETS);
        let chunks = subshell_bootstrap_chunks(&script);
        assert!(chunks.len() > 1, "{shell:?}: the script should need pacing");
        assert!(SUBSHELL_BOOTSTRAP_CHUNK_SIZE < 4095);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.len() <= SUBSHELL_BOOTSTRAP_CHUNK_SIZE)
        );
        assert_eq!(chunks.concat(), script.to_vec(), "{shell:?}");
    }
    assert!(subshell_bootstrap_chunks(b"").is_empty());
}

/// Extracts `is_interactive_ssh_session` from a bundled bootstrap script: from its `function`
/// line to the first later line that is just `}` (bash/zsh) or `end` (fish) at the same indent.
fn extract_ssh_gate_function(script: &str, closer: &str) -> String {
    let mut out = Vec::new();
    let mut indent = None;
    for line in script.lines() {
        if indent.is_none() {
            if line
                .trim_start()
                .starts_with("function is_interactive_ssh_session")
            {
                indent = Some(line.len() - line.trim_start().len());
                out.push(line);
            }
            continue;
        }
        out.push(line);
        if line.trim() == closer && line.len() - line.trim_start().len() == indent.unwrap() {
            break;
        }
    }
    assert!(indent.is_some(), "is_interactive_ssh_session not found");
    out.join("\n")
}

/// #999: the ssh wrapper must only wrap an interactive login. `-O <cmd>`, `-N`, `-f` (also in
/// clusters), `-V`, `-G`, `-Q`, `-T`, `-W` and any remote command pass straight to real ssh,
/// while plain `ssh host` and option-with-argument forms still get the wrapper. Runs the real
/// function text from every bundled shell body, in that shell.
#[test]
fn ssh_wrapper_only_wraps_interactive_logins() {
    let sources = [
        (
            "bash",
            include_str!("../../assets/bundled/bootstrap/bash_body.sh"),
            "}",
        ),
        (
            "zsh",
            include_str!("../../assets/bundled/bootstrap/zsh_body.sh"),
            "}",
        ),
        (
            "fish",
            include_str!("../../assets/bundled/bootstrap/fish.sh"),
            "end",
        ),
    ];
    let wrapped: &[&[&str]] = &[
        &["host"],
        &["-p", "22", "host"],
        &["-p22", "host"],
        &["-o", "X=Y", "host"],
        &["-i", "key", "-l", "user", "host"],
        &["-L", "1:h:2", "-F", "cfg", "host"],
        &["-tt", "host"],
        &["-A", "-v", "host"],
        &["user@host"],
        // #1003: jump-host logins are interactive logins and are wrapped.
        &["-J", "j", "host"],
        &["-J", "u@j:2222", "host"],
        &["-J", "j1,j2", "host"],
        &["-Jj", "host"],
        &["-J", "j", "-p", "2200", "-A", "host"],
        &["-o", "ProxyJump=j", "host"],
        &["-oProxyJump=j", "host"],
        &["-oProxyCommand=nc %h %p", "host"],
        &["-o", "ProxyCommand=nc %h %p", "host"],
    ];
    let passthrough: &[&[&str]] = &[
        &["-O", "check", "host"],
        &["-O", "exit", "host"],
        &["-O", "stop", "host"],
        &["-O", "forward", "-L", "1:h:2", "host"],
        &["-p", "22", "-O", "check", "host"],
        &["-N", "host"],
        &["-f", "host"],
        &["-fN", "host"],
        &["-Nf", "-L", "1:h:2", "host"],
        &["-V"],
        &["-G", "host"],
        &["-Q", "cipher"],
        &["-W", "h:22", "host"],
        &["-T", "host"],
        &["host", "ls"],
        &["-p", "22", "host", "uptime"],
        // #1003: a jump host changes nothing about what makes a session non-interactive.
        &["-J", "j", "host", "cmd"],
        &["-J", "j", "-N", "host"],
        &["-J", "j", "-O", "check", "host"],
        &["-J", "j", "-W", "h:22", "host"],
        &["-J", "j", "-T", "host"],
        &["-J", "j", "-G", "host"],
        &["-J", "j", "-fN", "host"],
        &["-o", "ProxyJump=j", "-N", "host"],
        &["-oProxyJump=j", "host", "cmd"],
        // #1035: -w (tun device), -M and -S (user-chosen master / control socket) cannot ride
        // the wrapper's multiplexed session, so they run as plain ssh.
        &["-w", "any", "host"],
        &["-w", "0:0", "host"],
        &["-M", "host"],
        &["-S", "/tmp/ctl", "host"],
    ];
    // The Rust typing gate (`parse_interactive_ssh_command`) must agree with the shell wrappers
    // on every form above. (ssh word-splits the same way here: no argument contains a space
    // except the ProxyCommand ones, which are quoted for the parser.)
    let as_command = |args: &[&str]| {
        let words = args
            .iter()
            .map(|a| {
                if a.contains(' ') {
                    format!("'{a}'")
                } else {
                    (*a).to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("ssh {words}")
    };
    for args in wrapped {
        assert!(
            crate::terminal::ssh::util::parse_interactive_ssh_command(&as_command(args)).is_some(),
            "parser: {args:?} must be interactive"
        );
    }
    for args in passthrough {
        assert!(
            crate::terminal::ssh::util::parse_interactive_ssh_command(&as_command(args)).is_none(),
            "parser: {args:?} must not be interactive"
        );
    }
    // The jump spec is an option argument, never mistaken for the destination.
    for (cmd, host) in [
        ("ssh -J j host", "host"),
        ("ssh -J u@j:2222 host", "host"),
        ("ssh -J j1,j2 u@host", "u@host"),
        ("ssh -Jj host", "host"),
        ("ssh -o ProxyJump=j host", "host"),
    ] {
        let parsed = crate::terminal::ssh::util::parse_interactive_ssh_command(cmd).unwrap();
        assert_eq!(parsed.host.as_deref(), Some(host), "{cmd}");
    }
    for (shell, source, closer) in sources {
        if std::process::Command::new(shell)
            .arg("-c")
            .arg("true")
            .output()
            .is_err()
        {
            eprintln!("skipping {shell}: not installed");
            continue;
        }
        let function = extract_ssh_gate_function(source, closer);
        let run = |args: &[&str]| -> bool {
            let quoted = args
                .iter()
                .map(|a| format!("'{a}'"))
                .collect::<Vec<_>>()
                .join(" ");
            let status_var = if shell == "fish" { "$status" } else { "$?" };
            let script = format!(
                "{function}\nis_interactive_ssh_session {quoted}\necho RESULT=={status_var}"
            );
            let out = std::process::Command::new(shell)
                .args(["-c", &script])
                .output()
                .expect("run shell");
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains("RESULT==0")
        };
        for args in wrapped {
            assert!(run(args), "{shell}: {args:?} must still get the wrapper");
        }
        for args in passthrough {
            assert!(
                !run(args),
                "{shell}: {args:?} must pass straight through to ssh"
            );
        }
    }
}

/// #1014: the probe and the known-shell stage are typed into a shell whose rc files already ran,
/// so they must survive hostile settings: `set -euo pipefail` (bash), `ERR_EXIT NO_UNSET
/// PIPE_FAIL PRINT_EXIT_VALUE` (zsh) and `set -eu` (POSIX sh). The shell must stay alive, print
/// nothing extra (zsh's PRINT_EXIT_VALUE reports every non-zero line), run the staged script and
/// get its own options back. The old prelude counted `setopt | grep -c histignorespace`, which
/// exits 1 when the option is off and killed a zsh with ERR_EXIT.
#[test]
#[cfg(unix)]
fn test_bootstrap_lines_survive_errexit_nounset_pipefail_rc_settings() {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let probe = init_subshell_command_lines(None, &[], SessionId::from(4242u64), ctx);
            // (shell, args, rc file name, rc text, known-shell stage, extra zsh setting)
            let cases: [(&str, &[&str], &str, &str, Option<ShellType>); 4] = [
                ("bash", &["-i"], "rc", "set -euo pipefail\n", Some(ShellType::Bash)),
                (
                    "zsh",
                    &["-i"],
                    ".zshrc",
                    "setopt ERR_EXIT NO_UNSET PIPE_FAIL PRINT_EXIT_VALUE\n",
                    Some(ShellType::Zsh),
                ),
                (
                    "zsh",
                    &["-i"],
                    ".zshrc",
                    "setopt ERR_EXIT NO_UNSET PIPE_FAIL PRINT_EXIT_VALUE HIST_IGNORE_SPACE\n",
                    Some(ShellType::Zsh),
                ),
                ("dash", &["-i"], "rc", "set -eu\n", None),
            ];
            for (bin, args, rc_name, rc, known) in cases {
                let Ok(path) = Command::new("sh")
                    .args(["-c", &format!("command -v {bin}")])
                    .output()
                else {
                    continue;
                };
                let path = String::from_utf8_lossy(&path.stdout).trim().to_owned();
                if path.is_empty() {
                    eprintln!("skipping {bin}: not installed");
                    continue;
                }
                let dir = std::env::temp_dir().join(format!(
                    "phosphor-hostile-{bin}-{}-{}",
                    std::process::id(),
                    rc.len()
                ));
                std::fs::remove_dir_all(&dir).ok();
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(rc_name), rc).unwrap();
                let mut command = Command::new(&path);
                if bin == "bash" {
                    // GNU long options must come before `-i`.
                    command.arg("--rcfile").arg(dir.join(rc_name));
                }
                command.args(args);
                let mut child = command
                    .env_clear()
                    .env("PATH", std::env::var("PATH").unwrap_or_default())
                    .env("HOME", &dir)
                    .env("ZDOTDIR", &dir)
                    .env("ENV", dir.join(rc_name))
                    .env("HISTFILE", dir.join("history"))
                    .env("TERM", "dumb")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                {
                    let stdin = child.stdin.as_mut().unwrap();
                    let mut lines = probe.clone();
                    if let Some(known) = known {
                        lines.extend(chunked_eval_lines(known, "echo STAGE2-RAN"));
                    }
                    lines.push(" echo ALIVE-AFTER".to_owned());
                    if bin == "zsh" {
                        lines.push(
                            " print -r -- \"OPTS ee=$options[errexit] nu=$options[unset] pf=$options[pipefail] pev=$options[printexitvalue] hand=${WARP_RESTORE_PRINT_EXIT_VALUE-} whi=$options[histignorespace] residue=[${__whi-}${__wpe-}${_wbs-}]\""
                                .to_owned(),
                        );
                    }
                    if bin == "bash" {
                        lines.push(" echo \"OPTS $- residue=[${__hc0-}${_wbs-}]\"".to_owned());
                    }
                    for line in lines {
                        stdin.write_all(line.as_bytes()).unwrap();
                        stdin.write_all(b"\n").unwrap();
                    }
                    stdin.write_all(b"exit\n").unwrap();
                }
                let out = child.wait_with_output().unwrap();
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                let all = format!("{stdout}{stderr}");
                assert!(
                    all.contains("ALIVE-AFTER"),
                    "{bin} ({rc:?}): the shell died during the bootstrap: {all:?}"
                );
                assert!(
                    all.contains("InitSubshell"),
                    "{bin} ({rc:?}): the probe never reported: {all:?}"
                );
                if known.is_some() {
                    assert!(
                        all.contains("STAGE2-RAN"),
                        "{bin} ({rc:?}): the staged script did not run: {all:?}"
                    );
                }
                for noise in ["exit 1", "not found", "unbound", "parameter not set", "invalid"] {
                    assert!(
                        !all.contains(noise),
                        "{bin} ({rc:?}): bootstrap noise ({noise}): {all:?}"
                    );
                }
                if bin == "zsh" {
                    // print_exit_value is handed to the tail of zsh.sh (WARP_RESTORE_PRINT_EXIT_VALUE),
                    // not put back by the stage: the next typed line would print `zsh: exit 1`.
                    let hist_ignore_space = rc.contains("HIST_IGNORE_SPACE");
                    let expect = format!(
                        "OPTS ee=on nu=off pf=on pev=off hand=1 whi={} residue=[]",
                        if hist_ignore_space { "on" } else { "off" }
                    );
                    assert!(
                        stdout.contains(&expect),
                        "{bin} ({rc:?}): the user's options were not put back: {stdout:?}"
                    );
                }
                if bin == "bash" {
                    let opts = stdout
                        .lines()
                        .find_map(|l| l.strip_prefix("OPTS "))
                        .unwrap_or_default();
                    assert!(
                        stdout.contains("residue=[]")
                            && opts.split_whitespace().next().is_some_and(|f| {
                                f.contains('e') && f.contains('u')
                            }),
                        "{bin}: the user's options were not put back: {stdout:?}"
                    );
                }
                std::fs::remove_dir_all(&dir).ok();
            }
        });
    });
}

/// Drives a real interactive shell on a pty (python3's `pty`, so no extra dependency): types each
/// line of argv[3] followed by CR, then the contents of argv[4] (the full bootstrap script), then
/// the commands of argv[5], and prints everything the shell wrote, hex-escaped as `\xNN` for ESC
/// only so the assertions can match plain text. argv[1] is the shell binary, argv[2] the rc dir.
#[cfg(unix)]
const PTY_DRIVER: &str = r#"
import os, pty, select, sys, time, fcntl
shell, home, lines_file, script_file, cmds_file = sys.argv[1:6]
env = {"PATH": os.environ.get("PATH", ""), "HOME": home, "ZDOTDIR": home, "TERM": "xterm",
       "HISTFILE": os.path.join(home, "history")}
args = [shell, "-i"]
if os.path.basename(shell) == "bash":
    args = [shell, "--rcfile", os.path.join(home, "rc"), "-i"]
pid, fd = pty.fork()
if pid == 0:
    os.execve(shell, args, env)
fcntl.fcntl(fd, fcntl.F_SETFL, os.O_NONBLOCK | fcntl.fcntl(fd, fcntl.F_GETFL))
buf = b""
def pump(t):
    global buf
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                x = os.read(fd, 65536)
            except OSError:
                return
            if not x:
                return
            buf += x
def send(data):
    for i in range(0, len(data), 1000):
        chunk = data[i:i + 1000]
        while chunk:
            try:
                n = os.write(fd, chunk)
                chunk = chunk[n:]
            except BlockingIOError:
                pump(0.05)
            except OSError:
                return
        time.sleep(0.03)
pump(1.5)
for line in open(lines_file, "rb").read().split(b"\n"):
    if line:
        send(line + b"\r")
        pump(0.15)
pump(1.0)
send(open(script_file, "rb").read())
pump(0.5)
send(b"\r")
pump(6)
for cmd in open(cmds_file, "rb").read().split(b"\n"):
    if cmd:
        send(cmd + b"\r")
        pump(0.8)
sys.stdout.write(buf.decode("utf-8", "replace").replace("\x1b", "<ESC>").replace("\r", ""))
"#;

/// #1014 (round 2): the REAL bootstrap -- the probe, the real known-shell stage that emits
/// InitShell, and the real `zsh.sh` / `bash.sh` body -- typed into a real interactive shell on a
/// pty whose rc sets errexit / nounset / pipefail / print_exit_value. The first fix was checked
/// with a stub stage (`echo STAGE2-RAN`) and the real body still died on `$WARP_BOOTSTRAPPED`
/// under NO_UNSET, leaving the pane on "Starting shell...". The shell must survive, print no
/// bootstrap noise, report InitShell and Bootstrapped, run later commands, and the user's options
/// must be theirs again afterwards.
#[test]
#[cfg(unix)]
fn test_real_bootstrap_body_survives_errexit_nounset_pipefail_rc_settings() {
    use std::process::Command;

    let which = |bin: &str| -> Option<String> {
        let out = Command::new("sh")
            .args(["-c", &format!("command -v {bin}")])
            .output()
            .ok()?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (!path.is_empty()).then_some(path)
    };
    let Some(python) = which("python3") else {
        eprintln!("skipping: python3 not installed");
        return;
    };

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let probe = init_subshell_command_lines(None, &[], SessionId::from(4242u64), ctx);
            // (shell, rc file name, rc text, shell type)
            let cases = [
                ("zsh", ".zshrc", "setopt ERR_EXIT NO_UNSET PIPE_FAIL PRINT_EXIT_VALUE\n", ShellType::Zsh),
                (
                    "zsh",
                    ".zshrc",
                    "setopt ERR_EXIT ERR_RETURN NO_UNSET PIPE_FAIL PRINT_EXIT_VALUE HIST_IGNORE_SPACE\n",
                    ShellType::Zsh,
                ),
                ("bash", "rc", "set -euo pipefail\n", ShellType::Bash),
            ];
            for (bin, rc_name, rc, shell_type) in cases {
                let Some(shell) = which(bin) else {
                    eprintln!("skipping {bin}: not installed");
                    continue;
                };
                let dir = std::env::temp_dir().join(format!(
                    "phosphor-realbody-{bin}-{}-{}",
                    std::process::id(),
                    rc.len()
                ));
                std::fs::remove_dir_all(&dir).ok();
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(rc_name), rc).unwrap();

                let mut lines = probe.clone();
                lines.extend(init_subshell_command_lines(
                    Some(shell_type),
                    &[],
                    SessionId::from(4242u64),
                    ctx,
                ));
                std::fs::write(dir.join("lines.txt"), lines.join("\n")).unwrap();
                std::fs::write(
                    dir.join("script.txt"),
                    &*script_for_shell(shell_type, &crate::ASSETS),
                )
                .unwrap();
                let cmds = if bin == "zsh" {
                    " echo ALIVE=$((40+2)).\n print -r -- \"OPT pf=$options[pipefail] unset=$options[unset] pev=$options[printexitvalue]\"\n echo $phosphor_undefined_var_zz\n echo STILL=$((1+1)).\n"
                } else {
                    " echo ALIVE=$((40+2)).\n echo \"OPT $-\"\n echo STILL=$((1+1)).\n"
                };
                std::fs::write(dir.join("cmds.txt"), cmds).unwrap();

                let out = Command::new(&python)
                    .arg("-I")
                    .arg("-c")
                    .arg(PTY_DRIVER)
                    .arg(&shell)
                    .arg(&dir)
                    .arg(dir.join("lines.txt"))
                    .arg(dir.join("script.txt"))
                    .arg(dir.join("cmds.txt"))
                    .output()
                    .unwrap();
                let all = String::from_utf8_lossy(&out.stdout).into_owned();
                let case = format!("{bin} ({rc:?})");
                assert!(
                    all.contains("ALIVE=42."),
                    "{case}: the shell died during the real bootstrap: {all:?}"
                );
                assert!(
                    all.contains("<ESC>P$d"),
                    "{case}: no InitShell hook was emitted: {all:?}"
                );
                // The Bootstrapped hook is DCS-hex-encoded JSON; "Bootstrapped" in hex.
                assert!(
                    all.contains("426f6f747374726170706564"),
                    "{case}: the body never reported Bootstrapped: {all:?}"
                );
                let before_user_commands = all.split("ALIVE=42.").next().unwrap_or_default();
                for noise in ["\nzsh: exit ", ": parameter not set\n", ": unbound variable\n"] {
                    assert!(
                        !before_user_commands.contains(noise),
                        "{case}: bootstrap noise {noise:?}: {all:?}"
                    );
                }
                assert!(
                    all.contains("STILL=2."),
                    "{case}: the shell stopped accepting commands: {all:?}"
                );
                if bin == "zsh" {
                    assert!(
                        all.contains("OPT pf=on unset=off pev=on"),
                        "{case}: the user's options were not put back: {all:?}"
                    );
                    // Their NO_UNSET stays theirs: the undefined name is reported, the shell lives.
                    assert!(
                        all.contains("phosphor_undefined_var_zz: parameter not set"),
                        "{case}: the user's NO_UNSET was not restored: {all:?}"
                    );
                } else {
                    // `OPT <flags>` (the typed line echoes `OPT $-`, which starts with `$`).
                    let restored = all.match_indices("OPT ").any(|(at, _)| {
                        let flags = all[at + 4..].split_whitespace().next().unwrap_or_default();
                        !flags.starts_with('$') && flags.contains('e') && flags.contains('u')
                    });
                    assert!(restored, "{case}: set -eu was not put back: {all:?}");
                }
                std::fs::remove_dir_all(&dir).ok();
            }
        });
    });
}

/// #974: the typed init lines are the first thing a shell with `setopt PRINT_EXIT_VALUE` runs, and
/// `if [ cond ]; then ..; fi` with a false condition counts as a failed statement there, so it
/// printed `zsh: exit 1`. A source check that needs no shell.
#[test]
fn zsh_init_lines_have_no_failing_condition_statement() {
    for file in [
        include_str!("../../assets/bundled/bootstrap/zsh_init_shell.sh"),
        include_str!("../../assets/bundled/bootstrap/zsh_init_subshell.sh"),
    ] {
        for line in file.lines().filter(|l| !l.trim_start().starts_with('#')) {
            assert!(
                !line.trim_start().starts_with("if [ "),
                "a bare `if [ .. ]` prints `zsh: exit 1` under PRINT_EXIT_VALUE; use `case`: {line}"
            );
        }
    }
}

/// #974: the MAIN (non-subshell) zsh bootstrap -- the init-shell line, then the real `zsh.sh` --
/// typed into a real interactive zsh whose rc only does `setopt PRINT_EXIT_VALUE`. Nothing of
/// Phosphor's may print `zsh: exit N` (the old init line's `if [ .. ]`, and zsh.sh's own `read`
/// at the end of the heredoc, which fails by design, both did); the user's option must be back
/// afterwards; and a plain `preexec()` function of the user's, which zsh runs before the
/// `preexec_functions` array, must write after Phosphor's Preexec hook, not before it (that
/// output was drawn inside the command header).
#[test]
#[cfg(unix)]
fn test_main_zsh_bootstrap_prints_no_exit_value_and_orders_user_preexec() {
    use std::process::Command;

    let which = |bin: &str| -> Option<String> {
        let out = Command::new("sh")
            .args(["-c", &format!("command -v {bin}")])
            .output()
            .ok()?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (!path.is_empty()).then_some(path)
    };
    let (Some(python), Some(zsh)) = (which("python3"), which("zsh")) else {
        eprintln!("skipping: python3 or zsh not installed");
        return;
    };

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let _ = ctx;
            let dir =
                std::env::temp_dir().join(format!("phosphor-974-main-{}", std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(".zshrc"),
                "setopt PRINT_EXIT_VALUE\npreexec() { print -u2 -r -- \"USERPREEXEC-ERR\"; }\n",
            )
            .unwrap();
            let init = init_shell_script_for_shell(
                ShellType::Zsh,
                &crate::ASSETS,
                SessionId::from(4242u64),
            );
            std::fs::write(dir.join("lines.txt"), init).unwrap();
            std::fs::write(
                dir.join("script.txt"),
                &*script_for_shell(ShellType::Zsh, &crate::ASSETS),
            )
            .unwrap();
            std::fs::write(
                dir.join("cmds.txt"),
                " echo ALIVE=$((40+2)).\n print -r -- \"OPT pev=$options[printexitvalue]\"\n",
            )
            .unwrap();
            let out = Command::new(&python)
                .arg("-I")
                .arg("-c")
                .arg(PTY_DRIVER)
                .arg(&zsh)
                .arg(&dir)
                .arg(dir.join("lines.txt"))
                .arg(dir.join("script.txt"))
                .arg(dir.join("cmds.txt"))
                .output()
                .unwrap();
            let all = String::from_utf8_lossy(&out.stdout).into_owned();
            assert!(all.contains("ALIVE=42."), "the shell died: {all:?}");
            assert!(
                all.contains("426f6f747374726170706564"),
                "the body never reported Bootstrapped: {all:?}"
            );
            assert!(
                !all.contains("\nzsh: exit "),
                "bootstrap printed an exit value: {all:?}"
            );
            assert!(
                all.contains("OPT pev=on"),
                "the user's PRINT_EXIT_VALUE was not put back: {all:?}"
            );
            // Preexec hook ("Preexec" in hex) for the last command, then the user's stderr.
            let preexec = all
                .rfind("7b22686f6f6b223a202250726565786563")
                .expect("a Preexec hook");
            let user_err = all
                .rfind("USERPREEXEC-ERR")
                .expect("the user's preexec ran");
            assert!(
                preexec < user_err,
                "the user's preexec output came before the Preexec hook: {all:?}"
            );
            std::fs::remove_dir_all(&dir).ok();
        });
    });
}

// ---------------------------------------------------------------------------------------------
// nushell
// ---------------------------------------------------------------------------------------------

/// The nushell shipped on this machine: `PATH`, then the Homebrew locations (Linuxbrew, Apple
/// Silicon, Intel). `None` makes the real-shell tests below skip, visibly.
#[cfg(unix)]
fn find_nu() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("nu"))
                .collect()
        })
        .unwrap_or_default();
    for fixed in [
        "/home/linuxbrew/.linuxbrew/bin/nu",
        "/opt/homebrew/bin/nu",
        "/usr/local/bin/nu",
    ] {
        candidates.push(fixed.into());
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// Drives a real interactive `nu` on a pty (python3's `pty`, like the other real-shell tests): sends
/// every step of the spec in order, answers nushell's cursor-position queries the way a terminal
/// would, and prints a JSON object with the decoded Phosphor hooks (`hooks`) and the raw output.
/// argv[1] is a JSON spec: `{nu, args, home, steps: [str | {"wait": secs}]}`.
#[cfg(unix)]
const NU_PTY_DRIVER: &str = r#"
import json, os, pty, re, select, sys, time, fcntl, termios, struct
spec = json.load(open(sys.argv[1]))
home = spec["home"]
env = dict(os.environ)
env.update({"HOME": home, "XDG_CONFIG_HOME": home + "/cfg", "XDG_DATA_HOME": home + "/data",
            "TERM": "xterm-256color", "LANG": "C.UTF-8", "WARP_IS_LOCAL_SHELL_SESSION": "1"})
env.update(spec.get("env", {}))
os.makedirs(home + "/cfg/nushell", exist_ok=True)
open(home + "/cfg/nushell/env.nu", "w").write("")
open(home + "/cfg/nushell/config.nu", "w").write(spec.get("config", ""))
pid, fd = pty.fork()
if pid == 0:
    os.chdir(home)
    os.execve(spec["nu"], [spec["nu"]] + spec["args"], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
out = b""
seen = 0
def drain(t):
    global out, seen
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                data = os.read(fd, 65536)
            except OSError:
                return
            if not data:
                return
            out += data
            while seen < out.count(b"\x1b[6n"):
                os.write(fd, b"\x1b[1;1R")
                seen += 1
drain(2.0)
for step in spec["steps"]:
    if isinstance(step, dict):
        drain(step["wait"])
        continue
    os.write(fd, step.encode())
    drain(0.5 + 0.012 * len(step))
drain(1.0)
try:
    os.kill(pid, 9)
    os.waitpid(pid, 0)
except OSError:
    pass
text = out.decode("latin-1")
hooks = []
for m in re.finditer(r"\x1bP\$d([0-9a-f]*)\x9c", text):
    try:
        hooks.append(json.loads(bytes.fromhex(m.group(1)).decode()))
    except Exception as e:
        hooks.append({"hook": "UNPARSEABLE", "error": str(e), "hex": m.group(1)[:200]})
for m in re.finditer(r"\x1b\]9277;A\x07(\d+);([0-9a-f]*)\x1b\]9277;B\x07", text):
    hooks.append({"hook": "GeneratorOutput", "length": int(m.group(1)), "text": bytes.fromhex(m.group(2)).decode("utf-8", "replace")})
print(json.dumps({"hooks": hooks, "raw": text}))
"#;

/// Result of one [`run_nu_session`].
#[cfg(unix)]
struct NuSession {
    hooks: Vec<serde_json::Value>,
    raw: String,
    home: std::path::PathBuf,
}

#[cfg(unix)]
impl NuSession {
    fn hook_names(&self) -> Vec<String> {
        self.hooks
            .iter()
            .map(|hook| hook["hook"].as_str().unwrap_or("?").to_owned())
            .collect()
    }

    fn hooks_named(&self, name: &str) -> Vec<&serde_json::Value> {
        self.hooks
            .iter()
            .filter(|hook| hook["hook"] == name)
            .collect()
    }

    fn history(&self) -> String {
        std::fs::read_to_string(self.home.join("cfg/nushell/history.txt")).unwrap_or_default()
    }
}

/// Runs `nu` with `args` and `config` (its config.nu) in a scratch home, sends `steps`, and
/// returns what it printed. `None` when nu or python3 is missing.
#[cfg(unix)]
fn run_nu_session(
    tag: &str,
    args: &[String],
    config: &str,
    env: &[(&str, &str)],
    steps: &[String],
) -> Option<NuSession> {
    use std::process::Command;

    let Some(nu) = find_nu() else {
        eprintln!("skipping: nu is not installed");
        return None;
    };
    let python_ok = Command::new("python3")
        .args(["-c", "import pty"])
        .status()
        .is_ok_and(|status| status.success());
    if !python_ok {
        eprintln!("skipping: python3 is not installed");
        return None;
    }
    // The caller may already have written the bootstrap file into this directory.
    let home = std::env::temp_dir().join(format!("phosphor-nu-{tag}-{}", std::process::id()));
    for stale in ["cfg", "data"] {
        let _ = std::fs::remove_dir_all(home.join(stale));
    }
    std::fs::create_dir_all(&home).unwrap();
    let spec = serde_json::json!({
        "nu": nu,
        "args": args,
        "home": home,
        "config": config,
        "env": env.iter().cloned().collect::<std::collections::HashMap<_, _>>(),
        "steps": steps,
    });
    let spec_path = home.join("spec.json");
    std::fs::write(&spec_path, spec.to_string()).unwrap();
    let driver_path = home.join("driver.py");
    std::fs::write(&driver_path, NU_PTY_DRIVER).unwrap();
    let output = Command::new("python3")
        .arg(&driver_path)
        .arg(&spec_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    Some(NuSession {
        hooks: result["hooks"].as_array().cloned().unwrap_or_default(),
        raw: result["raw"].as_str().unwrap_or_default().to_owned(),
        home,
    })
}

/// The temp file the app would write the full bootstrap script to and `source`.
#[cfg(unix)]
fn write_nu_bootstrap_file(home: &std::path::Path) -> String {
    let path = home.join("bootstrap.nu");
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(&path, script_for_shell(ShellType::Nu, &crate::ASSETS)).unwrap();
    path.display().to_string()
}

#[test]
fn test_nu_scripts_are_loaded_and_have_no_leftover_placeholders() {
    let script = script_for_shell(ShellType::Nu, &crate::ASSETS);
    let bootstrap = decode_script(&script);
    assert!(bootstrap.ends_with('\n'));
    // Comments and blank lines are dropped to keep the file small.
    assert!(
        !bootstrap
            .lines()
            .any(|line| line.trim_start().starts_with('#'))
    );
    assert!(!bootstrap.lines().any(|line| line.trim().is_empty()));
    // The generator wrapper's name is a protocol surface shared with the Rust app.
    assert!(
        bootstrap.contains(&format!(
            "def --env {} [",
            crate::terminal::model::session::command_executor::POSIX_GENERATOR_WRAPPER
        )),
        "nu.nu must define the generator wrapper the in-band executor calls"
    );

    let init = init_shell_script_for_shell(ShellType::Nu, &crate::ASSETS, SessionId::from(4242u64));
    assert!(init.contains("\"4242\""), "{init}");
    assert!(!init.contains("@@"), "{init}");
    assert!(
        !init.contains('\n'),
        "the init script is one --execute argument line: {init}"
    );
    // Every statement ends in the DCS hook; nothing is deleted from history.
    assert!(init.contains("InitShell") && init.contains("\"shell\": \"nu\""));
    assert!(!init.contains("history -c") && !init.contains("history.txt"));
}

#[test]
#[cfg(feature = "local_fs")]
fn test_nu_is_bootstrapped_through_a_sourced_file() {
    use crate::terminal::model::session::SessionInfo;

    // nushell cannot eval a string, so the temp-file route is not optional for it.
    let info = SessionInfo::new_for_test().with_shell_type(ShellType::Nu);
    assert!(should_use_rc_file_bootstrap_method(ShellType::Nu, &info));
}

#[test]
fn test_nu_init_subshell_lines_are_short_independent_lines() {
    use crate::env_vars::{EnvVar, EnvVarValue};

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let hostile = EnvVar {
                name: "WEIRD NAME".to_owned(),
                value: EnvVarValue::Constant(
                    "it's a '# \"quoted\" \\ value\nwith a newline".to_owned(),
                ),
                description: None,
            };
            let lines = init_subshell_command_lines(
                Some(ShellType::Nu),
                &[hostile],
                SessionId::from(4242u64),
                ctx,
            );
            assert!(lines.len() >= 5, "{lines:?}");
            for line in &lines {
                assert!(
                    line.starts_with(' '),
                    "history.ignore_space_prefixed needs a space: {line:?}"
                );
                assert!(
                    !line.contains('\n') && !line.contains('\r'),
                    "each typed line must be one line: {line:?}"
                );
                assert!(
                    line.len() <= MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES,
                    "{} bytes: {line:?}",
                    line.len()
                );
            }
            // Only the first line may touch the history setting, and it saves the prior value.
            assert!(lines[0].contains("$env.__wbh = $env.config.history.ignore_space_prefixed"));
            assert!(
                lines[1..].iter().all(|l| !l.contains("history.")),
                "{lines:?}"
            );
            // Nothing ever deletes a history entry (#967).
            assert!(
                !lines
                    .iter()
                    .any(|l| l.contains("history -") || l.contains("history.txt"))
            );
            // The raw string delimiter has more `#`s than the longest run in the script.
            assert!(lines[1].starts_with(" $env._wbs = r##'"), "{}", lines[1]);
            // The last lines write, source and clean up the stage-one file.
            let n = lines.len();
            assert!(lines[n - 3].contains("save -f $_wbf"));
            assert_eq!(lines[n - 2], " source $_wbf");
            assert!(lines[n - 1].starts_with(" rm -f $_wbf"));
            assert!(lines[n - 3].contains("phosphor-nu-4242.nu"));
            assert!(
                !lines[n - 3].contains("/tmp"),
                "the file lives in the user's cache dir"
            );
        });
    });
}

/// The whole local flow in a real nushell: `--execute <init>` prints InitShell, the bootstrap file
/// is sourced by a space-led line, and every hook arrives with the payload shape fish.sh uses.
#[test]
#[cfg(unix)]
fn test_nu_real_local_bootstrap_emits_hooks_and_keeps_history_clean() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let init = app.read(|_| {
            init_shell_script_for_shell(ShellType::Nu, &crate::ASSETS, SessionId::from(4242u64))
        });
        let home = std::env::temp_dir().join(format!("phosphor-nu-local-{}", std::process::id()));
        let bootstrap = write_nu_bootstrap_file(&home);
        let steps = vec![
            format!(" source '{bootstrap}'\r"),
            "echo hi\r".to_owned(),
            "^sh -c 'exit 7'\r".to_owned(),
            // A user command with a control character and quotes in it.
            "echo \"a\\tb\"\r".to_owned(),
            "ls /definitely/not/here\r".to_owned(),
            "cd /tmp\r".to_owned(),
            // Typeahead report (Alt-i) while text is in the buffer, then the kill-buffer chord.
            "half typed".to_owned(),
            "\x1bi".to_owned(),
            "\x10".to_owned(),
            "echo after\r".to_owned(),
            // A multi-line command through bracketed paste, as the app sends it.
            "\x10\x1b[200~if true {\n    echo multi\n}\x1b[201~\r".to_owned(),
            "clear\r".to_owned(),
        ];
        let Some(session) = run_nu_session(
            "local",
            &["--login".to_owned(), "--execute".to_owned(), init],
            // A user config that records space-led lines: the init script must still hide ours.
            // nushell ships no default aliases (0.116), so the config defines one to prove
            // `Bootstrapped.aliases` reports the shell's real alias table.
            "$env.config.history.ignore_space_prefixed = false\nalias phosphor_test_alias = echo aliased\n",
            &[("WARP_HONOR_PS1", "1")],
            &steps,
        ) else {
            return;
        };

        let names = session.hook_names();
        assert_eq!(names[0], "InitShell", "{names:?}\n{}", session.raw);
        assert_eq!(session.hooks[0]["value"]["shell"], "nu");
        assert_eq!(session.hooks[0]["value"]["session_id"], 4242);

        // CommandFinished, Precmd, Bootstrapped come from the sourced file, in that order.
        assert_eq!(
            &names[1..4],
            ["CommandFinished", "Precmd", "Bootstrapped"],
            "{names:?}"
        );
        let bootstrapped = &session.hooks[3]["value"];
        assert_eq!(bootstrapped["shell"], "nu");
        assert_eq!(bootstrapped["session_id"], 4242);
        for key in [
            "histfile",
            "home_dir",
            "path",
            "aliases",
            "function_names",
            "env_var_names",
            "builtins",
            "shell_version",
            "os_category",
            "shell_path",
        ] {
            assert!(
                bootstrapped[key].as_str().is_some_and(|v| !v.is_empty()),
                "Bootstrapped.{key} is empty: {bootstrapped}"
            );
        }
        assert!(
            bootstrapped["aliases"]
                .as_str()
                .unwrap()
                .contains("phosphor_test_alias -> echo aliased"),
            "alias table: {}",
            bootstrapped["aliases"]
        );
        let builtins: Vec<&str> = bootstrapped["builtins"].as_str().unwrap().lines().collect();
        for builtin in ["ls", "where", "each", "open", "get", "select", "http"] {
            assert!(
                builtins.contains(&builtin),
                "{builtin} missing from builtins"
            );
        }
        assert_eq!(bootstrapped["shell_options"], "ignore_space_prefixed");

        // Each command: Preexec with the exact text, then CommandFinished + Precmd with its code.
        let preexec: Vec<&str> = session
            .hooks_named("Preexec")
            .iter()
            .map(|hook| hook["value"]["command"].as_str().unwrap())
            .collect();
        assert_eq!(
            preexec,
            [
                "echo hi",
                "^sh -c 'exit 7'",
                "echo \"a\\tb\"",
                "ls /definitely/not/here",
                "cd /tmp",
                "echo after",
                "if true {\n    echo multi\n}",
                "clear",
            ]
        );
        let exit_codes: Vec<i64> = session
            .hooks_named("CommandFinished")
            .iter()
            .map(|hook| hook["value"]["exit_code"].as_i64().unwrap())
            .collect();
        // the bootstrap (twice: its own report and the prompt after it), echo hi, exit 7, echo,
        // ls (an error is 1), cd, echo after, multi, clear
        assert_eq!(exit_codes, [0, 0, 0, 7, 0, 1, 0, 0, 0, 0], "{exit_codes:?}");
        let precmds = session.hooks_named("Precmd");
        assert_eq!(precmds.last().unwrap()["value"]["pwd"], "/tmp");
        assert_eq!(
            precmds.last().unwrap()["value"]["ignore_prompt_markers"],
            false
        );
        // Block ids chain: each CommandFinished names the block its Precmd starts.
        for (finished, precmd) in session
            .hooks_named("CommandFinished")
            .iter()
            .zip(session.hooks_named("Precmd"))
        {
            assert_eq!(
                finished["value"]["next_block_id"],
                precmd["value"]["next_block_id"]
            );
        }

        // The typeahead report carries the buffer and is not itself a command.
        let input = session.hooks_named("InputBuffer");
        assert_eq!(input.len(), 1, "{:?}", session.hook_names());
        assert_eq!(input[0]["value"]["buffer"], "half typed");
        assert_eq!(session.hooks_named("Clear").len(), 1);

        // History: the user's commands only. The `source` line and the key-bound helpers were
        // typed with a leading space, and the init script switched the option on even though
        // this user's config has it off.
        let history = session.history();
        for command in ["echo hi", "echo after", "cd /tmp"] {
            assert!(history.contains(command), "{history:?}");
        }
        assert!(
            !history.contains("source")
                && !history.contains("__warp")
                && !history.contains("bootstrap.nu"),
            "bootstrap text reached the history: {history:?}"
        );
        assert!(
            history.contains("if true {<\\n>    echo multi<\\n>}"),
            "{history:?}"
        );
        std::fs::remove_dir_all(&session.home).ok();
    });
}

/// With `WARP_HONOR_PS1=0` Precmd tells the app to ignore nushell's own OSC 133 markers, the
/// prompt is blank, and the user's prompt text travels hex-encoded for the preview.
#[test]
#[cfg(unix)]
fn test_nu_real_warp_prompt_mode_blanks_the_prompt_and_previews_the_users() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let init = app.read(|_| {
            init_shell_script_for_shell(ShellType::Nu, &crate::ASSETS, SessionId::from(9u64))
        });
        let home = std::env::temp_dir().join(format!("phosphor-nu-warp-{}", std::process::id()));
        let bootstrap = write_nu_bootstrap_file(&home);
        let steps = vec![
            format!(" source '{bootstrap}'\r"),
            "echo one\r".to_owned(),
            // Alt-p switches to the user's prompt, Alt-w back; neither is a command.
            "\x1bp".to_owned(),
            "echo two\r".to_owned(),
            "\x1bw".to_owned(),
            "echo three\r".to_owned(),
        ];
        let Some(session) = run_nu_session(
            "warp",
            &["--login".to_owned(), "--execute".to_owned(), init],
            "$env.PROMPT_COMMAND = {|| 'MYPROMPT' }\n$env.PROMPT_INDICATOR = '$$ '\n",
            &[("WARP_HONOR_PS1", "0")],
            &steps,
        ) else {
            return;
        };
        let precmds = session.hooks_named("Precmd");
        let ignore: Vec<bool> = precmds
            .iter()
            .map(|p| p["value"]["ignore_prompt_markers"].as_bool().unwrap())
            .collect();
        // The bootstrap's own report, the prompt after it, one, two (the user's prompt, switched
        // on by Alt-p before it ran), three (back to Warp's after Alt-w).
        assert_eq!(ignore, [true, true, true, false, true], "{ignore:?}");
        let preview = |p: &serde_json::Value| -> String {
            let hex = p["value"]["ps1"].as_str().unwrap();
            let bytes: Vec<u8> = (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
                .collect();
            String::from_utf8(bytes).unwrap()
        };
        assert_eq!(preview(precmds[2]), "MYPROMPT$$ ");
        assert_eq!(precmds[3]["value"]["ps1"], "");
        let preexec: Vec<&str> = session
            .hooks_named("Preexec")
            .iter()
            .map(|hook| hook["value"]["command"].as_str().unwrap())
            .collect();
        assert_eq!(preexec, ["echo one", "echo two", "echo three"]);
        std::fs::remove_dir_all(&session.home).ok();
    });
}

/// The in-band generator wrapper in a real nushell: the POSIX command runs under bash with stderr
/// merged, its output and status come back in the OSC 9277 envelope, the call is not recorded,
/// and the prompt after it is flagged as following an in-band command.
#[test]
#[cfg(unix)]
fn test_nu_real_generator_wrapper_round_trip() {
    use crate::terminal::model::session::command_executor::POSIX_GENERATOR_WRAPPER;

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let init = app.read(|_| {
            init_shell_script_for_shell(ShellType::Nu, &crate::ASSETS, SessionId::from(11u64))
        });
        let home = std::env::temp_dir().join(format!("phosphor-nu-gen-{}", std::process::id()));
        let bootstrap = write_nu_bootstrap_file(&home);
        // The same text the in-band executor builds, including a quote and a `'#` in the command.
        let command = "echo \"it's out\"; echo '#' err >&2; echo \"a'#b\"; exit 3";
        let call = format!(
            " {POSIX_GENERATOR_WRAPPER} 77 {}",
            crate::terminal::model::session::command_executor::nu_raw_string(command)
        );
        let steps = vec![format!(" source '{bootstrap}'\r"), format!("{call}\r")];
        let Some(session) = run_nu_session(
            "gen",
            &["--login".to_owned(), "--execute".to_owned(), init],
            "",
            &[("WARP_HONOR_PS1", "1")],
            &steps,
        ) else {
            return;
        };
        let output = session.hooks_named("GeneratorOutput");
        assert_eq!(output.len(), 1, "{:?}", session.hook_names());
        assert_eq!(
            output[0]["text"], "77;it's out\n# err\na'#b\n;3",
            "id;output;status"
        );
        let last_precmd = session.hooks_named("Precmd").pop().unwrap();
        assert_eq!(last_precmd["value"]["is_after_in_band_command"], true);
        assert!(
            !session.history().contains(POSIX_GENERATOR_WRAPPER),
            "{:?}",
            session.history()
        );
        std::fs::remove_dir_all(&session.home).ok();
    });
}

/// The subshell flow in a real nushell: the typed lines rebuild the stage-one script (with a
/// hostile env var in it), write it to the cache dir, source it and clean up. It announces the
/// subshell, the full bootstrap is sourced after InitShell, and with a config that records
/// space-led lines only the first, short, line reaches the history (#967).
#[test]
#[cfg(unix)]
fn test_nu_real_subshell_bootstrap_keeps_all_but_one_line_out_of_history() {
    use crate::env_vars::{EnvVar, EnvVarValue};

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let home = std::env::temp_dir().join(format!("phosphor-nu-sub-{}", std::process::id()));
        let bootstrap = write_nu_bootstrap_file(&home);
        let weird_value = "it's a '# \"quoted\" \\ value";
        let lines = app.read(|ctx| {
            init_subshell_command_lines(
                Some(ShellType::Nu),
                &[EnvVar {
                    name: "PHOSPHOR_WEIRD".to_owned(),
                    value: EnvVarValue::Constant(weird_value.to_owned()),
                    description: None,
                }],
                SessionId::from(4242u64),
                ctx,
            )
        });
        let out_file = home.join("weird.txt");
        let mut steps: Vec<String> = lines.iter().map(|line| format!("{line}\r")).collect();
        steps.push(format!(" source '{bootstrap}'\r"));
        steps.push(format!(
            "$env.PHOSPHOR_WEIRD | save -f '{}'\r",
            out_file.display()
        ));
        let Some(session) = run_nu_session(
            "sub",
            &[],
            "$env.config.show_banner = false\n$env.config.history.ignore_space_prefixed = false\n",
            &[("WARP_HONOR_PS1", "1")],
            &steps,
        ) else {
            return;
        };
        let names = session.hook_names();
        assert_eq!(names[0], "InitShell", "{names:?}\n{}", session.raw);
        let init = &session.hooks[0]["value"];
        assert_eq!(init["is_subshell"], true);
        assert_eq!(init["session_id"], 4242);
        assert_eq!(init["shell"], "nu");
        assert!(names.contains(&"Bootstrapped".to_owned()), "{names:?}");
        assert_eq!(
            std::fs::read_to_string(&out_file).unwrap_or_default(),
            weird_value,
            "the env var survived the raw-string chunking"
        );
        // The script arrived whole, so the "skipped" notice was never printed (the typed line
        // that holds its text is echoed, but the printed notice starts a new line).
        assert!(
            !session.raw.contains("\r\nPhosphor: subshell setup skipped"),
            "{}",
            session.raw
        );
        // User config records space-led lines; only the first prelude line may be in history,
        // and the user's setting is back afterwards.
        let history = session.history();
        assert!(
            history.contains("$env.__wbh = $env.config.history.ignore_space_prefixed"),
            "{history:?}"
        );
        assert!(
            !history.contains("_wbs") && !history.contains("_wbf") && !history.contains("source"),
            "bootstrap text reached the history: {history:?}"
        );
        std::fs::remove_dir_all(&session.home).ok();
    });
}

/// A script that arrives incomplete is not run: the file holds a notice instead, and the shell
/// carries on.
#[test]
#[cfg(unix)]
fn test_nu_real_subshell_bootstrap_with_a_lost_chunk_is_skipped_with_a_notice() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let lines = app.read(|ctx| {
            init_subshell_command_lines(Some(ShellType::Nu), &[], SessionId::from(5u64), ctx)
        });
        // Drop the accumulating line(s): the length check must fail.
        let chunk_lines: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with(" $env._wbs = "))
            .map(|(index, _)| index)
            .collect();
        let dropped = *chunk_lines.last().unwrap();
        let mut steps: Vec<String> = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != dropped || chunk_lines.len() == 1)
            .map(|(_, line)| format!("{line}\r"))
            .collect();
        steps.push("echo still-alive\r".to_owned());
        let Some(session) = run_nu_session("lost", &[], "", &[], &steps) else {
            return;
        };
        if chunk_lines.len() > 1 {
            assert!(
                session.raw.contains("\r\nPhosphor: subshell setup skipped"),
                "{}",
                session.raw
            );
            assert!(!session.hook_names().contains(&"InitShell".to_owned()));
        }
        assert!(session.raw.contains("still-alive"), "{}", session.raw);
        std::fs::remove_dir_all(&session.home).ok();
    });
}

/// #1047: PSReadLine (Windows PowerShell 5.1) crashed with `SetCursorPosition ... Actual value was
/// -2` on one Phosphorize in two. The ^K/^U line-clearing prefix is POSIX-line-editor only, and
/// the PowerShell lines need a real gap between them.
#[test]
fn powershell_bootstrap_is_psreadline_safe() {
    assert!(!subshell_bootstrap_clears_line_editor(Some(
        ShellType::PowerShell
    )));
    for shell in [
        None,
        Some(ShellType::Bash),
        Some(ShellType::Zsh),
        Some(ShellType::Fish),
        Some(ShellType::Nu),
    ] {
        assert!(subshell_bootstrap_clears_line_editor(shell), "{shell:?}");
        assert_eq!(
            subshell_bootstrap_line_delay(shell),
            std::time::Duration::from_millis(30),
            "{shell:?}"
        );
    }
    assert!(
        subshell_bootstrap_line_delay(Some(ShellType::PowerShell))
            >= std::time::Duration::from_millis(200)
    );
}

#[test]
fn powershell_bootstrap_lines_carry_no_control_characters() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let lines = init_subshell_command_lines(
                Some(ShellType::PowerShell),
                &[],
                SessionId::from(1u64),
                ctx,
            );
            assert!(lines.len() >= 2);
            for line in &lines {
                assert!(
                    !line.chars().any(char::is_control),
                    "a control character would reach PSReadLine as typed input: {line:?}"
                );
            }
        });
    });
}

fn bash_here_document_script() -> &'static [u8] {
    b"before\n builtin read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :\n body1\n body2\nEOM\n # comment\n stty sane\n eval \"$WARP_BOOTSTRAP_VAR\"; unset WARP_BOOTSTRAP_VAR\n"
}

#[test]
fn bash_counted_read_bootstrap_replaces_the_here_document_with_a_counted_read() {
    // #1172: the body moves out of the here-document; what is before and after it is untouched.
    let rewritten =
        bash_counted_read_bootstrap(bash_here_document_script()).expect("has the here-document");
    // #1549: the `stty sane` and the `eval` run from the command line; nothing follows the body.
    assert_eq!(
        decode_script(&rewritten),
        "before\n command -p stty -echo; WARP_BOOTSTRAP_VAR=$(LC_ALL=C command -p head -c 14); \
         command -p stty echo; stty sane; eval \"$WARP_BOOTSTRAP_VAR\"; unset WARP_BOOTSTRAP_VAR\r \
         body1\n body2\n"
    );
}

#[test]
fn bash_counted_read_bootstrap_counts_bytes_not_characters() {
    // `head -c` counts bytes; a multi-byte character in the body must be counted as its bytes.
    let script = "x\n builtin read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :\n \u{2328}\nEOM\n stty sane\n eval \"$WARP_BOOTSTRAP_VAR\"; unset WARP_BOOTSTRAP_VAR\n"
        .as_bytes()
        .to_vec();
    let rewritten = bash_counted_read_bootstrap(&script).expect("has the here-document");
    // " \u{2328}\n" is 1 + 3 + 1 bytes.
    assert!(decode_script(&rewritten).contains("head -c 5)"));
}

#[test]
fn bash_counted_read_bootstrap_leaves_other_scripts_alone() {
    assert_eq!(bash_counted_read_bootstrap(b"echo hi\n"), None);
    // An unterminated here-document is not rewritten either.
    assert_eq!(
        bash_counted_read_bootstrap(
            b" builtin read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :\n body\n"
        ),
        None
    );
}

#[test]
fn bash_counted_read_bootstrap_applies_to_the_bundled_bash_script() {
    // Guards the markers against edits to bash.sh: the real script (its `#include` line replaced
    // by the body, as `script_for_shell` does before trimming) must be rewritable, and the byte
    // count in the new command must be exactly the length of the body that follows it.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bundled/bootstrap/");
    let top = std::fs::read_to_string(format!("{dir}bash.sh")).expect("bash.sh");
    let body = std::fs::read_to_string(format!("{dir}bash_body.sh")).expect("bash_body.sh");
    let script = top.replace("#include bundled/bootstrap/bash_body.sh", body.trim_end());
    let rewritten = bash_counted_read_bootstrap(script.as_bytes())
        .expect("bash.sh must keep the here-document that bash_counted_read_bootstrap rewrites");
    let text = decode_script(&rewritten);
    let after_head = text.split("head -c ").nth(1).expect("a head -c command");
    let count: usize = after_head
        .split(')')
        .next()
        .and_then(|n| n.parse().ok())
        .expect("a byte count");
    let command_end =
        text.find("unset WARP_BOOTSTRAP_VAR\r").unwrap() + "unset WARP_BOOTSTRAP_VAR\r".len();
    // #1549: the body is the end of the write; the `stty sane` and `eval` that followed the
    // terminator line in the original now run from the command line.
    assert_eq!(text.len() - command_end, count);
    assert!(text.contains("command -p stty echo; stty sane; eval \"$WARP_BOOTSTRAP_VAR\""));
    assert!(!text.contains("<< 'EOM'"));
}

#[test]
fn strip_local_only_block_drops_exactly_the_marked_lines() {
    let script: Cow<'static, [u8]> =
        Cow::Borrowed(b" a\n : __warp_local_only_begin\n ssh()\n : __warp_local_only_end\n b\n");
    assert_eq!(&*strip_local_only_block(script), &b" a\n b\n"[..]);
    // A missing, doubled or reversed marker leaves the script alone.
    for bad in [
        &b" a\n : __warp_local_only_begin\n b\n"[..],
        b" a\n : __warp_local_only_end\n b\n",
        b" : __warp_local_only_end\n x\n : __warp_local_only_begin\n",
        b" : __warp_local_only_begin\n : __warp_local_only_begin\n : __warp_local_only_end\n",
        b" plain\n",
    ] {
        let input: Cow<'static, [u8]> = Cow::Owned(bad.to_vec());
        assert_eq!(&*strip_local_only_block(input), bad);
    }
}

#[test]
fn bundled_bash_body_has_one_local_only_block_holding_the_ssh_wrapper() {
    // #1164: the markers must stay in bash_body.sh, the stripped body must not define the ssh
    // wrapper, and the unstripped one must.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bundled/bootstrap/");
    let body = std::fs::read_to_string(format!("{dir}bash_body.sh")).expect("bash_body.sh");
    let stripped = strip_local_only_block(Cow::Owned(body.clone().into_bytes()));
    let stripped = String::from_utf8(stripped.into_owned()).unwrap();
    assert!(body.contains("function warp_ssh_helper()"));
    assert!(!stripped.contains("warp_ssh_helper"));
    assert!(!stripped.contains("__warp_ssh_impl"));
    assert!(stripped.len() < body.len());
    assert!(stripped.contains("function warp_bootstrapped"));
}

#[test]
fn counted_read_rewrite_ends_exactly_with_the_body_for_every_shell() {
    // #1549: macOS `head` over-reads a tty and swallows whatever is queued after the counted
    // bytes, so nothing may follow the body.
    let bash_body = b" body1\n body2\n";
    let zsh_body = b" body1\n body2\n";
    for (script, shell, body) in [
        (bash_here_document_script(), ShellType::Bash, &bash_body[..]),
        (zsh_here_document_script(), ShellType::Zsh, &zsh_body[..]),
    ] {
        let (rewritten, prefix_len) = counted_read_rewrite(script, shell).expect("rewritable");
        assert!(rewritten.ends_with(body), "{shell:?}: bytes after the body");
        assert_eq!(&rewritten[prefix_len..], body, "{shell:?}");
    }
    // Fish and the other shells are not rewritten at all.
    for shell in [ShellType::Fish, ShellType::Nu, ShellType::PowerShell] {
        assert_eq!(
            counted_read_rewrite(bash_here_document_script(), shell),
            None
        );
    }
    // A script with an unknown line after the here-document would put it after the body (or lose
    // it): not rewritten.
    assert_eq!(
        counted_read_rewrite(
            b" builtin read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :\n body\nEOM\n stty sane\n eval \"$WARP_BOOTSTRAP_VAR\"; unset WARP_BOOTSTRAP_VAR\n echo extra\n",
            ShellType::Bash
        ),
        None
    );
}

#[test]
fn bash_counted_read_runs_the_eval_from_the_head_line() {
    let (rewritten, prefix_len) =
        counted_read_rewrite(bash_here_document_script(), ShellType::Bash).expect("rewritable");
    let command = decode_script(&rewritten[..prefix_len]);
    let command = command.rsplit('\n').next().unwrap();
    assert_eq!(
        command,
        " command -p stty -echo; WARP_BOOTSTRAP_VAR=$(LC_ALL=C command -p head -c 14); \
         command -p stty echo; stty sane; eval \"$WARP_BOOTSTRAP_VAR\"; unset WARP_BOOTSTRAP_VAR\r"
    );
}

#[test]
fn counted_read_count_is_the_body_byte_length_including_multibyte() {
    for shell in [ShellType::Bash, ShellType::Zsh] {
        let body = " \u{2328} h\u{e9}llo \u{1f600}\n";
        let script = match shell {
            ShellType::Bash => format!(
                "x\n builtin read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :\n{body}EOM\n stty sane\n eval \"$WARP_BOOTSTRAP_VAR\"; unset WARP_BOOTSTRAP_VAR\n"
            ),
            _ => format!(
                "x\n read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :; eval \"$WARP_BOOTSTRAP_VAR\"\n{body}EOM\n"
            ),
        };
        let (rewritten, prefix_len) =
            counted_read_rewrite(script.as_bytes(), shell).expect("rewritable");
        let command = decode_script(&rewritten[..prefix_len]);
        let count: usize = command
            .split("head -c ")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .and_then(|n| n.parse().ok())
            .expect("a byte count");
        assert_eq!(count, body.len(), "{shell:?}");
        assert_eq!(&rewritten[prefix_len..], body.as_bytes(), "{shell:?}");
    }
}

#[test]
fn counted_read_bootstrap_is_on_unless_switched_off() {
    assert!(counted_read_bootstrap_enabled_from_env(None));
    assert!(counted_read_bootstrap_enabled_from_env(Some("1")));
    assert!(!counted_read_bootstrap_enabled_from_env(Some("0")));
    assert!(!counted_read_bootstrap_enabled_from_env(Some(" off ")));
}

#[test]
fn bootstrap_for_session_rewrites_only_bash_over_ssh_that_asked_for_it() {
    let script: Cow<'static, [u8]> = Cow::Borrowed(
        b" builtin read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :\n body\nEOM\n stty sane\n eval \"$WARP_BOOTSTRAP_VAR\"; unset WARP_BOOTSTRAP_VAR\n",
    );
    let ssh_raw = SessionInfo::new_for_test()
        .with_ssh_socket_path(std::path::PathBuf::from("/tmp/sock"))
        .with_raw_bootstrap(true);
    let rewritten = bootstrap_for_session(script.clone(), ShellType::Bash, &ssh_raw);
    assert!(decode_script(&rewritten).contains("head -c 6)"));

    // The remote did not report support: unchanged.
    let ssh_plain = SessionInfo::new_for_test()
        .with_ssh_socket_path(std::path::PathBuf::from("/tmp/sock"))
        .with_raw_bootstrap(false);
    assert_eq!(
        bootstrap_for_session(script.clone(), ShellType::Bash, &ssh_plain),
        script
    );
    // Another shell: unchanged.
    assert_eq!(
        bootstrap_for_session(script.clone(), ShellType::Zsh, &ssh_raw),
        script
    );
    // Not an ssh session: unchanged.
    assert_eq!(
        bootstrap_for_session(
            script.clone(),
            ShellType::Bash,
            &SessionInfo::new_for_test()
        ),
        script
    );
}

// ---- #1226: counted read for local bash/zsh tabs and sudo/su subshells ----

fn zsh_here_document_script() -> &'static [u8] {
    b" setopt x\n read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :; eval \"$WARP_BOOTSTRAP_VAR\" || :; unset WARP_BOOTSTRAP_VAR\n body1\n body2\nEOM\n"
}

#[test]
fn zsh_counted_read_keeps_the_eval_on_the_command_line_after_the_read() {
    // The zsh script runs the body from the same line as the `read`, so the rewritten command
    // line reads the body first and then runs what followed the `read`.
    let (rewritten, prefix_len) = counted_read_rewrite(zsh_here_document_script(), ShellType::Zsh)
        .expect("zsh here-document");
    assert_eq!(
        decode_script(&rewritten),
        " setopt x\n command -p stty -echo; WARP_BOOTSTRAP_VAR=$(LC_ALL=C command -p head -c 14); \
         command -p stty echo; eval \"$WARP_BOOTSTRAP_VAR\" || :; unset WARP_BOOTSTRAP_VAR\r body1\n body2\n"
    );
    // The prefix ends with the CR of the command line; the body follows it.
    assert_eq!(rewritten[prefix_len - 1], b'\r');
    assert_eq!(&rewritten[prefix_len..], b" body1\n body2\n");
}

#[test]
fn counted_read_rewrite_only_handles_bash_and_zsh_scripts_it_recognises() {
    assert_eq!(
        counted_read_rewrite(zsh_here_document_script(), ShellType::Fish),
        None
    );
    // The zsh marker is not the bash marker and the other way round.
    assert_eq!(
        counted_read_rewrite(zsh_here_document_script(), ShellType::Bash),
        None
    );
    assert_eq!(counted_read_rewrite(b"echo hi\n", ShellType::Zsh), None);
    // The `read` line without a newline after it, or an unterminated here-document: unchanged.
    assert_eq!(
        counted_read_rewrite(
            b" read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :; eval x",
            ShellType::Zsh
        ),
        None
    );
    assert_eq!(
        counted_read_rewrite(
            b" read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :; eval x\n body\n",
            ShellType::Zsh
        ),
        None
    );
}

#[test]
fn zsh_counted_read_applies_to_the_bundled_zsh_script() {
    // Guards the markers against edits to zsh.sh, and the byte count against the body that follows.
    let script = script_for_shell(ShellType::Zsh, &crate::ASSETS);
    let (rewritten, prefix_len) = counted_read_rewrite(&script, ShellType::Zsh)
        .expect("zsh.sh must keep the here-document that counted_read_rewrite rewrites");
    let text = decode_script(&rewritten);
    assert!(!text.contains("<< 'EOM'"));
    let count: usize = text
        .split("head -c ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|n| n.parse().ok())
        .expect("a byte count");
    // The zsh script ends with the body: the here-document is its last construct.
    assert_eq!(rewritten.len() - prefix_len, count);
    // Everything the original line ran after the `read` still runs, after the body is read.
    let command = decode_script(&rewritten[..prefix_len]);
    let command = command.rsplit('\n').next().unwrap();
    assert!(command.contains(
        "command -p stty echo; eval \"$WARP_BOOTSTRAP_VAR\" || :; unset WARP_BOOTSTRAP_VAR;"
    ));
    assert!(command.ends_with(
        "unset WARP_RESTORE_PRINT_EXIT_VALUE WARP_RESTORE_PIPE_FAIL WARP_RESTORE_NOUNSET\r"
    ));
}

#[test]
fn bundled_bootstrap_bodies_are_safe_to_read_through_a_cooked_tty() {
    // While `head` reads the body the tty is cooked: a control character would act on the tty
    // (^C, ^Z, ^D, ^\ ...), and a line over the canonical limit is cut (#962; the subshell limit
    // is the smaller 1024 of macOS). Tab and newline are fine.
    for shell_type in [ShellType::Bash, ShellType::Zsh] {
        let script = script_for_shell(shell_type, &crate::ASSETS);
        // 1024 is the canonical line limit on macOS (see MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES).
        assert!(
            script.split(|b| *b == b'\n').all(|line| line.len() < 1024),
            "{shell_type:?}: a line of 1024 bytes or more"
        );
        assert!(
            !script
                .iter()
                .any(|b| (*b < 0x20 && *b != b'\n' && *b != b'\t') || *b == 0x7f),
            "{shell_type:?} carries a control character"
        );
    }
}

#[test]
fn local_counted_bootstrap_is_on_unless_switched_off() {
    assert!(local_counted_bootstrap_enabled_from_env(None));
    assert!(local_counted_bootstrap_enabled_from_env(Some("1")));
    assert!(!local_counted_bootstrap_enabled_from_env(Some("0")));
    assert!(!local_counted_bootstrap_enabled_from_env(Some(" off ")));
}

fn subshell_of(command: &str) -> SessionInfo {
    let mut info = SessionInfo::new_for_test();
    info.subshell_info = Some(
        crate::terminal::model::terminal_model::SubshellInitializationInfo {
            spawning_command: command.to_owned(),
            was_triggered_by_rc_file_snippet: false,
            env_var_collection_name: None,
            ssh_connection_info: None,
        },
    );
    info
}

#[cfg(unix)]
#[test]
fn local_counted_bootstrap_applies_to_local_bash_zsh_and_host_subshells_only() {
    let local = SessionInfo::new_for_test();
    assert!(local_counted_bootstrap_applies(ShellType::Bash, &local));
    assert!(local_counted_bootstrap_applies(ShellType::Zsh, &local));
    // Other shells keep their own way in (fish and pwsh source a file).
    assert!(!local_counted_bootstrap_applies(ShellType::Fish, &local));
    assert!(!local_counted_bootstrap_applies(
        ShellType::PowerShell,
        &local
    ));

    // The host's own privilege-changing subshells; env prefixes and paths are looked through.
    for command in [
        "sudo -s",
        "su -",
        "ksu",
        "doas -s",
        "FOO=1 sudo -i",
        "/usr/bin/sudo -s",
    ] {
        assert!(
            local_counted_bootstrap_applies(ShellType::Bash, &subshell_of(command)),
            "{command}"
        );
    }
    // Subshells whose filesystem or tty may not be this host's keep the typed form.
    for command in [
        "docker exec -it c bash",
        "kubectl exec -it pod -- bash",
        "nix-shell",
        "ssh host",
        "poetry shell",
        "",
    ] {
        assert!(
            !local_counted_bootstrap_applies(ShellType::Bash, &subshell_of(command)),
            "{command}"
        );
    }
    // A sudo subshell that is really an ssh one.
    let mut ssh_sudo = subshell_of("sudo -s");
    ssh_sudo.subshell_info.as_mut().unwrap().ssh_connection_info = Some(Default::default());
    assert!(!local_counted_bootstrap_applies(ShellType::Bash, &ssh_sudo));

    // An ssh session is the business of `bootstrap_for_session`.
    let ssh = SessionInfo::new_for_test().with_ssh_socket_path(std::path::PathBuf::from("/tmp/s"));
    assert!(!local_counted_bootstrap_applies(ShellType::Bash, &ssh));
}

#[test]
fn counted_subshell_chunks_pace_the_prefix_and_send_the_body_as_one_write() {
    let mut script = vec![b'a'; SUBSHELL_BOOTSTRAP_CHUNK_SIZE * 2 + 100];
    let prefix_len = script.len();
    script.resize(prefix_len + SUBSHELL_BOOTSTRAP_CHUNK_SIZE * 5, b'b');
    let chunks = counted_subshell_bootstrap_chunks(&script, prefix_len);
    // 3 paced prefix chunks, then the whole body.
    assert_eq!(chunks.len(), 4);
    assert!(
        chunks[..3]
            .iter()
            .all(|c| c.len() <= SUBSHELL_BOOTSTRAP_CHUNK_SIZE && c.iter().all(|b| *b == b'a'))
    );
    assert_eq!(chunks[3].len(), SUBSHELL_BOOTSTRAP_CHUNK_SIZE * 5);
    assert_eq!(chunks.concat(), script);
    // Nothing after the prefix: no empty body write.
    assert_eq!(
        counted_subshell_bootstrap_chunks(b"abc", 3),
        vec![b"abc".to_vec()]
    );
    assert_eq!(
        counted_subshell_bootstrap_chunks(b"abc", 99),
        vec![b"abc".to_vec()]
    );
}

/// #1354: the setup lines of bash, zsh and fish end in a marker the shell prints once it ran the
/// line, so the app can write the next line when the previous one was consumed instead of after
/// a delay. The last line (it evaluates the script) carries none, and a plain build of the lines
/// is unchanged.
#[test]
fn marked_setup_lines_end_in_a_marker_except_the_last() {
    let raw = "echo 'x' ".repeat(500);
    for shell_type in [ShellType::Bash, ShellType::Zsh, ShellType::Fish] {
        let plain = chunked_eval_lines(shell_type, &raw);
        let marked = chunked_eval_lines_with_markers(shell_type, &raw, Some(77));
        assert_eq!(plain.len(), marked.len(), "{shell_type:?}");
        let last = marked.len() - 1;
        for (index, (plain_line, marked_line)) in plain.iter().zip(&marked).enumerate() {
            if index == last {
                assert_eq!(
                    plain_line, marked_line,
                    "{shell_type:?}: last line is unmarked"
                );
            } else {
                assert!(
                    marked_line.starts_with(plain_line.as_str()),
                    "{shell_type:?} {index}"
                );
                assert!(
                    marked_line.ends_with(&format!("\\e]7771;77;{index}\\a'")),
                    "{shell_type:?} {index}: {marked_line}"
                );
            }
        }
        assert!(
            marked
                .iter()
                .all(|l| l.len() < MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES),
            "{shell_type:?}: the marker must not push a line past the tty line limit"
        );
        assert_eq!(
            chunked_eval_lines(shell_type, &raw),
            chunked_eval_lines_with_markers(shell_type, &raw, None)
        );
    }
}

/// #1354: typed into a real bash, zsh and fish, the marked lines print every marker, in order,
/// and the script still runs.
#[test]
#[cfg(unix)]
fn marked_setup_lines_print_their_markers_in_a_real_shell() {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let cases: [(&str, &[&str], ShellType); 3] = [
        ("bash", &["--norc", "-i"], ShellType::Bash),
        ("zsh", &["-f", "-i"], ShellType::Zsh),
        ("fish", &["--no-config", "-i"], ShellType::Fish),
    ];
    for (bin, args, shell_type) in cases {
        let Ok(path) = Command::new("sh")
            .args(["-c", &format!("command -v {bin}")])
            .output()
        else {
            continue;
        };
        let path = String::from_utf8_lossy(&path.stdout).trim().to_owned();
        if path.is_empty() {
            eprintln!("skipping {bin}: not installed");
            continue;
        }
        let lines = chunked_eval_lines_with_markers(shell_type, "echo STAGE2-RAN", Some(4242));
        let dir =
            std::env::temp_dir().join(format!("phosphor-marker-{bin}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("data/fish")).unwrap();
        let mut child = Command::new(&path)
            .args(args)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", &dir)
            .env("XDG_DATA_HOME", dir.join("data"))
            .env("XDG_CONFIG_HOME", dir.join("cfg"))
            .env("TERM", "dumb")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            let stdin = child.stdin.as_mut().unwrap();
            for line in &lines {
                stdin.write_all(line.as_bytes()).unwrap();
                stdin.write_all(b"\n").unwrap();
            }
            stdin.write_all(b"exit\n").unwrap();
        }
        let out = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut from = 0;
        for index in 0..lines.len() - 1 {
            let marker = format!("\u{1b}]7771;4242;{index}\u{7}");
            let found = stdout[from..].find(&marker).unwrap_or_else(|| {
                panic!("{bin}: marker {index} missing or out of order: {stdout:?}")
            });
            from += found + marker.len();
        }
        assert!(
            stdout.contains("STAGE2-RAN"),
            "{bin}: script did not run: {stdout:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// #1372: the nushell setup lines end in a marker (the same OSC 7771 as bash/zsh/fish, written in
/// nushell syntax so the typed text holds no escape byte), except the last; each line is still
/// one `;`-joined statement sequence under the tty line limit; an unmarked build is unchanged.
#[test]
fn marked_nu_setup_lines_end_in_a_nu_marker_except_the_last() {
    let raw = "print 'x'\n".repeat(300);
    let session = SessionId::from(77u64);
    let plain = nu_subshell_lines_with_markers(&raw, session, false);
    let marked = nu_subshell_lines_with_markers(&raw, session, true);
    assert_eq!(plain.len(), marked.len());
    assert!(marked.len() >= 4);
    let last = marked.len() - 1;
    for (index, (plain_line, marked_line)) in plain.iter().zip(&marked).enumerate() {
        if index == last {
            assert_eq!(plain_line, marked_line, "last line is unmarked");
        } else {
            assert!(marked_line.starts_with(plain_line.as_str()), "{index}");
            assert!(
                marked_line.ends_with(&format!(
                    "; print -n $\"(ansi esc)]7771;77;{index}(char bel)\""
                )),
                "{index}: {marked_line}"
            );
        }
        assert!(
            !marked_line.contains('\u{1b}'),
            "{index}: no escape byte typed"
        );
        assert!(
            marked_line.starts_with(' '),
            "{index}: stays out of history"
        );
    }
    assert!(
        marked
            .iter()
            .all(|l| l.len() < MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES)
    );
}

/// #1399: nushell 0.116 has no `char esc`; a marker built with it fails the whole line at parse
/// time, so no marker ever arrived. The marker uses `ansi esc` and no other unknown named char.
#[test]
fn nu_setup_marker_uses_only_names_nushell_0_116_has() {
    let marker = nu_setup_line_marker(9, 3);
    assert_eq!(marker, " print -n $\"(ansi esc)]7771;9;3(char bel)\"");
    assert!(!marker.contains("char esc"), "{marker}");
}

/// #1372: typed into a real nushell, the marked lines print every marker in order and the script
/// still runs. Skipped when nu (or python3, for the pty driver) is missing.
#[test]
#[cfg(unix)]
fn marked_nu_setup_lines_print_their_markers_in_a_real_shell() {
    let lines =
        nu_subshell_lines_with_markers("print 'STAGE2-RAN'\n", SessionId::from(4242u64), true);
    let steps: Vec<String> = lines.iter().map(|line| format!("{line}\r")).collect();
    let Some(session) = run_nu_session(
        "marker",
        &[],
        "$env.config.show_banner = false\n",
        &[],
        &steps,
    ) else {
        return;
    };
    let mut from = 0;
    for index in 0..lines.len() - 1 {
        let marker = format!("\u{1b}]7771;4242;{index}\u{7}");
        let found = session.raw[from..]
            .find(&marker)
            .unwrap_or_else(|| panic!("marker {index} missing or out of order: {:?}", session.raw));
        from += found + marker.len();
    }
    assert!(session.raw.contains("STAGE2-RAN"), "{:?}", session.raw);
}

/// #1372: the PowerShell setup lines carry the marker mechanics -- a `PSConsoleHostReadLine`
/// wrapper installed by the first line that prints the pending index when PSReadLine is about to
/// read, a pending index set by each of the first two lines -- and the last line, an unmarked
/// build and the history handler's trailing comment word are untouched.
#[test]
fn marked_powershell_setup_lines_set_a_pending_marker_and_install_the_read_hook() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let session = SessionId::from(18_446_744_073_709_551_000u64);
            let plain = init_subshell_command_lines(Some(ShellType::PowerShell), &[], session, ctx);
            let marked = init_subshell_command_lines_with_markers(
                Some(ShellType::PowerShell),
                &[],
                session,
                true,
                ctx,
            );
            assert_eq!(plain.len(), 3);
            assert_eq!(marked.len(), 3);
            assert_eq!(plain[2], marked[2], "the last line is unmarked");
            assert!(!plain[0].contains("7771") && !plain[1].contains("7771"));
            // Line 0 installs the hook that prints the marker, outside the history guard, before
            // the comment word; line 1 records that it ran.
            assert!(marked[0].starts_with(plain[0].trim_end_matches(" # apikey")));
            assert!(marked[0].trim_end().ends_with("# apikey"), "{}", marked[0]);
            assert!(marked[0].contains("PSConsoleHostReadLine"));
            assert!(marked[0].contains(&format!("]7771;{};", session.as_u64())));
            assert!(marked[0].contains("$global:_phMk = 0"));
            assert!(marked[1].starts_with(plain[1].as_str()));
            assert!(marked[1].ends_with("$global:_phMk = 1"), "{}", marked[1]);
            for line in &marked {
                assert!(line.starts_with(' ') || line.starts_with("if "), "{line}");
                assert!(!line.contains('\u{1b}'), "no escape byte is typed");
                assert!(
                    line.len() < MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES,
                    "{}",
                    line.len()
                );
            }
        });
    });
}

/// #1372: every PowerShell setup line, plain and marked, parses as PowerShell. The history
/// handler line shipped with an unescaped `'` inside a single-quoted string and never parsed, so
/// Windows PowerShell sat at its `>>` continuation prompt. Skipped without pwsh.
#[test]
#[cfg(unix)]
fn powershell_setup_lines_parse_as_powershell() {
    use std::process::Command;
    if Command::new("pwsh")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("1")
        .output()
        .is_err()
    {
        return;
    }
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            let dir = std::env::temp_dir().join(format!("phosphor-pwsh-parse-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            for marked in [false, true] {
                let lines = init_subshell_command_lines_with_markers(
                    Some(ShellType::PowerShell),
                    &[],
                    SessionId::from(42u64),
                    marked,
                    ctx,
                );
                for (index, line) in lines.iter().enumerate() {
                    let file = dir.join(format!("line{marked}{index}.ps1"));
                    std::fs::write(&file, line).unwrap();
                    let out = Command::new("pwsh")
                        .arg("-NoProfile")
                        .arg("-Command")
                        .arg("$e = $null; $t = $null; [void][System.Management.Automation.Language.Parser]::ParseFile($env:PHOSPHOR_PARSE_FILE, [ref]$t, [ref]$e); $e | ForEach-Object { $_.Message }")
                        .env("PHOSPHOR_PARSE_FILE", &file)
                        .output()
                        .unwrap();
                    let errors = String::from_utf8_lossy(&out.stdout);
                    assert!(errors.trim().is_empty(), "marked={marked} line {index}: {errors}");
                }
            }
            std::fs::remove_dir_all(&dir).ok();
        });
    });
}

/// #1172: the zsh wrapper no longer forks `printf | od | tr` to hex-encode the remote env script;
/// the constant head and tail are hex literals in the body. Decode them and check they are still
/// the script they replaced, so editing the script without re-encoding fails here.
#[test]
fn test_zsh_env_script_hex_constants_decode_to_the_script() {
    // A constant may be split over lines (`local name='..'` then `name+='..'`) to keep every typed
    // line under macOS MAX_CANON; the pieces are joined.
    fn constant(zsh: &str, name: &str) -> String {
        fn piece<'a>(zsh: &'a str, marker: &str) -> Option<&'a str> {
            let start = zsh.find(marker)? + marker.len();
            let len = zsh[start..].find('\'').expect("unterminated constant");
            Some(&zsh[start..start + len])
        }
        let mut hex = piece(zsh, &format!("local {name}='"))
            .unwrap_or_else(|| panic!("zsh_body.sh lost `local {name}='`"))
            .to_owned();
        let more = format!("{name}+='");
        for line in zsh.lines() {
            if let Some(rest) = line.trim_start().strip_prefix(&more) {
                hex.push_str(rest.split('\'').next().expect("unterminated constant"));
            }
        }
        hex
    }
    fn decode(hex: &str) -> String {
        assert_eq!(hex.len() % 2, 0, "odd hex length");
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("not hex"))
            .collect();
        String::from_utf8(bytes).expect("not utf-8")
    }

    let zsh = bundled_script("bundled/bootstrap/zsh_body.sh");
    let head = decode(&constant(&zsh, "zsh_env_head"));
    let tail = decode(&constant(&zsh, "zsh_env_tail"));
    assert_eq!(head, "unsetopt ZLE RCS GLOBAL_RCS; WARP_SESSION_ID=");
    assert!(
        tail.starts_with("_hostname=$(command -pv hostname"),
        "{tail}"
    );
    assert!(
        tail.ends_with("unset _hostname _user _msg _ju _jh"),
        "{tail}"
    );
    // No substituted placeholder may hide in a constant: those belong in the builtin-encoded middle.
    assert!(!tail.contains("@@"), "{tail}");
}
