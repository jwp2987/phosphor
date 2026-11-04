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
                super::read_attachments(prompt_attachments),
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
    assert!(!switches(
        false,
        CancellationReason::OptimisticCLISubagentCompletion
    ));
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
    use agent_proto as api;
    use settings::Setting as _;

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
                    Some(agent_proto::message::Message::ToolCallResult(result))
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

/// #1074 (e35f40c2c): the results of a subagent task (not the conversation's root task) carry
/// the summary in that task, so a CLI/LRC subagent loop that never pauses is compacted too and
/// resumes inside the subagent. (Before #1074 only root-task results carried it.)
#[test]
fn subagent_task_results_carry_the_auto_summary_in_their_own_task() {
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, _root_task_id) = tool_loop_controller(&mut app);

        // The task must exist in the conversation: the summary exchange is appended to it, and an
        // unknown task id fails that append, so no stream is tracked and the allowance stays unspent.
        let subagent_task_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, _| {
                history
                    .create_silent_cli_subagent_task_for_conversation(
                        crate::terminal::model::block::BlockId::new(),
                        conversation_id,
                    )
                    .expect("subagent task should be created")
            });

        controller.update(&mut app, |controller, ctx| {
            defer_compaction_to_tool_results(controller, conversation_id);
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    command_result("subagent-call", &subagent_task_id),
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
                "the subagent's results go with the automatic summary"
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

/// Results spanning several tasks have no single task to carry the summary (a request spanning
/// two tasks would append one exchange per task): they go as a plain follow-up and the
/// deferral is dropped. This is the case the pre-#1074 test of the subagent-only variant
/// covered; it is still the rule.
#[test]
fn results_spanning_tasks_never_carry_the_auto_summary() {
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);

        controller.update(&mut app, |controller, ctx| {
            defer_compaction_to_tool_results(controller, conversation_id);
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    command_result("root-call", &root_task_id),
                );
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
                "no summary request for results spanning tasks"
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
    message: agent_proto::message::Message,
    seconds: i64,
) -> agent_proto::Message {
    agent_proto::Message {
        id: id.to_owned(),
        task_id: "root".to_owned(),
        request_id: "req".to_owned(),
        message: Some(message),
        timestamp: Some(prost_types::Timestamp { seconds, nanos: 0 }),
        ..Default::default()
    }
}

fn run_shell_tool_call(id: &str, call_id: &str, seconds: i64) -> agent_proto::Message {
    use agent_proto::message::{Message, ToolCall, tool_call};
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

fn stream_done(prompt_tokens: u32) -> agent_proto::response_event::StreamFinished {
    use agent_proto::response_event::stream_finished;
    agent_proto::response_event::StreamFinished {
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
/// 32_000-token window, a step reporting 26_000 prompt tokens that asks for two (parallel)
/// tool calls (the issue's 20_000 is under the #1090 threshold of 24_000).
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
/// summary is then asked for once more without tools (#1079, e35f40c2c) instead of resuming
/// the task uncompacted: one retry request, no continue request yet.
#[test]
fn narration_before_a_dropped_tool_call_is_not_committed_as_the_summary() {
    drive_tool_loop_through_the_byop_path(SummaryStreamOutcome::NarrationThenToolCall);
}

/// #1078: an older turn (a question and a large answer) precedes the tool-loop turn, so the
/// summary covers only the older turn and the current one is the kept tail. The current
/// turn's instruction, calls and results must reach the continue request, after the summary.
/// Before the fix the summary was anchored at the current turn's user message, which was then
/// hidden although it was never summarized.
#[test]
fn the_kept_current_turn_survives_the_compaction_after_the_summary() {
    drive_tool_loop_through_the_byop_path(SummaryStreamOutcome::SummaryOfAnOlderTurn);
}

/// What the summarize stream answers in [`drive_tool_loop_through_the_byop_path`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum SummaryStreamOutcome {
    /// A real summary.
    Summary,
    /// "Let me check" followed by a tool call, which the stream drops.
    NarrationThenToolCall,
    /// A real summary, of an older turn that precedes the tool-loop turn (#1078).
    SummaryOfAnOlderTurn,
}

fn drive_tool_loop_through_the_byop_path(outcome: SummaryStreamOutcome) {
    use crate::ai::agent_providers::{AgentProviderSecrets, llm_id, lookup_byop};
    use crate::ai::byop_compaction::CompactionConfig;
    use crate::ai::llms::LLMPreferences;
    use crate::features::FeatureFlag;
    use crate::settings::{AISettings, AgentProvider, AgentProviderApiType, AgentProviderModel};
    use agent_proto as api;
    use settings::Setting;

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
        // #1078: an older, large turn the summary will cover, ahead of the tool-loop turn.
        let older_turn: Vec<api::Message> = if outcome == SummaryStreamOutcome::SummaryOfAnOlderTurn
        {
            vec![
                timestamped_message(
                    "u0",
                    api::message::Message::UserQuery(api::message::UserQuery {
                        query: "explain the build".to_owned(),
                        ..Default::default()
                    }),
                    -2,
                ),
                timestamped_message(
                    "a0",
                    api::message::Message::AgentOutput(api::message::AgentOutput {
                        // Over the summarize request's 6_000-token preserve budget, so the
                        // older turn is summarized and the tool-loop turn is kept.
                        text: "the build works like this. ".repeat(2_000),
                    }),
                    -1,
                ),
            ]
        } else {
            Vec::new()
        };
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
                                        messages: older_turn
                                            .clone()
                                            .into_iter()
                                            .chain([
                                                timestamped_message(
                                                    "q1",
                                                    api::message::Message::UserQuery(
                                                        api::message::UserQuery {
                                                            // ~4_800 estimated tokens of
                                                            // pasted context: history a
                                                            // summary can replace (#1090),
                                                            // and with the calls and results
                                                            // still within the 6_000-token
                                                            // kept-tail budget, so a lone
                                                            // turn is summarized whole.
                                                            query: format!(
                                                                "read both files{}",
                                                                " with this pasted context in mind"
                                                                    .repeat(580)
                                                            ),
                                                            ..Default::default()
                                                        },
                                                    ),
                                                    1,
                                                ),
                                                run_shell_tool_call("c1", "call-1", 2),
                                                run_shell_tool_call("c2", "call-2", 3),
                                            ])
                                            .collect(),
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
            // 1. The step finishes over the threshold asking for tool calls: 26_000 of
            //    32_000, where the default reserve (a quarter of the window, #1090) puts the
            //    threshold at 24_000, and the ~4_900-token turn is what a summary would
            //    replace (26_000 - 4_900 + a 2_000 summary is back under 24_000).
            controller.handle_response_stream_finished(
                &step_stream,
                stream_done(26_000),
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
            let messages: Vec<&agent_proto::Message> = conversation
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
                let results: Vec<&&agent_proto::Message> = messages
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
            // The summarize request is built from that history, with the configured model's
            // limits as `generate_byop_output` passes them (a 6_000-token kept tail on this
            // 32_000 window): its recorded head covers the calls and their results.
            crate::ai::agent_providers::chat_stream::build_chat_request_for_test_with_limit(
                &summary_params,
                CompactionConfig::model_limit(ctx),
            )
            .expect("the summarize request builds");
            let head = crate::ai::agent_providers::chat_stream::take_last_summarization_head(
                conversation_id,
            )
            .expect("the summarize request recorded its head");
            if outcome == SummaryStreamOutcome::SummaryOfAnOlderTurn {
                assert_eq!(head.head_message_ids, ["u0", "a0"], "only the older turn");
                assert_eq!(head.tail_start_id.as_deref(), Some("q1"));
            } else {
                for id in ["c1", "c2"]
                    .iter()
                    .map(|id| id.to_string())
                    .chain(result_message_ids.iter().cloned())
                {
                    assert!(head.head_message_ids.contains(&id), "{id} is summarized");
                }
            }
            // Re-record it for the commit below, as the stream itself would have.
            crate::ai::agent_providers::chat_stream::build_chat_request_for_test(&summary_params)
                .expect("the summarize request builds");

            // 3. The summary stream answers and finishes.
            let summary_text = match outcome {
                SummaryStreamOutcome::Summary => "## Goal\n- read both files",
                SummaryStreamOutcome::SummaryOfAnOlderTurn => "## Goal\n- explain the build",
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
                SummaryStreamOutcome::SummaryOfAnOlderTurn => {
                    let completed = completed.expect("the summary committed");
                    assert_eq!(completed.head_message_ids, ["u0", "a0"]);
                    assert_eq!(
                        completed.user_msg_id, "u0",
                        "anchored inside the head it summarizes"
                    );
                    let hidden = history
                        .conversation(&conversation_id)
                        .unwrap()
                        .compaction_state
                        .hidden_message_ids();
                    for kept in ["q1", "c1", "c2"]
                        .iter()
                        .map(|id| id.to_string())
                        .chain(result_message_ids.iter().cloned())
                    {
                        assert!(!hidden.contains(&kept), "{kept} is kept and stays visible");
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
            if outcome == SummaryStreamOutcome::NarrationThenToolCall {
                // #1079 (e35f40c2c): a summary the model answered with a tool call is asked for
                // once more, without tools, and the task resumes only after that one commits.
                assert_eq!(continue_requests, 0, "no continue before the retried summary");
                let retries = controller
                    .in_flight_response_streams
                    .streams_for_test()
                    .filter(|stream| {
                        stream.as_ref(ctx).params_for_test().input.iter().any(|input| {
                            matches!(
                                input,
                                AIAgentInput::SummarizeConversation { prompt: Some(prompt), .. }
                                    if prompt.as_str() == crate::ai::byop_compaction::prompt::SUMMARY_RETRY_INSTRUCTION
                            )
                        })
                    })
                    .count();
                assert_eq!(retries, 1, "the summary is asked for once more, without tools");
            } else {
                assert_eq!(continue_requests, 1, "exactly one continue request after the commit");
            }

            if outcome == SummaryStreamOutcome::SummaryOfAnOlderTurn {
                // What the continue request sends: the summary first, then the kept turn's
                // instruction, then the continue message.
                let continue_params = controller
                    .in_flight_response_streams
                    .streams_for_test()
                    .map(|stream| stream.as_ref(ctx).params_for_test().clone())
                    .find(|params| {
                        params.input.iter().any(|input| {
                            matches!(input, AIAgentInput::UserQuery { query, .. } if *query == continue_text)
                        })
                    })
                    .expect("the continue request");
                let request =
                    crate::ai::agent_providers::chat_stream::build_chat_request_for_test(
                        &continue_params,
                    )
                    .expect("the continue request builds");
                let texts: Vec<String> = request
                    .messages
                    .iter()
                    .map(|m| m.content.first_text().unwrap_or_default().to_owned())
                    .collect();
                let position = |needle: &str| {
                    texts
                        .iter()
                        .position(|text| text.contains(needle))
                        .unwrap_or_else(|| panic!("{needle:?} is not in the continue request"))
                };
                let summary_at = position("explain the build");
                let instruction_at = position("read both files");
                let continue_at = position(&continue_text);
                assert!(
                    summary_at < instruction_at && instruction_at < continue_at,
                    "summary ({summary_at}) < kept instruction ({instruction_at}) < continue ({continue_at})"
                );
                assert!(
                    !texts.iter().any(|text| text.contains("the build works like this")),
                    "the summarized answer itself is hidden"
                );
            }
        });
    });
}

/// #1178: the facts under which a finished monitored command is reported to the model.
mod monitored_completion {
    use super::super::{MonitoredCompletionDecision, MonitoredCompletionFacts, silent_stop_reason};
    use super::*;

    fn idle_after_text_reply() -> MonitoredCompletionFacts {
        MonitoredCompletionFacts {
            last_turn_succeeded: true,
            ..Default::default()
        }
    }

    /// The reported case: the model answered the "running" snapshot with text and ended its
    /// turn, then the command finished. Fails on the old code, which sent nothing.
    #[test]
    fn sends_one_follow_up_when_the_turn_already_ended() {
        assert!(idle_after_text_reply().should_follow_up());
    }

    #[test]
    fn never_sends_a_second_follow_up_for_the_same_block() {
        let facts = MonitoredCompletionFacts {
            already_followed_up: true,
            ..idle_after_text_reply()
        };
        assert!(!facts.should_follow_up());
    }

    #[test]
    fn guarded_cases_send_nothing() {
        let base = idle_after_text_reply();
        // The model is mid-turn (a new user turn, or it polled the output itself).
        assert!(
            !MonitoredCompletionFacts {
                stream_in_flight: true,
                ..base
            }
            .should_follow_up()
        );
        // The user stopped the conversation, it failed, or a new turn is in progress.
        assert!(
            !MonitoredCompletionFacts {
                last_turn_succeeded: false,
                ..base
            }
            .should_follow_up()
        );
        // The pane is closed.
        assert!(
            !MonitoredCompletionFacts {
                suspended_for_close: true,
                ..base
            }
            .should_follow_up()
        );
        // A compaction is under way.
        assert!(
            !MonitoredCompletionFacts {
                mid_compaction: true,
                ..base
            }
            .should_follow_up()
        );
        // Results are on their way to the model already.
        assert!(
            !MonitoredCompletionFacts {
                has_unsent_actions: true,
                ..base
            }
            .should_follow_up()
        );
    }

    fn result_of(
        result: crate::ai::agent::AIAgentActionResultType,
    ) -> std::sync::Arc<crate::ai::agent::AIAgentActionResult> {
        std::sync::Arc::new(crate::ai::agent::AIAgentActionResult {
            id: AIAgentActionId::from("call".to_owned()),
            task_id: TaskId::new("task".to_owned()),
            result,
        })
    }

    fn completed_130(interrupted: bool) -> crate::ai::agent::AIAgentActionResultType {
        crate::ai::agent::AIAgentActionResultType::RequestCommandOutput(
            crate::ai::agent::RequestCommandOutputResult::Completed {
                block_id: crate::terminal::model::block::BlockId::new(),
                command: "sh -c 'exit 130'".to_owned(),
                output: String::new(),
                exit_code: 130.into(),
                start_ts: None,
                completed_ts: None,
                interrupted,
            },
        )
    }

    /// B: a self-exited 130 produces a normal result, so a request follows and nothing is
    /// reported as a silent stop; a user interrupt does not trigger one.
    #[test]
    fn self_exited_130_continues_and_an_interrupt_does_not() {
        let self_exit = result_of(completed_130(false));
        assert!(self_exit.result.should_trigger_request_upon_completion());
        assert_eq!(silent_stop_reason(None, &[self_exit]), None);

        let interrupted = result_of(completed_130(true));
        assert!(!interrupted.result.should_trigger_request_upon_completion());
    }

    /// A turn that ends with a cancelled result nobody asked for says why; a deliberate stop
    /// (a cancellation reason is set) stays quiet.
    #[test]
    fn unrequested_cancelled_result_reports_a_stop_reason() {
        let interrupted = result_of(completed_130(true));
        let reason = silent_stop_reason(None, std::slice::from_ref(&interrupted))
            .expect("an unrequested cancelled result must be reported");
        assert!(reason.contains("sh -c 'exit 130'"), "{reason}");
        assert_eq!(
            silent_stop_reason(Some(CancellationReason::ManuallyCancelled), &[interrupted]),
            None
        );
    }

    /// A request about the conversation is in flight or coming -- the model mid-reply, a tool
    /// step, a compaction, a scheduled resume: the completion is held for that request's end,
    /// never dropped. Fails on the old code, which only knew send-or-nothing.
    #[test]
    fn a_completion_while_a_request_is_in_flight_or_coming_is_deferred() {
        use MonitoredCompletionDecision as D;
        let base = idle_after_text_reply();
        // Mid-reply the status is still `InProgress`.
        assert_eq!(
            MonitoredCompletionFacts {
                stream_in_flight: true,
                last_turn_succeeded: false,
                ..base
            }
            .decision(),
            D::Defer
        );
        assert_eq!(
            MonitoredCompletionFacts {
                mid_compaction: true,
                ..base
            }
            .decision(),
            D::Defer
        );
        assert_eq!(
            MonitoredCompletionFacts {
                has_unsent_actions: true,
                ..base
            }
            .decision(),
            D::Defer
        );
        assert_eq!(
            MonitoredCompletionFacts {
                resume_scheduled: true,
                ..base
            }
            .decision(),
            D::Defer
        );
    }

    /// Nothing coming and the turn did not end normally (the user stopped it, it failed), or
    /// the block was already reported, or the pane is closed: dropped, whatever else is going on.
    #[test]
    fn a_stopped_or_failed_turn_with_nothing_coming_drops_the_completion() {
        use MonitoredCompletionDecision as D;
        assert_eq!(MonitoredCompletionFacts::default().decision(), D::Drop);
        assert_eq!(
            MonitoredCompletionFacts {
                already_followed_up: true,
                stream_in_flight: true,
                ..idle_after_text_reply()
            }
            .decision(),
            D::Drop
        );
        assert_eq!(
            MonitoredCompletionFacts {
                suspended_for_close: true,
                stream_in_flight: true,
                ..idle_after_text_reply()
            }
            .decision(),
            D::Drop
        );
        assert_eq!(idle_after_text_reply().decision(), D::Send);
    }

    /// The controller holds a completion that arrives while a stream is in flight, keeps it
    /// across the command's own optimistic completion, and drops it when the user stops the
    /// conversation. Fails on the old code: the first call returned false and nothing was kept.
    #[test]
    fn a_completion_during_the_models_reply_is_held_until_the_user_stops_or_the_turn_ends() {
        App::test((), |mut app| async move {
            initialize_app_for_terminal_view(&mut app);
            // Stopping a conversation with a live exchange persists it, which reads this singleton.
            let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
            app.add_singleton_model(|_| {
                crate::GlobalResourceHandlesProvider::new(global_resource_handles)
            });
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
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
                let block_id = crate::terminal::model::block::BlockId::new();
                view.ai_controller().update(ctx, |controller, ctx| {
                    controller.register_mock_stream_for_test(
                        stream_id,
                        conversation_id,
                        stream,
                        ctx,
                    );
                    assert!(
                        !controller.follow_up_on_monitored_command_completed(
                            conversation_id,
                            &block_id,
                            ctx
                        ),
                        "nothing is sent into a running stream"
                    );
                    assert!(
                        controller.has_deferred_monitored_completion(conversation_id),
                        "the completion is held for the stream's end"
                    );
                    // The command finishing is not the user acting: the hold survives.
                    controller.cancel_conversation_progress(
                        conversation_id,
                        CancellationReason::OptimisticCLISubagentCompletion,
                        ctx,
                    );
                    assert!(controller.has_deferred_monitored_completion(conversation_id));
                    // The user stopping (or sending a new prompt) drops it: nothing resumes on
                    // their behalf.
                    controller.cancel_conversation_progress(
                        conversation_id,
                        CancellationReason::ManuallyCancelled,
                        ctx,
                    );
                    assert!(!controller.has_deferred_monitored_completion(conversation_id));
                });
            });
        });
    }

    /// #1264: a user ^C inside a monitored command is not news that restarts a turn.
    #[test]
    fn a_command_the_user_interrupted_with_ctrl_c_sends_nothing() {
        let facts = MonitoredCompletionFacts {
            interrupted_by_ctrl_c: true,
            ..idle_after_text_reply()
        };
        assert!(!facts.should_follow_up());
        assert_eq!(facts.decision(), MonitoredCompletionDecision::Drop);
    }

    /// #1264: a second command finishing while one is held does not erase the first. Fails on
    /// the old code, where the second insert replaced the first block id.
    #[test]
    fn a_second_completion_does_not_replace_the_held_one() {
        App::test((), |mut app| async move {
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
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
                let first = crate::terminal::model::block::BlockId::new();
                let second = crate::terminal::model::block::BlockId::new();
                view.ai_controller().update(ctx, |controller, ctx| {
                    controller.register_mock_stream_for_test(
                        stream_id,
                        conversation_id,
                        stream,
                        ctx,
                    );
                    controller.follow_up_on_monitored_command_completed(
                        conversation_id,
                        &first,
                        ctx,
                    );
                    controller.follow_up_on_monitored_command_completed(
                        conversation_id,
                        &second,
                        ctx,
                    );
                    controller.follow_up_on_monitored_command_completed(
                        conversation_id,
                        &second,
                        ctx,
                    );
                    assert_eq!(
                        controller.deferred_monitored_blocks_for_test(conversation_id),
                        vec![first, second],
                        "both are held, in order, once each"
                    );
                });
            });
        });
    }

    /// #1264: finished results a step kept back (no request follows them) do not hold a
    /// completion; actions still pending or running always do, and finished results do while a
    /// request to send them is coming.
    #[test]
    fn retained_results_block_only_while_a_request_is_coming() {
        use super::super::unsent_actions_block;
        assert!(unsent_actions_block(true, false, false));
        assert!(unsent_actions_block(true, false, true));
        assert!(unsent_actions_block(false, true, true));
        assert!(!unsent_actions_block(false, true, false));
        assert!(!unsent_actions_block(false, false, true));
    }

    /// #1264: while a completion is held, the view is told a report is outstanding, so no queued
    /// prompt fires into it; the command's own optimistic completion keeps it, the user's stop
    /// clears it, and a closed pane drops every hold at once.
    #[test]
    fn a_held_completion_is_an_outstanding_report_until_the_user_stops_or_the_pane_closes() {
        App::test((), |mut app| async move {
            initialize_app_for_terminal_view(&mut app);
            // Stopping a conversation with a live exchange persists it, which reads this singleton.
            let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
            app.add_singleton_model(|_| {
                crate::GlobalResourceHandlesProvider::new(global_resource_handles)
            });
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
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
                let block_id = crate::terminal::model::block::BlockId::new();
                view.ai_controller().update(ctx, |controller, ctx| {
                    assert!(!controller.completion_report_outstanding(conversation_id));
                    controller.register_mock_stream_for_test(
                        stream_id,
                        conversation_id,
                        stream,
                        ctx,
                    );
                    assert!(!controller.follow_up_on_monitored_command_completed(
                        conversation_id,
                        &block_id,
                        ctx
                    ));
                    assert!(
                        controller.completion_report_outstanding(conversation_id),
                        "a held completion is an outstanding report"
                    );
                    controller.cancel_conversation_progress(
                        conversation_id,
                        CancellationReason::OptimisticCLISubagentCompletion,
                        ctx,
                    );
                    assert!(controller.completion_report_outstanding(conversation_id));
                    controller.cancel_conversation_progress(
                        conversation_id,
                        CancellationReason::ManuallyCancelled,
                        ctx,
                    );
                    assert!(!controller.completion_report_outstanding(conversation_id));
                });
            });
        });
    }

    #[test]
    fn closing_the_pane_drops_the_held_completion() {
        App::test((), |mut app| async move {
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
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
                let block_id = crate::terminal::model::block::BlockId::new();
                view.ai_controller().update(ctx, |controller, ctx| {
                    controller.register_mock_stream_for_test(
                        stream_id,
                        conversation_id,
                        stream,
                        ctx,
                    );
                    controller.follow_up_on_monitored_command_completed(
                        conversation_id,
                        &block_id,
                        ctx,
                    );
                    assert!(controller.has_deferred_monitored_completion(conversation_id));
                    controller.set_suspended_for_close(true);
                    assert!(!controller.has_deferred_monitored_completion(conversation_id));
                    assert!(!controller.completion_report_outstanding(conversation_id));
                });
            });
        });
    }
}

