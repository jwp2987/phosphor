//! UI-signal marker-style tools: executing them just means "notify the frontend to
//! do something", and the result is a fixed ack.
//!
//! - `open_code_review`: open the Code Review panel
//! - `transfer_shell_command_control_to_user`: hand PTY control of a long-running
//!   command back to the user
//!
//! These tools' protobuf fields are minimal (an empty message or a single field);
//! the executor mostly just takes the marker path that returns a fixed result
//! directly, and the actual side effect on the client is triggered when the
//! UI/Terminal listens for the corresponding ToolCall message.

use agent_proto as api;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{Value, json};

use super::OpenAiTool;

// ---------------------------------------------------------------------------
// open_code_review
// ---------------------------------------------------------------------------

fn empty_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn open_code_review_from_args(_args: &str) -> Result<api::message::tool_call::Tool> {
    Ok(api::message::tool_call::Tool::OpenCodeReview(
        api::message::tool_call::OpenCodeReview {},
    ))
}

fn open_code_review_result_to_json(
    result: &api::message::tool_call_result::Result,
) -> Option<Value> {
    use api::message::tool_call_result::Result as R;
    match result {
        R::OpenCodeReview(_) => Some(json!({ "status": "ok" })),
        _ => None,
    }
}

pub static OPEN_CODE_REVIEW: OpenAiTool = OpenAiTool {
    name: "open_code_review",
    description: "Open the Code Review panel for the current project (client UI signal, no \
                  parameters). Use when the user explicitly asks for a code review, or context \
                  shows the review phase is starting.",
    parameters: empty_parameters,
    from_args: open_code_review_from_args,
    result_to_json: open_code_review_result_to_json,
};

// ---------------------------------------------------------------------------
// transfer_shell_command_control_to_user
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TransferArgs {
    /// Explanation shown to the user: why control is being handed back.
    #[serde(default)]
    reason: String,
}

fn transfer_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "reason": {
                "type": "string",
                "description": "Explanation shown to the user for why control is being handed back (e.g. \"you need to log in interactively now\"). Write it in the same language as the user's messages."
            }
        },
        "additionalProperties": false
    })
}

fn transfer_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: TransferArgs = if args.trim().is_empty() {
        TransferArgs {
            reason: String::new(),
        }
    } else {
        serde_json::from_str(args)?
    };
    Ok(
        api::message::tool_call::Tool::TransferShellCommandControlToUser(
            api::message::tool_call::TransferShellCommandControlToUser {
                reason: parsed.reason,
            },
        ),
    )
}

fn transfer_result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::message::tool_call_result::Result as R;
    use api::transfer_shell_command_control_to_user_result::Result as TR;
    let r = match result {
        R::TransferShellCommandControlToUser(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(TR::LongRunningCommandSnapshot(s)) => json!({
            "status": "transferred",
            "command_id": s.command_id,
            "output": s.output,
            "is_alt_screen_active": s.is_alt_screen_active,
        }),
        Some(TR::CommandFinished(f)) => json!({
            "status": "completed",
            "command_id": f.command_id,
            "exit_code": f.exit_code,
            "output": f.output,
        }),
        // #1799: say why nothing was handed over, so the model does not retry blindly.
        Some(TR::Error(_)) => json!({
            "status": "error",
            "message": "block_not_found",
            "detail": TRANSFER_NOTHING_RUNNING_DETAIL,
        }),
        // Not what the model reads for a cancel: a cancelled transfer has no API form
        // (`convert.rs` ignores it), so its text comes from the result's `Display`, which
        // carries the reason (`TRANSFER_CANCELLED_REASON`, #1799).
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

/// The `detail` of a hand-over that found no running command (#1799).
const TRANSFER_NOTHING_RUNNING_DETAIL: &str = "No command was running, so nothing was handed \
     to the user. If the command this was for was rejected or has not started, it never ran.";

pub static TRANSFER_SHELL_CONTROL: OpenAiTool = OpenAiTool {
    name: "transfer_shell_command_control_to_user",
    description: "Hand PTY control of the current long-running shell command back to the user. \
                  Use when the command needs manual interaction and \
                  write_to_long_running_shell_command is a poor fit (interactive logins, cases \
                  where the user must watch live terminal output to decide the next step, etc.). \
                  The reason field is shown to the user to explain the handback.",
    parameters: transfer_parameters,
    from_args: transfer_from_args,
    result_to_json: transfer_result_to_json,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer_result(
        result: Option<api::transfer_shell_command_control_to_user_result::Result>,
    ) -> api::message::tool_call_result::Result {
        api::message::tool_call_result::Result::TransferShellCommandControlToUser(
            api::TransferShellCommandControlToUserResult { result },
        )
    }

    /// #1799: a hand-over cancelled with its turn (the command it was for was rejected) tells
    /// the model why. Through the real serialization: a cancelled transfer has no API form, so
    /// `serialize_action_result` gives nothing and the live request (`chat_stream`) and the
    /// persisted message both send `display_fallback_result`, the result's `Display`.
    #[test]
    fn cancelled_transfer_reason_reaches_the_model() {
        use crate::ai::agent::task::TaskId;
        use crate::ai::agent::{
            AIAgentActionId, AIAgentActionResult, AIAgentActionResultType,
            TransferShellCommandControlToUserResult,
        };

        let result = AIAgentActionResult {
            id: AIAgentActionId::from("hand-over".to_owned()),
            task_id: TaskId::new("task".to_owned()),
            result: AIAgentActionResultType::TransferShellCommandControlToUser(
                TransferShellCommandControlToUserResult::Cancelled,
            ),
        };
        assert_eq!(
            super::super::serialize_action_result(&result),
            None,
            "no API form: the Display fallback is what the model reads"
        );
        let text = super::super::display_fallback_result(&result);
        assert!(text.contains("cancelled"), "{text}");
        assert!(text.contains("rejected an earlier tool call"), "{text}");
    }

    /// #1799: a hand-over with no running command (it resolves without approval) says that
    /// nothing was handed over; the `block_not_found` code is kept for anything matching on it.
    #[test]
    fn transfer_with_nothing_running_says_why() {
        let value = transfer_result_to_json(&transfer_result(Some(
            api::transfer_shell_command_control_to_user_result::Result::Error(Default::default()),
        )))
        .expect("transfer result");
        assert_eq!(value["status"], "error");
        assert_eq!(value["message"], "block_not_found");
        assert!(
            value["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("nothing was handed")),
            "{value}"
        );
    }
}
