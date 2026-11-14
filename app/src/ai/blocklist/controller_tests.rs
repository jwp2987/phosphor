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
                CancellationReason::Rejected,
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

/// #2029: whether a reply will be followed up by the app itself is read from the messages that
/// reply added, when its stream finishes, so the turn's `Success` can be held until the
/// follow-up is decided. Only a locally written result (no structured result, a resume marker
/// in its text) that THIS reply added counts: an old one in the history must not hold every
/// later turn, and an ordinary tool result is answered by the action queue.
#[test]
fn a_reply_that_left_a_local_error_result_expects_a_follow_up() {
    use super::{is_local_result_needing_resume, reply_needs_local_follow_up};
    use agent_proto::message;

    let result_message = |id: &str, text: String| agent_proto::Message {
        id: id.to_owned(),
        task_id: "task".to_owned(),
        server_message_data: text,
        citations: vec![],
        fetched_memories: vec![],
        message: Some(message::Message::ToolCallResult(message::ToolCallResult {
            tool_call_id: "call_1".to_owned(),
            context: None,
            result: None,
        })),
        request_id: "request".to_owned(),
        timestamp: None,
    };
    let invalid = result_message(
        "m-invalid",
        serde_json::json!({ "error": "invalid_arguments", "tool": "ask_user_question" })
            .to_string(),
    );
    let refused = result_message(
        "m-refused",
        serde_json::json!({ "status": "error", "error": "tool_not_offered" }).to_string(),
    );
    assert!(is_local_result_needing_resume(&invalid));
    assert!(
        !is_local_result_needing_resume(&refused),
        "a dispatch refusal does not resume (#1067), so nothing is waited for"
    );

    let history = [refused.clone(), invalid.clone()];
    assert!(reply_needs_local_follow_up(&history, |id| id == "m-invalid"));
    assert!(
        !reply_needs_local_follow_up(&history, |id| id == "m-refused"),
        "the reply added only the refusal"
    );
    assert!(
        !reply_needs_local_follow_up(&history, |_| false),
        "an error result of an earlier reply does not hold this turn"
    );
    assert!(!reply_needs_local_follow_up(
        std::iter::empty::<&agent_proto::Message>(),
        |_| true
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
                CancellationReason::Rejected,
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

    /// #1896: the cancelled results `cancel_discarded_actions` records for actions that were never
    /// queued emit `FinishedAction { FollowUpSubmitted { same } }`; a batch made only of cancelled
    /// results sends NO request of its own (the user's follow-up is the one request that carries
    /// them), so a steer sends exactly one request.
    #[test]
    fn discarded_actions_of_a_follow_up_send_no_request_of_their_own() {
        let plan = plan_finished_batch(
            Some(CancellationReason::FollowUpSubmitted {
                is_for_same_conversation: true,
            }),
            &[cancelled_read(), rejected()],
            false,
            false,
        );
        assert_eq!(plan.follow_up, None);
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
        // The card's own Reject (its own reason since the 2026-11-09 decision) with nothing
        // else finished: nothing to report, same quiet end.
        let plan = plan_finished_batch(
            Some(CancellationReason::Rejected),
            &[rejected(), cancelled_read()],
            false,
            false,
        );
        assert_eq!(plan.follow_up, None);
        assert_eq!(plan.final_status, ConversationStatus::Cancelled);
        assert_eq!(plan.silent_stop, None);
    }

    /// #1942, second part of the owner decision (2026-11-09): a Reject is not a stop. Rejecting a
    /// card after an earlier call of the same step finished sends the step's results back to the
    /// model at once (an automatic follow-up) and the agent continues; the later calls the reject
    /// cancelled with it (#1799) go in the same batch. Fails on the code before this change,
    /// where a Reject arrived as `ManuallyCancelled` and planned no follow-up.
    #[test]
    fn a_reject_after_a_finished_result_tells_the_model_at_once() {
        for results in [
            vec![completed(0, false), rejected()],
            vec![completed(2, false), rejected(), cancelled_read()],
        ] {
            let plan =
                plan_finished_batch(Some(CancellationReason::Rejected), &results, false, false);
            assert_eq!(plan.follow_up, Some(FollowUpTrigger::Auto), "{results:?}");
            // The user chose it: no "Agent stopped" notice either way.
            assert_eq!(plan.silent_stop, None, "{results:?}");
        }
        // The reasons a reject must not be confused with.
        assert!(!CancellationReason::Rejected.is_user_stop());
        assert!(CancellationReason::ManuallyCancelled.is_user_stop());
        assert!(CancellationReason::Deleted.is_user_stop());
        assert!(
            !CancellationReason::FollowUpSubmitted {
                is_for_same_conversation: true
            }
            .is_user_stop()
        );
    }

    /// #1942: Stop means stop. A batch stopped by the user after one of its results had already
    /// finished (`read_files` done, the command card then stopped) sends NO follow-up request and
    /// ends `Cancelled`, quietly: the finished result rides on the user's next request instead.
    /// Fails on the old code, which planned `Auto` off the one non-cancelled result and opened a
    /// request after the user had stopped the agent.
    #[test]
    fn a_user_stop_after_a_finished_result_sends_no_request() {
        for reason in [
            CancellationReason::ManuallyCancelled,
            CancellationReason::Deleted,
            CancellationReason::UserCommandExecuted,
            CancellationReason::FollowUpSubmitted {
                is_for_same_conversation: false,
            },
        ] {
            let plan = plan_finished_batch(
                Some(reason),
                &[completed(0, false), rejected()],
                false,
                false,
            );
            assert_eq!(plan.follow_up, None, "{reason:?}");
            assert_eq!(
                plan.final_status,
                ConversationStatus::Cancelled,
                "{reason:?}"
            );
            assert_eq!(plan.silent_stop, None, "{reason:?}");
        }
        // Even a batch whose every result finished ends Cancelled when the user stopped it: the
        // stop came after the last result and before its follow-up went out.
        let plan = plan_finished_batch(
            Some(CancellationReason::ManuallyCancelled),
            &[completed(0, false)],
            false,
            false,
        );
        assert_eq!(plan.follow_up, None);
        assert_eq!(plan.final_status, ConversationStatus::Cancelled);
        // The same mix with nobody stopping still follows up (the #1178 `None`-reason case).
        let plan = plan_finished_batch(None, &[completed(0, false), rejected()], false, false);
        assert_eq!(plan.follow_up, Some(FollowUpTrigger::Auto));
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

    /// #1951 (owner decision 2026-11-09): which results of a step are written into the
    /// conversation when the step ends with no follow-up. Only an answered file edit of a
    /// passive code diff: accepted (applied) or accepted and failed to apply. A cancelled or
    /// rejected edit is not answered, and no other step records anything here (its follow-up
    /// request does).
    #[test]
    fn only_an_answered_passive_code_diff_is_recorded_without_a_follow_up() {
        use super::super::answered_passive_diff_results;
        use crate::ai::agent::RequestFileEditsResult;

        let accepted = result_of(AIAgentActionResultType::RequestFileEdits(
            RequestFileEditsResult::Success {
                diff: String::new(),
                updated_files: Vec::new(),
                deleted_files: Vec::new(),
                lines_added: 3,
                lines_removed: 1,
            },
        ));
        let failed = result_of(AIAgentActionResultType::RequestFileEdits(
            RequestFileEditsResult::DiffApplicationFailed {
                error: "the file changed on disk".to_owned(),
            },
        ));
        let rejected_edit = result_of(AIAgentActionResultType::RequestFileEdits(
            RequestFileEditsResult::Cancelled,
        ));

        let recorded = answered_passive_diff_results(true, &[accepted.clone()]);
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].id, accepted.id);
        assert!(matches!(
            recorded[0].result,
            AIAgentActionResultType::RequestFileEdits(RequestFileEditsResult::Success { .. })
        ));
        assert_eq!(answered_passive_diff_results(true, &[failed]).len(), 1);
        // Not answered: the call stays open on disk, and a relaunch shows it Cancelled.
        assert!(answered_passive_diff_results(true, &[rejected_edit]).is_empty());
        // Not a passive code diff: the step's own follow-up carries its results.
        assert!(answered_passive_diff_results(false, &[accepted]).is_empty());
        // Only the edit itself, never another tool's result.
        assert!(answered_passive_diff_results(true, &[completed(0, false)]).is_empty());
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
        // OS not reported yet: the rules follow the shape of the remote's paths (#1647), so a
        // `/home/u` home is unix; before #1647 this fell back to the local OS's rules, which on a
        // Windows client built `\home\u\...` paths for a Linux remote.
        assert_eq!(session.path_host(), PathHost::RemoteUnix);
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

/// #1799: the persisted BYOP tool-result message for a cancelled hand-over carries the reason.
/// A cancelled transfer has no API form, so the message has no structured result and its
/// `server_message_data` is the `Display` fallback, which names the reject that cancelled it.
#[test]
fn byop_message_for_a_cancelled_transfer_says_why() {
    use crate::ai::agent::{
        AIAgentActionResult, AIAgentActionResultType, TransferShellCommandControlToUserResult,
    };
    use agent_proto::message;

    let result = AIAgentActionResult {
        id: AIAgentActionId::from("hand-over".to_owned()),
        task_id: TaskId::new("task".to_owned()),
        result: AIAgentActionResultType::TransferShellCommandControlToUser(
            TransferShellCommandControlToUserResult::Cancelled,
        ),
    };
    let message = super::BlocklistAIController::byop_action_result_message("request", &result);

    assert!(
        matches!(
            &message.message,
            Some(message::Message::ToolCallResult(tool_result)) if tool_result.result.is_none()
        ),
        "a cancelled transfer has no structured result"
    );
    assert!(
        message
            .server_message_data
            .contains("rejected an earlier tool call"),
        "{}",
        message.server_message_data
    );
}

/// #1942 (owner decision 2026-11-09): the tool-result message for the call the user rejected on
/// its card says it was rejected, not just "cancelled". Like the synthetic cancellation message,
/// it has no structured result and its text travels in `server_message_data`, which is what the
/// request builder sends for such a message.
#[test]
fn byop_message_for_a_rejected_call_says_the_user_rejected_it() {
    use crate::ai::agent::{
        AIAgentActionResult, AIAgentActionResultType, RequestCommandOutputResult,
    };
    use agent_proto::message;

    let result = AIAgentActionResult {
        id: AIAgentActionId::from("call_2".to_owned()),
        task_id: TaskId::new("task".to_owned()),
        result: AIAgentActionResultType::RequestCommandOutput(
            RequestCommandOutputResult::CancelledBeforeExecution,
        ),
    };
    let message = super::BlocklistAIController::byop_rejected_by_user_message("request", &result);

    assert_eq!(message.task_id, "task");
    assert!(
        matches!(
            &message.message,
            Some(message::Message::ToolCallResult(tool_result))
                if tool_result.tool_call_id == "call_2" && tool_result.result.is_none()
        ),
        "the rejected call is answered, with no structured result: {:?}",
        message.message
    );
    let text: serde_json::Value =
        serde_json::from_str(&message.server_message_data).expect("the text is JSON");
    assert_eq!(text["status"], "rejected");
    assert_eq!(text["reason"], "rejected_by_user");
    assert_eq!(text["detail"], super::REJECTED_BY_USER_DETAIL);
    // The ordinary form of the same result, for contrast: a bare cancel that never names the
    // user's decision.
    let ordinary = super::BlocklistAIController::byop_action_result_message("request", &result);
    assert!(
        !ordinary.server_message_data.contains("rejected"),
        "{}",
        ordinary.server_message_data
    );
}

/// #2036: a tool call that a broken reply stream left behind (the exchange that added it ended
/// in an error) is answered as "connection lost, did not run", never as the user's interrupt.
/// The automatic resume after the error is the request that writes this placeholder, and no
/// user did anything there. A call the user's own action left behind keeps its text.
///
/// Red on the old code: there was one placeholder, `interrupted_by_user`, whoever left the call.
#[test]
fn a_call_left_by_a_broken_reply_is_answered_as_connection_lost_not_as_a_user_interrupt() {
    use super::{CONNECTION_LOST_DETAIL, MissingResultCause, missing_result_cause_for};
    use crate::ai::agent::{
        AIAgentOutputStatus, CancellationReason, FinishedAIAgentOutput, RenderableAIError,
    };
    use agent_proto::message;

    let broken = AIAgentOutputStatus::Finished {
        finished_output: FinishedAIAgentOutput::Error {
            output: None,
            error: RenderableAIError::Other {
                error_message: "connection reset".to_owned(),
                will_attempt_resume: true,
                waiting_for_network: false,
            },
        },
    };
    let stopped = AIAgentOutputStatus::Finished {
        finished_output: FinishedAIAgentOutput::Cancelled {
            output: None,
            reason: CancellationReason::ManuallyCancelled,
        },
    };
    let streaming = AIAgentOutputStatus::Streaming { output: None };
    assert_eq!(
        missing_result_cause_for(Some(&broken)),
        MissingResultCause::ReplyCutOff
    );
    // The user stopped the reply, a reply still streaming, an exchange nobody knows: the
    // placeholder that was always written.
    assert_eq!(
        missing_result_cause_for(Some(&stopped)),
        MissingResultCause::UserInterrupt
    );
    assert_eq!(
        missing_result_cause_for(Some(&streaming)),
        MissingResultCause::UserInterrupt
    );
    assert_eq!(
        missing_result_cause_for(None),
        MissingResultCause::UserInterrupt
    );

    let text = |cause| -> serde_json::Value {
        let message = super::BlocklistAIController::byop_synthetic_cancellation_message(
            "request", "task", "call_1", cause,
        );
        assert_eq!(message.task_id, "task");
        assert!(
            matches!(
                &message.message,
                Some(message::Message::ToolCallResult(tool_result))
                    if tool_result.tool_call_id == "call_1" && tool_result.result.is_none()
            ),
            "the call is answered, with no structured result: {:?}",
            message.message
        );
        serde_json::from_str(&message.server_message_data).expect("the text is JSON")
    };
    let lost = text(MissingResultCause::ReplyCutOff);
    assert_eq!(lost["status"], "cancelled");
    assert_eq!(lost["reason"], "connection_lost");
    assert_eq!(lost["detail"], CONNECTION_LOST_DETAIL);
    assert!(
        !lost.to_string().contains("interrupted_by_user"),
        "nobody interrupted this call: {lost}"
    );
    let by_user = text(MissingResultCause::UserInterrupt);
    assert_eq!(by_user["status"], "cancelled");
    assert_eq!(by_user["reason"], "interrupted_by_user");
    assert!(by_user.get("detail").is_none(), "unchanged: {by_user}");
}

/// The kill switches of this file read `0` as off and everything else as on.
#[test]
fn a_kill_switch_is_on_unless_it_says_zero() {
    use super::kill_switch_on;
    assert!(kill_switch_on(None));
    assert!(kill_switch_on(Some("1")));
    assert!(kill_switch_on(Some("")));
    assert!(!kill_switch_on(Some("0")));
    assert!(!kill_switch_on(Some(" 0 ")));
}

/// The agent file tools follow the connected server's capabilities (#1804): a limited server
/// without `FILE_CONTEXT` (the bundled helper) is the same as no server, so `read_files`,
/// `apply_file_diffs` and `read_skill` are withdrawn exactly as in the no-helper case; a legacy
/// full daemon (no capability list) and a server that lists the capability keep them.
#[test]
fn file_tools_are_withdrawn_for_a_server_without_file_context_and_offered_for_a_legacy_daemon() {
    use remote_server::capability::{
        FILE_CONTEXT, FS_BASIC, HELPER_CAPABILITIES, RUN_COMMAND, ServerCapabilities,
    };
    use remote_server::proto::InitializeResponse;

    let caps = |names: &[&str]| {
        ServerCapabilities::from_initialize_response(&InitializeResponse {
            capabilities: names.iter().map(|n| n.to_string()).collect(),
            ..Default::default()
        })
    };

    // No server at all: withdrawn.
    assert!(!super::file_tools_available(None));
    // Legacy full daemon: an empty list means everything.
    assert!(super::file_tools_available(Some(&caps(&[]))));
    // The helper as it ships now serves `file_context` (remote parity P1), so it offers the
    // tools; the withdrawn case is a limited server that does not list the capability.
    assert!(super::file_tools_available(Some(&caps(
        HELPER_CAPABILITIES
    ))));
    assert!(!super::file_tools_available(Some(&caps(&[
        FS_BASIC,
        RUN_COMMAND
    ]))));
    // A server that does list it.
    assert!(super::file_tools_available(Some(&caps(&[FILE_CONTEXT]))));

    // The end of the chain: a remote session whose server cannot read files has no file tools,
    // one whose server can has them.
    assert!(
        super::SessionContext::new_warpified_remote_for_test(false).is_remote_without_file_tools()
    );
    assert!(
        !super::SessionContext::new_warpified_remote_for_test(true).is_remote_without_file_tools()
    );
}

/// #1948, owner decision 2026-11-09: a request stashed behind a live tool call that carries the
/// user's own typed prompt is never dropped on Stop. It is not sent (Stop means stop), and its
/// text goes back into the input with a notice. An automatic stashed request is still dropped.
mod stashed_prompt {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use super::super::{PendingByopRequest, RequestInput, stashed_user_prompt};
    use super::*;
    use crate::ai::agent::conversation::AIConversationId;

    fn user_query(query: &str, user_query_mode: UserQueryMode) -> AIAgentInput {
        AIAgentInput::UserQuery {
            query: query.to_owned(),
            context: Arc::from(Vec::<AIAgentContext>::new()),
            static_query_type: None,
            referenced_attachments: HashMap::new(),
            user_query_mode,
            running_command: None,
            intended_agent: None,
        }
    }

    fn resume() -> AIAgentInput {
        AIAgentInput::ResumeConversation {
            context: Arc::from(Vec::<AIAgentContext>::new()),
        }
    }

    fn command_result() -> AIAgentInput {
        AIAgentInput::ActionResult {
            result: crate::ai::agent::AIAgentActionResult {
                id: AIAgentActionId::from("call_1".to_owned()),
                task_id: TaskId::new("root-task".to_owned()),
                result: crate::ai::agent::AIAgentActionResultType::RequestCommandOutput(
                    crate::ai::agent::RequestCommandOutputResult::CancelledBeforeExecution,
                ),
            },
            context: Arc::from(Vec::<AIAgentContext>::new()),
        }
    }

    fn request_with(conversation_id: AIConversationId, inputs: Vec<AIAgentInput>) -> RequestInput {
        RequestInput {
            conversation_id,
            input_messages: HashMap::from([(TaskId::new("root-task".to_owned()), inputs)]),
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

    fn stash(request_input: RequestInput) -> PendingByopRequest {
        PendingByopRequest {
            request_input,
            query_metadata: None,
            default_to_follow_up_on_success: false,
            recovery: RecoveryBudget::fresh(),
            is_queued_prompt: false,
        }
    }

    /// Which stashed requests carry a prompt the user typed, and the text that comes back.
    #[test]
    fn only_a_typed_prompt_counts_as_the_users_own() {
        let conversation_id = AIConversationId::new();
        let typed = request_with(
            conversation_id,
            vec![
                command_result(),
                user_query("fix the failing test", UserQueryMode::Normal),
            ],
        );
        assert_eq!(
            stashed_user_prompt(&typed).as_deref(),
            Some("fix the failing test")
        );
        // A plan-mode prompt comes back the way the user would type it again.
        let planned = request_with(
            conversation_id,
            vec![user_query("refactor the parser", UserQueryMode::Plan)],
        );
        let restored = stashed_user_prompt(&planned).expect("a typed prompt");
        assert!(restored.ends_with("refactor the parser"), "{restored:?}");
        assert_ne!(restored, "refactor the parser", "the plan prefix is kept");
        // Automatic requests: tool results alone, a resume, nothing at all.
        for automatic in [vec![command_result()], vec![resume()], vec![]] {
            assert_eq!(
                stashed_user_prompt(&request_with(conversation_id, automatic)),
                None
            );
        }
        // A blank prompt is nothing to put back.
        let blank = request_with(
            conversation_id,
            vec![user_query("  ", UserQueryMode::Normal)],
        );
        assert_eq!(stashed_user_prompt(&blank), None);
    }

    type Restored = Rc<RefCell<Vec<(AIConversationId, String)>>>;
    type Terminal = warpui::ViewHandle<crate::terminal::TerminalView>;

    fn setup(app: &mut App) -> (Terminal, AIConversationId, Restored) {
        initialize_app_for_terminal_view(app);
        let global_resource_handles = crate::GlobalResourceHandles::mock(app);
        app.add_singleton_model(|_| {
            crate::GlobalResourceHandlesProvider::new(global_resource_handles)
        });
        // The restore shows a notice; production registers the toast stack.
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(app, None);
        let terminal_view_id = terminal.read(app, |view, _| view.id());
        let conversation_id = BlocklistAIHistoryModel::handle(app).update(app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let controller = terminal.read(app, |view, _| view.ai_controller().clone());
        let restored: Restored = Rc::new(RefCell::new(Vec::new()));
        let restored_sub = restored.clone();
        app.update(|ctx| {
            ctx.subscribe_to_model(&controller, move |_, event, _| {
                if let BlocklistAIControllerEvent::StashedPromptRestored {
                    conversation_id,
                    text,
                } = event
                {
                    restored_sub
                        .borrow_mut()
                        .push((*conversation_id, text.clone()));
                }
            });
        });
        (terminal, conversation_id, restored)
    }

    fn input_text(app: &App, terminal: &Terminal) -> String {
        terminal.read(app, |view, ctx| view.input().as_ref(ctx).buffer_text(ctx))
    }

    /// Stashes `inputs` as the conversation's waiting request, types `draft` into the input,
    /// cancels with `reason`, and returns (stash still held, restore events, input text, a
    /// stream was opened).
    fn cancel_with_stash(
        inputs: Vec<AIAgentInput>,
        draft: &'static str,
        reason: CancellationReason,
    ) -> (bool, Vec<String>, String, bool) {
        App::test((), |mut app| async move {
            let (terminal, conversation_id, restored) = setup(&mut app);
            let controller = terminal.read(&app, |view, _| view.ai_controller().clone());
            if !draft.is_empty() {
                terminal.update(&mut app, |view, ctx| {
                    view.input().update(ctx, |input, ctx| {
                        input.replace_buffer_content(draft, ctx);
                    });
                });
            }
            let (still_stashed, stream_opened) = controller.update(&mut app, |controller, ctx| {
                controller.pending_byop_requests.insert(
                    conversation_id,
                    stash(request_with(conversation_id, inputs)),
                );
                controller.cancel_conversation_progress(conversation_id, reason, ctx);
                (
                    controller
                        .pending_byop_requests
                        .contains_key(&conversation_id),
                    controller.has_active_stream_for_conversation(conversation_id, ctx),
                )
            });
            let events: Vec<String> = restored
                .borrow()
                .iter()
                .map(|(restored_for, text)| {
                    assert_eq!(*restored_for, conversation_id);
                    text.clone()
                })
                .collect();
            let input = input_text(&app, &terminal);
            (still_stashed, events, input, stream_opened)
        })
    }

    /// Stop with the user's prompt stashed: nothing is sent, the stash is gone, and the prompt
    /// is back in the input. Fails on the code before this change, which dropped the stash and
    /// the prompt with it (the input stayed empty and nothing said where the text went).
    #[test]
    fn stop_puts_a_stashed_typed_prompt_back_into_the_input() {
        let (still_stashed, events, input, stream_opened) = cancel_with_stash(
            vec![user_query("fix the failing test", UserQueryMode::Normal)],
            "",
            CancellationReason::ManuallyCancelled,
        );
        assert!(!still_stashed, "the stopped conversation keeps no stash");
        assert!(!stream_opened, "Stop means stop: the request is not sent");
        assert_eq!(
            events,
            vec!["fix the failing test".to_owned()],
            "the typed prompt is handed back exactly once"
        );
        assert_eq!(input, "fix the failing test");
    }

    /// The restore never overwrites what the user is typing: the prompt goes on a line of its
    /// own after the draft.
    #[test]
    fn the_restored_prompt_is_appended_to_a_draft() {
        let (still_stashed, events, input, stream_opened) = cancel_with_stash(
            vec![
                command_result(),
                user_query("and run the linter", UserQueryMode::Normal),
            ],
            "a draft",
            CancellationReason::ManuallyCancelled,
        );
        assert!(!still_stashed);
        assert!(!stream_opened);
        assert_eq!(events, vec!["and run the linter".to_owned()]);
        assert_eq!(input, "a draft\nand run the linter");
    }

    /// Deleting the conversation is a stop too, and the typed prompt still comes back, as the
    /// queued prompts of a deleted conversation do (#1026).
    #[test]
    fn delete_puts_a_stashed_typed_prompt_back_into_the_input() {
        let (still_stashed, events, input, stream_opened) = cancel_with_stash(
            vec![user_query("fix the failing test", UserQueryMode::Normal)],
            "",
            CancellationReason::Deleted,
        );
        assert!(!still_stashed);
        assert!(!stream_opened);
        assert_eq!(events, vec!["fix the failing test".to_owned()]);
        assert_eq!(input, "fix the failing test");
    }

    /// An automatic stashed request (tool results, a resume) is still dropped on Stop, silently:
    /// nothing is sent, nothing is put into the input.
    #[test]
    fn stop_still_drops_an_automatic_stashed_request() {
        let (still_stashed, events, input, stream_opened) = cancel_with_stash(
            vec![command_result(), resume()],
            "",
            CancellationReason::ManuallyCancelled,
        );
        assert!(!still_stashed);
        assert!(!stream_opened);
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(input, "");
    }

    /// A same-conversation follow-up is not a stop: the stash is left for the finishing call to
    /// flush (#1948), neither dropped nor restored.
    #[test]
    fn a_follow_up_in_the_same_conversation_keeps_the_stash() {
        let (still_stashed, events, input, _) = cancel_with_stash(
            vec![user_query("keep going", UserQueryMode::Normal)],
            "",
            CancellationReason::FollowUpSubmitted {
                is_for_same_conversation: true,
            },
        );
        assert!(still_stashed);
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(input, "");
    }
}

/// #2100: the LRC diag line is an info line; the running command's text is in it only with the
/// text opt-in and never during a secret read.
#[test]
fn the_lrc_running_command_line_withholds_the_command() {
    let line = |shown| {
        super::lrc_running_command_filled_line(1, 2, true, 640, "mysql -phunter2pw", false, shown)
    };
    assert_eq!(
        line(warpui::input_order::TextShown::fixed(false, false)),
        "[byop-diag] LRC running_command filled: 1/2 UserQuery slot(s); should_spawn=true \
         grid_contents_len=640 command=<17 chars> is_alt_screen=false"
    );
    warpui::input_order::assert_line_withholds(line, &["hunter2pw", "mysql"]);
}

// -- #2103: a pane close keeps the status of the calls whose results it discards ------------
//
// Owner decision 2026-11-11, with its condition: the results a close discards (#848) are kept
// for the calls' STATUS only and are never sent to the model. No BYOP provider is configured
// here, so the preflight is skipped and a request is seen as the controller builds it, with
// its `ActionResult` inputs (the same footing as the #1059 tests above).

fn grep_result(
    id: &str,
    task_id: &TaskId,
    result: crate::ai::agent::GrepResult,
) -> crate::ai::agent::AIAgentActionResult {
    crate::ai::agent::AIAgentActionResult {
        id: AIAgentActionId::from(id.to_owned()),
        task_id: task_id.clone(),
        result: crate::ai::agent::AIAgentActionResultType::Grep(result),
    }
}

/// What `suspend_agent_work_for_close` and `resume_after_close_undone` do to the controller,
/// without the view: the pane is closed (its finished results are discarded) and restored.
fn close_and_restore_pane(
    controller: &mut super::BlocklistAIController,
    conversation_id: crate::ai::agent::conversation::AIConversationId,
    ctx: &mut warpui::ModelContext<super::BlocklistAIController>,
) {
    controller.set_suspended_for_close(true);
    controller.discard_pending_work_for_close(conversation_id, ctx);
    controller.set_suspended_for_close(false);
}

/// The id of every tool result that any request of the conversation carries or that its
/// history holds: the `ActionResult` inputs of every exchange and of every in-flight stream's
/// parameters, and every `ToolCallResult` message of every task.
fn result_ids_sent_or_recorded(
    controller: &super::BlocklistAIController,
    conversation_id: crate::ai::agent::conversation::AIConversationId,
    app: &warpui::AppContext,
) -> Vec<String> {
    let mut ids = Vec::new();
    for stream in controller.in_flight_response_streams.streams_for_test() {
        ids.extend(action_result_ids(
            &stream.as_ref(app).params_for_test().input,
        ));
    }
    let history = BlocklistAIHistoryModel::as_ref(app);
    let conversation = history
        .conversation(&conversation_id)
        .expect("conversation should exist");
    for exchange in conversation.all_exchanges() {
        ids.extend(action_result_ids(&exchange.input));
    }
    for task in conversation.all_tasks() {
        for message in task.messages() {
            if let Some(agent_proto::message::Message::ToolCallResult(result)) = &message.message {
                ids.push(result.tool_call_id.clone());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

/// The prompts the conversation's exchanges carry: proof that a request was built at all.
fn prompts_sent(
    conversation_id: crate::ai::agent::conversation::AIConversationId,
    app: &warpui::AppContext,
) -> Vec<String> {
    BlocklistAIHistoryModel::as_ref(app)
        .conversation(&conversation_id)
        .expect("conversation should exist")
        .all_exchanges()
        .into_iter()
        .flat_map(|exchange| exchange.input.iter().filter_map(AIAgentInput::user_query))
        .collect()
}

/// #2103: a pane close discards the finished, unsent results of the reply it stops, and each
/// of their calls keeps its status: the call that ran is still a success, the call that was
/// cancelled is still cancelled. Nothing is left to send, and the copy / export text (which can
/// be attached to a prompt) does not see the discarded results.
///
/// Red on the old code: the close erased the results and both calls had no status at all.
#[test]
fn a_pane_close_discards_finished_results_and_their_calls_keep_their_status() {
    use crate::ai::agent::GrepResult;

    App::test((), |mut app| async move {
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
        assert!(
            super::super::action_model::close_keeps_call_status_enabled(),
            "precondition: the kill switch is not set in the test environment"
        );
        let (ran, stopped) = (
            AIAgentActionId::from("ran-before-close".to_owned()),
            AIAgentActionId::from("stopped-before-close".to_owned()),
        );

        controller.update(&mut app, |controller, ctx| {
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    grep_result(
                        "ran-before-close",
                        &root_task_id,
                        GrepResult::Success {
                            matched_files: vec![],
                        },
                    ),
                );
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    grep_result("stopped-before-close", &root_task_id, GrepResult::Cancelled),
                );
            });
            close_and_restore_pane(controller, conversation_id, ctx);

            let action_model = controller.action_model.as_ref(ctx);
            assert!(
                action_model
                    .get_finished_action_results(conversation_id)
                    .is_none_or(|results| results.is_empty()),
                "#848: nothing of the stopped reply is left to ride on a later request"
            );
            assert!(
                action_model
                    .get_action_status(&ran)
                    .is_some_and(|status| status.is_success()),
                "the call that ran before the close still shows as run"
            );
            assert!(
                action_model
                    .get_action_status(&stopped)
                    .is_some_and(|status| status.is_cancelled()),
                "the cancelled call still shows as cancelled"
            );
            for id in [&ran, &stopped] {
                assert!(action_model.is_result_discarded_by_close(id));
                assert!(
                    action_model.get_action_result_for_export(id).is_none(),
                    "a discarded result is not in the copy / export text"
                );
            }
        });
    });
}

/// The kill switch's arm (`PHOSPHOR_CLOSE_KEEPS_CALL_STATUS=0`): the results are erased as
/// before #2103 and the call has no status. Driven with the flag passed in, so the test does
/// not depend on the process environment.
#[test]
fn with_the_kill_switch_off_a_pane_close_erases_the_results() {
    use crate::ai::agent::GrepResult;
    use crate::ai::blocklist::action_model::close_keeps_call_status_enabled_from as enabled;

    assert!(enabled(None));
    assert!(enabled(Some("1")));
    for off in ["0", " 0 ", "off", "FALSE", "no"] {
        assert!(!enabled(Some(off)), "{off:?} turns it off");
    }

    App::test((), |mut app| async move {
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
        let ran = AIAgentActionId::from("ran-before-close".to_owned());
        controller.update(&mut app, |controller, ctx| {
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    grep_result(
                        "ran-before-close",
                        &root_task_id,
                        GrepResult::Success {
                            matched_files: vec![],
                        },
                    ),
                );
                let discarded =
                    action_model.discard_finished_action_results_for_close(conversation_id, false);
                assert_eq!(discarded, 1);
                assert!(action_model.get_action_status(&ran).is_none());
                assert!(!action_model.is_result_discarded_by_close(&ran));
                assert!(
                    action_model
                        .get_finished_action_results(conversation_id)
                        .is_none_or(|results| results.is_empty())
                );
            });
        });
    });
}

/// #2103, the owner's condition: close a pane with a finished-but-unsent result, undo the
/// close, send a follow-up. The discarded result is in NO request and is not recorded in the
/// conversation: not in the automatic or the user-requested follow-up (there is none: nothing
/// is left to send), not in the user's next prompt, not after the prompt after that. Its call
/// has a status the whole time, which is the only thing the kept result is for.
#[test]
fn a_result_a_pane_close_discarded_is_in_no_later_request() {
    use crate::ai::agent::GrepResult;
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
        let discarded_id = AIAgentActionId::from("ran-before-close".to_owned());

        controller.update(&mut app, |controller, ctx| {
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    grep_result(
                        "ran-before-close",
                        &root_task_id,
                        GrepResult::Success {
                            matched_files: vec![],
                        },
                    ),
                );
            });
            close_and_restore_pane(controller, conversation_id, ctx);

            // The follow-ups an action or the user can trigger: nothing to send.
            for trigger in [
                super::FollowUpTrigger::Auto,
                super::FollowUpTrigger::UserRequested,
            ] {
                controller.send_follow_up_for_conversation(conversation_id, trigger, ctx);
                assert_eq!(
                    controller
                        .in_flight_response_streams
                        .streams_for_test()
                        .count(),
                    0,
                    "a follow-up after the undo has no results to send ({trigger:?})"
                );
            }

            // The user's next prompt.
            controller.send_user_query_in_conversation(
                "what next".to_owned(),
                conversation_id,
                None,
                ctx,
            );
        });
        // Wait by wall clock, not a yield count (#2219).
        crate::test_util::wait_for(|| {
            controller.read(&app, |controller, ctx| {
                prompts_sent(conversation_id, ctx) == vec!["what next".to_owned()]
            })
        })
        .await;
        controller.update(&mut app, |controller, ctx| {
            assert_eq!(
                prompts_sent(conversation_id, ctx),
                vec!["what next".to_owned()],
                "precondition: the prompt after the undo was built into a request"
            );
            assert_eq!(
                result_ids_sent_or_recorded(controller, conversation_id, ctx),
                Vec::<String>::new(),
                "the request after the undo carries no tool result: the discarded one is not sent"
            );
            assert!(
                controller
                    .action_model
                    .as_ref(ctx)
                    .get_action_status(&discarded_id)
                    .is_some_and(|status| status.is_success()),
                "the call keeps its status across the request"
            );

            // And the prompt after that (it supersedes the first: the same drain runs again).
            controller.send_user_query_in_conversation(
                "and then".to_owned(),
                conversation_id,
                None,
                ctx,
            );
        });
        // Wait by wall clock, not a yield count (#2219).
        crate::test_util::wait_for(|| {
            controller.read(&app, |controller, ctx| {
                prompts_sent(conversation_id, ctx).contains(&"and then".to_owned())
            })
        })
        .await;
        controller.update(&mut app, |controller, ctx| {
            assert!(
                prompts_sent(conversation_id, ctx).contains(&"and then".to_owned()),
                "precondition: the second prompt was built into a request"
            );
            assert_eq!(
                result_ids_sent_or_recorded(controller, conversation_id, ctx),
                Vec::<String>::new()
            );
        });
    });
}

