//! Most of `TuiApiKeysMenuModel`'s behavior (open/list/accept persisting through
//! `AgentProviderSecrets`) reads app-side singletons (`AISettings`, `AgentProviderSecrets`)
//! that the lightweight fixture the older tests here use does not provide -- see
//! `crate::exchange_menu` / `crate::profile_menu` tests for the same tradeoff. That
//! end-to-end behavior is instead exercised via the PTY harness
//! (`crates/warp_tui/scripts/tui_harness.py`) and, on the app side, via
//! `app/src/ai/agent_providers/mod_test.rs`'s `tui_*_agent_provider_*` tests (which cover the
//! actual persistence: adding a key, clearing it, and the connected-key predicate).
//!
//! What *is* free of that dependency -- and so tested directly here -- is
//! [`build_provider_rows`], the pure row-building/filtering logic this menu's `refresh_rows`
//! delegates to.
//!
//! The input-ownership tests at the bottom (ported from the pin for #599) do need those
//! singletons -- leaving key entry runs `refresh_rows` -- so they provision the full
//! `register_tui_session_view_test_singletons` fixture, which registers `AISettings`,
//! `AgentProviderSecrets` and a no-op secure storage. They assert only ownership and buffer
//! state, never a persisted key: that stays with the app-side tests above.

use warp::editor::CodeEditorModel;
use warp::tui_export::{Appearance, TuiApiKeyProvider, register_tui_session_view_test_singletons};
use warpui_core::App;

// `use super::*` also carries the parent module's imports into scope here: `ModelHandle`,
// `AppContext`, and the `CoreEditorModel` trait whose `user_insert`/`clear_buffer` the
// ownership tests below drive the shared editor with.
use super::*;
use crate::inline_menu::TuiInlineMenuInputOwnership;
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};
use crate::test_fixtures::add_test_semantic_selection;

fn provider(provider_id: &str, display_name: &str, has_key: bool) -> TuiApiKeyProvider {
    TuiApiKeyProvider {
        provider_id: provider_id.to_owned(),
        display_name: display_name.to_owned(),
        api_type_label: "OpenAI",
        has_key,
    }
}

#[test]
fn new_menu_is_closed() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        app.update(|ctx| {
            add_test_semantic_selection(ctx);
            let editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let menu = ctx.add_model(|ctx| TuiApiKeysMenuModel::new(editor, mode, ctx));
            assert!(!menu.as_ref(ctx).is_open(ctx));
            assert!(menu.as_ref(ctx).snapshot(ctx).is_none());
        });
    });
}

#[test]
fn selecting_a_provider_row_switches_into_key_entry_for_that_provider() {
    // Only the List -> EnteringKey transition is exercised here: it's pure UI-state (no
    // AgentProviderSecrets/AISettings touch), unlike the Clear action or an EnteringKey submit,
    // which do persist through AgentProviderSecrets and so need the app-side harness -- see
    // `app/src/ai/agent_providers/mod_test.rs` for those.
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        app.update(|ctx| {
            add_test_semantic_selection(ctx);
            let editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            mode.update(ctx, |mode, ctx| {
                mode.set_mode(TuiInputSuggestionsMode::ApiKeys, ctx);
            });
            let menu = ctx.add_model(|_| {
                TuiApiKeysMenuModel::new_for_test(
                    editor,
                    mode,
                    vec![("Local Ollama", false), ("DeepSeek Official", true)],
                )
            });

            // Before accepting: the list shows both providers.
            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu should be open");
            assert_eq!(snapshot.rows.len(), 2);
            assert_eq!(snapshot.selected_index, Some(0));

            // Move to and accept the second row -> switches into key entry for it.
            menu.update(ctx, |menu, ctx| {
                menu.select_next(ctx);
                menu.accept_selected(ctx);
            });

            let snapshot = menu
                .as_ref(ctx)
                .snapshot(ctx)
                .expect("menu should still be open, now entering a key");
            assert_eq!(
                snapshot.header.and_then(|h| h.title),
                Some("API key · DeepSeek Official".to_owned())
            );
            assert!(
                snapshot.rows.is_empty(),
                "key-entry sub-state has no row list"
            );
        });
    });
}

