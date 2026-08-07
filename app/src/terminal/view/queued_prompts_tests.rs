//! Tests for the auto-fire drain logic that runs from [`super::TerminalView::drain_queued_prompts`].
//!
//! `TerminalView` orchestrates the input editor and the singleton `QueuedQueryModel` on
//! `FinishedReceivingOutput`. The lightweight tests exercise the per-conversation drain semantics
//! at the model level; the heavier tests construct a full `TerminalView` and call
//! `drain_queued_prompts` directly to validate the restore-to-input paths.
//!
//! Zap-adapted port of warp/master's `queued_prompts_tests.rs`: the cloud-mode integration tests
//! (dispatched cloud prompt/follow-up, cloud-setup enter/cleanup, promptless-setup, the
//! `InitialCloudMode` locked-row and its copy affordance, the `QueuedPromptsV2` cloud-terminal
//! follow-up tests) are dropped, since Cloud Mode is a Warp cloud feature Zap does not have.
use std::cell::RefCell;
use std::rc::Rc;

use warpui::{App, SingletonEntity, TypedActionView, ViewContext, ViewHandle};

use super::queued_prompts_panel::{
    QueuedPromptsPanelAction, QueuedPromptsPanelEvent, QueuedPromptsPanelView,
};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::ImageContext;
use crate::ai::blocklist::agent_view::AgentViewEntryOrigin;
use crate::ai::blocklist::block::FinishReason;
use crate::ai::blocklist::{
    AutofireAction, BlocklistAIControllerEvent, BlocklistAIHistoryModel, PendingAttachment,
    QueuedQuery, QueuedQueryId, QueuedQueryModel, QueuedQueryOrigin, ResponseStreamId,
};
use crate::features::FeatureFlag;
use crate::search::slash_command_menu::static_commands::commands;
use crate::terminal::input::{Event as InputEvent, Input};
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

#[test]
fn on_queued_command_finished_clears_in_flight_and_drains_next() {
    // When a dispatched queued command's block completes, the in-flight marker is cleared and
    // the next queued row auto-fires (here an empty queue, so the drain simply no-ops).
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

        QueuedQueryModel::handle(&app).update(&mut app, |model, _ctx| {
            model.arm_command_in_flight(conversation_id);
        });
        terminal.read(&app, |_, ctx| {
            assert!(QueuedQueryModel::as_ref(ctx).has_command_in_flight(conversation_id));
        });

        terminal.update(&mut app, |view, ctx| {
            view.on_queued_command_finished(ctx);
        });

        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert!(!model.has_command_in_flight(conversation_id));
            assert!(model.queue(conversation_id).is_empty());
        });
    });
}

#[test]
fn send_lrc_queued_prompts_delivers_lrc_rows_when_no_active_subagent() {
    // With no active subagent for the conversation, `send_lrc_queued_prompts` fires the leading
    // `LrcAutoQueue` rows immediately (removing them from the queue).
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        // The prompt-submission path (`send_queued_user_query_in_conversation`) reads the global
        // model-event sender, so the provider must be registered for delivery to run.
        let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(move |_| {
            crate::GlobalResourceHandlesProvider::new(global_resource_handles.clone())
        });

        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.view_id);
        let conversation_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
                let id = history.start_new_conversation(terminal_view_id, false, false, ctx);
                history.set_active_conversation_id(id, terminal_view_id, ctx);
                id
            });

        QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(
                conversation_id,
                QueuedQuery::new("lrc prompt".to_owned(), QueuedQueryOrigin::LrcAutoQueue),
                ctx,
            );
        });

        terminal.update(&mut app, |view, ctx| {
            view.send_lrc_queued_prompts(conversation_id, ctx);
        });

        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert!(model.queue(conversation_id).is_empty());
        });
    });
}

