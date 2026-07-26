//! `todowrite` BYOP tool descriptor.
//!
//! Like webfetch / websearch, this does **not** map to a protobuf executor variant — instead
//! `chat_stream.rs` intercepts by name before `parse_incoming_tool_call` and synthesizes a
//! `Message::UpdateTodos` directly into the conversation, triggering a chip + popup UI update.
//!
//! Protocol design aligned with opencode's `todowrite`:
//! - Input: `{ todos: [{ content, status, priority? }] }`, **full-overwrite semantics** (every
//!   call replaces the entire list)
//! - status: `pending` / `in_progress` / `completed` / `cancelled`
//! - The client computes a stable id from `content` (SHA-256 prefix, 16 hex) to avoid the chip
//!   numbers churning on every refresh
//!
//! ## Emit strategy
//!
//! Each intercepted todowrite call synthesizes two `Message::UpdateTodos`:
//! 1. `CreateTodoList { initial_todos: [all todos] }` (everything starts as pending)
//! 2. `MarkTodosCompleted { todo_ids: [ids with status=completed/cancelled] }`
//!
//! `update_todo_list_from_todo_op` moves the items matched by the second message from pending
//! to completed (`mark_todos_complete` looks up the id within pending), so the final
//! `AIAgentTodoList` state is: `completed_items = [completed]`, `pending_items = [pending +
//! in_progress]`. The Zap UI's `in_progress_item()` takes `pending_items.first()`, so the
//! in_progress todo should be the first item in the `todos` array whose
//! `status != completed/cancelled`.
//!
//! A pair of `Message::ToolCall` (carrier, tool=None) + `Message::ToolCallResult` is then
//! synthesized to unblock the upstream model.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use warp_multi_agent_api as api;

use super::OpenAiTool;

pub const TOOL_NAME: &str = "todowrite";

#[derive(Debug, Deserialize)]
pub struct Args {
    pub todos: Vec<TodoArg>,
}

#[derive(Debug, Deserialize)]
pub struct TodoArg {
    pub content: String,
    /// `pending` | `in_progress` | `completed` | `cancelled`. The model occasionally sends
    /// some other string; unrecognized values fall back to `pending` during parsing.
    #[serde(default)]
    pub status: String,
    /// The opencode protocol carries a priority; Zap's data model doesn't distinguish it, so
    /// it's accepted here but unused — kept so the model can send it out of opencode habit
    /// without erroring.
    #[serde(default, rename = "priority")]
    pub _priority: Option<String>,
}

fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "todos": {
                "type": "array",
                "description": "The full updated todo list. Pass every item every call (overwrite semantics).",
                "items": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "Brief description of the task (1 line)."
                        },
                        "status": {
                            "type": "string",
                            "enum": ["pending", "in_progress", "completed", "cancelled"],
                            "description": "Current status."
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["high", "medium", "low"],
                            "description": "Optional priority. Currently advisory only."
                        }
                    },
                    "required": ["content", "status"]
                }
            }
        },
        "required": ["todos"],
        "additionalProperties": false
    })
}

fn from_args(_args: &str) -> Result<api::message::tool_call::Tool> {
    Err(anyhow!(
        "todowrite is intercepted by chat_stream BYOP todo dispatcher; \
         from_args should never be called"
    ))
}

fn result_to_json(_result: &api::message::tool_call_result::Result) -> Option<Value> {
    None
}

pub static TODOWRITE: OpenAiTool = OpenAiTool {
    name: TOOL_NAME,
    description: include_str!("../prompts/tool_descriptions/todowrite.md"),
    parameters,
    from_args,
    result_to_json,
};

/// Synthesizes the todowrite tool_result shown to the upstream model.
///
/// `todowrite` is a locally intercepted tool that never produces an `AIAgentAction`, so it
/// must carry the `_byop_intercepted` sentinel. The controller uses this flag to trigger
/// auto-resume, letting the model continue the loop once it receives the tool_result on the
/// next round.
pub fn success_result_to_json(message: &'static str) -> Value {
    json!({
        "_byop_intercepted": true,
        "status": "ok",
        "message": message,
    })
}

