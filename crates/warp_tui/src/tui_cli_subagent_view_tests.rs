use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use warp::tui_export::{
    AIAgentAction, AIAgentActionId, AIAgentActionType, AIAgentPtyWriteMode, AIConversationId,
    BlockId, BlocklistAIActionEvent, CancellationReason, LongRunningCommandControlState, TaskId,
    UserTakeOverReason, queue_tui_permission_action, register_tui_action_execution_test_singletons,
    tui_command_review,
};
use warpui_core::App;

use super::{
    BlockedActionPresentation, BlockedCommandGate, blocked_action_presentation,
    blocked_command_gate, blocked_command_is_hidden, can_allow_blocked_action,
    cancel_blocked_action, display_pty_input, execute_blocked_action, format_next_check_remaining,
    hidden_command_status_text, remaining_for_fixed_delay, resolve_latest_instruction,
    show_full_blocked_command, terminal_use_status_text,
};
use crate::test_fixtures::add_test_action_model;

#[test]
fn terminal_use_status_covers_control_and_lifecycle_states() {
    let agent = LongRunningCommandControlState::Agent {
        is_blocked: false,
        should_hide_responses: false,
    };
    assert_eq!(
        terminal_use_status_text(&agent, false, true, true),
        "Agent is monitoring command · ctrl-c to take control"
    );
    assert_eq!(
        terminal_use_status_text(&agent, false, false, true),
        "Agent waiting for instructions · ctrl-c to take control"
    );
    assert_eq!(
        terminal_use_status_text(&agent, true, true, true),
        "Command finished"
    );

    let blocked = LongRunningCommandControlState::Agent {
        is_blocked: true,
        should_hide_responses: false,
    };
    assert_eq!(
        terminal_use_status_text(&blocked, false, true, true),
        "Agent needs your input · ctrl-o to allow · ctrl-r to reject"
    );
    // The blocked hint takes priority over the streaming/idle hints below --
    // is_blocked wins regardless of output_streaming.
    assert_eq!(
        terminal_use_status_text(&blocked, false, false, true),
        "Agent needs your input · ctrl-o to allow · ctrl-r to reject"
    );

    let manual = LongRunningCommandControlState::User {
        reason: UserTakeOverReason::Manual,
    };
    assert_eq!(
        terminal_use_status_text(&manual, false, false, true),
        "User is in control · ctrl-g to hand back"
    );

    let stopped = LongRunningCommandControlState::User {
        reason: UserTakeOverReason::Stop {
            should_auto_resume: true,
        },
    };
    assert_eq!(
        terminal_use_status_text(&stopped, false, false, true),
        "Agent paused · user is in control · ctrl-g to hand back"
    );

    let transferred = LongRunningCommandControlState::User {
        reason: UserTakeOverReason::TransferFromAgent {
            reason: "enter password".to_owned(),
        },
    };
    assert_eq!(
        terminal_use_status_text(&transferred, false, false, true),
        "Agent handed control to you · ctrl-g to hand back"
    );

    // A password-prompt hand-over reads differently from the agent-initiated transfer above:
    // nobody chose to hand over, the command stopped and needs a keystroke the agent cannot
    // supply. The hand-back binding is still the way out once the prompt is answered.
    let blocked_on_input = LongRunningCommandControlState::User {
        reason: UserTakeOverReason::BlockedOnInput,
    };
    assert_eq!(
        terminal_use_status_text(&blocked_on_input, false, false, true),
        "Command needs your input · user is in control · ctrl-g to hand back"
    );
    assert_eq!(
        terminal_use_status_text(&blocked_on_input, true, false, true),
        "Command finished"
    );
}

/// A blocked `ask_user_question` is resolved by answering it, which ctrl-o cannot do (see
/// `execute_blocked_action`). The hint must not offer ctrl-o for it, and must still offer the
/// reject key.
///
/// Red on the current code: `terminal_use_status_text` has no notion of the blocked action and
/// always renders "ctrl-o to allow".
#[test]
fn blocked_question_status_does_not_offer_allow() {
    let blocked = LongRunningCommandControlState::Agent {
        is_blocked: true,
        should_hide_responses: false,
    };
    let status = terminal_use_status_text(&blocked, false, true, false);
    assert_eq!(
        status,
        "Agent is waiting for your answer in the conversation \u{b7} ctrl-r to reject"
    );
    assert!(!status.contains("ctrl-o"));
    // "Command finished" still wins, as it does for every other blocked action.
    assert_eq!(
        terminal_use_status_text(&blocked, true, true, false),
        "Command finished"
    );
}

