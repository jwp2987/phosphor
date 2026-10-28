//! Unit tests for [`BlocklistAIController`] request construction.

use std::collections::HashMap;

use uuid::Uuid;
use warpui::{App, SingletonEntity};

use crate::ai::agent::conversation::ConversationStatus;
use crate::ai::agent::task::TaskId;
use crate::ai::agent::{
    AIAgentAction, AIAgentActionId, AIAgentActionType, AIAgentAttachment, AIAgentContext,
    AIAgentExchange, AIAgentExchangeId, AIAgentInput, AIAgentOutputStatus, CancellationReason,
    ImageContext, PassiveSuggestionTrigger, UserQueryMode,
};
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::blocklist::controller::response_stream::{
    PendingResume, RecoveryBudget, ResponseStream, ResponseStreamId,
};
use crate::ai::blocklist::{
    BlocklistAIControllerEvent, BlocklistAIHistoryModel, PendingAttachment, PendingFile,
    QueuedQuery, QueuedQueryModel, QueuedQueryOrigin,
};
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
/// cancelling_conversation_aborts_pending_auto_resume`); every symbol it drives
/// exists in this fork (`schedule_auto_resume_after_error` at
/// `controller.rs:1866`, `pending_auto_resume_handles` at `:444`, and the
/// `remove(..).abort()` arm of `cancel_conversation_progress` at `:3468`).
///
/// Adapted 2026-08-29 for the shared recovery budget (upstream `1a29f680d`):
/// `schedule_auto_resume_after_error` now takes the `PendingResume` the failed
/// request scheduled. `PendingResume::immediate(RecoveryBudget::fresh())` is
/// the zero-backoff form, so the future this test aborts is still the bare
/// `wait_until_online()` the original drove.
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

/// Scheduling an auto-resume for a conversation that already has one armed must abort
/// the old one. `SpawnedFutureHandle` does not abort on drop, so the old `insert`
/// silently orphaned the first resume: it stayed armed, fired after its replacement,
/// and was out of reach of `cancel_conversation_progress` (which only aborts the handle
/// still in the map). The stream-finished handler could reach this with one stream
/// (the BYOP local resume plus the stream's own `pending_resume`).
///
/// Fails without the fix: the first handle's abort flag is never set.
#[test]
fn rescheduling_auto_resume_aborts_the_replaced_handle() {
    use crate::ai::agent::conversation::AIConversationId;

    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);

        // An ID with no backing conversation: if a scheduled wait ever completes,
        // the resume is a harmless no-op.
        let conversation_id = AIConversationId::new();

        terminal.update(&mut app, |terminal, ctx| {
            terminal.ai_controller().update(ctx, |controller, ctx| {
                controller.schedule_auto_resume_after_error(
                    conversation_id,
                    PendingResume::immediate(RecoveryBudget::fresh()),
                    ctx,
                );
                let first = controller
                    .pending_auto_resume_handles
                    .get(&conversation_id)
                    .cloned()
                    .expect("first resume should be scheduled");

                controller.schedule_auto_resume_after_error(
                    conversation_id,
                    PendingResume::immediate(RecoveryBudget::fresh()),
                    ctx,
                );
                let second = controller
                    .pending_auto_resume_handles
                    .get(&conversation_id)
                    .cloned()
                    .expect("second resume should be scheduled");

                assert_ne!(first.future_id(), second.future_id());
                assert!(
                    first.abort_handle().is_aborted(),
                    "the replaced auto-resume must be aborted, not orphaned"
                );
                assert!(!second.abort_handle().is_aborted());

                // The surviving handle is still the one the cancel path reaches.
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
                assert!(second.abort_handle().is_aborted());
            });
        });
    });
}

/// Only a real cancellation hands Classic input back to the shell. The fork had dropped the
/// pin's `CancellationOutcome::Cancelled` gate and flipped input on every cancel reason, so a
/// same-conversation follow-up dropped the user out of AI mode mid-conversation.
///
/// This tests the gate directly. An end-to-end version was tried and abandoned: in the test
/// harness a Classic input set to AI does not survive its own effects flushing (a precondition
/// assert after the flush fails with and without the fix), so it could not observe the gate.
/// An earlier version asserted inside the update, before the queued `AfterStreamFinished`
/// handler ran, and so passed without the fix.
#[test]
fn only_a_real_cancellation_switches_input_to_shell() {
    use super::cancellation_switches_input_to_shell as switches;

    // Real cancellations do (outcome `Cancelled`).
    assert!(switches(false, CancellationReason::ManuallyCancelled));
    assert!(switches(false, CancellationReason::UserCommandExecuted));
    assert!(switches(false, CancellationReason::Deleted));
    assert!(switches(
        false,
        CancellationReason::FollowUpSubmitted {
            is_for_same_conversation: false,
        }
    ));

    // The turn goes on, or ended well, or is finalized elsewhere: input stays put. Each of
    // these returned `true` before the fix.
    assert!(!switches(
        false,
        CancellationReason::FollowUpSubmitted {
            is_for_same_conversation: true,
        }
    ));
    assert!(!switches(false, CancellationReason::OptimisticCLISubagentCompletion));
    assert!(!switches(false, CancellationReason::Reverted));
    assert!(!switches(false, CancellationReason::AgentExitedShell));

    // A passive request never changes the input mode.
    assert!(!switches(true, CancellationReason::ManuallyCancelled));
}

/// A generic, non-autoexecutable shell command -- mirrors `action_model_tests.rs`'s helper of
/// the same name (private to that module, so duplicated here rather than shared).
fn shell_command() -> AIAgentActionType {
    AIAgentActionType::RequestCommandOutput {
        command: "free -h".to_owned(),
        is_read_only: None,
        is_risky: None,
        rationale: None,
        uses_pager: None,
        wait_until_completion: true,
        citations: vec![],
    }
}

