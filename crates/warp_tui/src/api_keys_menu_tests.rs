//! Most of `TuiApiKeysMenuModel`'s behavior (open/list/accept persisting through
//! `AgentProviderSecrets`) reads app-side singletons (`AISettings`,
//! `AgentProviderSecrets`) that aren't available in `warp_tui`'s lightweight test harness --
//! see `crate::exchange_menu` / `crate::profile_menu` tests for the same tradeoff. That
//! end-to-end behavior is instead exercised via the PTY harness
//! (`crates/warp_tui/scripts/tui_harness.py`) and, on the app side, via
//! `app/src/ai/agent_providers/mod_test.rs`'s `tui_*_agent_provider_*` tests (which cover the
//! actual persistence: adding a key, clearing it, and the connected-key predicate).
//!
//! What *is* free of that dependency -- and so tested directly here -- is
//! [`build_provider_rows`], the pure row-building/filtering logic this menu's `refresh_rows`
//! delegates to.

use warp::editor::CodeEditorModel;
use warp::tui_export::{Appearance, TuiApiKeyProvider};
use warpui_core::App;

use super::*;
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