/// [Allow] is offered for exactly the actions `execute_blocked_action` will run.
///
/// Red on the current code: the view renders [Allow] unconditionally for any blocked action,
/// including a question, where clicking it does nothing.
#[test]
fn allow_is_offered_only_for_actions_a_plain_accept_can_confirm() {
    assert!(!can_allow_blocked_action(Some(
        &question_action("q").action
    )));
    assert!(can_allow_blocked_action(Some(&test_action("w").action)));
    assert!(
        can_allow_blocked_action(None),
        "an unknown blocked action keeps the existing offer"
    );
}

fn question_action(id: &str) -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from(id.to_owned()),
        task_id: TaskId::new("terminal-use-task".to_owned()),
        action: AIAgentActionType::AskUserQuestion { questions: vec![] },
        requires_result: true,
    }
}

/// Collects the ids of every action the model starts executing.
fn record_executing_actions(
    action_model: &warpui_core::ModelHandle<warp::tui_export::BlocklistAIActionModel>,
    ctx: &mut warpui_core::AppContext,
) -> Rc<RefCell<Vec<AIAgentActionId>>> {
    let executing_ids = Rc::new(RefCell::new(Vec::new()));
    let executing_ids_for_event = executing_ids.clone();
    ctx.subscribe_to_model(action_model, move |_, event, _| {
        if let BlocklistAIActionEvent::ExecutingAction(action_id) = event {
            executing_ids_for_event.borrow_mut().push(action_id.clone());
        }
    });
    executing_ids
}

/// ctrl-o / [Allow] on a blocked question must not run it: executing it as the user with no
/// answer on `AskUserQuestionExecutor`'s channel hangs the turn forever.
///
/// Guards the call-site check in `execute_blocked_action` (it calls `execute_action` by id, so
/// the model-level guard in `execute_next_action_for_user` does not cover it). Red if that
/// check is removed: the question is then dequeued and executed.
#[test]
fn allow_refuses_a_blocked_question() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        // See the note in allow_executes_the_exact_displayed_action.
        register_tui_action_execution_test_singletons(&mut app);
        let conversation_id = AIConversationId::new();
        let question = question_action("question");

        let executing_ids = app.update(|ctx| {
            let executing_ids = record_executing_actions(&action_model, ctx);
            action_model.update(ctx, |action_model, ctx| {
                queue_tui_permission_action(action_model, question.clone(), conversation_id, ctx);
                execute_blocked_action(action_model, conversation_id, &question, ctx);
            });
            executing_ids
        });

        assert!(executing_ids.borrow().is_empty());
        app.read(|ctx| {
            assert!(
                action_model
                    .as_ref(ctx)
                    .get_pending_actions_for_conversation(&conversation_id)
                    .any(|action| action.id == question.id),
                "the question stays pending so it can still be answered or rejected"
            );
        });
    });
}

/// The AI block's Enter key (`AIBlockAction::ExecuteNextPendingAction`, bound whenever any
/// action is pending) and the GUI CLI subagent view's accept both reach
/// `BlocklistAIActionModel::execute_next_action_for_user`, which runs the queue front as the
/// user. A front question must be refused there -- and the path must not skip past it to the
/// action queued behind it, which would act before the user has answered.
///
/// Red on the current code: `execute_next_action_for_user` takes the queue front
/// unconditionally, so the question is dequeued and executed with an empty answer channel.
#[test]
fn execute_next_action_for_user_refuses_a_front_question() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        // See the note in allow_executes_the_exact_displayed_action.
        register_tui_action_execution_test_singletons(&mut app);
        let conversation_id = AIConversationId::new();
        let question = question_action("question");
        let behind = test_action("behind");

        let executing_ids = app.update(|ctx| {
            let executing_ids = record_executing_actions(&action_model, ctx);
            action_model.update(ctx, |action_model, ctx| {
                queue_tui_permission_action(action_model, question.clone(), conversation_id, ctx);
                queue_tui_permission_action(action_model, behind.clone(), conversation_id, ctx);
                action_model.execute_next_action_for_user(conversation_id, ctx);
            });
            executing_ids
        });

        assert!(executing_ids.borrow().is_empty());
        app.read(|ctx| {
            let pending: Vec<_> = action_model
                .as_ref(ctx)
                .get_pending_actions_for_conversation(&conversation_id)
                .map(|action| action.id.clone())
                .collect();
            assert_eq!(pending, vec![question.id.clone(), behind.id.clone()]);
        });
    });
}

