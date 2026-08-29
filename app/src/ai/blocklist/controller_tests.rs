//! Unit tests for [`BlocklistAIController`] request construction.

use std::collections::HashMap;

use uuid::Uuid;
use warpui::{App, SingletonEntity};

use crate::ai::agent::task::TaskId;
use crate::ai::agent::{
    AIAgentAttachment, AIAgentContext, AIAgentExchange, AIAgentExchangeId, AIAgentInput,
    AIAgentOutputStatus, CancellationReason, ImageContext, PassiveSuggestionTrigger,
    UserQueryMode,
};
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::blocklist::controller::response_stream::{
    PendingResume, RecoveryBudget, ResponseStream, ResponseStreamId,
};
use crate::ai::blocklist::{BlocklistAIHistoryModel, PendingAttachment, PendingFile};
use crate::ai::llms::LLMId;
use crate::test_util::terminal::{add_window_with_terminal, initialize_app_for_terminal_view};

/// Minimal streaming exchange, mirroring `terminal/view_test.rs`'s local `exchange_with_inputs`
/// helper (not `pub`, so duplicated here rather than shared).
fn streaming_exchange() -> AIAgentExchange {
    AIAgentExchange {
        id: AIAgentExchangeId::new(),
        input: vec![],
        output_status: AIAgentOutputStatus::Streaming { output: None },
        added_message_ids: Default::default(),
        start_time: chrono::Local::now(),
        finish_time: None,
        time_to_first_token_ms: None,
        working_directory: None,
        model_id: LLMId::from("test-model"),
        request_cost: None,
        coding_model_id: LLMId::from("test-coding-model"),
        cli_agent_model_id: LLMId::from("test-cli-agent-model"),
        computer_use_model_id: LLMId::from("test-computer-use-model"),
        response_initiator: None,
    }
}

fn new_ambient_agent_task_id() -> AmbientAgentTaskId {
    Uuid::new_v4().to_string().parse().unwrap()
}

fn image_attachment(file_name: &str) -> PendingAttachment {
    PendingAttachment::Image(ImageContext {
        data: String::new(),
        mime_type: "image/png".to_owned(),
        file_name: file_name.to_owned(),
        is_figma: false,
    })
}

fn file_attachment(file_name: &str) -> PendingAttachment {
    PendingAttachment::File(PendingFile {
        file_name: file_name.to_owned(),
        file_path: file_name.into(),
        mime_type: "text/plain".to_owned(),
    })
}

/// Passive suggestion requests are separate, read-only requests. They must never be
/// attributed to the ambient agent task that owns the controller, otherwise the request
/// is billed/attributed to that run.
#[test]
fn passive_suggestions_request_params_omit_ambient_agent_task_id() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |terminal, ctx| {
            let task_id = new_ambient_agent_task_id();
            let conversation_id =
                BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                    history_model.start_new_conversation(terminal.id(), false, false, ctx)
                });

            terminal.ai_controller().update(ctx, |controller, ctx| {
                controller.set_ambient_agent_task_id(Some(task_id), ctx);

                assert_eq!(controller.get_ambient_agent_task_id(), Some(task_id));
                assert_eq!(
                    controller
                        .build_passive_suggestions_request_params(
                            Some(conversation_id),
                            PassiveSuggestionTrigger::FilesChanged,
                            vec![],
                            ctx,
                        )
                        .expect("existing conversation should build passive suggestion params")
                        .1
                        .ambient_agent_task_id,
                    None
                );
                assert_eq!(
                    controller
                        .build_passive_suggestions_request_params(
                            None,
                            PassiveSuggestionTrigger::FilesChanged,
                            vec![],
                            ctx,
                        )
                        .expect("new conversation should build passive suggestion params")
                        .1
                        .ambient_agent_task_id,
                    None
                );
            });
        });
    });
}