/// Everything a request of the conversation is built from or leaves behind, and the text a copy
/// or an export of it gives, as one string: the parameters of every in-flight stream, the
/// inputs of every exchange, every message of every task, and `export_to_markdown` (the text
/// `terminal/input.rs` attaches to a prompt). The same places
/// [`result_ids_sent_or_recorded`] reads, as text.
fn everything_sent_recorded_or_exported(
    controller: &super::BlocklistAIController,
    conversation_id: crate::ai::agent::conversation::AIConversationId,
    app: &warpui::AppContext,
) -> String {
    let mut text = String::new();
    for stream in controller.in_flight_response_streams.streams_for_test() {
        text.push_str(&format!("{:?}\n", stream.as_ref(app).params_for_test()));
    }
    let history = BlocklistAIHistoryModel::as_ref(app);
    let conversation = history
        .conversation(&conversation_id)
        .expect("conversation should exist");
    for exchange in conversation.all_exchanges() {
        text.push_str(&format!("{:?}\n", exchange.input));
    }
    for task in conversation.all_tasks() {
        for message in task.messages() {
            text.push_str(&format!("{message:?}\n"));
        }
    }
    text.push_str(&conversation.export_to_markdown(Some(controller.action_model.as_ref(app))));
    text
}

/// #2103, the note of a restored pane (owner decision 2026-11-11): the reply whose finished
/// results a close discarded says "Results from before this was closed were not sent to the
/// agent. ..." on screen. It is SCREEN ONLY: on the paths the test above checks for the
/// discarded result (every request, every exchange, every task message) and in the copy /
/// export text, the note's words never appear, before the next prompt or after it. The note
/// itself stays with its reply as history across those prompts.
#[test]
fn the_note_of_a_restored_pane_is_in_no_request_and_no_export() {
    use crate::ai::agent::GrepResult;
    use crate::ai::blocklist::action_model::RESULTS_WITHHELD_BY_CLOSE_NOTE as NOTE;
    use crate::features::FeatureFlag;

    // Any request or export that held the note would hold this part of it.
    const NEEDLE: &str = "were not sent to the agent";
    assert!(NOTE.contains(NEEDLE));

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
        let discarded_id = AIAgentActionId::from("ran-before-close".to_owned());
        let reply = [discarded_id.clone()];

        controller.update(&mut app, |controller, ctx| {
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    grep_result(
                        "ran-before-close",
                        &root_task_id,
                        GrepResult::Success {
                            matched_files: vec![],
                        },
                    ),
                );
            });
            assert_eq!(
                controller
                    .action_model
                    .as_ref(ctx)
                    .results_withheld_by_close_note(&reply),
                None,
                "control: no close, no note"
            );
            close_and_restore_pane(controller, conversation_id, ctx);
            assert_eq!(
                controller
                    .action_model
                    .as_ref(ctx)
                    .results_withheld_by_close_note(&reply),
                Some(NOTE),
                "precondition: the restored reply shows the note"
            );
            controller.send_user_query_in_conversation(
                "what next".to_owned(),
                conversation_id,
                None,
                ctx,
            );
        });
        // Wait by wall clock, not a yield count (#2219).
        crate::test_util::wait_for(|| {
            controller.read(&app, |controller, ctx| {
                everything_sent_recorded_or_exported(controller, conversation_id, ctx)
                    .contains("what next")
            })
        })
        .await;
        controller.update(&mut app, |controller, ctx| {
            let text = everything_sent_recorded_or_exported(controller, conversation_id, ctx);
            assert!(
                text.contains("what next"),
                "precondition: the prompt after the undo is in what was read: {text}"
            );
            assert!(
                !text.contains(NEEDLE),
                "the note is on screen only; it is in a request, the history or the export: {text}"
            );
            controller.send_user_query_in_conversation(
                "and then".to_owned(),
                conversation_id,
                None,
                ctx,
            );
        });
        // Wait by wall clock, not a yield count (#2219).
        crate::test_util::wait_for(|| {
            controller.read(&app, |controller, ctx| {
                everything_sent_recorded_or_exported(controller, conversation_id, ctx)
                    .contains("and then")
            })
        })
        .await;
        controller.update(&mut app, |controller, ctx| {
            let text = everything_sent_recorded_or_exported(controller, conversation_id, ctx);
            assert!(text.contains("and then"), "precondition: {text}");
            assert!(!text.contains(NEEDLE), "{text}");
            assert_eq!(
                controller
                    .action_model
                    .as_ref(ctx)
                    .results_withheld_by_close_note(&reply),
                Some(NOTE),
                "the note stays with its reply as history: the card it explains still shows \
                 the kept status"
            );
        });
    });
}