/// The end-of-step decision behind the `FinishedAction` subscriber: every finished batch ends
/// in a follow-up request or in a final status, and a stop nobody asked for is named.
mod finished_batch {
    use super::super::{FollowUpTrigger, plan_finished_batch};
    use super::*;
    use crate::ai::agent::{AIAgentActionResult, AIAgentActionResultType};
    use crate::ai::agent::{ReadFilesResult, RequestCommandOutputResult};
    use crate::terminal::model::block::BlockId;
    use std::sync::Arc;

    fn result_of(result: AIAgentActionResultType) -> Arc<AIAgentActionResult> {
        Arc::new(AIAgentActionResult {
            id: AIAgentActionId::from("call".to_owned()),
            task_id: TaskId::new("task".to_owned()),
            result,
        })
    }

    fn completed(exit_code: i32, interrupted: bool) -> Arc<AIAgentActionResult> {
        result_of(AIAgentActionResultType::RequestCommandOutput(
            RequestCommandOutputResult::Completed {
                block_id: BlockId::new(),
                command: "make test".to_owned(),
                output: String::new(),
                exit_code: exit_code.into(),
                start_ts: None,
                completed_ts: None,
                interrupted,
            },
        ))
    }

    fn rejected() -> Arc<AIAgentActionResult> {
        result_of(AIAgentActionResultType::RequestCommandOutput(
            RequestCommandOutputResult::CancelledBeforeExecution,
        ))
    }

