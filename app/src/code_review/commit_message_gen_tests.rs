use async_io::block_on;

use super::*;
use crate::settings::{AgentProviderApiType, ReasoningEffortSetting};

/// A config that is structurally valid but points at a base URL nothing listens
/// on. Only used by the "we never reach the network" test below.
fn unreachable_config() -> OneshotConfig {
    OneshotConfig {
        base_url: "http://127.0.0.1:1/v1".to_owned(),
        api_key: "test-key".to_owned(),
        model_id: "test-model".to_owned(),
        api_type: AgentProviderApiType::OpenAi,
        reasoning_effort: ReasoningEffortSetting::Auto,
    }
}

#[test]
fn commit_message_prompt_is_registered_and_usable() {
    let prompt = prompt_renderer::commit_message_system_prompt();
    assert!(
        !prompt.trim().is_empty(),
        "the built-in commit-message prompt must not be empty"
    );
    assert!(
        prompt.contains("commit message"),
        "prompt should describe the commit-message task, got: {prompt}"
    );
}

#[test]
fn user_prompt_carries_branch_and_diff() {
    let prompt = build_user_prompt("diff --git a/a.rs b/a.rs\n+fn a() {}", "feat/tabs");
    assert!(prompt.contains("<branch>feat/tabs</branch>"));
    assert!(prompt.contains("diff --git a/a.rs b/a.rs"));
    assert!(prompt.contains("</diff>"));
}

#[test]
fn empty_diff_short_circuits_before_any_request() {
    // A blank diff must fail without a model round trip; if it did reach the
    // network this would hang or fail with a connection error instead.
    let err = block_on(generate_commit_message_from_diff(
        &unreachable_config(),
        "   \n\t\n",
        "main",
    ))
    .expect_err("a blank diff has nothing to summarize");
    assert!(
        err.to_string().contains("no changes"),
        "expected the no-changes bail, got: {err}"
    );
}

#[test]
fn sanitize_keeps_a_plain_subject_line() {
    assert_eq!(
        sanitize_commit_message("Fix off-by-one in tab index clamping\n"),
        Some("Fix off-by-one in tab index clamping".to_owned())
    );
}

#[test]
fn sanitize_preserves_subject_and_body() {
    let raw = "Add commit message generation\n\n- Draft from the diff\n- Reuse the one-shot path\n";
    assert_eq!(
        sanitize_commit_message(raw),
        Some(
            "Add commit message generation\n\n- Draft from the diff\n- Reuse the one-shot path"
                .to_owned()
        ),
        "a multi-line message must keep its body"
    );
}

#[test]
fn sanitize_strips_reasoning_blocks() {
    let raw = "<think>The diff renames a field.</think>\nRename field to snake_case";
    assert_eq!(
        sanitize_commit_message(raw),
        Some("Rename field to snake_case".to_owned())
    );
}

#[test]
fn sanitize_strips_repeated_reasoning_blocks() {
    let raw = "<reasoning>one</reasoning><reasoning>two</reasoning>Fix the parser";
    assert_eq!(
        sanitize_commit_message(raw),
        Some("Fix the parser".to_owned())
    );
}

#[test]
fn sanitize_strips_code_fence() {
    let raw = "```\nFix the parser\n\n- handle empty input\n```";
    assert_eq!(
        sanitize_commit_message(raw),
        Some("Fix the parser\n\n- handle empty input".to_owned())
    );
}

#[test]
fn sanitize_strips_language_tagged_code_fence() {
    assert_eq!(
        sanitize_commit_message("```text\nFix the parser\n```"),
        Some("Fix the parser".to_owned())
    );
}

#[test]
fn sanitize_strips_unclosed_code_fence() {
    assert_eq!(
        sanitize_commit_message("```\nFix the parser"),
        Some("Fix the parser".to_owned())
    );
}

#[test]
fn sanitize_strips_label_prefix() {
    assert_eq!(
        sanitize_commit_message("Commit message: Fix the parser"),
        Some("Fix the parser".to_owned())
    );
    assert_eq!(
        sanitize_commit_message("SUBJECT:\n\nFix the parser"),
        Some("Fix the parser".to_owned())
    );
}

#[test]
fn sanitize_strips_wrapping_quotes() {
    assert_eq!(
        sanitize_commit_message("\"Fix the parser\""),
        Some("Fix the parser".to_owned())
    );
    assert_eq!(
        sanitize_commit_message("`Fix the parser`"),
        Some("Fix the parser".to_owned())
    );
}

#[test]
fn sanitize_keeps_unbalanced_quotes() {
    // Only a matching pair is a wrapper; a stray leading quote is message text.
    assert_eq!(
        sanitize_commit_message("\"Fix the parser"),
        Some("\"Fix the parser".to_owned())
    );
}

#[test]
fn sanitize_returns_none_for_empty_output() {
    assert_eq!(sanitize_commit_message(""), None);
    assert_eq!(sanitize_commit_message("   \n\n  "), None);
    assert_eq!(sanitize_commit_message("<think>only reasoning</think>"), None);
    assert_eq!(sanitize_commit_message("```"), None);
}

#[test]
fn sanitize_handles_non_ascii_leading_character() {
    // Guards the label-prefix slicing against a multi-byte first character.
    assert_eq!(
        sanitize_commit_message("\u{dc}nify the parser entry points"),
        Some("\u{dc}nify the parser entry points".to_owned())
    );
}