/// Positive control for the test above: with no close, the same finished result DOES travel
/// with the user's next prompt (`send_query` prepends the finished results). Without this,
/// "no result in the request" could be an artifact of the setup.
#[test]
fn without_a_close_the_finished_result_travels_with_the_next_prompt() {
    use crate::ai::agent::GrepResult;
    use crate::features::FeatureFlag;

    App::test((), |mut app| async move {
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);

        controller.update(&mut app, |controller, ctx| {
            controller.action_model.update(ctx, |action_model, _| {
                action_model.push_finished_action_result_for_test(
                    conversation_id,
                    grep_result(
                        "ran-before-close",
                        &root_task_id,
                        GrepResult::Success {
                            matched_files: vec![],
                        },
                    ),
                );
            });
            controller.send_user_query_in_conversation(
                "what next".to_owned(),
                conversation_id,
                None,
                ctx,
            );
        });
        // Wait by wall clock, not a yield count (#2219).
        crate::test_util::wait_for(|| {
            controller.read(&app, |controller, ctx| {
                prompts_sent(conversation_id, ctx) == vec!["what next".to_owned()]
            })
        })
        .await;
        controller.update(&mut app, |controller, ctx| {
            assert_eq!(
                prompts_sent(conversation_id, ctx),
                vec!["what next".to_owned()]
            );
            assert_eq!(
                result_ids_sent_or_recorded(controller, conversation_id, ctx),
                vec!["ran-before-close".to_owned()],
                "an unsent finished result rides on the user's next prompt"
            );
        });
    });
}