/// Control for the test above: the same harness and path DO execute an ordinary front action,
/// so the refusal is the guard's doing and not the harness failing to run anything.
#[test]
fn execute_next_action_for_user_still_runs_an_ordinary_front_action() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        // See the note in allow_executes_the_exact_displayed_action.
        register_tui_action_execution_test_singletons(&mut app);
        let conversation_id = AIConversationId::new();
        let front = test_action("front");

        let executing_ids = app.update(|ctx| {
            let executing_ids = record_executing_actions(&action_model, ctx);
            action_model.update(ctx, |action_model, ctx| {
                queue_tui_permission_action(action_model, front.clone(), conversation_id, ctx);
                action_model.execute_next_action_for_user(conversation_id, ctx);
            });
            executing_ids
        });

        assert_eq!(executing_ids.borrow().first(), Some(&front.id));
    });
}

fn test_action(id: &str) -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from(id.to_owned()),
        task_id: TaskId::new("terminal-use-task".to_owned()),
        action: AIAgentActionType::WriteToLongRunningShellCommand {
            block_id: BlockId::new(),
            input: b"input".to_vec().into(),
            mode: AIAgentPtyWriteMode::Raw,
        },
        requires_result: true,
    }
}

#[test]
fn allow_executes_the_exact_displayed_action() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        // These tests drive actions without a full session view, so the
        // execution-pipeline singletons are not otherwise registered.
        register_tui_action_execution_test_singletons(&mut app);
        let conversation_id = AIConversationId::new();
        let first = test_action("first");
        let displayed = test_action("displayed");
        let executing_ids = Rc::new(RefCell::new(Vec::new()));
        let executing_ids_for_event = executing_ids.clone();

        app.update(|ctx| {
            ctx.subscribe_to_model(&action_model, move |_, event, _| {
                if let BlocklistAIActionEvent::ExecutingAction(action_id) = event {
                    executing_ids_for_event.borrow_mut().push(action_id.clone());
                }
            });
            action_model.update(ctx, |action_model, ctx| {
                queue_tui_permission_action(action_model, first, conversation_id, ctx);
                queue_tui_permission_action(action_model, displayed.clone(), conversation_id, ctx);
                execute_blocked_action(action_model, conversation_id, &displayed, ctx);
            });
        });

        assert_eq!(executing_ids.borrow().first(), Some(&displayed.id));
    });
}

#[test]
fn reject_cancels_only_the_exact_displayed_action() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        // See the note in allow_executes_the_exact_displayed_action.
        register_tui_action_execution_test_singletons(&mut app);
        let conversation_id = AIConversationId::new();
        let first = test_action("first");
        let displayed = test_action("displayed");
        let finished_actions = Rc::new(RefCell::new(Vec::new()));
        let finished_actions_for_event = finished_actions.clone();

        app.update(|ctx| {
            ctx.subscribe_to_model(&action_model, move |_, event, _| {
                if let BlocklistAIActionEvent::FinishedAction {
                    action_id,
                    cancellation_reason,
                    ..
                } = event
                {
                    finished_actions_for_event
                        .borrow_mut()
                        .push((action_id.clone(), *cancellation_reason));
                }
            });
            action_model.update(ctx, |action_model, ctx| {
                queue_tui_permission_action(action_model, first, conversation_id, ctx);
                queue_tui_permission_action(action_model, displayed.clone(), conversation_id, ctx);
                cancel_blocked_action(action_model, conversation_id, &displayed, ctx);
            });
        });

        assert!(finished_actions.borrow().iter().any(|(action_id, reason)| {
            action_id == &displayed.id && *reason == Some(CancellationReason::ManuallyCancelled)
        }));
    });
}

#[test]
fn controller_instruction_precedes_stale_exchange_input() {
    assert_eq!(
        resolve_latest_instruction(
            Some("new instruction".to_owned()),
            Some("old instruction".to_owned())
        ),
        Some("new instruction".to_owned())
    );
}

