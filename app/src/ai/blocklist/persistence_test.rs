use std::collections::HashMap;

use warpui::{App, EntityId};

use super::{maybe_build_ai_query_upsert_event, PersistedAIAgentActionType, PersistedAIInputType};
use crate::ai::agent::conversation::{AIConversation, AIConversationId};
use crate::ai::agent::{AIAgentActionType, UseComputerRequest};
use crate::ai::blocklist::{BlocklistAIHistoryEvent, BlocklistAIHistoryModel};
use crate::persistence::ModelEvent;

fn user_query_message(task_id: &str, query: &str) -> warp_multi_agent_api::Message {
    warp_multi_agent_api::Message {
        fetched_memories: vec![],
        id: "message-id".to_owned(),
        task_id: task_id.to_owned(),
        server_message_data: String::new(),
        citations: Vec::new(),
        message: Some(warp_multi_agent_api::message::Message::UserQuery(
            warp_multi_agent_api::message::UserQuery {
                query: query.to_owned(),
                context: None,
                referenced_attachments: HashMap::new(),
                mode: None,
                intended_agent: Default::default(),
            },
        )),
        request_id: "request-id".to_owned(),
        timestamp: None,
    }
}

#[test]
fn query_exchange_event_builds_persistence_upsert() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let conversation_id = AIConversationId::new();
        let task_id = "task-id";
        let conversation = AIConversation::new_restored(
            conversation_id,
            vec![warp_multi_agent_api::Task {
                id: task_id.to_owned(),
                messages: vec![user_query_message(task_id, "persist this prompt")],
                dependencies: None,
                description: String::new(),
                summary: String::new(),
                server_data: String::new(),
            }],
            None,
        )
        .expect("conversation should restore");
        let exchange_id = conversation
            .root_task_exchanges()
            .next()
            .expect("conversation should contain the query exchange")
            .id;
        let task_id = conversation.get_root_task_id().clone();

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        history_model.update(&mut app, |model, ctx| {
            model.restore_conversations(terminal_view_id, vec![conversation], ctx);
        });

        let event = BlocklistAIHistoryEvent::AppendedExchange {
            exchange_id,
            task_id,
            terminal_view_id,
            conversation_id,
            is_hidden: false,
            response_stream_id: None,
        };
        let persistence_event = app
            .read(|ctx| maybe_build_ai_query_upsert_event(&event, terminal_view_id, false, ctx))
            .expect("query exchange should produce a persistence event");
        let ModelEvent::UpsertAIQuery { query } = persistence_event else {
            panic!("query exchange should produce an AI-query upsert");
        };
        assert_eq!(query.conversation_id, conversation_id);
        assert_eq!(query.exchange_id, exchange_id);
        assert_eq!(
            query.inputs,
            vec![PersistedAIInputType::Query {
                text: "persist this prompt".to_owned(),
                context: Default::default(),
                referenced_attachments: Default::default(),
            }]
        );
    });
}

/// Conversations persisted before `UseComputerRequest::actions` became
/// `Vec<TargetedAction>` stored each element as a bare `computer_use::Action`.
/// Those rows must still restore, targeting the whole screen.
#[test]
fn persisted_use_computer_accepts_legacy_bare_actions() {
    let json = r#"{
        "UseComputer": {
            "action_summary": "Click button",
            "actions": [{ "MouseUp": { "button": "Left" } }],
            "screenshot_params": null
        }
    }"#;

    let persisted: PersistedAIAgentActionType =
        serde_json::from_str(json).expect("legacy bare-action shape must deserialize");

    let PersistedAIAgentActionType::UseComputer { actions, .. } = &persisted else {
        panic!("expected a UseComputer action, got {persisted:?}");
    };
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].target, computer_use::Target::Screen);
    assert_eq!(
        actions[0].action,
        computer_use::Action::MouseUp {
            button: computer_use::MouseButton::Left
        }
    );
}

/// The current `{ action, target }` shape round-trips through persistence with
/// the window target intact.
#[test]
fn persisted_use_computer_round_trips_window_target() {
    let request = UseComputerRequest {
        action_summary: "Click button".to_owned(),
        actions: vec![computer_use::TargetedAction {
            action: computer_use::Action::MouseUp {
                button: computer_use::MouseButton::Left,
            },
            target: computer_use::Target::Window {
                window_id: 42,
                pid: 7,
            },
        }],
        screenshot_params: None,
    };
    let action = AIAgentActionType::UseComputer(request);

    let persisted = PersistedAIAgentActionType::from(&action);
    let json = serde_json::to_string(&persisted).expect("persisted action must serialize");
    let restored: PersistedAIAgentActionType =
        serde_json::from_str(&json).expect("persisted action must deserialize");
    assert_eq!(restored, persisted);

    let restored_action =
        AIAgentActionType::try_from(restored).expect("persisted action must convert back");
    assert_eq!(restored_action, action);
}
