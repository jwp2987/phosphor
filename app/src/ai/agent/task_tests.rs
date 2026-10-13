use warp_multi_agent_api as api;

use super::{ExtractMessagesError, Task};
use crate::test_util::ai_agent_tasks::{
    create_api_subtask, create_api_task, create_message, create_subagent_tool_call_message,
};

/// Creates a Task backed by server data from the given api::Task.
fn create_server_task(api_task: api::Task) -> Task {
    Task::new_restored_root(api_task, std::iter::empty())
}

// =============================================================================
// Tests for Task::splice_messages()
// =============================================================================

#[test]
fn test_splice_messages_happy_path() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
            create_message("m4", task_id),
            create_message("m5", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Extract m2, m3, m4 (middle 3 messages).
    let replacement = vec![create_message("replacement", task_id)];
    let result = task.splice_messages("m2", "m4", 3, replacement);

    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 3);
    assert_eq!(extracted[0].id, "m2");
    assert_eq!(extracted[1].id, "m3");
    assert_eq!(extracted[2].id, "m4");

    // Verify the task now has: m1, replacement, m5.
    let remaining_ids: Vec<_> = task.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["m1", "replacement", "m5"]);
}

#[test]
fn test_splice_messages_single_message() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Extract just m2.
    let replacement = vec![create_message("replacement", task_id)];
    let result = task.splice_messages("m2", "m2", 1, replacement);

    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].id, "m2");

    // Verify the task now has: m1, replacement, m3.
    let remaining_ids: Vec<_> = task.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["m1", "replacement", "m3"]);
}

#[test]
fn test_splice_messages_all_messages() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Extract all messages.
    let replacement = vec![create_message("replacement", task_id)];
    let result = task.splice_messages("m1", "m3", 3, replacement);

    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 3);

    // Verify the task now only has the replacement.
    let remaining_ids: Vec<_> = task.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["replacement"]);
}

#[test]
fn test_splice_messages_empty_replacement() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Extract m2 with no replacement (pure deletion).
    let result = task.splice_messages("m2", "m2", 1, vec![]);

    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].id, "m2");

    // Verify the task now has: m1, m3.
    let remaining_ids: Vec<_> = task.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["m1", "m3"]);
}

#[test]
fn test_splice_messages_multiple_replacements() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Extract m2 and replace with two messages.
    let replacement = vec![create_message("r1", task_id), create_message("r2", task_id)];
    let result = task.splice_messages("m2", "m2", 1, replacement);

    assert!(result.is_ok());

    // Verify the task now has: m1, r1, r2, m3.
    let remaining_ids: Vec<_> = task.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["m1", "r1", "r2", "m3"]);
}

#[test]
fn test_splice_messages_first_message_not_found() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![create_message("m1", task_id), create_message("m2", task_id)],
    );
    let mut task = create_server_task(api_task);

    let result = task.splice_messages("nonexistent", "m2", 1, vec![]);

    assert!(matches!(
        result,
        Err(ExtractMessagesError::FirstMessageNotFound(id)) if id == "nonexistent"
    ));
}

#[test]
fn test_splice_messages_last_message_not_found() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![create_message("m1", task_id), create_message("m2", task_id)],
    );
    let mut task = create_server_task(api_task);

    let result = task.splice_messages("m1", "nonexistent", 1, vec![]);

    assert!(matches!(
        result,
        Err(ExtractMessagesError::LastMessageNotFound(id)) if id == "nonexistent"
    ));
}

#[test]
fn test_splice_messages_invalid_range() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // first_message_id appears after last_message_id.
    let result = task.splice_messages("m3", "m1", 3, vec![]);

    assert!(matches!(result, Err(ExtractMessagesError::InvalidRange)));
}

#[test]
fn test_splice_messages_checksum_mismatch_too_few() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Claim there are 5 messages when there are only 3 in the range.
    let result = task.splice_messages("m1", "m3", 5, vec![]);

    assert!(matches!(
        result,
        Err(ExtractMessagesError::ChecksumMismatch {
            expected: 5,
            actual: 3
        })
    ));
}