    fn cancelled_read() -> Arc<AIAgentActionResult> {
        result_of(AIAgentActionResultType::ReadFiles(
            ReadFilesResult::Cancelled,
        ))
    }

    /// A finished command -- succeeded, failed, or exited 130 on its own -- goes back to the
    /// model; the turn is not over.
    #[test]
    fn a_finished_result_sends_an_automatic_follow_up() {
        for result in [
            completed(0, false),
            completed(2, false),
            completed(130, false),
        ] {
            let plan = plan_finished_batch(None, &[result], false, false);
            assert_eq!(plan.follow_up, Some(FollowUpTrigger::Auto));
        }
        // One finished result among cancelled ones is enough.
        let plan = plan_finished_batch(None, &[rejected(), completed(0, false)], false, false);
        assert_eq!(plan.follow_up, Some(FollowUpTrigger::Auto));
    }

    /// A user Reject or Stop ends the turn quietly: the user chose it.
    #[test]
    fn a_user_stop_ends_the_turn_without_a_notice() {
        let plan = plan_finished_batch(
            Some(CancellationReason::ManuallyCancelled),
            &[rejected()],
            false,
            false,
        );
        assert_eq!(plan.follow_up, None);
        assert_eq!(plan.final_status, ConversationStatus::Cancelled);
        assert_eq!(plan.silent_stop, None);
    }