/// #2104: a stop (or a pane close's stop, or a new turn) while a batch of tool calls is still
/// PREPROCESSING. The batch is held there by the action model's test hold
/// (`set_preprocess_hold_for_test`): the stand-in for a slow diff apply, and for a hung one
/// (#1894) when the test never releases it.
///
/// Before #2104 the stop dropped the batch and left its calls with no status and no result
/// until the batch's future came back; the late result was then recorded without a look at what
/// the call already had, and without a `FinishedAction`.
mod stop_while_preprocessing_2104 {
    use std::time::Duration;

    use super::super::super::action_model::{
        CancelledPendingActions, stop_settles_preprocessing_enabled,
    };
    use super::*;

    type Controller = super::super::BlocklistAIController;
    type ConversationId = crate::ai::agent::conversation::AIConversationId;

    fn command(id: &str, task_id: &TaskId) -> AIAgentAction {
        AIAgentAction {
            id: AIAgentActionId::from(id.to_owned()),
            task_id: task_id.clone(),
            action: shell_command(),
            requires_result: false,
        }
    }

    fn ids(names: &[&str]) -> Vec<AIAgentActionId> {
        names
            .iter()
            .map(|name| AIAgentActionId::from((*name).to_owned()))
            .collect()
    }

    /// Queues `names` as ONE reply's batch, the way the controller does when a stream ends
    /// (`queue_actions`), and holds it in preprocessing.
    fn queue_held_batch(
        controller: &mut Controller,
        conversation_id: ConversationId,
        task_id: &TaskId,
        names: &[&str],
        ctx: &mut warpui::ModelContext<Controller>,
    ) -> Vec<AIAgentAction> {
        let actions: Vec<AIAgentAction> = names.iter().map(|name| command(name, task_id)).collect();
        let queued = actions.clone();
        controller.action_model.update(ctx, |action_model, ctx| {
            action_model.set_preprocess_hold_for_test(true);
            action_model.queue_actions(queued, conversation_id, ctx);
        });
        let action_model = controller.action_model.as_ref(ctx);
        for action in &actions {
            assert!(
                action_model
                    .get_action_status(&action.id)
                    .is_some_and(|status| status.is_preprocessing()),
                "precondition: {} is still preprocessing",
                action.id
            );
        }
        actions
    }