/// jwp2987/phosphor#725: rejecting a blocked command must not leave a `PendingApprovalFollowUp`
/// queued row locked forever.
///
/// A command action needing confirmation blocks the conversation
/// (`ConversationStatus::Blocked`) and a follow-up submitted while blocked queues as
/// `PendingApprovalFollowUp` (locked for submission until the blocked action resolves). Clicking
/// Reject reaches `BlocklistAIActionModel::cancel_action_with_id`, which resolves the action as
/// `RequestCommandOutputResult::CancelledBeforeExecution` -- a result `should_trigger_request_
/// upon_completion` reports `false` for. With this the only pending action, no follow-up request
/// is sent, so `BlocklistAIController`'s `FinishedAction` subscriber (`controller.rs`) finalizes
/// the conversation status directly instead: no new request means no future
/// `FinishedReceivingOutput` ever arrives to run `TerminalView::drain_queued_prompts`, which is
/// the only other place that calls `unlock_pending_approval_rows`. Before the fix, that left the
/// row `PendingApprovalFollowUp` (locked) for the life of the process -- Send Now inert, not
/// deletable via the ordinary unlocked path, exactly the symptom reported in X11 testing.
#[test]
fn reject_of_a_blocked_command_with_no_follow_up_unlocks_the_queued_row() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.id());

        let conversation_id = BlocklistAIHistoryModel::handle(&app)
            .update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });

        let pending_id = QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(
                conversation_id,
                QueuedQuery::new(
                    "finish this once the command resolves".to_owned(),
                    QueuedQueryOrigin::PendingApprovalFollowUp,
                ),
                ctx,
            )
        });

        let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
        // #1026: the reject must also tell the view to restore the row, since no turn end will.
        let released_events = std::rc::Rc::new(std::cell::RefCell::new(0usize));
        let released_events_sub = released_events.clone();
        app.update(|ctx| {
            ctx.subscribe_to_model(&controller, move |_, event, _| {
                if matches!(
                    event,
                    BlocklistAIControllerEvent::QueuedPromptsReleased { .. }
                ) {
                    *released_events_sub.borrow_mut() += 1;
                }
            });
        });
        let action_model = controller.read(&app, |controller, _| controller.action_model.clone());

        let action_id = AIAgentActionId::from("reject-me".to_owned());
        action_model.update(&mut app, |action_model, ctx| {
            action_model.queue_action_for_test(
                AIAgentAction {
                    id: action_id.clone(),
                    task_id: TaskId::new("task".to_owned()),
                    action: shell_command(),
                    requires_result: false,
                },
                conversation_id,
                ctx,
            );
        });

        // `queue_action_for_test` preprocesses via `ctx.spawn`; pump the foreground executor
        // until the action lands, unexecuted, at the head of the pending queue.
        for _ in 0..64 {
            let is_blocked = action_model.read(&app, |action_model, _| {
                action_model
                    .get_action_status(&action_id)
                    .is_some_and(|status| status.is_blocked())
            });
            if is_blocked {
                break;
            }
            futures_lite::future::yield_now().await;
        }
        action_model.read(&app, |action_model, _| {
            assert!(
                action_model
                    .get_action_status(&action_id)
                    .is_some_and(|status| status.is_blocked()),
                "the command must be blocked on confirmation before the reject under test"
            );
        });
        BlocklistAIHistoryModel::handle(&app).read(&app, |history, _| {
            assert!(
                matches!(
                    history.conversation(&conversation_id).map(|c| c.status()),
                    Some(&ConversationStatus::Blocked { .. })
                ),
                "precondition: the conversation must be Blocked on the command"
            );
        });
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert_eq!(
                model.queue(conversation_id)[0].origin(),
                QueuedQueryOrigin::PendingApprovalFollowUp,
                "precondition: the follow-up is still locked while the command is blocked"
            );
        });

        // Reject: what `RequestedCommandView`'s Reject button does
        // (`RequestedCommandViewEvent::Rejected` -> `BlockView::cancel_action` ->
        // `cancel_action_with_id`).
        action_model.update(&mut app, |action_model, ctx| {
            action_model.cancel_action_with_id(
                conversation_id,
                &action_id,
                CancellationReason::ManuallyCancelled,
                ctx,
            );
        });

        // Wait for `FinishedAction`'s effect to be delivered and handled: the conversation
        // status write and (with the fix) the queue unlock both happen inside that subscriber.
        for _ in 0..64 {
            let unlocked = QueuedQueryModel::handle(&app).read(&app, |model, _| {
                model
                    .queue(conversation_id)
                    .first()
                    .is_some_and(|row| row.origin() == QueuedQueryOrigin::ApprovalFollowUp)
            });
            if unlocked {
                break;
            }
            futures_lite::future::yield_now().await;
        }

        BlocklistAIHistoryModel::handle(&app).read(&app, |history, _| {
            assert_eq!(
                history.conversation(&conversation_id).map(|c| c.status()),
                Some(&ConversationStatus::Cancelled),
                "the rejected-and-only action leaves the conversation Cancelled, not Blocked"
            );
        });
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            let queue = model.queue(conversation_id);
            assert_eq!(
                queue.len(),
                1,
                "unlocking must not itself send the row -- no turn has completed yet"
            );
            assert_eq!(queue[0].id(), pending_id);
            assert_eq!(
                queue[0].origin(),
                QueuedQueryOrigin::ApprovalFollowUp,
                "the row must unlock even though rejecting sent no follow-up request"
            );
            assert!(!queue[0].is_locked());
        });
        assert_eq!(
            *released_events.borrow(),
            1,
            "a reject with no follow-up must release the queue to the view exactly once (#1026)"
        );
    });
}

/// #848: a controller whose pane is closed (suspended for undo-close) starts no request --
/// `send_request_input` is the single place a request starts, so a resume (as here), an action
/// follow-up, a flushed BYOP request and a passive suggestion are all refused there. Lifting the
/// suspension restores normal sending, which shows the refusal is the guard and not the setup.
#[test]
fn suspended_controller_sends_no_request_until_lifted() {
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(|_| {
            crate::GlobalResourceHandlesProvider::new(global_resource_handles)
        });
        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.id());
        let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
        let conversation_id = BlocklistAIHistoryModel::handle(&app)
            .update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });

        controller.update(&mut app, |controller, ctx| {
            controller.set_suspended_for_close(true);
            controller.resume_conversation(conversation_id, false, vec![], ctx);
            assert!(!controller.has_active_stream_for_conversation(conversation_id, ctx));

            // No auto-resume is armed either: it would fire after an undo.
            controller.schedule_auto_resume_after_error(
                conversation_id,
                PendingResume::immediate(RecoveryBudget::fresh()),
                ctx,
            );
            assert!(
                !controller
                    .pending_auto_resume_handles
                    .contains_key(&conversation_id)
            );

            controller.set_suspended_for_close(false);
            controller.resume_conversation(conversation_id, false, vec![], ctx);
            assert!(controller.has_active_stream_for_conversation(conversation_id, ctx));
        });
    });
}

/// A finished exchange of `stream_id`'s response whose only output is `action` -- the shape the
/// stream-finished handler turns into `actions_to_queue`. Appended while streaming (only a
/// streaming exchange is registered against its response stream) and completed by the caller.
fn streaming_exchange_with_action(action: AIAgentAction) -> AIAgentExchange {
    use crate::ai::agent::{
        AIAgentOutput, AIAgentOutputMessage, AIAgentOutputMessageType, MessageId, Shared,
    };

    let output = AIAgentOutput {
        messages: vec![AIAgentOutputMessage {
            id: MessageId::new("tag-in-action-message".to_owned()),
            message: AIAgentOutputMessageType::Action(action),
            citations: Vec::new(),
        }],
        ..Default::default()
    };
    AIAgentExchange {
        output_status: AIAgentOutputStatus::Streaming {
            output: Some(Shared::new(output)),
        },
        ..streaming_exchange()
    }
}