#[test]
fn test_splice_messages_checksum_mismatch_too_many() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Claim there is 1 message when there are 3 in the range.
    let result = task.splice_messages("m1", "m3", 1, vec![]);

    assert!(matches!(
        result,
        Err(ExtractMessagesError::ChecksumMismatch {
            expected: 1,
            actual: 3
        })
    ));
}

#[test]
fn test_splice_messages_optimistic_task_not_initialized() {
    let mut task = Task::new_optimistic_root();

    let result = task.splice_messages("m1", "m2", 2, vec![]);

    assert!(matches!(
        result,
        Err(ExtractMessagesError::TaskNotInitialized)
    ));
}

#[test]
fn test_splice_messages_from_beginning() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
            create_message("m4", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Extract from the beginning.
    let replacement = vec![create_message("replacement", task_id)];
    let result = task.splice_messages("m1", "m2", 2, replacement);

    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 2);

    // Verify the task now has: replacement, m3, m4.
    let remaining_ids: Vec<_> = task.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["replacement", "m3", "m4"]);
}

#[test]
fn test_splice_messages_from_end() {
    let task_id = "task1";
    let api_task = create_api_task(
        task_id,
        vec![
            create_message("m1", task_id),
            create_message("m2", task_id),
            create_message("m3", task_id),
            create_message("m4", task_id),
        ],
    );
    let mut task = create_server_task(api_task);

    // Extract from the end.
    let replacement = vec![create_message("replacement", task_id)];
    let result = task.splice_messages("m3", "m4", 2, replacement);

    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 2);

    // Verify the task now has: m1, m2, replacement.
    let remaining_ids: Vec<_> = task.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["m1", "m2", "replacement"]);
}

// =============================================================================
// Tests for Task::new_moved_messages_subtask()
// =============================================================================

#[test]
fn test_new_moved_messages_subtask_basic() {
    let parent_id = "parent";
    let subtask_id = "subtask";

    // Create parent task with a subagent call referencing the subtask.
    let parent_api_task = create_api_task(
        parent_id,
        vec![
            create_message("m1", parent_id),
            create_subagent_tool_call_message("subagent_call", parent_id, subtask_id, None),
            create_message("m2", parent_id),
        ],
    );

    // Create the subtask api::Task with some messages.
    let subtask_api_task = create_api_task(
        subtask_id,
        vec![
            create_message("s1", subtask_id),
            create_message("s2", subtask_id),
        ],
    );

    let subtask = Task::new_moved_messages_subtask(subtask_api_task, &parent_api_task);

    assert_eq!(subtask.id().to_string(), subtask_id);
    assert!(subtask.exchanges().next().is_none()); // No exchanges.
    assert_eq!(subtask.messages().count(), 2);

    // Should have subagent_params extracted from parent.
    let subagent_params = subtask.subagent_params();
    assert!(subagent_params.is_some());
    assert_eq!(
        subagent_params.unwrap().tool_call_id,
        "subagent_call_tool_call"
    );
}

#[test]
fn test_new_moved_messages_subtask_with_summarization_metadata() {
    let parent_id = "parent";
    let subtask_id = "subtask";

    // Create parent task with a summarization subagent call.
    let parent_api_task = create_api_task(
        parent_id,
        vec![create_subagent_tool_call_message(
            "summary_call",
            parent_id,
            subtask_id,
            Some(api::message::tool_call::subagent::Metadata::Summarization(
                (),
            )),
        )],
    );

    let subtask_api_task = create_api_task(subtask_id, vec![create_message("s1", subtask_id)]);

    let subtask = Task::new_moved_messages_subtask(subtask_api_task, &parent_api_task);

    // Check that subagent_params has the summarization metadata.
    let subagent_params = subtask.subagent_params();
    assert!(subagent_params.is_some());

    let call = &subagent_params.unwrap().call;
    assert!(matches!(
        call.metadata,
        Some(api::message::tool_call::subagent::Metadata::Summarization(
            _
        ))
    ));
}

