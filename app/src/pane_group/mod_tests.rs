//! Ported from Warp's `app/src/pane_group/mod_tests.rs` at the pin recorded
//! in `ORACLE.md` (`02b53fcd8`). The pin carries 49 tests here (48 active,
//! 1 already commented out upstream as flaky); the fork had none.
//!
//! Most of the pin's suite is out of scope for this fork:
//! - ~36 tests exercise cloud/remote orchestration (`decide_remote_child_hydration_*`,
//!   `test_*_shared_session*`, `test_ambient_transcript_restore_*_cloud_mode*`,
//!   `test_entering_remote_parent_agent_view_*`) or the ambient-agent UI
//!   subsystem that was physically removed (see the `Zap Wave 7-3` comments
//!   in `mod.rs`).
//! - A handful more depended on a *lazy* hidden-child-agent-pane restoration
//!   mechanism (`restore_missing_child_agent_panes_for_parent`,
//!   `ensure_hidden_child_agent_pane_for_conversation`, the pin's
//!   `enter_agent_view_for_conversation` test helper) that this fork did not
//!   have -- it only restored child panes *eagerly*, once, at
//!   `PaneGroup::new_internal` time (`create_missing_child_agent_panes`).
//!   **That mechanism is now ported** (see the "Lazy hidden-child-agent pane
//!   restoration" section below and the production code in `mod.rs` /
//!   `pane/terminal_pane.rs`), so the non-cloud half of those tests is live
//!   here. The cloud half -- anything whose subject is a *remote* child agent
//!   (`is_remote_child`, `hydrate_task_backed_hidden_child_pane`, ambient
//!   session re-attach) -- stays out of scope: this fork has no remote runner,
//!   and `BlocklistAIHistoryModel::mark_conversation_as_remote_child`'s doc
//!   comment records that `is_remote_child` is permanently `false` here.
//! - `test_start_shared_session_from_modal` / `test_stop_shared_session` call
//!   `TerminalView::attempt_to_share_session`, which is a declared no-op here
//!   ("Zap: the Shared Session network entry point has been cut" --
//!   `terminal/view/shared_session/view_impl.rs`); testing it would be a test
//!   against a gutted stub, which `script/check_stub_coverage` exists to
//!   forbid.
//!
//! This file ports the tests whose full dependency chain was confirmed
//! present in this fork's `PaneGroup` by reading the source, not just
//! grepping a name match. See individual test doc comments for per-test
//! notes on API drift from the pin.
//!
//! Three more (`test_initial_widths_are_computed_correctly`,
//! `test_pane_focus_does_not_have_an_infinite_event_loop`,
//! `test_focused_pane_is_synchronized_with_application_focus`) needed a
//! purpose-built harness rather than the plain `mock_pane_group` above --
//! see `MockOptions`/`mock_pane_group_with_options` and `FocusDetectionView`
//! further down.

use std::collections::HashMap;

use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;
use warpui::App;
use warpui::platform::{WindowBounds, WindowStyle};

use crate::ai::blocklist::orchestration_topology::descendant_conversation_ids_in_spawn_order;
use crate::notebooks::notebook::NotebookView;
use crate::terminal::shared_session::SharedSessionStatus;
use warpui::windowing::state::ApplicationStage;

use super::*;

/// Builds a real, single-terminal-pane `PaneGroup` using the fork's
/// `workspace`-level test harness (`workspace::view::tests::initialize_app` +
/// `mock_workspace`), the same harness `pane/terminal_pane_tests.rs` already
/// uses successfully. The pin's own `mod_tests.rs` has its own
/// `initialize_app`/`mock_pane_group` pair, but those pull in cloud
/// singletons (`ServerApiProvider`, `IapManager`, `CodebaseIndexManager`,
/// `CloudModel`, ...) that don't exist in this fork.
fn mock_pane_group(app: &mut App) -> ViewHandle<PaneGroup> {
    crate::workspace::view::tests::initialize_app(app);
    let workspace = crate::workspace::view::tests::mock_workspace(app);
    workspace
        .read(app, |workspace, _| workspace.tab_views().next().cloned())
        .expect("mock_workspace has an initial tab")
}

fn get_newly_created_pane_id(panes: &PaneGroup, existing_ids: &[PaneId]) -> PaneId {
    panes
        .pane_ids()
        .find(|id| !existing_ids.contains(id))
        .unwrap()
}

fn split_pane_state(panes: &PaneGroup, pane_id: PaneId, ctx: &AppContext) -> SplitPaneState {
    panes
        .focus_state_handle()
        .as_ref(ctx)
        .split_pane_state_for(pane_id)
}

fn is_active_session(panes: &PaneGroup, pane_id: PaneId, ctx: &AppContext) -> bool {
    panes.active_session_id(ctx).map(Into::into) == Some(pane_id)
}

fn new_notebook(ctx: &mut ViewContext<PaneGroup>) -> ViewHandle<NotebookView> {
    ctx.add_typed_action_view(NotebookView::new)
}

/// A second, independent `PaneGroup` living in its own workspace window, for
/// the cross-tab ownership cases.
///
/// The pin's `mod_tests.rs` just calls its own `mock_pane_group` twice. This
/// fork's `mock_pane_group` (above) also runs `initialize_app`, which registers
/// singleton models and must run exactly once per `App`, so the second group
/// goes through `mock_workspace` only. Both groups still share the one
/// `BlocklistAIHistoryModel` singleton, which is what these tests are about.
fn additional_mock_pane_group(app: &mut App) -> ViewHandle<PaneGroup> {
    let workspace = crate::workspace::view::tests::mock_workspace(app);
    workspace
        .read(app, |workspace, _| workspace.tab_views().next().cloned())
        .expect("mock_workspace has an initial tab")
}

/// Fork drift: the pin's `BlocklistAIHistoryModel::start_new_conversation`
/// takes three booleans (`is_autoexecute_override`, `is_ambient_agent`,
/// `is_viewing_shared_session`); this fork's takes two -- the ambient-agent
/// flag went with the ambient-agent UI subsystem (see the `Zap Wave 7-3`
/// comments in `mod.rs`).
fn start_parent_conversation_for_terminal_view(
    terminal_view_id: EntityId,
    ctx: &mut ViewContext<PaneGroup>,
) -> AIConversationId {
    BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
        history_model.start_new_conversation(terminal_view_id, false, false, ctx)
    })
}

fn start_parent_conversation(
    panes: &PaneGroup,
    parent_pane_id: PaneId,
    ctx: &mut ViewContext<PaneGroup>,
) -> AIConversationId {
    let parent_terminal_view_id = panes
        .terminal_view_from_pane_id(parent_pane_id, ctx)
        .expect("parent pane should have a terminal view")
        .id();
    start_parent_conversation_for_terminal_view(parent_terminal_view_id, ctx)
}

fn restore_conversation_for_terminal_view(
    terminal_view_id: EntityId,
    conversation: AIConversation,
    ctx: &mut ViewContext<PaneGroup>,
) -> AIConversationId {
    let conversation_id = conversation.id();

    BlocklistAIHistoryModel::handle(ctx).update(ctx, |history_model, ctx| {
        history_model.restore_conversations(terminal_view_id, vec![conversation], ctx);
    });

    conversation_id
}

/// Fork drift: `AIConversation::new` takes a single `is_viewing_shared_session`
/// flag here; the pin's takes two (the second is the dropped cloud
/// `is_ambient_agent`).
fn restore_child_conversation_for_terminal_view(
    terminal_view_id: EntityId,
    parent_conversation_id: AIConversationId,
    ctx: &mut ViewContext<PaneGroup>,
) -> AIConversationId {
    let mut child_conversation = AIConversation::new(false);
    child_conversation.set_parent_conversation_id(parent_conversation_id);
    restore_conversation_for_terminal_view(terminal_view_id, child_conversation, ctx)
}

fn restore_child_conversation(
    panes: &PaneGroup,
    pane_id: PaneId,
    parent_conversation_id: AIConversationId,
    ctx: &mut ViewContext<PaneGroup>,
) -> AIConversationId {
    let terminal_view_id = panes
        .terminal_view_from_pane_id(pane_id, ctx)
        .expect("child pane should have a terminal view")
        .id();
    restore_child_conversation_for_terminal_view(terminal_view_id, parent_conversation_id, ctx)
}

fn enter_agent_view_for_conversation(
    panes: &PaneGroup,
    pane_id: PaneId,
    conversation_id: AIConversationId,
    ctx: &mut ViewContext<PaneGroup>,
) {
    panes
        .terminal_view_from_pane_id(pane_id, ctx)
        .expect("pane should have a terminal view")
        .update(ctx, |terminal_view, ctx| {
            terminal_view.enter_agent_view_for_conversation(
                None,
                AgentViewEntryOrigin::RestoreExistingConversation,
                conversation_id,
                ctx,
            );
        });
}

