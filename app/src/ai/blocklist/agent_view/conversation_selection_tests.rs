//! Ported from the pin (`app/src/ai/blocklist/agent_view/conversation_selection_tests.rs`,
//! `02b53fcd8`) for #316, adapted to `classify_gui_list_entry`'s narrowed 4-arg signature (no
//! `has_open_action` closure -- see `conversation_selection.rs`'s module doc for why) and to this
//! fork's `AgentViewController::new`, which additionally takes an `AmbientAgentViewModel` handle
//! (construction pattern taken from `context_model_test.rs`, which already builds one of these
//! for a different test). `gui_list_policy_classifies_unavailable_entry` isn't ported: the
//! adapted `classify_gui_list_entry` can no longer return `Unavailable` at all.

use std::sync::Arc;

use parking_lot::FairMutex;
use warpui::r#async::executor::Background;
use warpui::{App, EntityId, ModelHandle};

use super::{classify_gui_list_entry, AgentViewConversationSelection};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::agent_view::{
    AgentViewController, AgentViewEntryOrigin, EphemeralMessageModel,
};
use crate::ai::blocklist::conversation_selection::ConversationSelection;
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::ai::conversation_entry::{AgentConversationEntryId, AgentConversationListEntryState};
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::cloud_object::update_manager::UpdateManager;
use crate::terminal::color::{self, Colors};
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::test_utils::block_size;
use crate::terminal::model::TerminalModel;
use crate::terminal::view::ambient_agent::AmbientAgentViewModel;
use crate::test_util::settings::initialize_settings_for_tests;

#[test]
fn gui_list_policy_classifies_selected_entry() {
    let entry_id = AgentConversationEntryId::Conversation(AIConversationId::new());
    assert_eq!(
        classify_gui_list_entry(Some(entry_id), entry_id, Some(EntityId::new()), EntityId::new()),
        AgentConversationListEntryState::Selected
    );
}

#[test]
fn gui_list_policy_classifies_entry_open_elsewhere() {
    let entry_id = AgentConversationEntryId::Conversation(AIConversationId::new());
    assert_eq!(
        classify_gui_list_entry(None, entry_id, Some(EntityId::new()), EntityId::new()),
        AgentConversationListEntryState::OpenElsewhere
    );
}

#[test]
fn gui_list_policy_classifies_available_entry() {
    let entry_id = AgentConversationEntryId::Conversation(AIConversationId::new());
    // Not selected, and not open in a different terminal view (open in this same one, or not
    // open anywhere): Available.
    let this_view = EntityId::new();
    assert_eq!(
        classify_gui_list_entry(None, entry_id, Some(this_view), this_view),
        AgentConversationListEntryState::Available
    );
    assert_eq!(
        classify_gui_list_entry(None, entry_id, None, this_view),
        AgentConversationListEntryState::Available
    );
}

/// Builds a real [`AgentViewController`] for a terminal view, mirroring
/// `context_model_test.rs`'s `build_test_context_model` fixture setup.
fn build_test_agent_view_controller(
    app: &mut App,
    terminal_view_id: EntityId,
) -> ModelHandle<AgentViewController> {
    let terminal_model = Arc::new(FairMutex::new(TerminalModel::new_for_test(
        block_size(),
        color::List::from(&Colors::default()),
        ChannelEventListener::new_for_test(),
        Arc::new(Background::default()),
        false, /* should_show_bootstrap_block */
        None,  /* restored_blocks */
        false, /* honor_ps1 */
        false, /* is_inverted */
        None,  /* session_startup_path */
    )));

    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(UpdateManager::mock);

    let ambient_agent_view_model =
        app.add_model(|ctx| AmbientAgentViewModel::new(terminal_view_id, false, ctx));
    let ephemeral_message_model = app.add_model(|_| EphemeralMessageModel::new());
    app.add_model(|ctx| {
        AgentViewController::new(
            terminal_model,
            terminal_view_id,
            ambient_agent_view_model,
            ephemeral_message_model,
            ctx,
        )
    })
}

#[test]
fn gui_selection_delegates_to_agent_view() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let terminal_view_id = EntityId::new();
        let agent_view_controller = build_test_agent_view_controller(&mut app, terminal_view_id);
        let selection = app.add_model(|ctx| {
            Box::new(AgentViewConversationSelection::new(
                terminal_view_id,
                agent_view_controller.clone(),
                ctx,
            )) as Box<dyn ConversationSelection>
        });
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        selection.update(&mut app, |selection, ctx| {
            selection.select_existing_conversation(
                conversation_id,
                AgentViewEntryOrigin::ConversationSelector,
                ctx,
            );
        });

        selection.read(&app, |selection, ctx| {
            assert_eq!(
                selection.selected_conversation_id(ctx),
                Some(conversation_id)
            );
        });
        agent_view_controller.read(&app, |controller, _| {
            assert!(controller.is_active());
        });
    });
}
