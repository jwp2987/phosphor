use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone, Utc};
use itertools::Itertools;
use warpui::{App, EntityId};

use crate::{
    ai::{
        agent::{
            api::ServerConversationToken,
            conversation::{AIConversationId, ConversationStatus, TodoStatus},
            todos::AIAgentTodoList,
            AIAgentExchange, AIAgentExchangeId, AIAgentInput, AIAgentOutputStatus, AIAgentTodo,
            AIAgentTodoId, FinishedAIAgentOutput, RenderableAIError, Shared,
            TransientNetworkErrorKind, UserQueryMode,
        },
        ambient_agents::{
            conversation_output_status_from_conversation, AmbientAgentTaskId,
            AmbientConversationStatus,
        },
        blocklist::{controller::RequestInput, ResponseStreamId},
        byop_readiness::{RepairRecord, RepairSource, RepairState, RepairStateStatus, ToolCallKey},
        llms::LLMId,
    },
    input_suggestions::HistoryInputSuggestion,
    persistence::{
        model::{
            AgentConversation, AgentConversationData, AgentConversationRecord,
            PersistedAutoexecuteMode,
        },
        ModelEvent,
    },
    terminal::model::session::SessionId,
    test_util::settings::initialize_settings_for_tests,
    GlobalResourceHandles, GlobalResourceHandlesProvider,
};
use warp_multi_agent_api as api;

use super::{
    convert_persisted_conversation_to_ai_conversation_with_metadata, AIConversationMetadata,
    AIQueryHistoryOutputStatus, BeginConversationRenameError, BlocklistAIHistoryModel,
    ForkConversationError, PersistedAIInput, PersistedAIInputType,
};

fn initialize_history_model_test_app(app: &mut App) {
    initialize_settings_for_tests(app);
    let global_resource_handles = GlobalResourceHandles::mock(app);
    app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));
}

/// Helper function to create a PersistedAIInput for testing
fn create_persisted_query(
    query_text: &str,
    conversation_id: AIConversationId,
    start_time: DateTime<Local>,
) -> PersistedAIInput {
    PersistedAIInput {
        exchange_id: AIAgentExchangeId::new(),
        conversation_id,
        start_ts: start_time,
        inputs: vec![PersistedAIInputType::Query {
            text: query_text.to_string(),
            context: Default::default(),
            referenced_attachments: Default::default(),
        }],
        output_status: AIQueryHistoryOutputStatus::Completed,
        working_directory: None,
        model_id: LLMId::from("test-model"),
        coding_model_id: LLMId::from("test-coding-model"),
    }
}

/// Helper function to create an AIAgentExchange for testing
fn create_exchange_with_query(
    query_text: &str,
    start_time: DateTime<Local>,
    working_directory: Option<String>,
) -> AIAgentExchange {
    AIAgentExchange {
        id: AIAgentExchangeId::new(),
        input: vec![AIAgentInput::UserQuery {
            query: query_text.to_string(),
            context: Default::default(),
            static_query_type: None,
            referenced_attachments: Default::default(),
            user_query_mode: UserQueryMode::default(),
            running_command: None,
            intended_agent: None,
        }],
        output_status: AIAgentOutputStatus::Finished {
            finished_output: FinishedAIAgentOutput::Success {
                output: Shared::new(Default::default()),
            },
        },
        added_message_ids: HashSet::new(),
        start_time,
        finish_time: None,
        time_to_first_token_ms: None,
        working_directory,
        model_id: LLMId::from("test-model"),
        request_cost: None,
        coding_model_id: LLMId::from("test-coding-model"),
        cli_agent_model_id: LLMId::from("test-cli-agent-model"),
        computer_use_model_id: LLMId::from("test-computer-use-model"),
        response_initiator: None,
    }
}

fn byop_test_task(task_id: &str, messages: Vec<api::Message>) -> api::Task {
    api::Task {
        id: task_id.to_owned(),
        messages,
        dependencies: None,
        description: String::new(),
        summary: String::new(),
        server_data: String::new(),
    }
}

fn empty_agent_conversation_data_for_test() -> AgentConversationData {
    AgentConversationData {
        is_remote_child: false,
        server_conversation_token: None,
        conversation_usage_metadata: None,
        reverted_action_ids: None,
        forked_from_server_conversation_token: None,
        artifacts_json: None,
        parent_agent_id: None,
        agent_name: None,
        parent_conversation_id: None,
        run_id: None,
        autoexecute_override: None,
        last_event_sequence: None,
        compaction_state_json: None,
        byop_repair_state_json: None,
        cli_subagent_block_snapshots_json: None,
    }
}

fn timestamp_from_local_time(time: DateTime<Local>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: time.timestamp(),
        nanos: time.timestamp_subsec_nanos() as i32,
    }
}

fn persisted_agent_conversation_for_test(
    conversation_id: AIConversationId,
    last_modified_at: chrono::NaiveDateTime,
    messages: Vec<api::Message>,
) -> AgentConversation {
    AgentConversation {
        conversation: AgentConversationRecord {
            id: 1,
            conversation_id: conversation_id.to_string(),
            conversation_data: serde_json::to_string(&empty_agent_conversation_data_for_test())
                .expect("test conversation data should serialize"),
            last_modified_at,
            summary: None,
        },
        tasks: vec![byop_test_task("root-task", messages)],
    }
}

fn byop_user_query_message(
    message_id: &str,
    request_id: &str,
    query: &str,
    current_time: Option<DateTime<Local>>,
) -> api::Message {
    api::Message {
        id: message_id.to_owned(),
        task_id: "root-task".to_owned(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::UserQuery(api::message::UserQuery {
            query: query.to_owned(),
            context: current_time.map(|time| api::InputContext {
                current_time: Some(timestamp_from_local_time(time)),
                ..Default::default()
            }),
            referenced_attachments: Default::default(),
            mode: None,
            intended_agent: Default::default(),
        })),
        request_id: request_id.to_owned(),
        timestamp: None,
    }
}

fn byop_tool_call_message(task_id: &str, message_id: &str, call_id: &str) -> api::Message {
    api::Message {
        id: message_id.to_owned(),
        task_id: task_id.to_owned(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::ToolCall(api::message::ToolCall {
            tool_call_id: call_id.to_owned(),
            tool: None,
        })),
        request_id: "request-1".to_owned(),
        timestamp: None,
    }
}

fn byop_tool_result_message(task_id: &str, message_id: &str, call_id: &str) -> api::Message {
    api::Message {
        id: message_id.to_owned(),
        task_id: task_id.to_owned(),
        server_message_data: "{}".to_owned(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::ToolCallResult(
            api::message::ToolCallResult {
                tool_call_id: call_id.to_owned(),
                context: None,
                result: None,
            },
        )),
        request_id: "request-1".to_owned(),
        timestamp: None,
    }
}

#[test]
fn persisted_conversation_uses_record_timestamp_when_restored_messages_have_no_time() {
    let conversation_id = AIConversationId::new();
    let fallback_time = Utc
        .with_ymd_and_hms(2026, 7, 2, 7, 13, 11)
        .unwrap()
        .naive_utc();
    let persisted_conversation = persisted_agent_conversation_for_test(
        conversation_id,
        fallback_time,
        vec![byop_user_query_message(
            "user-query-1",
            "request-1",
            "restore me",
            None,
        )],
    );

    let restored =
        convert_persisted_conversation_to_ai_conversation_with_metadata(persisted_conversation)
            .expect("persisted conversation should restore");

    let restored_time = restored
        .last_modified_at()
        .expect("restored conversation should have an exchange timestamp");
    assert_eq!(restored_time, Local.from_utc_datetime(&fallback_time));
    assert_ne!(restored_time, Local.timestamp_opt(0, 0).single().unwrap());
}

#[test]
fn persisted_conversation_keeps_restored_message_time_when_present() {
    let conversation_id = AIConversationId::new();
    let fallback_time = Utc
        .with_ymd_and_hms(2026, 7, 2, 7, 13, 11)
        .unwrap()
        .naive_utc();
    let message_time = Local.with_ymd_and_hms(2026, 7, 2, 16, 42, 0).unwrap();
    let persisted_conversation = persisted_agent_conversation_for_test(
        conversation_id,
        fallback_time,
        vec![byop_user_query_message(
            "user-query-1",
            "request-1",
            "restore me",
            Some(message_time),
        )],
    );

    let restored =
        convert_persisted_conversation_to_ai_conversation_with_metadata(persisted_conversation)
            .expect("persisted conversation should restore");

    assert_eq!(restored.last_modified_at(), Some(message_time));
}

#[test]
fn byop_fork_repair_records_cover_tool_results_omitted_by_fork() {
    let source_tasks = vec![byop_test_task(
        "source-task",
        vec![
            byop_tool_call_message("source-task", "assistant-1", "call-a"),
            byop_tool_call_message("source-task", "assistant-2", "call-b"),
            byop_tool_result_message("source-task", "result-a", "call-a"),
            byop_tool_result_message("source-task", "result-b", "call-b"),
        ],
    )];
    let forked_tasks = vec![byop_test_task(
        "fork-task",
        vec![
            byop_tool_call_message("fork-task", "assistant-1", "call-a"),
            byop_tool_call_message("fork-task", "assistant-2", "call-b"),
            byop_tool_result_message("fork-task", "result-a", "call-a"),
        ],
    )];

    let records = super::collect_byop_fork_repair_records(
        &source_tasks,
        &forked_tasks,
        &RepairStateStatus::default(),
        Some("exchange-1".to_owned()),
    );

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source, RepairSource::ForkedHistory);
    assert_eq!(
        records[0].key,
        ToolCallKey::new("fork-task", "assistant-1", "call-b")
    );
    assert_eq!(records[0].exchange_id.as_deref(), Some("exchange-1"));
}

#[test]
fn byop_fork_repair_records_match_duplicate_call_ids_by_assistant_message() {
    let source_tasks = vec![byop_test_task(
        "source-task",
        vec![
            byop_tool_call_message("source-task", "assistant-1", "call-a"),
            byop_tool_result_message("source-task", "result-a-1", "call-a"),
            byop_tool_call_message("source-task", "assistant-2", "call-a"),
            byop_tool_result_message("source-task", "result-a-2", "call-a"),
        ],
    )];
    let forked_tasks = vec![byop_test_task(
        "fork-task",
        vec![
            byop_tool_call_message("fork-task", "assistant-1", "call-a"),
            byop_tool_result_message("fork-task", "result-a-1", "call-a"),
            byop_tool_call_message("fork-task", "assistant-2", "call-a"),
        ],
    )];

    let records = super::collect_byop_fork_repair_records(
        &source_tasks,
        &forked_tasks,
        &RepairStateStatus::default(),
        None,
    );

    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].key,
        ToolCallKey::new("fork-task", "assistant-2", "call-a")
    );
}