#[test]
fn test_new_moved_messages_subtask_no_matching_subagent_call() {
    let parent_id = "parent";
    let subtask_id = "subtask";

    // Parent task has no subagent call to this subtask.
    let parent_api_task = create_api_task(
        parent_id,
        vec![
            create_message("m1", parent_id),
            // Subagent call references a different task.
            create_subagent_tool_call_message("other_call", parent_id, "other_task", None),
        ],
    );

    let subtask_api_task = create_api_task(subtask_id, vec![create_message("s1", subtask_id)]);

    let subtask = Task::new_moved_messages_subtask(subtask_api_task, &parent_api_task);

    // No subagent_params since no matching call was found.
    assert!(subtask.subagent_params().is_none());
}

#[test]
fn test_new_moved_messages_subtask_preserves_messages() {
    let parent_id = "parent";
    let subtask_id = "subtask";

    let parent_api_task = create_api_task(
        parent_id,
        vec![create_subagent_tool_call_message(
            "call", parent_id, subtask_id, None,
        )],
    );

    // Subtask with multiple messages.
    let subtask_api_task = create_api_task(
        subtask_id,
        vec![
            create_message("s1", subtask_id),
            create_message("s2", subtask_id),
            create_message("s3", subtask_id),
        ],
    );

    let subtask = Task::new_moved_messages_subtask(subtask_api_task, &parent_api_task);

    // All messages should be preserved.
    let message_ids: Vec<_> = subtask.messages().map(|m| m.id.as_str()).collect();
    assert_eq!(message_ids, vec!["s1", "s2", "s3"]);
}

// =============================================================================
// Tests for Warp docs subagent classification
// =============================================================================

#[test]
fn test_is_warp_documentation_search_subagent() {
    let parent_id = "parent";
    let subtask_id = "subtask";
    let parent_api_task = create_api_task(
        parent_id,
        vec![create_subagent_tool_call_message(
            "docs_call",
            parent_id,
            subtask_id,
            Some(api::message::tool_call::subagent::Metadata::WarpDocumentationSearch(())),
        )],
    );
    let subtask_api_task = create_api_subtask(subtask_id, parent_id, vec![]);
    let subtask = Task::new_restored_subtask(subtask_api_task, &parent_api_task, vec![]);

    assert!(subtask.is_warp_documentation_search_subagent());
    assert!(!subtask.is_conversation_search_subagent());
}

// =============================================================================
// Tests for incrementally converted streamed text (#849)
// =============================================================================

mod streamed_text {
    use std::collections::HashSet;

    use ai::skills::SkillPathOrigin;
    use chrono::Local;
    use prost_types::FieldMask;
    use warp_multi_agent_api as api;

    use crate::ai::agent::util::parse_markdown_into_text_and_code_sections;
    use crate::ai::agent::{
        AIAgentExchange, AIAgentExchangeId, AIAgentOutput, AIAgentOutputMessageType,
        AIAgentOutputStatus, AIAgentTextSection, MessageId, Shared,
    };
    use crate::ai::llms::LLMId;
    use crate::test_util::ai_agent_tasks::{create_api_task, create_message};

    use super::super::Task;

    const TASK_ID: &str = "task1";

    fn streaming_exchange() -> AIAgentExchange {
        AIAgentExchange {
            id: AIAgentExchangeId::new(),
            input: vec![],
            output_status: AIAgentOutputStatus::Streaming {
                output: Some(Shared::new(AIAgentOutput::default())),
            },
            added_message_ids: HashSet::new(),
            start_time: Local::now(),
            finish_time: None,
            time_to_first_token_ms: None,
            working_directory: None,
            model_id: LLMId::from(""),
            request_cost: None,
            coding_model_id: LLMId::from(""),
            cli_agent_model_id: LLMId::from(""),
            computer_use_model_id: LLMId::from(""),
            response_initiator: None,
        }
    }

    fn text_message(id: &str, text: &str) -> api::Message {
        let mut message = create_message(id, TASK_ID);
        message.message = Some(api::message::Message::AgentOutput(
            api::message::AgentOutput {
                text: text.to_string(),
            },
        ));
        message
    }

