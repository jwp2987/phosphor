use std::collections::HashSet;

use warpui::App;

use super::*;
use crate::ai::agent::conversation::AIConversationId;
use crate::persistence::ModelEvent;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::{GlobalResourceHandles, GlobalResourceHandlesProvider};

#[test]
fn toggle_pin_in_set_flips_membership_for_each_call() {
    let mut pinned: HashSet<AIConversationId> = HashSet::new();
    let id = AIConversationId::new();

    // Initially empty.
    assert!(!pinned.contains(&id));

    // First toggle pins the id and reports the new state.
    assert!(toggle_pin_in_set(&mut pinned, id));
    assert!(pinned.contains(&id));
    assert_eq!(pinned.len(), 1);

    // Second toggle unpins it.
    assert!(!toggle_pin_in_set(&mut pinned, id));
    assert!(!pinned.contains(&id));
    assert!(pinned.is_empty());

    // Third toggle re-pins, confirming the function is idempotent under
    // pairs and doesn't accumulate state.
    assert!(toggle_pin_in_set(&mut pinned, id));
    assert!(pinned.contains(&id));
}

#[test]
fn toggle_pin_in_set_only_affects_target_id() {
    let mut pinned: HashSet<AIConversationId> = HashSet::new();
    let a = AIConversationId::new();
    let b = AIConversationId::new();

    toggle_pin_in_set(&mut pinned, a);
    toggle_pin_in_set(&mut pinned, b);
    assert!(pinned.contains(&a));
    assert!(pinned.contains(&b));

    // Unpinning `a` leaves `b` alone.
    toggle_pin_in_set(&mut pinned, a);
    assert!(!pinned.contains(&a));
    assert!(pinned.contains(&b));
}

/// End-to-end check that toggling a pin (a) updates the in-memory
/// `AIConversation.pinned` flag on the history model and (b) emits an
/// `UpdateMultiAgentConversation` SQLite event carrying `pinned: true`,
/// which is what makes the pin survive an app restart.
#[test]
fn toggle_pin_persists_pinned_state_to_sqlite_event() {
    App::test((), |mut app| async move {
        // `write_updated_conversation_state` reads `GeneralSettings`,
        // `AppExecutionMode`, and the global model_event_sender; wire all
        // three up so the SQLite write path actually runs.
        initialize_settings_for_tests(&mut app);
        let (sender, receiver) = std::sync::mpsc::sync_channel::<ModelEvent>(4);
        let mut global_resource_handles = GlobalResourceHandles::mock(&mut app);
        global_resource_handles.model_event_sender = Some(sender);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        // Restore a conversation that starts unpinned. The history model is
        // the source of truth for the persisted flag.
        let terminal_view_id = warpui::EntityId::new();
        let conversation_id = history_model.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        // Pin model must be registered after the history model because it
        // subscribes to history events on construction.
        let pin_model =
            app.add_singleton_model(|ctx| OrchestrationPillBarModel::new(HashSet::new(), ctx));

        pin_model.update(&mut app, |model, ctx| {
            model.toggle_pin(conversation_id, ctx);
        });

        // In-memory pin set reflects the toggle.
        pin_model.read(&app, |model, _| {
            assert!(
                model.is_pinned(&conversation_id),
                "pin set should contain the conversation after toggle"
            );
        });

        // The conversation's persisted `pinned` flag is now true.
        history_model.read(&app, |model, _| {
            let pinned = model.conversation(&conversation_id).map(|c| c.is_pinned());
            assert_eq!(
                pinned,
                Some(true),
                "AIConversation.is_pinned() should reflect the toggle"
            );
        });

        // The SQLite writer should have received an UpdateMultiAgentConversation
        // event with pinned=true. The fire-and-forget send happens on the
        // executor, so allow a short timeout.
        let event = receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("expected an UpdateMultiAgentConversation event after toggle_pin");
        match event {
            ModelEvent::UpdateMultiAgentConversation {
                conversation_id: persisted_id,
                conversation_data,
                ..
            } => {
                assert_eq!(persisted_id, conversation_id.to_string());
                assert!(
                    conversation_data.pinned,
                    "persisted AgentConversationData.pinned should be true"
                );
            }
            other => panic!("expected UpdateMultiAgentConversation, got {other:?}"),
        }

        // Toggling again should write pinned=false through the same path.
        pin_model.update(&mut app, |model, ctx| {
            model.toggle_pin(conversation_id, ctx);
        });
        let event = receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("expected a second UpdateMultiAgentConversation event after unpin");
        match event {
            ModelEvent::UpdateMultiAgentConversation {
                conversation_data, ..
            } => {
                assert!(
                    !conversation_data.pinned,
                    "persisted AgentConversationData.pinned should be false after unpin"
                );
            }
            other => panic!("expected UpdateMultiAgentConversation, got {other:?}"),
        }
    });
}

/// `reset()` has no production caller in this fork (see its doc comment —
/// Phosphor's BYOP auth has no multi-account logout that clears AI state),
/// so this test is what keeps it from being a dead symbol. Confirms it
/// clears both the pinned set and the scroll-handle cache.
#[test]
fn reset_clears_pinned_set_and_scroll_handles() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let global_resources = GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resources));

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let terminal_view_id = warpui::EntityId::new();
        let conversation_id = history_model.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let pin_model =
            app.add_singleton_model(|ctx| OrchestrationPillBarModel::new(HashSet::new(), ctx));

        pin_model.update(&mut app, |model, ctx| {
            model.toggle_pin(conversation_id, ctx);
            let _ = model.horizontal_scroll_state_for(conversation_id);
        });
        pin_model.read(&app, |model, _| {
            assert!(model.is_pinned(&conversation_id));
            assert_eq!(model.horizontal_scroll_states.borrow().len(), 1);
        });

        pin_model.update(&mut app, |model, _| {
            model.reset();
        });

        pin_model.read(&app, |model, _| {
            assert!(
                !model.is_pinned(&conversation_id),
                "reset should clear the pinned set"
            );
            assert!(
                model.horizontal_scroll_states.borrow().is_empty(),
                "reset should clear the scroll-handle cache"
            );
        });
    });
}

/// When a conversation is removed from the history model, the pill bar
/// singleton should prune it from the pinned set and drop its scroll
/// handle, so deleted orchestrators don't leak entries.
#[test]
fn removed_conversation_is_pruned_from_pin_set_and_scroll_states() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let global_resources = GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resources));

        let history_model = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let terminal_view_id = warpui::EntityId::new();
        let conversation_id = history_model.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let pin_model =
            app.add_singleton_model(|ctx| OrchestrationPillBarModel::new(HashSet::new(), ctx));

        pin_model.update(&mut app, |model, ctx| {
            model.toggle_pin(conversation_id, ctx);
            let _ = model.horizontal_scroll_state_for(conversation_id);
        });
        pin_model.read(&app, |model, _| {
            assert!(model.is_pinned(&conversation_id));
        });

        history_model.update(&mut app, |history, ctx| {
            history.remove_conversation(conversation_id, terminal_view_id, ctx);
        });

        pin_model.read(&app, |model, _| {
            assert!(
                !model.is_pinned(&conversation_id),
                "removed conversation should be pruned from the pinned set"
            );
            assert!(
                model.horizontal_scroll_states.borrow().is_empty(),
                "removed conversation's scroll handle should be dropped"
            );
        });
    });
}