#[test]
fn byop_fork_repair_records_do_not_convert_orphan_results() {
    let source_tasks = vec![byop_test_task(
        "source-task",
        vec![byop_tool_result_message(
            "source-task",
            "orphan-result",
            "call-a",
        )],
    )];
    let forked_tasks = vec![byop_test_task(
        "fork-task",
        vec![byop_tool_result_message(
            "fork-task",
            "orphan-result",
            "call-a",
        )],
    )];

    let records = super::collect_byop_fork_repair_records(
        &source_tasks,
        &forked_tasks,
        &RepairStateStatus::default(),
        None,
    );

    assert!(records.is_empty());
}

#[test]
fn byop_fork_repair_records_do_not_infer_unexplained_missing_results() {
    let source_tasks = vec![byop_test_task(
        "source-task",
        vec![byop_tool_call_message(
            "source-task",
            "assistant-1",
            "call-a",
        )],
    )];
    let forked_tasks = vec![byop_test_task(
        "fork-task",
        vec![byop_tool_call_message("fork-task", "assistant-1", "call-a")],
    )];

    let records = super::collect_byop_fork_repair_records(
        &source_tasks,
        &forked_tasks,
        &RepairStateStatus::default(),
        None,
    );

    assert!(records.is_empty());
}

#[test]
fn byop_fork_repair_records_translate_existing_repair_keys() {
    let source_tasks = vec![byop_test_task(
        "source-task",
        vec![byop_tool_call_message(
            "source-task",
            "assistant-1",
            "call-a",
        )],
    )];
    let forked_tasks = vec![byop_test_task(
        "fork-task",
        vec![byop_tool_call_message("fork-task", "assistant-1", "call-a")],
    )];
    let source_record = RepairRecord::new(
        RepairSource::RestoredLegacyHistory,
        ToolCallKey::new("source-task", "assistant-1", "call-a"),
    );

    let records = super::collect_byop_fork_repair_records(
        &source_tasks,
        &forked_tasks,
        &RepairStateStatus::Valid(RepairState::new(vec![source_record])),
        None,
    );

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source, RepairSource::RestoredLegacyHistory);
    assert_eq!(
        records[0].key,
        ToolCallKey::new("fork-task", "assistant-1", "call-a")
    );
}

#[test]
fn test_ai_queries_for_terminal_view_up_arrow_history() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);

        let now = Local::now();
        let terminal_view_id = EntityId::new();
        let current_session_id = SessionId::from(0);
        let all_live_session_ids = HashSet::from([current_session_id]);

        // Create initial persisted queries
        let conversation_id_1 = AIConversationId::new();
        let conversation_id_2 = AIConversationId::new();

        let persisted_queries = vec![
            create_persisted_query(
                "restored query 1",
                conversation_id_1,
                now - chrono::Duration::seconds(10),
            ),
            create_persisted_query(
                "restored query 2",
                conversation_id_2,
                now - chrono::Duration::seconds(5),
            ),
        ];

        // Create history model with persisted queries as a singleton
        let history_model =
            app.add_singleton_model(|_| BlocklistAIHistoryModel::new(persisted_queries, &[]));

        // Helper function to get and sort AI queries using the same logic as Input
        let get_sorted_queries = |model: &BlocklistAIHistoryModel| -> Vec<String> {
            model
                .all_ai_queries(Some(terminal_view_id))
                .map(|query| HistoryInputSuggestion::AIQuery { entry: query })
                .sorted_by(|a, b| a.cmp(b, Some(current_session_id), &all_live_session_ids))
                .map(|suggestion| suggestion.text().to_string())
                .collect()
        };

        // Test initial state with just persisted queries
        let queries = history_model.read(&app, |model, _| get_sorted_queries(model));
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0], "restored query 1");
        assert_eq!(queries[1], "restored query 2");

        // Start a new conversation and add "live query 1"
        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let stream_id = ResponseStreamId::new_for_test();
        history_model.update(&mut app, |history_model, ctx| {
            let exchange = create_exchange_with_query("live query 1", now, None);
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();
            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };
            history_model
                .update_conversation_for_new_request_input(
                    request_input,
                    stream_id,
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        // Test state after adding live query 1
        let queries = history_model.read(&app, |model, _| get_sorted_queries(model));
        assert_eq!(queries.len(), 3);
        assert_eq!(queries[0], "restored query 1");
        assert_eq!(queries[1], "restored query 2");
        assert_eq!(queries[2], "live query 1");

        // Start another new conversation and add "live query 2"
        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        history_model.update(&mut app, |history_model, ctx| {
            let exchange = create_exchange_with_query(
                "live query 2",
                now + chrono::Duration::seconds(1),
                None,
            );
            let stream_id = ResponseStreamId::new_for_test();
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();
            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };
            history_model
                .update_conversation_for_new_request_input(
                    request_input,
                    stream_id,
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        // Test state after adding live query 2
        let queries = history_model.read(&app, |model, _| get_sorted_queries(model));
        assert_eq!(queries.len(), 4);
        assert_eq!(queries[0], "restored query 1");
        assert_eq!(queries[1], "restored query 2");
        assert_eq!(queries[2], "live query 1");
        assert_eq!(queries[3], "live query 2");

        // Clear the blocklist
        history_model.update(&mut app, |history_model, ctx| {
            history_model.clear_conversations_in_terminal_view(terminal_view_id, ctx);
        });

        // Test state after clearing - should remain the same
        let queries = history_model.read(&app, |model, _| get_sorted_queries(model));
        assert_eq!(queries.len(), 4);
        assert_eq!(queries[0], "restored query 1");
        assert_eq!(queries[1], "restored query 2");
        assert_eq!(queries[2], "live query 1");
        assert_eq!(queries[3], "live query 2");

        // Start a new conversation after clearing and add "new query after clear"
        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        history_model.update(&mut app, |history_model, ctx| {
            let stream_id = ResponseStreamId::new_for_test();
            let exchange = create_exchange_with_query(
                "new query after clear",
                now + chrono::Duration::seconds(2),
                None,
            );
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();
            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };
            history_model
                .update_conversation_for_new_request_input(
                    request_input,
                    stream_id,
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        // Test final state
        let queries = history_model.read(&app, |model, _| get_sorted_queries(model));
        assert_eq!(queries.len(), 5);
        assert_eq!(queries[0], "restored query 1");
        assert_eq!(queries[1], "restored query 2");
        assert_eq!(queries[2], "live query 1");
        assert_eq!(queries[3], "live query 2");
        assert_eq!(queries[4], "new query after clear");
    });
}

#[test]
fn test_transcript_viewer_terminal_view_is_not_marked_historical() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);

        let now = Local::now();
        let terminal_view_id = EntityId::new();

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        history_model.update(&mut app, |history_model, ctx| {
            let exchange = create_exchange_with_query("query", now, None);
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();

            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };

            history_model
                .update_conversation_for_new_request_input(
                    request_input,
                    ResponseStreamId::new_for_test(),
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        history_model.update(&mut app, |history_model, _| {
            history_model.mark_terminal_view_as_conversation_transcript_viewer(terminal_view_id);
            history_model.mark_conversations_historical_for_terminal_view(terminal_view_id);
        });

        let historical_count = history_model.read(&app, |history_model, _| {
            history_model.get_local_conversations_metadata().count()
        });
        assert_eq!(historical_count, 0);
    });
}

#[test]
fn test_ambient_agent_conversations_excluded_from_list_but_accessible_by_id() {
    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let regular_id = AIConversationId::new();
        let ambient_id = AIConversationId::new();

        let ambient_task_id: AmbientAgentTaskId = uuid::Uuid::new_v4().to_string().parse().unwrap();

        history_model.update(&mut app, |model, _| {
            let regular_metadata = AIConversationMetadata {
                id: regular_id,
                title: "Regular Conversation".to_string(),
                initial_query: String::new(),
                last_modified_at: Utc::now().naive_utc(),
                initial_working_directory: None,
                credits_spent: Some(5.0),
                server_conversation_token: Some(ServerConversationToken::new(
                    "token-regular".to_string(),
                )),
                is_restorable_locally: false,
                artifacts: Vec::new(),
                ambient_agent_task_id: None,
                parent_conversation_id: None,
                parent_agent_id: None,
            };
            model
                .all_conversations_metadata
                .insert(regular_id, regular_metadata);

            let ambient_metadata = AIConversationMetadata {
                id: ambient_id,
                title: "Ambient Conversation".to_string(),
                initial_query: String::new(),
                last_modified_at: Utc::now().naive_utc(),
                initial_working_directory: None,
                credits_spent: Some(3.0),
                server_conversation_token: Some(ServerConversationToken::new(
                    "token-ambient".to_string(),
                )),
                is_restorable_locally: false,
                artifacts: Vec::new(),
                ambient_agent_task_id: Some(ambient_task_id),
                parent_conversation_id: None,
                parent_agent_id: None,
            };
            model
                .all_conversations_metadata
                .insert(ambient_id, ambient_metadata);
        });

        history_model.read(&app, |model, _| {
            // get_local_conversations_metadata should exclude the ambient conversation
            let listed: Vec<&AIConversationMetadata> =
                model.get_local_conversations_metadata().collect();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].id, regular_id);

            // get_conversation_metadata should return both by ID
            assert!(model.get_conversation_metadata(&regular_id).is_some());
            assert!(model.get_conversation_metadata(&ambient_id).is_some());
            assert_eq!(
                model.get_conversation_metadata(&ambient_id).unwrap().title,
                "Ambient Conversation"
            );
        });
    });
}