    /// How many finished, unsent results `id` has in the conversation.
    fn result_count(
        controller: &Controller,
        conversation_id: ConversationId,
        id: &AIAgentActionId,
        app: &warpui::AppContext,
    ) -> usize {
        controller
            .action_model
            .as_ref(app)
            .get_finished_action_results(conversation_id)
            .map_or(0, |results| {
                results.iter().filter(|result| &result.id == id).count()
            })
    }

    /// Lets the main loop turn: posted events are handled, and a released batch's preprocess
    /// result arrives. Long enough for both by a wide margin (the kill-switch test is the
    /// control: the same wait there DOES bring the late result).
    async fn let_the_main_loop_turn() {
        for _ in 0..8 {
            for _ in 0..32 {
                futures_lite::future::yield_now().await;
            }
            warpui::r#async::Timer::after(Duration::from_millis(1)).await;
        }
    }

    /// Fails on the old code at the first assertion after the stop: the calls had no status.
    #[test]
    fn a_stop_gives_the_calls_of_a_preprocessing_batch_their_cancelled_result_at_the_stop() {
        App::test((), |mut app| async move {
            let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
            assert!(
                stop_settles_preprocessing_enabled(),
                "precondition: the kill switch is not set in the test environment"
            );
            let names = ["pre-a", "pre-b"];
            controller.update(&mut app, |controller, ctx| {
                queue_held_batch(controller, conversation_id, &root_task_id, &names, ctx);
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
                // In the same call, with the batch's future still out (the hold was never
                // released: a hung apply).
                for id in ids(&names) {
                    assert!(
                        controller
                            .action_model
                            .as_ref(ctx)
                            .get_action_status(&id)
                            .is_some_and(|status| status.is_cancelled()),
                        "{id} must be cancelled AT the stop, not when its batch comes back"
                    );
                    assert_eq!(result_count(controller, conversation_id, &id, ctx), 1);
                }
                let order: Vec<String> = controller
                    .action_model
                    .as_ref(ctx)
                    .get_finished_action_results(conversation_id)
                    .map(|results| results.iter().map(|r| r.id.to_string()).collect())
                    .unwrap_or_default();
                assert_eq!(order, names, "the results are in the reply's order");
            });
            let_the_main_loop_turn().await;
            controller.update(&mut app, |controller, ctx| {
                assert!(
                    matches!(
                        BlocklistAIHistoryModel::as_ref(ctx)
                            .conversation(&conversation_id)
                            .expect("conversation should exist")
                            .status(),
                        ConversationStatus::Cancelled
                    ),
                    "the stop ends the turn (#1944), now through the cancelled calls' own \
                     FinishedAction"
                );
                assert!(
                    !controller
                        .action_model
                        .as_ref(ctx)
                        .has_unfinished_actions_for_conversation(conversation_id)
                );
            });
        });
    }