    fn text_mask() -> FieldMask {
        FieldMask {
            paths: vec!["agent_output.text".to_string()],
        }
    }

    fn stored_text(task: &Task, id: &str) -> String {
        task.messages()
            .find(|message| message.id == id)
            .and_then(|message| match &message.message {
                Some(api::message::Message::AgentOutput(output)) => Some(output.text.clone()),
                _ => None,
            })
            .expect("stored agent output message")
    }

    fn shown_sections(
        task: &Task,
        exchange_id: AIAgentExchangeId,
        id: &str,
    ) -> Vec<AIAgentTextSection> {
        let exchange = task.exchange(exchange_id).expect("exchange exists");
        let AIAgentOutputStatus::Streaming {
            output: Some(output),
        } = &exchange.output_status
        else {
            panic!("exchange is streaming");
        };
        let output = output.get();
        output
            .messages
            .iter()
            .find(|message| message.id.0 == id)
            .and_then(|message| match &message.message {
                AIAgentOutputMessageType::Text(text) => Some(text.sections.clone()),
                _ => None,
            })
            .expect("text output message")
    }

    /// The sections shown for `id` equal a full conversion of its stored text, parsed lines
    /// included (section equality alone compares only their raw text and links).
    fn assert_shows_full_conversion(task: &Task, exchange_id: AIAgentExchangeId, id: &str) {
        let text = stored_text(task, id);
        let shown = shown_sections(task, exchange_id, id);
        let expected = parse_markdown_into_text_and_code_sections(&text);
        assert_eq!(shown, expected, "{id}: {text:?}");
        for (shown, expected) in shown.iter().zip(&expected) {
            if let (
                AIAgentTextSection::PlainText { text: shown },
                AIAgentTextSection::PlainText { text: expected },
            ) = (shown, expected)
            {
                assert_eq!(
                    shown.formatted_text_arc().as_deref(),
                    expected.formatted_text_arc().as_deref()
                );
            }
        }
    }

    fn append_in_chunks(
        task: &mut Task,
        exchange_id: AIAgentExchangeId,
        id: &str,
        text: &str,
        chunk_len: usize,
    ) {
        let mut start = 0;
        while start < text.len() {
            let mut end = (start + chunk_len).min(text.len());
            while !text.is_char_boundary(end) {
                end += 1;
            }
            let before = stored_text(task, id);
            task.append_to_message_content(
                text_message(id, &text[start..end]),
                exchange_id,
                None,
                None,
                &SkillPathOrigin::Local,
                text_mask(),
            )
            .expect("append applies");
            assert_eq!(
                stored_text(task, id),
                format!("{before}{}", &text[start..end])
            );
            assert_shows_full_conversion(task, exchange_id, id);
            start = end;
        }
    }

    #[test]
    fn streamed_text_matches_full_conversion_through_upsert_remove_and_finalize() {
        let exchange = streaming_exchange();
        let exchange_id = exchange.id;
        let mut task = Task::new_restored_root(
            create_api_task(
                TASK_ID,
                vec![text_message("m1", ""), text_message("m2", "")],
            ),
            std::iter::once(exchange),
        );

        let answer = "Intro paragraph with `code`.\n\n```rust\nfn main() {}\n```\n\n\
                      | A | B |\n| --- | --- |\n| 1 | 2 |\n\n![img](a.png)\nafter\n";
        append_in_chunks(&mut task, exchange_id, "m1", answer, 7);
        assert!(task.streaming_text.contains_key("m1"));

        // A non-append update replaces the text mid-stream: the incremental state for the old
        // text must not be applied to the new one.
        task.upsert_message(
            text_message("m1", "Replaced text.\n\n```\nopen code"),
            exchange_id,
            None,
            None,
            &SkillPathOrigin::Local,
            text_mask(),
            false,
        )
        .expect("upsert applies");
        assert!(!task.streaming_text.contains_key("m1"));
        assert_shows_full_conversion(&task, exchange_id, "m1");
        append_in_chunks(&mut task, exchange_id, "m1", " more\n```\nand prose\n", 5);

        append_in_chunks(
            &mut task,
            exchange_id,
            "m2",
            "second message\n\nwith text\n",
            4,
        );
        let mut removed = HashSet::new();
        removed.insert(MessageId::new("m2".to_string()));
        task.remove_messages(&removed);
        assert!(!task.streaming_text.contains_key("m2"));

        task.finalize_streamed_text();
        assert!(task.streaming_text.is_empty());
        assert_shows_full_conversion(&task, exchange_id, "m1");
    }