#[test]
fn test_child_agent_conversations_excluded_from_list_but_accessible_by_id() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let regular_id = AIConversationId::new();

        // One child linked via a local parent placeholder, one via the
        // parent's server-side run identifier (driver-hosted processes).
        let mut local_child = AIConversation::new(false);
        local_child.set_parent_conversation_id(AIConversationId::new());
        let local_child_id = local_child.id();
        let mut driver_child = AIConversation::new(false);
        driver_child.set_parent_agent_id("parent-run-id".to_string());
        let driver_child_id = driver_child.id();

        history_model.update(&mut app, |model, _| {
            let regular_metadata = AIConversationMetadata {
                id: regular_id,
                title: "Regular Conversation".to_string(),
                initial_query: String::new(),
                last_modified_at: Utc::now().naive_utc(),
                initial_working_directory: None,
                credits_spent: Some(5.0),
                server_conversation_token: Some(ServerConversationToken::new(
                    "token-regular-child-test".to_string(),
                )),
                is_restorable_locally: false,
                artifacts: Vec::new(),
                ambient_agent_task_id: None,
                parent_conversation_id: None,
                parent_agent_id: None,
            };
            model
                .all_conversations_metadata
                .insert(regular_id, regular_metadata);
            model
                .all_conversations_metadata
                .insert(local_child_id, AIConversationMetadata::from(&local_child));
            model
                .all_conversations_metadata
                .insert(driver_child_id, AIConversationMetadata::from(&driver_child));
        });

        history_model.read(&app, |model, _| {
            let listed: Vec<AIConversationId> = model
                .get_local_conversations_metadata()
                .map(|m| m.id)
                .collect();
            assert_eq!(
                listed,
                vec![regular_id],
                "child agent conversations must be excluded from the navigable list"
            );

            // Both children remain accessible by ID.
            assert!(model.get_conversation_metadata(&local_child_id).is_some());
            assert!(model.get_conversation_metadata(&driver_child_id).is_some());
        });
    });
}

#[test]
fn test_initialize_historical_conversations_indexes_child_conversations() {
    use crate::persistence::model::{AgentConversation, AgentConversationRecord};
    use chrono::NaiveDateTime;

    App::test((), |app| async move {
        let parent_id = AIConversationId::new();
        let child_id = AIConversationId::new();

        // Build a child AgentConversation whose conversation_data contains
        // a parent_conversation_id.  The child needs no tasks because
        // initialize_historical_conversations returns None (filters it out)
        // before inspecting tasks.
        let child_conversation_data = format!(r#"{{"parent_conversation_id":"{parent_id}"}}"#);

        let conversations = vec![AgentConversation {
            conversation: AgentConversationRecord {
                id: 1,
                conversation_id: child_id.to_string(),
                conversation_data: child_conversation_data,
                last_modified_at: NaiveDateTime::default(),
                summary: None,
            },
            tasks: vec![],
        }];

        let history_model =
            app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &conversations));

        history_model.read(&app, |model, _| {
            // The child conversation should be indexed under its parent.
            assert_eq!(model.child_conversation_ids_of(&parent_id), &[child_id]);

            // The child should NOT appear in navigable conversation metadata.
            let metadata_ids: Vec<AIConversationId> = model
                .get_local_conversations_metadata()
                .map(|m| m.id)
                .collect();
            assert!(
                !metadata_ids.contains(&child_id),
                "child conversation should be excluded from metadata"
            );
        });
    });
}

#[test]
fn test_set_parent_for_conversation_populates_index() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        // Create parent and child conversations via start_new_conversation.
        let parent_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        // Set the parent-child relationship.
        history_model.update(&mut app, |model, _| {
            model.set_parent_for_conversation(child_id, parent_id);
        });

        // Verify the index is populated and the conversation has the parent set.
        history_model.read(&app, |model, _| {
            assert_eq!(model.child_conversation_ids_of(&parent_id), &[child_id]);
            assert_eq!(model.child_conversations_of(parent_id).len(), 1);
            assert_eq!(model.child_conversations_of(parent_id)[0].id(), child_id);
            assert!(
                model
                    .conversation(&child_id)
                    .unwrap()
                    .parent_conversation_id()
                    == Some(parent_id)
            );
        });
    });
}

#[test]
fn test_set_parent_for_conversation_dedup() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let parent_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        // Set the same parent-child relationship twice.
        history_model.update(&mut app, |model, _| {
            model.set_parent_for_conversation(child_id, parent_id);
            model.set_parent_for_conversation(child_id, parent_id);
        });

        // Should have exactly one entry, not two.
        history_model.read(&app, |model, _| {
            assert_eq!(model.child_conversation_ids_of(&parent_id), &[child_id]);
        });
    });
}

#[test]
fn test_set_parent_multiple_children() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let parent_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_a = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_b = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        history_model.update(&mut app, |model, _| {
            model.set_parent_for_conversation(child_a, parent_id);
            model.set_parent_for_conversation(child_b, parent_id);
        });

        history_model.read(&app, |model, _| {
            let children = model.child_conversation_ids_of(&parent_id);
            assert_eq!(children.len(), 2);
            assert!(children.contains(&child_a));
            assert!(children.contains(&child_b));
            assert_eq!(model.child_conversations_of(parent_id).len(), 2);
        });
    });
}

#[test]
fn test_child_conversation_ids_of_unknown_parent() {
    App::test((), |app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let unknown_id = AIConversationId::new();

        history_model.read(&app, |model, _| {
            assert!(model.child_conversation_ids_of(&unknown_id).is_empty());
            assert!(model.child_conversations_of(unknown_id).is_empty());
        });
    });
}

#[test]
fn test_restore_conversations_maintains_children_by_parent() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let parent_id = AIConversationId::new();
        let mut child_conv = AIConversation::new(false);
        child_conv.set_parent_conversation_id(parent_id);
        let child_id = child_conv.id();

        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![child_conv], ctx);
        });

        history_model.read(&app, |model, _| {
            assert_eq!(model.child_conversation_ids_of(&parent_id), &[child_id]);
        });
    });
}

#[test]
fn test_restore_conversations_dedup_children_by_parent() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let parent_id = AIConversationId::new();
        let mut child_conv_a = AIConversation::new(false);
        child_conv_a.set_parent_conversation_id(parent_id);
        let child_id = child_conv_a.id();
        let child_conv_b = child_conv_a.clone();

        // Restore the same child conversation twice (simulates close + reopen).
        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![child_conv_a], ctx);
        });
        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![child_conv_b], ctx);
        });

        // Should have exactly one entry, not two.
        history_model.read(&app, |model, _| {
            assert_eq!(model.child_conversation_ids_of(&parent_id), &[child_id]);
        });
    });
}

#[test]
fn test_all_cleared_conversations_includes_terminal_view_id() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);

        let now = Local::now();
        let terminal_view_id = EntityId::new();

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        history_model.update(&mut app, |history_model, ctx| {
            let exchange = create_exchange_with_query("query", now, None);
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();

            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };

            history_model
                .update_conversation_for_new_request_input(
                    request_input,
                    ResponseStreamId::new_for_test(),
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        history_model.update(&mut app, |history_model, ctx| {
            history_model.clear_conversations_in_terminal_view(terminal_view_id, ctx);
        });

        let has_cleared = history_model.read(&app, |history_model, _| {
            history_model
                .all_cleared_conversations()
                .iter()
                .any(|(id, convo)| *id == terminal_view_id && convo.id() == conversation_id)
        });

        assert!(has_cleared);
    });
}

#[test]
fn test_toggle_autoexecute_override_persists_updated_conversation_state() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let mut global_resource_handles = GlobalResourceHandles::mock(&mut app);
        global_resource_handles.model_event_sender = Some(sender);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let terminal_view_id = EntityId::new();

        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        history_model.update(&mut app, |history_model, ctx| {
            history_model.toggle_autoexecute_override(&conversation_id, terminal_view_id, ctx);
        });

        let event = receiver.recv_timeout(Duration::from_secs(1)).unwrap();

        let ModelEvent::UpdateMultiAgentConversation {
            conversation_id: persisted_conversation_id,
            conversation_data,
            ..
        } = event
        else {
            panic!("expected UpdateMultiAgentConversation event");
        };

        assert_eq!(persisted_conversation_id, conversation_id.to_string());
        assert_eq!(
            conversation_data.autoexecute_override,
            Some(PersistedAutoexecuteMode::RunToCompletion)
        );
    });
}

#[test]
fn test_find_by_token_after_restore_conversations() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let terminal_view_id = EntityId::new();

        let mut conversation = AIConversation::new(false);
        conversation.set_server_conversation_token("restored-token".to_string());
        let conversation_id = conversation.id();

        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
        });

        let token = ServerConversationToken::new("restored-token".to_string());
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                Some(conversation_id),
            );
        });
    });
}

#[test]
fn test_find_by_token_returns_none_after_remove_conversation() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // `delete_conversation` publishes persistence events via
        // `GlobalResourceHandlesProvider`, so we need a mock sender wired up.
        let (sender, _receiver) = std::sync::mpsc::sync_channel(2);
        let mut global_resource_handles = GlobalResourceHandles::mock(&mut app);
        global_resource_handles.model_event_sender = Some(sender);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let mut conversation = AIConversation::new(false);
        conversation.set_server_conversation_token("removable-token".to_string());
        let conversation_id = conversation.id();

        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(EntityId::new(), vec![conversation], ctx);
        });

        let token = ServerConversationToken::new("removable-token".to_string());
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                Some(conversation_id),
                "token should resolve before removal",
            );
        });

        history_model.update(&mut app, |model, ctx| {
            model.delete_conversation(conversation_id, None, ctx);
        });

        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                None,
                "reverse index must be cleared when the conversation is removed",
            );
        });
    });
}

#[test]
fn test_find_by_token_returns_none_after_reset() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let mut conversation = AIConversation::new(false);
        conversation.set_server_conversation_token("reset-token".to_string());
        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(EntityId::new(), vec![conversation], ctx);
        });

        let token = ServerConversationToken::new("reset-token".to_string());

        history_model.read(&app, |model, _| {
            assert!(model.find_conversation_id_by_server_token(&token).is_some());
        });

        history_model.update(&mut app, |model, _| {
            model.reset();
        });

        history_model.read(&app, |model, _| {
            assert_eq!(model.find_conversation_id_by_server_token(&token), None);
        });
    });
}

#[test]
fn test_find_by_token_after_initialize_output_for_response_stream() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);

        let now = Local::now();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let terminal_view_id = EntityId::new();

        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        // Prime a pending request so StreamInit can install the token.
        let stream_id = ResponseStreamId::new_for_test();
        history_model.update(&mut app, |history_model, ctx| {
            let exchange = create_exchange_with_query("query", now, None);
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();
            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };
            history_model
                .update_conversation_for_new_request_input(
                    request_input,
                    stream_id.clone(),
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        let server_token_str = "init-token".to_string();
        history_model.update(&mut app, |history_model, ctx| {
            history_model.initialize_output_for_response_stream(
                &stream_id,
                conversation_id,
                terminal_view_id,
                warp_multi_agent_api::response_event::StreamInit {
                    request_id: String::new(),
                    conversation_id: server_token_str.clone(),
                    run_id: String::new(),
                },
                ctx,
            );
        });

        let token = ServerConversationToken::new(server_token_str);
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                Some(conversation_id),
            );
        });
    });
}