    /// The batch's future comes back after the stop, and after a second writer has been at the
    /// same calls (`cancel_discarded_actions`, which is what the #1967 heal used): each call
    /// still has exactly ONE result.
    ///
    /// Fails on the old code: the stop recorded nothing, `cancel_discarded_actions` recorded
    /// one result per call, and the late preprocess result recorded a second
    /// (`record_stale_cancelled` checked nothing).
    #[test]
    fn the_late_result_of_a_stopped_batch_is_dropped() {
        App::test((), |mut app| async move {
            let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
            let names = ["late-a", "late-b"];
            controller.update(&mut app, |controller, ctx| {
                let actions =
                    queue_held_batch(controller, conversation_id, &root_task_id, &names, ctx);
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
                controller.action_model.update(ctx, |action_model, ctx| {
                    action_model.cancel_discarded_actions(
                        actions,
                        conversation_id,
                        Some(CancellationReason::ManuallyCancelled),
                        ctx,
                    );
                    for id in ids(&names) {
                        assert!(
                            action_model
                                .awaits_late_preprocess_result_for_test(conversation_id, &id),
                            "{id}: the batch's future is still out"
                        );
                    }
                    assert_eq!(
                        action_model.release_held_preprocessing_for_test(),
                        1,
                        "one batch was held"
                    );
                });
            });
            let_the_main_loop_turn().await;
            controller.update(&mut app, |controller, ctx| {
                for id in ids(&names) {
                    assert_eq!(
                        result_count(controller, conversation_id, &id, ctx),
                        1,
                        "{id}: one result, whatever came back late"
                    );
                    assert!(
                        !controller
                            .action_model
                            .as_ref(ctx)
                            .awaits_late_preprocess_result_for_test(conversation_id, &id),
                        "{id}: the late result arrived and was dropped"
                    );
                }
            });
        });
    }

