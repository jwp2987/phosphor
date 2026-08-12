use ai::agent::action::AskUserQuestionType;
use ai::skills::SkillPathOrigin;
use warp_multi_agent_api as api;

use super::{
    convert_api_question, ConversionParams, ConvertAPIMessageToClientOutputMessage,
    MaybeAIAgentOutputMessage,
};
use crate::ai::agent::task::TaskId;
use crate::ai::agent::{AIAgentActionType, AIAgentOutputMessageType};

fn file_artifact_created_message(filepath: &str, description: &str) -> api::Message {
    api::Message {
        fetched_memories: vec![],
        id: "message-id".to_string(),
        task_id: "task-id".to_string(),
        server_message_data: String::new(),
        citations: vec![],
        message: Some(api::message::Message::ArtifactEvent(
            api::message::ArtifactEvent {
                event: Some(api::message::artifact_event::Event::Created(
                    api::message::artifact_event::ArtifactCreated {
                        artifact: Some(
                            api::message::artifact_event::artifact_created::Artifact::File(
                                api::message::artifact_event::FileArtifact {
                                    artifact_uid: "artifact-uid".to_string(),
                                    filepath: filepath.to_string(),
                                    mime_type: "text/plain".to_string(),
                                    size_bytes: 42,
                                    description: description.to_string(),
                                },
                            ),
                        ),
                    },
                )),
            },
        )),
        request_id: "request-id".to_string(),
        timestamp: None,
    }
}

fn build_multiple_choice_question(
    recommended_option_index: i32,
) -> api::ask_user_question::Question {
    api::ask_user_question::Question {
        question_id: "q1".to_string(),
        question: "Which option should we prefer?".to_string(),
        question_type: Some(
            api::ask_user_question::question::QuestionType::MultipleChoice(
                api::ask_user_question::MultipleChoice {
                    is_multiselect: false,
                    options: vec![
                        api::ask_user_question::Option {
                            label: "First".to_string(),
                        },
                        api::ask_user_question::Option {
                            label: "Second".to_string(),
                        },
                    ],
                    recommended_option_index,
                    supports_other: false,
                },
            ),
        ),
    }
}

#[test]
fn convert_api_question_treats_negative_recommended_index_as_no_recommendation() {
    let converted = convert_api_question(build_multiple_choice_question(-1))
        .expect("multiple choice questions should convert");

    let AskUserQuestionType::MultipleChoice { options, .. } = converted.question_type;
    assert_eq!(options.len(), 2);
    assert!(options.iter().all(|option| !option.recommended));
}

#[test]
fn convert_api_question_uses_zero_based_recommended_index_when_present() {
    let converted = convert_api_question(build_multiple_choice_question(0))
        .expect("multiple choice questions should convert");

    let AskUserQuestionType::MultipleChoice { options, .. } = converted.question_type;
    assert_eq!(options.len(), 2);
    assert!(options[0].recommended);
    assert!(!options[1].recommended);
}

fn extract_file_artifact_created(
    output: MaybeAIAgentOutputMessage,
) -> (String, String, Option<String>, i64) {
    let MaybeAIAgentOutputMessage::Message(output_message) = output else {
        panic!("expected output message");
    };
    let AIAgentOutputMessageType::ArtifactCreated(artifact) = output_message.message else {
        panic!("expected artifact created output message");
    };
    let crate::ai::agent::ArtifactCreatedData::File {
        filepath,
        filename,
        description,
        size_bytes,
        ..
    } = artifact
    else {
        panic!("expected file artifact created output message");
    };
    (filepath, filename, description, size_bytes)
}

#[test]
fn converts_file_artifact_created_message_with_filename() {
    let task_id = TaskId::new("task-id".to_string());
    let message =
        file_artifact_created_message("outputs/report.txt", "Build output for the latest run");

    let output = message
        .to_client_output_message(ConversionParams {
            task_id: &task_id,
            current_todo_list: None,
            active_code_review: None,
            skill_path_origin: &SkillPathOrigin::Local,
        })
        .expect("conversion should succeed");

    let (filepath, filename, description, size_bytes) = extract_file_artifact_created(output);

    assert_eq!(filepath, "outputs/report.txt");
    assert_eq!(filename, "report.txt");
    assert_eq!(
        description.as_deref(),
        Some("Build output for the latest run")
    );
    assert_eq!(size_bytes, 42);
}

