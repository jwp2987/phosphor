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
    ///
    /// Audit of every migration in here for "pin predicate reading a fork-diverged
    /// default", so it is not re-derived (the NLD one below was an instance and is
    /// fixed; the rest are recorded as checked):
    ///
    /// * NLD-in-terminal — **was** an instance; see the long comment at its call
    ///   site. `nld_in_terminal_enabled_internal` defaults `true` here vs `false` at
    ///   the pin.
    /// * Adeberry theme — compares against the literal `ThemeKind::Phenomenon`, not
    ///   against the default, so a diverged default cannot make it write the wrong
    ///   value. It does diverge in the harmless direction: `ThemeKind::default()` is
    ///   `PhosphorAmber` here and `Dark` at the pin, so a fresh fork user never
    ///   matches `Phenomenon` and the override simply never fires. That is fine —
    ///   `PhosphorAmber` is the fork's intended branding default. Moot in practice
    ///   while the enclosing `is_onboarded() == Some(false)` block is unreachable.
    /// * Universal input box / `honor_ps1` — `input_box_type` defaults to
    ///   `InputBoxType::Classic` in both trees, so the comparison means the same
    ///   thing. Also inside the unreachable block.
    /// * Windows monospace font size — a constant, reads no setting.
    /// * `KeepThinkingExpanded` -> `ThinkingDisplayMode` — reads the two raw
    ///   preference keys directly rather than any typed default, and only writes
    ///   when the old key was explicitly `true`. Immune to default divergence.
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

        // Migrate the old, previously-global autodetection setting
        // (`ai_autodetection_enabled_internal`) to the new per-surface
        // `nld_in_terminal_enabled_internal`, when AgentView is enabled.
        //
        // ---------------------------------------------------------------------
        // DO NOT "restore parity" here on the next re-pin. This deliberately does
        // not match `42effe840:app/src/settings/initializer.rs:110-125`, and the
        // pin's version is a user-facing regression in this tree.
        // ---------------------------------------------------------------------
        //
        // The pin computes
        //
        //     let is_existing_user = auth_state.is_onboarded() == Some(true);
        //     let was_global_autodetection_enabled_for_existing_user =
        //         *ai_settings.ai_autodetection_enabled_internal && is_existing_user;
        //
        // and writes that boolean unconditionally, guarded only by "the target has
        // not been explicitly set". Both of its inputs mean something different
        // here, so the predicate cannot be ported as-is:
        //
        // 1. `is_onboarded()` is a constant `Some(true)` in this fork. The local
        //    placeholder user hardcodes `is_onboarded: true` (`app/src/auth/mod.rs:213`)
        //    and nothing outside tests clears it, so `is_existing_user` cannot tell
        //    an upgrading user from a fresh install. It is dead weight, not a guard.
        // 2. `nld_in_terminal_enabled_internal` defaults to `true` here, where the
        //    pin defaults it to `false` (`app/src/settings/ai.rs:1888-1896` vs
        //    `42effe840:app/src/settings/ai.rs:1198-1207`). That divergence is
        //    deliberate and documented: it is what lets Chinese-speaking users type
        //    Chinese straight into the terminal and have the heuristic classifier
        //    route it to the agent. The fork has no cloud AgentView fullscreen entry
        //    point, so the terminal is the primary input surface.
        // 3. `ai_autodetection_enabled_internal` defaults to `false`, matching the
        //    pin (`app/src/settings/ai.rs:1872-1876`).
        //
        // Combine (1)-(3) and the pin's expression evaluates to `false && true ==
        // false` for every user who has never touched either setting -- so on the
        // first launch after this migration ships it would *explicitly* write
        // `nld_in_terminal_enabled = false` for all of them, silently killing the
        // fork's default. Worse, the value would then be explicitly set, so deleting
        // this migration later would not bring the default back.
        //
        // The fix keys off whether the *source* setting was explicitly set rather
        // than whether it happens to be truthy. "Explicitly set" is what the pin's
        // `is_existing_user` term was standing in for -- "this user configured the
        // old global flag before it was split per-surface" -- and unlike
        // `is_onboarded()` it is a question this tree can actually answer:
        //
        //   * never touched `ai_autodetection_enabled_internal`  -> no write at all,
        //     so the fork's `true` default keeps applying and stays non-explicit.
        //   * explicitly set it                                   -> carry that intent
        //     across to the new setting in both directions, which is what the pin
        //     intended for the users it could identify. Note that a user who
        //     explicitly turned the old flag off asked for exactly this: "do not
        //     auto-detect natural language in my input".
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

                // No explicit choice on the old global flag means there is no user
                // intent to carry over. Leave the new setting alone so its
                // (fork-diverged) default keeps applying.
                if !ai_settings
                    .ai_autodetection_enabled_internal
                    .is_value_explicitly_set()
                {
                    return;
                }

                let was_global_autodetection_enabled =
                    *ai_settings.ai_autodetection_enabled_internal;
                report_if_error!(ai_settings
                    .nld_in_terminal_enabled_internal
                    .set_value(was_global_autodetection_enabled, ctx));
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

#[cfg(test)]
mod tests {
    //! Inline because there is no `initializer_tests.rs` sibling and the
    //! behaviour under test — "the NLD migration must not clobber this fork's
    //! `nld_in_terminal_enabled` default" — belongs next to the migration it
    //! guards.

    use std::sync::Arc;

