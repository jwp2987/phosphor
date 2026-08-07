//! Unit tests for [`BlocklistAIContextModel::has_locking_attachment`].
//!
//! These tests deliberately bypass the production [`BlocklistAIContextModel::new`] constructor
//! (which subscribes to several singletons) and instead use [`BlocklistAIContextModel::new_for_test`]
//! together with [`super::agent_view::AgentViewController::new`]. That keeps the fixture small
//! enough to focus on the lock logic without standing up `BlocklistAIHistoryModel`,
//! `LLMPreferences`, `ObjectStoreModel`, `UpdateManager`, or `AppExecutionMode`.

use std::sync::Arc;

use parking_lot::FairMutex;
use warpui::r#async::executor::Background;
use warpui::{App, EntityId, ModelHandle};

use super::{BlocklistAIContextModel, PendingAttachment, PendingFile};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::{AIAgentAttachment, ImageContext};
use crate::ai::blocklist::agent_view::{
    AgentViewController, AgentViewEntryOrigin, EphemeralMessageModel,
};
use crate::ai::blocklist::{
    BlocklistAIHistoryModel, QueuedQuery, QueuedQueryModel, QueuedQueryOrigin,
};
use crate::global_resource_handles::{GlobalResourceHandles, GlobalResourceHandlesProvider};
use crate::test_util::settings::initialize_settings_for_tests;
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::cloud_object::update_manager::UpdateManager;
use crate::terminal::color::{self, Colors};
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::test_utils::block_size;
use crate::terminal::model::{BlockId, TerminalModel};
use crate::terminal::view::ambient_agent::AmbientAgentViewModel;

impl BlocklistAIContextModel {
    pub(crate) fn append_pending_attachments_for_test(
        &mut self,
        attachments: Vec<PendingAttachment>,
    ) {
        self.pending_attachments.extend(attachments);
    }

    pub(crate) fn insert_pending_block_id_for_test(&mut self, block_id: BlockId) {
        self.pending_context_block_ids.insert(block_id);
    }

    pub(crate) fn set_pending_selected_text_for_test(&mut self, text: Option<String>) {
        self.pending_context_selected_text = text;
    }
}

/// Builds a [`BlocklistAIContextModel`] with stub dependencies. None of the dependencies are
/// exercised by the methods under test; they only need to satisfy the struct's field types.
fn build_test_context_model(app: &mut App) -> ModelHandle<BlocklistAIContextModel> {
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
    let terminal_view_id = EntityId::new();

    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(UpdateManager::mock);

    let ambient_agent_view_model =
        app.add_model(|ctx| AmbientAgentViewModel::new(terminal_view_id, false, ctx));
    let ephemeral_message_model = app.add_model(|_| EphemeralMessageModel::new());
    let agent_view_controller = app.add_model(|ctx| {
        AgentViewController::new(
            terminal_model.clone(),
            terminal_view_id,
            ambient_agent_view_model,
            ephemeral_message_model,
            ctx,
        )
    });

    app.add_model(|_| {
        BlocklistAIContextModel::new_for_test(
            terminal_model,
            terminal_view_id,
            agent_view_controller,
        )
    })
}

fn make_image_attachment(file_name: &str) -> PendingAttachment {
    PendingAttachment::Image(ImageContext {
        data: String::new(),
        mime_type: "image/png".to_owned(),
        file_name: file_name.to_owned(),
        is_figma: false,
    })
}

fn make_file_attachment(file_name: &str) -> PendingAttachment {
    PendingAttachment::File(PendingFile {
        file_name: file_name.to_owned(),
        file_path: file_name.into(),
        mime_type: "text/plain".to_owned(),
    })
}

#[test]
fn has_locking_attachment_is_false_for_default_state() {
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.read(&app, |m, _| {
            assert!(!m.has_locking_attachment());
        });
    });
}

#[test]
fn has_locking_attachment_is_true_with_pending_block_id() {
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.update(&mut app, |m, _| {
            m.insert_pending_block_id_for_test(BlockId::new());
        });

        model.read(&app, |m, _| assert!(m.has_locking_attachment()));
    });
}

#[test]
fn has_locking_attachment_is_false_with_only_pending_selected_text() {
    // Selected text alone is *not* a locking attachment: the user could be selecting shell
    // command text (e.g. to copy a previously-run command), and forcing the input into AI
    // mode in that case would be wrong. Only images, files, or blocks should force the lock.
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.update(&mut app, |m, _| {
            m.set_pending_selected_text_for_test(Some("hello".to_owned()));
        });

        model.read(&app, |m, _| assert!(!m.has_locking_attachment()));
    });
}

#[test]
fn has_locking_attachment_is_true_with_pending_image_attachment() {
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.update(&mut app, |m, _| {
            m.append_pending_attachments_for_test(vec![make_image_attachment("a.png")]);
        });

        model.read(&app, |m, _| assert!(m.has_locking_attachment()));
    });
}

#[test]
fn has_locking_attachment_is_true_with_only_file_attachments() {
    // File attachments are locking attachments — the user has explicitly attached a file as
    // context, which is unambiguously a signal that the next query is intended for the agent.
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.update(&mut app, |m, _| {
            m.append_pending_attachments_for_test(vec![
                make_file_attachment("notes.txt"),
                make_file_attachment("readme.md"),
            ]);
        });

        model.read(&app, |m, _| assert!(m.has_locking_attachment()));
    });
}

#[test]
fn has_locking_attachment_is_true_with_mixed_image_and_file_attachments() {
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.update(&mut app, |m, _| {
            m.append_pending_attachments_for_test(vec![
                make_file_attachment("notes.txt"),
                make_image_attachment("a.png"),
            ]);
        });

        model.read(&app, |m, _| assert!(m.has_locking_attachment()));
    });
}

