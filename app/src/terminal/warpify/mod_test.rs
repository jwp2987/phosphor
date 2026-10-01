//! Guards the bundled auto-bootstrap snippet text against Warp branding
//! leaking back in. These `.txt` files are spliced verbatim into the "Run the
//! following to automatically Phosphorize in the future:" snippet
//! (`success_block.rs`) and, for the shell variants, written into the user's
//! own rc file (`~/.config/fish/config.fish`, `~/.bashrc`, `~/.zshrc`) as a
//! `#`-comment the user will read the next time they open it. `# Auto-Warpify`
//! shipped there even though `check_brand_strings` treats `Warp` inside an
//! identifier like `SourcedRcFileForWarp` as lineage, not branding: the guard
//! only scans Rust string literals and Fluent values, not bundled assets, so
//! a plain-text asset file was a blind spot. These tests close that
//! particular gap without teaching the shell script to parse Rust.
//!
//! `SourcedRcFileForWarp` itself is intentionally NOT asserted away here: it
//! is the wire value of the DCS hook this fork's own ANSI parser matches on
//! (`DProtoHook::SourcedRcFileForWarp`, `terminal/model/ansi/dcs_hooks.rs`),
//! so renaming the string in the asset without renaming the enum variant (and
//! every terminal on the other end of the pipe) would just break detection.

use super::subshell_bootstrap_success_block_bytes;
use crate::terminal::model::terminal_model::SubshellInitializationInfo;
use crate::terminal::shell::ShellType;
use channel_versions::overrides::TargetOS;

const FISH_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/fish_subshell_bootstrap_block_output.txt"
));
const BASH_ZSH_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/bash_zsh_subshell_bootstrap_block_output.txt"
));
const LEGACY_REMOTE_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/legacy_remote_subshell_bootstrap_block_output.txt"
));
const PWSH_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/pwsh_subshell_bootstrap_block_output.txt"
));

/// Not scanned by `assert_no_stray_warp_branding` below: this script is riddled with
/// internal `Warp-*` function/variable names (`Warp-Send-JsonMessage`, `$global:_warpOriginalPrompt`,
/// ...) that are lineage, not branding, exactly like `check_brand_strings`' own carve-out for
/// identifiers such as `SourcedRcFileForWarp`. Only the one genuinely user-visible string in
/// it -- the `Write-Error` shown directly in the user's PowerShell session when the OS
/// execution policy blocks Phosphorization -- is asserted here.
const PWSH_INIT_SHELL_SNIPPET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/bundled/bootstrap/pwsh_init_shell.ps1"
));

/// The one identifier these snippets are allowed to carry: the DCS hook name
/// itself, which is a wire value, not prose.
const ALLOWED_WARP_IDENTIFIER: &str = "SourcedRcFileForWarp";

fn assert_no_stray_warp_branding(snippet: &str, label: &str) {
    let scrubbed = snippet.replace(ALLOWED_WARP_IDENTIFIER, "");
    assert!(
        !scrubbed.contains("Warp") && !scrubbed.contains("warp"),
        "{label} still names Warp outside of the {ALLOWED_WARP_IDENTIFIER} wire value: {snippet:?}"
    );
}

#[test]
fn fish_bootstrap_snippet_comment_says_phosphorize() {
    assert!(
        FISH_SNIPPET.contains("# Auto-Phosphorize"),
        "expected the fish rc-file comment to say Phosphorize: {FISH_SNIPPET:?}"
    );
    assert_no_stray_warp_branding(FISH_SNIPPET, "fish bootstrap snippet");
}

#[test]
fn bash_zsh_bootstrap_snippet_comment_says_phosphorize() {
    assert!(
        BASH_ZSH_SNIPPET.contains("# Auto-Phosphorize"),
        "expected the bash/zsh rc-file comment to say Phosphorize: {BASH_ZSH_SNIPPET:?}"
    );
    assert_no_stray_warp_branding(BASH_ZSH_SNIPPET, "bash/zsh bootstrap snippet");
}

#[test]
fn legacy_remote_snippet_names_phosphor() {
    assert!(
        LEGACY_REMOTE_SNIPPET.contains("Phosphor runs commands"),
        "expected the legacy remote-subshell snippet to name Phosphor: {LEGACY_REMOTE_SNIPPET:?}"
    );
    assert_no_stray_warp_branding(LEGACY_REMOTE_SNIPPET, "legacy remote-subshell snippet");
}

/// Both hook-carrying snippets emit the same DCS hook the ANSI parser expects
/// (`dcs_hooks.rs`'s `"SourcedRcFileForWarp" => Some(DProtoHook::SourcedRcFileForWarp { .. })`).
/// If this ever drifts, auto-bootstrap silently stops being recognized on the
/// other end.
#[test]
fn pwsh_init_shell_execution_policy_error_says_phosphorize() {
    assert!(
        PWSH_INIT_SHELL_SNIPPET.contains("Unable to Phosphorize this PowerShell session."),
        "expected the pwsh execution-policy Write-Error to say Phosphorize: not found in \
         pwsh_init_shell.ps1"
    );
    assert!(
        !PWSH_INIT_SHELL_SNIPPET.contains("Warpify"),
        "pwsh_init_shell.ps1's user-visible execution-policy error still says Warpify"
    );
}

