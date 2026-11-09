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
        None => json!({ "status": "cancelled", "detail": TRANSFER_CANCELLED_DETAIL }),
    };
    Some(value)
}

/// The `detail` of a hand-over that found no running command (#1799).
const TRANSFER_NOTHING_RUNNING_DETAIL: &str = "No command was running, so nothing was handed \
     to the user. If the command this was for was rejected or has not started, it never ran.";

/// The `detail` of a cancelled hand-over (#1799). A user's reject of one tool call cancels the
/// later calls of the same turn (`rejection_cancels_rest_of_turn` in the action model), and a
/// hand-over is usually such a later call: the command it was meant for was the one rejected.
const TRANSFER_CANCELLED_DETAIL: &str = "Control was not handed to the user: the user \
     cancelled this call, or rejected an earlier tool call of the same turn, which cancels the \
     rest of that turn. If a hand-over had started, it was stopped.";

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
    /// the model why, instead of a bare `cancelled`.
    #[test]
    fn cancelled_transfer_says_why() {
        let value = transfer_result_to_json(&transfer_result(None)).expect("transfer result");
        assert_eq!(value["status"], "cancelled");
        let detail = value["detail"].as_str().expect("a detail for the model");
        assert!(detail.contains("rejected an earlier tool call"), "{detail}");
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
