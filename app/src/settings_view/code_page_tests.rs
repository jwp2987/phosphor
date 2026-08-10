//! Tests for the Code settings page, focused on the restored language-server
//! section (LSP step 6b).
//!
//! The pinned oracle's `code_page_tests.rs` tests exactly one thing -- a
//! remote-index failure-message predicate that does not exist in this fork -- so
//! these are fork-added rather than ported.
//!
//! They exist because the failure mode this section is most likely to have is not
//! "it renders wrong", it is "it renders and reaches nothing": a page that draws a
//! switch which never touches `PersistedWorkspace` looks identical to a working one
//! in a screenshot. So the assertions are on the models behind the controls, plus
//! on the page actually containing the widgets (a widget nobody added to
//! `build_page` is dead code that still compiles).
//!
//! **What is deliberately not covered, and why.** The enable direction of
//! `ToggleLspServer` (and `EnableSuggestedLspServer`) ends in
//! `PersistedWorkspace::execute_lsp_task(LspTask::Spawn { .. })`, whose whole job
//! is to capture the interactive shell PATH and then *start a real language server
//! process*. Driving that from a unit test would spawn `rust-analyzer` on the test
//! machine. The disable direction is asserted here in full; the enable direction's
//! persistence half is `enable_lsp_server_for_path`, covered in
//! `app/src/ai/persisted_workspace_tests.rs`.

use std::collections::HashMap;
use std::path::PathBuf;

use ai::project_context::model::ProjectContextModel;
use ai::workspace::WorkspaceMetadata;
use chrono::Utc;
use lsp::supported_servers::LSPServerType;
use warp_core::features::FeatureFlag;
use warpui::platform::WindowStyle;
use warpui::{App, SingletonEntity as _, TypedActionView as _};

use super::{CodeSettingsPageAction, CodeSettingsPageView};
use crate::ai::persisted_workspace::{EnablementState, PersistedWorkspace};
use crate::appearance::Appearance;
use crate::settings::CodeSettings;
use crate::settings_view::settings_page::SettingsPageMeta as _;
use crate::settings_view::SettingsSection;

fn add_code_page(app: &mut App) -> warpui::ViewHandle<CodeSettingsPageView> {
    crate::test_util::settings::initialize_settings_for_tests(app);
    app.add_singleton_model(|_| Appearance::mock());
    // `CodeSettingsPageView::new` subscribes to it, and an unregistered
    // singleton panics rather than being created on demand.
    app.add_singleton_model(|_| ai::project_context::model::ProjectContextModel::default());

    let (_window_id, page) = app.add_window(WindowStyle::NotStealFocus, CodeSettingsPageView::new);
    page
}

const WORKSPACE: &str = "/tmp/zap-code-page-tests/repo";

/// Registers the singletons `CodeSettingsPageView::new` reads, minus
/// `PersistedWorkspace` and `LspManagerModel` -- whether the page survives their
/// absence is itself one of the things under test.
fn register_base_singletons(app: &mut App) {
    crate::test_util::settings::initialize_settings_for_tests(app);
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|ctx| ProjectContextModel::new_from_persisted(Vec::new(), ctx));
}

/// A `PersistedWorkspace` holding one persisted workspace with `server_type` in
/// `state`. `navigated_ts` must be set: `workspaces()` filters out non-persisted
/// entries, so a workspace without a timestamp never reaches the page at all.
///
/// Callers must hold a `FullSourceCodeEmbedding` override for the duration --
/// that flag is on by default in this fork, and it is what makes
/// `PersistedWorkspace::new` subscribe to `CodebaseIndexManager` and
/// `BlocklistAIHistoryModel`, two singletons this page never touches and whose
/// `handle()` would panic here. Nothing on the LSP path reads the flag.
fn register_persisted_workspace(app: &mut App, server_type: LSPServerType, state: EnablementState) {
    let path = PathBuf::from(WORKSPACE);
    let metadata = WorkspaceMetadata {
        path: path.clone(),
        navigated_ts: Some(Utc::now()),
        modified_ts: None,
        queried_ts: None,
    };
    let language_servers = HashMap::from([(path.clone(), HashMap::from([(server_type, state)]))]);
    app.add_singleton_model(move |ctx| {
        PersistedWorkspace::new(vec![metadata], language_servers, None, ctx)
    });
}