#[test]
fn next_check_countdown_decreases_and_expires() {
    assert_eq!(
        remaining_for_fixed_delay(Duration::from_secs(10), Duration::from_secs(3)),
        Some(Duration::from_secs(7))
    );
    assert_eq!(
        remaining_for_fixed_delay(Duration::from_secs(10), Duration::from_secs(10)),
        None
    );
}

#[test]
fn next_check_countdown_formats_seconds_and_minutes() {
    assert_eq!(
        format_next_check_remaining(Duration::from_secs(12)),
        " · Check in 12s"
    );
    assert_eq!(
        format_next_check_remaining(Duration::from_secs(65)),
        " · Check in 1m"
    );
}

#[test]
fn write_action_presentation_shows_input_and_mode_without_internal_ids() {
    let action = AIAgentActionType::WriteToLongRunningShellCommand {
        block_id: BlockId::new(),
        input: b"iRoses\nViolets\x1b".to_vec().into(),
        mode: AIAgentPtyWriteMode::Raw,
    };

    let presentation = blocked_action_presentation(&action);

    assert_eq!(
        presentation,
        BlockedActionPresentation {
            summary: "Agent wants to write to the running command".to_owned(),
            detail: Some("Input:\niRoses\nViolets<Esc>".to_owned()),
        }
    );
    assert!(!presentation.summary.contains("block id"));
    assert!(!presentation.detail.unwrap().contains("block id"));
}

#[test]
fn transfer_action_presentation_shows_the_agents_reason() {
    let presentation =
        blocked_action_presentation(&AIAgentActionType::TransferShellCommandControlToUser {
            reason: "Enter the sudo password".to_owned(),
        });

    assert_eq!(
        presentation,
        BlockedActionPresentation {
            summary: "Agent wants to hand command control to you".to_owned(),
            detail: Some("Reason: Enter the sudo password".to_owned()),
        }
    );
}

#[test]
fn pty_input_display_names_control_bytes_and_preserves_lines() {
    assert_eq!(
        display_pty_input(b"first\r\nsecond\x03"),
        "first<Enter>\nsecond<0x03>"
    );
}

fn command_action(id: &str, command: &str) -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from(id.to_owned()),
        task_id: TaskId::new("terminal-use-task".to_owned()),
        action: AIAgentActionType::RequestCommandOutput {
            command: command.to_owned(),
            is_read_only: Some(true),
            is_risky: Some(false),
            wait_until_completion: true,
            uses_pager: Some(false),
            rationale: None,
            citations: Vec::new(),
        },
        requires_result: true,
    }
}

/// #865: the refusal is explained, with the size of what is hidden and the key that shows it.
#[test]
fn hidden_command_status_says_why_and_offers_show_full() {
    let status = hidden_command_status_text("echo a\necho b");
    assert!(status.contains("too long to approve unseen"), "{status}");
    assert!(status.contains("2 lines, 14 chars"), "{status}");
    assert!(status.contains("ctrl-s to view it in full"), "{status}");
    assert!(status.contains("ctrl-o to allow"), "{status}");
    assert!(status.contains("ctrl-r to reject"), "{status}");
}

/// #865: Allow on a blocked command that the card would elide is refused (the #847 gate in the
/// model) until the full command was shown; showing it lifts the gate for that exact text only,
/// and the command then runs.
#[test]
fn allow_of_a_long_command_waits_for_show_full() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        register_tui_action_execution_test_singletons(&mut app);
        let conversation_id = AIConversationId::new();
        // A 200-char run is elided by the collapsed preview but fits the row cap in full.
        let hidden = format!("echo {}; touch ~/hidden-payload.txt", "A".repeat(200));
        let action = command_action("long", &hidden);

        let executing_ids = app.update(|ctx| {
            let executing_ids = record_executing_actions(&action_model, ctx);
            action_model.update(ctx, |action_model, ctx| {
                queue_tui_permission_action(action_model, action.clone(), conversation_id, ctx);
                execute_blocked_action(action_model, conversation_id, &action, ctx);
            });
            executing_ids
        });
        assert!(executing_ids.borrow().is_empty(), "accept must be refused");
        app.read(|ctx| {
            let model = action_model.as_ref(ctx);
            assert!(blocked_command_is_hidden(model, conversation_id, &action));
            assert!(!model.is_full_command_shown(conversation_id, &action.id, &hidden));
        });

        app.update(|ctx| {
            action_model.update(ctx, |action_model, ctx| {
                assert!(show_full_blocked_command(
                    action_model,
                    conversation_id,
                    &action
                ));
                execute_blocked_action(action_model, conversation_id, &action, ctx);
            });
        });
        app.read(|ctx| {
            let model = action_model.as_ref(ctx);
            assert!(!blocked_command_is_hidden(model, conversation_id, &action));
        });
        assert_eq!(executing_ids.borrow().first(), Some(&action.id));
    });
}