    /// A cancellation nobody asked for (a ^C into the command, a cancelled tool) ends the turn
    /// with a visible reason (#1178): never a finished-looking turn with no message.
    #[test]
    fn an_unrequested_cancellation_ends_the_turn_with_a_reason() {
        let plan = plan_finished_batch(None, &[completed(130, true)], false, false);
        assert_eq!(plan.follow_up, None);
        assert_eq!(plan.final_status, ConversationStatus::Cancelled);
        assert!(
            plan.silent_stop
                .as_deref()
                .is_some_and(|reason| reason.contains("make test")),
            "{:?}",
            plan.silent_stop
        );
        let plan = plan_finished_batch(None, &[cancelled_read()], false, false);
        assert_eq!(plan.follow_up, None);
        assert!(plan.silent_stop.is_some());
    }

    /// An optimistic long-running-command completion or a revert is the terminal result:
    /// `Success`, no follow-up, no notice -- whatever the results say.
    #[test]
    fn a_succeeded_cancellation_ends_the_turn_successfully() {
        for reason in [
            CancellationReason::OptimisticCLISubagentCompletion,
            CancellationReason::Reverted,
        ] {
            let plan = plan_finished_batch(Some(reason), &[rejected()], false, false);
            assert_eq!(plan.follow_up, None);
            assert_eq!(plan.final_status, ConversationStatus::Success);
            assert_eq!(plan.silent_stop, None);
        }
    }