/// Drives one LRC tag-in round to completion through the controller's real stream-finished
/// path: the request says the user tagged the agent in while the command holds the alt screen
/// (the exact shape #617 kept auto-accepting), and the model's only tool call is a PTY write.
/// Returns the write's status once it has settled out of preprocessing.
///
/// `allow_pty_writes` sets the profile's "Interact with running commands" to Always allow --
/// the user's own explicit opt-in, which must still be honoured.
///
/// The write targets a block id that is not registered, so if it does execute it resolves at
/// once (`BlockNotFound`) instead of writing anywhere; and that lookup miss still consults the
/// `write_to_pty` permission (#615), so the profile is what decides.
async fn alt_screen_tag_in_write_status(
    app: &mut App,
    allow_pty_writes: bool,
) -> Option<crate::ai::blocklist::action_model::AIActionStatus> {
    use crate::ai::agent::{AIAgentPtyWriteMode, RunningCommand};
    use crate::ai::blocklist::BlocklistAIPermissions;
    use crate::ai::blocklist::controller::response_stream::ResponseStreamEvent;
    use crate::ai::execution_profiles::WriteToPtyPermission;
    use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
    use crate::terminal::model::block::BlockId;

    initialize_app_for_terminal_view(app);
    // Completing the exchange persists the conversation, which reads this singleton.
    let global_resource_handles = crate::GlobalResourceHandles::mock(app);
    app.add_singleton_model(|_| crate::GlobalResourceHandlesProvider::new(global_resource_handles));
    let terminal = add_window_with_terminal(app, None);
    let terminal_view_id = terminal.read(&*app, |view, _| view.id());

    if allow_pty_writes {
        AIExecutionProfilesModel::handle(&*app).update(app, |model, ctx| {
            let profile_id = *model.active_profile(Some(terminal_view_id), ctx).id();
            model.set_write_to_pty(profile_id, &WriteToPtyPermission::AlwaysAllow, ctx);
        });
    }
    let pty_permission = app.read(|ctx| {
        BlocklistAIPermissions::as_ref(ctx).get_write_to_pty_setting(ctx, Some(terminal_view_id))
    });
    assert_eq!(
        pty_permission,
        if allow_pty_writes {
            WriteToPtyPermission::AlwaysAllow
        } else {
            WriteToPtyPermission::AlwaysAsk
        },
        "fixture precondition: the profile's pty-write permission is what the test says it is"
    );

    let action_id = AIAgentActionId::from("tag-in-pty-write".to_owned());
    let action = AIAgentAction {
        id: action_id.clone(),
        task_id: TaskId::new("task".to_owned()),
        action: AIAgentActionType::WriteToLongRunningShellCommand {
            block_id: BlockId::new(),
            input: b":!echo injected\n".to_vec().into(),
            mode: AIAgentPtyWriteMode::Raw,
        },
        requires_result: true,
    };

    let controller = terminal.read(&*app, |view, _| view.ai_controller().clone());
    let action_model = controller.read(&*app, |controller, _| controller.action_model.clone());
    terminal.update(app, |view, ctx| {
        let stream_id = ResponseStreamId::new_for_test();
        let history = BlocklistAIHistoryModel::handle(ctx);
        let conversation_id = history.update(ctx, |history, ctx| {
            let conversation_id =
                history.start_new_conversation(terminal_view_id, false, false, ctx);
            history
                .conversation_mut(&conversation_id)
                .expect("conversation should exist")
                .append_reassigned_exchange(
                    &stream_id,
                    streaming_exchange_with_action(action),
                    terminal_view_id,
                    ctx,
                )
                .expect("exchange should append");
            conversation_id
        });
        let stream = ctx.add_model(|_| {
            let mut stream = ResponseStream::new_for_test(stream_id.clone());
            let params = stream.params_mut_for_test();
            params.lrc_command_id = Some("lrc-block".to_owned());
            params.lrc_should_spawn_subagent = true;
            params.lrc_running_command = Some(RunningCommand {
                command: "vim notes.md".to_owned(),
                block_id: BlockId::new(),
                grid_contents: String::new(),
                cursor: String::new(),
                requested_command_id: None,
                is_alt_screen_active: true,
            });
            stream
        });
        view.ai_controller().update(ctx, |controller, ctx| {
            controller.register_mock_stream_for_test(
                stream_id.clone(),
                conversation_id,
                stream.clone(),
                ctx,
            );
        });
        history.update(ctx, |history, ctx| {
            history.mark_response_stream_completed_successfully(
                &stream_id,
                conversation_id,
                terminal_view_id,
                ctx,
            );
        });
        stream.update(ctx, |_, ctx| {
            ctx.emit(ResponseStreamEvent::AfterStreamFinished { cancellation: None });
        });
    });

    // Queued actions preprocess via `ctx.spawn`; pump until the write leaves preprocessing.
    let mut status = None;
    for _ in 0..64 {
        status = action_model.read(&*app, |action_model, _| {
            action_model.get_action_status(&action_id)
        });
        if status
            .as_ref()
            .is_some_and(|status| status.is_blocked() || status.is_running() || status.is_done())
        {
            break;
        }
        futures_lite::future::yield_now().await;
    }
    status
}

/// #1070: tagging the agent in to a full-screen program (vim, htop, less, tmux, a full-screen
/// ssh) must not let the agent type into it without the user's approval.
///
/// The controller used to queue an alt-screen tag-in round with an auto-accept that stood in
/// for the user's Accept, so with "Interact with running commands: Always ask" the model's
/// first `write_to_long_running_shell_command` went straight into the PTY of the program the
/// user was in. #617 narrowed that to the alt screen; it did not remove it. Red on that code:
/// the write executes (here: resolves `BlockNotFound`) instead of waiting for approval.
#[test]
fn alt_screen_tag_in_does_not_auto_accept_a_pty_write() {
    App::test((), |mut app| async move {
        let status = alt_screen_tag_in_write_status(&mut app, false).await;
        assert!(
            status.as_ref().is_some_and(|status| status.is_blocked()),
            "an alt-screen tag-in round must leave the PTY write waiting for the user's \
             approval, got {status:?}"
        );
    });
}

/// #1070, the other half: removing the forged Accept must not remove the user's own opt-in.
/// With "Interact with running commands: Always allow" the same round's write runs without a
/// prompt, through the ordinary profile check.
#[test]
fn alt_screen_tag_in_still_honours_an_always_allow_profile() {
    App::test((), |mut app| async move {
        let status = alt_screen_tag_in_write_status(&mut app, true).await;
        assert!(
            status
                .as_ref()
                .is_some_and(|status| status.is_running() || status.is_done()),
            "an explicit Always allow must still let the write run unprompted, got {status:?}"
        );
    });
}

/// #1070 (port of mehmetbaykar/zap `c401597cb`): only the CLI subagent's own turns -- the user's
/// tag-in turn, or a later turn targeting that subagent's task -- are limited to the CLI-agent
/// tool set. The agent that started or monitors the command keeps its tools.
#[test]
fn lrc_cli_subagent_turn_only_for_tag_in_or_the_subagent_task() {
    let subagent = TaskId::new("cli-subagent".to_owned());
    assert!(super::is_lrc_cli_subagent_turn(true, None, Some("root")));
    assert!(super::is_lrc_cli_subagent_turn(
        false,
        Some(&subagent),
        Some("cli-subagent")
    ));
    assert!(!super::is_lrc_cli_subagent_turn(
        false,
        Some(&subagent),
        Some("root")
    ));
    assert!(!super::is_lrc_cli_subagent_turn(
        false,
        Some(&subagent),
        None
    ));
    assert!(!super::is_lrc_cli_subagent_turn(false, None, Some("root")));
}