/// A short command is never gated, so no show-full step is asked of the user; and show-full on
/// an action that is not a command marks nothing.
#[test]
fn short_command_and_non_command_need_no_show_full() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        let conversation_id = AIConversationId::new();
        let short = command_action("short", "echo hi");
        let write = test_action("write");
        app.update(|ctx| {
            action_model.update(ctx, |action_model, _| {
                assert!(!blocked_command_is_hidden(
                    action_model,
                    conversation_id,
                    &short
                ));
                assert!(!blocked_command_is_hidden(
                    action_model,
                    conversation_id,
                    &write
                ));
                assert!(!show_full_blocked_command(
                    action_model,
                    conversation_id,
                    &write
                ));
            });
        });
    });
}

fn gate_of(
    app: &mut App,
    action_model: &warpui_core::ModelHandle<warp::tui_export::BlocklistAIActionModel>,
    conversation_id: AIConversationId,
    action: &AIAgentAction,
) -> BlockedCommandGate {
    app.read(|ctx| blocked_command_gate(action_model.as_ref(ctx), conversation_id, action))
}

fn show_full(
    app: &mut App,
    action_model: &warpui_core::ModelHandle<warp::tui_export::BlocklistAIActionModel>,
    conversation_id: AIConversationId,
    action: &AIAgentAction,
) {
    app.update(|ctx| {
        action_model.update(ctx, |action_model, _| {
            assert!(show_full_blocked_command(
                action_model,
                conversation_id,
                action
            ));
        });
    });
}

/// #865/#847: ESC, CR and a bidi override render as `<U+XXXX>` markers, never raw, even in a
/// short command; and such a command needs the show-full confirmation before Allow.
#[test]
fn short_command_with_control_chars_is_shown_escaped_and_gated() {
    for (raw, marker) in [
        ("echo \u{1b}[2Jhi", "<U+001B>"),
        ("echo hi\rrm -rf x", "<U+000D>"),
        ("echo \u{202e}txt.exe", "<U+202E>"),
    ] {
        let review = tui_command_review(raw);
        assert!(review.has_escapes, "{raw:?}");
        assert!(review.text.contains(marker), "{}", review.text);
        assert!(
            !review.text.contains(['\u{1b}', '\r', '\u{202e}']),
            "raw control char leaked: {:?}",
            review.text
        );
        assert!(review.ends_with.starts_with("Ends with: "));
        assert!(review.ends_with.contains(marker) || raw.len() > 120);

        App::test((), |mut app| async move {
            let action_model = add_test_action_model(&mut app);
            register_tui_action_execution_test_singletons(&mut app);
            let conversation_id = AIConversationId::new();
            let action = command_action("esc", raw);
            let executing_ids = app.update(|ctx| {
                let executing_ids = record_executing_actions(&action_model, ctx);
                action_model.update(ctx, |action_model, ctx| {
                    queue_tui_permission_action(action_model, action.clone(), conversation_id, ctx);
                    execute_blocked_action(action_model, conversation_id, &action, ctx);
                });
                executing_ids
            });
            assert!(executing_ids.borrow().is_empty(), "accept must be refused");
            assert_eq!(
                gate_of(&mut app, &action_model, conversation_id, &action),
                BlockedCommandGate::NeedsShowFull
            );
            show_full(&mut app, &action_model, conversation_id, &action);
            assert_eq!(
                gate_of(&mut app, &action_model, conversation_id, &action),
                BlockedCommandGate::Clear
            );
            app.update(|ctx| {
                action_model.update(ctx, |action_model, ctx| {
                    execute_blocked_action(action_model, conversation_id, &action, ctx);
                });
            });
            assert_eq!(executing_ids.borrow().first(), Some(&action.id));
        });
    }
}