/// Ported from the pinned oracle (`02b53fcd8`). See #341: the fork's `input_for_query` used to
/// read the context model's live-staged pending attachments directly, so a fired queued prompt
/// sent whatever happened to be staged when it drained rather than what was staged when it was
/// queued.
#[test]
fn input_for_query_converts_prompt_attachments_and_ignores_live_staging() {
    // `input_for_query` builds its image/file context purely from the explicitly-provided
    // attachment set (resolved by `send_query` from either the queued row or live staging),
    // never from the context model's pending attachments.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);

        terminal.update(&mut app, |terminal, ctx| {
            let conversation_id =
                BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
                    history_model.start_new_conversation(terminal.id(), false, false, ctx)
                });

            let controller = terminal.ai_controller();
            let context_model = controller.as_ref(ctx).context_model.clone();
            let active_session = controller.as_ref(ctx).active_session.clone();

            // Stage *live* attachments that must NOT leak into a query built from a different,
            // explicitly-provided attachment set.
            context_model.update(ctx, |m, ctx| {
                m.append_pending_attachments(
                    vec![image_attachment("live.png"), file_attachment("live.txt")],
                    ctx,
                );
            });

            let task_id = TaskId::new("test-task".to_owned());
            // Two files sharing a basename to exercise duplicate-basename suffixing.
            let prompt_attachments = vec![
                image_attachment("queued.png"),
                file_attachment("notes.txt"),
                file_attachment("notes.txt"),
            ];

            let input = super::input_for_query(
                "build a query".to_owned(),
                &task_id,
                conversation_id,
                None,
                UserQueryMode::Normal,
                None,
                HashMap::new(),
                prompt_attachments,
                context_model.as_ref(ctx),
                active_session.as_ref(ctx),
                ctx,
            );

            let AIAgentInput::UserQuery {
                context,
                referenced_attachments,
                ..
            } = input
            else {
                panic!("expected UserQuery");
            };

            // The provided image is attached as image context; the live-staged image is not.
            let image_names: Vec<&str> = context
                .iter()
                .filter_map(|c| match c {
                    AIAgentContext::Image(img) => Some(img.file_name.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(image_names, vec!["queued.png"]);

            // The provided files are attached as FilePathReference with duplicate-basename
            // suffixing; the live-staged file is not.
            let mut file_names: Vec<String> = referenced_attachments
                .values()
                .filter_map(|a| match a {
                    AIAgentAttachment::FilePathReference { file_name, .. } => {
                        Some(file_name.clone())
                    }
                    _ => None,
                })
                .collect();
            file_names.sort();
            assert_eq!(
                file_names,
                vec!["notes.txt".to_owned(), "notes.txt".to_owned()]
            );
            assert!(referenced_attachments.contains_key("notes.txt"));
            assert!(referenced_attachments.contains_key("notes.txt (1)"));
            assert!(!referenced_attachments.contains_key("live.txt"));
        });
    });
}

/// When an agent command exits the shell, the conversation must be finalized as
/// `Error` (not `Cancelled`), and a subsequent `ManuallyCancelled` (as fired by
/// the pane-close path) must not overwrite that failure.
///
/// Ported from the pin (`app/src/ai/blocklist/controller_tests.rs:338`, `02b53fcd8`) for #341.
/// Adapted to set up the in-flight stream via `append_reassigned_exchange` +
/// `register_mock_stream_for_test`, the pattern this fork's own
/// `terminal/view_test.rs` mock-stream tests already use, rather than the pin's
/// `RequestInput`/`update_conversation_for_new_request_input` path -- both make
/// `is_processing_response_stream` return true for the stream id, which is all
/// `fail_conversation_due_to_shell_exit` and `cancel_conversation_progress` need.
#[test]
fn fail_conversation_due_to_shell_exit_reports_error_and_survives_manual_cancel() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        // #341's `fail_conversation_due_to_shell_exit` reaches this singleton on the
        // error path; `get_singleton_model_as_ref` panics rather than returning None when
        // it is absent. Registered here rather than in `initialize_app_for_terminal_view`
        // because nine other callers of that helper already register it themselves.
        let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(|_| {
            crate::global_resource_handles::GlobalResourceHandlesProvider::new(
                global_resource_handles,
            )
        });
        let terminal = add_window_with_terminal(&mut app, None);

        let conversation_id = terminal.update(&mut app, |view, ctx| {
            let terminal_view_id = view.id();
            let stream_id = ResponseStreamId::new_for_test();
            let conversation_id =
                BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                    let conversation_id =
                        history.start_new_conversation(terminal_view_id, false, false, ctx);
                    history
                        .conversation_mut(&conversation_id)
                        .expect("conversation should exist")
                        .append_reassigned_exchange(
                            &stream_id,
                            streaming_exchange(),
                            terminal_view_id,
                            ctx,
                        )
                        .expect("exchange should append");
                    conversation_id
                });
            let stream = ctx.add_model(|_| ResponseStream::new_for_test(stream_id.clone()));
            view.ai_controller().update(ctx, |controller, ctx| {
                controller.register_mock_stream_for_test(stream_id, conversation_id, stream, ctx);
                controller.fail_conversation_due_to_shell_exit(
                    conversation_id,
                    "exit 1".to_string(),
                    ctx,
                );
            });
            conversation_id
        });

        // The in-flight request is finalized as Error (with the shell-exit error
        // on its exchange), not Cancelled.
        BlocklistAIHistoryModel::handle(&app).read(&app, |history, _| {
            assert_eq!(
                history.conversation(&conversation_id).map(|c| c.status()),
                Some(&crate::ai::agent::conversation::ConversationStatus::Error)
            );
        });

        // The pane-close cancellation path must be a no-op now that the
        // conversation is terminal.
        terminal.update(&mut app, |view, ctx| {
            view.ai_controller().update(ctx, |controller, ctx| {
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
            });
        });
        BlocklistAIHistoryModel::handle(&app).read(&app, |history, _| {
            assert_eq!(
                history.conversation(&conversation_id).map(|c| c.status()),
                Some(&crate::ai::agent::conversation::ConversationStatus::Error)
            );
        });
    });
}