/// Builds a detached `TerminalPane` whose terminal view is *already* in a
/// fullscreen agent view with a restored child conversation underneath it --
/// i.e. the state a pane is in when it is handed to `add_pane_with_direction`
/// or `replace_pane` after being restored, which is exactly the case where
/// nothing ever fires the `EnteredAgentView` subscription for it.
///
/// Fork drift: this fork's `create_terminal_pane_data` takes no
/// `IsSharedSessionCreator` argument (the shared-session network entry point
/// is cut here), so the pin's fourth positional argument is absent.
fn create_already_fullscreen_parent_pane_data(
    panes: &PaneGroup,
    ctx: &mut ViewContext<PaneGroup>,
) -> (TerminalPane, PaneId, AIConversationId) {
    let (pane_data, terminal_view) =
        panes.create_terminal_pane_data(None, HashMap::new(), None, None, ctx);
    let pane_id = pane_data.terminal_pane_id().into();
    let parent_conversation_id =
        start_parent_conversation_for_terminal_view(terminal_view.id(), ctx);
    let child_conversation_id = restore_child_conversation_for_terminal_view(
        terminal_view.id(),
        parent_conversation_id,
        ctx,
    );

    terminal_view.update(ctx, |terminal_view, ctx| {
        terminal_view.enter_agent_view_for_conversation(
            None,
            AgentViewEntryOrigin::RestoreExistingConversation,
            parent_conversation_id,
            ctx,
        );
    });

    (pane_data, pane_id, child_conversation_id)
}

/// Reads back the ambient task id that a hidden child pane's own
/// `BlocklistAIController` is carrying.
///
/// Ported verbatim from the pin's test helper
/// (`42effe840:app/src/pane_group/mod_tests.rs:592`).
fn request_ambient_agent_task_id_for_hidden_child(
    panes: &PaneGroup,
    child_pane_id: PaneId,
    ctx: &mut ViewContext<PaneGroup>,
) -> Option<AmbientAgentTaskId> {
    let terminal_view = panes
        .terminal_view_from_pane_id(child_pane_id, ctx)
        .expect("child pane should have a terminal view");
    let ai_controller = terminal_view.as_ref(ctx).ai_controller().clone();

    ai_controller.update(ctx, |controller, _| controller.get_ambient_agent_task_id())
}

#[test]
#[allow(clippy::clone_on_copy)]
fn test_pane_focus_on_close() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            let first_pane_id = get_newly_created_pane_id(panes, &[]);

            // Add pane Left.
            panes.add_terminal_pane(Direction::Left, None, ctx);
            let second_pane_id = get_newly_created_pane_id(panes, &[first_pane_id]);

            assert!(panes.prev_pane_id(second_pane_id).unwrap() == first_pane_id);

            // Add pane Up.
            panes.add_terminal_pane(Direction::Up, None, ctx);
            let third_pane_id = get_newly_created_pane_id(panes, &[first_pane_id, second_pane_id]);

            // Close the third pane and check that the second pane opened is now focused.
            panes.close_pane(third_pane_id, ctx);
            assert_eq!(second_pane_id, panes.focused_pane_id(ctx));
        })
    });
}

/// Fork drift: the fork's `insert_terminal_pane_hidden_for_child_agent` has
/// no `IsSharedSessionCreator` parameter -- the pin's version takes one to
/// decide transitive sharing, and `transitively_shared_child_panes` (what
/// that decision feeds) does not exist in this fork at all.
#[test]
fn test_insert_hidden_child_agent_pane_keeps_focus_and_active_session() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            let parent_pane_id = get_newly_created_pane_id(panes, &[]);
            let initial_tree_pane_count = panes.pane_count();
            let initial_content_pane_count = panes.pane_ids().count();
            let initial_visible_count = panes.visible_pane_count();
            let initial_active_session = panes.active_session_id(ctx);

            let child_pane_id = panes.insert_terminal_pane_hidden_for_child_agent(
                parent_pane_id,
                HashMap::new(),
                ctx,
            );

            // NOTE -- kept exactly as the pin has it, not weakened: the pin's
            // `insert_terminal_pane_hidden_for_child_agent` attaches the child
            // pane fully off the split tree via a dedicated
            // `attach_child_pane_off_tree`, so `PaneData::len` (what
            // `pane_count()` returns) is untouched by it, and the pin exposes
            // `PaneData::is_pane_in_tree` to assert that directly.
            //
            // This fork instead routes hidden-child-agent panes through the
            // ordinary `add_pane_with_options` -> `PaneData::split` path
            // (`NewPaneVisibility::HiddenForChildAgent`, mod.rs ~5535-5546),
            // the same as any other split pane -- and `PaneData::split`
            // unconditionally increments `len` (tree.rs ~456-462). Visibility
            // is handled separately via `hide_pane_for_child_agent`, so the
            // pane is correctly excluded from `visible_pane_count()` /
            // `pane_id_by_index()`, but `pane_count()` is NOT held constant
            // the way the pin's contract expects. `PaneData::is_pane_in_tree`
            // does not exist in this fork's tree.rs at all, so that pin
            // assertion has no fork equivalent and is omitted below rather
            // than approximated with something else.
            //
            // If the `pane_count()` assertion below goes red, that is this
            // divergence surfacing, not a bad port -- see the pane_count()
            // note in this test's module doc / the porting report for detail.
            assert_eq!(panes.pane_count(), initial_tree_pane_count);
            assert_eq!(panes.pane_ids().count(), initial_content_pane_count + 1);
            assert_eq!(panes.terminal_pane_ids().count(), 2);
            assert_eq!(panes.visible_pane_count(), initial_visible_count);
            assert!(panes.has_pane_id(child_pane_id.into()));

            // The new child pane should remain hidden and not affect visible ordering.
            assert_eq!(panes.pane_id_by_index(0), Some(parent_pane_id));
            assert_eq!(panes.pane_id_by_index(1), None);
            // The hidden child terminal stays *registered* (2 terminal pane ids
            // above) but must not be *visible*: that split is exactly what the
            // integration-test getters got wrong.
            let visible_terminal_views = panes.visible_terminal_views(ctx);
            assert_eq!(visible_terminal_views.len(), 1);
            assert_eq!(
                visible_terminal_views[0].id(),
                panes
                    .terminal_view_from_pane_id(parent_pane_id, ctx)
                    .unwrap()
                    .id()
            );

            // Creating a hidden child pane should not steal focus or active session.
            assert_eq!(panes.focused_pane_id(ctx), parent_pane_id);
            assert_eq!(panes.active_session_id(ctx), initial_active_session);
        });
    });
}

// ---------------------------------------------------------------------------
// Lazy hidden-child-agent pane restoration.
//
// Ported from the pin's `mod_tests.rs`. The production side lives in
// `mod.rs` (`restore_missing_child_agent_panes_for_parent`,
// `restore_missing_child_agent_panes_for_terminal_pane_if_needed`,
// `ensure_hidden_child_agent_pane_for_conversation`) and in
// `pane/terminal_pane.rs` (the `EnteredAgentView` subscription). Before it,
// this fork only ever rebuilt child panes in the *eager*
// `create_missing_child_agent_panes` sweep at `PaneGroup::new_internal`, so a
// tab restore, a pane replacement or an undo-close silently lost every child
// agent until the next cold start.
//
// One assertion differs from the pin in every test below, deliberately and
// with no loss of coverage: the pin asserts
// `!panes.panes.is_pane_in_tree(child_pane_id)` because its hidden child panes
// are attached entirely off the split tree (`attach_child_pane_off_tree`).
// This fork routes them through the ordinary `add_pane_with_options` ->
// `PaneData::split` path with `NewPaneVisibility::HiddenForChildAgent`, so the
// pane *is* in the tree but is registered in `hidden_panes`; `is_pane_in_tree`
// does not exist in this fork's `tree.rs` at all. The fork-equivalent
// invariant -- "the materialized child pane is excluded from the visible
// layout" -- is asserted as `panes.panes.is_pane_hidden(&child_pane_id)`, and
// the surrounding `pane_count()` / `visible_pane_count()` assertions (which
// the fork's `pane_count` compensates for via
// `PaneData::num_child_agent_hidden_panes`) are kept exactly as the pin has
// them. See the divergence note on
// `test_insert_hidden_child_agent_pane_keeps_focus_and_active_session` above.
// ---------------------------------------------------------------------------

#[test]
fn test_reattach_panes_restores_hidden_child_when_parent_is_already_fullscreen() {
    let _agent_view = FeatureFlag::AgentView.override_enabled(true);

    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Must come AFTER the harness above: the helper
        // guards on `has_singleton_model`, so calling it first would let that harness
        // re-register the same singletons and trip the debug-assert.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        // Restoring the persisted child reaches
        // `TerminalView::maybe_send_lrc_queued_prompts_after_subagent_handoff`, which reads
        // `QueuedQueryModel`. Registered here rather than in the shared helper: constructing
        // it reads `BlocklistAIHistoryModel`, so it must come after the harness above.
        app.add_singleton_model(crate::ai::blocklist::QueuedQueryModel::new);

        pane_group.update(&mut app, |panes, ctx| {
            let parent_pane_id = get_newly_created_pane_id(panes, &[]);
            let parent_conversation_id = start_parent_conversation(panes, parent_pane_id, ctx);
            let child_conversation_id =
                restore_child_conversation(panes, parent_pane_id, parent_conversation_id, ctx);
            let initial_pane_count = panes.pane_count();
            let initial_visible_pane_count = panes.visible_pane_count();

            // Detaching drops the `EnteredAgentView` subscription, so the
            // parent goes fullscreen with nobody listening -- the tab-restore
            // race this test exists for.
            panes.detach_panes(ctx);
            enter_agent_view_for_conversation(panes, parent_pane_id, parent_conversation_id, ctx);
            assert!(!panes.child_agent_panes.contains_key(&child_conversation_id));

            panes.reattach_panes(ctx);

            let child_pane_id = panes
                .child_agent_panes
                .get(&child_conversation_id)
                .copied()
                .expect(
                    "reattaching an already-fullscreen parent should materialize the child pane",
                );

            assert!(panes.has_pane_id(child_pane_id));
            assert_eq!(panes.pane_count(), initial_pane_count);
            assert_eq!(panes.visible_pane_count(), initial_visible_pane_count);
            assert!(panes.panes.is_pane_hidden(&child_pane_id));
            assert_eq!(
                panes.pane_id_for_owned_conversation(child_conversation_id, ctx),
                Some(child_pane_id)
            );
        });
    });
}

