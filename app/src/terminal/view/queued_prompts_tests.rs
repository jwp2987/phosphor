//! Tests for the auto-fire drain logic that runs from [`super::TerminalView::drain_queued_prompts`].
//!
//! `TerminalView` orchestrates the input editor and the singleton `QueuedQueryModel` on
//! `FinishedReceivingOutput`. The lightweight tests exercise the per-conversation drain semantics
//! at the model level; the heavier tests construct a full `TerminalView` and call
//! `drain_queued_prompts` directly to validate the restore-to-input paths.
//!
//! Zap-adapted port of warp/master's `queued_prompts_tests.rs`: the cloud-mode integration tests
//! (dispatched cloud prompt/follow-up, cloud-setup enter/cleanup, promptless-setup, the
//! `InitialCloudMode` locked-row and its copy affordance) are dropped, since Cloud Mode is a Warp
//! cloud feature Zap does not have. The `/compact-and` `enqueue_followup_prompt` and
//! `send_lrc_queued_prompts` command-finish tests are left for the follow-up that ports those
//! methods.
use warpui::{App, SingletonEntity};

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::agent_view::AgentViewEntryOrigin;
use crate::ai::blocklist::block::FinishReason;
use crate::ai::blocklist::{
    AutofireAction, BlocklistAIHistoryModel, QueuedQuery, QueuedQueryModel, QueuedQueryOrigin,
};
use crate::features::FeatureFlag;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::test_util::terminal::{add_window_with_terminal, initialize_app_for_terminal_view};

fn user_query(text: &str) -> QueuedQuery {
    QueuedQuery::new(text.to_owned(), QueuedQueryOrigin::QueueSlashCommand)
}

fn command_query(text: &str) -> QueuedQuery {
    QueuedQuery::new_command(text.to_owned(), QueuedQueryOrigin::AutoQueueToggle)
}

/// Mirrors `TerminalView::drain_queued_prompts`' Complete path at the model level: peek the head
/// row's action, then remove the fired row (both `AutofireAction` variants carry the row id).
fn drain_one(
    model: &warpui::ModelHandle<QueuedQueryModel>,
    app: &mut App,
    conv: AIConversationId,
) -> Option<AutofireAction> {
    model.update(app, |m, ctx| {
        let action = m.peek_autofire(conv);
        if let Some(
            AutofireAction::Submit { query_id, .. }
            | AutofireAction::PopFromEditMode { query_id, .. },
        ) = &action
        {
            m.remove_fired_row(conv, *query_id, ctx);
        }
        action
    })
}

fn with_singleton<F>(test: F)
where
    F: FnOnce(App, warpui::ModelHandle<QueuedQueryModel>, AIConversationId) + 'static,
{
    App::test((), |mut app| async move {
        // `QueuedQueryModel::new` reads and subscribes to `AISettings`, so settings
        // must be registered before it.
        initialize_settings_for_tests(&mut app);
        let _ = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let model = app.add_singleton_model(QueuedQueryModel::new);
        test(app, model, AIConversationId::new());
    });
}

#[test]
fn complete_drain_pops_head_and_returns_submit_action() {
    // On Complete, the next queued prompt fires via Submit.
    with_singleton(|mut app, model, conv| {
        model.update(&mut app, |m, ctx| {
            m.append(conv, user_query("first"), ctx);
            m.append(conv, user_query("second"), ctx);
        });

        let action = drain_one(&model, &mut app, conv);
        match action {
            Some(AutofireAction::Submit { text, .. }) => assert_eq!(text, "first"),
            other => panic!("expected Submit, got {other:?}"),
        }
        model.read(&app, |m, _| {
            assert_eq!(m.queue(conv).len(), 1);
            assert_eq!(m.queue(conv)[0].text(), "second");
        });
    });
}

#[test]
fn complete_drain_with_first_row_in_edit_mode_returns_pop_from_edit_mode() {
    // When the first row is being edited, drain produces a PopFromEditMode action carrying the
    // row's last-committed text (per spec, NOT any uncommitted live-editor buffer text).
    with_singleton(|mut app, model, conv| {
        let id_a = model.update(&mut app, |m, ctx| m.append(conv, user_query("first"), ctx));
        model.update(&mut app, |m, ctx| {
            m.append(conv, user_query("second"), ctx);
            m.enter_edit_mode(conv, id_a, ctx);
        });

        let action = drain_one(&model, &mut app, conv);
        match action {
            Some(AutofireAction::PopFromEditMode {
                text, is_command, ..
            }) => {
                assert_eq!(text, "first");
                assert!(!is_command);
            }
            other => panic!("expected PopFromEditMode, got {other:?}"),
        }
        // Edit mode is cleared after pop.
        model.read(&app, |m, _| {
            assert_eq!(m.editing_row(conv), None);
            assert_eq!(m.queue(conv).len(), 1);
            assert_eq!(m.queue(conv)[0].text(), "second");
        });
    });
}

