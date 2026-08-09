//! Tests for [`TuiCompletionsMenuModel`] and the pure span-replacement helper
//! [`apply_completion_replacement`].

use warp::tui_export::TuiCompletionCandidate;
use warpui_core::App;

use super::*;
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};

fn rows(pairs: &[(&str, &str)]) -> Vec<TuiCompletionCandidate> {
    pairs
        .iter()
        .map(|(display, replacement)| TuiCompletionCandidate {
            display: (*display).to_owned(),
            replacement: (*replacement).to_owned(),
            description: None,
            is_directory: false,
        })
        .collect()
}

// ── apply_completion_replacement ────────────────────────────────────────────

#[test]
fn replacement_at_end_of_buffer() {
    assert_eq!(
        apply_completion_replacement("git che", "checkout", &(4..7), false),
        Some("git checkout".to_owned())
    );
}

#[test]
fn replacement_in_middle_keeps_tail() {
    // Replace "sr" (bytes 3..5) with "src" in "cd sr/foo".
    assert_eq!(
        apply_completion_replacement("cd sr/foo", "src", &(3..5), false),
        Some("cd src/foo".to_owned())
    );
}

#[test]
fn empty_span_inserts_at_offset() {
    assert_eq!(
        apply_completion_replacement("ls ", "-la", &(3..3), false),
        Some("ls -la".to_owned())
    );
}

#[test]
fn stale_span_out_of_bounds_is_rejected() {
    assert_eq!(
        apply_completion_replacement("git", "checkout", &(4..8), false),
        None
    );
}

#[test]
fn inverted_span_is_rejected() {
    assert_eq!(
        apply_completion_replacement("git ch", "checkout", &(6..4), false),
        None
    );
}

#[test]
fn non_char_boundary_span_is_rejected() {
    // "é" is two bytes; a span landing inside it must be rejected, not panic.
    assert_eq!(apply_completion_replacement("é", "x", &(0..1), false), None);
}

#[test]
fn append_space_inserts_a_trailing_space() {
    assert_eq!(
        apply_completion_replacement("git che", "checkout", &(4..7), true),
        Some("git checkout ".to_owned())
    );
}

// ── TuiCompletionsMenuModel ─────────────────────────────────────────────────

#[test]
fn show_opens_with_rows_and_sets_mode() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));

            let opened = menu.update(ctx, |m, ctx| {
                m.show(
                    rows(&[("checkout", "checkout"), ("cherry-pick", "cherry-pick")]),
                    4..7,
                    false,
                    ctx,
                )
            });
            assert!(opened);
            assert!(menu.as_ref(ctx).is_open(ctx));
            assert_eq!(
                suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Completions
            );
            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu is open");
            assert_eq!(snapshot.rows.len(), 2);
            assert_eq!(snapshot.selected_index, Some(0));
            assert_eq!(snapshot.rows[0].title, "checkout");
        });
    });
}

#[test]
fn show_with_no_rows_does_not_open() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));

            let opened = menu.update(ctx, |m, ctx| m.show(Vec::new(), 0..0, false, ctx));
            assert!(!opened);
            assert!(!menu.as_ref(ctx).is_open(ctx));
            assert_eq!(
                suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Closed
            );
        });
    });
}

#[test]
fn select_next_cycles_selection() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));
            menu.update(ctx, |m, ctx| {
                m.show(
                    rows(&[("a", "a"), ("b", "b"), ("c", "c")]),
                    0..1,
                    false,
                    ctx,
                )
            });

            menu.update(ctx, |m, ctx| m.select_next(ctx));
            assert_eq!(
                menu.as_ref(ctx).snapshot(ctx).unwrap().selected_index,
                Some(1)
            );
        });
    });
}

#[test]
fn accept_selected_returns_replacement_and_span_then_dismisses() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));
            menu.update(ctx, |m, ctx| {
                m.show(rows(&[("checkout", "checkout")]), 4..7, false, ctx)
            });

            let accepted = menu
                .update(ctx, |m, ctx| m.accept_selected(ctx))
                .expect("accepts");
            assert_eq!(accepted.replacement, "checkout");
            assert_eq!(accepted.span, 4..7);
            assert!(!accepted.append_space);
            // Accepting closes the popup.
            assert!(!menu.as_ref(ctx).is_open(ctx));
            assert_eq!(
                suggestions_mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::Closed
            );
        });
    });
}

#[test]
fn accept_selected_appends_space_at_buffer_end_but_not_for_directories() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));
            let candidates = vec![
                TuiCompletionCandidate {
                    display: "checkout".to_owned(),
                    replacement: "checkout".to_owned(),
                    description: None,
                    is_directory: false,
                },
                TuiCompletionCandidate {
                    display: "src/".to_owned(),
                    replacement: "src/".to_owned(),
                    description: None,
                    is_directory: true,
                },
            ];
            menu.update(ctx, |m, ctx| m.show(candidates, 4..7, true, ctx));

            menu.update(ctx, |m, ctx| m.select_next(ctx));
            let accepted = menu
                .update(ctx, |m, ctx| m.accept_selected(ctx))
                .expect("accepts");
            assert!(
                !accepted.append_space,
                "a directory completion must not append a trailing space"
            );
        });
    });
}

#[test]
fn dismiss_closes_popup() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let suggestions_mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|_| TuiCompletionsMenuModel::new(suggestions_mode.clone()));
            menu.update(ctx, |m, ctx| m.show(rows(&[("a", "a")]), 0..1, false, ctx));

            menu.update(ctx, |m, ctx| m.dismiss(ctx));
            assert!(!menu.as_ref(ctx).is_open(ctx));
        });
    });
}