#[test]
fn test_find_by_token_after_assign_run_id_for_conversation() {
    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let terminal_view_id = EntityId::new();

        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            let id = history_model.start_new_conversation(terminal_view_id, false, false, ctx);
            // Seed a token so assign_run_id has one to forward into the index.
            history_model
                .conversation_mut(&id)
                .expect("conversation should exist")
                .set_server_conversation_token("run-id-token".to_string());
            id
        });

        history_model.update(&mut app, |history_model, ctx| {
            history_model.assign_run_id_for_conversation(
                conversation_id,
                "run-1".to_string(),
                None,
                terminal_view_id,
                ctx,
            );
        });

        let token = ServerConversationToken::new("run-id-token".to_string());
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                Some(conversation_id),
            );
        });
    });
}

#[test]
fn test_find_by_token_after_insert_forked_conversation_from_tasks() {
    use crate::persistence::model::AgentConversationData;

    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let forked_conversation_id = AIConversationId::new();
        let conversation_data = AgentConversationData {
            is_remote_child: false,
            server_conversation_token: Some("forked-token".to_string()),
            conversation_usage_metadata: None,
            reverted_action_ids: None,
            forked_from_server_conversation_token: None,
            artifacts_json: None,
            parent_agent_id: None,
            agent_name: None,
            parent_conversation_id: None,
            run_id: None,
            autoexecute_override: None,
            last_event_sequence: None,
            compaction_state_json: None,
            byop_repair_state_json: None,
            cli_subagent_block_snapshots_json: None,
        };
        let tasks = vec![warp_multi_agent_api::Task {
            id: "root-task".to_string(),
            messages: vec![],
            dependencies: None,
            description: String::new(),
            summary: String::new(),
            server_data: String::new(),
        }];

        history_model.update(&mut app, |model, _| {
            model
                .insert_forked_conversation_from_tasks(
                    forked_conversation_id,
                    tasks,
                    conversation_data,
                )
                .expect("forked conversation should insert");
        });

        let token = ServerConversationToken::new("forked-token".to_string());
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                Some(forked_conversation_id),
            );
        });
    });
}

#[test]
fn test_find_by_token_after_mark_conversations_historical_for_terminal_view() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);

        let now = Local::now();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let terminal_view_id = EntityId::new();

        // Needs a real exchange to pass `conversation_would_render_in_blocklist`.
        let mut conversation = AIConversation::new(false);
        conversation.set_server_conversation_token("historical-token".to_string());
        let conversation_id = conversation.id();

        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
        });

        history_model.update(&mut app, |history_model, ctx| {
            let exchange = create_exchange_with_query("historical query", now, None);
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();
            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };
            history_model
                .update_conversation_for_new_request_input(
                    request_input,
                    ResponseStreamId::new_for_test(),
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        // Sanity check: token resolves after restore_conversations.
        let token = ServerConversationToken::new("historical-token".to_string());
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                Some(conversation_id),
            );
        });

        history_model.update(&mut app, |model, _| {
            model.mark_conversations_historical_for_terminal_view(terminal_view_id);
        });

        // Token still resolves via the metadata-side index entry.
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&token),
                Some(conversation_id),
            );
            assert!(
                model.get_conversation_metadata(&conversation_id).is_some(),
                "metadata entry must exist so the reverse index is not dangling",
            );
        });
    });
}

#[test]
fn test_set_server_conversation_token_rebinds_reverse_index() {
    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let terminal_view_id = EntityId::new();

        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            let id = history_model.start_new_conversation(terminal_view_id, false, false, ctx);
            history_model.set_server_conversation_token_for_conversation(id, "old".to_string());
            id
        });

        let old_token = ServerConversationToken::new("old".to_string());
        history_model.read(&app, |model, _| {
            assert_eq!(
                model.find_conversation_id_by_server_token(&old_token),
                Some(conversation_id),
            );
        });

        history_model.update(&mut app, |history_model, _| {
            history_model
                .set_server_conversation_token_for_conversation(conversation_id, "new".to_string());
        });

        let new_token = ServerConversationToken::new("new".to_string());
        history_model.read(&app, |model, _| {
            // Stale lookups must not resolve to the rebound conversation.
            assert_eq!(model.find_conversation_id_by_server_token(&old_token), None);
            assert_eq!(
                model.find_conversation_id_by_server_token(&new_token),
                Some(conversation_id),
            );
        });
    });
}

#[test]
fn test_fork_conversation_rejects_an_empty_source() {
    use crate::ai::agent::conversation::AIConversation;

    App::test((), |mut app| async move {
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let source = AIConversation::new(false);

        let error = history_model.update(&mut app, |model, ctx| {
            model
                .fork_conversation(&source, "[Fork] ", ctx)
                .expect_err("forking an empty conversation should fail")
        });

        assert_eq!(
            error.downcast_ref::<ForkConversationError>(),
            Some(&ForkConversationError::EmptyConversation),
        );
    });
}

/// Builds a restored conversation with a single root task whose description
/// serves as the initial (generated) title.
#[cfg(test)]
fn restored_conversation_with_title(
    conversation_id: AIConversationId,
    description: &str,
) -> crate::ai::agent::conversation::AIConversation {
    use crate::ai::agent::conversation::AIConversation;
    AIConversation::new_restored(
        conversation_id,
        vec![api::Task {
            id: "root-task".to_string(),
            messages: vec![],
            dependencies: None,
            description: description.to_string(),
            summary: String::new(),
            server_data: String::new(),
        }],
        None,
    )
    .expect("conversation should restore")
}

#[test]
fn begin_conversation_rename_updates_title_and_cached_metadata() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let conversation_id = AIConversationId::new();
        let conversation = restored_conversation_with_title(conversation_id, "Generated title");

        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
            model.set_server_conversation_token_for_conversation(
                conversation_id,
                "server-conversation-token".to_string(),
            );
            let metadata = AIConversationMetadata::from(
                model
                    .conversation(&conversation_id)
                    .expect("conversation should exist"),
            );
            model
                .all_conversations_metadata
                .insert(conversation_id, metadata);
            let server_conversation_token = model
                .begin_conversation_rename(conversation_id, "Manual title".to_string(), ctx)
                .expect("rename should begin");
            assert_eq!(server_conversation_token, "server-conversation-token");
        });

        history_model.read(&app, |model, _| {
            let conversation = model
                .conversation(&conversation_id)
                .expect("conversation should exist");
            assert_eq!(conversation.title().as_deref(), Some("Manual title"));
            assert_eq!(
                conversation
                    .get_root_task()
                    .map(|root_task| root_task.description()),
                Some("Manual title"),
            );
            assert_eq!(
                model
                    .get_conversation_metadata(&conversation_id)
                    .map(|metadata| metadata.title.as_str()),
                Some("Manual title"),
            );
            assert!(model
                .in_flight_conversation_renames
                .contains_key(&conversation_id));
        });
    });
}

#[test]
fn begin_conversation_rename_rejects_conversation_without_server_token() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let conversation_id = AIConversationId::new();
        let conversation = restored_conversation_with_title(conversation_id, "Generated title");

        let result = history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
            let metadata = AIConversationMetadata::from(
                model
                    .conversation(&conversation_id)
                    .expect("conversation should exist"),
            );
            model
                .all_conversations_metadata
                .insert(conversation_id, metadata);
            model.begin_conversation_rename(conversation_id, "Manual title".to_string(), ctx)
        });

        assert_eq!(
            result,
            Err(BeginConversationRenameError::MissingServerConversationToken)
        );
        history_model.read(&app, |model, _| {
            let conversation = model
                .conversation(&conversation_id)
                .expect("conversation should exist");
            assert_eq!(conversation.title().as_deref(), Some("Generated title"));
            assert_eq!(
                model
                    .get_conversation_metadata(&conversation_id)
                    .map(|metadata| metadata.title.as_str()),
                Some("Generated title"),
            );
            assert!(!model
                .in_flight_conversation_renames
                .contains_key(&conversation_id));
        });
    });
}

#[test]
fn begin_conversation_rename_rejects_optimistic_root_task() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let (conversation_id, result) = history_model.update(&mut app, |model, ctx| {
            let conversation_id = model.start_new_conversation(terminal_view_id, false, false, ctx);
            model.set_server_conversation_token_for_conversation(
                conversation_id,
                "server-conversation-token".to_string(),
            );
            let result =
                model.begin_conversation_rename(conversation_id, "Manual title".to_string(), ctx);
            (conversation_id, result)
        });

        assert_eq!(
            result,
            Err(BeginConversationRenameError::ConversationNotReady)
        );
        history_model.read(&app, |model, _| {
            let conversation = model
                .conversation(&conversation_id)
                .expect("conversation should exist");
            let root_task = conversation
                .get_root_task()
                .expect("conversation should have a root task");
            assert!(root_task.source().is_none());
            assert_eq!(root_task.description(), "");
            assert!(!model
                .in_flight_conversation_renames
                .contains_key(&conversation_id));
        });
    });
}

#[test]
fn complete_conversation_rename_applies_normalized_title_and_clears_in_flight_state() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let conversation_id = AIConversationId::new();
        let conversation = restored_conversation_with_title(conversation_id, "Generated title");

        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
            model.set_server_conversation_token_for_conversation(
                conversation_id,
                "server-conversation-token".to_string(),
            );
            let metadata = AIConversationMetadata::from(
                model
                    .conversation(&conversation_id)
                    .expect("conversation should exist"),
            );
            model
                .all_conversations_metadata
                .insert(conversation_id, metadata);
            model
                .begin_conversation_rename(conversation_id, "Manual title".to_string(), ctx)
                .expect("rename should begin");
            model.complete_conversation_rename(conversation_id, "Normalized title".to_string(), ctx);
        });

        history_model.read(&app, |model, _| {
            let conversation = model
                .conversation(&conversation_id)
                .expect("conversation should exist");
            assert_eq!(conversation.title().as_deref(), Some("Normalized title"));
            assert_eq!(
                conversation
                    .get_root_task()
                    .map(|root_task| root_task.description()),
                Some("Normalized title"),
            );
            assert_eq!(
                model
                    .get_conversation_metadata(&conversation_id)
                    .map(|metadata| metadata.title.as_str()),
                Some("Normalized title"),
            );
            assert!(!model
                .in_flight_conversation_renames
                .contains_key(&conversation_id));
        });
    });
}