#[test]
fn test_restore_closed_pane_restores_hidden_child_when_parent_is_already_fullscreen() {
    let _agent_view = FeatureFlag::AgentView.override_enabled(true);
    let _undo_closed_panes = FeatureFlag::UndoClosedPanes.override_enabled(true);

    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Must come AFTER the harness above: the helper
        // guards on `has_singleton_model`, so calling it first would let that harness
        // re-register the same singletons and trip the debug-assert.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        // Restoring the persisted child reaches
        // `TerminalView::maybe_send_lrc_queued_prompts_after_subagent_handoff`, which reads
        // `QueuedQueryModel`. Registered here rather than in the shared helper: constructing
        // it reads `BlocklistAIHistoryModel`, so it must come after the harness above.
        app.add_singleton_model(crate::ai::blocklist::QueuedQueryModel::new);

        pane_group.update(&mut app, |panes, ctx| {
            let parent_pane_id = get_newly_created_pane_id(panes, &[]);
            // A second pane so `close_pane` takes the hide-for-undo branch
            // instead of emitting `Exited` for an empty group.
            panes.add_pane_with_direction(
                Direction::Right,
                NotebookPane::new(new_notebook(ctx), ctx),
                false,
                ctx,
            );

            let parent_conversation_id = start_parent_conversation(panes, parent_pane_id, ctx);
            let child_conversation_id =
                restore_child_conversation(panes, parent_pane_id, parent_conversation_id, ctx);
            let initial_pane_count = panes.pane_count();
            let initial_visible_pane_count = panes.visible_pane_count();

            panes.close_pane(parent_pane_id, ctx);
            assert!(panes.is_pane_hidden_for_close(parent_pane_id));

            enter_agent_view_for_conversation(panes, parent_pane_id, parent_conversation_id, ctx);
            assert!(!panes.child_agent_panes.contains_key(&child_conversation_id));

            assert!(panes.restore_closed_pane(parent_pane_id, ctx));

            let child_pane_id = panes
                .child_agent_panes
                .get(&child_conversation_id)
                .copied()
                .expect(
                    "restoring an already-fullscreen closed parent should materialize the child \
                     pane",
                );

            assert!(panes.has_pane_id(child_pane_id));
            assert_eq!(panes.pane_count(), initial_pane_count);
            assert_eq!(panes.visible_pane_count(), initial_visible_pane_count);
            assert!(panes.panes.is_pane_hidden(&child_pane_id));
            assert_eq!(panes.focused_pane_id(ctx), parent_pane_id);
            assert_eq!(
                panes.pane_id_for_owned_conversation(child_conversation_id, ctx),
                Some(child_pane_id)
            );
        });
    });
}

#[test]
fn test_replace_pane_restores_hidden_child_when_replacement_is_already_fullscreen() {
    let _agent_view = FeatureFlag::AgentView.override_enabled(true);

    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Must come AFTER the harness above: the helper
        // guards on `has_singleton_model`, so calling it first would let that harness
        // re-register the same singletons and trip the debug-assert.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        // Restoring the persisted child reaches
        // `TerminalView::maybe_send_lrc_queued_prompts_after_subagent_handoff`, which reads
        // `QueuedQueryModel`. Registered here rather than in the shared helper: constructing
        // it reads `BlocklistAIHistoryModel`, so it must come after the harness above.
        app.add_singleton_model(crate::ai::blocklist::QueuedQueryModel::new);

        pane_group.update(&mut app, |panes, ctx| {
            let original_pane_id = get_newly_created_pane_id(panes, &[]);
            let initial_pane_count = panes.pane_count();
            let initial_visible_pane_count = panes.visible_pane_count();
            let (replacement_pane, replacement_pane_id, child_conversation_id) =
                create_already_fullscreen_parent_pane_data(panes, ctx);

            assert!(panes.replace_pane(original_pane_id, replacement_pane, false, ctx));

            let child_pane_id = panes
                .child_agent_panes
                .get(&child_conversation_id)
                .copied()
                .expect(
                    "replacing with an already-fullscreen parent should materialize the child pane",
                );

            assert!(!panes.has_pane_id(original_pane_id));
            assert!(panes.has_pane_id(replacement_pane_id));
            assert!(panes.has_pane_id(child_pane_id));
            assert_eq!(panes.pane_count(), initial_pane_count);
            assert_eq!(panes.visible_pane_count(), initial_visible_pane_count);
            assert!(panes.panes.is_pane_hidden(&child_pane_id));
            assert_eq!(panes.focused_pane_id(ctx), replacement_pane_id);
            assert_eq!(
                panes.pane_id_for_owned_conversation(child_conversation_id, ctx),
                Some(child_pane_id)
            );
        });
    });
}

#[test]
fn test_ensure_hidden_child_agent_pane_skips_child_owned_by_another_pane_group() {
    let _agent_view = FeatureFlag::AgentView.override_enabled(true);

    App::test((), |mut app| async move {
        let parent_pane_group = mock_pane_group(&mut app);
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Must come AFTER the harness above: the helper
        // guards on `has_singleton_model`, so calling it first would let that harness
        // re-register the same singletons and trip the debug-assert.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        // Restoring the persisted child reaches
        // `TerminalView::maybe_send_lrc_queued_prompts_after_subagent_handoff`, which reads
        // `QueuedQueryModel`. Registered here rather than in the shared helper: constructing
        // it reads `BlocklistAIHistoryModel`, so it must come after the harness above.
        app.add_singleton_model(crate::ai::blocklist::QueuedQueryModel::new);
        let other_pane_group = additional_mock_pane_group(&mut app);

        let parent_conversation_id = parent_pane_group.update(&mut app, |panes, ctx| {
            let parent_pane_id = get_newly_created_pane_id(panes, &[]);
            start_parent_conversation(panes, parent_pane_id, ctx)
        });
        let (child_conversation_id, child_owner_terminal_view_id) =
            other_pane_group.update(&mut app, |panes, ctx| {
                let child_pane_id = get_newly_created_pane_id(panes, &[]);
                let child_conversation_id =
                    restore_child_conversation(panes, child_pane_id, parent_conversation_id, ctx);
                let initial_owner_terminal_view_id = panes
                    .terminal_view_from_pane_id(child_pane_id, ctx)
                    .expect("child pane should have a terminal view")
                    .id();

                enter_agent_view_for_conversation(panes, child_pane_id, child_conversation_id, ctx);
                (child_conversation_id, initial_owner_terminal_view_id)
            });

        parent_pane_group.update(&mut app, |panes, ctx| {
            let initial_pane_count = panes.pane_count();

            assert!(
                panes.ensure_hidden_child_agent_pane_for_conversation(child_conversation_id, ctx),
                "cross-tab child ownership should be treated as already reachable"
            );
            assert!(!panes.child_agent_panes.contains_key(&child_conversation_id));
            assert_eq!(panes.pane_count(), initial_pane_count);
            // Fork drift: the pin renamed this lookup to
            // `terminal_surface_id_for_conversation`; the index and its
            // semantics are unchanged here.
            assert_eq!(
                BlocklistAIHistoryModel::as_ref(ctx)
                    .terminal_view_id_for_conversation(&child_conversation_id),
                Some(child_owner_terminal_view_id)
            );
        });
    });
}

