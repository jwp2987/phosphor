use super::*;
use crate::persistence::model::{AgentConversation, AgentConversationRecord};

fn persisted_conversation(conversation_id: AIConversationId) -> AgentConversation {
    let task_id = format!("task-{conversation_id}");
    AgentConversation {
        conversation: AgentConversationRecord {
            id: 0,
            conversation_id: conversation_id.to_string(),
            conversation_data: r#"{"server_conversation_token":null}"#.to_string(),
            last_modified_at: chrono::NaiveDateTime::default(),
            summary: None,
        },
        tasks: vec![warp_multi_agent_api::Task {
            id: task_id,
            messages: vec![],
            dependencies: None,
            description: "Test conversation".to_string(),
            summary: String::new(),
            server_data: String::new(),
        }],
    }
}

fn ai_conversation(conversation_id: AIConversationId) -> AIConversation {
    convert_persisted_conversation_to_ai_conversation_with_metadata(persisted_conversation(
        conversation_id,
    ))
    .expect("test conversation should convert")
}

#[test]
fn take_conversation_hands_out_each_conversation_at_most_once() {
    let conversation_id = AIConversationId::new();
    let mut store =
        RestoredAgentConversations::new_seeded(vec![persisted_conversation(conversation_id)]);

    assert!(store.take_conversation(&conversation_id).is_some());
    assert!(
        store.take_conversation(&conversation_id).is_none(),
        "a taken conversation must not be handed out again"
    );
    assert!(
        store.get_conversation(&conversation_id).is_none(),
        "a taken conversation must not be readable either"
    );
}

#[test]
fn failed_take_does_not_consume_the_restore_opportunity() {
    let conversation_id = AIConversationId::new();
    // No seed and no database: the first take fails to load.
    let mut store = RestoredAgentConversations::new_seeded(vec![]);
    assert!(store.take_conversation(&conversation_id).is_none());

    // Once the conversation becomes available (e.g. the earlier failure was
    // transient), a retry must still succeed — a failed load must not have
    // marked the ID as taken.
    store
        .conversations
        .insert(conversation_id, ai_conversation(conversation_id));
    assert!(
        store.take_conversation(&conversation_id).is_some(),
        "a failed load must not permanently consume the restore"
    );
    assert!(store.take_conversation(&conversation_id).is_none());
}

/// Regression tests for the startup restore that brought every agent
/// conversation back hollow (zero AI blocks, missing from the conversation
/// history list) even though `agent_conversations` / `agent_tasks` held the full
/// conversation. Startup reads metadata only (`read_agent_conversation_metadata`
/// returns records with EMPTY `tasks`), and the restore store used to convert
/// those records eagerly; the lenient local-DB conversion then synthesized an
/// empty optimistic root instead of reading the persisted task.
#[cfg(feature = "local_fs")]
mod startup_restore {
    use diesel::{Connection as _, SqliteConnection};
    use diesel_migrations::MigrationHarness as _;
    use warp_multi_agent_api as api;

    use super::*;
    use crate::ai::agent::{AIAgentActionResultType, AIAgentInput};
    use crate::ai::blocklist::BlocklistAIHistoryModel;
    use crate::persistence::agent::{read_agent_conversation_metadata, upsert_agent_conversation};
    use crate::persistence::model::AgentConversationData;
    use crate::terminal::view::blocklist_filter::{
        conversation_would_render_in_blocklist, exchanges_for_blocklist,
    };

    const INITIAL_QUERY: &str = "Tiny plan: create hello.txt containing hi, then cat it.";
    const DOCUMENT_ID: &str = "91ad819d-4214-44a3-9ded-bcfbecd0c069";