#[test]
fn shell_snippets_still_emit_the_expected_wire_hook() {
    for (label, snippet) in [("fish", FISH_SNIPPET), ("bash/zsh", BASH_ZSH_SNIPPET)] {
        assert!(
            snippet.contains(r#""hook": "SourcedRcFileForWarp""#),
            "{label} snippet no longer emits the SourcedRcFileForWarp hook: {snippet:?}"
        );
    }
}

/// The pwsh RC-snippet template (#800) builds the hook JSON with backtick-escaped
/// quotes (it is itself the body of a future PowerShell double-quoted string literal),
/// so it carries the same wire hook as the other snippets but spelled differently --
/// checked on its own rather than folded into `shell_snippets_still_emit_the_expected_wire_hook`.
#[test]
fn pwsh_bootstrap_snippet_comment_says_phosphorize_and_emits_the_expected_wire_hook() {
    assert!(
        PWSH_SNIPPET.contains("# Auto-Phosphorize"),
        "expected the pwsh rc-file comment to say Phosphorize: {PWSH_SNIPPET:?}"
    );
    assert_no_stray_warp_branding(PWSH_SNIPPET, "pwsh bootstrap snippet");
    assert!(
        PWSH_SNIPPET.contains(r#"`"hook`": `"SourcedRcFileForWarp`""#),
        "pwsh snippet no longer emits the SourcedRcFileForWarp hook: {PWSH_SNIPPET:?}"
    );
}

/// `replace_template_chars_with_arguments` `debug_assert!`s if the number of '%'
/// placeholders in the asset doesn't match the number of arguments supplied -- this
/// guards the pwsh template (#800) against silently drifting out of sync with that
/// argument count, and against `get_subshell_bootstrap_success_block_path` regressing
/// back to `None` for `ShellType::PowerShell`.
#[test]
fn pwsh_subshell_bootstrap_success_block_bytes_is_non_empty_and_executable() {
    let subshell_initialization_info = SubshellInitializationInfo {
        spawning_command: "pwsh -NoLogo".to_owned(),
        was_triggered_by_rc_file_snippet: false,
        env_var_collection_name: None,
        ssh_connection_info: None,
    };

    let (bytes, is_executable) = subshell_bootstrap_success_block_bytes(
        &subshell_initialization_info,
        ShellType::PowerShell,
        TargetOS::Linux,
        false,
    );

    assert!(
        !bytes.is_empty(),
        "expected a non-empty pwsh subshell bootstrap success block"
    );
    assert!(
        is_executable,
        "expected the pwsh subshell bootstrap success block to be executable on Linux"
    );
}

/// `ShellType::PowerShell.rc_file_paths(TargetOS::Linux)` returns *two* paths (it
/// writes to both the PowerShell Core and Windows PowerShell profile locations), so
/// `subshell_bootstrap_success_block_bytes` concatenates two per-path commands here --
/// unlike every other shell, which has exactly one rc file and so never exercises the
/// multi-command concatenation at all. Each per-path command is a complete,
/// terminator-free statement (a single `Add-Content ... -Path '<path>'` call), so
/// gluing them together with no separator produces one malformed `Add-Content`
/// invocation that pwsh rejects with "parameter 'Value' is specified more than once"
/// and that appends to neither profile file (confirmed by running the generated
/// command through pwsh directly). This asserts the two invocations stay separate
/// statements and both target paths are actually present in the output.
#[test]
fn pwsh_subshell_bootstrap_success_block_keeps_multiple_rc_commands_separate() {
    let subshell_initialization_info = SubshellInitializationInfo {
        spawning_command: "pwsh -NoLogo".to_owned(),
        was_triggered_by_rc_file_snippet: false,
        env_var_collection_name: None,
        ssh_connection_info: None,
    };

    let (bytes, _) = subshell_bootstrap_success_block_bytes(
        &subshell_initialization_info,
        ShellType::PowerShell,
        TargetOS::Linux,
        false,
    );
    let command = String::from_utf8(bytes).expect("command should be utf8");

    assert_eq!(
        command.matches("Add-Content").count(),
        2,
        "expected two separate Add-Content invocations (one per PowerShell profile \
         location), got: {command:?}"
    );
    assert!(
        !command.contains("'Add-Content"),
        "the end of one Add-Content invocation was glued directly onto the start of \
         the next with no statement separator, which pwsh cannot parse as two \
         statements: {command:?}"
    );
    assert!(
        command.contains("Documents/PowerShell/Microsoft.PowerShell_profile.ps1")
            && command.contains("Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"),
        "expected both PowerShell profile paths to appear in the generated command: \
         {command:?}"
    );
}