/// Sets up a terminal whose controller holds a restored BYOP conversation. The root task
/// (`root-task`) holds a user query and one tool call (`call_1`) whose result has not been
/// committed, and the active model is a usable BYOP provider pointed at a closed local port, so a
/// dispatched stream can never reach a real server. Returns the controller, the conversation, and
/// the follow-up `RequestInput` that carries `call_1`'s result, which is what
/// `send_follow_up_for_conversation` builds after a tool finishes.
fn byop_conversation_awaiting_tool_result(
    app: &mut App,
    persist_conversations: bool,
) -> (
    warpui::ModelHandle<super::BlocklistAIController>,
    crate::ai::agent::conversation::AIConversationId,
    super::RequestInput,
) {
    use settings::Setting as _;
    use warp_multi_agent_api as api;

    use crate::ai::agent_providers::llm_id;
    use crate::settings::{AISettings, AgentProvider, AgentProviderApiType, AgentProviderModel};
    use crate::terminal::general_settings::GeneralSettings;

    initialize_app_for_terminal_view(app);
    // `GlobalResourceHandles::mock` has no SQLite sender.
    let global_resource_handles = crate::GlobalResourceHandles::mock(app);
    app.add_singleton_model(|_| crate::GlobalResourceHandlesProvider::new(global_resource_handles));

    let provider_id = "provider-1066";
    app.update(|ctx| {
        AISettings::handle(ctx).update(ctx, |settings, ctx| {
            let provider = AgentProvider {
                id: provider_id.to_owned(),
                name: "Test Ollama".to_owned(),
                kind: Default::default(),
                api_type: AgentProviderApiType::Ollama,
                // Port 9 (discard) on loopback: nothing listens, so the spawned stream fails fast
                // instead of reaching a real model server on the test host.
                base_url: "http://127.0.0.1:9".to_owned(),
                models: vec![AgentProviderModel::from_id("llama3.2".to_owned())],
                extra_headers: Vec::new(),
                vertex_project: String::new(),
                vertex_location: String::new(),
                disabled: false,
                token_price: None,
            };
            settings
                .agent_providers
                .set_value(vec![provider], ctx)
                .expect("setting should update");
        });
        GeneralSettings::handle(ctx).update(ctx, |settings, ctx| {
            settings
                .persist_conversations
                .set_value(persist_conversations, ctx)
                .expect("setting should update");
        });
    });
    let model_id = llm_id::encode(provider_id, "llama3.2");
    app.read(|ctx| {
        assert!(
            crate::ai::agent_providers::lookup_byop(ctx, &model_id).is_some(),
            "the test provider must resolve, or the BYOP preflight is skipped"
        );
    });

    let message = |id: &str, message: api::message::Message| api::Message {
        id: id.to_owned(),
        task_id: "root-task".to_owned(),
        server_message_data: String::new(),
        citations: vec![],
        fetched_memories: vec![],
        message: Some(message),
        request_id: "request-1".to_owned(),
        timestamp: None,
    };
    let root_task = api::Task {
        id: "root-task".to_owned(),
        messages: vec![
            message(
                "user-query-1",
                api::message::Message::UserQuery(api::message::UserQuery {
                    query: "list files".to_owned(),
                    context: None,
                    referenced_attachments: Default::default(),
                    mode: None,
                    intended_agent: Default::default(),
                }),
            ),
            message(
                "tool-call-1",
                api::message::Message::ToolCall(api::message::ToolCall {
                    tool_call_id: "call_1".to_owned(),
                    tool: None,
                }),
            ),
        ],
        dependencies: None,
        description: String::new(),
        summary: String::new(),
        server_data: String::new(),
    };
    let conversation_id = crate::ai::agent::conversation::AIConversationId::new();
    let conversation = crate::ai::agent::conversation::AIConversation::new_restored(
        conversation_id,
        vec![root_task],
        None,
    )
    .expect("conversation should restore");

    let terminal = add_window_with_terminal(app, None);
    let terminal_view_id = terminal.read(app, |view, _| view.id());
    let controller = terminal.read(app, |view, _| view.ai_controller().clone());
    BlocklistAIHistoryModel::handle(app).update(app, |history, ctx| {
        history.restore_conversations(terminal_view_id, vec![conversation], ctx);
    });

    let task_id = TaskId::new("root-task".to_owned());
    let request_input = super::RequestInput {
        conversation_id,
        input_messages: HashMap::from([(
            task_id.clone(),
            vec![AIAgentInput::ActionResult {
                result: crate::ai::agent::AIAgentActionResult {
                    id: AIAgentActionId::from("call_1".to_owned()),
                    task_id,
                    result: crate::ai::agent::AIAgentActionResultType::InitProject,
                },
                context: std::sync::Arc::from(Vec::<AIAgentContext>::new()),
            }],
        )]),
        working_directory: None,
        model_id: model_id.clone(),
        coding_model_id: model_id.clone(),
        cli_agent_model_id: model_id.clone(),
        computer_use_model_id: model_id,
        shared_session_response_initiator: None,
        request_start_ts: chrono::Local::now(),
        supported_tools_override: None,
    };
    (controller, conversation_id, request_input)
}

/// How many `ToolCallResult`s for `call_1` the conversation's history holds.
fn call_1_result_count(
    app: &App,
    conversation_id: crate::ai::agent::conversation::AIConversationId,
) -> usize {
    BlocklistAIHistoryModel::handle(app).read(app, |history, _| {
        history
            .conversation(&conversation_id)
            .expect("conversation should exist")
            .all_tasks()
            .flat_map(|task| task.messages())
            .filter(|msg| {
                matches!(
                    msg.message.as_ref(),
                    Some(warp_multi_agent_api::message::Message::ToolCallResult(result))
                        if result.tool_call_id == "call_1"
                )
            })
            .count()
    })
}

/// #1066 end to end: with conversation persistence off, the follow-up that carries a tool result
/// passes the BYOP preflight, commits the result to history exactly once, and is dispatched. The
/// old gate refused the commit, so no stream opened and the turn stalled.
#[test]
fn byop_follow_up_with_tool_result_is_dispatched_when_persistence_is_off() {
    App::test((), |mut app| async move {
        let (controller, conversation_id, request_input) =
            byop_conversation_awaiting_tool_result(&mut app, false);

        controller.update(&mut app, |controller, ctx| {
            controller
                .send_request_input(
                    request_input,
                    None,
                    /*default_to_follow_up_on_success*/ false,
                    RecoveryBudget::fresh(),
                    /*is_queued_prompt*/ false,
                    ctx,
                )
                .expect("the follow-up should be sent");
            assert!(
                controller.has_active_stream_for_conversation(conversation_id, ctx),
                "the follow-up request must be dispatched"
            );
        });
        assert_eq!(
            call_1_result_count(&app, conversation_id),
            1,
            "the tool result must be in history exactly once"
        );
    });
}

/// #1066: persistence on but SQLite unavailable still blocks dispatch, and the controller now
/// renders the blocked request instead of returning an error that the follow-up path discarded.
/// (The old handler's `downcast_ref::<UpdateConversationError>()` missed the error, which arrives
/// wrapped in `UpdateHistoryError`.)
#[test]
fn byop_follow_up_is_blocked_and_rendered_when_expected_persistence_is_unavailable() {
    App::test((), |mut app| async move {
        let (controller, conversation_id, request_input) =
            byop_conversation_awaiting_tool_result(&mut app, true);

        controller.update(&mut app, |controller, ctx| {
            let sent = controller.send_request_input(
                request_input,
                None,
                /*default_to_follow_up_on_success*/ false,
                RecoveryBudget::fresh(),
                /*is_queued_prompt*/ false,
                ctx,
            );
            assert!(
                sent.is_ok(),
                "a persistence failure must render as a blocked request, got {sent:?}"
            );
            assert!(
                !controller.has_active_stream_for_conversation(conversation_id, ctx),
                "nothing may be dispatched on an in-memory-only result while persistence is on"
            );
        });
        assert_eq!(
            call_1_result_count(&app, conversation_id),
            0,
            "the refused commit must leave no result behind"
        );
    });
}

/// #1067: the dispatch gate's refusals must not auto-resume (a model that keeps naming a
/// withheld tool would loop on the user's API key), while the two existing resume triggers
/// still do.
#[test]
fn dispatch_refusals_do_not_auto_resume() {
    for code in [
        "tool_not_offered",
        "plan_mode_blocked",
        "computer_use_disabled",
    ] {
        let payload = serde_json::json!({
            "status": "error",
            "error": code,
            "tool": "websearch",
            "executed": false,
        })
        .to_string();
        assert!(
            !super::byop_synthetic_result_needs_resume(&payload),
            "{code} must not schedule an auto-resume: {payload}"
        );
    }
    let invalid = serde_json::json!({ "error": "invalid_arguments", "tool": "x" }).to_string();
    assert!(super::byop_synthetic_result_needs_resume(&invalid));
    let intercepted = serde_json::json!({ "status": "ok", "_byop_intercepted": true }).to_string();
    assert!(super::byop_synthetic_result_needs_resume(&intercepted));
    assert!(!super::byop_synthetic_result_needs_resume(
        r#"{"status":"ok"}"#
    ));
}