#[test]
fn fail_conversation_rename_reverts_title_and_cached_metadata() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let conversation_id = AIConversationId::new();
        let conversation = restored_conversation_with_title(conversation_id, "Generated title");

        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
            model.set_server_conversation_token_for_conversation(
                conversation_id,
                "server-conversation-token".to_string(),
            );
            let metadata = AIConversationMetadata::from(
                model
                    .conversation(&conversation_id)
                    .expect("conversation should exist"),
            );
            model
                .all_conversations_metadata
                .insert(conversation_id, metadata);
            model
                .begin_conversation_rename(conversation_id, "Manual title".to_string(), ctx)
                .expect("rename should begin");
            model.fail_conversation_rename(conversation_id, ctx);
        });

        history_model.read(&app, |model, _| {
            let conversation = model
                .conversation(&conversation_id)
                .expect("conversation should exist");
            assert_eq!(conversation.title().as_deref(), Some("Generated title"));
            assert_eq!(
                conversation
                    .get_root_task()
                    .map(|root_task| root_task.description()),
                Some("Generated title"),
            );
            assert_eq!(
                model
                    .get_conversation_metadata(&conversation_id)
                    .map(|metadata| metadata.title.as_str()),
                Some("Generated title"),
            );
            assert!(!model
                .in_flight_conversation_renames
                .contains_key(&conversation_id));
        });
    });
}

#[test]
fn begin_conversation_rename_rejects_second_rename_while_in_flight() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let conversation_id = AIConversationId::new();
        let conversation = restored_conversation_with_title(conversation_id, "Generated title");

        let second_result = history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
            model.set_server_conversation_token_for_conversation(
                conversation_id,
                "server-conversation-token".to_string(),
            );
            model
                .begin_conversation_rename(conversation_id, "Manual title".to_string(), ctx)
                .expect("rename should begin");
            model.begin_conversation_rename(conversation_id, "Second title".to_string(), ctx)
        });

        assert_eq!(
            second_result,
            Err(BeginConversationRenameError::RenameInProgress)
        );
        history_model.read(&app, |model, _| {
            let conversation = model
                .conversation(&conversation_id)
                .expect("conversation should exist");
            assert_eq!(conversation.title().as_deref(), Some("Manual title"));
            assert!(model
                .in_flight_conversation_renames
                .contains_key(&conversation_id));
        });
    });
}

#[test]
fn test_update_event_sequence_persists_updated_conversation_state() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let mut global_resource_handles = GlobalResourceHandles::mock(&mut app);
        global_resource_handles.model_event_sender = Some(sender);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let terminal_view_id = EntityId::new();

        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        history_model.update(&mut app, |history_model, ctx| {
            history_model.update_event_sequence(conversation_id, 42, ctx);
        });

        let event = receiver.recv_timeout(Duration::from_secs(1)).unwrap();

        let ModelEvent::UpdateMultiAgentConversation {
            conversation_id: persisted_conversation_id,
            conversation_data,
            ..
        } = event
        else {
            panic!("expected UpdateMultiAgentConversation event");
        };

        assert_eq!(persisted_conversation_id, conversation_id.to_string());
        assert_eq!(conversation_data.last_event_sequence, Some(42));

        history_model.read(&app, |history_model, _| {
            let conversation = history_model
                .conversation(&conversation_id)
                .expect("conversation should exist");
            assert_eq!(conversation.last_event_sequence(), Some(42));
        });
    });
}

#[test]
fn test_remove_child_conversation_cleans_parent_index() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let parent_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let child_id = history_model.update(&mut app, |model, ctx| {
            let child_id = model.start_new_conversation(terminal_view_id, false, false, ctx);
            model.set_parent_for_conversation(child_id, parent_id);
            child_id
        });

        history_model.update(&mut app, |model, ctx| {
            model.remove_conversation(child_id, terminal_view_id, ctx);
        });

        history_model.read(&app, |model, _| {
            assert!(model.conversation(&child_id).is_none());
            assert!(model.child_conversation_ids_of(&parent_id).is_empty());
        });
    });
}

#[test]
fn test_remove_parent_conversation_cleans_incoming_and_outgoing_index_entries() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));
        let grandparent_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let parent_id = history_model.update(&mut app, |model, ctx| {
            let parent_id = model.start_new_conversation(terminal_view_id, false, false, ctx);
            model.set_parent_for_conversation(parent_id, grandparent_id);
            parent_id
        });
        let child_id = history_model.update(&mut app, |model, ctx| {
            let child_id = model.start_new_conversation(terminal_view_id, false, false, ctx);
            model.set_parent_for_conversation(child_id, parent_id);
            child_id
        });

        history_model.update(&mut app, |model, ctx| {
            model.remove_conversation(parent_id, terminal_view_id, ctx);
        });

        history_model.read(&app, |model, _| {
            assert!(model.conversation(&parent_id).is_none());
            assert!(model.conversation(&child_id).is_some());
            assert!(model.child_conversation_ids_of(&grandparent_id).is_empty());
            assert!(model.child_conversation_ids_of(&parent_id).is_empty());
        });
    });
}

/// Drives a conversation to a stream error and returns both the resulting
/// conversation status and the ambient-SDK-facing derived outcome.
fn statuses_after_stream_error(
    error: RenderableAIError,
    recovery_pending: bool,
) -> (Option<ConversationStatus>, Option<AmbientConversationStatus>) {
    type Captured = (Option<ConversationStatus>, Option<AmbientConversationStatus>);
    let derived: std::sync::Arc<std::sync::Mutex<Captured>> =
        std::sync::Arc::new(std::sync::Mutex::<Captured>::new((None, None)));
    let derived_for_test = std::sync::Arc::clone(&derived);
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let conversation_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let stream_id = ResponseStreamId::new_for_test();
        history_model.update(&mut app, |model, ctx| {
            let exchange = create_exchange_with_query("test query", Local::now(), None);
            let task_id = model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();
            let request_input = RequestInput {
                conversation_id,
                input_messages: std::collections::HashMap::from([(task_id, exchange.input)]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };
            model
                .update_conversation_for_new_request_input(
                    request_input,
                    stream_id.clone(),
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });

        history_model.update(&mut app, |model, ctx| {
            model.mark_response_stream_completed_with_error(
                error,
                recovery_pending,
                &stream_id,
                conversation_id,
                terminal_view_id,
                ctx,
            );
        });

        *derived_for_test.lock().unwrap() = history_model.read(&app, |model, _| {
            let conversation = model.conversation(&conversation_id).unwrap();
            (
                Some(conversation.status().clone()),
                conversation_output_status_from_conversation(conversation),
            )
        });
    });

    std::mem::take(&mut *derived.lock().unwrap())
}

/// A failure with a recovery scheduled moves the conversation to the
/// non-terminal `TransientError` status, and the driver-facing conversion must
/// not report a terminal outcome for it.
#[test]
fn recovery_pending_error_sets_transient_error_status() {
    let (status, derived) = statuses_after_stream_error(
        RenderableAIError::transient_network_error(
            true,
            false,
            TransientNetworkErrorKind::UnfinishedExchange,
        ),
        /*recovery_pending*/ true,
    );

    assert_eq!(status, Some(ConversationStatus::TransientError));
    assert!(
        derived.is_none(),
        "a pending recovery must not derive a terminal outcome, got {derived:?}"
    );
}

/// The structured exchange error (and its rendering hints) must survive the
/// conversion to `AmbientConversationStatus`.
#[test]
fn structured_exchange_error_is_preserved_in_output_status() {
    let (status, derived) = statuses_after_stream_error(
        RenderableAIError::transient_network_error(
            true,
            false,
            TransientNetworkErrorKind::UnfinishedExchange,
        ),
        /*recovery_pending*/ false,
    );

    assert_eq!(status, Some(ConversationStatus::Error));
    let Some(AmbientConversationStatus::Error { error }) = derived else {
        panic!("expected an error status, got {derived:?}");
    };
    assert!(
        error.will_attempt_resume(),
        "the structured exchange error must be preserved, got {error:?}"
    );
}

/// A stream error without a pending recovery stays terminal.
#[test]
fn non_resumable_stream_error_stays_terminal_in_output_status() {
    let (status, derived) = statuses_after_stream_error(
        RenderableAIError::transient_network_error(
            false,
            false,
            TransientNetworkErrorKind::UnfinishedExchange,
        ),
        /*recovery_pending*/ false,
    );

    assert_eq!(status, Some(ConversationStatus::Error));
    let Some(AmbientConversationStatus::Error { error }) = derived else {
        panic!("expected an error status, got {derived:?}");
    };
    assert!(
        !error.will_attempt_resume(),
        "will_attempt_resume must be false for a non-recoverable error, got {error:?}"
    );
}

// --- rewind truncation regression (REPRO) ---
//
// Ported from the pinned Warp oracle `02b53fcd8`
// (`app/src/ai/blocklist/history_model_tests.rs`). Assertions are verbatim;
// the only adaptations are noted inline.

/// Builds a user-query message on an explicit task, so a test can put queries
/// on both a root task and a subtask. (The file's existing
/// `byop_user_query_message` hard-codes the `"root-task"` task id.)
fn create_user_query_message(
    id: &str,
    task_id: &str,
    request_id: &str,
    query: &str,
) -> api::Message {
    api::Message {
        fetched_memories: vec![],
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        message: Some(api::message::Message::UserQuery(api::message::UserQuery {
            query: query.to_string(),
            context: None,
            referenced_attachments: HashMap::new(),
            mode: None,
            intended_agent: Default::default(),
        })),
        request_id: request_id.to_string(),
        timestamp: None,
    }
}

fn agent_output_message(id: &str, task_id: &str, request_id: &str, text: &str) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::AgentOutput(
            api::message::AgentOutput {
                text: text.to_string(),
            },
        )),
        request_id: request_id.to_string(),
        timestamp: None,
    }
}

fn subagent_tool_call_result_message(
    id: &str,
    task_id: &str,
    tool_call_id: &str,
    request_id: &str,
) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::ToolCallResult(
            api::message::ToolCallResult {
                tool_call_id: tool_call_id.to_string(),
                context: None,
                result: Some(api::message::tool_call_result::Result::Cancel(())),
            },
        )),
        request_id: request_id.to_string(),
        timestamp: None,
    }
}

/// Builds a sub-agent tool-call message with `request_id` set, returning the
/// message and its `tool_call_id` (for pairing with a result message).
fn make_subagent_call(
    id: &str,
    task_id: &str,
    subtask_id: &str,
    request_id: &str,
    metadata: Option<api::message::tool_call::subagent::Metadata>,
) -> (api::Message, String) {
    use crate::ai::agent::task::helper::MessageExt;
    use crate::test_util::ai_agent_tasks::create_subagent_tool_call_message;
    let mut call = create_subagent_tool_call_message(id, task_id, subtask_id, metadata);
    call.request_id = request_id.to_string();
    let tool_call_id = call.tool_call().unwrap().tool_call_id.clone();
    (call, tool_call_id)
}

