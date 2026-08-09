use warp_cli::agent::Harness;

use super::{
    build_local_claude_child_command, build_local_codex_child_command,
    build_local_opencode_child_command, local_claude_child_prompt,
    normalize_local_child_harness, validate_local_harness_shell,
};
use crate::terminal::shell::ShellType;

/// Ported from the pin (`app/src/pane_group/pane/local_harness_launch_tests.rs:26-38`,
/// `02b53fcd8`) verbatim, for #323.
#[test]
fn local_claude_child_prompt_includes_oz_cli_messaging_instructions() {
    let prompt = local_claude_child_prompt("List files");

    assert!(prompt.contains("OZ_CLI"));
    assert!(prompt.contains("OZ_RUN_ID"));
    assert!(prompt.contains("OZ_PARENT_RUN_ID"));
    assert!(prompt.contains("run message send --sender-run-id"));
    assert!(prompt.contains("All four send arguments are required"));
    assert!(prompt.contains("Do not pass \"$OZ_PARENT_RUN_ID\" as a positional argument to send"));
    assert!(prompt.contains("run message list \"$OZ_RUN_ID\" --limit 25"));
    assert!(prompt.contains("do not rely on --unread"));
    assert!(!prompt.contains("--unread --limit"));
    assert!(prompt.contains("Do not use Claude Code Agent or SendMessage tools"));
    assert!(prompt.ends_with("Task:\nList files"));
}

#[test]
fn normalize_local_child_harness_accepts_supported_aliases() {
    assert_eq!(
        normalize_local_child_harness("claude"),
        Some(Harness::Claude)
    );
    assert_eq!(
        normalize_local_child_harness("claude-code"),
        Some(Harness::Claude)
    );
    assert_eq!(
        normalize_local_child_harness("claude_code"),
        Some(Harness::Claude)
    );
    assert_eq!(
        normalize_local_child_harness("opencode"),
        Some(Harness::OpenCode)
    );
    assert_eq!(
        normalize_local_child_harness("open-code"),
        Some(Harness::OpenCode)
    );
    assert_eq!(
        normalize_local_child_harness("open_code"),
        Some(Harness::OpenCode)
    );
}

#[test]
fn normalize_local_child_harness_rejects_unsupported_values() {
    assert_eq!(normalize_local_child_harness("oz"), None);
    assert_eq!(normalize_local_child_harness(""), None);
}

#[test]
fn normalize_local_child_harness_accepts_codex() {
    // Issue #411's pinned-parity requirement made `Harness::parse_local_child_harness`
    // recognize "codex", so it now parses successfully. #323 completed the launch
    // side (`build_local_codex_child_command`, wired into
    // `prepare_local_harness_child_launch`'s `Harness::Codex` arm), so parsing and
    // launching are both supported now.
    assert_eq!(
        normalize_local_child_harness("codex"),
        Some(Harness::Codex)
    );
}

#[test]
fn validate_local_harness_shell_accepts_supported_shells() {
    assert_eq!(validate_local_harness_shell(Some(ShellType::Bash)), Ok(()));
    assert_eq!(validate_local_harness_shell(Some(ShellType::Zsh)), Ok(()));
    assert_eq!(validate_local_harness_shell(Some(ShellType::Fish)), Ok(()));
}

#[test]
fn validate_local_harness_shell_rejects_unsupported_shells() {
    assert_eq!(
        validate_local_harness_shell(Some(ShellType::PowerShell)),
        Err(
            "Local child harnesses currently require bash, zsh, or fish; PowerShell is not supported."
                .to_string()
        )
    );
    assert_eq!(
        validate_local_harness_shell(None),
        Err(
            "Local child harnesses currently require a detected bash, zsh, or fish session."
                .to_string()
        )
    );
}

#[test]
fn build_local_claude_child_command_quotes_the_prompt() {
    let command = build_local_claude_child_command("hello world");

    assert!(command.starts_with("claude --session-id "));
    assert!(command.ends_with(" --dangerously-skip-permissions 'hello world'"));
}

#[test]
fn build_local_opencode_child_command_quotes_the_prompt() {
    assert_eq!(
        build_local_opencode_child_command("hello world"),
        "opencode --prompt 'hello world'"
    );
}

/// Ported from the pin (`local_harness_launch_tests.rs:165-171`, `02b53fcd8`) verbatim, for #323.
#[test]
fn build_local_codex_child_command_quotes_the_prompt() {
    assert_eq!(
        build_local_codex_child_command("hello world"),
        "codex --dangerously-bypass-approvals-and-sandbox 'hello world'"
    );
}
