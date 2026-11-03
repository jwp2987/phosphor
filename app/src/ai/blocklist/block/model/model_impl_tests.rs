use std::collections::HashMap;

use chrono::Local;
use warpui::{App, EntityId};

use super::exchange_is_streaming;
use crate::{
    GlobalResourceHandles, GlobalResourceHandlesProvider,
    ai::{
        agent::{AIAgentExchangeId, AIAgentInput, UserQueryMode},
        blocklist::{
            ResponseStreamId, controller::RequestInput, history_model::BlocklistAIHistoryModel,
        },
        llms::LLMId,
    },
    test_util::settings::initialize_settings_for_tests,
};

// A finished exchange must not be repainted when a sibling exchange streams (#1259); the check
// that decides it reads the exchange's own output status.
#[test]
fn exchange_is_streaming_follows_the_output_status() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let global_resource_handles = GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));

        let history_model =
            app.add_singleton_model(|_| BlocklistAIHistoryModel::new(vec![], vec![], &[]));
        let terminal_view_id = EntityId::new();
        let conversation_id = history_model.update(&mut app, |history_model, ctx| {
            history_model.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let stream_id = ResponseStreamId::new_for_test();
        history_model.update(&mut app, |history_model, ctx| {
            let task_id = history_model
                .conversation(&conversation_id)
                .unwrap()
                .get_root_task_id()
                .clone();
            let input = vec![AIAgentInput::UserQuery {
                query: "query".to_string(),
                context: Default::default(),
                static_query_type: None,
                referenced_attachments: Default::default(),
                user_query_mode: UserQueryMode::default(),
                running_command: None,
                intended_agent: None,
            }];
            let request_input = RequestInput {
                conversation_id,
                input_messages: HashMap::from([(task_id, input)]),
                working_directory: None,
                model_id: LLMId::from("test-model"),
                coding_model_id: LLMId::from("test-coding-model"),
                cli_agent_model_id: LLMId::from("test-cli-agent-model"),
                computer_use_model_id: LLMId::from("test-computer-use-model"),
                shared_session_response_initiator: None,
                request_start_ts: Local::now(),
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

        let exchange_id = history_model.read(&app, |model, _| {
            model
                .conversation(&conversation_id)
                .unwrap()
                .latest_exchange()
                .unwrap()
                .id
        });

        // The stream's init is what gives the exchange its output; a stream that never produced
        // one cannot be completed (`OutputAlreadyFinished`).
        history_model.update(&mut app, |history_model, ctx| {
            history_model.initialize_output_for_response_stream(
                &stream_id,
                conversation_id,
                terminal_view_id,
                warp_multi_agent_api::response_event::StreamInit {
                    request_id: String::new(),
                    conversation_id: String::new(),
                    run_id: String::new(),
                },
                ctx,
            );
        });

        app.read(|ctx| {
            assert!(exchange_is_streaming(conversation_id, exchange_id, ctx));
            // An exchange that cannot be found keeps the conservative answer: repaint.
            assert!(exchange_is_streaming(
                conversation_id,
                AIAgentExchangeId::new(),
                ctx
            ));
        });

        history_model.update(&mut app, |history_model, ctx| {
            history_model.mark_response_stream_completed_successfully(
                &stream_id,
                conversation_id,
                terminal_view_id,
                ctx,
            );
        });

        app.read(|ctx| {
            assert!(!exchange_is_streaming(conversation_id, exchange_id, ctx));
        });
    });
}
