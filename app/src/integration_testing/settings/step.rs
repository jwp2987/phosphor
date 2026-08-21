use settings::{Setting, ToggleableSetting};
use warpui::{async_assert, integration::TestStep, windowing::WindowManager, SingletonEntity};

use crate::{
    integration_testing::{
        step::new_step_with_default_assertions, view_getters::theme_chooser_view,
    },
    settings_view::SettingsAction,
    terminal::safe_mode_settings::SafeModeSettings,
    window_settings::WindowSettings,
    workspace::{Workspace, WorkspaceAction},
};

/// Builds a step that will toggle a setting by [`SettingsAction`]. This can
/// only update settings with a corresponding action on the settings view.
pub fn toggle_setting(action: SettingsAction) -> TestStep {
    new_step_with_default_assertions(&format!("Toggle setting: {action:?}")).with_action(
        move |app, _, _| {
            let window_id = app.read(|ctx| {
                WindowManager::as_ref(ctx)
                    .active_window()
                    .expect("no active window")
            });
            let workspace_view_id = app
                .views_of_type::<Workspace>(window_id)
                .and_then(|views| views.first().map(|view| view.id()))
                .expect("no workspace view");
            app.dispatch_typed_action(
                window_id,
                &[workspace_view_id],
                &WorkspaceAction::DispatchToSettingsTab(action.clone()),
            );
        },
    )
}

pub fn toggle_safe_mode_setting() -> TestStep {
    new_step_with_default_assertions("Toggle safe mode setting").with_action(move |app, _, _| {
        SafeModeSettings::handle(app).update(app, |settings, ctx| {
            let _ = settings.safe_mode_enabled.toggle_and_save_value(ctx);
        });
    })
}

pub fn toggle_hide_secrets_in_block_list_setting() -> TestStep {
    new_step_with_default_assertions("Toggle hide secrets in block list setting").with_action(
        move |app, _, _| {
            SafeModeSettings::handle(app).update(app, |settings, ctx| {
                let _ = settings
                    .hide_secrets_in_block_list
                    .toggle_and_save_value(ctx);
            });
        },
    )
}

pub fn assert_theme_chooser_contains(theme_name: &'static str, count: usize) -> TestStep {
    TestStep::new("Assert the theme chooser contents match our expectations").add_named_assertion(
        format!("The theme chooser contains {count} theme(s) named \"{theme_name}\""),
        move |app, window_id| {
            let theme_chooser = theme_chooser_view(app, window_id);

            let result: usize = theme_chooser.read(app, |theme_chooser, _| {
                theme_chooser
                    .themes()
                    .filter(|theme| theme.matches(theme_name))
                    .count()
            });
            async_assert!(
                result == count,
                "Should have exactly {count} theme(s) named test theme. Instead had {result}"
            )
        },
    )
}

/// Set a custom size for new windows. This updates:
/// * The boolean setting for whether or not to use the custom size
/// * The setting for the window width in rows
/// * The setting for the window height in columns
pub fn set_window_custom_size(rows: u16, columns: u16) -> TestStep {
    TestStep::new("Set custom size for new windows").with_action(move |app, _, _| {
        // Deliberately still panics, and deliberately not `report_if_error!`.
        // This is an integration-test driver (the module is behind the
        // `integration_tests` feature and is not in a shipped build); a setup
        // step that silently did nothing would leave every later assertion
        // measuring the default window size -- a vacuous pass. The messages are
        // corrected: `set_value` can now fail because the preferences backend
        // refused the write, not only because serialization failed. (The row and
        // column messages also had "width" and "height" the wrong way round.)
        WindowSettings::handle(app).update(app, |settings, ctx| {
            settings
                .open_windows_at_custom_size
                .set_value(true, ctx)
                .expect("could not enable custom window sizes (the preferences backend refused the write)");
            settings
                .new_windows_num_rows
                .set_value(rows, ctx)
                .expect("could not set the window height in rows (the preferences backend refused the write)");
            settings
                .new_windows_num_columns
                .set_value(columns, ctx)
                .expect("could not set the window width in columns (the preferences backend refused the write)");
        })
    })
}