fn lsp_state(app: &App, server_type: LSPServerType) -> Option<EnablementState> {
    app.read(|ctx| {
        PersistedWorkspace::as_ref(ctx)
            .all_lsp_servers(&PathBuf::from(WORKSPACE), true)?
            .find(|(server, _)| *server == server_type)
            .map(|(_, state)| state)
    })
}

#[test]
fn code_page_registers_under_the_code_section() {
    assert_eq!(CodeSettingsPageView::section(), SettingsSection::Code);
}

#[test]
fn the_page_contains_the_language_server_and_format_on_save_widgets() {
    let _flag = FeatureFlag::ZapNewSettingsModes.override_enabled(true);
    let _indexing = FeatureFlag::FullSourceCodeEmbedding.override_enabled(false);
    App::test((), |mut app| async move {
        register_base_singletons(&mut app);
        register_persisted_workspace(&mut app, LSPServerType::RustAnalyzer, EnablementState::Yes);

        let (_window_id, page) =
            app.add_window(WindowStyle::NotStealFocus, CodeSettingsPageView::new);

        // Both queries are unique to one widget's search terms, so a match proves
        // that widget is installed in the page rather than merely defined.
        page.update(&mut app, |view, ctx| {
            assert!(
                view.update_filter("rust-analyzer", ctx).is_truthy(),
                "the language-server section is not reachable from the Code page"
            );
            assert!(
                view.update_filter("autoformat", ctx).is_truthy(),
                "the format-on-save toggle is not reachable from the Code page"
            );
            // Restore the unfiltered page so the view is left as found.
            view.update_filter("", ctx);
        });
    });
}

#[test]
fn disabling_a_server_from_the_page_writes_through_to_persisted_workspace() {
    let _flag = FeatureFlag::ZapNewSettingsModes.override_enabled(true);
    let _indexing = FeatureFlag::FullSourceCodeEmbedding.override_enabled(false);
    App::test((), |mut app| async move {
        register_base_singletons(&mut app);
        register_persisted_workspace(&mut app, LSPServerType::RustAnalyzer, EnablementState::Yes);

        let (_window_id, page) =
            app.add_window(WindowStyle::NotStealFocus, CodeSettingsPageView::new);

        assert_eq!(
            lsp_state(&app, LSPServerType::RustAnalyzer),
            Some(EnablementState::Yes),
            "test setup did not take"
        );

        page.update(&mut app, |page, ctx| {
            page.handle_action(
                &CodeSettingsPageAction::ToggleLspServer {
                    workspace_path: PathBuf::from(WORKSPACE),
                    server_type: LSPServerType::RustAnalyzer,
                    currently_enabled: true,
                },
                ctx,
            );
        });

        // `No`, not "absent": the disable has to be recorded explicitly, or the
        // next available-server scan would re-suggest a server the user turned off.
        assert_eq!(
            lsp_state(&app, LSPServerType::RustAnalyzer),
            Some(EnablementState::No),
            "turning the switch off did not reach PersistedWorkspace"
        );
    });
}