    /// A requested follow-up goes out even when nothing else would send one; a passive code
    /// diff never autosends.
    #[test]
    fn manual_follow_up_and_passive_code_diff_override_the_results() {
        let plan = plan_finished_batch(
            Some(CancellationReason::ManuallyCancelled),
            &[rejected()],
            false,
            true,
        );
        assert_eq!(plan.follow_up, Some(FollowUpTrigger::UserRequested));
        let plan = plan_finished_batch(None, &[completed(0, false)], true, false);
        assert_eq!(plan.follow_up, None);
        assert_eq!(plan.final_status, ConversationStatus::Success);
        assert_eq!(plan.silent_stop, None);
    }
}

/// The automatic resumes a turn gets for synthetic local tool results (bad arguments, unknown
/// tool, intercepted tool): each is a model request on the user's provider, so they are bounded.
mod synthetic_result_resumes {
    use super::super::{
        MAX_SYNTHETIC_RESULT_RESUMES_PER_TURN, byop_synthetic_result_needs_resume,
        charge_synthetic_result_resume,
    };

    /// An unknown tool name is answered with the available names and a request to re-issue
    /// the call; that answer only helps if the turn resumes. Fails on the old code, which
    /// resumed for bad arguments and intercepted tools only.
    #[test]
    fn an_unknown_tool_result_resumes_like_bad_arguments() {
        assert!(byop_synthetic_result_needs_resume(
            r#"{"error":"unknown_tool","available_tools":["read_files"],"executed":false}"#
        ));
        assert!(byop_synthetic_result_needs_resume(
            r#"{"error":"invalid_arguments","executed":false}"#
        ));
        assert!(byop_synthetic_result_needs_resume(
            r#"{"_byop_intercepted":true,"status":"ok"}"#
        ));
        // The dispatch gate's refusals stay non-resuming (#1067).
        assert!(!byop_synthetic_result_needs_resume(
            r#"{"error":"tool_not_offered","executed":false}"#
        ));
    }