pub fn invalid_arguments_result_to_json(detail: String, received_args: &str) -> Value {
    json!({
        "_byop_intercepted": true,
        "error": "invalid_arguments",
        "detail": detail,
        "tool": TOOL_NAME,
        "received_args": received_args,
        "hint": "Expected { todos: [{ content: string, status: string }] }.",
    })
}

/// Computes a stable id from content. When the model sends the same content again for a
/// todo, it gets the same id, so `mark_todos_complete(todo_ids)` can match it within pending
/// → move it to completed.
fn stable_id(content: &str) -> String {
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    let bytes = h.finalize();
    // Take the first 8 bytes = 16 hex chars, stable and short enough.
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn to_todo_item(arg: &TodoArg) -> api::TodoItem {
    api::TodoItem {
        id: stable_id(&arg.content),
        title: arg.content.clone(),
        description: String::new(),
    }
}

fn is_completed_status(s: &str) -> bool {
    matches!(s, "completed" | "cancelled")
}

/// Synthesizes two `Message::UpdateTodos` (create the new list + mark completed).
/// Called by chat_stream when it intercepts todowrite, which yields out the returned messages.
pub fn build_update_todos_messages(
    args_str: &str,
    task_id: &str,
    request_id: &str,
) -> Result<Vec<api::Message>> {
    let parsed: Args =
        serde_json::from_str(args_str).map_err(|e| anyhow!("todowrite args parse error: {e}"))?;

    // All todos start as pending (keeping the model's given order); this feeds CreateTodoList.
    let initial_todos: Vec<api::TodoItem> = parsed.todos.iter().map(to_todo_item).collect();
    // Then mark the ids with status=completed/cancelled as done.
    let completed_ids: Vec<String> = parsed
        .todos
        .iter()
        .filter(|t| is_completed_status(&t.status))
        .map(|t| stable_id(&t.content))
        .collect();

    let mut messages = Vec::with_capacity(2);

    messages.push(make_update_todos_message(
        task_id,
        request_id,
        api::message::update_todos::Operation::CreateTodoList(api::CreateTodoList {
            initial_todos,
        }),
    ));

    if !completed_ids.is_empty() {
        messages.push(make_update_todos_message(
            task_id,
            request_id,
            api::message::update_todos::Operation::MarkTodosCompleted(api::MarkTodosCompleted {
                todo_ids: completed_ids,
            }),
        ));
    }

    Ok(messages)
}

fn make_update_todos_message(
    task_id: &str,
    request_id: &str,
    operation: api::message::update_todos::Operation,
) -> api::Message {
    api::Message {
        id: Uuid::new_v4().to_string(),
        task_id: task_id.to_owned(),
        server_message_data: String::new(),
        citations: vec![],
        message: Some(api::message::Message::UpdateTodos(
            api::message::UpdateTodos {
                operation: Some(operation),
            },
        )),
        request_id: request_id.to_owned(),
        timestamp: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intercepted_result_payloads_include_auto_resume_sentinel() {
        let ok = success_result_to_json("todo list updated");
        assert_eq!(ok["_byop_intercepted"], true);
        assert_eq!(ok["status"], "ok");
        let ok_string = serde_json::to_string(&ok).unwrap();
        assert!(ok_string.contains(r#""_byop_intercepted":true"#));

        let err = invalid_arguments_result_to_json("bad args".to_owned(), "{}");
        assert_eq!(err["_byop_intercepted"], true);
        assert_eq!(err["error"], "invalid_arguments");
        assert_eq!(err["tool"], TOOL_NAME);
        let err_string = serde_json::to_string(&err).unwrap();
        assert!(err_string.contains(r#""_byop_intercepted":true"#));
    }
}
