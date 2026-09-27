//! Unit tests for format_command_text in requested_command.rs

use super::{
    COMMAND_CANCELLED_BY_USER_MESSAGE, COMMAND_CANCELLED_FOR_FOLLOW_UP_MESSAGE,
    COMMAND_CANCELLED_FOR_RUNNING_COMMAND_MESSAGE, COMMAND_CANCELLED_FOR_SHELL_EXIT_MESSAGE,
    COMMAND_CANCELLED_FOR_USER_COMMAND_MESSAGE, COMMAND_DENYLISTED_MESSAGE,
    COMMAND_REJECTED_BY_USER_MESSAGE, cancel_explanation_for_reason, denylisted_command_message,
    format_command_text, header_message_for_user_take_over_reason, mcp_blocked_title_text,
    mcp_viewing_detail_title_text,
};
use crate::ai::agent::{CancellationReason, RequestCommandOutputResult};
use crate::ai::blocklist::block::cli_controller::UserTakeOverReason;

#[test]
fn single_line_without_newline_is_unchanged_ascii() {
    let input = "echo hello world";
    let output = format_command_text(input);
    assert_eq!(output, input);
}

#[test]
fn single_line_without_newline_preserves_multibyte_characters() {
    let input = "echo 🚀✨";
    let output = format_command_text(input);
    assert_eq!(output, input);

    // Additional sanity check: string is valid UTF-8 and can be iterated by chars without panic
    let collected: String = output.chars().collect();
    assert_eq!(collected, output);
}

#[test]
fn truncates_at_first_newline_and_appends_ellipsis_when_more_content_exists() {
    let input = "cargo build\n--release";
    let output = format_command_text(input);
    assert_eq!(output, "cargo build…");
}

#[test]
fn truncates_at_first_newline_without_ellipsis_when_rest_is_whitespace() {
    let input = "git status\n   \t  ";
    let output = format_command_text(input);
    assert_eq!(output, "git status");
}

#[test]
fn does_not_split_multibyte_char_across_utf8_boundaries_when_newline_follows() {
    // The emoji is a multi-byte sequence; ensure truncation at the newline does not split it.
    let input = "echo 🧪\nthen do something";
    let output = format_command_text(input);
    assert_eq!(output, "echo 🧪…");

    // Validate resulting string is valid UTF-8 by iterating graphemes via chars
    let reconstructed: String = output.chars().collect();
    assert_eq!(reconstructed, output);
}

#[test]
fn preserves_combining_characters_when_newline_is_after_cluster() {
    // "e" + combining acute accent
    // Sanity checks that the formatter doesn't split this unicode sequence
    let composed = format!("{}{}", 'e', '\u{0301}');
    let input = format!("echo {composed}\nnext");
    let output = format_command_text(&input);
    assert_eq!(output, format!("echo {composed}…"));

    // Still valid UTF-8 and same when re-collected from chars
    let reconstructed: String = output.chars().collect();
    assert_eq!(reconstructed, output);
}

#[test]
fn mcp_blocked_title_surfaces_tool_and_server_when_known() {
    assert_eq!(
        mcp_blocked_title_text("create_issue", Some("github")),
        "OK if I call MCP tool create_issue on server github"
    );
}

#[test]
fn mcp_blocked_title_falls_back_to_tool_name_when_server_unknown() {
    assert_eq!(
        mcp_blocked_title_text("create_issue", None),
        "OK if I call MCP tool create_issue"
    );
}

#[test]
fn mcp_blocked_title_falls_back_to_generic_message_when_tool_name_empty() {
    assert_eq!(
        mcp_blocked_title_text("", Some("github")),
        "OK if I call this MCP tool?"
    );
    assert_eq!(
        mcp_blocked_title_text("", None),
        "OK if I call this MCP tool?"
    );
}

#[test]
fn mcp_viewing_detail_title_surfaces_tool_and_server_when_known() {
    assert_eq!(
        mcp_viewing_detail_title_text("create_issue", Some("github")),
        "Viewing MCP tool create_issue on github"
    );
    assert_eq!(
        mcp_viewing_detail_title_text("create_issue", None),
        "Viewing MCP tool create_issue"
    );
}

#[test]
fn mcp_viewing_detail_title_falls_back_to_generic_message_when_tool_name_empty() {
    assert_eq!(
        mcp_viewing_detail_title_text("", Some("github")),
        "Viewing MCP tool call detail"
    );
}