    /// The allowance is spent one resume at a time and then refuses; a model looping on the
    /// same bad call is stopped after a bounded number of requests. Fails on the old code,
    /// which had no bound for anything but skill names.
    #[test]
    fn the_per_turn_allowance_is_bounded() {
        let mut spent = 0u8;
        for _ in 0..MAX_SYNTHETIC_RESULT_RESUMES_PER_TURN {
            assert!(charge_synthetic_result_resume(&mut spent));
        }
        assert!(!charge_synthetic_result_resume(&mut spent));
        assert!(!charge_synthetic_result_resume(&mut spent));
        assert_eq!(spent, MAX_SYNTHETIC_RESULT_RESUMES_PER_TURN);
    }
}

#[cfg(unix)]
mod remote_home_1288 {
    use crate::ai::blocklist::SessionContext;

    #[test]
    fn remote_session_path_rules_follow_the_remote_os() {
        use crate::ai::paths::PathHost;
        let mut session = SessionContext::new_remote_with_home_for_test(None, "/home/u");
        // OS not reported yet: previous (local) behaviour.
        assert_eq!(session.path_host(), PathHost::Local);
        session.remote_os_category = Some("Windows".to_owned());
        assert_eq!(session.path_host(), PathHost::RemoteWindows);
        session.remote_os_category = Some("Linux".to_owned());
        assert_eq!(session.path_host(), PathHost::RemoteUnix);
        // A local session never takes remote rules, even with a stale category.
        let mut local = SessionContext::new_for_test();
        local.remote_os_category = Some("Windows".to_owned());
        assert_eq!(local.path_host(), PathHost::Local);
    }

