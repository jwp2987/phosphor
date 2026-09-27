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
    use diesel::{
        Connection as _, ExpressionMethods as _, QueryDsl as _, RunQueryDsl as _, SqliteConnection,
    };
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
    const ROOT_TASK_ID: &str = "54ad35d7-3c14-470e-b981-6bbe42a22bcb";

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
        let task = byop_root_task(ROOT_TASK_ID);
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

    /// The `agent_tasks` ids persisted for `conversation_id`, sorted.
    fn persisted_task_ids(
        conn: &mut SqliteConnection,
        conversation_id: AIConversationId,
    ) -> Vec<String> {
        use crate::persistence::schema::agent_tasks::dsl;
        let mut ids: Vec<String> = dsl::agent_tasks
            .filter(dsl::conversation_id.eq(conversation_id.to_string()))
            .select(dsl::task_id)
            .load::<String>(conn)
            .expect("agent_tasks should be readable");
        ids.sort();
        ids
    }

    /// The history list's view of the database: what startup would list for
    /// `conversation_id`, if anything.
    fn listed_initial_query(
        conn: &mut SqliteConnection,
        conversation_id: AIConversationId,
    ) -> Option<String> {
        let (startup_records, _backfills) =
            read_agent_conversation_metadata(conn).expect("metadata read should succeed");
        let history = BlocklistAIHistoryModel::new(vec![], vec![], &startup_records);
        history
            .get_local_conversations_metadata()
            .find(|metadata| metadata.id == conversation_id)
            .map(|metadata| metadata.initial_query.clone())
    }

    /// The model-level harness these tests share: settings, a captured sqlite sender, and
    /// the history model.
    fn history_model_harness(
        app: &mut warpui::App,
    ) -> (
        warpui::ModelHandle<BlocklistAIHistoryModel>,
        std::sync::mpsc::Receiver<crate::persistence::ModelEvent>,
    ) {
        use crate::test_util::settings::initialize_settings_for_tests;
        use crate::{GlobalResourceHandles, GlobalResourceHandlesProvider};

        initialize_settings_for_tests(app);
        let (sender, receiver) = std::sync::mpsc::sync_channel(64);
        let mut global_resource_handles = GlobalResourceHandles::mock(app);
        global_resource_handles.model_event_sender = Some(sender);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));
        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        (history_model, receiver)
    }

    /// A follow-up prompt on `task_id`, as the controller builds it.
    fn follow_up_request_input(
        conversation_id: AIConversationId,
        task_id: crate::ai::agent::task::TaskId,
        query: &str,
    ) -> crate::ai::blocklist::controller::RequestInput {
        use crate::ai::agent::UserQueryMode;
        use crate::ai::llms::LLMId;

        crate::ai::blocklist::controller::RequestInput {
            conversation_id,
            input_messages: std::collections::HashMap::from([(
                task_id,
                vec![AIAgentInput::UserQuery {
                    query: query.to_string(),
                    context: Default::default(),
                    static_query_type: None,
                    referenced_attachments: Default::default(),
                    user_query_mode: UserQueryMode::default(),
                    running_command: None,
                    intended_agent: None,
                }],
            )]),
            working_directory: None,
            model_id: LLMId::from("test-model"),
            coding_model_id: LLMId::from("test-coding-model"),
            cli_agent_model_id: LLMId::from("test-cli-agent-model"),
            computer_use_model_id: LLMId::from("test-computer-use-model"),
            shared_session_response_initiator: None,
            request_start_ts: chrono::Local::now(),
            supported_tools_override: None,
        }
    }

    /// Applies one client action the way a response stream does.
    fn apply_action(
        conversation: &mut AIConversation,
        stream_id: &crate::ai::blocklist::ResponseStreamId,
        terminal_view_id: warpui::EntityId,
        action: api::client_action::Action,
        ctx: &mut warpui::ModelContext<BlocklistAIHistoryModel>,
    ) {
        conversation
            .apply_client_action(
                stream_id,
                terminal_view_id,
                action,
                &ai::skills::SkillPathOrigin::Unavailable,
                ctx,
            )
            .unwrap_or_else(|e| panic!("applying the client action should succeed: {e:?}"));
    }

    /// Replays every queued conversation persist through the sqlite writer's upsert,
    /// exactly as `handle_model_event` does, and returns how many there were.
    fn replay_persists(
        receiver: &std::sync::mpsc::Receiver<crate::persistence::ModelEvent>,
        conn: &mut SqliteConnection,
    ) -> usize {
        use crate::persistence::ModelEvent;
        use crate::persistence::agent::upsert_agent_conversation_with_retention;

        let mut persists = 0;
        while let Ok(event) = receiver.recv_timeout(std::time::Duration::from_millis(500)) {
            let ModelEvent::UpdateMultiAgentConversation {
                conversation_id,
                updated_tasks,
                conversation_data,
                task_retention,
            } = event
            else {
                continue;
            };
            upsert_agent_conversation_with_retention(
                conn,
                &conversation_id,
                &updated_tasks,
                conversation_data,
                task_retention,
            )
            .expect("replaying the persist should succeed");
            persists += 1;
        }
        persists
    }

    /// The reported trigger, in the 0.1.0–0.1.7 shape: the conversation comes back hollow
    /// (converted from its metadata-only startup record), and the user sends a follow-up in
    /// that pane. `update_for_new_request_input` persists at turn start, and the hollow
    /// root contributes nothing to that snapshot. Before the fix that empty persist deleted
    /// every `agent_tasks` row and blanked the summary, dropping the conversation from the
    /// history list.
    ///
    /// Deliberately stops at turn start. If the response then replaced the synthesized
    /// root with a new server root, the next save would prune the original root by id —
    /// ordinary replace semantics, which a snapshot cannot tell apart from a legitimate
    /// root replacement. That continuation is unreachable since `4b0d1300f`: the lazy store
    /// reads the rows, and `read_agent_conversation_by_id` refuses any it cannot decode, so
    /// a hollow conversation only arises when there are no rows to lose.
    #[test]
    fn follow_up_in_hollow_restored_conversation_does_not_wipe_history() {
        warpui::App::test((), |mut app| async move {
            let (history_model, receiver) = history_model_harness(&mut app);
            let conversation_id = AIConversationId::new();
            let (mut conn, startup_records) = persist_byop_conversation(conversation_id);
            let hollow = convert_persisted_conversation_to_ai_conversation_with_metadata(
                startup_records[0].clone(),
            )
            .expect("lenient conversion synthesizes a root for an empty task list");
            assert_eq!(hollow.exchange_count(), 0, "precondition: restored hollow");

            let terminal_view_id = warpui::EntityId::new();
            let stream_id = crate::ai::blocklist::ResponseStreamId::new_for_test();
            history_model.update(&mut app, |history_model, ctx| {
                history_model.restore_conversations(terminal_view_id, vec![hollow], ctx);
                let conversation = history_model
                    .conversation_mut(&conversation_id)
                    .expect("the restored conversation should be live");
                let root_task_id = conversation.get_root_task_id().clone();
                conversation
                    .update_for_new_request_input(
                        follow_up_request_input(
                            conversation_id,
                            root_task_id,
                            "And now delete hello.txt.",
                        ),
                        stream_id.clone(),
                        terminal_view_id,
                        ctx,
                    )
                    .expect("the follow-up should be accepted");
            });

            assert!(
                replay_persists(&receiver, &mut conn) >= 1,
                "precondition: the follow-up persists at turn start",
            );
            assert_eq!(
                persisted_task_ids(&mut conn, conversation_id),
                vec![ROOT_TASK_ID.to_string()],
                "the empty turn-start snapshot must not delete the persisted task",
            );
            assert_eq!(
                listed_initial_query(&mut conn, conversation_id).as_deref(),
                Some(INITIAL_QUERY),
                "the conversation must stay in the history list under its original query",
            );
        });
    }

    /// End to end on the path restore actually takes since `4b0d1300f`: the lazy store
    /// reads the full conversation, the user sends a follow-up and the agent answers with
    /// messages, every persist is replayed into the database, and the conversation is then
    /// restored from the database again. Both the original four exchanges and the
    /// follow-up must come back.
    #[test]
    fn follow_up_after_restore_keeps_both_histories_across_a_restart() {
        const FOLLOW_UP: &str = "And now delete hello.txt.";

        warpui::App::test((), |mut app| async move {
            let (history_model, receiver) = history_model_harness(&mut app);
            let conversation_id = AIConversationId::new();
            let (conn, _startup_records) = persist_byop_conversation(conversation_id);
            let mut store = RestoredAgentConversations::with_db_connection(conn);
            let restored = store
                .take_conversation(&conversation_id)
                .expect("the persisted conversation should restore");
            assert_eq!(restored.exchange_count(), 4);

            let terminal_view_id = warpui::EntityId::new();
            let stream_id = crate::ai::blocklist::ResponseStreamId::new_for_test();
            history_model.update(&mut app, |history_model, ctx| {
                history_model.restore_conversations(terminal_view_id, vec![restored], ctx);
                let conversation = history_model
                    .conversation_mut(&conversation_id)
                    .expect("the restored conversation should be live");
                let root_task_id = conversation.get_root_task_id().clone();
                conversation
                    .update_for_new_request_input(
                        follow_up_request_input(conversation_id, root_task_id, FOLLOW_UP),
                        stream_id.clone(),
                        terminal_view_id,
                        ctx,
                    )
                    .expect("the follow-up should be accepted");
                apply_action(
                    conversation,
                    &stream_id,
                    terminal_view_id,
                    api::client_action::Action::AddMessagesToTask(
                        api::client_action::AddMessagesToTask {
                            task_id: ROOT_TASK_ID.to_string(),
                            messages: vec![
                                message("m7", ROOT_TASK_ID, "req-5", user_query(FOLLOW_UP)),
                                message("m8", ROOT_TASK_ID, "req-5", agent_output("Deleted.")),
                            ],
                        },
                    ),
                    ctx,
                );
                conversation.write_updated_conversation_state(ctx);
            });

            let conn = store
                .db_connection
                .clone()
                .expect("the store keeps its connection");
            let mut conn = conn.lock().expect("connection lock should not be poisoned");
            assert!(replay_persists(&receiver, &mut conn) >= 2);
            assert_eq!(
                persisted_task_ids(&mut conn, conversation_id),
                vec![ROOT_TASK_ID.to_string()],
            );

            // The restart: a fresh read of the database, as the lazy store does.
            let reread = crate::persistence::agent::read_agent_conversation_by_id(
                &mut conn,
                &conversation_id.to_string(),
            )
            .expect("the re-read should succeed")
            .expect("the conversation should still be persisted");
            let rerestored =
                convert_persisted_conversation_to_ai_conversation_with_metadata(reread)
                    .expect("the conversation should restore again");
            assert_eq!(
                rerestored.exchange_count(),
                5,
                "the original four exchanges and the follow-up must all come back",
            );
            let queries: Vec<String> = rerestored
                .all_exchanges()
                .iter()
                .flat_map(|exchange| exchange.input.iter())
                .filter_map(|input| match input {
                    AIAgentInput::UserQuery { query, .. } => Some(query.clone()),
                    _ => None,
                })
                .collect();
            assert!(queries.iter().any(|query| query == INITIAL_QUERY));
            assert!(queries.iter().any(|query| query == FOLLOW_UP));
            assert_eq!(
                listed_initial_query(&mut conn, conversation_id).as_deref(),
                Some(INITIAL_QUERY),
            );
        });
    }

    /// A conversation restored with zero task rows (a child persisted before its first
    /// response) gets a synthesized root. Once it has content, rewinding past its first
    /// exchange must clear its rows on disk, like any other conversation's, or the rewound
    /// exchange returns on the next restore. The earlier lifetime `KeepMissing` override
    /// downgraded this rewind and left the row behind.
    #[test]
    fn rewind_to_empty_after_restore_with_no_rows_deletes_on_disk() {
        warpui::App::test((), |mut app| async move {
            let (history_model, receiver) = history_model_harness(&mut app);
            let conversation_id = AIConversationId::new();
            let mut conn = test_connection();
            crate::persistence::agent::upsert_agent_conversation(
                &mut conn,
                &conversation_id.to_string(),
                Vec::<&api::Task>::new(),
                serde_json::from_str(BYOP_CONVERSATION_DATA)
                    .expect("BYOP conversation data should deserialize"),
            )
            .expect("the empty conversation should persist");
            assert!(persisted_task_ids(&mut conn, conversation_id).is_empty());

            let mut store = RestoredAgentConversations::with_db_connection(conn);
            let restored = store
                .take_conversation(&conversation_id)
                .expect("a conversation with no task rows restores with a synthesized root");
            assert_eq!(restored.exchange_count(), 0);

            let terminal_view_id = warpui::EntityId::new();
            let stream_id = crate::ai::blocklist::ResponseStreamId::new_for_test();
            history_model.update(&mut app, |history_model, ctx| {
                history_model.restore_conversations(terminal_view_id, vec![restored], ctx);
                let conversation = history_model
                    .conversation_mut(&conversation_id)
                    .expect("the restored conversation should be live");
                let root_task_id = conversation.get_root_task_id().clone();
                conversation
                    .update_for_new_request_input(
                        follow_up_request_input(conversation_id, root_task_id, "First query"),
                        stream_id.clone(),
                        terminal_view_id,
                        ctx,
                    )
                    .expect("the first query should be accepted");
                apply_action(
                    conversation,
                    &stream_id,
                    terminal_view_id,
                    api::client_action::Action::CreateTask(api::client_action::CreateTask {
                        task: Some(api::Task {
                            id: "first-root".to_string(),
                            messages: vec![message(
                                "r1",
                                "first-root",
                                "req-first",
                                user_query("First query"),
                            )],
                            ..Default::default()
                        }),
                    }),
                    ctx,
                );
                conversation.write_updated_conversation_state(ctx);
            });

            let conn = store
                .db_connection
                .clone()
                .expect("the store keeps its connection");
            let mut conn = conn.lock().expect("connection lock should not be poisoned");
            replay_persists(&receiver, &mut conn);
            assert_eq!(
                persisted_task_ids(&mut conn, conversation_id),
                vec!["first-root".to_string()],
                "precondition: the answered first query is on disk",
            );

            history_model.update(&mut app, |history_model, ctx| {
                let conversation = history_model
                    .conversation_mut(&conversation_id)
                    .expect("the conversation should still be live");
                let first_exchange_id = conversation
                    .first_exchange()
                    .expect("the first query created an exchange")
                    .id;
                conversation
                    .truncate_from_exchange(first_exchange_id, ctx)
                    .expect("rewinding past the first exchange should succeed");
            });

            assert!(replay_persists(&receiver, &mut conn) >= 1);
            assert!(
                persisted_task_ids(&mut conn, conversation_id).is_empty(),
                "a rewind past the first exchange must delete the rewound root on disk",
            );
        });
    }

    /// Two parentless roots that both carry messages have no single root. Restore refuses
    /// the conversation — deterministically, every time — rather than pick one and let the
    /// next save delete the other; every row stays on disk.
    #[test]
    fn two_roots_with_messages_are_refused_and_both_rows_survive() {
        let conversation_id = AIConversationId::new();
        let (mut conn, _startup_records) = persist_byop_conversation(conversation_id);
        let second_root = api::Task {
            id: "second-root".to_string(),
            messages: vec![message(
                "s1",
                "second-root",
                "req-s",
                user_query("A second history"),
            )],
            ..Default::default()
        };
        {
            use crate::persistence::schema::agent_tasks::dsl;
            use prost::Message as _;
            diesel::insert_into(dsl::agent_tasks)
                .values((
                    dsl::conversation_id.eq(conversation_id.to_string()),
                    dsl::task_id.eq(second_root.id.clone()),
                    dsl::task.eq(second_root.encode_to_vec()),
                ))
                .execute(&mut conn)
                .expect("inserting the second root should succeed");
        }
        let mut store = RestoredAgentConversations::with_db_connection(conn);

        for attempt in 0..3 {
            assert!(
                store.take_conversation(&conversation_id).is_none(),
                "attempt {attempt}: an ambiguous conversation must not restore",
            );
        }

        let conn = store
            .db_connection
            .clone()
            .expect("the store keeps its connection");
        let mut conn = conn.lock().expect("connection lock should not be poisoned");
        let mut expected = vec![ROOT_TASK_ID.to_string(), "second-root".to_string()];
        expected.sort();
        assert_eq!(persisted_task_ids(&mut conn, conversation_id), expected);
    }

    /// The post-4b0d1300f trigger: every task row of a conversation fails to decode. The
    /// store must not hand out an editable, hollow conversation for it (whose next save
    /// would decide what to prune), and the unreadable rows must stay on disk.
    #[test]
    fn conversation_with_undecodable_tasks_is_not_restored_and_rows_survive() {
        let conversation_id = AIConversationId::new();
        let (mut conn, _startup_records) = persist_byop_conversation(conversation_id);
        {
            use crate::persistence::schema::agent_tasks::dsl;
            diesel::update(
                dsl::agent_tasks.filter(dsl::conversation_id.eq(conversation_id.to_string())),
            )
            .set(dsl::task.eq(vec![0xffu8, 0xff, 0xff]))
            .execute(&mut conn)
            .expect("corrupting the task row should succeed");
        }
        let mut store = RestoredAgentConversations::with_db_connection(conn);

        assert!(
            store.get_conversation(&conversation_id).is_none(),
            "a conversation whose tasks cannot be read must not be restored as editable",
        );
        assert!(store.take_conversation(&conversation_id).is_none());

        let conn = store
            .db_connection
            .clone()
            .expect("the store keeps its connection");
        let mut conn = conn.lock().expect("connection lock should not be poisoned");
        assert_eq!(
            persisted_task_ids(&mut conn, conversation_id),
            vec![ROOT_TASK_ID.to_string()],
        );
    }
}
