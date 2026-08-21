use std::sync::Arc;

use warp_core::{features::FeatureFlag, settings::Setting};
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::settings::{AISettings, FontSettings, ThinkingDisplayMode};
use crate::{
    auth::AuthState,
    report_if_error,
    settings::input::InputBoxType,
    settings::{InputSettings, PrivacySettings, ThemeSettings},
    terminal::session_settings::SessionSettings,
    themes::theme::ThemeKind,
};

pub struct SettingsInitializer;

impl Default for SettingsInitializer {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsInitializer {
    pub fn new() -> Self {
        Self
    }

    /// Adjusts settings values once the user identity is known.
    ///
    /// Specifically useful for adjusting settings for first-time users when the default value of a
    /// setting as set in define_settings_group! is no longer the desired default value,
    /// but we don't want to change it for existing users (which is what would happen if we changed the
    /// default value in define_settings_group! in code), and for one-shot migrations of renamed
    /// settings keys.
    ///
    /// Zap: at the pin this was `handle_user_fetched`, called from
    /// `42effe840:app/src/auth/auth_manager.rs:430` when the server returned the
    /// user. This fork deleted that call site with the cloud auth layer, which left
    /// the whole function — and therefore the `KeepThinkingExpanded` ->
    /// `ThinkingDisplayMode` migration below — unreachable. There is no user-fetch
    /// event left to hang it off: `AuthState` here is a local placeholder that is
    /// fully determined the moment it is constructed, so startup is the trigger and
    /// `settings::run_startup_settings_initialization` is the caller.
    ///
    /// Caveat, so it is not re-derived: the `is_onboarded() == Some(false)` block is
    /// still unreachable in this fork, because the local user hardcodes
    /// `is_onboarded: true` (`app/src/auth/mod.rs:213`). It is kept intact rather
    /// than deleted so it works again if a real onboarding state is ever introduced.
    /// The migrations below the block are the part that this fork actually needs,
    /// and they now run.
    pub fn apply_startup_settings_migrations(
        &self,
        auth_state: Arc<AuthState>,
        ctx: &mut ModelContext<Self>,
    ) {
        /// We use a font-size of 16px (12pt) on Windows to more closely match the default font size of
        /// Windows terminal.
        const DEFAULT_WINDOWS_MONOSPACE_FONT_SIZE: f32 = 16.;

        if auth_state.is_onboarded() == Some(false) {
            PrivacySettings::handle(ctx).update(ctx, |settings, ctx| {
                // Previously, secret redaction had a built-in default set of regexes that users couldn't change.
                // We want to add that default list to all existing users' lists, so we don't regress their current secret redaction experience.
                // However, for new users, we don't want to add these defaults without their explicit action, so we disable adding them here.
                settings.disable_default_regex_trigger(ctx);
            });

            if FeatureFlag::DefaultAdeberryTheme.is_enabled() {
                log::debug!("Setting default theme to Adeberry for new user");
                ThemeSettings::handle(ctx).update(ctx, |settings, ctx| {
                    if *settings.theme_kind.value() == ThemeKind::Phenomenon {
                        report_if_error!(settings.theme_kind.set_value(ThemeKind::Adeberry, ctx));
                    }
                });
            }

            if cfg!(windows) {
                log::debug!("Setting default font size to 16px (12pt) for a new Windows user");
                FontSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings
                        .monospace_font_size
                        .set_value(DEFAULT_WINDOWS_MONOSPACE_FONT_SIZE, ctx));
                })
            }

            let did_update_input_type = InputSettings::handle(ctx).update(ctx, |settings, ctx| {
                if !settings.input_box_type.is_value_explicitly_set()
                    && *settings.input_box_type.value() == InputBoxType::Classic
                {
                    log::debug!("Setting default input type to Phosphor prompt for new user");
                    report_if_error!(settings
                        .input_box_type
                        .set_value(InputBoxType::Universal, ctx));
                    ctx.notify();
                    return true;
                }
                false
            });
            // Keep honor_ps1 in sync: Universal input requires honor_ps1 = false.
            if did_update_input_type {
                SessionSettings::handle(ctx).update(ctx, |settings, ctx| {
                    if *settings.honor_ps1.value() {
                        report_if_error!(settings.honor_ps1.set_value(false, ctx));
                    }
                });
            }
        }

        // Migrate NLD settings when AgentView is enabled.
        //
        // Explicitly set `nld_in_terminal_enabled_internal` for all users if
        // it has not previously been set.
        //
        // For existing users, when the old, previously-global autodetection setting
        // (`ai_autodetection_enabled_internal`) true, set `nld_in_terminal_enabled_internal` to
        // true. Otherwise, explicitly set to `false`.
        //
        // Any further user modification of the setting will be via explicit update, so it'll
        // be exempt from this logic, which is effectively one-time upon first startup of a binary
        // containing this logic.
        //
        // TODO(zachbai): Remove this approximately 6 weeks from 2/5/26.
        if FeatureFlag::AgentView.is_enabled() {
            AISettings::handle(ctx).update(ctx, |ai_settings, ctx| {
                if ai_settings
                    .nld_in_terminal_enabled_internal
                    .is_value_explicitly_set()
                {
                    return;
                }

                let is_existing_user = auth_state.is_onboarded() == Some(true);
                let was_global_autodetection_enabled_for_existing_user =
                    *ai_settings.ai_autodetection_enabled_internal && is_existing_user;
                report_if_error!(ai_settings
                    .nld_in_terminal_enabled_internal
                    .set_value(was_global_autodetection_enabled_for_existing_user, ctx));
            });
        }

        // Migrate the old `KeepThinkingExpanded` bool setting to the new
        // `ThinkingDisplayMode` enum setting.
        //
        // The old setting was a boolean (default: false) that controlled whether
        // agent thinking blocks stayed expanded after streaming. It has been
        // replaced by a three-option enum: ShowAndCollapse (default),
        // AlwaysShow, and NeverShow.
        //
        // If the user explicitly set `KeepThinkingExpanded` to `true`, migrate
        // them to `ThinkingDisplayMode::AlwaysShow` so they don't lose their
        // preference when updating to the new client.
        //
        // TODO(jefflloyd): Remove this approximately 6 weeks from 3/19/26.
        {
            use warp_core::user_preferences::GetUserPreferences as _;

            AISettings::handle(ctx).update(ctx, |ai_settings, ctx| {
                // If the new setting already has a value in preferences, the
                // migration has already run (or the user set it directly).
                let new_key_exists = ctx
                    .private_user_preferences()
                    .read_value("ThinkingDisplayMode")
                    .unwrap_or_default()
                    .is_some();

                if new_key_exists {
                    return;
                }

                // Read the old boolean setting directly from preferences
                // because `KeepThinkingExpanded` has been removed from the
                // `AISettings` struct — there is no typed field left to query.
                let old_value_was_true = ctx
                    .private_user_preferences()
                    .read_value("KeepThinkingExpanded")
                    .unwrap_or_default()
                    .and_then(|v| serde_json::from_str::<bool>(&v).ok())
                    == Some(true);

                if old_value_was_true {
                    report_if_error!(ai_settings
                        .thinking_display_mode
                        .set_value(ThinkingDisplayMode::AlwaysShow, ctx));
                }

                // Clean up the old key.
                let _ = ctx
                    .private_user_preferences()
                    .remove_value("KeepThinkingExpanded");
            });
        }
    }
}

impl Entity for SettingsInitializer {
    type Event = ();
}

/// Mark PreferencesSyncer as global application state.
impl SingletonEntity for SettingsInitializer {}