#[test]
fn format_on_save_action_writes_through_to_code_settings() {
    let _flag = FeatureFlag::ZapNewSettingsModes.override_enabled(true);
    App::test((), |mut app| async move {
        register_base_singletons(&mut app);

        let (_window_id, page) =
            app.add_window(WindowStyle::NotStealFocus, CodeSettingsPageView::new);

        let before = app.read(|ctx| *CodeSettings::as_ref(ctx).format_on_save);

        page.update(&mut app, |page, ctx| {
            page.handle_action(&CodeSettingsPageAction::ToggleFormatOnSave, ctx);
        });
        assert_eq!(
            app.read(|ctx| *CodeSettings::as_ref(ctx).format_on_save),
            !before,
            "the toggle did not reach CodeSettings::format_on_save"
        );

        // The opt-out has to be as effective as the opt-in.
        page.update(&mut app, |page, ctx| {
            page.handle_action(&CodeSettingsPageAction::ToggleFormatOnSave, ctx);
        });
        assert_eq!(
            app.read(|ctx| *CodeSettings::as_ref(ctx).format_on_save),
            before
        );
    });
}

#[test]
fn the_page_is_constructible_without_the_lsp_singletons() {
    let _flag = FeatureFlag::ZapNewSettingsModes.override_enabled(true);
    App::test((), |mut app| async move {
        // Neither PersistedWorkspace nor LspManagerModel registered. Both
        // `handle()` and `as_ref()` panic on an unregistered singleton, so an
        // unguarded subscription or an unguarded read in the section's render
        // would blow up here -- which is exactly how this class of bug reached
        // the remote-server daemon once already.
        register_base_singletons(&mut app);

        let (_window_id, page) =
            app.add_window(WindowStyle::NotStealFocus, CodeSettingsPageView::new);

        page.read(&app, |view, ctx| {
            assert!(view.should_render(ctx));
            // Rendering must not reach for the missing singletons either.
            let _ = view.render_language_servers(Appearance::as_ref(ctx), ctx);
        });
    });
}

#[test]
fn code_page_action_writes_through_to_codebase_context_setting() {
    let _flag = FeatureFlag::ZapNewSettingsModes.override_enabled(false);
    App::test((), |mut app| async move {
        let page = add_code_page(&mut app);

        // Defaults off: indexing spends the user's embedding quota, so it is
        // opt-in (see `app/src/settings/code.rs`).
        assert!(!app.read(|ctx| *CodeSettings::as_ref(ctx).codebase_context_enabled));

        page.update(&mut app, |page, ctx| {
            page.handle_action(&CodeSettingsPageAction::ToggleCodebaseContext, ctx);
        });
        assert!(app.read(|ctx| *CodeSettings::as_ref(ctx).codebase_context_enabled));

        // The opt-out has to reach the setting as effectively as the opt-in:
        // `UserWorkspaces::is_codebase_context_enabled` reads exactly this value.
        page.update(&mut app, |page, ctx| {
            page.handle_action(&CodeSettingsPageAction::ToggleCodebaseContext, ctx);
        });
        assert!(!app.read(|ctx| *CodeSettings::as_ref(ctx).codebase_context_enabled));
    });
}

#[test]
fn code_page_action_writes_through_to_auto_indexing_setting() {
    let _flag = FeatureFlag::ZapNewSettingsModes.override_enabled(false);
    App::test((), |mut app| async move {
        let page = add_code_page(&mut app);

        assert!(!app.read(|ctx| *CodeSettings::as_ref(ctx).auto_indexing_enabled));

        page.update(&mut app, |page, ctx| {
            page.handle_action(&CodeSettingsPageAction::ToggleAutoIndexing, ctx);
        });
        assert!(app.read(|ctx| *CodeSettings::as_ref(ctx).auto_indexing_enabled));

        page.update(&mut app, |page, ctx| {
            page.handle_action(&CodeSettingsPageAction::ToggleAutoIndexing, ctx);
        });
        assert!(!app.read(|ctx| *CodeSettings::as_ref(ctx).auto_indexing_enabled));
    });
}

#[test]
fn code_page_is_hidden_when_the_feature_flag_is_off() {
    let _flag = FeatureFlag::ZapNewSettingsModes.override_enabled(false);
    App::test((), |mut app| async move {
        let page = add_code_page(&mut app);

        assert!(!page.read(&app, |view, ctx| view.should_render(ctx)));
    });
}