/// #1068: a rejected `read_skill` name auto-resumes at most
/// `MAX_SKILL_RESOLUTION_RESUMES_PER_TURN` times per turn, so a model (or injected tool
/// output) that keeps naming an unlisted skill cannot loop on the user's API key.
#[test]
fn skill_resolution_resumes_are_capped_per_turn() {
    let mut spent = 0u8;
    for attempt in 0..super::MAX_SKILL_RESOLUTION_RESUMES_PER_TURN {
        assert!(
            super::charge_skill_resolution_resume(&mut spent),
            "resume {attempt} is within the allowance"
        );
    }
    for _ in 0..3 {
        assert!(
            !super::charge_skill_resolution_resume(&mut spent),
            "past the allowance, no further resume"
        );
    }
    assert_eq!(spent, super::MAX_SKILL_RESOLUTION_RESUMES_PER_TURN);
    assert!(super::MAX_SKILL_RESOLUTION_RESUMES_PER_TURN > 0);
}

/// The cap applies to exactly the payload `chat_stream` emits for a rejected `read_skill`
/// name, and not to any other synthetic result (a schema-invalid call, an intercepted tool).
#[test]
fn skill_resolution_failures_are_recognised_by_their_reason() {
    for reason in ["skill_unavailable", "ambiguous_skill_name"] {
        let payload = serde_json::json!({
            "error": "invalid_arguments",
            "reason": reason,
            "tool": "read_skill",
            "executed": false,
        })
        .to_string();
        assert!(super::is_skill_resolution_failure(&payload), "{payload}");
    }
    let schema_error = serde_json::json!({
        "error": "invalid_arguments",
        "tool": "read_files",
        "executed": false,
    })
    .to_string();
    assert!(!super::is_skill_resolution_failure(&schema_error));
    let intercepted = serde_json::json!({ "_byop_intercepted": true, "status": "ok" }).to_string();
    assert!(!super::is_skill_resolution_failure(&intercepted));
}

// -- #1059: tool-loop auto-compaction -------------------------------------------------------
//
// Most of these drive the controller from the point the agent loop sends a step's tool
// results (`send_follow_up_for_conversation`, what the `FinishedAction` subscriber calls)
// with the conversation's compaction state set as `handle_response_stream_finished` leaves it
// after a tool-call step over the threshold. No BYOP provider is configured for them, so the
// BYOP preflight is skipped and they see the request *as the controller builds it*, with its
// `ActionResult` inputs. What production actually sends — the preflight records the results
// in history and strips them from the input — is covered end to end, with a BYOP provider
// configured and the real stream-finished wiring, by
// `tool_loop_over_the_window_compacts_with_its_results_through_the_byop_path`.

/// Sets up a controller with a fresh conversation, as `suspended_controller_sends_no_request_
/// until_lifted` does (the request path needs the global resource handles).
fn tool_loop_controller(
    app: &mut App,
) -> (
    warpui::ModelHandle<super::BlocklistAIController>,
    crate::ai::agent::conversation::AIConversationId,
    TaskId,
) {
    initialize_app_for_terminal_view(app);
    let global_resource_handles = crate::GlobalResourceHandles::mock(app);
    app.add_singleton_model(|_| crate::GlobalResourceHandlesProvider::new(global_resource_handles));
    let terminal = add_window_with_terminal(app, None);
    let terminal_view_id = terminal.read(&*app, |view, _| view.id());
    let controller = terminal.read(&*app, |view, _| view.ai_controller().clone());
    let (conversation_id, root_task_id) =
        BlocklistAIHistoryModel::handle(&*app).update(app, |history, ctx| {
            let conversation_id =
                history.start_new_conversation(terminal_view_id, false, false, ctx);
            let root_task_id = history
                .conversation(&conversation_id)
                .expect("conversation should exist")
                .get_root_task_id()
                .clone();
            (conversation_id, root_task_id)
        });
    (controller, conversation_id, root_task_id)
}

/// The state `handle_response_stream_finished` leaves after a tool-call step that ended over
/// the threshold with the allowance unspent.
fn defer_compaction_to_tool_results(
    controller: &mut super::BlocklistAIController,
    conversation_id: crate::ai::agent::conversation::AIConversationId,
) {
    let plan = controller.update_compaction_turn_state(conversation_id, |state| {
        state.on_stream_finished(crate::ai::byop_compaction::FinishedStream::Turn {
            requested_tool_calls: true,
            overflowing: Some(true),
        })
    });
    assert!(
        plan.compact_after_tool_results,
        "precondition: the step deferred its compaction"
    );
}

fn command_result(id: &str, task_id: &TaskId) -> crate::ai::agent::AIAgentActionResult {
    crate::ai::agent::AIAgentActionResult {
        id: AIAgentActionId::from(id.to_owned()),
        task_id: task_id.clone(),
        result: crate::ai::agent::AIAgentActionResultType::RequestCommandOutput(
            crate::ai::agent::RequestCommandOutputResult::Denylisted {
                command: format!("echo {id}"),
            },
        ),
    }
}

/// `summarization_overflow()` of every in-flight stream: `Some(true)` is an automatic summary,
/// `None` an ordinary request.
fn in_flight_summarization_flags(
    controller: &super::BlocklistAIController,
    app: &warpui::AppContext,
) -> Vec<Option<bool>> {
    controller
        .in_flight_response_streams
        .streams_for_test()
        .map(|stream| stream.as_ref(app).summarization_overflow())
        .collect()
}

/// The inputs of the exchange that a request carrying `SummarizeConversation` created, if any.
fn summary_exchange_inputs(
    conversation_id: crate::ai::agent::conversation::AIConversationId,
    app: &warpui::AppContext,
) -> Option<Vec<AIAgentInput>> {
    BlocklistAIHistoryModel::as_ref(app)
        .conversation(&conversation_id)?
        .all_exchanges()
        .into_iter()
        .find(|exchange| {
            exchange
                .input
                .iter()
                .any(|input| matches!(input, AIAgentInput::SummarizeConversation { .. }))
        })
        .map(|exchange| exchange.input.clone())
}

fn action_result_ids(inputs: &[AIAgentInput]) -> Vec<String> {
    let mut ids: Vec<String> = inputs
        .iter()
        .filter_map(|input| match input {
            AIAgentInput::ActionResult { result, .. } => Some(result.id.to_string()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids
}

/// #1059: the controller builds ONE request carrying the step's results (here two parallel
/// calls) together with the automatic summary, instead of a plain follow-up that keeps
/// growing. Every result is the real `ActionResult` (none dropped, none replaced by a
/// cancellation), and the request is an automatic summary (`overflow: true`), which is what
/// the #1060 marker and the continue-after-commit logic key on. (Without a BYOP provider the
/// preflight does not run, so the results are still inputs here; in production they are in
/// task history by the time the request is built — see the BYOP end-to-end test below.)
#[test]
fn deferred_compaction_builds_one_request_with_the_results_and_the_auto_summary() {
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);

        controller.update(&mut app, |controller, ctx| {
            defer_compaction_to_tool_results(controller, conversation_id);
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    command_result("call-a", &root_task_id),
                );
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    command_result("call-b", &root_task_id),
                );
            });

            controller.send_follow_up_for_conversation(
                conversation_id,
                super::FollowUpTrigger::Auto,
                ctx,
            );

            assert_eq!(
                in_flight_summarization_flags(controller, ctx),
                vec![Some(true)],
                "exactly one request, and it is the automatic summary"
            );
            let inputs = summary_exchange_inputs(conversation_id, ctx)
                .expect("the summary request appended an exchange");
            assert!(inputs.iter().any(|input| matches!(
                input,
                AIAgentInput::SummarizeConversation {
                    prompt: None,
                    overflow: true
                }
            )));
            assert_eq!(
                action_result_ids(&inputs),
                vec!["call-a".to_owned(), "call-b".to_owned()],
                "both parallel results travel with the summary"
            );
            let state = controller
                .compaction_turn_state
                .get(&conversation_id)
                .copied()
                .unwrap_or_default();
            assert!(state.auto_compacted(), "the turn's allowance is spent");
            assert!(
                !state.compaction_deferred_to_tool_results(),
                "the deferral is consumed"
            );
        });
    });
}