fn image_attachment(file_name: &str) -> PendingAttachment {
    PendingAttachment::Image(ImageContext {
        data: String::new(),
        mime_type: "image/png".to_owned(),
        file_name: file_name.to_owned(),
        is_figma: false,
    })
}

fn query_with_attachments(text: &str, attachments: Vec<PendingAttachment>) -> QueuedQuery {
    QueuedQuery::new_with_attachments(
        text.to_owned(),
        QueuedQueryOrigin::QueueSlashCommand,
        attachments,
    )
}

/// Builds (or reuses) the queued-prompts panel for a fresh terminal view with an active
/// conversation, mirroring the panel construction `Input::new` performs when
/// [`FeatureFlag::QueueSlashCommand`] is enabled. Callers that need the flag enabled must
/// override it before calling this (see [`FeatureFlag::override_enabled`]).
fn build_panel_with_active_conversation(
    app: &mut App,
) -> (
    ViewHandle<QueuedPromptsPanelView>,
    AIConversationId,
    ViewHandle<Input>,
) {
    let terminal = add_window_with_terminal(app, None);
    let terminal_view_id = terminal.read(app, |view, _| view.view_id);
    let conversation_id = BlocklistAIHistoryModel::handle(app).update(app, |history, ctx| {
        let id = history.start_new_conversation(terminal_view_id, false, false, ctx);
        history.set_active_conversation_id(id, terminal_view_id, ctx);
        id
    });
    let input = terminal.read(app, |view, _| view.input().clone());
    if let Some(panel) = input.read(app, |input, _| input.queued_prompts_panel().cloned()) {
        return (panel, conversation_id, input);
    }
    let (suggestions_mode_model, host_editor) = input.read(app, |input, _| {
        (
            input.suggestions_mode_model().clone(),
            input.editor().clone(),
        )
    });
    let cli_subagent_controller =
        terminal.read(app, |view, _| view.cli_subagent_controller.clone());
    let panel = terminal.update(app, |_, ctx| {
        ctx.add_view(move |ctx| {
            QueuedPromptsPanelView::new(
                terminal_view_id,
                suggestions_mode_model,
                cli_subagent_controller,
                host_editor,
                ctx,
            )
        })
    });
    (panel, conversation_id, input)
}

#[test]
fn can_send_prompt_gates_buttons_and_hint_while_nonempty_input_gates_only_the_hint() {
    // When the host reports prompts cannot be sent (read-only shared-session viewer), every
    // row's send-now button is disabled and the enter hint hides. A non-empty input hides the
    // hint but leaves the buttons alone.
    App::test((), |mut app| async move {
        let _queue_flag = FeatureFlag::QueueSlashCommand.override_enabled(true);
        initialize_app_for_terminal_view(&mut app);

        let (panel, conversation_id, input) = build_panel_with_active_conversation(&mut app);
        let row_id = QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(conversation_id, user_query("send me"), ctx)
        });

        // Default: sendable, hint shown.
        panel.read(&app, |panel, ctx| {
            assert_eq!(
                panel.send_now_button_disabled_for_test(row_id, ctx),
                Some(false)
            );
            assert!(panel.enter_hint_shown_for_test(ctx));
        });

        // Sending unavailable: button disabled and hint hidden.
        panel.update(&mut app, |panel, ctx| {
            panel.set_can_send_prompt(false, ctx);
        });
        panel.read(&app, |panel, ctx| {
            assert_eq!(
                panel.send_now_button_disabled_for_test(row_id, ctx),
                Some(true)
            );
            assert!(!panel.enter_hint_shown_for_test(ctx));
        });

        // Sending available again: button re-enabled and hint restored.
        panel.update(&mut app, |panel, ctx| {
            panel.set_can_send_prompt(true, ctx);
        });
        panel.read(&app, |panel, ctx| {
            assert_eq!(
                panel.send_now_button_disabled_for_test(row_id, ctx),
                Some(false)
            );
            assert!(panel.enter_hint_shown_for_test(ctx));
        });

        // Non-empty input: hint hidden, button stays enabled. The panel reads the host
        // editor's emptiness live, so writing into the input buffer is enough.
        input.update(&mut app, |input, ctx| {
            input.replace_buffer_content("draft", ctx);
        });
        panel.read(&app, |panel, ctx| {
            assert_eq!(
                panel.send_now_button_disabled_for_test(row_id, ctx),
                Some(false)
            );
            assert!(!panel.enter_hint_shown_for_test(ctx));
        });
    });
}