#[test]
fn transfer_control_tool_call_converts_to_action_message() {
    let task_id = TaskId::new("task".to_string());
    let reason = "Please finish the interactive flow".to_string();
    let message = api::Message {
        fetched_memories: vec![],
        id: "message".to_string(),
        task_id: "task".to_string(),
        server_message_data: String::new(),
        citations: vec![],
        message: Some(api::message::Message::ToolCall(api::message::ToolCall {
            tool_call_id: "tool_call".to_string(),
            tool: Some(
                api::message::tool_call::Tool::TransferShellCommandControlToUser(
                    api::message::tool_call::TransferShellCommandControlToUser {
                        reason: reason.clone(),
                    },
                ),
            ),
        })),
        request_id: "req".to_string(),
        timestamp: None,
    };

    let converted = message
        .to_client_output_message(ConversionParams {
            task_id: &task_id,
            current_todo_list: None,
            active_code_review: None,
            skill_path_origin: &SkillPathOrigin::Local,
        })
        .expect("transfer-control conversion should succeed");

    match converted {
        MaybeAIAgentOutputMessage::Message(output) => match output.message {
            AIAgentOutputMessageType::Action(action) => {
                assert_eq!(action.task_id, task_id);
                assert_eq!(
                    action.action,
                    AIAgentActionType::TransferShellCommandControlToUser { reason }
                );
                assert!(action.requires_result);
            }
            other => panic!("Expected action message, got {other:?}"),
        },
        MaybeAIAgentOutputMessage::NoClientRepresentation => {
            panic!("Expected transfer-control tool call to produce a client action")
        }
    }
}

fn tool_call_result_message(server_message_data: &str) -> api::Message {
    api::Message {
        fetched_memories: vec![],
        id: "message".to_string(),
        task_id: "task".to_string(),
        server_message_data: server_message_data.to_string(),
        citations: vec![],
        message: Some(api::message::Message::ToolCallResult(
            api::message::ToolCallResult {
                tool_call_id: "tool_call".to_string(),
                context: None,
                result: None,
            },
        )),
        request_id: "req".to_string(),
        timestamp: None,
    }
}

/// Extracts the `(tool, detail)` pair from an `AIAgentOutputMessageType::RejectedToolCall`,
/// panicking if the conversion produced anything else. Used to assert the new failure
/// representation's actual shape, not just that some text rendered.
fn extract_rejected_tool_call(output: MaybeAIAgentOutputMessage) -> (Option<String>, String) {
    let MaybeAIAgentOutputMessage::Message(output_message) = output else {
        panic!("expected an output message, got NoClientRepresentation");
    };
    match output_message.message {
        AIAgentOutputMessageType::RejectedToolCall { tool, detail } => (tool, detail),
        other => panic!("expected a RejectedToolCall output message, got {other:?}"),
    }
}