/// Ported from the pin's
/// `test_restored_hidden_child_pane_reapplies_ambient_task_id_to_controller`
/// (`42effe840:app/src/pane_group/mod_tests.rs:909`).
///
/// A locally spawned child agent carries an `AmbientAgentTaskId` on its
/// conversation (`AmbientAgentTaskId::new_local()` in
/// `prepare_local_harness_launch`, stamped on by
/// `assign_run_id_for_conversation`), and that id survives a restart because
/// `AIConversation`'s `task_id` *is* its persisted `run_id` here. A restored
/// child pane, though, gets a brand-new `BlocklistAIController` -- so
/// `create_hidden_child_agent_pane` has to re-apply the id, or the restored
/// child loses the task identity its requests are supposed to carry
/// (`api::ConversationData::ambient_agent_task_id`).
///
/// Fork drift from the pin: `AIConversation::new` takes one flag here (the
/// pin's second is the dropped cloud `is_ambient_agent`), and the task id is
/// minted with this fork's own `AmbientAgentTaskId::new_local()` rather than
/// the pin's `new_ambient_agent_task_id` test helper -- both are a
/// `Uuid::new_v4()`, the fork's just has a production constructor for it.
///
/// The pin's sibling test `test_hidden_child_creation_applies_ambient_task_id_to_controller`
/// is *not* ported: it drives `create_hidden_child_agent_conversation` /
/// `HiddenChildAgentConversationRequest`, the GUI dispatch API this fork does
/// not have, whose request carries the session-sharing `is_shared_session_creator`
/// that this fork cut. See `apply_hidden_child_agent_task_context` in `mod.rs`.
#[test]
fn test_restored_hidden_child_pane_reapplies_ambient_task_id_to_controller() {
    let _agent_view = FeatureFlag::AgentView.override_enabled(true);

    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Must come AFTER the harness above: the helper
        // guards on `has_singleton_model`, so calling it first would let that harness
        // re-register the same singletons and trip the debug-assert.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        // Restoring the persisted child reaches
        // `TerminalView::maybe_send_lrc_queued_prompts_after_subagent_handoff`, which reads
        // `QueuedQueryModel`. Registered here rather than in the shared helper: constructing
        // it reads `BlocklistAIHistoryModel`, so it must come after the harness above.
        app.add_singleton_model(crate::ai::blocklist::QueuedQueryModel::new);

        pane_group.update(&mut app, |panes, ctx| {
            let parent_pane_id = get_newly_created_pane_id(panes, &[]);
            let parent_conversation_id = start_parent_conversation(panes, parent_pane_id, ctx);
            let task_id = AmbientAgentTaskId::new_local();

            let mut child_conversation = AIConversation::new(false);
            child_conversation.set_parent_conversation_id(parent_conversation_id);
            child_conversation.set_task_id(task_id);
            let child_conversation_id = child_conversation.id();

            panes.create_hidden_child_agent_pane(child_conversation, parent_pane_id, ctx);

            let child_pane_id = panes
                .child_agent_panes
                .get(&child_conversation_id)
                .copied()
                .expect("restored hidden child pane should be tracked");

            assert_eq!(
                request_ambient_agent_task_id_for_hidden_child(panes, child_pane_id, ctx),
                Some(task_id)
            );
        });
    });
}

/// Integration coverage for the restart loop: after the history model has been
/// restored, the orchestration topology must already be wired up *before* the
/// parent's fullscreen agent view is entered, and entering it must then lazily
/// materialize the hidden child pane.
///
/// Fork drift from the pin: the pin's version additionally asserts
/// `history.conversation_id_for_agent_id(...)` round-trips for run ids minted
/// as `AmbientAgentTaskId`s; run ids are plain `String`s on this side of
/// `assign_run_id_for_conversation`, so they are minted as UUID strings here.
#[test]
fn test_pane_group_restore_loop_keeps_orchestration_topology_and_materializes_child_pane() {
    let _agent_view = FeatureFlag::AgentView.override_enabled(true);

    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Must come AFTER the harness above: the helper
        // guards on `has_singleton_model`, so calling it first would let that harness
        // re-register the same singletons and trip the debug-assert.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
        // Restoring the persisted child reaches
        // `TerminalView::maybe_send_lrc_queued_prompts_after_subagent_handoff`, which reads
        // `QueuedQueryModel`. Registered here rather than in the shared helper: constructing
        // it reads `BlocklistAIHistoryModel`, so it must come after the harness above.
        app.add_singleton_model(crate::ai::blocklist::QueuedQueryModel::new);

        let (
            parent_pane_id,
            parent_conversation_id,
            parent_run_id,
            child_conversation_id,
            child_run_id,
            child_agent_name,
        ) = pane_group.update(&mut app, |panes, ctx| {
            let parent_pane_id = get_newly_created_pane_id(panes, &[]);
            let parent_terminal_view_id = panes
                .terminal_view_from_pane_id(parent_pane_id, ctx)
                .expect("parent pane should have a terminal view")
                .id();

            let parent_conversation_id = start_parent_conversation(panes, parent_pane_id, ctx);
            let parent_run_id = Uuid::new_v4().to_string();
            let child_run_id = Uuid::new_v4().to_string();
            let child_agent_name = "Agent 1".to_string();

            // Restore a child conversation into the parent's terminal view --
            // the same code path `RestoredAgentConversations` feeds during
            // pane restoration.
            let mut child_conversation = AIConversation::new(false);
            child_conversation.set_parent_conversation_id(parent_conversation_id);
            child_conversation.set_agent_name(child_agent_name.clone());
            let child_conversation_id = child_conversation.id();
            BlocklistAIHistoryModel::handle(ctx).update(ctx, |history, ctx| {
                history.restore_conversations(
                    parent_terminal_view_id,
                    vec![child_conversation],
                    ctx,
                );
                // Stamp run_ids so orchestration agent_id lookups resolve.
                history.assign_run_id_for_conversation(
                    parent_conversation_id,
                    parent_run_id.clone(),
                    None,
                    parent_terminal_view_id,
                    ctx,
                );
                history.assign_run_id_for_conversation(
                    child_conversation_id,
                    child_run_id.clone(),
                    None,
                    parent_terminal_view_id,
                    ctx,
                );
            });

            (
                parent_pane_id,
                parent_conversation_id,
                parent_run_id,
                child_conversation_id,
                child_run_id,
                child_agent_name,
            )
        });

        // BEFORE the parent's fullscreen agent view is entered, the
        // orchestration data layer must already know:
        //   (a) the parent -> child topology (pill bar source),
        //   (b) the child's local conversation (with agent name set), and
        //   (c) the child's run_id -> conversation id.
        pane_group.read(&app, |panes, ctx| {
            let history = BlocklistAIHistoryModel::as_ref(ctx);

            assert_eq!(
                history.child_conversation_ids_of(&parent_conversation_id),
                &[child_conversation_id],
                "orchestration topology must list the restored child under its parent before any \
                 pane materializes",
            );
            assert_eq!(
                descendant_conversation_ids_in_spawn_order(history, parent_conversation_id),
                vec![child_conversation_id],
                "pill bar pre-order walker must reach the restored child before any pane \
                 materializes",
            );

            let child_conversation = history
                .conversation(&child_conversation_id)
                .expect("restored child must be in conversations_by_id before parent fullscreen");
            assert_eq!(
                child_conversation.agent_name(),
                Some(child_agent_name.as_str()),
                "restored child must retain its display name for transcript / pill bar rendering",
            );

            assert_eq!(
                history.conversation_id_for_agent_id(&child_run_id),
                Some(child_conversation_id),
                "child run_id must resolve to the restored child conversation",
            );
            assert_eq!(
                history.conversation_id_for_agent_id(&parent_run_id),
                Some(parent_conversation_id),
                "parent run_id must resolve to the parent conversation",
            );

            // Hidden child pane must NOT exist yet -- restoration is lazy and
            // only materializes when the parent's agent view is entered.
            assert!(
                !panes.child_agent_panes.contains_key(&child_conversation_id),
                "hidden child pane must not exist before parent fullscreen entry",
            );
        });

        // Enter the parent's fullscreen agent view. This is the trigger for
        // `restore_missing_child_agent_panes_for_parent`, i.e. the PaneGroup
        // side of the user-visible restart-loop bug.
        pane_group.update(&mut app, |panes, ctx| {
            enter_agent_view_for_conversation(panes, parent_pane_id, parent_conversation_id, ctx);
        });

        pane_group.read(&app, |panes, _ctx| {
            let child_pane_id = panes
                .child_agent_panes
                .get(&child_conversation_id)
                .copied()
                .expect("parent fullscreen entry must materialize the hidden child pane");
            assert!(
                panes.has_pane_id(child_pane_id),
                "materialized child pane must be tracked by the pane group",
            );
            assert!(
                panes.panes.is_pane_hidden(&child_pane_id),
                "materialized child pane must stay out of the visible layout",
            );
        });
    });
}

#[test]
fn test_active_session_id_reset_on_last_pane_close() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            let terminal_id = get_newly_created_pane_id(panes, &[]);
            assert_eq!(
                panes.active_session_id(ctx),
                terminal_id.as_terminal_pane_id()
            );

            // Add a non-terminal pane (Notebook) so the pane group remains alive when terminal is closed.
            panes.add_pane_with_direction(
                Direction::Right,
                NotebookPane::new(new_notebook(ctx), ctx),
                false, /* focus_new_pane */
                ctx,
            );

            // Close the terminal.
            panes.close_pane(terminal_id, ctx);

            // active_session_id should be None after closing the last terminal pane.
            assert_eq!(
                panes.active_session_id(ctx),
                None,
                "active_session_id should be None after closing the last pane"
            );
        });
    });
}

#[test]
fn test_group_without_terminals() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            let terminal_id = get_newly_created_pane_id(panes, &[]);

            // Add a notebook to the left.
            panes.add_pane_with_direction(
                Direction::Left,
                NotebookPane::new(new_notebook(ctx), ctx),
                true, /* focus_new_pane */
                ctx,
            );
            let notebook_id = get_newly_created_pane_id(panes, &[terminal_id]);

            // Close the terminal, which should leave the group without an active session.
            panes.close_pane(terminal_id, ctx);
            assert_eq!(panes.focused_pane_id(ctx), notebook_id);
            assert_eq!(panes.active_session_id(ctx), None);
            assert_eq!(
                split_pane_state(panes, notebook_id, ctx),
                SplitPaneState::NotInSplitPane
            );
        });
    });
}

