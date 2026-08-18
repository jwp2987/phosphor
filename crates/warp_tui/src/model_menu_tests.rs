use warp::editor::CodeEditorModel;
use warp::tui_export::Appearance;
use warpui_core::App;

use super::*;
use crate::test_fixtures::add_test_semantic_selection;

fn row(id: &str, is_selectable: bool) -> TuiModelMenuRow {
    row_with_state(id, is_selectable, false, false)
}

/// The pin's four-argument `row`, kept under a separate name so the
/// two-argument call sites in the selection tests below stay as they are.
/// `key_connected` is this fork's field name for the pin's `is_key_connected`,
/// and this fork's row carries no `discount_percentage`.
fn row_with_state(
    id: &str,
    is_selectable: bool,
    key_connected: bool,
    is_profile_default: bool,
) -> TuiModelMenuRow {
    TuiModelMenuRow {
        id: id.into(),
        title: id.to_owned(),
        is_selectable,
        key_connected,
        is_profile_default,
    }
}

#[test]
fn empty_query_prefers_active_model_and_filtered_query_prefers_best_match() {
    let rows = vec![row("auto", true), row("gpt-4", true), row("gpt-5", true)];

    assert_eq!(
        preferred_selection_index(&rows, &LLMId::from("gpt-4"), true),
        Some(1)
    );
    assert_eq!(
        preferred_selection_index(&rows, &LLMId::from("gpt-4"), false),
        Some(2)
    );
}

#[test]
fn model_selection_skips_disabled_rows() {
    let rows = vec![
        row("auto", true),
        row("gpt-5", true),
        row("disabled", false),
    ];

    assert_eq!(
        preferred_selection_index(&rows, &LLMId::from("disabled"), true),
        Some(1)
    );
    assert_eq!(
        preferred_selection_index(&rows, &LLMId::from("auto"), false),
        Some(1)
    );
}

#[test]
fn snapshot_shows_key_connected_suffix_only_for_connected_models() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        app.update(|ctx| {
            add_test_semantic_selection(ctx);
            let editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            mode.update(ctx, |mode, ctx| {
                mode.set_mode(TuiInputSuggestionsMode::ModelSelector, ctx);
            });
            let menu = ctx.add_model(|_| {
                TuiModelMenuModel::new_for_test(
                    editor,
                    mode,
                    vec![
                        (LLMId::from("byop:p1:model-a"), true, true),
                        (LLMId::from("byop:p1:model-b"), true, false),
                    ],
                    0,
                )
            });

            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu should be open");
            assert_eq!(snapshot.rows.len(), 2);
            assert_eq!(
                snapshot.rows[0].state_suffix.as_deref(),
                Some(KEY_CONNECTED_SUFFIX),
                "connected model must show the key-connected suffix"
            );
            assert_eq!(
                snapshot.rows[1].state_suffix, None,
                "model without a connected key must not show the suffix"
            );
        });
    });
}

/// #587: the suffix above reached the snapshot correctly all along -- it was the *rendering*
/// that dropped it, because `menu_result_row` emitted `state_suffix` from inside the
/// `description` block and a selectable model has no description. This runs the real menu
/// snapshot through the real `render_inline_menu` and asserts on the produced lines.
#[test]
fn selectable_key_connected_model_renders_the_suffix() {
    use warpui_core::elements::tui::{TuiBufferExt, TuiRect};
    use warpui_core::presenter::tui::TuiPresenter;

    use crate::inline_menu::render_inline_menu;
    use crate::tui_builder::TuiUiBuilder;

    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        app.update(|ctx| {
            add_test_semantic_selection(ctx);
            let editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            mode.update(ctx, |mode, ctx| {
                mode.set_mode(TuiInputSuggestionsMode::ModelSelector, ctx);
            });
            let menu = ctx.add_model(|_| {
                TuiModelMenuModel::new_for_test(
                    editor,
                    mode,
                    vec![
                        (LLMId::from("byop:p1:model-a"), true, true),
                        (LLMId::from("byop:p1:model-b"), true, false),
                    ],
                    0,
                )
            });

            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu should be open");
            let mut presenter = TuiPresenter::new();
            let frame = presenter.present_element(
                render_inline_menu(&snapshot, &TuiUiBuilder::from_app(ctx)),
                TuiRect::new(0, 0, 60, 3),
                ctx,
            );
            let lines = frame
                .buffer
                .to_lines()
                .into_iter()
                .map(|line| line.trim_end().to_owned())
                .collect::<Vec<_>>();

            assert!(
                lines
                    .iter()
                    .any(|line| line == "byop:p1:model-a  (key connected)"),
                "selectable key-connected model must render the marker, got {lines:?}"
            );
            assert!(
                lines.iter().any(|line| line == "byop:p1:model-b"),
                "model without a connected key must render no marker, got {lines:?}"
            );
        });
    });
}

/// Ported from the pin (`42effe840`). Both state badges share the single
/// `state_suffix` slot the inline-menu row model provides, so the profile
/// default has to fold into it rather than take a slot of its own: a row that
/// is *both* the profile default and key-connected shows both, in the pin's
/// order (profile default first).
///
/// This is the unit-level half of the badge's coverage. The live half --
/// resolving `is_profile_default` from real `LLMPreferences` against a real
/// session -- is `terminal_session_view_tests::model_menu_labels_the_profile_default_model`,
/// because `new_for_test` deliberately hard-codes `is_profile_default: false`
/// (see its doc comment).
#[test]
fn snapshot_marks_the_profile_default_model() {
    let default = snapshot_row(&row_with_state("auto", true, false, true));
    assert_eq!(default.state_suffix.as_deref(), Some("(default)"));

    let connected_default = snapshot_row(&row_with_state("gpt-5", true, true, true));
    assert_eq!(
        connected_default.state_suffix.as_deref(),
        Some("(default) (key connected)")
    );
}
