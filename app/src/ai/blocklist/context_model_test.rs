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
use crate::ai::agent::conversation::{AIConversationAutoexecuteMode, AIConversationId};
use crate::ai::agent::{AIAgentAttachment, ImageContext};
use crate::ai::blocklist::agent_view::conversation_selection::AgentViewConversationSelection;
use crate::ai::blocklist::agent_view::{
    AgentViewController, AgentViewEntryOrigin, EnterAgentViewError, EphemeralMessageModel,
};
use crate::ai::blocklist::conversation_selection::{ConversationSelection, ConversationSelectionEvent};
use crate::ai::blocklist::{
    BlocklistAIHistoryEvent, BlocklistAIHistoryModel, QueuedQuery, QueuedQueryModel,
    QueuedQueryOrigin,
};
use crate::ai::conversation_entry::{
    AgentConversationEntry, AgentConversationListEntryState, AgentConversationListPolicy,
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
#[cfg(feature = "local_fs")]
use crate::ai::agent::AIAgentContext;
#[cfg(feature = "local_fs")]
use crate::code_review::git_status_update::GitRepoStatusModel;
#[cfg(feature = "local_fs")]
use crate::code_review::github_repo_model::GitHubRepoModel;
#[cfg(feature = "local_fs")]
use crate::util::git::{PrInfo, RepositoryInfo};
#[cfg(feature = "local_fs")]
use repo_metadata::DirectoryWatcher;
#[cfg(feature = "local_fs")]
use warp_util::standardized_path::StandardizedPath;
#[cfg(feature = "local_fs")]
use warpui::SingletonEntity as _;

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
    // #316's `AgentViewConversationSelection::new` subscribes to
    // `BlocklistAIHistoryModel::handle(ctx)`, so constructing the selection below now
    // requires this singleton. `get_singleton_model_as_ref` panics rather than
    // returning None when it is missing.
    app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

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
    let conversation_selection = app.add_model(|ctx| {
        Box::new(AgentViewConversationSelection::new(
            terminal_view_id,
            agent_view_controller.clone(),
            ctx,
        )) as Box<dyn ConversationSelection>
    });

    app.add_model(|_| {
        BlocklistAIContextModel::new_for_test(
            terminal_model,
            terminal_view_id,
            agent_view_controller,
            conversation_selection,
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

/// Minimal fake [`ConversationSelection`] that actually creates and tracks conversations in
/// [`BlocklistAIHistoryModel`], unlike [`MockConversationSelection`] (a pure no-op stub). Used to
/// test [`BlocklistAIContextModel::try_start_new_conversation`] on a TUI-shaped context model
/// (no `agent_view_controller`) without depending on `crates/warp_tui`'s real
/// `TuiConversationSelection` (which can't be used from `app`'s own tests: `warp_tui` depends on
/// `app`, not the other way around).
///
/// Ported from the pin (`app/src/ai/blocklist/context_model_tests.rs:58-`, `02b53fcd8`), adapted
/// to this fork's `conversation_entry`-based `AgentConversationListPolicy`/`AgentConversationEntry`
/// (pin: `agent_conversations_model`) and `start_new_conversation`'s 3-bool-free arity (this fork
/// has no `is_cli_agent_transcript` parameter).
struct TestConversationSelection {
    terminal_view_id: EntityId,
    selected_conversation_id: Option<AIConversationId>,
}

impl TestConversationSelection {
    fn new(
        terminal_view_id: EntityId,
        _: &mut warpui::ModelContext<Box<dyn ConversationSelection>>,
    ) -> Self {
        Self {
            terminal_view_id,
            selected_conversation_id: None,
        }
    }
}

impl AgentConversationListPolicy for TestConversationSelection {
    fn classify_entry(
        &self,
        _: &AgentConversationEntry,
        _: &warpui::AppContext,
    ) -> AgentConversationListEntryState {
        AgentConversationListEntryState::Unavailable
    }
}

impl ConversationSelection for TestConversationSelection {
    fn selected_conversation_id(&self, _: &warpui::AppContext) -> Option<AIConversationId> {
        self.selected_conversation_id
    }

    fn is_conversation_active(&self, _: &warpui::AppContext) -> bool {
        self.selected_conversation_id.is_some()
    }

    fn is_conversation_fullscreen(&self, _: &warpui::AppContext) -> bool {
        self.selected_conversation_id.is_some()
    }

    fn select_existing_conversation(
        &mut self,
        conversation_id: AIConversationId,
        _: AgentViewEntryOrigin,
        ctx: &mut warpui::ModelContext<Box<dyn ConversationSelection>>,
    ) {
        if self.selected_conversation_id != Some(conversation_id) {
            self.selected_conversation_id = Some(conversation_id);
            ctx.emit(ConversationSelectionEvent::Changed);
        }
    }

    fn select_new_conversation(
        &mut self,
        _: AgentViewEntryOrigin,
        ctx: &mut warpui::ModelContext<Box<dyn ConversationSelection>>,
    ) {
        if self.selected_conversation_id.take().is_some() {
            ctx.emit(ConversationSelectionEvent::Changed);
        }
    }

    fn try_start_new_conversation(
        &mut self,
        _: AgentViewEntryOrigin,
        ctx: &mut warpui::ModelContext<Box<dyn ConversationSelection>>,
    ) -> Result<AIConversationId, EnterAgentViewError> {
        let conversation_id = BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
            history.start_new_conversation(self.terminal_view_id, false, false, ctx)
        });
        self.select_existing_conversation(conversation_id, AgentViewEntryOrigin::Cli, ctx);
        Ok(conversation_id)
    }

    fn pending_query_autoexecute_override(
        &self,
        app: &warpui::AppContext,
    ) -> AIConversationAutoexecuteMode {
        self.selected_conversation_id
            .as_ref()
            .and_then(|conversation_id| {
                BlocklistAIHistoryModel::as_ref(app).conversation(conversation_id)
            })
            .map(|conversation| conversation.autoexecute_override())
            .unwrap_or_default()
    }

    fn toggle_pending_query_autoexecute(
        &mut self,
        ctx: &mut warpui::ModelContext<Box<dyn ConversationSelection>>,
    ) {
        if let Some(conversation_id) = self.selected_conversation_id {
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.toggle_autoexecute_override(&conversation_id, self.terminal_view_id, ctx);
            });
        }
    }

    fn handle_history_event(
        &mut self,
        _: &BlocklistAIHistoryEvent,
        _: &mut warpui::ModelContext<Box<dyn ConversationSelection>>,
    ) {
    }
}