/// A plain short command has no escapes and is never gated.
#[test]
fn plain_short_command_has_no_escapes_and_is_clear() {
    let review = tui_command_review("echo hi");
    assert!(!review.has_escapes);
    assert_eq!(review.text, "echo hi");
    assert_eq!(review.ends_with, "Ends with: echo hi");
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        let conversation_id = AIConversationId::new();
        let action = command_action("plain", "echo hi");
        assert_eq!(
            gate_of(&mut app, &action_model, conversation_id, &action),
            BlockedCommandGate::Clear
        );
    });
}

/// #865: a command beyond the row cap cannot be allowed even after show-full; Allow is refused
/// and the action is not executed.
#[test]
fn command_beyond_row_cap_can_never_be_allowed() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        register_tui_action_execution_test_singletons(&mut app);
        let conversation_id = AIConversationId::new();
        let long = format!("echo {}; touch ~/hidden-payload.txt", "A".repeat(100_000));
        let action = command_action("huge", &long);
        let executing_ids = app.update(|ctx| {
            let executing_ids = record_executing_actions(&action_model, ctx);
            action_model.update(ctx, |action_model, ctx| {
                queue_tui_permission_action(action_model, action.clone(), conversation_id, ctx);
            });
            executing_ids
        });
        assert_eq!(
            gate_of(&mut app, &action_model, conversation_id, &action),
            BlockedCommandGate::NeedsShowFull
        );
        show_full(&mut app, &action_model, conversation_id, &action);
        assert!(matches!(
            gate_of(&mut app, &action_model, conversation_id, &action),
            BlockedCommandGate::TooLong { .. }
        ));
        app.update(|ctx| {
            action_model.update(ctx, |action_model, ctx| {
                execute_blocked_action(action_model, conversation_id, &action, ctx);
            });
        });
        assert!(executing_ids.borrow().is_empty());
    });
}

/// An edit after show-full is a different text, so it is gated again (hidden and escaped cases).
#[test]
fn edit_after_show_full_regates_the_command() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        let conversation_id = AIConversationId::new();
        for (before, after) in [
            (
                format!("echo {}", "A".repeat(200)),
                format!("echo {}; id", "A".repeat(200)),
            ),
            ("echo \u{1b}[2J".to_owned(), "echo \u{1b}[3J".to_owned()),
        ] {
            let original = command_action("edit", &before);
            let edited = command_action("edit", &after);
            show_full(&mut app, &action_model, conversation_id, &original);
            assert_eq!(
                gate_of(&mut app, &action_model, conversation_id, &original),
                BlockedCommandGate::Clear
            );
            assert_eq!(
                gate_of(&mut app, &action_model, conversation_id, &edited),
                BlockedCommandGate::NeedsShowFull
            );
        }
    });
}

/// The ctrl-s keymap flag is armed exactly while the gate is `NeedsShowFull`: not for a plain
/// command, not once shown, not for a too-long command (nothing would help), not for a
/// non-command action.
#[test]
fn show_full_key_is_armed_only_for_a_gated_command() {
    App::test((), |mut app| async move {
        let action_model = add_test_action_model(&mut app);
        let conversation_id = AIConversationId::new();
        let armed = |gate: BlockedCommandGate| gate == BlockedCommandGate::NeedsShowFull;
        let plain = command_action("p", "echo hi");
        let write = test_action("w");
        let esc = command_action("e", "echo \u{1b}");
        let huge = command_action("h", &format!("echo {}", "A ".repeat(50_000)));
        assert!(!armed(gate_of(
            &mut app,
            &action_model,
            conversation_id,
            &plain
        )));
        assert!(!armed(gate_of(
            &mut app,
            &action_model,
            conversation_id,
            &write
        )));
        assert!(armed(gate_of(
            &mut app,
            &action_model,
            conversation_id,
            &esc
        )));
        assert!(armed(gate_of(
            &mut app,
            &action_model,
            conversation_id,
            &huge
        )));
        show_full(&mut app, &action_model, conversation_id, &esc);
        show_full(&mut app, &action_model, conversation_id, &huge);
        assert!(!armed(gate_of(
            &mut app,
            &action_model,
            conversation_id,
            &esc
        )));
        assert!(!armed(gate_of(
            &mut app,
            &action_model,
            conversation_id,
            &huge
        )));
    });
}