#[test]
fn send_now_action_emits_row_kind_and_leaves_rows_for_host_to_fire() {
    // Clicking "send now" emits a SendNow event identifying the row and whether it is a command,
    // but leaves the row in the queue so the host can dispatch it and remove it afterward.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        // The panel keys its queue lookups on the history model's active conversation for its
        // terminal view, so seed one and build the panel as a child of that terminal view.
        let (panel, conversation_id, _) = build_panel_with_active_conversation(&mut app);

        let (prompt_id, command_id) =
            QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
                let prompt_id = model.append(conversation_id, user_query("send me now"), ctx);
                let command_id = model.append(conversation_id, command_query("echo 1"), ctx);
                (prompt_id, command_id)
            });

        let send_now_events = Rc::new(RefCell::new(Vec::<(
            AIConversationId,
            QueuedQueryId,
            String,
            bool,
        )>::new()));
        let send_now_events_for_subscription = send_now_events.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&panel, move |_, event: &QueuedPromptsPanelEvent, _| {
                if let QueuedPromptsPanelEvent::SendNow {
                    conversation_id,
                    query_id,
                    text,
                    is_command,
                } = event
                {
                    send_now_events_for_subscription.borrow_mut().push((
                        *conversation_id,
                        *query_id,
                        text.clone(),
                        *is_command,
                    ));
                }
            });
        });

        panel.update(&mut app, |panel, ctx| {
            panel.handle_action(&QueuedPromptsPanelAction::SendNow(prompt_id), ctx);
            panel.handle_action(&QueuedPromptsPanelAction::SendNow(command_id), ctx);
        });

        assert_eq!(
            send_now_events.borrow().as_slice(),
            [
                (conversation_id, prompt_id, "send me now".to_owned(), false),
                (conversation_id, command_id, "echo 1".to_owned(), true)
            ]
        );
        // The panel leaves each row in place; the host removes it after firing.
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert_eq!(model.queue(conversation_id).len(), 2);
        });
    });
}

#[test]
fn multi_cycle_queue_keeps_each_rows_attachments_independent() {
    // attach -> queue -> attach -> queue: each row owns its own attachments, and draining one
    // never disturbs the other's.
    with_singleton(|mut app, model, conv| {
        let first_id = model.update(&mut app, |m, ctx| {
            m.append(
                conv,
                query_with_attachments("first", vec![image_attachment("first.png")]),
                ctx,
            )
        });
        let second_id = model.update(&mut app, |m, ctx| {
            m.append(
                conv,
                query_with_attachments("second", vec![image_attachment("second.png")]),
                ctx,
            )
        });

        model.read(&app, |m, _| {
            assert_eq!(
                m.attachments_for(conv, first_id)[0].file_name(),
                "first.png"
            );
            assert_eq!(
                m.attachments_for(conv, second_id)[0].file_name(),
                "second.png"
            );
        });

        // Drain the first row; the second row's attachments are untouched.
        let action = drain_one(&model, &mut app, conv);
        match action {
            Some(AutofireAction::Submit { text, .. }) => assert_eq!(text, "first"),
            other => panic!("expected Submit, got {other:?}"),
        }
        model.read(&app, |m, _| {
            assert!(m.attachments_for(conv, first_id).is_empty());
            assert_eq!(m.attachments_for(conv, second_id).len(), 1);
            assert_eq!(
                m.attachments_for(conv, second_id)[0].file_name(),
                "second.png"
            );
        });
    });
}