/// Returns the flat list of user-query strings in `tasks` (root + subtasks),
/// in linearized order.
fn user_queries_in_tasks(tasks: &[api::Task]) -> Vec<String> {
    let mut queries = Vec::new();
    for task in tasks {
        for message in &task.messages {
            if let Some(api::message::Message::UserQuery(uq)) = &message.message {
                queries.push(uq.query.clone());
            }
        }
    }
    queries
}

/// Returns the root task from a `compute_active_tasks()` result.
fn find_root_task<'a>(tasks: &'a [api::Task], root_task_id: &str) -> &'a api::Task {
    tasks
        .iter()
        .find(|t| t.id == root_task_id)
        .expect("root task must be present in the active task set")
}

/// True if `task` contains a sub-agent `tool_call` whose matching
/// `tool_call_result` is absent (a dangling tool_use), or a `tool_call_result`
/// for a sub-agent `tool_call` that is absent. Used to assert the rewind
/// invariant that no sub-agent call/result half is ever left dangling.
fn has_dangling_subagent_pair(task: &api::Task) -> bool {
    use crate::ai::agent::task::helper::{MessageExt, ToolCallExt};
    let subagent_call_ids: HashSet<&str> = task
        .messages
        .iter()
        .filter_map(|m| {
            m.tool_call()
                .and_then(|tc| tc.subagent().map(|_| tc.tool_call_id.as_str()))
        })
        .collect();
    let result_ids: HashSet<&str> = task
        .messages
        .iter()
        .filter_map(|m| m.tool_call_result().map(|r| r.tool_call_id.as_str()))
        .collect();
    // A sub-agent call without its result.

    // A result for a sub-agent call that no longer exists. (Non-sub-agent tool
    // results, e.g. run_shell_command, are not tracked in `subagent_call_ids`
    // and so are correctly ignored here.)
    subagent_call_ids.iter().any(|id| !result_ids.contains(id))
}

/// Helper: build + restore a conversation, find the root exchange holding
/// `rewind_query`, truncate from it, and return the resulting
/// `compute_active_tasks()` plus the full set of task ids that survive in the
/// task store (so callers can assert subtask pruning).
fn restore_truncate_and_collect(
    app: &mut App,
    root_task: api::Task,
    subtasks: Vec<api::Task>,
    rewind_query: &str,
) -> (Vec<api::Task>, Vec<String>) {
    use crate::ai::agent::conversation::AIConversation;

    let terminal_view_id = EntityId::new();
    let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
    let conversation_id = AIConversationId::new();

    let mut tasks = vec![root_task];
    tasks.extend(subtasks);

    let conversation = AIConversation::new_restored(conversation_id, tasks, None)
        .expect("conversation should build");
    history_model.update(app, |model, ctx| {
        model.restore_conversations(terminal_view_id, vec![conversation], ctx);
    });

    let rewind_exchange_id = history_model.read(app, |model, _| {
        model
            .conversation(&conversation_id)
            .expect("conversation exists")
            .root_task_exchanges()
            .find(|exchange| {
                exchange.input.iter().any(|input| {
                    matches!(input, AIAgentInput::UserQuery { query, .. } if query == rewind_query)
                })
            })
            .map(|exchange| exchange.id)
            .expect("rewind exchange should exist")
    });

    history_model.update(app, |model, ctx| {
        model
            .truncate_conversation_from_exchange(conversation_id, rewind_exchange_id, ctx)
            .expect("truncate should succeed");
    });

    history_model.read(app, |model, _| {
        let conversation = model
            .conversation(&conversation_id)
            .expect("conversation exists");
        let active_tasks = conversation.compute_active_tasks();
        let all_task_ids = conversation
            .all_tasks()
            .map(|t| t.id().to_string())
            .collect::<Vec<_>>();
        (active_tasks, all_task_ids)
    })
}

/// Timing (a): a (terminal-use) sub-agent invoked AFTER the rewind point. The
/// root sub-agent tool-call message is in the rewound turn, so truncation
/// removes it; the now-orphaned subtask must be pruned from the task store and
/// must not appear in the next request.
#[test]
fn rewind_orphans_subagent_subtask_invoked_after_rewind_point() {
    use crate::test_util::ai_agent_tasks::{create_api_subtask, create_api_task};

    App::test((), |mut app| async move {
        // Adaptation: the pinned oracle calls
        // `initialize_history_persistence_for_tests`; this fork inlines the
        // same steps in `initialize_history_model_test_app` (settings +
        // `GlobalResourceHandlesProvider`).
        initialize_history_model_test_app(&mut app);
        let root_task_id = "root-task";
        let subtask_id = "sub-1";

        let (subagent_call, subagent_tool_call_id) =
            make_subagent_call("m4", root_task_id, subtask_id, "req-2", None);

        let root_task = create_api_task(
            root_task_id,
            vec![
                create_user_query_message("m1", root_task_id, "req-1", "keep me"),
                agent_output_message("m2", root_task_id, "req-1", "ok"),
                // The rewound turn that spawns the terminal sub-agent.
                create_user_query_message("m3", root_task_id, "req-2", "spawn terminal agent"),
                subagent_call,
                subagent_tool_call_result_message(
                    "m5",
                    root_task_id,
                    &subagent_tool_call_id,
                    "req-2",
                ),
            ],
        );
        let subtask = create_api_subtask(
            subtask_id,
            root_task_id,
            vec![agent_output_message(
                "s1",
                subtask_id,
                "req-2",
                "terminal work",
            )],
        );

        let (active_tasks, all_task_ids) = restore_truncate_and_collect(
            &mut app,
            root_task,
            vec![subtask],
            "spawn terminal agent",
        );

        assert!(
            !active_tasks.iter().any(|t| t.id == subtask_id),
            "orphaned sub-agent subtask must NOT be re-sent; active task ids: {:?}",
            active_tasks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
        );
        assert!(
            !all_task_ids.iter().any(|id| id == subtask_id),
            "orphaned sub-agent subtask must be pruned from the task store; task ids: {all_task_ids:?}",
        );
        assert!(
            !has_dangling_subagent_pair(find_root_task(&active_tasks, root_task_id)),
            "root must not contain a dangling sub-agent tool_call after rewind"
        );
    });
}

/// STRADDLE: a (terminal-use) sub-agent whose spawning ToolCall is BEFORE the
/// rewind point but whose ToolCallResult lands in a LATER turn that gets
/// rewound. The fix removes BOTH halves from the root and prunes the subtask,
/// so it is neither re-sent nor left as a dangling tool_use.
#[test]
fn rewind_straddle_subagent_call_kept_result_removed_does_not_resend_subtask() {
    use crate::test_util::ai_agent_tasks::{create_api_subtask, create_api_task};

    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let root_task_id = "root-task";
        let subtask_id = "sub-1";

        let (subagent_call, subagent_tool_call_id) =
            make_subagent_call("m2", root_task_id, subtask_id, "req-1", None);

        let root_task = create_api_task(
            root_task_id,
            vec![
                // Turn 1 (kept): spawns the terminal sub-agent.
                create_user_query_message("m1", root_task_id, "req-1", "spawn terminal agent"),
                subagent_call,
                // Turn 2 (rewound): the sub-agent's result arrives here.
                create_user_query_message("m3", root_task_id, "req-2", "continue"),
                subagent_tool_call_result_message(
                    "m4",
                    root_task_id,
                    &subagent_tool_call_id,
                    "req-2",
                ),
            ],
        );
        let subtask = create_api_subtask(
            subtask_id,
            root_task_id,
            vec![agent_output_message(
                "s1",
                subtask_id,
                "req-1",
                "terminal work",
            )],
        );

        let (active_tasks, all_task_ids) =
            restore_truncate_and_collect(&mut app, root_task, vec![subtask], "continue");

        assert!(
            !active_tasks.iter().any(|t| t.id == subtask_id),
            "straddle: subtask must NOT be re-sent after rewind; active task ids: {:?}",
            active_tasks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
        );
        assert!(
            !all_task_ids.iter().any(|id| id == subtask_id),
            "straddle: subtask must be pruned from the task store; task ids: {all_task_ids:?}",
        );
        assert!(
            !has_dangling_subagent_pair(find_root_task(&active_tasks, root_task_id)),
            "straddle: root must not retain a dangling sub-agent tool_call after rewind"
        );
    });
}

/// Timing (b): a sub-agent whose call AND result are BOTH before the rewind
/// point must be preserved (valid history) — the subtask stays in the store and
/// the root keeps both halves.
#[test]
fn rewind_preserves_subagent_fully_before_rewind_point() {
    use crate::test_util::ai_agent_tasks::{create_api_subtask, create_api_task};

    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let root_task_id = "root-task";
        let subtask_id = "sub-1";

        let (subagent_call, subagent_tool_call_id) =
            make_subagent_call("m2", root_task_id, subtask_id, "req-1", None);

        let root_task = create_api_task(
            root_task_id,
            vec![
                // Turn 1 (kept): sub-agent runs and finishes entirely.
                create_user_query_message("m1", root_task_id, "req-1", "do work"),
                subagent_call,
                subagent_tool_call_result_message(
                    "m3",
                    root_task_id,
                    &subagent_tool_call_id,
                    "req-1",
                ),
                agent_output_message("m4", root_task_id, "req-1", "done"),
                // Turn 2 (rewound).
                create_user_query_message("m5", root_task_id, "req-2", "continue"),
                agent_output_message("m6", root_task_id, "req-2", "more"),
            ],
        );
        let subtask = create_api_subtask(
            subtask_id,
            root_task_id,
            vec![agent_output_message(
                "s1",
                subtask_id,
                "req-1",
                "terminal work",
            )],
        );

        let (active_tasks, all_task_ids) =
            restore_truncate_and_collect(&mut app, root_task, vec![subtask], "continue");

        assert!(
            all_task_ids.iter().any(|id| id == subtask_id),
            "sub-agent fully before the rewind point must be preserved in the task store; task ids: {all_task_ids:?}",
        );
        let root = find_root_task(&active_tasks, root_task_id);
        assert!(
            !has_dangling_subagent_pair(root),
            "a preserved finished sub-agent must keep both its call and result in the root"
        );
    });
}