#[test]
fn referenced_at_context_attachments_prefers_longest_visible_reference() {
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.update(&mut app, |m, _| {
            m.register_at_context_attachment(
                "@commit".to_owned(),
                AIAgentAttachment::PlainText("old".to_owned()),
            );
            m.register_at_context_attachment(
                "@commit (4)".to_owned(),
                AIAgentAttachment::PlainText("new".to_owned()),
            );
        });

        model.read(&app, |m, _| {
            let attachments = m.referenced_at_context_attachments("@commit (4) hi");
            assert_eq!(attachments.len(), 1);
            assert_eq!(
                attachments.get("@commit (4)"),
                Some(&AIAgentAttachment::PlainText("new".to_owned()))
            );
        });
    });
}

#[test]
fn retain_at_context_attachments_in_query_drops_deleted_prefix_reference() {
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);

        model.update(&mut app, |m, _| {
            m.register_at_context_attachment(
                "@commit".to_owned(),
                AIAgentAttachment::PlainText("old".to_owned()),
            );
            m.register_at_context_attachment(
                "@commit (4)".to_owned(),
                AIAgentAttachment::PlainText("new".to_owned()),
            );
            m.retain_at_context_attachments_in_query("@commit (4) hi");
        });

        model.read(&app, |m, _| {
            assert!(!m.pending_at_context_attachments().contains_key("@commit"));
            assert!(m
                .pending_at_context_attachments()
                .contains_key("@commit (4)"));
        });
    });
}

#[test]
fn take_pending_attachments_drains_and_returns_all_staged() {
    App::test((), |mut app| async move {
        let model = build_test_context_model(&mut app);
        model.update(&mut app, |m, _| {
            m.append_pending_attachments_for_test(vec![
                make_image_attachment("a.png"),
                make_file_attachment("notes.txt"),
            ]);
        });

        let taken = model.update(&mut app, |m, ctx| m.take_pending_attachments(ctx));
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].file_name(), "a.png");
        assert_eq!(taken[1].file_name(), "notes.txt");

        // Draining clears the live staging so the input's attachment chips disappear.
        model.read(&app, |m, _| assert!(m.pending_attachments().is_empty()));
    });
}

#[test]
fn enqueue_moves_staged_attachments_onto_the_row_and_clears_input() {
    // Mirrors the enqueue sites in `input.rs`: `take_pending_attachments` drains the live input
    // staging and the drained set is stored on the queued row via `new_with_attachments`, leaving
    // no attachments behind in the input.
    App::test((), |mut app| async move {
        // `QueuedQueryModel::new` subscribes to the `BlocklistAIHistoryModel` singleton, which
        // the lock-logic fixture deliberately does not stand up. Register it (and the settings /
        // global-resource-handle singletons it needs) explicitly here rather than relying on
        // another test having run first. Mirrors `queued_query_tests.rs::with_model`.
        initialize_settings_for_tests(&mut app);
        let global_resource_handles = GlobalResourceHandles::mock(&mut app);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));
        app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

        let model = build_test_context_model(&mut app);
        let queued = app.add_singleton_model(QueuedQueryModel::new);
        let conv = AIConversationId::new();

        model.update(&mut app, |m, _| {
            m.append_pending_attachments_for_test(vec![
                make_image_attachment("a.png"),
                make_file_attachment("notes.txt"),
            ]);
        });

        // Capture-and-clear, then store on the row (the exact composition used at enqueue time).
        let taken = model.update(&mut app, |m, ctx| m.take_pending_attachments(ctx));
        let id = queued.update(&mut app, |q, ctx| {
            q.append(
                conv,
                QueuedQuery::new_with_attachments(
                    "queued".to_owned(),
                    QueuedQueryOrigin::AutoQueueToggle,
                    taken,
                ),
                ctx,
            )
        });

        // Live staging is cleared; the row owns the attachments.
        model.read(&app, |m, _| assert!(m.pending_attachments().is_empty()));
        queued.read(&app, |q, _| {
            let attachments = q.attachments_for(conv, id);
            assert_eq!(attachments.len(), 2);
            assert_eq!(attachments[0].file_name(), "a.png");
            assert_eq!(attachments[1].file_name(), "notes.txt");
        });
    });
}

/// Builds a context model for the TUI surface, which has no agent-view controller and
/// therefore tracks the selected conversation purely through `pending_query_state`.
fn build_tui_context_model(app: &mut App) -> ModelHandle<BlocklistAIContextModel> {
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
    let terminal_surface_id = EntityId::new();

    app.add_model(|_| {
        BlocklistAIContextModel::mock_agent_view_less(terminal_model, terminal_surface_id)
    })
}

#[test]
fn tui_context_tracks_selected_conversation() {
    App::test((), |mut app| async move {
        let model = build_tui_context_model(&mut app);
        let conversation_id = AIConversationId::new();

        model.update(&mut app, |model, ctx| {
            model.set_pending_query_state_for_existing_conversation(
                conversation_id,
                AgentViewEntryOrigin::Cli,
                ctx,
            );
        });
        model.read(&app, |model, ctx| {
            assert_eq!(model.selected_conversation_id(ctx), Some(conversation_id));
        });

        model.update(&mut app, |model, ctx| {
            model.set_pending_query_state_for_new_conversation(AgentViewEntryOrigin::Cli, ctx);
        });
        model.read(&app, |model, ctx| {
            assert_eq!(model.selected_conversation_id(ctx), None);
        });
    });
}