#[test]
fn redetermine_terminal_focus_preserves_focused_queued_prompt_editor() {
    App::test((), |mut app| async move {
        let _queue_flag = FeatureFlag::QueueSlashCommand.override_enabled(true);
        initialize_app_for_terminal_view(&mut app);

        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.view_id);
        let conversation_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
                let id = history.start_new_conversation(terminal_view_id, false, false, ctx);
                history.set_active_conversation_id(id, terminal_view_id, ctx);
                id
            });
        let input = terminal.read(&app, |view, _| view.input().clone());
        let panel = input
            .read(&app, |input, _| input.queued_prompts_panel().cloned())
            .expect("queue flag should create a queued prompts panel");
        let row_id = QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(conversation_id, user_query("edit me"), ctx)
        });

        panel.update(&mut app, |panel, ctx| {
            panel.handle_action(&QueuedPromptsPanelAction::StartEditingRow(row_id), ctx);
        });
        panel.read(&app, |panel, ctx| {
            assert!(panel.is_inline_edit_editor_focused(ctx));
        });

        terminal.update(&mut app, |view, ctx| {
            assert!(
                !view.redetermine_terminal_focus(ctx),
                "focused queued-prompt edits should hold focus during async focus reconciliation"
            );
        });

        panel.read(&app, |panel, ctx| {
            assert!(panel.is_inline_edit_editor_focused(ctx));
        });
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert_eq!(model.editing_row(conversation_id), Some(row_id));
        });
    });
}

#[test]
fn commit_edit_saves_current_editor_text_for_lrc_row() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _queue_flag = FeatureFlag::QueueSlashCommand.override_enabled(true);

        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.view_id);
        let conversation_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
                let id = history.start_new_conversation(terminal_view_id, false, false, ctx);
                history.set_active_conversation_id(id, terminal_view_id, ctx);
                id
            });
        let query_id = QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(
                conversation_id,
                QueuedQuery::new(
                    "stale committed".to_owned(),
                    QueuedQueryOrigin::LrcAutoQueue,
                ),
                ctx,
            )
        });
        QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.enter_edit_mode(conversation_id, query_id, ctx);
        });

        let queued_prompts_panel = terminal.read(&app, |view, ctx| {
            view.input()
                .as_ref(ctx)
                .queued_prompts_panel()
                .cloned()
                .expect("queue panel should exist")
        });
        queued_prompts_panel.update(&mut app, |panel, ctx| {
            panel.set_edit_buffer_text_for_test("edited before finish", ctx);
            panel.commit_edit(ctx);
        });
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            let queue = model.queue(conversation_id);
            assert_eq!(queue.len(), 1);
            assert_eq!(queue[0].id(), query_id);
            assert_eq!(queue[0].text(), "edited before finish");
            assert_eq!(model.editing_row(conversation_id), None);
        });
    });
}