    /// A pane close (#848) with a batch still preprocessing, in the view's order
    /// (`suspend_agent_work_for_close`: suspend, discard, stop), the batch's future coming back
    /// while the pane is closed, then the undo. Nothing of the stopped reply is left to ride
    /// on the user's next prompt, and each call shows as cancelled (#2103).
    ///
    /// Fails on the old code: the late result was recorded while the pane was closed, with no
    /// `FinishedAction`, so the close never discarded it and it was still there after the undo.
    #[test]
    fn a_pane_close_discards_the_cancelled_results_of_a_preprocessing_batch() {
        App::test((), |mut app| async move {
            let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
            let names = ["closed-a", "closed-b"];
            controller.update(&mut app, |controller, ctx| {
                queue_held_batch(controller, conversation_id, &root_task_id, &names, ctx);
                controller.set_suspended_for_close(true);
                controller.discard_pending_work_for_close(conversation_id, ctx);
                controller.cancel_conversation_progress(
                    conversation_id,
                    CancellationReason::ManuallyCancelled,
                    ctx,
                );
            });
            // The stop's `FinishedAction`s reach the suspended branch, which discards.
            let_the_main_loop_turn().await;
            controller.update(&mut app, |controller, ctx| {
                let released = controller.action_model.update(ctx, |action_model, _| {
                    action_model.release_held_preprocessing_for_test()
                });
                assert_eq!(released, 1, "one batch was held");
            });
            // The batch's future comes back while the pane is still closed.
            let_the_main_loop_turn().await;
            controller.update(&mut app, |controller, ctx| {
                controller.set_suspended_for_close(false);
                let action_model = controller.action_model.as_ref(ctx);
                assert!(
                    action_model
                        .get_finished_action_results(conversation_id)
                        .is_none_or(|results| results.is_empty()),
                    "#848: nothing of the stopped reply is left to ride on a later request: {:?}",
                    action_model.get_finished_action_results(conversation_id)
                );
                for id in ids(&names) {
                    assert!(
                        action_model
                            .get_action_status(&id)
                            .is_some_and(|status| status.is_cancelled()),
                        "{id} shows as cancelled on the restored pane"
                    );
                    assert!(action_model.is_result_discarded_by_close(&id));
                }
            });
        });
    }

    /// A new turn's batch gives up on an earlier batch that is still preprocessing
    /// (`queue_actions`, #1859). The earlier calls are settled then, not when their future
    /// comes back, and the late result adds nothing; the new batch is not touched.
    ///
    /// Fails on the old code at the first assertion: the earlier calls had no result yet.
    #[test]
    fn a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing() {
        App::test((), |mut app| async move {
            let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
            let earlier = ["earlier-a", "earlier-b"];
            let newer = AIAgentActionId::from("newer".to_owned());
            controller.update(&mut app, |controller, ctx| {
                queue_held_batch(controller, conversation_id, &root_task_id, &earlier, ctx);
                let next = vec![command("newer", &root_task_id)];
                controller.action_model.update(ctx, |action_model, ctx| {
                    action_model.queue_actions(next, conversation_id, ctx);
                });
                for id in ids(&earlier) {
                    assert!(
                        controller
                            .action_model
                            .as_ref(ctx)
                            .get_action_status(&id)
                            .is_some_and(|status| status.is_cancelled()),
                        "{id} is cancelled when the new turn gives up on its batch"
                    );
                    assert_eq!(result_count(controller, conversation_id, &id, ctx), 1);
                }
                assert!(
                    controller
                        .action_model
                        .as_ref(ctx)
                        .get_action_status(&newer)
                        .is_some_and(|status| status.is_preprocessing()),
                    "the new turn's batch is preprocessing"
                );
                let released = controller.action_model.update(ctx, |action_model, _| {
                    action_model.release_held_preprocessing_for_test()
                });
                assert_eq!(released, 2, "both batches were held");
            });
            let_the_main_loop_turn().await;
            controller.update(&mut app, |controller, ctx| {
                for id in ids(&earlier) {
                    assert_eq!(
                        result_count(controller, conversation_id, &id, ctx),
                        1,
                        "{id}: the late result of the earlier batch adds nothing"
                    );
                }
                let status = controller
                    .action_model
                    .as_ref(ctx)
                    .get_action_status(&newer);
                assert!(
                    status.as_ref().is_some_and(|status| {
                        !status.is_preprocessing() && !status.is_cancelled()
                    }),
                    "the new turn's call left preprocessing and was not cancelled: {status:?}"
                );
            });
        });
    }