    /// `conversation_data` as a BYOP run writes it: an EMPTY (not null) server
    /// token, zeroed usage metadata, and a local PLAN artifact. Copied from a
    /// real profile that reproduced the bug.
    const BYOP_CONVERSATION_DATA: &str = r#"{"server_conversation_token":"","conversation_usage_metadata":{"was_summarized":false,"context_window_usage":0.0,"credits_spent":0.0,"credits_spent_for_last_block":null,"token_usage":[],"tool_usage_metadata":{"run_command_stats":{"count":0,"commands_executed":0},"read_files_stats":{"count":0},"grep_stats":{"count":0},"file_glob_stats":{"count":0},"apply_file_diff_stats":{"count":0,"lines_added":0,"lines_removed":0,"files_changed":0},"write_to_long_running_shell_command_stats":{"count":0},"read_mcp_resource_stats":{"count":0},"call_mcp_tool_stats":{"count":0},"suggest_plan_stats":{"count":0},"suggest_create_plan_stats":{"count":0},"read_shell_command_output_stats":{"count":0},"use_computer_stats":{"count":0}}},"artifacts_json":"[{\"artifact_type\":\"PLAN\",\"data\":{\"document_uid\":\"91ad819d-4214-44a3-9ded-bcfbecd0c069\",\"notebook_uid\":null,\"title\":\"Plan: create hello.txt and cat it\"}}]","autoexecute_override":"RespectUserSettings"}"#;