#[test]
fn complete_drain_with_non_empty_input_preserves_edited_head_row() {
    // The host skips autofire when the queue head is being edited and the input already contains
    // text, which leaves the queued row in place for the next completion.
    with_singleton(|mut app, model, conv| {
        let id_a = model.update(&mut app, |m, ctx| m.append(conv, user_query("first"), ctx));
        model.update(&mut app, |m, ctx| {
            m.append(conv, user_query("second"), ctx);
            m.enter_edit_mode(conv, id_a, ctx);
        });

        let simulated_input_is_non_empty = true;
        if !(simulated_input_is_non_empty
            && model.read(&app, |m, _| m.first_row_is_in_edit_mode(conv)))
        {
            drain_one(&model, &mut app, conv);
        }

        model.read(&app, |m, _| {
            assert_eq!(m.editing_row(conv), Some(id_a));
            assert_eq!(m.queue(conv).len(), 2);
            assert_eq!(m.queue(conv)[0].text(), "first");
            assert_eq!(m.queue(conv)[1].text(), "second");
        });
    });
}

#[test]
fn complete_drain_with_empty_queue_returns_none() {
    with_singleton(|mut app, model, conv| {
        let action = drain_one(&model, &mut app, conv);
        assert!(action.is_none());
    });
}

#[test]
fn error_or_cancel_drain_pops_front_when_input_is_empty() {
    // On Error/Cancelled with an empty input, the next queued prompt's text is restored to the
    // input by popping it (which the host then writes into the buffer).
    with_singleton(|mut app, model, conv| {
        model.update(&mut app, |m, ctx| {
            m.append(conv, user_query("first"), ctx);
            m.append(conv, user_query("second"), ctx);
        });

        let popped = model.update(&mut app, |m, ctx| m.pop_front(conv, ctx));
        let popped = popped.expect("queue had a head");
        assert_eq!(popped.text(), "first");
        model.read(&app, |m, _| {
            assert_eq!(m.queue(conv).len(), 1);
            assert_eq!(m.queue(conv)[0].text(), "second");
        });
    });
}

#[test]
fn error_or_cancel_drain_leaves_queue_intact_when_input_is_non_empty() {
    // When the input is non-empty, the drain skips popping so the queue remains intact.
    //
    // The host (`TerminalView`) gates the pop on input-empty. We model that here by simply not
    // popping when the simulated input is non-empty, and asserting the queue remains unchanged.
    with_singleton(|mut app, model, conv| {
        model.update(&mut app, |m, ctx| {
            m.append(conv, user_query("first"), ctx);
            m.append(conv, user_query("second"), ctx);
        });

        let simulated_input_is_non_empty = true;
        if !simulated_input_is_non_empty {
            model.update(&mut app, |m, ctx| m.pop_front(conv, ctx));
        }

        model.read(&app, |m, _| {
            assert_eq!(m.queue(conv).len(), 2);
            assert_eq!(m.queue(conv)[0].text(), "first");
        });
    });
}

#[test]
fn complete_drain_after_error_drain_continues_with_next_row() {
    // After an Error/Cancelled drain pops one row and the user later submits successfully, the
    // *next* Complete drain pops the following row.
    with_singleton(|mut app, model, conv| {
        model.update(&mut app, |m, ctx| {
            m.append(conv, user_query("first"), ctx);
            m.append(conv, user_query("second"), ctx);
            m.append(conv, user_query("third"), ctx);
        });

        // Error: input is empty, pop "first" and restore to input.
        let popped = model.update(&mut app, |m, ctx| m.pop_front(conv, ctx));
        assert_eq!(
            popped.map(|q| q.text().to_owned()),
            Some("first".to_owned())
        );

        // Complete: pop "second".
        let action = drain_one(&model, &mut app, conv);
        match action {
            Some(AutofireAction::Submit { text, .. }) => assert_eq!(text, "second"),
            other => panic!("expected Submit(\"second\"), got {other:?}"),
        }

        // Complete again: pop "third".
        let action = drain_one(&model, &mut app, conv);
        match action {
            Some(AutofireAction::Submit { text, .. }) => assert_eq!(text, "third"),
            other => panic!("expected Submit(\"third\"), got {other:?}"),
        }

        // Queue is now empty; the next drain returns None.
        let action = drain_one(&model, &mut app, conv);
        assert!(action.is_none());
    });
}