#[test]
fn test_close_active_session() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            // Add two terminal sessions.
            let first_terminal_id = get_newly_created_pane_id(panes, &[]);
            panes.add_terminal_pane(Direction::Up, None, ctx);
            let second_terminal_id = get_newly_created_pane_id(panes, &[first_terminal_id]);

            // Add a notebook to the left.
            panes.add_pane_with_direction(
                Direction::Left,
                NotebookPane::new(new_notebook(ctx), ctx),
                true, /* focus_new_pane */
                ctx,
            );
            let notebook_id =
                get_newly_created_pane_id(panes, &[first_terminal_id, second_terminal_id]);
            assert_eq!(panes.focused_pane_id(ctx), notebook_id);
            assert_eq!(
                panes.active_session_id(ctx).map(Into::into),
                Some(second_terminal_id)
            );

            // Close the active session, which should leave the notebook focused and activate the
            // remaining session.
            panes.close_pane(second_terminal_id, ctx);
            assert_eq!(panes.focused_pane_id(ctx), notebook_id);
            assert_eq!(
                panes.active_session_id(ctx).map(Into::into),
                Some(first_terminal_id)
            );
            assert_eq!(
                split_pane_state(panes, first_terminal_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Unfocused)
            );
            assert!(is_active_session(panes, first_terminal_id, ctx));

            // Now, focus the remaining session, which should keep it activated.
            panes.focus_pane_by_id(first_terminal_id, ctx);
            assert_eq!(panes.focused_pane_id(ctx), first_terminal_id);
            assert_eq!(
                panes.active_session_id(ctx).map(Into::into),
                Some(first_terminal_id)
            );
            assert_eq!(
                split_pane_state(panes, first_terminal_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Focused)
            );
            assert_eq!(
                split_pane_state(panes, notebook_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Unfocused)
            );
            assert!(is_active_session(panes, first_terminal_id, ctx));
        });
    });
}

#[test]
fn test_focus_notebook() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            let first_terminal_id = get_newly_created_pane_id(panes, &[]);

            // Add a notebook to the left.
            panes.add_pane_with_direction(
                Direction::Left,
                NotebookPane::new(new_notebook(ctx), ctx),
                true, /* focus_new_pane */
                ctx,
            );
            let notebook_id = get_newly_created_pane_id(panes, &[first_terminal_id]);

            // The new pane should be focused, but the terminal is still the active session.
            assert_eq!(panes.focused_pane_id(ctx), notebook_id);
            assert_eq!(
                panes.active_session_id(ctx).map(Into::into),
                Some(first_terminal_id)
            );
            assert_eq!(
                split_pane_state(panes, first_terminal_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Unfocused)
            );
            assert!(is_active_session(panes, first_terminal_id, ctx));
            assert_eq!(
                split_pane_state(panes, notebook_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Focused)
            );

            // Add a terminal below.
            panes.add_terminal_pane(Direction::Down, None, ctx);
            let second_terminal_id =
                get_newly_created_pane_id(panes, &[first_terminal_id, notebook_id]);

            // The new terminal should be both focused and the active session.
            assert_eq!(panes.focused_pane_id(ctx), second_terminal_id);
            assert_eq!(
                panes.active_session_id(ctx).map(Into::into),
                Some(second_terminal_id)
            );
            assert_eq!(
                split_pane_state(panes, first_terminal_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Unfocused)
            );
            assert!(!is_active_session(panes, first_terminal_id, ctx));
            assert_eq!(
                split_pane_state(panes, second_terminal_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Focused)
            );
            assert!(is_active_session(panes, second_terminal_id, ctx));
            assert_eq!(
                split_pane_state(panes, notebook_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Unfocused)
            );

            // Close the new terminal. Focus should switch to the notebook, and the first terminal
            // session will activate.
            panes.close_pane(second_terminal_id, ctx);
            assert_eq!(panes.focused_pane_id(ctx), notebook_id);
            assert_eq!(
                panes.active_session_id(ctx).map(Into::into),
                Some(first_terminal_id)
            );
            assert_eq!(
                split_pane_state(panes, first_terminal_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Unfocused)
            );
            assert_eq!(
                split_pane_state(panes, notebook_id, ctx),
                SplitPaneState::InSplitPane(PaneState::Focused)
            );
            assert!(is_active_session(panes, first_terminal_id, ctx));
        })
    });
}

// Ensures that we always show the pane header for terminal panes, regardless of split state.
#[test]
fn test_terminal_pane_headers() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        // There should be a single terminal pane to start and the pane header should be shown.
        pane_group.read(&app, |pane_group, ctx| {
            assert_eq!(pane_group.pane_contents.len(), 1);

            let terminal_panes = pane_group.panes_of::<TerminalPane>().collect_vec();
            assert_eq!(terminal_panes.len(), 1);

            let pane_view = terminal_panes[0].pane_view();
            let header_visible = pane_view
                .as_ref(ctx)
                .header()
                .as_ref(ctx)
                .is_visible_in_pane_group();
            assert!(header_visible);
        });

        // Create a terminal split pane.
        pane_group.update(&mut app, |pane_group, ctx| {
            pane_group.add_terminal_pane(Direction::Left, None, ctx);
        });

        // There should be two terminal panes and they should both have the pane header.
        pane_group.read(&app, |pane_group, ctx| {
            assert_eq!(pane_group.pane_contents.len(), 2);

            let terminal_panes = pane_group.panes_of::<TerminalPane>().collect_vec();
            assert_eq!(terminal_panes.len(), 2);

            for terminal_pane in terminal_panes {
                let pane_view = terminal_pane.pane_view();
                assert!(
                    pane_view
                        .as_ref(ctx)
                        .header()
                        .as_ref(ctx)
                        .is_visible_in_pane_group()
                );
            }
        });

        // Close one of the panes; the remaining pane should still have a header.
        pane_group.update(&mut app, |pane_group, ctx| {
            pane_group.close_pane(pane_group.focused_pane_id(ctx), ctx);
        });

        pane_group.read(&app, |pane_group, ctx| {
            assert_eq!(pane_group.pane_contents.len(), 1);

            let terminal_panes = pane_group.panes_of::<TerminalPane>().collect_vec();
            assert_eq!(terminal_panes.len(), 1);

            let pane_view = terminal_panes[0].pane_view();
            assert!(
                pane_view
                    .as_ref(ctx)
                    .header()
                    .as_ref(ctx)
                    .is_visible_in_pane_group()
            );
        });

        // Create a non-terminal split pane. Terminal pane header remains visible.
        pane_group.update(&mut app, |pane_group, ctx| {
            pane_group.add_pane_with_direction(
                Direction::Left,
                NotebookPane::new(new_notebook(ctx), ctx),
                true, /* focus_new_pane */
                ctx,
            );
        });

        pane_group.read(&app, |pane_group, ctx| {
            assert_eq!(pane_group.pane_contents.len(), 2);

            let terminal_panes = pane_group.panes_of::<TerminalPane>().collect_vec();
            assert_eq!(terminal_panes.len(), 1);

            let pane_view = terminal_panes[0].pane_view();
            assert!(
                pane_view
                    .as_ref(ctx)
                    .header()
                    .as_ref(ctx)
                    .is_visible_in_pane_group()
            );
        });
    });
}

/// Uses `prev_pane_id_navigation`/`next_pane_id` -- the navigation helpers that
/// skip panes hidden for undo-close, distinct from `prev_pane_id` (raw split order).
#[test]
fn test_navigation_skips_hidden_closed_panes() {
    let _guard = FeatureFlag::UndoClosedPanes.override_enabled(true);
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            // Add second terminal to the right to create a horizontal pair
            panes.add_terminal_pane(Direction::Right, None, ctx);

            // Add third terminal; place it to the right of current focus
            panes.add_terminal_pane(Direction::Right, None, ctx);

            // Determine ordered visible panes by index 0..2
            let a = panes.pane_id_by_index(0).expect("pane 0 exists");
            let b = panes.pane_id_by_index(1).expect("pane 1 exists");
            let c = panes.pane_id_by_index(2).expect("pane 2 exists");

            // Focus C and confirm prev would be B when all are visible
            panes.focus_pane_by_id(c, ctx);
            assert_eq!(panes.prev_pane_id_navigation(c), Some(b));

            // Close B (it will be hidden for undo and excluded from visible navigation)
            panes.close_pane(b, ctx);

            // Now prev from C should skip B and go to A
            assert_eq!(panes.prev_pane_id_navigation(c), Some(a));

            // And next from A should skip B and go to C
            assert_eq!(panes.next_pane_id(a), Some(c));
        })
    });
}

/// A minimal `PaneContent` whose `pre_attach` hook always refuses attachment,
/// used to exercise `add_pane_with_direction`'s abort path.
struct PreAttachReturnsFalsePane {
    pane_id: PaneId,
    pane_configuration: ModelHandle<PaneConfiguration>,
}

impl PreAttachReturnsFalsePane {
    fn new(ctx: &mut ViewContext<PaneGroup>) -> Self {
        Self {
            pane_id: PaneId::dummy_pane_id(),
            pane_configuration: ctx.add_model(|_ctx| PaneConfiguration::new("")),
        }
    }
}