    fn test_connection() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:")
            .expect("in-memory sqlite connection should open");
        conn.run_pending_migrations(::persistence::MIGRATIONS)
            .expect("migrations should run");
        conn
    }

    fn message(
        id: &str,
        task_id: &str,
        request_id: &str,
        message: api::message::Message,
    ) -> api::Message {
        api::Message {
            id: id.to_string(),
            task_id: task_id.to_string(),
            request_id: request_id.to_string(),
            message: Some(message),
            ..Default::default()
        }
    }

    fn user_query(query: &str) -> api::message::Message {
        api::message::Message::UserQuery(api::message::UserQuery {
            query: query.to_string(),
            ..Default::default()
        })
    }

    fn agent_output(text: &str) -> api::message::Message {
        api::message::Message::AgentOutput(api::message::AgentOutput {
            text: text.to_string(),
        })
    }

    /// The root task of the reproducing conversation: a `/plan` exchange, then a
    /// follow-up that made the agent call `create_documents`. The tool result
    /// carries a BYOP-local `byop-preflight:` request id, as BYOP writes it.
    fn byop_root_task(task_id: &str) -> api::Task {
        let tool_call_id = "toolu_015w3DCRHwzjxDrg48LurFm4";
        api::Task {
            id: task_id.to_string(),
            description: "hello.txt create and cat plan".to_string(),
            dependencies: None,
            summary: String::new(),
            server_data: String::new(),
            messages: vec![
                message("m1", task_id, "req-1", user_query(INITIAL_QUERY)),
                message(
                    "m2",
                    task_id,
                    "req-1",
                    agent_output("**Goal:** create hello.txt"),
                ),
                message(
                    "m3",
                    task_id,
                    "req-2",
                    user_query("Now save that exact plan as a plan document."),
                ),
                message(
                    "m4",
                    task_id,
                    "req-2",
                    api::message::Message::ToolCall(api::message::ToolCall {
                        tool_call_id: tool_call_id.to_string(),
                        tool: Some(api::message::tool_call::Tool::CreateDocuments(
                            api::message::tool_call::CreateDocuments {
                                new_documents: vec![
                                    api::message::tool_call::create_documents::NewDocument {
                                        title: "Plan: create hello.txt and cat it".to_string(),
                                        content: "**Goal:** create hello.txt".to_string(),
                                    },
                                ],
                            },
                        )),
                    }),
                ),
                message(
                    "m5",
                    task_id,
                    "byop-preflight:1eabf6eb-7e98-4378-a62e-0b2fac139bbd",
                    api::message::Message::ToolCallResult(api::message::ToolCallResult {
                        tool_call_id: tool_call_id.to_string(),
                        context: None,
                        result: Some(api::message::tool_call_result::Result::CreateDocuments(
                            api::CreateDocumentsResult {
                                result: Some(api::create_documents_result::Result::Success(
                                    api::create_documents_result::Success {
                                        created_documents: vec![api::DocumentContent {
                                            document_id: DOCUMENT_ID.to_string(),
                                            content: "**Goal:** create hello.txt".to_string(),
                                            line_range: None,
                                        }],
                                    },
                                )),
                            },
                        )),
                    }),
                ),
                message(
                    "m6",
                    task_id,
                    "req-4",
                    agent_output("Saved the plan document."),
                ),
            ],
        }
    }

    /// Persists the BYOP-shaped conversation and returns the connection plus
    /// the records exactly as startup reads them.
    fn persist_byop_conversation(
        conversation_id: AIConversationId,
    ) -> (SqliteConnection, Vec<AgentConversation>) {
        let mut conn = test_connection();
        let task = byop_root_task("54ad35d7-3c14-470e-b981-6bbe42a22bcb");
        let conversation_data: AgentConversationData = serde_json::from_str(BYOP_CONVERSATION_DATA)
            .expect("BYOP conversation data should deserialize");
        upsert_agent_conversation(
            &mut conn,
            &conversation_id.to_string(),
            [&task],
            conversation_data,
        )
        .expect("upsert should succeed");

        let (startup_records, backfills) =
            read_agent_conversation_metadata(&mut conn).expect("metadata read should succeed");
        assert!(backfills.is_empty());
        assert_eq!(startup_records.len(), 1);
        assert!(
            startup_records[0].tasks.is_empty(),
            "precondition: startup reads metadata only, so records carry no tasks"
        );
        (conn, startup_records)
    }

    /// Documents why the store must not convert startup records itself:
    /// converting a metadata-only record yields a hollow conversation.
    #[test]
    fn converting_a_metadata_only_record_yields_a_hollow_conversation() {
        let conversation_id = AIConversationId::new();
        let (_conn, startup_records) = persist_byop_conversation(conversation_id);

        let hollow = convert_persisted_conversation_to_ai_conversation_with_metadata(
            startup_records[0].clone(),
        )
        .expect("lenient conversion synthesizes a root for an empty task list");
        assert_eq!(hollow.exchange_count(), 0);
        assert!(!conversation_would_render_in_blocklist(&hollow));
    }

    #[test]
    fn startup_restore_loads_byop_conversation_with_its_blocks() {
        let conversation_id = AIConversationId::new();
        let (conn, _startup_records) = persist_byop_conversation(conversation_id);
        let mut store = RestoredAgentConversations::with_db_connection(conn);

        // Pane restoration first filters on `get_conversation`, then takes.
        let peeked = store
            .get_conversation(&conversation_id)
            .expect("the persisted conversation should load from sqlite");
        assert!(peeked.all_tasks().next().is_some());
        assert!(!peeked.is_entirely_passive());

        let restored = store
            .take_conversation(&conversation_id)
            .expect("the persisted conversation should be restorable");
        assert_eq!(restored.id(), conversation_id);

        // One exchange per request id: req-1, req-2, byop-preflight, req-4.
        assert_eq!(restored.exchange_count(), 4);
        assert_eq!(
            exchanges_for_blocklist(&restored).len(),
            4,
            "every restored exchange should become an AI block"
        );
        assert!(
            conversation_would_render_in_blocklist(&restored),
            "a non-rendering restored conversation shadows its history-list entry"
        );

        let first_exchange = restored
            .first_exchange()
            .expect("restored conversation should have a first exchange");
        assert!(
            first_exchange.input.iter().any(|input| matches!(
                input,
                AIAgentInput::UserQuery { query, .. } if query == INITIAL_QUERY
            )),
            "first exchange should carry the initial query"
        );
        assert!(
            restored.all_exchanges().iter().any(|exchange| {
                exchange.input.iter().any(|input| {
                    input.action_result().is_some_and(|result| {
                        matches!(result.result, AIAgentActionResultType::CreateDocuments(_))
                    })
                })
            }),
            "the create_documents result should be restored"
        );

        assert!(
            store.take_conversation(&conversation_id).is_none(),
            "a restored conversation is handed out at most once"
        );
    }

    #[test]
    fn startup_records_list_byop_conversation_in_history() {
        let conversation_id = AIConversationId::new();
        let (_conn, startup_records) = persist_byop_conversation(conversation_id);

        let history = BlocklistAIHistoryModel::new(vec![], vec![], &startup_records);
        let listed = history
            .get_local_conversations_metadata()
            .find(|metadata| metadata.id == conversation_id)
            .expect("the BYOP conversation should be in the history list's data source");
        assert_eq!(listed.initial_query, INITIAL_QUERY);
    }
}
