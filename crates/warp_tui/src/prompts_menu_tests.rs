//! Construction/closed-state tests for [`TuiPromptsMenuModel`]. Opening the menu
//! reads the `ObjectStoreModel` singleton (a heavy registration chain), so the
//! open/list/accept behavior is exercised end-to-end via the PTY harness instead
//! (see `crates/warp_tui/scripts/tui_harness.py`).

use warp::editor::CodeEditorModel;
use warp::tui_export::Appearance;
use warpui_core::App;

use super::*;
use crate::input_suggestions_mode::TuiInputSuggestionsModeModel;
use crate::test_fixtures::add_test_semantic_selection;

#[test]
fn new_menu_is_closed() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        app.update(|ctx| {
            add_test_semantic_selection(ctx);
            let editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|ctx| TuiPromptsMenuModel::new(editor, mode, ctx));
            assert!(!menu.as_ref(ctx).is_open(ctx));
            assert!(menu.as_ref(ctx).snapshot(ctx).is_none());
        });
    });
}