/// Without a deferral the results go out as the ordinary follow-up (positive control for the
/// test above: the summary comes from the deferral, not from the setup).
#[test]
fn tool_results_without_a_deferral_go_as_a_plain_follow_up() {
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);

        controller.update(&mut app, |controller, ctx| {
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    command_result("call-a", &root_task_id),
                );
            });
            controller.send_follow_up_for_conversation(
                conversation_id,
                super::FollowUpTrigger::Auto,
                ctx,
            );
            assert_eq!(in_flight_summarization_flags(controller, ctx), vec![None]);
            assert!(summary_exchange_inputs(conversation_id, ctx).is_none());
        });
    });
}

/// Stop / Ctrl-C while the step's tools run drops the deferral: the results that come in
/// afterwards are not turned into a summary behind the user's back.
#[test]
fn stop_drops_a_deferred_tool_loop_compaction() {
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);

        controller.update(&mut app, |controller, ctx| {
            defer_compaction_to_tool_results(controller, conversation_id);
            controller.cancel_conversation_progress(
                conversation_id,
                CancellationReason::ManuallyCancelled,
                ctx,
            );
            assert!(
                !controller
                    .compaction_turn_state
                    .contains_key(&conversation_id),
                "nothing left to remember: no deferral, allowance unspent"
            );

            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    command_result("call-a", &root_task_id),
                );
            });
            controller.send_follow_up_for_conversation(
                conversation_id,
                super::FollowUpTrigger::Auto,
                ctx,
            );
            assert_eq!(in_flight_summarization_flags(controller, ctx), vec![None]);
        });
    });
}

/// Results of a subagent task (not the conversation's root task) never carry the summary:
/// the summary input belongs to the root task, and a request spanning two tasks would append
/// one exchange per task. They go as a plain follow-up and the deferral is dropped.
#[test]
fn subagent_task_results_never_carry_the_auto_summary() {
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, _root_task_id) = tool_loop_controller(&mut app);

        controller.update(&mut app, |controller, ctx| {
            defer_compaction_to_tool_results(controller, conversation_id);
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    command_result("subagent-call", &TaskId::new("subagent-task".to_owned())),
                );
            });
            controller.send_follow_up_for_conversation(
                conversation_id,
                super::FollowUpTrigger::Auto,
                ctx,
            );
            assert!(
                in_flight_summarization_flags(controller, ctx)
                    .iter()
                    .all(Option::is_none),
                "no summary request for a subagent's results"
            );
            assert!(
                !controller
                    .compaction_turn_state
                    .contains_key(&conversation_id),
                "deferral dropped, allowance unspent"
            );
        });
    });
}

/// The user rejects the step's only tool call: no follow-up is sent, and the deferral is
/// dropped with it (otherwise it would summarize the next, unrelated follow-up).
#[test]
fn rejecting_the_only_tool_call_drops_the_deferred_compaction() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.id());
        let conversation_id = BlocklistAIHistoryModel::handle(&app)
            .update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });
        let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
        let action_model = controller.read(&app, |controller, _| controller.action_model.clone());

        let action_id = AIAgentActionId::from("reject-me".to_owned());
        action_model.update(&mut app, |action_model, ctx| {
            action_model.queue_action_for_test(
                AIAgentAction {
                    id: action_id.clone(),
                    task_id: TaskId::new("task".to_owned()),
                    action: shell_command(),
                    requires_result: false,
                },
                conversation_id,
                ctx,
            );
        });
        for _ in 0..64 {
            let is_blocked = action_model.read(&app, |action_model, _| {
                action_model
                    .get_action_status(&action_id)
                    .is_some_and(|status| status.is_blocked())
            });
            if is_blocked {
                break;
            }
            futures_lite::future::yield_now().await;
        }
        action_model.read(&app, |action_model, _| {
            assert!(
                action_model
                    .get_action_status(&action_id)
                    .is_some_and(|status| status.is_blocked()),
                "precondition: the command is blocked on confirmation"
            );
        });

        controller.update(&mut app, |controller, _| {
            defer_compaction_to_tool_results(controller, conversation_id);
        });
        action_model.update(&mut app, |action_model, ctx| {
            action_model.cancel_action_with_id(
                conversation_id,
                &action_id,
                CancellationReason::ManuallyCancelled,
                ctx,
            );
        });
        for _ in 0..64 {
            let dropped = controller.read(&app, |controller, _| {
                !controller
                    .compaction_turn_state
                    .contains_key(&conversation_id)
            });
            if dropped {
                break;
            }
            futures_lite::future::yield_now().await;
        }

        controller.read(&app, |controller, ctx| {
            assert!(
                !controller
                    .compaction_turn_state
                    .contains_key(&conversation_id),
                "the reject sent no follow-up, so the deferral must be gone"
            );
            assert!(!controller.has_active_stream_for_conversation(conversation_id, ctx));
        });
    });
}

fn timestamped_message(
    id: &str,
    message: warp_multi_agent_api::message::Message,
    seconds: i64,
) -> warp_multi_agent_api::Message {
    warp_multi_agent_api::Message {
        id: id.to_owned(),
        task_id: "root".to_owned(),
        request_id: "req".to_owned(),
        message: Some(message),
        timestamp: Some(prost_types::Timestamp { seconds, nanos: 0 }),
        ..Default::default()
    }
}

fn run_shell_tool_call(id: &str, call_id: &str, seconds: i64) -> warp_multi_agent_api::Message {
    use warp_multi_agent_api::message::{Message, ToolCall, tool_call};
    timestamped_message(
        id,
        Message::ToolCall(ToolCall {
            tool_call_id: call_id.to_owned(),
            tool: Some(tool_call::Tool::RunShellCommand(
                tool_call::RunShellCommand {
                    command: format!("cat file-{call_id}"),
                    ..Default::default()
                },
            )),
        }),
        seconds,
    )
}