impl PaneContent for PreAttachReturnsFalsePane {
    fn id(&self) -> PaneId {
        self.pane_id
    }

    fn pre_attach(&self, _group: &PaneGroup, _ctx: &mut ViewContext<PaneGroup>) -> bool {
        false
    }

    fn attach(
        &self,
        _group: &PaneGroup,
        _focus_handle: focus_state::PaneFocusHandle,
        _ctx: &mut ViewContext<PaneGroup>,
    ) {
    }

    fn detach(
        &self,
        _group: &PaneGroup,
        _detach_type: pane::DetachType,
        _ctx: &mut ViewContext<PaneGroup>,
    ) {
    }

    fn snapshot(&self, _app: &AppContext) -> LeafContents {
        LeafContents::GetStarted
    }

    fn has_application_focus(&self, _ctx: &mut ViewContext<PaneGroup>) -> bool {
        false
    }

    fn focus(&self, _ctx: &mut ViewContext<PaneGroup>) {}

    fn shareable_link(
        &self,
        _ctx: &mut ViewContext<PaneGroup>,
    ) -> Result<pane::ShareableLink, pane::ShareableLinkError> {
        Ok(pane::ShareableLink::Base)
    }

    fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn is_pane_being_dragged(&self, _ctx: &AppContext) -> bool {
        false
    }
}

#[test]
fn test_add_pane_aborts_cleanly_when_pre_attach_returns_false() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            let before_snapshot = panes.snapshot(ctx);
            let before_count = panes.pane_count();

            panes.add_pane_with_direction(
                Direction::Right,
                PreAttachReturnsFalsePane::new(ctx),
                true, /* focus_new_pane */
                ctx,
            );

            assert_eq!(panes.pane_count(), before_count);
            assert_eq!(panes.snapshot(ctx), before_snapshot);
        });
    });
}

/// The pin's counterparts (`test_start_shared_session_from_modal`,
/// `test_stop_shared_session`) drive sharing through
/// `TerminalView::attempt_to_share_session`, which this fork has turned into
/// a declared no-op ("Zap: the Shared Session network entry point has been
/// cut" -- `terminal/view/shared_session/view_impl.rs`) now that the
/// session-sharing websocket transport is gone. This test instead sets
/// `SharedSessionStatus` directly on the `TerminalModel`, bypassing that
/// no-op entirely, so it exercises real, non-stub `PaneGroup` bookkeeping
/// (`is_terminal_pane_being_shared`/`number_of_shared_sessions`) rather than
/// the removed transport.
#[test]
fn test_is_terminal_pane_being_shared() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            assert!(!panes.is_terminal_pane_being_shared(ctx));

            // Add another pane; the pane group should still be "unshared".
            panes.add_terminal_pane(Direction::Left, None, ctx);
            assert!(!panes.is_terminal_pane_being_shared(ctx));

            // Make one of the terminal panes shared. There is now at least one terminal pane being shared.
            panes
                .terminal_session_by_pane_index(0)
                .expect("terminal pane exists")
                .terminal_manager(ctx)
                .as_ref(ctx)
                .model()
                .lock()
                .set_shared_session_status(SharedSessionStatus::ActiveSharer);
            assert!(panes.is_terminal_pane_being_shared(ctx));
        });
    });
}

/// See `test_is_terminal_pane_being_shared`'s doc comment for why this sets
/// `SharedSessionStatus` directly instead of going through the now-no-op
/// `attempt_to_share_session`.
#[test]
fn test_number_of_shared_panes() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.update(&mut app, |panes, ctx| {
            // We have two terminal sessions. Neither is shared
            let first_pane_id = get_newly_created_pane_id(panes, &[]);
            panes.add_terminal_pane(Direction::Up, None, ctx);
            assert_eq!(panes.number_of_shared_sessions(ctx), 0);

            // Make one pane shared
            panes
                .terminal_manager(0, ctx)
                .unwrap()
                .as_ref(ctx)
                .model()
                .lock()
                .set_shared_session_status(SharedSessionStatus::ActiveSharer);
            assert_eq!(panes.number_of_shared_sessions(ctx), 1);

            // Make both panes shared
            panes
                .terminal_manager(1, ctx)
                .unwrap()
                .as_ref(ctx)
                .model()
                .lock()
                .set_shared_session_status(SharedSessionStatus::ActiveSharer);
            assert_eq!(panes.number_of_shared_sessions(ctx), 2);

            // Close a pane
            panes.close_pane(first_pane_id, ctx);
            assert_eq!(panes.number_of_shared_sessions(ctx), 1);
        });
    });
}

#[test]
fn test_update_session_visibility() {
    App::test((), |mut app| async move {
        // The options harness, not the plain `mock_pane_group` the other ported
        // tests use: this is the one test that turns on `PaneGroup::focus`, and
        // `update_session_visibility` bails unless `ctx.is_self_or_child_focused()`.
        // A `Workspace`-built pane group is not its window's root, so it is not
        // focused at that point and every session stays invisible. The pin's only
        // harness builds the pane group directly as the window's view, which is what
        // this harness does -- so use it here, exactly as the pin's version does.
        let pane_group = mock_pane_group_with_options(&mut app, Default::default());
        pane_group.update(&mut app, |panes, ctx| {
            // Assert that there is no active window.
            WindowManager::handle(ctx).read(ctx, |state, _| {
                assert_eq!(state.stage(), ApplicationStage::Starting);
                assert!(state.active_window().is_none());
            });

            fn visibility_matches(panes: &PaneGroup, expected: bool, ctx: &ViewContext<PaneGroup>) {
                for data in panes.panes_of::<TerminalPane>() {
                    let view = data.terminal_view(ctx).as_ref(ctx);
                    assert_eq!(
                        view.was_ever_visible(),
                        expected,
                        "View {} visibility was {}, expected {}",
                        data.terminal_view(ctx).id(),
                        view.was_ever_visible(),
                        expected
                    );
                }
            }

            // Add pane Left.
            panes.add_terminal_pane(Direction::Left, None, ctx);

            // Assert that neither of the panes are marked as visible (due
            // to the fact that the window is not active).
            visibility_matches(panes, false, ctx);

            let window_id = ctx.window_id();
            WindowManager::handle(ctx).update(ctx, |state, ctx| {
                state.overwrite_for_test(ApplicationStage::Active, Some(window_id));
                ctx.notify();
            });

            // Assert that both of the panes are still not marked as
            // visible, given the fact that the pane group is not focused.
            visibility_matches(panes, false, ctx);

            panes.focus(ctx);

            // Assert that both of the panes are now visible.
            visibility_matches(panes, true, ctx);
        })
    });
}

/// Mirrors the pin's `MockOptions`/`mock_pane_group(app, options)` pair. The
/// plain `mock_pane_group(app)` above (this file's harness for the first 12
/// ported tests) always builds a single default terminal pane via the
/// `Workspace`-level test harness (`mock_workspace`); the three tests below
/// each need a specific split-tree layout and/or exact window bounds instead,
/// so they build the `PaneGroup` directly.
///
/// This still goes through `PaneGroup::new_with_panes_layout` -- the same
/// constructor `Workspace::add_tab_with_pane_layout`
/// (`workspace/view.rs`) and `terminal_view_for_viewer`
/// (`terminal/view/shared_session/test_utils.rs`) use -- so
/// `workspace::view::tests::initialize_app`, which already registers every
/// singleton a `Workspace`-built `PaneGroup` needs, is sufficient here too,
/// without going through `Workspace` itself. `GlobalResourceHandles::mock`
/// (used the same way in `terminal_view_for_viewer`) supplies the
/// `tips_completed`/banner/`model_event_sender` handles the pin's version
/// sourced from its own ad hoc `app.add_model` calls plus
/// `ServerApiProvider` -- the fork's `new_with_panes_layout` has no
/// `ServerApiProvider` parameter to begin with (cloud dropped).
struct MockOptions {
    layout: PanesLayout,
    window_bounds: WindowBounds,
}

impl Default for MockOptions {
    fn default() -> Self {
        Self {
            layout: Default::default(),
            window_bounds: WindowBounds::ExactPosition(RectF::new(
                Vector2F::zero(),
                Vector2F::new(1024., 768.),
            )),
        }
    }
}

fn mock_pane_group_with_options(app: &mut App, options: MockOptions) -> ViewHandle<PaneGroup> {
    crate::workspace::view::tests::initialize_app(app);
    let global_resource_handles = crate::GlobalResourceHandles::mock(app);
    let (_, pane_group) =
        app.add_window_with_bounds(WindowStyle::NotStealFocus, options.window_bounds, |ctx| {
            PaneGroup::new_with_panes_layout(
                global_resource_handles.tips_completed.clone(),
                global_resource_handles
                    .user_default_shell_unsupported_banner_model_handle
                    .clone(),
                options.layout,
                Arc::new(HashMap::new()),
                global_resource_handles.model_event_sender.clone(),
                ctx,
            )
        });
    pane_group
}