#[test]
fn lrc_finish_commits_edited_lrc_row_before_sending() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        // The prompt-submission path (`send_queued_user_query_in_conversation`) reads the global
        // model-event sender, so the provider must be registered for delivery to run.
        let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(move |_| {
            crate::GlobalResourceHandlesProvider::new(global_resource_handles.clone())
        });
        let _queue_flag = FeatureFlag::QueueSlashCommand.override_enabled(true);

        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.view_id);
        let conversation_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
                let id = history.start_new_conversation(terminal_view_id, false, false, ctx);
                history.set_active_conversation_id(id, terminal_view_id, ctx);
                id
            });
        let query_id = QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(
                conversation_id,
                QueuedQuery::new(
                    "stale committed".to_owned(),
                    QueuedQueryOrigin::LrcAutoQueue,
                ),
                ctx,
            )
        });
        QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.enter_edit_mode(conversation_id, query_id, ctx);
        });

        let queued_prompts_panel = terminal.read(&app, |view, ctx| {
            view.input()
                .as_ref(ctx)
                .queued_prompts_panel()
                .cloned()
                .expect("queue panel should exist")
        });
        queued_prompts_panel.update(&mut app, |panel, ctx| {
            panel.set_edit_buffer_text_for_test("edited before finish", ctx);
        });

        let edit_commit_count = Rc::new(RefCell::new(0));
        let edit_commit_count_for_subscription = edit_commit_count.clone();
        app.update(|ctx| {
            ctx.subscribe_to_model(
                &QueuedQueryModel::handle(ctx),
                move |_, event: &crate::ai::blocklist::QueuedQueryEvent, _| {
                    if matches!(
                        event,
                        crate::ai::blocklist::QueuedQueryEvent::EditCommitted { .. }
                    ) {
                        *edit_commit_count_for_subscription.borrow_mut() += 1;
                    }
                },
            );
        });

        let ai_query_count = Rc::new(RefCell::new(0));
        let input = terminal.read(&app, |view, _| view.input().clone());
        let ai_query_count_for_subscription = ai_query_count.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&input, move |_, event: &InputEvent, _| {
                if matches!(event, InputEvent::ExecuteAIQuery) {
                    *ai_query_count_for_subscription.borrow_mut() += 1;
                }
            });
        });

        terminal.update(&mut app, |view, ctx| {
            view.send_lrc_queued_prompts(conversation_id, ctx);
        });

        assert_eq!(*edit_commit_count.borrow(), 1);
        assert_eq!(*ai_query_count.borrow(), 1);
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert!(model.queue(conversation_id).is_empty());
        });
    });
}

#[test]
fn lrc_finish_queued_compact_and_sends_followup_after_summary() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        // The prompt-submission path (`send_queued_user_query_in_conversation`) reads the global
        // model-event sender, so the provider must be registered for delivery to run.
        let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(move |_| {
            crate::GlobalResourceHandlesProvider::new(global_resource_handles.clone())
        });
        let _agent_view = FeatureFlag::AgentView.override_enabled(true);
        let _queue_flag = FeatureFlag::QueueSlashCommand.override_enabled(true);
        let _queued_prompts_v2 = FeatureFlag::QueuedPromptsV2.override_enabled(true);
        let _summarization = FeatureFlag::SummarizationConversationCommand.override_enabled(true);

        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.read(&app, |view, _| view.view_id);
        let conversation_id =
            BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
                let id = history.start_new_conversation(terminal_view_id, false, false, ctx);
                history.set_active_conversation_id(id, terminal_view_id, ctx);
                id
            });
        terminal.read(&app, |view, ctx| {
            assert_eq!(
                view.ai_context_model()
                    .as_ref(ctx)
                    .selected_conversation_id(ctx),
                None
            );
        });

        QueuedQueryModel::handle(&app).update(&mut app, |model, ctx| {
            model.append(
                conversation_id,
                QueuedQuery::new_with_attachments(
                    format!("{} follow up", commands::COMPACT_AND.name),
                    QueuedQueryOrigin::LrcAutoQueue,
                    vec![image_attachment("queued-context.png")],
                ),
                ctx,
            );
        });

        terminal.update(&mut app, |view, ctx| {
            view.send_lrc_queued_prompts(conversation_id, ctx);
        });

        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            let queue = model.queue(conversation_id);
            assert_eq!(queue.len(), 1);
            assert_eq!(queue[0].text(), "follow up");
            assert_eq!(queue[0].origin(), QueuedQueryOrigin::CompactAndSlashCommand);
            assert_eq!(queue[0].attachments().len(), 1);
        });

        let ai_query_count = Rc::new(RefCell::new(0));
        let input = terminal.read(&app, |view, _| view.input().clone());
        let ai_query_count_for_subscription = ai_query_count.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&input, move |_, event: &InputEvent, _| {
                if matches!(event, InputEvent::ExecuteAIQuery) {
                    *ai_query_count_for_subscription.borrow_mut() += 1;
                }
            });
        });

        terminal.update(&mut app, |view, ctx| {
            view.drain_queued_prompts(conversation_id, FinishReason::Complete, ctx);
        });

        assert_eq!(*ai_query_count.borrow(), 1);
        QueuedQueryModel::handle(&app).read(&app, |model, _| {
            assert!(model.queue(conversation_id).is_empty());
        });
    });
}

