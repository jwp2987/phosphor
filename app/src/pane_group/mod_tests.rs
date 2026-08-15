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
//! - A handful more depend on a *lazy* hidden-child-agent-pane restoration
//!   mechanism (`restore_missing_child_agent_panes_for_parent`,
//!   `ensure_hidden_child_agent_pane_for_conversation`, the pin's
//!   `enter_agent_view_for_conversation` test helper) that this fork does not
//!   have -- it only restores child panes *eagerly*, once, at
//!   `PaneGroup::new_internal`/`reattach_panes` time
//!   (`create_missing_child_agent_panes`).
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