    use warp_core::features::FeatureFlag;
    use warp_core::settings::Setting;
    use warpui::{App, SingletonEntity};

    use super::SettingsInitializer;
    use crate::auth::AuthState;
    use crate::settings::AISettings;
    use crate::test_util::settings::initialize_settings_for_tests;

    fn run_startup_migrations(app: &mut App) {
        app.add_singleton_model(|_| SettingsInitializer::new());
        app.update(|ctx| {
            // Mirrors `settings::run_startup_settings_initialization`: the local
            // placeholder `AuthState` is what production hands the migrations,
            // and it always reports `is_onboarded() == Some(true)`.
            let auth_state = Arc::new(AuthState::new_for_test());
            SettingsInitializer::handle(ctx).update(ctx, |initializer, ctx| {
                initializer.apply_startup_settings_migrations(auth_state, ctx);
            });
        });
    }

    /// The regression this file's long comment describes. Ported verbatim from
    /// the pin, the NLD migration computes
    /// `*ai_autodetection_enabled_internal (false here) && is_onboarded() (always
    /// Some(true) here)` and *explicitly* writes `false`, permanently destroying
    /// the fork's `true` default for every user who never touched the setting.
    #[test]
    fn untouched_user_keeps_the_fork_nld_default_and_gets_no_explicit_write() {
        App::test((), |mut app| async move {
            let _agent_view = FeatureFlag::AgentView.override_enabled(true);

            initialize_settings_for_tests(&mut app);

            // Precondition: this fork diverges from the pin here on purpose.
            app.read(|ctx| {
                let ai_settings = AISettings::as_ref(ctx);
                assert!(
                    *ai_settings.nld_in_terminal_enabled_internal,
                    "precondition: this fork defaults nld_in_terminal_enabled to true"
                );
                assert!(
                    !*ai_settings.ai_autodetection_enabled_internal,
                    "precondition: ai_autodetection_enabled defaults to false (pin-aligned)"
                );
                assert!(
                    !ai_settings
                        .ai_autodetection_enabled_internal
                        .is_value_explicitly_set(),
                    "precondition: an untouched user has no explicit autodetection value"
                );
            });

            run_startup_migrations(&mut app);

            app.read(|ctx| {
                let ai_settings = AISettings::as_ref(ctx);
                assert!(
                    *ai_settings.nld_in_terminal_enabled_internal,
                    "the startup migration must not turn off NLD-in-terminal for a user \
                     who never touched it -- that is the fork default that lets CJK input \
                     in the terminal be routed to the agent"
                );
                assert!(
                    !ai_settings
                        .nld_in_terminal_enabled_internal
                        .is_value_explicitly_set(),
                    "the migration must not write an explicit value for an untouched user; \
                     an explicit write would pin the setting forever, so that even deleting \
                     this migration could not restore the default"
                );
            });
        });
    }

    /// The migration still does its job for the only users this tree can
    /// identify: those who explicitly configured the old global autodetection
    /// flag before it was split per-surface. Without this, the fix above would
    /// be indistinguishable from deleting the migration.
    #[test]
    fn explicit_autodetection_choice_is_carried_over_to_nld_in_terminal() {
        App::test((), |mut app| async move {
            let _agent_view = FeatureFlag::AgentView.override_enabled(true);

            initialize_settings_for_tests(&mut app);

            // The user explicitly turned the old global flag off.
            app.update(|ctx| {
                AISettings::handle(ctx).update(ctx, |ai_settings, ctx| {
                    ai_settings
                        .ai_autodetection_enabled_internal
                        .set_value(false, ctx)
                        .expect("in-memory preferences accept the write");
                });
            });
            app.read(|ctx| {
                assert!(
                    AISettings::as_ref(ctx)
                        .ai_autodetection_enabled_internal
                        .is_value_explicitly_set(),
                    "precondition: the explicit set must be recorded as explicit"
                );
            });

            run_startup_migrations(&mut app);

            app.read(|ctx| {
                let ai_settings = AISettings::as_ref(ctx);
                assert!(
                    !*ai_settings.nld_in_terminal_enabled_internal,
                    "an explicit 'do not auto-detect natural language' choice must carry \
                     over to the new per-surface setting"
                );
                assert!(
                    ai_settings
                        .nld_in_terminal_enabled_internal
                        .is_value_explicitly_set(),
                    "carrying the choice over means writing it explicitly"
                );
            });
        });
    }

    /// Turning the old flag on explicitly carries over too, in the other
    /// direction. Keeps the test above from passing merely because the
    /// migration writes `false` unconditionally.
    #[test]
    fn explicit_autodetection_enabled_carries_over_as_enabled() {
        App::test((), |mut app| async move {
            let _agent_view = FeatureFlag::AgentView.override_enabled(true);

            initialize_settings_for_tests(&mut app);

            app.update(|ctx| {
                AISettings::handle(ctx).update(ctx, |ai_settings, ctx| {
                    ai_settings
                        .ai_autodetection_enabled_internal
                        .set_value(true, ctx)
                        .expect("in-memory preferences accept the write");
                });
            });

            run_startup_migrations(&mut app);

            app.read(|ctx| {
                let ai_settings = AISettings::as_ref(ctx);
                assert!(*ai_settings.nld_in_terminal_enabled_internal);
                assert!(ai_settings
                    .nld_in_terminal_enabled_internal
                    .is_value_explicitly_set());
            });
        });
    }
}