/// Summarization is implemented as a sub-agent (`MoveMessagesToNewTask`) that
/// relocates earlier conversation messages — including user queries — into a
/// summary subtask. If a rewind removes the summarization result while keeping
/// its call, the summary subtask would otherwise become "unfinished" and be
/// re-sent, dragging the summarized-away user query back into the request. The
/// fix removes both halves and prunes the summary subtask.
#[test]
fn rewind_removes_summarized_away_user_query_from_next_request() {
    use crate::test_util::ai_agent_tasks::{create_api_subtask, create_api_task};

    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let root_task_id = "root-task";
        let summary_subtask_id = "summary-sub";
        let summarized_away = "summarized away question";

        let (summarization_call, summarization_tool_call_id) = make_subagent_call(
            "m_sum",
            root_task_id,
            summary_subtask_id,
            "req-1",
            Some(api::message::tool_call::subagent::Metadata::Summarization(
                (),
            )),
        );

        let root_task = create_api_task(
            root_task_id,
            vec![
                // Turn 1 (kept): summarization runs and moves the earlier query away.
                create_user_query_message("m1", root_task_id, "req-1", "start"),
                summarization_call,
                // Turn 2 (rewound): the summarization result lands here.
                create_user_query_message("m2", root_task_id, "req-2", "continue"),
                subagent_tool_call_result_message(
                    "m3",
                    root_task_id,
                    &summarization_tool_call_id,
                    "req-2",
                ),
            ],
        );
        let summary_subtask = create_api_subtask(
            summary_subtask_id,
            root_task_id,
            vec![create_user_query_message(
                "s1",
                summary_subtask_id,
                "req-0",
                summarized_away,
            )],
        );

        let (active_tasks, all_task_ids) =
            restore_truncate_and_collect(&mut app, root_task, vec![summary_subtask], "continue");

        let queries = user_queries_in_tasks(&active_tasks);
        assert!(
            !queries.iter().any(|q| q == summarized_away),
            "the summarized-away user query must not be dragged back into the request; queries: {queries:?}",
        );
        assert!(
            !all_task_ids.iter().any(|id| id == summary_subtask_id),
            "the summary subtask must be pruned from the task store; task ids: {all_task_ids:?}",
        );
        assert!(
            !has_dangling_subagent_pair(find_root_task(&active_tasks, root_task_id)),
            "root must not retain a dangling summarization tool_call after rewind"
        );
    });
}

/// DURABILITY + MULTI-TURN: after a straddle rewind, the repair lives in the
/// task store, so the first post-rewind request (A), a follow-up request (B),
/// AND a persist -> `new_restored` round-trip are all clean (no re-sent
/// subtask, no dangling sub-agent tool_call).
#[test]
fn straddle_rewind_followup_requests_are_clean_and_durable() {
    use crate::ai::agent::conversation::AIConversation;
    use crate::ai::agent::task::TaskId;
    use crate::test_util::ai_agent_tasks::{create_api_subtask, create_api_task};

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let (sender, receiver) = std::sync::mpsc::sync_channel(16);
        let mut global_resource_handles = GlobalResourceHandles::mock(&mut app);
        global_resource_handles.model_event_sender = Some(sender);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));

        let terminal_view_id = EntityId::new();
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let conversation_id = AIConversationId::new();
        let root_task_id = "root-task";
        let subtask_id = "sub-1";

        let (subagent_call, subagent_tool_call_id) =
            make_subagent_call("m2", root_task_id, subtask_id, "req-1", None);

        let root_task = create_api_task(
            root_task_id,
            vec![
                create_user_query_message("m1", root_task_id, "req-1", "spawn terminal agent"),
                subagent_call,
                create_user_query_message("m3", root_task_id, "req-2", "continue"),
                subagent_tool_call_result_message(
                    "m4",
                    root_task_id,
                    &subagent_tool_call_id,
                    "req-2",
                ),
            ],
        );
        let subtask = create_api_subtask(
            subtask_id,
            root_task_id,
            vec![agent_output_message(
                "s1",
                subtask_id,
                "req-1",
                "terminal work",
            )],
        );

        // Adaptation: the pinned oracle spells out an `AgentConversationData`
        // literal. This fork's struct has a different (cloud-free) field set,
        // so the shared `empty_agent_conversation_data_for_test()` helper is
        // used and only the server token — the one field the oracle sets to a
        // non-default value — is overridden.
        let conversation_data = AgentConversationData {
            is_remote_child: false,
            server_conversation_token: Some("token-1".to_string()),
            ..empty_agent_conversation_data_for_test()
        };
        let conversation = AIConversation::new_restored(
            conversation_id,
            vec![root_task, subtask],
            Some(conversation_data),
        )
        .expect("conversation should build");
        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
        });

        let rewind_exchange_id = history_model.read(&app, |model, _| {
            model
                .conversation(&conversation_id)
                .unwrap()
                .root_task_exchanges()
                .find(|e| {
                    e.input.iter().any(|i| {
                        matches!(i, AIAgentInput::UserQuery { query, .. } if query == "continue")
                    })
                })
                .map(|e| e.id)
                .unwrap()
        });
        history_model.update(&mut app, |model, ctx| {
            model
                .truncate_conversation_from_exchange(conversation_id, rewind_exchange_id, ctx)
                .unwrap();
        });

        // Request A (first post-rewind send) is clean.
        let active_a = history_model.read(&app, |model, _| {
            model
                .conversation(&conversation_id)
                .unwrap()
                .compute_active_tasks()
        });
        assert!(
            !active_a.iter().any(|t| t.id == subtask_id),
            "request A must not re-send the subtask"
        );
        assert!(
            !has_dangling_subagent_pair(find_root_task(&active_a, root_task_id)),
            "request A root must not contain a dangling sub-agent tool_call"
        );

        // Durable: capture the persisted snapshot from the rewind and restore
        // it. The persist is dispatched from a spawned future, so block until
        // the `UpdateMultiAgentConversation` event arrives.
        let restored_tasks: Vec<api::Task> = loop {
            match receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(ModelEvent::UpdateMultiAgentConversation { updated_tasks, .. }) => {
                    break updated_tasks;
                }
                Ok(_) => continue,
                Err(_) => panic!("rewind must persist a task snapshot"),
            }
        };
        assert!(
            restored_tasks.iter().any(|t| t.id == root_task_id),
            "persisted snapshot must contain the root task"
        );
        assert!(
            !restored_tasks.iter().any(|t| t.id == subtask_id),
            "persisted snapshot must not contain the pruned subtask: {:?}",
            restored_tasks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
        );
        let restored = AIConversation::new_restored(AIConversationId::new(), restored_tasks, None)
            .expect("restore from rewind snapshot should succeed");
        let restored_active = restored.compute_active_tasks();
        assert!(
            !restored_active.iter().any(|t| t.id == subtask_id),
            "restored conversation must not re-send the subtask"
        );
        assert!(
            !has_dangling_subagent_pair(find_root_task(&restored_active, root_task_id)),
            "restored conversation root must not contain a dangling sub-agent tool_call"
        );

        // Request B (follow-up) is also clean.
        let stream_id = ResponseStreamId::new_for_test();
        history_model.update(&mut app, |model, ctx| {
            let exchange = create_exchange_with_query("follow up B", Local::now(), None);
            let request_input = RequestInput {
                conversation_id,
                input_messages: HashMap::from([(
                    TaskId::new(root_task_id.to_string()),
                    exchange.input,
                )]),
                working_directory: exchange.working_directory,
                model_id: exchange.model_id,
                coding_model_id: exchange.coding_model_id,
                cli_agent_model_id: exchange.cli_agent_model_id,
                computer_use_model_id: exchange.computer_use_model_id,
                shared_session_response_initiator: exchange.response_initiator,
                request_start_ts: exchange.start_time,
                supported_tools_override: None,
            };
            model
                .update_conversation_for_new_request_input(
                    request_input,
                    stream_id,
                    terminal_view_id,
                    ctx,
                )
                .unwrap();
        });
        let active_b = history_model.read(&app, |model, _| {
            model
                .conversation(&conversation_id)
                .unwrap()
                .compute_active_tasks()
        });
        assert!(
            !active_b.iter().any(|t| t.id == subtask_id),
            "follow-up request B must not re-send the subtask"
        );
        assert!(
            !has_dangling_subagent_pair(find_root_task(&active_b, root_task_id)),
            "follow-up request B root must not contain a dangling sub-agent tool_call"
        );
    });
}

// --- fork-from-here (exact-exchange) dangling tool_use reconciliation ---

/// Builds a regular (non-sub-agent) `run_shell_command` tool_call message with
/// `request_id` set and `tool_call_id`.
fn regular_tool_call_message(
    id: &str,
    task_id: &str,
    tool_call_id: &str,
    request_id: &str,
) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::ToolCall(api::message::ToolCall {
            tool_call_id: tool_call_id.to_string(),
            tool: Some(api::message::tool_call::Tool::RunShellCommand(
                api::message::tool_call::RunShellCommand {
                    command: "echo hi".to_string(),
                    ..Default::default()
                },
            )),
        })),
        request_id: request_id.to_string(),
        timestamp: None,
    }
}

/// Builds a real (non-cancel) `run_shell_command` tool_call_result message.
fn regular_tool_call_result_message(
    id: &str,
    task_id: &str,
    tool_call_id: &str,
    request_id: &str,
) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::ToolCallResult(
            api::message::ToolCallResult {
                tool_call_id: tool_call_id.to_string(),
                context: None,
                result: Some(api::message::tool_call_result::Result::RunShellCommand(
                    api::RunShellCommandResult::default(),
                )),
            },
        )),
        request_id: request_id.to_string(),
        timestamp: None,
    }
}

/// Builds a server-handled tool_call message (like the `RunPrimaryAgent`
/// bootstrap call at the start of every root task), which never receives a
/// result by design.
fn server_tool_call_message(
    id: &str,
    task_id: &str,
    tool_call_id: &str,
    request_id: &str,
) -> api::Message {
    api::Message {
        id: id.to_string(),
        task_id: task_id.to_string(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(api::message::Message::ToolCall(api::message::ToolCall {
            tool_call_id: tool_call_id.to_string(),
            tool: Some(api::message::tool_call::Tool::Server(
                api::message::tool_call::Server {
                    payload: String::new(),
                },
            )),
        })),
        request_id: request_id.to_string(),
        timestamp: None,
    }
}

/// Returns the non-empty forked root task from a fork result.
fn forked_root_task(forked: &crate::ai::agent::conversation::AIConversation) -> &api::Task {
    forked
        .all_tasks()
        .filter_map(|t| t.source())
        .find(|t| !t.messages.is_empty())
        .expect("forked root task exists")
}

/// Restores `source` into the history model and returns the exchange id whose
/// input is the given user query.
fn restore_and_find_exchange(
    app: &mut App,
    history_model: &warpui::ModelHandle<BlocklistAIHistoryModel>,
    source: crate::ai::agent::conversation::AIConversation,
    query: &str,
) -> (AIConversationId, AIAgentExchangeId) {
    let terminal_view_id = EntityId::new();
    let source_id = source.id();
    history_model.update(app, |model, ctx| {
        model.restore_conversations(terminal_view_id, vec![source], ctx);
    });
    let exchange_id = history_model.read(app, |model, _| {
        model
            .conversation(&source_id)
            .unwrap()
            .root_task_exchanges()
            .find(|e| {
                e.input
                    .iter()
                    .any(|i| matches!(i, AIAgentInput::UserQuery { query: q, .. } if q == query))
            })
            .map(|e| e.id)
            .expect("exchange for query should exist")
    });
    (source_id, exchange_id)
}

/// Wires up the sqlite sender the fork path requires. Returns the receiver,
/// which the caller must keep alive so the bounded channel stays open (a
/// dropped receiver makes the fork's persist `send` fail).
fn install_mock_model_event_sender(app: &mut App) -> std::sync::mpsc::Receiver<ModelEvent> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    let mut global_resource_handles = GlobalResourceHandles::mock(app);
    global_resource_handles.model_event_sender = Some(sender);
    app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));
    receiver
}