    /// A message with every field set, so a fast path that dropped or altered one would show.
    fn full_message(id: &str, body: api::message::Message) -> api::Message {
        api::Message {
            id: id.to_string(),
            task_id: format!("{id}-task"),
            server_message_data: format!("{id}-server-data"),
            citations: vec![],
            fetched_memories: vec![],
            message: Some(body),
            request_id: format!("{id}-request"),
            timestamp: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 7,
            }),
        }
    }

    fn output(text: &str) -> api::message::Message {
        api::message::Message::AgentOutput(api::message::AgentOutput {
            text: text.to_string(),
        })
    }

    fn reasoning(text: &str, seconds: i64) -> api::message::Message {
        api::message::Message::AgentReasoning(api::message::AgentReasoning {
            reasoning: text.to_string(),
            finished_duration: Some(prost_types::Duration { seconds, nanos: 3 }),
        })
    }

    #[test]
    fn in_place_text_append_matches_the_field_mask_append() {
        let cases = [
            ("agent_output.text", output("stored "), output("chunk")),
            ("agent_output.text", output(""), output("chunk")),
            ("agent_output.text", output("stored"), output("")),
            ("agent_output.text", output(""), output("")),
            (
                "agent_reasoning.reasoning",
                reasoning("thinking ", 5),
                reasoning("more", 9),
            ),
            (
                "agent_reasoning.reasoning",
                reasoning("", 5),
                reasoning("", 9),
            ),
        ];
        for (path, stored_body, patch_body) in cases {
            let stored = full_message("stored", stored_body);
            let patch = full_message("patch", patch_body);
            let mask = FieldMask {
                paths: vec![path.to_string()],
            };
            assert!(super::super::is_in_place_text_append(
                &stored, &patch, &mask
            ));

            let general = field_mask::FieldMaskOperation::append(
                &api::MESSAGE_DESCRIPTOR,
                &stored,
                &patch,
                mask,
            )
            .apply()
            .expect("general append applies");
            let mut in_place = stored.clone();
            super::super::push_appended_text(&mut in_place, &patch);
            assert_eq!(in_place, general, "{path}");
        }
    }

    #[test]
    fn other_masks_and_mismatched_messages_take_the_general_path() {
        let stored = full_message("stored", output("a"));
        for (path, patch) in [
            ("agent_reasoning.reasoning", full_message("p", output("b"))),
            ("agent_output.text", full_message("p", reasoning("b", 1))),
            ("server_message_data", full_message("p", output("b"))),
        ] {
            let mask = FieldMask {
                paths: vec![path.to_string()],
            };
            assert!(!super::super::is_in_place_text_append(
                &stored, &patch, &mask
            ));
        }
        let two_paths = FieldMask {
            paths: vec![
                "agent_output.text".to_string(),
                "server_message_data".to_string(),
            ],
        };
        assert!(!super::super::is_in_place_text_append(
            &stored,
            &full_message("p", output("b")),
            &two_paths
        ));
    }

    #[test]
    fn a_failed_in_place_append_leaves_the_stored_message_unchanged() {
        let exchange = streaming_exchange();
        let mut task = Task::new_restored_root(
            create_api_task(TASK_ID, vec![text_message("m1", "kept")]),
            std::iter::once(exchange),
        );
        let result = task.append_to_message_content(
            text_message("m1", " dropped"),
            AIAgentExchangeId::new(),
            None,
            None,
            &SkillPathOrigin::Local,
            text_mask(),
        );
        assert!(result.is_err());
        assert_eq!(stored_text(&task, "m1"), "kept");
        assert!(task.streaming_text.is_empty());
    }
}