// Every take-over reason has to say something specific in the block header. A password-prompt
// hand-over in particular must not read as "User is in control": the user did not choose to
// take it, and without naming the reason the block looks identical to a manual take-over while
// the actual prompt may be off-screen (under tmux, on another pane entirely).
#[test]
fn header_message_names_every_take_over_reason() {
    assert_eq!(
        header_message_for_user_take_over_reason(&UserTakeOverReason::Manual),
        "User is in control."
    );
    assert_eq!(
        header_message_for_user_take_over_reason(&UserTakeOverReason::Stop {
            should_auto_resume: true
        }),
        "Paused agent. User is in control."
    );
    assert_eq!(
        header_message_for_user_take_over_reason(&UserTakeOverReason::TransferFromAgent {
            reason: "enter password".to_owned()
        }),
        "User in control"
    );
    assert_eq!(
        header_message_for_user_take_over_reason(&UserTakeOverReason::BlockedOnInput),
        "Command needs your input. User is in control."
    );
}

#[test]
fn newline_then_multibyte_results_in_ellipsis_only() {
    let input = "\n🚀";
    let output = format_command_text(input);
    assert_eq!(output, "…");

    // Sanity: output remains valid UTF-8
    let reconstructed: String = output.chars().collect();
    assert_eq!(reconstructed, output);
}

// A rejected/cancelled command used to show a bare icon next to the command text, with
// nothing distinguishing "you rejected this" from "it failed" from "the app dropped it" --
// c6124b6e4 fixed only the no-reason (long-running-command drain) case. See issue #692.

#[test]
fn no_reason_and_no_block_is_the_running_command_drain() {
    assert_eq!(
        cancel_explanation_for_reason(None, false, false),
        Some(COMMAND_CANCELLED_FOR_RUNNING_COMMAND_MESSAGE)
    );
}

#[test]
fn a_command_block_that_actually_started_is_never_explained_here() {
    // Whatever the reason, a block existing means the command started and was torn down some
    // other way, which has its own rendering (the block's own exit-code icon).
    for reason in [
        None,
        Some(CancellationReason::ManuallyCancelled),
        Some(CancellationReason::UserCommandExecuted),
        Some(CancellationReason::AgentExitedShell),
    ] {
        assert_eq!(cancel_explanation_for_reason(reason, true, true), None);
    }
}

#[test]
fn reject_button_says_rejected_by_you() {
    assert_eq!(
        cancel_explanation_for_reason(Some(CancellationReason::ManuallyCancelled), false, true),
        Some(COMMAND_REJECTED_BY_USER_MESSAGE)
    );
}

#[test]
fn manually_cancelled_without_the_reject_flag_says_cancelled_by_you() {
    // e.g. the conversation's Stop button firing while this action was still pending --
    // distinct from this row's own Reject button.
    assert_eq!(
        cancel_explanation_for_reason(Some(CancellationReason::ManuallyCancelled), false, false),
        Some(COMMAND_CANCELLED_BY_USER_MESSAGE)
    );
}

#[test]
fn follow_up_submitted_names_the_follow_up() {
    assert_eq!(
        cancel_explanation_for_reason(
            Some(CancellationReason::FollowUpSubmitted {
                is_for_same_conversation: true
            }),
            false,
            false
        ),
        Some(COMMAND_CANCELLED_FOR_FOLLOW_UP_MESSAGE)
    );
}

#[test]
fn user_command_executed_names_the_terminal_command() {
    assert_eq!(
        cancel_explanation_for_reason(Some(CancellationReason::UserCommandExecuted), false, false),
        Some(COMMAND_CANCELLED_FOR_USER_COMMAND_MESSAGE)
    );
}

#[test]
fn agent_exited_shell_names_the_shell_exit() {
    assert_eq!(
        cancel_explanation_for_reason(Some(CancellationReason::AgentExitedShell), false, false),
        Some(COMMAND_CANCELLED_FOR_SHELL_EXIT_MESSAGE)
    );
}

#[test]
fn torn_down_rather_than_cancelled_reasons_stay_unlabelled() {
    for reason in [
        CancellationReason::Reverted,
        CancellationReason::Deleted,
        CancellationReason::OptimisticCLISubagentCompletion,
    ] {
        assert_eq!(
            cancel_explanation_for_reason(Some(reason), false, false),
            None
        );
    }
}

#[test]
fn denylisted_result_is_labelled() {
    assert_eq!(
        denylisted_command_message(&RequestCommandOutputResult::Denylisted {
            command: "rm -rf /".to_owned()
        }),
        Some(COMMAND_DENYLISTED_MESSAGE)
    );
}

#[test]
fn non_denylisted_result_is_not_labelled_here() {
    assert_eq!(
        denylisted_command_message(&RequestCommandOutputResult::CancelledBeforeExecution),
        None
    );
}