#[test]
fn select_at_snapshot_index_and_scroll_by_delta_update_the_list() {
    // Both are pure `List`-state operations (no `AgentProviderSecrets`/`AISettings` touch), so
    // they're safe to exercise directly here -- unlike `accept_selected`'s Clear/EnteringKey
    // paths, which need the app-side harness (see the module docs above).
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        app.update(|ctx| {
            add_test_semantic_selection(ctx);
            let editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            mode.update(ctx, |mode, ctx| {
                mode.set_mode(TuiInputSuggestionsMode::ApiKeys, ctx);
            });
            let menu = ctx.add_model(|_| {
                TuiApiKeysMenuModel::new_for_test(
                    editor,
                    mode,
                    vec![("Local Ollama", false), ("DeepSeek Official", true)],
                )
            });

            // Clicking the second row selects it directly, without stepping through it.
            let selected = menu.update(ctx, |menu, ctx| menu.select_at_snapshot_index(1, ctx));
            assert!(selected, "row 1 is in bounds and should be selectable");
            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu should be open");
            assert_eq!(snapshot.selected_index, Some(1));

            // An out-of-bounds index is rejected and leaves the selection untouched.
            let selected = menu.update(ctx, |menu, ctx| menu.select_at_snapshot_index(99, ctx));
            assert!(!selected, "index 99 is out of bounds for a 2-row list");
            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu should still be open");
            assert_eq!(snapshot.selected_index, Some(1));

            // Scrolling must not itself be treated as a bug if it doesn't move a 2-row list that
            // fits entirely within the viewport -- this only checks that the call is safe to make
            // (no panic) while the menu remains open.
            menu.update(ctx, |menu, ctx| menu.scroll_by_delta(1, ctx));
            assert!(menu.as_ref(ctx).snapshot(ctx).is_some(), "menu should still be open");
        });
    });
}

#[test]
fn build_provider_rows_adds_a_clear_row_only_for_keyed_providers() {
    let rows = build_provider_rows(
        vec![
            provider("p1", "Local Ollama", false),
            provider("p2", "DeepSeek Official", true),
        ],
        "",
    );

    // p1 (no key): just its Edit row. p2 (keyed): Edit row + Clear row.
    assert_eq!(rows.len(), 3);

    assert_eq!(rows[0].title, "Local Ollama");
    assert_eq!(rows[0].state_suffix.as_deref(), Some("(no key)"));
    assert!(matches!(&rows[0].action, TuiApiKeysRowAction::Edit(id) if id == "p1"));

    assert_eq!(rows[1].title, "DeepSeek Official");
    assert_eq!(rows[1].state_suffix.as_deref(), Some("(key connected)"));
    assert!(matches!(&rows[1].action, TuiApiKeysRowAction::Edit(id) if id == "p2"));

    assert_eq!(rows[2].title, "Clear key · DeepSeek Official");
    assert!(matches!(&rows[2].action, TuiApiKeysRowAction::Clear(id) if id == "p2"));
}

#[test]
fn build_provider_rows_filters_by_case_insensitive_display_name() {
    let providers = vec![
        provider("p1", "Local Ollama", false),
        provider("p2", "DeepSeek Official", true),
    ];

    let rows = build_provider_rows(providers.clone(), "deepseek");
    assert_eq!(rows.len(), 2, "matching provider plus its Clear-key row");
    assert_eq!(rows[0].title, "DeepSeek Official");

    let rows = build_provider_rows(providers, "nonexistent");
    assert!(rows.is_empty());
}

#[test]
fn build_provider_rows_empty_input_lists_everything() {
    let rows = build_provider_rows(vec![provider("p1", "Local Ollama", false)], "   ");
    assert_eq!(rows.len(), 1, "whitespace-only query must not filter anything out");
}

// ── Input ownership (#599) ────────────────────────────────────────────────────
//
// Ported from the pin (`42effe840`) `crates/warp_tui/src/api_keys_menu_tests.rs`. The pin's
// three ownership tests map onto this fork's states as `Browsing` -> `List`,
// `EditingProvider` -> `EnteringKey`; its `ConnectingGrok` state has no counterpart here
// (`DECLINED.md` #319). Assertions about upstream's fixed provider catalog, its
// Warp-credit-fallback row and its `TuiApiKeysFooter` are dropped with the features they
// belong to, not weakened: this fork has no such rows and no footer type.

