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

use std::collections::HashMap;

use warpui::App;

use crate::notebooks::notebook::NotebookView;

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
            assert_eq!(panes.visible_pane_count(), initial_visible_count);
            assert!(panes.has_pane_id(child_pane_id.into()));

            // The new child pane should remain hidden and not affect visible ordering.
            assert_eq!(panes.pane_id_by_index(0), Some(parent_pane_id));
            assert_eq!(panes.pane_id_by_index(1), None);

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
