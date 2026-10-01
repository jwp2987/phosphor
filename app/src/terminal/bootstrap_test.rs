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
                !command.contains("[ -z $WARP_BOOTSTRAPPED ]"),
                "the POSIX guard is not valid PowerShell syntax: {command}"
            );
            assert!(
                !command.contains("eval '"),
                "PowerShell doesn't need the eval-wrapping bash/zsh/fish use: {command}"
            );
        });
    });
}