#[test]
fn test_initial_widths_are_computed_correctly() {
    use launch_config::PaneTemplateType::*;

    App::test((), |mut app| async move {
        // Define a simple macro to help us create new leaf panes.
        macro_rules! leaf_pane {
            () => {
                PaneTemplate {
                    is_focused: None,
                    cwd: "".into(),
                    commands: vec![],
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                }
            };
        }

        // Pick an arbitrary initial window that isn't the same as the
        // fallback value.
        let window_width = 864.;
        let window_height = 636.;
        assert_ne!(window_width, FALLBACK_INITIAL_WINDOW_SIZE.x());
        assert_ne!(window_height, FALLBACK_INITIAL_WINDOW_SIZE.y());

        // Create a template that looks like the following, with each pane
        // numbered by its index in the pane group:
        //
        //  ---------------------
        //  |         0         |
        //  | __________________|
        //  |     1   |____2____|
        //  | ________|____3____|
        //  |   4  |   5  |  6  |
        //  |      |      |     |
        //  ---------------------
        let template = PaneBranchTemplate {
            split_direction: launch_config::SplitDirection::Vertical,
            panes: vec![
                leaf_pane!(),
                PaneBranchTemplate {
                    split_direction: launch_config::SplitDirection::Horizontal,
                    panes: vec![
                        leaf_pane!(),
                        PaneBranchTemplate {
                            split_direction: launch_config::SplitDirection::Vertical,
                            panes: vec![leaf_pane!(), leaf_pane!()],
                        },
                    ],
                },
                PaneBranchTemplate {
                    split_direction: launch_config::SplitDirection::Horizontal,
                    panes: vec![leaf_pane!(), leaf_pane!(), leaf_pane!()],
                },
            ],
        };

        let window_size = Vector2F::new(window_width, window_height);
        let pane_group = mock_pane_group_with_options(
            &mut app,
            MockOptions {
                layout: PanesLayout::Template(template),
                window_bounds: WindowBounds::ExactPosition(RectF::new(
                    Vector2F::zero(),
                    window_size,
                )),
            },
        );

        // Assert that the window created by the call to
        // `mock_pane_group_with_options` has the expected bounds.
        let window_id = app.read(|ctx| pane_group.window_id(ctx));
        app.update(|ctx| {
            assert_eq!(
                Some(window_size),
                ctx.window_bounds(&window_id).map(|rect| rect.size())
            );
        });

        let pane_group_width = window_width - 2.0 * workspace::WORKSPACE_PADDING;
        let pane_group_height =
            window_height - workspace::TOTAL_TAB_BAR_HEIGHT - 2.0 * workspace::WORKSPACE_PADDING;

        pane_group.read(&app, |pane_group, ctx| {
            // Make assertions about the expected widths of the various
            // panes.
            assert_eq!(
                pane_group
                    .terminal_view_at_pane_index(0, ctx)
                    .unwrap()
                    .as_ref(ctx)
                    .size_info()
                    .pane_width_px()
                    .as_f32(),
                pane_group_width,
                "Pane with index 0 had unexpected width!"
            );
            let half_width = (pane_group_width - tree::get_divider_thickness()) / 2.;
            for i in 1..=3 {
                assert_eq!(
                    pane_group
                        .terminal_view_at_pane_index(i, ctx)
                        .unwrap()
                        .as_ref(ctx)
                        .size_info()
                        .pane_width_px()
                        .as_f32(),
                    half_width,
                    "Pane with index {i} had unexpected width!"
                );
            }
            let one_third_width = (pane_group_width - (2. * tree::get_divider_thickness())) / 3.;
            for i in 4..=6 {
                assert_eq!(
                    pane_group
                        .terminal_view_at_pane_index(i, ctx)
                        .unwrap()
                        .as_ref(ctx)
                        .size_info()
                        .pane_width_px()
                        .as_f32(),
                    one_third_width,
                    "Pane with index {i} had unexpected width!"
                );
            }

            // Make assertions about the expected heights of the various
            // panes.
            let one_third_height = (pane_group_height - (2. * tree::get_divider_thickness())) / 3.;
            for i in (0..=1).chain(4..=6) {
                assert_eq!(
                    pane_group
                        .terminal_view_at_pane_index(i, ctx)
                        .unwrap()
                        .as_ref(ctx)
                        .size_info()
                        .pane_height_px()
                        .as_f32(),
                    one_third_height,
                    "Pane with index {i} had unexpected height!"
                );
            }
            let one_sixth_height = (pane_group_height - (5. * tree::get_divider_thickness())) / 6.;
            for i in 2..=3 {
                assert_eq!(
                    pane_group
                        .terminal_view_at_pane_index(i, ctx)
                        .unwrap()
                        .as_ref(ctx)
                        .size_info()
                        .pane_height_px()
                        .as_f32(),
                    one_sixth_height,
                    "Pane with index {i} had unexpected height!"
                );
            }
        });
    });
}

/// Tests that focusing two different panes in quick succession does not cause
/// an infinite loop of focus changes, as outlined in this PR's description:
/// https://github.com/warpdotdev/warp-internal/pull/8990
#[cfg_attr(windows, ignore = "TODO(CORE-3626)")]
#[test]
fn test_pane_focus_does_not_have_an_infinite_event_loop() {
    App::test((), |mut app| async move {
        // Create a pane group with two terminal panes that will fight for
        // focus.
        let mock_options = MockOptions {
            layout: PanesLayout::Template(PaneTemplateType::PaneBranchTemplate {
                split_direction: crate::launch_configs::launch_config::SplitDirection::Horizontal,
                panes: vec![
                    PaneTemplateType::PaneTemplate {
                        is_focused: Some(true),
                        cwd: "/".into(),
                        commands: vec![],
                        pane_mode: PaneMode::Terminal,
                        shell: None,
                    },
                    PaneTemplateType::PaneTemplate {
                        is_focused: None,
                        cwd: "/".into(),
                        commands: vec![],
                        pane_mode: PaneMode::Terminal,
                        shell: None,
                    },
                ],
            }),
            ..Default::default()
        };
        let pane_group = mock_pane_group_with_options(&mut app, mock_options);

        // The cycle requires that we are constantly trying to focus the input.
        // An active and long-running block causes focus to move to the
        // terminal instead of the input, so we need to wait until we've
        // finished bootstrapping to ensure no such block will exist.
        loop {
            let mut all_terminals_bootstrapped = true;
            pane_group.update(&mut app, |pane_group, ctx| {
                pane_group.for_all_terminal_panes(
                    |terminal_view, _ctx| {
                        let model = terminal_view.model.lock();
                        let active_block = model.block_list().active_block();
                        if active_block.bootstrap_stage()
                            != crate::terminal::model::bootstrap::BootstrapStage::PostBootstrapPrecmd
                            || active_block.is_active_and_long_running()
                        {
                            all_terminals_bootstrapped = false;
                        }
                    },
                    ctx,
                );
            });
            if all_terminals_bootstrapped {
                break;
            }
            // Return control back to the executor briefly so we can make
            // progress.
            futures_lite::future::yield_now().await;
        }

        pane_group.update(&mut app, |pane_group, ctx| {
            // Switch panes twice in quick succession.  We want to make
            // sure the test terminates and doesn't get into an infinite
            // loop.
            pane_group.navigate_next_pane(ctx);
            pane_group.navigate_next_pane(ctx);
        });
    });
}

/// A view to help us react to focus changes and know that they were processed
/// synchronously, not asynchronously (via an Effect::Event).
struct FocusDetectionView {
    pane_group: ViewHandle<PaneGroup>,
    new_focused_pane_id: Option<PaneId>,
}

impl FocusDetectionView {
    fn new(pane_group: ViewHandle<PaneGroup>, ctx: &mut ViewContext<Self>) -> Self {
        ctx.subscribe_to_view(&pane_group, |me, pane_group, event, ctx| {
            let Event::OpenPromptEditor = event else {
                return;
            };
            // This event is enqueued by us after the `Focus` effect, and so
            // by the time we receive it, application focus will have been
            // moved to the second pane, and (crucially) the pane group should
            // have updated its internal state accordingly (which is what we're
            // asserting here).

            let new_focused_pane_id = me
                .new_focused_pane_id
                .expect("should have set this already");
            pane_group.read(ctx, |pane_group, ctx| {
                assert_eq!(pane_group.focused_pane_id(ctx), new_focused_pane_id);
                assert_eq!(
                    pane_group.active_session_id(ctx),
                    new_focused_pane_id.as_terminal_pane_id()
                );
            });
        });
        Self {
            pane_group,
            new_focused_pane_id: None,
        }
    }
}

impl Entity for FocusDetectionView {
    type Event = ();
}

impl View for FocusDetectionView {
    fn ui_name() -> &'static str {
        "FocusDetectionView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        ChildView::new(&self.pane_group).finish()
    }
}

impl TypedActionView for FocusDetectionView {
    type Action = ();
}