/// Ported from the pin (`42effe840:app/src/ai/blocklist/controller_tests.rs::
/// cancelling_conversation_aborts_pending_auto_resume`) verbatim; every symbol
/// it drives exists unchanged in this fork
/// (`schedule_auto_resume_after_error` at `controller.rs:1813`,
/// `pending_auto_resume_handles` at `:432`, and the `remove(..).abort()` arm of
/// `cancel_conversation_progress` at `:3403`).
///
/// The interaction matters because the scheduled resume is a
/// `wait_until_online()` future that can outlive the user's decision to stop:
/// without the abort, cancelling a conversation would leave a future armed that
/// re-sends a request the user just cancelled, as soon as the network returns.
#[test]
fn cancelling_conversation_aborts_pending_auto_resume() {
    use crate::ai::agent::conversation::AIConversationId;

    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);

        // An ID with no backing conversation: if the scheduled wait ever
        // completes, the resume is a harmless no-op.
        let conversation_id = AIConversationId::new();

        terminal.update(&mut app, |terminal, ctx| {
            terminal.ai_controller().update(ctx, |controller, ctx| {
                controller.schedule_auto_resume_after_error(
                    conversation_id,
                    PendingResume::immediate(RecoveryBudget::fresh()),
                    ctx,
                );
                assert!(
                    controller
                        .pending_auto_resume_handles
                        .contains_key(&conversation_id)
                );

                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
                assert!(
                    !controller
                        .pending_auto_resume_handles
                        .contains_key(&conversation_id)
                );
            });
        });
    });
}

/// An optimistic long-running-command completion that cancels an in-flight
/// stream must finalize the conversation as `Success`, not `Cancelled`. This is
/// a regression test for the reason -> status mapping living in a single place
/// (`CancellationReason::conversation_outcome`).
///
/// Ported from the pin (`42effe840:app/src/ai/blocklist/controller_tests.rs::
/// optimistic_cli_subagent_completion_with_in_flight_stream_reports_success`).
/// The pin's reason is named `CommandFinishedDuringInlineAgentView`; this fork
/// carries the same variant under its older name
/// `OptimisticCLISubagentCompletion`. Set up the in-flight stream with
/// `append_reassigned_exchange` + `register_mock_stream_for_test` (the pattern
/// the sibling shell-exit test above already uses) rather than the pin's
/// `RequestInput`/`update_conversation_for_new_request_input` path -- both make
/// `is_processing_response_stream` true for the stream id, which is all
/// `cancel_conversation_progress` needs.
#[test]
fn optimistic_cli_subagent_completion_with_in_flight_stream_reports_success() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        // Cancelling the stream reaches `write_updated_conversation_state`, which
        // reads this singleton; `get_singleton_model_as_ref` panics rather than
        // returning None when it is absent.
        let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(|_| {
            crate::global_resource_handles::GlobalResourceHandlesProvider::new(
                global_resource_handles,
            )
        });
        let terminal = add_window_with_terminal(&mut app, None);

        let conversation_id = terminal.update(&mut app, |view, ctx| {
            let terminal_view_id = view.id();
            let stream_id = ResponseStreamId::new_for_test();
            let conversation_id =
                BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                    let conversation_id =
                        history.start_new_conversation(terminal_view_id, false, false, ctx);
                    history
                        .conversation_mut(&conversation_id)
                        .expect("conversation should exist")
                        .append_reassigned_exchange(
                            &stream_id,
                            streaming_exchange(),
                            terminal_view_id,
                            ctx,
                        )
                        .expect("exchange should append");
                    conversation_id
                });
            let stream = ctx.add_model(|_| ResponseStream::new_for_test(stream_id.clone()));
            view.ai_controller().update(ctx, |controller, ctx| {
                controller.register_mock_stream_for_test(stream_id, conversation_id, stream, ctx);
                // The long-running command finished while the agent was still
                // streaming, cancelling the in-flight stream optimistically.
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::OptimisticCLISubagentCompletion,
                    ctx,
                );
            });
            conversation_id
        });

        BlocklistAIHistoryModel::handle(&app).read(&app, |history, _| {
            assert_eq!(
                history.conversation(&conversation_id).map(|c| c.status()),
                Some(&crate::ai::agent::conversation::ConversationStatus::Success)
            );
        });
    });
}