/// Builds a context model for the TUI surface, which has no agent-view controller and
/// therefore tracks the selected conversation purely through `pending_query_state`. Backed by
/// [`TestConversationSelection`] (not [`MockConversationSelection`]) so
/// `try_start_new_conversation` actually creates a conversation, matching the real TUI.
fn build_tui_context_model(app: &mut App) -> (ModelHandle<BlocklistAIContextModel>, EntityId) {
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
    let conversation_selection = app.add_model(|ctx| {
        Box::new(TestConversationSelection::new(terminal_view_id, ctx))
            as Box<dyn ConversationSelection>
    });

    let model = app.add_model(|_| {
        BlocklistAIContextModel::mock_agent_view_less(
            terminal_model,
            terminal_view_id,
            conversation_selection,
        )
    });
    (model, terminal_view_id)
}

#[test]
fn tui_context_tracks_selected_conversation() {
    App::test((), |mut app| async move {
        let (model, _) = build_tui_context_model(&mut app);
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

/// Ported from the pin (`app/src/ai/blocklist/context_model_tests.rs:312-334`, `02b53fcd8`),
/// adapted to `all_live_conversations_for_terminal_view` (this fork's name for
/// `all_live_conversations_for_terminal_surface`). #343: before that issue,
/// `try_start_new_conversation` (then `try_enter_agent_view_for_new_conversation`) always
/// returned `Err` on a TUI-shaped context model (no `agent_view_controller`), so this scenario
/// was unreachable.
#[test]
fn tui_new_conversation_is_selected_and_terminal_surface_scoped() {
    App::test((), |mut app| async move {
        // `TestConversationSelection::try_start_new_conversation` calls
        // `BlocklistAIHistoryModel::handle`, which panics if the singleton was never registered
        // (unlike `build_test_context_model`'s fixture, `build_tui_context_model` doesn't
        // register it, since `tui_context_tracks_selected_conversation` above doesn't need it).
        let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let (model, terminal_view_id) = build_tui_context_model(&mut app);

        let conversation_id = model
            .update(&mut app, |model, ctx| {
                model.try_start_new_conversation(AgentViewEntryOrigin::Cli, ctx)
            })
            .expect("TUI conversation creation should succeed");

        model.read(&app, |model, ctx| {
            assert_eq!(model.selected_conversation_id(ctx), Some(conversation_id));
        });
        history.read(&app, |history, _| {
            assert_eq!(
                history
                    .all_live_conversations_for_terminal_view(terminal_view_id)
                    .map(|conversation| conversation.id())
                    .collect::<Vec<_>>(),
                vec![conversation_id]
            );
        });
    });
}

// ─── Ported from the pinned oracle ───────────────────────────────────────────
// `02b53fcd8:app/src/ai/blocklist/context_model_tests.rs`. The pin's
// `GitRepoStatusModel::new_local_for_test(repo, metadata, ctx)` is this fork's
// `GitRepoStatusModel::new_for_test(repo, metadata)`.

/// Builds an inert `GitHubRepoModel` over a throwaway sibling git-status model.
#[cfg(feature = "local_fs")]
fn new_github_repo_model_for_test(
    app: &mut App,
) -> (tempfile::TempDir, ModelHandle<GitHubRepoModel>) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let watcher_handle = app.add_singleton_model(DirectoryWatcher::new_for_testing);
    let repository = watcher_handle.update(app, |watcher, ctx| {
        watcher
            .add_directory(
                StandardizedPath::from_local_canonicalized(temp_dir.path()).unwrap(),
                ctx,
            )
            .unwrap()
    });
    let git_status = app.add_model(move |_| GitRepoStatusModel::new_for_test(repository, None));
    let model = app.add_model(move |ctx| GitHubRepoModel::new_local_for_test(git_status, ctx));
    (temp_dir, model)
}

#[cfg(feature = "local_fs")]
#[test]
fn repository_context_reads_github_repo_model() {
    App::test((), |mut app| async move {
        let context_model = build_test_context_model(&mut app);
        let (_temp_dir, github_repo_model) = new_github_repo_model_for_test(&mut app);

        github_repo_model.update(&mut app, |model, ctx| {
            model.set_repository_info_for_test(
                Some(RepositoryInfo {
                    name: "warp-internal".to_owned(),
                    owner: Some("warpdotdev".to_owned()),
                    host: Some("github.com".to_owned()),
                }),
                ctx,
            );
        });

        context_model.update(&mut app, |model, _| {
            model.set_github_repo_model(Some(github_repo_model.downgrade()));
        });

        context_model.read(&app, |model, ctx| {
            assert_eq!(
                model.repository_context(ctx),
                Some(AIAgentContext::Repository {
                    name: "warp-internal".to_owned(),
                    owner: Some("warpdotdev".to_owned()),
                    host: Some("github.com".to_owned()),
                })
            );
        });

        context_model.update(&mut app, |model, _| {
            model.set_github_repo_model(None);
        });

        context_model.read(&app, |model, ctx| {
            assert_eq!(model.repository_context(ctx), None);
        });
    });
}

#[cfg(feature = "local_fs")]
#[test]
fn pull_request_context_reads_github_repo_model() {
    App::test((), |mut app| async move {
        let context_model = build_test_context_model(&mut app);
        let (_temp_dir, github_repo_model) = new_github_repo_model_for_test(&mut app);

        github_repo_model.update(&mut app, |model, ctx| {
            model.set_pr_info_for_test(
                Some(PrInfo {
                    number: 123,
                    url: "https://github.com/warpdotdev/warp/pull/123".to_owned(),
                    state: "OPEN".to_owned(),
                    draft: false,
                    base_branch: "main".to_owned(),
                }),
                ctx,
            );
        });

        context_model.update(&mut app, |model, _| {
            model.set_github_repo_model(Some(github_repo_model.downgrade()));
        });

        context_model.read(&app, |model, ctx| {
            assert_eq!(
                model.pull_request_context(ctx),
                Some(AIAgentContext::PullRequest {
                    number: 123,
                    state: "OPEN".to_owned(),
                    draft: false,
                    base_branch: "main".to_owned(),
                    url: "https://github.com/warpdotdev/warp/pull/123".to_owned(),
                })
            );
        });

        context_model.update(&mut app, |model, _| {
            model.set_github_repo_model(None);
        });

        context_model.read(&app, |model, ctx| {
            assert_eq!(model.pull_request_context(ctx), None);
        });
    });
}

#[cfg(feature = "local_fs")]
#[test]
fn repository_context_from_repository_info_converts_to_agent_context() {
    let repository_info = RepositoryInfo {
        name: "warp-internal".to_owned(),
        owner: Some("warpdotdev".to_owned()),
        host: Some("github.com".to_owned()),
    };

    assert_eq!(
        BlocklistAIContextModel::repository_context_from_repository_info(&repository_info),
        AIAgentContext::Repository {
            name: "warp-internal".to_owned(),
            owner: Some("warpdotdev".to_owned()),
            host: Some("github.com".to_owned()),
        }
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn pull_request_context_from_pr_info_includes_url() {
    let pr_info = PrInfo {
        number: 123,
        url: "https://github.com/warpdotdev/warp/pull/123".to_owned(),
        state: "OPEN".to_owned(),
        draft: true,
        base_branch: "main".to_owned(),
    };

    assert_eq!(
        BlocklistAIContextModel::pull_request_context_from_pr_info(&pr_info),
        Some(AIAgentContext::PullRequest {
            number: 123,
            state: "OPEN".to_owned(),
            draft: true,
            base_branch: "main".to_owned(),
            url: "https://github.com/warpdotdev/warp/pull/123".to_owned(),
        })
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn pull_request_context_from_pr_info_rejects_numbers_that_do_not_fit_agent_context() {
    let pr_info = PrInfo {
        number: i32::MAX as u64 + 1,
        url: "https://github.com/warpdotdev/warp/pull/2147483648".to_owned(),
        state: "OPEN".to_owned(),
        draft: false,
        base_branch: "main".to_owned(),
    };

    assert_eq!(
        BlocklistAIContextModel::pull_request_context_from_pr_info(&pr_info),
        None
    );
}