/// This test ensures that a change in application focus causes the pane group
/// focused pane to update synchronously, without needing to wait for effect
/// flushing to occur.
///
/// The goal is to avoid situations where a delayed response to application
/// focus changes leads to an infinite loop of focusing and re-focusing two
/// different panes.
#[test]
fn test_focused_pane_is_synchronized_with_application_focus() {
    App::test((), |mut app| async move {
        crate::workspace::view::tests::initialize_app(&mut app);
        let global_resource_handles = crate::GlobalResourceHandles::mock(&mut app);

        // Create a pane group with two terminal panes, so that we can move
        // focus and observe the effects.
        let panes_layout = PanesLayout::Template(PaneTemplateType::PaneBranchTemplate {
            split_direction: crate::launch_configs::launch_config::SplitDirection::Horizontal,
            panes: vec![
                PaneTemplateType::PaneTemplate {
                    is_focused: Some(true),
                    cwd: "/".into(),
                    commands: vec![],
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
                PaneTemplateType::PaneTemplate {
                    is_focused: None,
                    cwd: "/".into(),
                    commands: vec![],
                    pane_mode: PaneMode::Terminal,
                    shell: None,
                },
            ],
        });

        let (_, root_view) = app.add_window_with_bounds(
            WindowStyle::NotStealFocus,
            WindowBounds::Default,
            move |ctx| {
                let pane_group = ctx.add_typed_action_view(|ctx| {
                    PaneGroup::new_with_panes_layout(
                        global_resource_handles.tips_completed.clone(),
                        global_resource_handles
                            .user_default_shell_unsupported_banner_model_handle
                            .clone(),
                        panes_layout,
                        Arc::new(HashMap::new()),
                        global_resource_handles.model_event_sender.clone(),
                        ctx,
                    )
                });

                FocusDetectionView::new(pane_group, ctx)
            },
        );
        let pane_group = root_view.read(&app, |root_view, _ctx| root_view.pane_group.clone());

        let (focused_pane_id, active_session_id) = pane_group.read(&app, |pane_group, ctx| {
            (
                pane_group.focused_pane_id(ctx),
                pane_group.active_session_id(ctx),
            )
        });

        let second_pane_id = pane_group.read(&app, |pane_group, _ctx| {
            pane_group
                .pane_ids()
                .find(|pane_id| *pane_id != focused_pane_id)
                .expect("should have more than one pane")
        });

        // Verify that the "second" pane is not focused or active.
        assert_ne!(focused_pane_id, second_pane_id);
        assert_ne!(active_session_id, second_pane_id.as_terminal_pane_id());

        root_view.update(&mut app, |root_view, _ctx| {
            root_view.new_focused_pane_id = Some(second_pane_id);
        });

        pane_group.update(&mut app, |pane_group, ctx| {
            // First, request a change of application focus to the second
            // pane's terminal view.
            pane_group
                .terminal_view_from_pane_id(second_pane_id, ctx)
                .expect("second pane is a terminal pane")
                .update(ctx, |_terminal_view, ctx| {
                    ctx.focus_self();
                });

            // Second, emit an event on the pane group to trigger assertion
            // logic in the FocusDetectionView.  This event effect is enqueued after
            // the focus effect but before the focus effect is processed, meaning
            // it will observe any changes that occurred synchronously as part
            // of the focus effect but will _not_ observe any changes that result
            // from events dispatched during focus handling.
            //
            // We use `OpenPromptEditor` because we can be confident that
            // nothing else above may have emitted this event.
            //
            // IMPORTANT: This MUST be emitted in the same pane group update
            // during which we focus the terminal view, to ensure that the
            // effect queue doesn't get processed or further modified before we
            // enqueue this event on the effect queue.
            ctx.emit(Event::OpenPromptEditor);
        });
    });
}

/// APP-5243: closing a file pane only hides it while undo-close is available, and the same view is
/// reattached without reopening its file. Releasing the file on close would therefore leave a
/// restored pane rendering content that can never update again. The file is released only once the
/// pane is permanently discarded.
///
/// Fork drift from the pin:
/// - The pin's `mock_pane_group` is its own harness; this fork's wraps
///   `workspace::view::tests::{initialize_app, mock_workspace}` (see the helper above). The two
///   lines that build the group are inlined here so `FileModel` -- which the workspace harness does
///   not register -- can be added before the workspace window is built, exactly where the pin adds
///   it.
/// - `PaneGroup::file_notebook_panes` does not exist in this fork (it is a separate, unrelated pin
///   gap; the fork has only `code_panes`). The test module is a child of `pane_group`, so it uses
///   the same private `panes_of::<FilePane>()` + `id()` + `file_view()` the pin's accessor is
///   defined as, which is the identical lookup.
/// - This fork's `FilePane::new` takes `Option<PathBuf>` rather than the pin's
///   `Option<LocalOrRemotePath>`; a local path is passed directly.
#[cfg(feature = "local_fs")]
#[test]
fn test_undo_close_keeps_a_file_pane_watching_its_file() {
    use warp_files::FileModel;

    let _undo_closed_panes = FeatureFlag::UndoClosedPanes.override_enabled(true);

    App::test((), |mut app| async move {
        crate::workspace::view::tests::initialize_app(&mut app);
        app.add_singleton_model(FileModel::new);
        let workspace = crate::workspace::view::tests::mock_workspace(&mut app);
        let pane_group = workspace
            .read(&app, |workspace, _| workspace.tab_views().next().cloned())
            .expect("mock_workspace has an initial tab");

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("notes.md");
        std::fs::write(&path, "# before").expect("write file");

        pane_group.update(&mut app, |panes, ctx| {
            let pane = FilePane::new(Some(path.clone()), None, None, ctx);
            panes.add_pane_with_direction(Direction::Right, pane, true, ctx);
        });

        let (file_pane_id, file_view) = pane_group.read(&app, |panes, ctx| {
            panes
                .panes_of::<FilePane>()
                .map(|pane| (pane.id(), pane.file_view(ctx)))
                .next()
                .expect("the file pane should exist")
        });

        // Let the read settle so the pane is fully loaded and watching.
        let loaded = file_view.update(&mut app, |view, ctx| {
            let file_id = view.file_id_for_test().expect("the file should be open");
            let future_handle = FileModel::as_ref(ctx)
                .get_future_handle(file_id)
                .expect("Loading future should be present");
            ctx.await_spawned_future(future_handle.future_id())
        });
        loaded.await;

        // Close the way the pane header's close button does, which is the path that reaches
        // `BackingView::close` before the pane group hides the pane.
        file_view.update(&mut app, BackingView::close);
        pane_group.update(&mut app, |panes, ctx| {
            assert!(
                panes.is_pane_hidden_for_close(file_pane_id),
                "closing should hide the pane for undo rather than discard it"
            );
            assert!(
                panes.restore_closed_pane(file_pane_id, ctx),
                "the closed pane should be restorable"
            );
        });

        app.read(|ctx| {
            let file_id = file_view
                .as_ref(ctx)
                .file_id_for_test()
                .expect("a restored pane should still hold its file open");
            assert!(
                FileModel::as_ref(ctx).file_path(file_id).is_some(),
                "a restored pane should still be tracked by the file model"
            );
        });

        // Permanently discarding the pane does release it.
        pane_group.update(&mut app, |panes, ctx| {
            panes.close_pane(file_pane_id, ctx);
            panes.cleanup_closed_pane(file_pane_id, ctx);
        });

        app.read(|ctx| {
            assert!(
                file_view.as_ref(ctx).file_id_for_test().is_none(),
                "a permanently discarded pane should release its file"
            );
        });
    });
}

/// Regression test for the missing `View::child_view_ids` overrides.
///
/// `AppContext::transfer_view_tree_to_window` (used by cross-window tab drag
/// and by tearing a tab into a new window) walks the render-time parent graph
/// plus each view's `child_view_ids`. Views that a `PaneGroup`/`PaneView` owns
/// but does not currently render -- the pane group's banner, and every
/// non-active view in a pane's `pane_stack` -- are reachable *only* through
/// `child_view_ids`, so without these overrides they stay behind in the source
/// window and later trip a "circular view reference" panic when the new window
/// renders them.
///
/// `crates/warpui_core` already covers the walk itself, but only against its
/// own mock views, which is exactly why the production overrides being absent
/// went unnoticed. This asserts the *real* `PaneGroup`/`PaneView` report their
/// children, so it fails against the default `Vec::new()` impl.
#[test]
fn test_child_view_ids_reports_owned_but_unrendered_views() {
    App::test((), |mut app| async move {
        let pane_group = mock_pane_group(&mut app);

        pane_group.read(&app, |pane_group, ctx| {
            // The pane group's banner is only rendered while it is open, so it
            // is absent from the render-time parent graph.
            let group_children = View::child_view_ids(pane_group, ctx);
            assert!(
                group_children.contains(&pane_group.user_default_shell_changed_banner.id()),
                "PaneGroup::child_view_ids must report the shell-changed banner, \
                 or a cross-window transfer orphans it"
            );

            let terminal_panes = pane_group.panes_of::<TerminalPane>().collect_vec();
            assert_eq!(terminal_panes.len(), 1);

            let pane_view_handle = terminal_panes[0].pane_view();
            let pane_view = pane_view_handle.as_ref(ctx);
            let pane_children = View::child_view_ids(pane_view, ctx);

            assert!(
                pane_children.contains(&pane_view.header().id()),
                "PaneView::child_view_ids must report its header"
            );

            let backing_view_ids = pane_view
                .pane_stack()
                .as_ref(ctx)
                .views()
                .map(|view| view.id())
                .collect_vec();
            assert!(
                !backing_view_ids.is_empty(),
                "a mock terminal pane should have at least one backing view"
            );
            for backing_view_id in backing_view_ids {
                assert!(
                    pane_children.contains(&backing_view_id),
                    "PaneView::child_view_ids must report every pane_stack view, \
                     or a cross-window transfer orphans the non-active ones"
                );
            }
        });
    });
}
