//! Tests for the Code settings page.
//!
//! The pinned oracle (`02b53fcd8`) ships no tests for `code_page.rs`. These are
//! fork-added, and they exist for one reason: `code.indexing.*` was reachable
//! only by hand-editing `settings.toml`, so the page's job is to be the path
//! that writes it. A widget that renders but never reaches the settings model
//! is the "ported but never wired" defect, and only an assertion on
//! `CodeSettings` catches it.
//!
//! The page is built with `ZapNewSettingsModes` off on purpose: the action
//! handler is independent of the flag, while building the flag-on page pulls in
//! the external-editor subview and its dropdowns, which is not what these tests
//! are about.

use warp_core::features::FeatureFlag;
use warpui::platform::WindowStyle;
use warpui::{App, SingletonEntity as _, TypedActionView as _};

use super::{CodeSettingsPageAction, CodeSettingsPageView};
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

#[test]
fn code_page_registers_under_the_code_section() {
    assert_eq!(CodeSettingsPageView::section(), SettingsSection::Code);
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
