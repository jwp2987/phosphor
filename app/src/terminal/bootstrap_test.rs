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
/// `${...}`, must keep the bare form.
#[test]
fn test_init_subshell_command_guard_is_nounset_safe_for_bash_and_zsh_only() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        app.read(|ctx| {
            for shell_type in [ShellType::Bash, ShellType::Zsh] {
                let command =
                    init_subshell_command(Some(shell_type), &[], SessionId::from(1u64), ctx);
                assert!(
                    command.starts_with(r#" [ -z "${WARP_BOOTSTRAPPED-}" ] && eval '"#),
                    "expected a nounset-safe guard for {shell_type:?}: {command}"
                );
            }

            let command =
                init_subshell_command(Some(ShellType::Fish), &[], SessionId::from(1u64), ctx);
            assert!(
                command.starts_with(" [ -z $WARP_BOOTSTRAPPED ] && eval '"),
                "fish cannot parse `${{...}}`, so its guard must stay bare: {command}"
            );
            assert!(
                !command.contains("${"),
                "fish cannot parse `${{...}}`: {command}"
            );
        });
    });
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