    #[test]
    fn resolve_path_uses_the_remote_home_not_the_local_one() {
        let session = SessionContext::new_remote_with_home_for_test(
            Some("/home/remoteuser"),
            "/home/remoteuser/proj",
        );
        assert_eq!(session.resolve_path("~/x"), "/home/remoteuser/x");
        assert_eq!(session.resolve_path("~"), "/home/remoteuser");
        assert_eq!(session.resolve_path("y"), "/home/remoteuser/proj/y");
        assert_eq!(session.resolve_path("/etc/z"), "/etc/z");
        assert!(!session.tilde_unresolvable("~/x"));
    }

    #[test]
    fn unknown_remote_home_is_flagged_for_tilde_paths_only() {
        let session = SessionContext::new_remote_with_home_for_test(None, "/srv");
        assert!(session.tilde_unresolvable("~/x"));
        assert!(session.tilde_unresolvable("~"));
        assert!(!session.tilde_unresolvable("/srv/x"));
        assert!(!session.tilde_unresolvable("rel/x"));
    }

    #[test]
    fn a_local_session_expands_with_the_local_home() {
        let session = SessionContext::new_local_with_cwd_for_test("/srv");
        assert!(!session.tilde_unresolvable("~/x"));
        if let Some(home) = dirs::home_dir() {
            assert_eq!(
                session.resolve_path("~/x"),
                home.join("x").to_string_lossy()
            );
        }
    }
}