/// A `from_args` parse failure previously vanished with no trace: `chat_stream` emitted a
/// carrier `ToolCall(tool: None)` plus this synthetic `ToolCallResult(result: None)` — both of
/// which mapped to `NoClientRepresentation`, so the model got the retry signal but the user saw
/// nothing at all (the same silent-failure class as `edit.rs`'s `summary` bug). This is the
/// fix: the synthetic `invalid_arguments` marker must now produce a visible
/// `RejectedToolCall` message — a genuine failure element, not an ordinary text paragraph.
#[test]
fn invalid_arguments_tool_call_result_becomes_a_rejected_tool_call_message() {
    let task_id = TaskId::new("task".to_string());
    // Mirrors the payload chat_stream::parse_incoming_tool_call's `Err` arm builds when
    // `from_args` rejects a malformed call.
    let payload = serde_json::json!({
        "error": "invalid_arguments",
        "detail": "missing field `operations`",
        "tool": "apply_file_diffs",
        "received_args": "{}",
        "hint": "Re-emit the tool call with corrected types / required fields.",
    })
    .to_string();
    let message = tool_call_result_message(&payload);

    let converted = message
        .to_client_output_message(ConversionParams {
            task_id: &task_id,
            current_todo_list: None,
            active_code_review: None,
            skill_path_origin: &SkillPathOrigin::Local,
        })
        .expect("conversion should succeed");

    let (tool, detail) = extract_rejected_tool_call(converted);
    assert_eq!(
        tool.as_deref(),
        Some("apply_file_diffs"),
        "the tool name must be carried structurally, not just embedded in prose"
    );
    assert!(detail.contains("missing field `operations`"), "{detail}");

    // The shared rendering text (what GUI/TUI/copy/SDK all actually show the user) must
    // still say the call was rejected, not just relay the raw detail.
    let rendered = crate::ai::agent::rejected_tool_call_text(tool.as_deref(), &detail);
    assert!(
        rendered.to_lowercase().contains("rejected"),
        "the message must say the call was rejected, not just relay the raw detail: {rendered}"
    );
}

/// A `tool` field missing from the marker (should never happen in practice, but the parser
/// must not panic on it) must still produce a `RejectedToolCall` with `tool: None` — the
/// renderers fall back to a tool-agnostic phrasing rather than showing an empty name.
#[test]
fn invalid_arguments_without_a_tool_name_still_produces_a_rejected_tool_call() {
    let task_id = TaskId::new("task".to_string());
    let payload = serde_json::json!({
        "error": "invalid_arguments",
        "detail": "missing field `operations`",
    })
    .to_string();
    let message = tool_call_result_message(&payload);

    let converted = message
        .to_client_output_message(ConversionParams {
            task_id: &task_id,
            current_todo_list: None,
            active_code_review: None,
            skill_path_origin: &SkillPathOrigin::Local,
        })
        .expect("conversion should succeed");

    let (tool, detail) = extract_rejected_tool_call(converted);
    assert_eq!(tool, None);
    assert!(detail.contains("missing field `operations`"), "{detail}");
}

/// A genuinely different `result: None` payload — the cancellation marker
/// `BlocklistAIController::byop_synthetic_cancellation_message` writes for a
/// user-interrupted command — must keep falling through to `NoClientRepresentation`. Only the
/// specific `invalid_arguments` marker should ever render.
#[test]
fn unrelated_result_none_payload_stays_invisible() {
    let task_id = TaskId::new("task".to_string());
    let payload = serde_json::json!({
        "status": "cancelled",
        "reason": "interrupted_by_user",
    })
    .to_string();
    let message = tool_call_result_message(&payload);

    let converted = message
        .to_client_output_message(ConversionParams {
            task_id: &task_id,
            current_todo_list: None,
            active_code_review: None,
            skill_path_origin: &SkillPathOrigin::Local,
        })
        .expect("conversion should succeed");

    assert!(
        matches!(converted, MaybeAIAgentOutputMessage::NoClientRepresentation),
        "an unrelated result:None payload must not spuriously render"
    );
}

/// Genuinely malformed `server_message_data` (not JSON at all) must not panic the conversion
/// pipeline — it should just fall back to no client representation, same as any other
/// `ToolCallResult` the client doesn't need to render.
#[test]
fn non_json_server_message_data_does_not_panic_and_stays_invisible() {
    let task_id = TaskId::new("task".to_string());
    let message = tool_call_result_message("not json at all");

    let converted = message
        .to_client_output_message(ConversionParams {
            task_id: &task_id,
            current_todo_list: None,
            active_code_review: None,
            skill_path_origin: &SkillPathOrigin::Local,
        })
        .expect("conversion should succeed");

    assert!(matches!(
        converted,
        MaybeAIAgentOutputMessage::NoClientRepresentation
    ));
}