fn stream_done(prompt_tokens: u32) -> warp_multi_agent_api::response_event::StreamFinished {
    use warp_multi_agent_api::response_event::stream_finished;
    warp_multi_agent_api::response_event::StreamFinished {
        reason: Some(stream_finished::Reason::Done(stream_finished::Done {})),
        token_usage: vec![stream_finished::TokenUsage {
            model_id: "mock-model".to_owned(),
            total_input: prompt_tokens,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// #1059 end to end through the BYOP path, on the issue's scenario: a provider with a
/// 32_000-token window, a step reporting 20_000 prompt tokens that asks for two (parallel)
/// tool calls.
///
/// 1. The real stream-finished wiring (`handle_response_stream_finished`, reading the tool
///    calls off the stream's own exchange and the window off the configured model) defers
///    the compaction instead of summarizing — no request, the calls are not cancelled.
/// 2. The step's results go out with the automatic summary. With a BYOP provider configured
///    the preflight runs: each tool call has exactly ONE real result in task history before
///    the summarize request is built (none synthesized, none cancelled), the request carries
///    no `ActionResult` any more, and the recorded summarization head covers the calls and
///    their results.
/// 3. The summary stream finishes with its own text: the commit records it (head included),
///    and exactly ONE continue request goes out.
///
/// No network: the BYOP requests are opened but their futures are never polled (this test
/// never awaits after sending).
#[test]
fn tool_loop_over_the_window_compacts_with_its_results_through_the_byop_path() {
    drive_tool_loop_through_the_byop_path(SummaryStreamOutcome::Summary);
}

/// #1059 refute round 3: a summarize stream that streams a sentence of narration and then a
/// tool call (providers whose genai adapter ignores `tool_choice`). The call is dropped, and
/// the narration is this stream's own AgentOutput — but it is not a summary, so nothing is
/// committed: the head (the step's calls and results included) stays visible. The deferred
/// summary still resumes the task: exactly one continue request.
#[test]
fn narration_before_a_dropped_tool_call_is_not_committed_as_the_summary() {
    drive_tool_loop_through_the_byop_path(SummaryStreamOutcome::NarrationThenToolCall);
}

/// What the summarize stream answers in [`drive_tool_loop_through_the_byop_path`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum SummaryStreamOutcome {
    /// A real summary.
    Summary,
    /// "Let me check" followed by a tool call, which the stream drops.
    NarrationThenToolCall,
}

fn drive_tool_loop_through_the_byop_path(outcome: SummaryStreamOutcome) {
    use crate::ai::agent_providers::{AgentProviderSecrets, llm_id, lookup_byop};
    use crate::ai::byop_compaction::CompactionConfig;
    use crate::ai::llms::LLMPreferences;
    use crate::features::FeatureFlag;
    use crate::settings::{AISettings, AgentProvider, AgentProviderApiType, AgentProviderModel};
    use settings::Setting;
    use warp_multi_agent_api as api;

    const PROVIDER_ID: &str = "tool-loop-provider";
    const MODEL_ID: &str = "tool-loop-model";

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        initialize_app_for_terminal_view(&mut app);
        // The BYOP preflight persists the results it records; it refuses without a sender.
        let (sender, _persisted_events) = std::sync::mpsc::sync_channel(4096);
        let mut global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
        global_resource_handles.model_event_sender = Some(sender);
        app.add_singleton_model(|_| {
            crate::GlobalResourceHandlesProvider::new(global_resource_handles)
        });
        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.id());
        let controller = terminal.read(&app, |view, _| view.ai_controller().clone());

        // A BYOP provider whose model has the issue's 32_000-token window, selected.
        let model_llm_id = llm_id::encode(PROVIDER_ID, MODEL_ID);
        app.update(|ctx| {
            let mut model = AgentProviderModel::from_id(MODEL_ID.to_owned());
            model.context_window = 32_000;
            let provider = AgentProvider {
                id: PROVIDER_ID.to_owned(),
                name: "Tool loop provider".to_owned(),
                kind: Default::default(),
                api_type: AgentProviderApiType::OpenAi,
                // Never contacted: the request futures are not polled.
                base_url: "http://127.0.0.1:9/v1".to_owned(),
                models: vec![model],
                extra_headers: Vec::new(),
                vertex_project: String::new(),
                vertex_location: String::new(),
                disabled: false,
                token_price: None,
            };
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings.agent_providers.set_value(vec![provider], ctx);
                let _ = settings
                    .byop_last_used_model_id
                    .set_value(model_llm_id.to_string(), ctx);
            });
            AgentProviderSecrets::handle(ctx).update(ctx, |secrets, ctx| {
                secrets.set(PROVIDER_ID, "not-a-real-key".to_owned(), ctx);
            });
        });
        app.read(|ctx| {
            let active = LLMPreferences::as_ref(ctx)
                .get_active_base_model(ctx, Some(terminal_view_id))
                .id
                .clone();
            assert_eq!(
                active, model_llm_id,
                "precondition: the BYOP model is selected"
            );
            assert!(
                lookup_byop(ctx, &active).is_some(),
                "precondition: BYOP resolves"
            );
            assert_eq!(
                CompactionConfig::model_limit(ctx).context,
                32_000,
                "precondition: the window comes from the configured model"
            );
        });

        // The tool-call step: a user query, then a stream that adds two tool calls, exactly
        // as the BYOP stream applies them (StreamInit, CreateTask upgrading the optimistic
        // root, AddMessagesToTask).
        let step_stream = ResponseStreamId::new_for_test();
        let conversation_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
                let conversation_id =
                    history.start_new_conversation(terminal_view_id, false, false, ctx);
                let conversation = history
                    .conversation_mut(&conversation_id)
                    .expect("conversation should exist");
                let optimistic_root = conversation.get_root_task_id().clone();
                conversation
                    .update_for_new_request_input(
                        super::RequestInput {
                            conversation_id,
                            input_messages: HashMap::from([(
                                optimistic_root,
                                vec![AIAgentInput::UserQuery {
                                    query: "read both files".to_owned(),
                                    context: Default::default(),
                                    static_query_type: None,
                                    referenced_attachments: Default::default(),
                                    user_query_mode: UserQueryMode::default(),
                                    running_command: None,
                                    intended_agent: None,
                                }],
                            )]),
                            working_directory: None,
                            model_id: model_llm_id.clone(),
                            coding_model_id: model_llm_id.clone(),
                            cli_agent_model_id: model_llm_id.clone(),
                            computer_use_model_id: model_llm_id.clone(),
                            shared_session_response_initiator: None,
                            request_start_ts: chrono::Local::now(),
                            supported_tools_override: None,
                        },
                        step_stream.clone(),
                        terminal_view_id,
                        ctx,
                    )
                    .expect("the step's request should be accepted");
                conversation
                    .initialize_output_for_response_stream(
                        &step_stream,
                        api::response_event::StreamInit {
                            request_id: String::new(),
                            conversation_id: String::new(),
                            run_id: String::new(),
                        },
                        terminal_view_id,
                        ctx,
                    )
                    .expect("stream init should succeed");
                history
                    .apply_client_actions(
                        &step_stream,
                        vec![
                            api::ClientAction {
                                action: Some(api::client_action::Action::CreateTask(
                                    api::client_action::CreateTask {
                                        task: Some(api::Task {
                                            id: "root".to_owned(),
                                            ..Default::default()
                                        }),
                                    },
                                )),
                            },
                            api::ClientAction {
                                action: Some(api::client_action::Action::AddMessagesToTask(
                                    api::client_action::AddMessagesToTask {
                                        task_id: "root".to_owned(),
                                        messages: vec![
                                            timestamped_message(
                                                "q1",
                                                api::message::Message::UserQuery(
                                                    api::message::UserQuery {
                                                        query: "read both files".to_owned(),
                                                        ..Default::default()
                                                    },
                                                ),
                                                1,
                                            ),
                                            run_shell_tool_call("c1", "call-1", 2),
                                            run_shell_tool_call("c2", "call-2", 3),
                                        ],
                                    },
                                )),
                            },
                        ],
                        conversation_id,
                        terminal_view_id,
                        &ai::skills::SkillPathOrigin::Unavailable,
                        ctx,
                    )
                    .expect("the step's messages should apply");
                conversation_id
            });
        let root_task_id = TaskId::new("root".to_owned());

        controller.update(&mut app, |controller, ctx| {
            // 1. The step finishes over the threshold (20_000 of 32_000 with the default
            //    reserve) asking for tool calls.
            controller.handle_response_stream_finished(
                &step_stream,
                stream_done(20_000),
                conversation_id,
                /*did_request_contain_user_query*/ true,
                /*summarize_overflow*/ None,
                ctx,
            );
            assert!(
                controller
                    .compaction_turn_state
                    .get(&conversation_id)
                    .is_some_and(|state| state.compaction_deferred_to_tool_results()),
                "the real stream-finished wiring defers the compaction to the step's results"
            );
            assert_eq!(
                controller.in_flight_response_streams.streams_for_test().count(),
                0,
                "nothing is summarized while the step's calls are pending"
            );

            // 2. Both calls ran; their results go out.
            controller.action_model.update(ctx, |action_model, _| {
                for call_id in ["call-1", "call-2"] {
                    action_model
                        .push_finished_action_result_for_test(conversation_id, command_result(call_id, &root_task_id));
                }
            });
            controller.send_follow_up_for_conversation(
                conversation_id,
                super::FollowUpTrigger::Auto,
                ctx,
            );

            let streams: Vec<_> = controller
                .in_flight_response_streams
                .streams_for_test()
                .cloned()
                .collect();
            assert_eq!(streams.len(), 1, "one request: the summary with the results");
            let summary_stream = streams[0].clone();
            assert_eq!(summary_stream.as_ref(ctx).summarization_overflow(), Some(true));
            let summary_params = summary_stream.as_ref(ctx).params_for_test().clone();
            assert!(
                !summary_params
                    .input
                    .iter()
                    .any(|input| matches!(input, AIAgentInput::ActionResult { .. })),
                "the preflight recorded the results in history and took them out of the input"
            );

            let history = BlocklistAIHistoryModel::as_ref(ctx);
            let conversation = history.conversation(&conversation_id).unwrap();
            let messages: Vec<&warp_multi_agent_api::Message> = conversation
                .get_task(&root_task_id)
                .expect("root task")
                .messages()
                .collect();
            let call_ids: Vec<String> = messages
                .iter()
                .filter_map(|msg| match msg.message.as_ref() {
                    Some(api::message::Message::ToolCall(call)) => Some(call.tool_call_id.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(call_ids, ["call-1", "call-2"]);
            let mut result_message_ids = Vec::new();
            for call_id in &call_ids {
                let results: Vec<&&warp_multi_agent_api::Message> = messages
                    .iter()
                    .filter(|msg| {
                        matches!(
                            msg.message.as_ref(),
                            Some(api::message::Message::ToolCallResult(result))
                                if &result.tool_call_id == call_id
                        )
                    })
                    .collect();
                assert_eq!(results.len(), 1, "{call_id}: exactly one result in history");
                assert!(
                    !results[0].server_message_data.contains("interrupted_by_user")
                        && !results[0].server_message_data.contains("\"cancelled\""),
                    "{call_id}: the real result, not a synthesized cancellation"
                );
                result_message_ids.push(results[0].id.clone());
            }
            assert_eq!(
                messages
                    .iter()
                    .filter(|msg| matches!(
                        msg.message.as_ref(),
                        Some(api::message::Message::ToolCallResult(_))
                    ))
                    .count(),
                2,
                "no other (orphan or cancellation) results"
            );
            // The summarize request is built from that history: its recorded head covers
            // the calls and their results.
            crate::ai::agent_providers::chat_stream::build_chat_request_for_test(&summary_params)
                .expect("the summarize request builds");
            let head = crate::ai::agent_providers::chat_stream::take_last_summarization_head(
                conversation_id,
            )
            .expect("the summarize request recorded its head");
            for id in ["c1", "c2"]
                .iter()
                .map(|id| id.to_string())
                .chain(result_message_ids.iter().cloned())
            {
                assert!(head.head_message_ids.contains(&id), "{id} is summarized");
            }
            // Re-record it for the commit below, as the stream itself would have.
            crate::ai::agent_providers::chat_stream::build_chat_request_for_test(&summary_params)
                .expect("the summarize request builds");

            // 3. The summary stream answers and finishes.
            let summary_text = match outcome {
                SummaryStreamOutcome::Summary => "## Goal\n- read both files",
                SummaryStreamOutcome::NarrationThenToolCall => "Let me check the next file.",
            };
            if outcome == SummaryStreamOutcome::NarrationThenToolCall {
                // What `generate_byop_output` does with the call at the end of the stream.
                let dispatched = crate::ai::agent_providers::chat_stream::tool_calls_to_dispatch_for_test(
                    &summary_params,
                    vec!["call-3"],
                );
                assert_eq!(dispatched, 0, "a summarize stream dispatches no tool call");
            }
            let summary_stream_id = summary_stream.as_ref(ctx).id().clone();
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history
                    .conversation_mut(&conversation_id)
                    .unwrap()
                    .initialize_output_for_response_stream(
                        &summary_stream_id,
                        api::response_event::StreamInit {
                            request_id: String::new(),
                            conversation_id: String::new(),
                            run_id: String::new(),
                        },
                        terminal_view_id,
                        ctx,
                    )
                    .expect("summary stream init should succeed");
                history
                    .apply_client_actions(
                        &summary_stream_id,
                        vec![api::ClientAction {
                            action: Some(api::client_action::Action::AddMessagesToTask(
                                api::client_action::AddMessagesToTask {
                                    task_id: "root".to_owned(),
                                    messages: vec![timestamped_message(
                                        "s1",
                                        api::message::Message::AgentOutput(
                                            api::message::AgentOutput {
                                                text: summary_text.to_owned(),
                                            },
                                        ),
                                        4,
                                    )],
                                },
                            )),
                        }],
                        conversation_id,
                        terminal_view_id,
                        &ai::skills::SkillPathOrigin::Unavailable,
                        ctx,
                    )
                    .expect("the summary text should apply");
            });
            controller.handle_response_stream_finished(
                &summary_stream_id,
                stream_done(3_000),
                conversation_id,
                /*did_request_contain_user_query*/ false,
                /*summarize_overflow*/ Some(true),
                ctx,
            );

            let history = BlocklistAIHistoryModel::as_ref(ctx);
            let completed = history
                .conversation(&conversation_id)
                .unwrap()
                .compaction_state
                .completed()
                .last()
                .cloned();
            match outcome {
                SummaryStreamOutcome::Summary => {
                    let completed = completed.expect("the summary committed");
                    assert_eq!(completed.assistant_msg_id, "s1", "the stream's own text");
                    assert!(completed.auto);
                    for id in ["c1", "c2"]
                        .iter()
                        .map(|id| id.to_string())
                        .chain(result_message_ids.iter().cloned())
                    {
                        assert!(completed.head_message_ids.contains(&id), "{id} is compacted");
                    }
                }
                SummaryStreamOutcome::NarrationThenToolCall => {
                    assert!(
                        completed.is_none(),
                        "narration before a dropped tool call is not a summary"
                    );
                    assert!(
                        crate::ai::agent_providers::chat_stream::take_last_summarization_head(
                            conversation_id
                        )
                        .is_none(),
                        "the uncommitted summary's recorded head is dropped"
                    );
                }
            }

            let continue_text = crate::ai::byop_compaction::prompt::build_continue_message(false);
            let continue_requests = controller
                .in_flight_response_streams
                .streams_for_test()
                .filter(|stream| {
                    stream.as_ref(ctx).params_for_test().input.iter().any(|input| {
                        matches!(input, AIAgentInput::UserQuery { query, .. } if *query == continue_text)
                    })
                })
                .count();
            assert_eq!(continue_requests, 1, "exactly one continue request after the commit");
        });
    });
}