    /// The kill switch's arm (`PHOSPHOR_STOP_SETTLES_PREPROCESSING=0`): the stop drops the
    /// batch and its calls have nothing until the batch's future comes back, which then records
    /// their cancelled result, as before #2104. Driven with the flag passed in, so the test
    /// does not depend on the process environment.
    ///
    /// It is also the control for `the_late_result_of_a_stopped_batch_is_dropped`: the same
    /// release and the same wait DO deliver a late result here.
    #[test]
    fn with_the_kill_switch_off_a_stop_leaves_the_preprocessing_calls_to_their_late_result() {
        use crate::ai::blocklist::action_model::stop_settles_preprocessing_enabled_from as enabled;

        assert!(enabled(None));
        assert!(enabled(Some("1")));
        for off in ["0", " 0 ", "off", "FALSE", "no"] {
            assert!(!enabled(Some(off)), "{off:?} turns it off");
        }

        App::test((), |mut app| async move {
            let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
            let names = ["old-a", "old-b"];
            controller.update(&mut app, |controller, ctx| {
                queue_held_batch(controller, conversation_id, &root_task_id, &names, ctx);
                let cancelled = controller.action_model.update(ctx, |action_model, ctx| {
                    action_model.cancel_all_pending_actions_with(
                        conversation_id,
                        Some(CancellationReason::ManuallyCancelled),
                        false,
                        ctx,
                    )
                });
                assert_eq!(
                    cancelled,
                    CancelledPendingActions {
                        cancelled_any: false,
                        dropped_preprocessing: true,
                    }
                );
                for id in ids(&names) {
                    assert!(
                        controller
                            .action_model
                            .as_ref(ctx)
                            .get_action_status(&id)
                            .is_none(),
                        "{id}: no status until the batch comes back (the old behaviour)"
                    );
                }
                controller.action_model.update(ctx, |action_model, _| {
                    action_model.release_held_preprocessing_for_test();
                });
            });
            let_the_main_loop_turn().await;
            controller.update(&mut app, |controller, ctx| {
                for id in ids(&names) {
                    assert_eq!(
                        result_count(controller, conversation_id, &id, ctx),
                        1,
                        "{id}: the late result records the cancelled result"
                    );
                }
            });
        });
    }

    /// The switch on, the same call: what it reports to the stop (#1944 reads it).
    #[test]
    fn a_stop_reports_the_preprocessing_calls_it_cancelled() {
        App::test((), |mut app| async move {
            let (controller, conversation_id, root_task_id) = tool_loop_controller(&mut app);
            controller.update(&mut app, |controller, ctx| {
                queue_held_batch(controller, conversation_id, &root_task_id, &["only"], ctx);
                let cancelled = controller.action_model.update(ctx, |action_model, ctx| {
                    action_model.cancel_all_pending_actions_with(
                        conversation_id,
                        Some(CancellationReason::ManuallyCancelled),
                        true,
                        ctx,
                    )
                });
                assert_eq!(
                    cancelled,
                    CancelledPendingActions {
                        cancelled_any: true,
                        dropped_preprocessing: true,
                    }
                );
            });
        });
    }
}

/// #1952: a stream that ended with tool calls queued and a resume scheduled keeps the calls and
/// drops the resume; a resume alone is still scheduled, and no resume stays none.
#[test]
fn a_stream_end_never_arms_both_queued_calls_and_a_resume() {
    use super::{ResumeAtStreamEnd, resume_at_stream_end};
    assert_eq!(
        resume_at_stream_end(
            /*has_pending_resume*/ true, /*actions_were_queued*/ true
        ),
        ResumeAtStreamEnd::Drop
    );
    assert_eq!(
        resume_at_stream_end(true, false),
        ResumeAtStreamEnd::Schedule
    );
    assert_eq!(
        resume_at_stream_end(false, true),
        ResumeAtStreamEnd::NoResume
    );
    assert_eq!(
        resume_at_stream_end(false, false),
        ResumeAtStreamEnd::NoResume
    );
}

/// #1949: a BYOP request blocked before it opens a stream (persistence expected but unavailable)
/// ends its turn visibly: its local stream id is forgotten, so the conversation no longer reads
/// as processing it, and `FinishedReceivingOutput` is announced so the view drains or unlocks
/// the conversation's queued prompts.
#[test]
fn a_blocked_byop_request_ends_its_turn_with_a_stream_end() {
    App::test((), |mut app| async move {
        let (controller, conversation_id, request_input) =
            byop_conversation_awaiting_tool_result(&mut app, true);

        let finished = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let finished_sub = finished.clone();
        app.update(|ctx| {
            ctx.subscribe_to_model(&controller, move |_, event, _| {
                if let BlocklistAIControllerEvent::FinishedReceivingOutput {
                    stream_id,
                    conversation_id,
                } = event
                {
                    finished_sub
                        .borrow_mut()
                        .push((stream_id.clone(), *conversation_id));
                }
            });
        });

        let (sent_conversation_id, stream_id) = controller.update(&mut app, |controller, ctx| {
            controller
                .send_request_input(
                    request_input,
                    None,
                    /*default_to_follow_up_on_success*/ false,
                    RecoveryBudget::fresh(),
                    /*is_queued_prompt*/ false,
                    ctx,
                )
                .expect("a blocked request renders as an ended turn, not an error")
        });
        assert_eq!(sent_conversation_id, conversation_id);

        let still_processing = BlocklistAIHistoryModel::handle(&app).read(&app, |history, _| {
            history
                .conversation(&conversation_id)
                .expect("conversation should exist")
                .is_processing_response_stream(&stream_id)
        });
        assert!(
            !still_processing,
            "the blocked request's local stream id must not stay in the conversation's map"
        );
        let finished = finished.borrow();
        assert_eq!(
            finished.len(),
            1,
            "exactly one turn end must be announced, got {finished:?}"
        );
        assert_eq!(finished[0].0, stream_id);
        assert_eq!(finished[0].1, conversation_id);
    });
}

/// #1947: a submit parked for its attachment read supersedes a request that opened during the
/// read (a monitored command finishing resumes the conversation). The typed prompt is sent; it is
/// not refused by the in-flight guard and dropped.
#[test]
fn a_parked_submit_supersedes_a_request_opened_during_its_read() {
    App::test((), |mut app| async move {
        let (controller, conversation_id, request_input) =
            byop_conversation_awaiting_tool_result(&mut app, false);
        // A landing read toasts only on a failed file; production registers this.
        app.add_singleton_model(|_| crate::workspace::ToastStack);

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
                .expect("the request that opens during the read should be sent");
            assert!(
                controller.has_active_stream_for_conversation(conversation_id, ctx),
                "precondition: a request is in flight when the read lands"
            );
            let seq = controller.park_query_send_for_test(
                conversation_id,
                TaskId::new("root-task".to_owned()),
                "typed during the read",
            );
            controller.land_attachment_read_for_test(seq, ctx);
            assert_eq!(controller.pending_query_send_count_for_test(), 0);
        });

        let prompt_sent = BlocklistAIHistoryModel::handle(&app).read(&app, |history, _| {
            history
                .conversation(&conversation_id)
                .expect("conversation should exist")
                .all_exchanges()
                .iter()
                .any(|exchange| {
                    exchange
                        .input
                        .iter()
                        .any(|input| input.user_query().as_deref() == Some("typed during the read"))
                })
        });
        assert!(
            prompt_sent,
            "the parked prompt must reach the conversation, not be lost to the in-flight guard"
        );
    });
}
