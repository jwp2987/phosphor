use std::cell::RefCell;
use std::rc::Rc;

use warp_core::features::FeatureFlag;
use warpui::{App, EntityId, SingletonEntity};

use super::BlocklistAIStatusBarEvent;
use crate::ai::agent::conversation::{AIConversation, AIConversationId};
use crate::ai::blocklist::{BlocklistAIHistoryEvent, BlocklistAIHistoryModel};
use crate::test_util::ai_agent_tasks::{create_api_task, create_message};
use crate::test_util::terminal::{add_window_with_terminal, initialize_app_for_terminal_view};

/// Ported from upstream eafc6d18d: the status bar used to register a new history subscription each
/// time its active exchange changed, so an exchange that was no longer active still processed
/// (and closed the summarization dialog on) every later output event.
#[test]
fn inactive_exchange_updates_do_not_close_summarization_dialog() {
    App::test((), |mut app| async move {
        let _tips = FeatureFlag::AgentTips.override_enabled(false);
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let terminal_view_id = terminal.id();
        let status_bar = terminal.read(&app, |terminal, ctx| {
            terminal.input().as_ref(ctx).agent_status_bar().clone()
        });
        let history = app.read(BlocklistAIHistoryModel::handle);
        let conversation = AIConversation::new_restored(
            AIConversationId::new(),
            vec![create_api_task(
                "root",
                (0..2)
                    .map(|index| {
                        let mut message = create_message(&format!("message-{index}"), "root");
                        message.request_id = format!("request-{index}");
                        message
                    })
                    .collect(),
            )],
            None,
        )
        .unwrap();
        let conversation_id = conversation.id();
        let task_id = conversation.get_root_task_id().clone();
        let exchange_ids: Vec<_> = conversation
            .all_exchanges()
            .iter()
            .map(|exchange| exchange.id)
            .collect();
        assert_eq!(exchange_ids.len(), 2);
        history.update(&mut app, |history, ctx| {
            history.restore_conversations(terminal_view_id, vec![conversation], ctx);
        });
        for exchange_id in &exchange_ids {
            history.update(&mut app, |_, ctx| {
                ctx.emit(BlocklistAIHistoryEvent::AppendedExchange {
                    exchange_id: *exchange_id,
                    task_id: task_id.clone(),
                    terminal_view_id,
                    conversation_id,
                    is_hidden: false,
                    response_stream_id: None,
                });
            });
        }
        let dialog_events = Rc::new(RefCell::new(Vec::new()));
        let observed_dialog_events = dialog_events.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&status_bar, move |_, event, _| {
                if let BlocklistAIStatusBarEvent::SummarizationCancelDialogToggled { is_open } =
                    event
                {
                    observed_dialog_events.borrow_mut().push(*is_open);
                }
            });
        });
        status_bar.update(&mut app, |bar, ctx| {
            bar.open_summarization_cancel_dialog(ctx);
        });

        // A stale (no longer active) exchange must not touch the dialog.
        history.update(&mut app, |_, ctx| {
            ctx.emit(BlocklistAIHistoryEvent::UpdatedStreamingExchange {
                exchange_id: exchange_ids[0],
                terminal_view_id,
                conversation_id,
                is_hidden: false,
            });
        });
        assert_eq!(*dialog_events.borrow(), vec![true]);

        // The active exchange still processes output.
        history.update(&mut app, |_, ctx| {
            ctx.emit(BlocklistAIHistoryEvent::UpdatedStreamingExchange {
                exchange_id: exchange_ids[1],
                terminal_view_id: EntityId::new(),
                conversation_id,
                is_hidden: false,
            });
        });
        assert_eq!(*dialog_events.borrow(), vec![true, false]);
    });
}

/// #1962: the first character typed into an empty shell-mode input used to re-render the status
/// bar, whose `render` reads nothing of the buffer. It is skipped only when the bar drew nothing
/// and would draw nothing.
#[test]
fn an_empty_state_change_does_not_rerender_a_bar_that_draws_nothing() {
    use super::empty_state_change_needs_render;
    assert!(!empty_state_change_needs_render(true, true, true));
}

#[test]
fn an_empty_state_change_rerenders_a_bar_that_drew_or_would_draw_something() {
    use super::empty_state_change_needs_render;
    // An agent turn started since the last render (the warping indicator is due).
    assert!(empty_state_change_needs_render(true, false, true));
    // The last render drew something (or its outcome is not known to be empty).
    assert!(empty_state_change_needs_render(false, true, true));
    assert!(empty_state_change_needs_render(false, false, true));
    // Kill switch: every change renders.
    assert!(empty_state_change_needs_render(true, true, false));
}

/// The state the skip rests on, on a real terminal: with no agent view, no tagged-in block and no
/// exchange in progress, `render` is known to return an empty element.
#[test]
fn a_terminal_mode_status_bar_is_known_to_draw_nothing() {
    App::test((), |mut app| async move {
        let _tips = FeatureFlag::AgentTips.override_enabled(false);
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let status_bar = terminal.read(&app, |terminal, ctx| {
            terminal.input().as_ref(ctx).agent_status_bar().clone()
        });
        status_bar.read(&app, |bar, ctx| {
            let terminal_model = bar.terminal_model.lock();
            assert!(bar.renders_nothing(&terminal_model, ctx));
        });
    });
}
