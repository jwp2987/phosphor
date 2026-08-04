use warp::editor::CodeEditorModel;
use warp::tui_export::Appearance;
use warpui_core::App;

use super::*;
use crate::test_fixtures::add_test_semantic_selection;

fn row(id: &str, is_selectable: bool) -> TuiModelMenuRow {
    TuiModelMenuRow {
        id: id.into(),
        title: id.to_owned(),
        is_selectable,
        key_connected: false,
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
