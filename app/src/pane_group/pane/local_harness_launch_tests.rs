use warp_cli::agent::Harness;

use super::{
    build_local_claude_child_command, build_local_opencode_child_command,
    compose_child_agent_prompt, normalize_local_child_harness,
    prepare_local_harness_child_launch, split_orchestrate_tasks, validate_local_harness_shell,
};
use crate::terminal::shell::ShellType;

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
    // recognize "codex", so it now parses successfully. That does NOT mean local
    // Codex child harnesses can actually be launched yet -- see
    // `prepare_local_harness_child_launch_rejects_codex` below, which proves the
    // rejection moved from parsing to launch rather than disappearing.
    assert_eq!(
        normalize_local_child_harness("codex"),
        Some(Harness::Codex)
    );
}

#[tokio::test]
async fn prepare_local_harness_child_launch_rejects_codex() {
    // "codex" now parses (see `normalize_local_child_harness_accepts_codex`), but
    // there is no local-child spawn implementation for it yet (that's issue #323's
    // scope). Launching must still fail clearly instead of silently no-oping.
    let result = prepare_local_harness_child_launch(
        "prompt".to_string(),
        "codex".to_string(),
        None,
        Some(ShellType::Bash),
        None,
    )
    .await;

    match result {
        Err(message) => assert_eq!(
            message,
            "Local Codex child harness support is not yet implemented."
        ),
        Ok(_) => panic!("local Codex child harness launch unexpectedly succeeded"),
    }
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

#[test]
fn split_orchestrate_tasks_splits_on_semicolon() {
    assert_eq!(
        split_orchestrate_tasks("write tests; update the docs"),
        vec!["write tests".to_string(), "update the docs".to_string()]
    );
}

#[test]
fn split_orchestrate_tasks_trims_and_drops_empty_segments() {
    // Leading, trailing, and doubled `;` should not produce empty tasks.
    assert_eq!(
        split_orchestrate_tasks("; write tests ;; update the docs ; "),
        vec!["write tests".to_string(), "update the docs".to_string()]
    );
}

#[test]
fn split_orchestrate_tasks_single_task_has_no_semicolon() {
    assert_eq!(
        split_orchestrate_tasks("write tests"),
        vec!["write tests".to_string()]
    );
}

#[test]
fn split_orchestrate_tasks_blank_argument_spawns_nothing() {
    assert_eq!(split_orchestrate_tasks("   "), Vec::<String>::new());
}

#[test]
fn compose_child_agent_prompt_trims_whitespace() {
    assert_eq!(
        compose_child_agent_prompt("  write tests  "),
        "write tests"
    );
}

#[test]
fn compose_child_agent_prompt_is_a_verbatim_passthrough() {
    // No parent-transcript summarization or wrapping -- see the doc comment
    // on `compose_child_agent_prompt` for why.
    let task = "Refactor `foo.rs` to use the new API; keep tests green";
    assert_eq!(compose_child_agent_prompt(task), task);
}