/// #1362: a submit parked on its attachment read is discarded by Ctrl-C / stop and by pane
/// close; the read landing afterwards (id-guarded by `seq`) sends nothing. Without the discard
/// the landing read starts a request.
mod parked_query_sends {
    use super::*;
    use crate::ai::agent::conversation::AIConversationId;

    fn setup(
        app: &mut App,
    ) -> (
        warpui::ViewHandle<crate::terminal::TerminalView>,
        AIConversationId,
    ) {
        initialize_app_for_terminal_view(app);
        let global_resource_handles = crate::GlobalResourceHandles::mock(app);
        app.add_singleton_model(|_| {
            crate::GlobalResourceHandlesProvider::new(global_resource_handles)
        });
        // A give-up toasts the user (`notify_attachment_read_failures`); production registers this.
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(app, None);
        let terminal_view_id = terminal.read(app, |view, _| view.id());
        let conversation_id = BlocklistAIHistoryModel::handle(app).update(app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        (terminal, conversation_id)
    }

    #[test]
    fn ctrl_c_then_read_lands_sends_nothing() {
        App::test((), |mut app| async move {
            let (terminal, conversation_id) = setup(&mut app);
            let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
            controller.update(&mut app, |controller, ctx| {
                let seq = controller.park_query_send_for_test(
                    conversation_id,
                    TaskId::new("t".to_owned()),
                    "hi",
                );
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
                assert_eq!(controller.pending_query_send_count_for_test(), 0);
                controller.land_attachment_read_for_test(seq, ctx);
                assert!(!controller.has_active_stream_for_conversation(conversation_id, ctx));
            });
        });
    }

    #[test]
    fn follow_up_supersede_keeps_earlier_parked_send() {
        App::test((), |mut app| async move {
            let (terminal, conversation_id) = setup(&mut app);
            let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
            controller.update(&mut app, |controller, ctx| {
                controller.park_query_send_for_test(
                    conversation_id,
                    TaskId::new("t".to_owned()),
                    "first",
                );
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::FollowUpSubmitted {
                        is_for_same_conversation: true,
                    },
                    ctx,
                );
                assert_eq!(controller.pending_query_send_count_for_test(), 1);
            });
        });
    }

    #[test]
    fn pane_close_then_read_lands_sends_nothing() {
        App::test((), |mut app| async move {
            let (terminal, conversation_id) = setup(&mut app);
            let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
            controller.update(&mut app, |controller, ctx| {
                let seq = controller.park_query_send_for_test(
                    conversation_id,
                    TaskId::new("t".to_owned()),
                    "hi",
                );
                controller.set_suspended_for_close(true);
                assert_eq!(controller.pending_query_send_count_for_test(), 0);
                controller.land_attachment_read_for_test(seq, ctx);
                assert!(!controller.has_active_stream_for_conversation(conversation_id, ctx));
            });
        });
    }

    #[test]
    fn hung_read_does_not_block_a_later_submit() {
        App::test((), |mut app| async move {
            let (terminal, conversation_id) = setup(&mut app);
            let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
            controller.update(&mut app, |controller, ctx| {
                let hung = controller.park_query_send_for_test(
                    conversation_id,
                    TaskId::new("t".to_owned()),
                    "hung",
                );
                let later = controller.park_query_send_for_test(
                    conversation_id,
                    TaskId::new("t".to_owned()),
                    "later",
                );
                // The later submit's own read is done, but it waits behind the hung one.
                controller.land_attachment_read_for_test(later, ctx);
                assert_eq!(controller.pending_query_send_count_for_test(), 2);
                // The give-up fails only the hung submit and releases the later one.
                controller.give_up_attachment_read_for_test(hung, ctx);
                assert_eq!(controller.pending_query_send_count_for_test(), 0);
            });
        });
    }

    /// A submit deferred on its read takes its attachments out of live staging, so a second
    /// submit during the read cannot copy them again (#1370 F4).
    #[test]
    fn deferred_submit_takes_its_attachments_out_of_staging() {
        use crate::ai::blocklist::PendingAttachment;
        App::test((), |mut app| async move {
            let (terminal, _conversation_id) = setup(&mut app);
            let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
            controller.update(&mut app, |controller, ctx| {
                let file =
                    PendingAttachment::File(crate::ai::blocklist::context_model::PendingFile {
                        file_name: "a.txt".to_owned(),
                        file_path: std::path::PathBuf::from("/nonexistent/a.txt"),
                        mime_type: "text/plain".to_owned(),
                    });
                controller.stage_attachments_for_test(vec![file], ctx);
                controller.send_user_query_for_test("first", ctx);
                assert_eq!(controller.pending_query_send_count_for_test(), 1);
                assert!(
                    controller.staged_attachment_count_for_test(ctx) == 0,
                    "the deferred submit owns its attachments; a second submit must not copy them"
                );
            });
        });
    }
}