/// Provisions the full session-view fixture and returns the shared editor, the mode model,
/// and an already-open menu seeded with `rows`.
///
/// The heavier fixture (rather than the lightweight one the tests above use) is what lets
/// key entry be left at all: both submitting and cancelling end in `refresh_rows`, which
/// reads the configured providers through `tui_list_agent_provider_keys` and so needs
/// `AISettings` plus `AgentProviderSecrets`.
fn add_seeded_menu(
    app: &mut App,
    rows: Vec<(&str, bool)>,
) -> (
    ModelHandle<CodeEditorModel>,
    ModelHandle<TuiInputSuggestionsModeModel>,
    ModelHandle<TuiApiKeysMenuModel>,
) {
    register_tui_session_view_test_singletons(app);
    app.update(|ctx| {
        add_test_semantic_selection(ctx);
        let editor = ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));
        let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
        mode.update(ctx, |mode, ctx| {
            mode.set_mode(TuiInputSuggestionsMode::ApiKeys, ctx);
        });
        let menu = ctx
            .add_model(|_| TuiApiKeysMenuModel::new_for_test(editor.clone(), mode.clone(), rows));
        (editor, mode, menu)
    })
}

#[test]
fn changing_the_shared_menu_mode_deactivates_api_keys_state() {
    App::test((), |mut app| async move {
        let (editor, mode, menu) = add_seeded_menu(&mut app, vec![("Local Ollama", false)]);
        editor.update(&mut app, |editor, ctx| editor.user_insert("query", ctx));
        mode.update(&mut app, |mode, ctx| {
            mode.set_mode(TuiInputSuggestionsMode::ModelSelector, ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                mode.as_ref(ctx).mode(),
                TuiInputSuggestionsMode::ModelSelector
            );
            assert!(!menu.as_ref(ctx).is_open(ctx));
            // A menu that is not the active mode never owns the editor, whatever sub-state it
            // is parked in -- otherwise a stale menu could mask (or unmask) the composer.
            assert_eq!(
                menu.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::Composer
            );
        });
    });
}

#[test]
fn listing_providers_owns_the_input_as_plain_text() {
    App::test((), |mut app| async move {
        let (_, mode, menu) = add_seeded_menu(
            &mut app,
            vec![("Local Ollama", false), ("DeepSeek Official", true)],
        );
        app.read(|ctx| {
            assert_eq!(mode.as_ref(ctx).mode(), TuiInputSuggestionsMode::ApiKeys);
            // The list's buffer is a search query, not a credential: it stays readable.
            assert_eq!(
                menu.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuPlainText
            );
            let snapshot = menu.as_ref(ctx).snapshot(ctx).expect("menu should be open");
            assert_eq!(
                snapshot
                    .rows
                    .iter()
                    .map(|row| row.title.as_str())
                    .collect::<Vec<_>>(),
                vec!["Local Ollama", "DeepSeek Official"]
            );
        });
    });
}

#[test]
fn entering_a_key_masks_the_input_and_saving_returns_to_plain_text() {
    App::test((), |mut app| async move {
        let (editor, _, menu) = add_seeded_menu(&mut app, vec![("DeepSeek Official", true)]);
        menu.update(&mut app, |menu, ctx| menu.accept_selected(ctx));

        app.read(|ctx| {
            assert_eq!(
                menu.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuMasked
            );
            // The pin prefills the stored secret so it can be edited; this fork starts empty
            // and never reads a stored key back out into the buffer.
            assert_eq!(input_text(&editor, ctx), "");
        });

        editor.update(&mut app, |editor, ctx| {
            editor.user_insert("replacement-secret", ctx);
        });
        app.read(|ctx| {
            assert_eq!(
                menu.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuMasked
            );
            // Masking is paint-only: the backing model keeps the real text, which is what
            // makes editing (and submitting) still work.
            assert_eq!(input_text(&editor, ctx), "replacement-secret");
        });

        menu.update(&mut app, |menu, ctx| menu.accept_selected(ctx));
        app.read(|ctx| {
            assert_eq!(
                menu.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuPlainText
            );
            // Masking ends with the sub-state, so the key must not survive into the
            // now-unmasked buffer.
            assert_eq!(input_text(&editor, ctx), "");
        });
    });
}