#[test]
fn drain_is_isolated_per_conversation() {
    // A drain for conversation A must not pop rows from conversation B.
    with_singleton(|mut app, model, conv_a| {
        let conv_b = AIConversationId::new();
        model.update(&mut app, |m, ctx| {
            m.append(conv_a, user_query("a-first"), ctx);
            m.append(conv_b, user_query("b-first"), ctx);
        });

        let action = drain_one(&model, &mut app, conv_a);
        match action {
            Some(AutofireAction::Submit { text, .. }) => assert_eq!(text, "a-first"),
            other => panic!("expected Submit(\"a-first\"), got {other:?}"),
        }
        model.read(&app, |m, _| {
            assert_eq!(m.queue(conv_a).len(), 0);
            assert_eq!(m.queue(conv_b).len(), 1);
            assert_eq!(m.queue(conv_b)[0].text(), "b-first");
        });
    });
}

#[test]
fn complete_drain_of_edited_command_restores_text_in_shell_mode() {
    // A command row being edited when the agent finishes cleanly is popped into the input in
    // shell mode, so the restored text stays a command rather than being submitted as a prompt.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _agent_view = FeatureFlag::AgentView.override_enabled(true);

        let terminal = add_window_with_terminal(&mut app, None);
        // Entering agent view puts the input in agent (AI) mode, so the drain must actively
        // switch it to shell mode for the restored command.
        let conversation_id = terminal.update(&mut app, |view, ctx| {
            view.agent_view_controller().update(ctx, |controller, ctx| {
                controller
                    .try_enter_agent_view(
                        None,
                        AgentViewEntryOrigin::Input {
                            was_prompt_autodetected: false,
                        },
                        ctx,
                    )
                    .expect("should enter agent view")
            })
        });
        terminal.read(&app, |view, ctx| {
            assert!(view.ai_input_model.as_ref(ctx).is_ai_input_enabled());
        });

        QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            let id = model.append(conversation_id, command_query("echo 1"), ctx);
            model.enter_edit_mode(conversation_id, id, ctx);
        });

        terminal.update(&mut app, |view, ctx| {
            view.drain_queued_prompts(conversation_id, FinishReason::Complete, ctx);
        });

        terminal.read(&app, |view, ctx| {
            assert_eq!(view.input().as_ref(ctx).buffer_text(ctx), "echo 1");
            assert!(!view.ai_input_model.as_ref(ctx).is_ai_input_enabled());
        });
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert!(model.queue(conversation_id).is_empty());
        });
    });
}

#[test]
fn error_drain_of_command_restores_text_in_shell_mode() {
    // On a non-clean finish, the head command is popped into the empty input in shell mode, so a
    // restored command stays a command.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _agent_view = FeatureFlag::AgentView.override_enabled(true);

        let terminal = add_window_with_terminal(&mut app, None);
        // The cancel restore path only fires for the conversation the user is viewing; entering
        // agent view makes `conversation_id` active and puts the input in agent (AI) mode.
        let conversation_id = terminal.update(&mut app, |view, ctx| {
            view.agent_view_controller().update(ctx, |controller, ctx| {
                controller
                    .try_enter_agent_view(
                        None,
                        AgentViewEntryOrigin::Input {
                            was_prompt_autodetected: false,
                        },
                        ctx,
                    )
                    .expect("should enter agent view")
            })
        });

        QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(conversation_id, command_query("echo 1"), ctx);
        });

        terminal.update(&mut app, |view, ctx| {
            view.drain_queued_prompts(conversation_id, FinishReason::Cancelled, ctx);
        });

        terminal.read(&app, |view, ctx| {
            assert_eq!(view.input().as_ref(ctx).buffer_text(ctx), "echo 1");
            assert!(!view.ai_input_model.as_ref(ctx).is_ai_input_enabled());
        });
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert!(model.queue(conversation_id).is_empty());
        });
    });
}

#[test]
fn complete_drain_keeps_command_row_when_dispatch_fails_with_draft() {
    // A queued command whose dispatch is blocked by a user draft in the input stays queued.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.view_id);
        let conversation_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
                let id = history.start_new_conversation(terminal_view_id, false, false, ctx);
                history.set_active_conversation_id(id, terminal_view_id, ctx);
                id
            });
        let query_id = QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(conversation_id, command_query("echo 1"), ctx)
        });

        terminal.update(&mut app, |view, ctx| {
            view.input().update(ctx, |input, ctx| {
                input.replace_buffer_content("draft in progress", ctx);
            });
            view.drain_queued_prompts(conversation_id, FinishReason::Complete, ctx);
        });

        terminal.read(&app, |view, ctx| {
            assert_eq!(
                view.input().as_ref(ctx).buffer_text(ctx),
                "draft in progress"
            );
        });
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            let queue = model.queue(conversation_id);
            assert_eq!(queue.len(), 1);
            assert_eq!(queue[0].id(), query_id);
            assert_eq!(queue[0].text(), "echo 1");
            assert!(queue[0].is_command());
        });
    });
}