#[test]
fn finish_reason_is_scoped_to_the_finished_conversation() {
    // An orchestration pane hosts the lead and local child conversations in one view, so the
    // most recent block in the pane can belong to a sibling conversation that is still
    // mid-turn. The per-conversation lookup must report the finished conversation's own block
    // as Complete (so its queued prompts drain) and the streaming sibling's as unfinished.
    // Refs #365.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            let finished_block =
                view.insert_dummy_ai_block("review".to_owned(), "done".to_owned(), ctx);
            let finished_conversation = finished_block.as_ref(ctx).conversation_id();
            // Inserted after the finished block, so it is the last block in the pane and
            // masks the pane-global `active_ai_block` / `last_ai_block` lookups.
            let streaming_block = view.insert_dummy_streaming_ai_block("working".to_owned(), ctx);
            let streaming_conversation = streaming_block.as_ref(ctx).conversation_id();
            assert_ne!(finished_conversation, streaming_conversation);

            assert_eq!(
                view.finish_reason_for_conversation(finished_conversation, ctx),
                Some(FinishReason::Complete)
            );
            assert_eq!(
                view.finish_reason_for_conversation(streaming_conversation, ctx),
                None
            );
            // A conversation with no blocks in this pane has no finish reason.
            assert_eq!(
                view.finish_reason_for_conversation(AIConversationId::new(), ctx),
                None
            );
        });
    });
}

#[test]
fn finished_receiving_output_drains_queue_when_sibling_block_masks_turn_end() {
    // End-to-end through the controller-event path: `FinishedReceivingOutput` for a finished
    // conversation must drain that conversation's queue even when a sibling conversation's
    // still-streaming block is the most recent block in the pane (orchestration panes host the
    // lead and local child conversations in one view). This is the regression this fork had:
    // the pane-global `last_ai_block()` lookup would see the streaming sibling's block, read
    // its `finish_reason()` as `None`, and silently skip the drain. Refs #365.
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            let finished_block =
                view.insert_dummy_ai_block("review".to_owned(), "done".to_owned(), ctx);
            let finished_conversation = finished_block.as_ref(ctx).conversation_id();
            // Inserted after the finished block, so it is the last block in the pane and
            // masks the pane-global `active_ai_block` / `last_ai_block` lookups.
            let streaming_block = view.insert_dummy_streaming_ai_block("working".to_owned(), ctx);
            let streaming_conversation = streaming_block.as_ref(ctx).conversation_id();
            assert_ne!(finished_conversation, streaming_conversation);

            QueuedQueryModel::handle(ctx).update(ctx, |model, ctx| {
                model.append(finished_conversation, user_query("queued follow up"), ctx);
                model.append(
                    streaming_conversation,
                    user_query("sibling stays queued"),
                    ctx,
                );
            });

            view.handle_ai_controller_event(
                view.ai_controller.clone(),
                &BlocklistAIControllerEvent::FinishedReceivingOutput {
                    stream_id: ResponseStreamId::new_for_test(),
                    conversation_id: finished_conversation,
                },
                ctx,
            );

            // The finished conversation's queued prompt fired; the still-streaming sibling's
            // queue is untouched.
            let model = QueuedQueryModel::as_ref(ctx);
            assert!(model.queue(finished_conversation).is_empty());
            assert_eq!(model.queue(streaming_conversation).len(), 1);
            assert_eq!(
                model.queue(streaming_conversation)[0].text(),
                "sibling stays queued"
            );
        });
    });
}
