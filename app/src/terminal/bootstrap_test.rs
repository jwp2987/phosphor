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
                    last.contains(r#" [ -z "${WARP_BOOTSTRAPPED-}" ] && "#)
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

/// #935: a pasted multi-line command must title the tab with a visible separator instead of
/// running the lines together ("gradle onegradle two"). Runs the real `warp_title` of the bash
/// bootstrap with a stub `__warp_emit`; skipped, visibly, when bash is missing.
#[cfg(unix)]
#[test]
fn bash_warp_title_joins_multiline_commands_with_a_separator() {
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
        ("gradle one\ngradle two", "gradle one ; gradle two"),
        ("a\r\nb", "a ; b"),
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

/// #934: a native command that inherits the terminal's stdin makes .NET reconfigure the tty
/// around the child, which drops the Enter the user typed ahead while the prompt ran (two lines
/// then run as one). Every native command the PowerShell bootstrap runs must get a redirected
/// stdin: pipe an empty string into it. This scans the bootstrap for call-operator invocations
/// and fails on any that is neither piped nor on the list of PowerShell-only callees below.
#[test]
fn powershell_bootstrap_native_calls_redirect_stdin() {
    // Callees that are PowerShell script blocks, not native processes: the user's own prompt
    // and the PSConsoleHostReadLine wrapper. A user's prompt that runs native commands is theirs.
    const NOT_NATIVE: [&str; 2] = [
        "& $global:_warpOriginalPrompt",
        "& $script:oldPSConsoleHostReadLine",
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
        });
    });
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
