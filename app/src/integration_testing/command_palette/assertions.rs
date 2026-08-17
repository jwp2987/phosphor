use crate::integration_testing::view_getters::{command_palette_view, workspace_view};
use warpui::async_assert;
use warpui::integration::{AssertionCallback, AssertionOutcome};

/// Asserts that the command palette is currently open.
pub fn assert_command_palette_is_open() -> AssertionCallback {
    Box::new(move |app, window_id| {
        let workspace = workspace_view(app, window_id);

        workspace.read(app, |workspace, _| {
            async_assert!(
                workspace.is_palette_open(),
                "Expected palette to be open, but it was closed"
            )
        })
    })
}

/// Asserts that the command palette is currently closed.
pub fn assert_command_palette_is_closed() -> AssertionCallback {
    Box::new(move |app, window_id| {
        let workspace = workspace_view(app, window_id);

        workspace.read(app, |workspace, _| {
            async_assert!(
                !workspace.is_palette_open(),
                "Expected palette to be closed, but it was open"
            )
        })
    })
}

/// Asserts that the command palette currently has at least one search result.
pub fn assert_command_palette_has_results() -> AssertionCallback {
    Box::new(move |app, window_id| {
        let palette = command_palette_view(app, window_id);

        palette.read(app, |palette, ctx| {
            async_assert!(
                palette.search_results(ctx).next().is_some(),
                "Expected command palette to have results, but it was empty"
            )
        })
    })
}

/// Asserts the command palette's SELECTED result is the action the caller asked
/// for -- i.e. the thing Enter will actually run.
///
/// `assert_command_palette_has_results` only checks the list is non-empty, so a
/// step that typed an action name and pressed Enter would silently run whatever
/// fuzzy search happened to rank first. `test_pane_group_state_multi_pane`
/// failed 4 runs in 5 that way, and instrumentation showed its workspace action
/// was never dispatched at all -- a different palette entry ran instead.
pub fn assert_command_palette_selection_matches(action: &'static str) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let palette = command_palette_view(app, window_id);
        palette.read(app, |palette, ctx| {
            let selected = palette
                .selected_search_result(ctx)
                .map(|result| result.accessibility_label());
            match selected {
                Some(label) => async_assert!(
                    label.to_lowercase().contains(&action.to_lowercase()),
                    "command palette selection is {label:?}, expected it to match {action:?} -- \
                     pressing enter would run the wrong action"
                ),
                None => AssertionOutcome::failure(format!(
                    "command palette has no selected result while looking for {action:?}"
                )),
            }
        })
    })
}
