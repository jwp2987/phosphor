//! Tests for the Scripting settings page.
//!
//! The pinned oracle (`02b53fcd8`) ships no tests for `scripting_page.rs`, so
//! these are fork-added rather than ported. They exist because the *point* of
//! the page is to be the one user-reachable path that flips
//! `crate::settings::LocalControlSettings` -- on a public channel that setting
//! defaults to `Disabled` and nothing else in the UI can change it. A page
//! that renders but never reaches the setting would be exactly the
//! "ported but never wired" defect this port is fixing, and only an assertion
//! on the settings model catches that.

use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warpui::platform::WindowStyle;
use warpui::{App, SingletonEntity as _, TypedActionView as _};

use super::{ScriptingSettingsPageAction, ScriptingSettingsPageView};
use crate::appearance::Appearance;
use crate::settings::{LocalControlMode, LocalControlSettings};
use crate::settings_view::settings_page::SettingsPageMeta as _;
use crate::settings_view::SettingsSection;

#[test]
fn scripting_page_registers_under_the_scripting_section() {
    assert_eq!(
        ScriptingSettingsPageView::section(),
        SettingsSection::Scripting
    );
}

#[test]
fn scripting_page_action_writes_through_to_local_control_settings() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(true);
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(|_| Appearance::mock());

        let (_window_id, page) =
            app.add_window(WindowStyle::NotStealFocus, ScriptingSettingsPageView::new);

        // Enabling from the page must reach the authoritative settings group,
        // which is what `app/src/local_control/permissions.rs` gates on.
        page.update(&mut app, |page, ctx| {
            page.handle_action(
                &ScriptingSettingsPageAction::SetLocalControlMode(LocalControlMode::Enabled),
                ctx,
            );
        });
        let mode = app.read(|ctx| LocalControlSettings::as_ref(ctx).mode());
        assert_eq!(mode, LocalControlMode::Enabled);
        assert!(app.read(|ctx| LocalControlSettings::as_ref(ctx).is_enabled()));

        // And disabling must reach it too -- the opt-out has to be as
        // effective as the opt-in.
        page.update(&mut app, |page, ctx| {
            page.handle_action(
                &ScriptingSettingsPageAction::SetLocalControlMode(LocalControlMode::Disabled),
                ctx,
            );
        });
        let mode = app.read(|ctx| LocalControlSettings::as_ref(ctx).mode());
        assert_eq!(mode, LocalControlMode::Disabled);
        assert!(!app.read(|ctx| LocalControlSettings::as_ref(ctx).is_enabled()));
    });
}

#[test]
fn scripting_page_is_hidden_when_the_feature_flag_is_off() {
    let _flag = FeatureFlag::WarpControlCli.override_enabled(false);
    App::test((), |mut app| async move {
        crate::test_util::settings::initialize_settings_for_tests(&mut app);
        app.add_singleton_model(|_| Appearance::mock());

        let (_window_id, page) =
            app.add_window(WindowStyle::NotStealFocus, ScriptingSettingsPageView::new);

        assert!(!page.read(&app, |view, ctx| view.should_render(ctx)));
    });
}