/// Forking at an exact exchange reconciles exactly the client tool_calls in
/// the fork-point exchange: a completed call gets its REAL result pulled
/// forward from the source, an in-flight call gets a synthesized `Cancel`
/// right after the call, and unresolved server tool calls plus danglers from
/// earlier exchanges are left untouched.
#[test]
fn fork_exact_reconciles_fork_point_client_tool_calls() {
    use crate::ai::agent::conversation::AIConversation;
    use crate::ai::agent::task::helper::MessageExt;
    use crate::test_util::ai_agent_tasks::create_api_task;

    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let _receiver = install_mock_model_event_sender(&mut app);
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &[]));

        let root_task_id = "root-task";
        let orphan_id = "toolu_orphan";
        let server_id = "toolu_server";
        let completed_id = "toolu_completed";
        let inflight_id = "toolu_inflight";
        // req-1 holds a client tool_call that never received a result (a
        // pre-existing dangler). The fork point (req-2) holds an unresolved
        // server tool_call plus two client tool_calls: one whose real result
        // lives in req-3 (truncated by the fork) and one that never receives a
        // result (in-flight).
        let root_task = create_api_task(
            root_task_id,
            vec![
                create_user_query_message("m1", root_task_id, "req-1", "first"),
                regular_tool_call_message("m2", root_task_id, orphan_id, "req-1"),
                create_user_query_message("m3", root_task_id, "req-2", "second"),
                server_tool_call_message("m4", root_task_id, server_id, "req-2"),
                regular_tool_call_message("m5", root_task_id, completed_id, "req-2"),
                regular_tool_call_message("m6", root_task_id, inflight_id, "req-2"),
                regular_tool_call_result_message("m7", root_task_id, completed_id, "req-3"),
                agent_output_message("m8", root_task_id, "req-3", "done"),
            ],
        );
        let source = AIConversation::new_restored(AIConversationId::new(), vec![root_task], None)
            .expect("source conversation should build");

        let (source_id, exchange_id) =
            restore_and_find_exchange(&mut app, &history_model, source, "second");

        let forked = history_model.update(&mut app, |model, ctx| {
            let source = model.conversation(&source_id).unwrap().clone();
            // Adaptation: this fork's `fork_conversation_at_exchange` has no
            // `title_override` parameter (it is used only by Warp's cloud
            // hand-off path), so the trailing `None` is dropped.
            model
                .fork_conversation_at_exchange(&source, exchange_id, true, "[Fork] ", ctx)
                .expect("fork should succeed")
        });

        let root = forked_root_task(&forked);
        let result_for = |tool_call_id: &str| {
            root.messages.iter().find(|m| {
                m.tool_call_result()
                    .is_some_and(|r| r.tool_call_id == tool_call_id)
            })
        };

        // Completed call: the REAL result (m7) is pulled forward, not a Cancel.
        let completed = result_for(completed_id).expect("completed tool_call must be paired");
        assert_eq!(
            completed.id, "m7",
            "the REAL result message should be pulled forward"
        );
        assert!(
            matches!(
                completed.tool_call_result().and_then(|r| r.result.as_ref()),
                Some(api::message::tool_call_result::Result::RunShellCommand(_))
            ),
            "the pulled-forward result must be the real run_shell_command result, not a Cancel"
        );

        // In-flight call: a Cancel is synthesized immediately after the call,
        // carrying the call's request_id.
        let inflight =
            result_for(inflight_id).expect("in-flight tool_call must be paired with a Cancel");
        assert!(
            matches!(
                inflight.tool_call_result().and_then(|r| r.result.as_ref()),
                Some(api::message::tool_call_result::Result::Cancel(_))
            ),
            "in-flight tool_call must fall back to a Cancel result"
        );
        let call_idx = root.messages.iter().position(|m| m.id == "m6").unwrap();
        let next = &root.messages[call_idx + 1];
        assert_eq!(
            next.id, inflight.id,
            "Cancel must immediately follow its tool_call"
        );
        assert_eq!(
            next.request_id, "req-2",
            "Cancel carries the call's request_id"
        );

        // The unresolved server tool_call must stay unresolved: a synthesized
        // Cancel would pop an agent off the server's run stack on restore.
        assert!(root.messages.iter().any(|m| m.id == "m4"));
        assert!(
            result_for(server_id).is_none(),
            "no result may be synthesized for a server tool_call"
        );

        // The pre-existing dangler outside the fork point is reproduced as-is.
        assert!(root.messages.iter().any(|m| m.id == "m2"));
        assert!(
            result_for(orphan_id).is_none(),
            "no result may be synthesized outside the fork-point exchange"
        );
    });
}

/// A newly started conversation in a terminal view that previously held a
/// `WaitingForEvents` conversation does not inherit the waiting state.
/// Each fresh `start_new_conversation` begins in the default
/// in-progress-ready state.
///
/// Note: this covers only the history-model invariant. Warp's `WaitForEvents`
/// *action* and its orchestration executor are a deliberate keep-dropped
/// decision in this fork; the status is driven here through the ordinary
/// `update_conversation_status` path instead.
#[test]
fn test_new_conversation_does_not_inherit_waiting_for_events() {
    App::test((), |mut app| async move {
        initialize_history_model_test_app(&mut app);
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let terminal_view_id = EntityId::new();

        // First conversation enters the waiting state via the normal
        // status-update path.
        let first_id = history_model.update(&mut app, |model, ctx| {
            let id = model.start_new_conversation(terminal_view_id, false, false, ctx);
            model.update_conversation_status(
                terminal_view_id,
                id,
                ConversationStatus::WaitingForEvents,
                ctx,
            );
            id
        });
        history_model.read(&app, |model, _| {
            let first = model.conversation(&first_id).expect("first should exist");
            assert!(matches!(
                first.status(),
                ConversationStatus::WaitingForEvents
            ));
        });

        // Starting a new conversation in the same terminal view must not
        // copy the waiting state forward.
        let second_id = history_model.update(&mut app, |model, ctx| {
            model.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        assert_ne!(
            first_id, second_id,
            "a fresh conversation should have a distinct id",
        );
        history_model.read(&app, |model, _| {
            let second = model.conversation(&second_id).expect("second should exist");
            assert!(
                !matches!(second.status(), ConversationStatus::WaitingForEvents),
                "a newly started conversation must not start in WaitingForEvents",
            );
        });
    });
}

#[test]
fn test_initialize_historical_conversations_uses_root_task_description_title() {
    App::test((), |app| async move {
        let conversation_id = AIConversationId::new();
        let now = Utc::now().naive_utc();
        let task_id = format!("task-{conversation_id}");
        let conversations = vec![AgentConversation {
            conversation: AgentConversationRecord {
                id: 0,
                conversation_id: conversation_id.to_string(),
                // Adaptation: this fork's `AgentConversationData` has a
                // different (cloud-free) field set, so the shared
                // `empty_agent_conversation_data_for_test()` helper stands in
                // for the oracle's struct literal; only the server token is
                // overridden, as in the oracle.
                conversation_data: serde_json::to_string(&AgentConversationData {
                    is_remote_child: false,
                    server_conversation_token: Some("renamed-title-token".to_string()),
                    ..empty_agent_conversation_data_for_test()
                })
                .expect("conversation data should serialize"),
                last_modified_at: now,
                summary: None,
            },
            tasks: vec![api::Task {
                id: task_id.clone(),
                messages: vec![create_user_query_message(
                    "message-1",
                    &task_id,
                    "request-1",
                    "Initial query",
                )],
                dependencies: None,
                description: "Renamed root title".to_string(),
                summary: String::new(),
                server_data: String::new(),
            }],
        }];

        let history_model =
            app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], &conversations));

        history_model.read(&app, |model, _| {
            let metadata = model
                .get_conversation_metadata(&conversation_id)
                .expect("conversation metadata should be initialized");
            assert_eq!(metadata.title, "Renamed root title");
            assert_eq!(metadata.initial_query, "Initial query");
        });
    });
}

#[test]
fn todo_projections_delegate_to_the_conversation() {
    App::test((), |mut app| async move {
        // Adaptation: the pinned oracle relies on the ambient test app; this
        // fork's `start_new_conversation` reaches the settings singletons, so
        // the shared test-app initializer runs first.
        initialize_history_model_test_app(&mut app);
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let terminal_view_id = EntityId::new();
        let conversation_id = history_model.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let completed = AIAgentTodo::new("t1".to_owned().into(), "one".to_owned(), String::new());
        let pending_first =
            AIAgentTodo::new("t2".to_owned().into(), "two".to_owned(), String::new());
        let pending_second =
            AIAgentTodo::new("t3".to_owned().into(), "three".to_owned(), String::new());
        history_model.update(&mut app, |history, _| {
            history
                .conversation_mut(&conversation_id)
                .expect("conversation exists")
                .set_todo_lists_for_test(vec![
                    AIAgentTodoList::default()
                        .with_completed_items(vec![completed.clone()])
                        .with_pending_items(vec![pending_first, pending_second.clone()]),
                ]);
        });

        history_model.read(&app, |history, _| {
            assert_eq!(
                history.todo_status(&conversation_id, &completed.id),
                Some(TodoStatus::Completed)
            );
            // Non-head pending items are Pending regardless of conversation status.
            assert_eq!(
                history.todo_status(&conversation_id, &pending_second.id),
                Some(TodoStatus::Pending)
            );
            assert_eq!(
                history.todo_status(&conversation_id, &AIAgentTodoId::from("missing".to_owned())),
                None
            );
            assert_eq!(
                history
                    .active_todo_list(&conversation_id)
                    .map(AIAgentTodoList::len),
                Some(3)
            );
            // Unknown conversations yield no projections at all.
            let unknown = AIConversationId::new();
            assert_eq!(history.todo_status(&unknown, &completed.id), None);
            assert!(history.active_todo_list(&unknown).is_none());
        });
    });
}
