use warp_core::{features::FeatureFlag, settings::Setting};
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::report_if_error;
use crate::settings::{AISettings, ThinkingDisplayMode};

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

    /// One-shot migrations of renamed or restructured settings keys, run once at
    /// startup.
    ///
    /// Upstream's version was also the place to give first-time users a different
    /// default from the one declared in `define_settings_group!`. That half is gone
    /// here — see the "no new-user branch" note below.
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
    /// **There is no "new user" branch here, on purpose (#634).** This function used
    /// to open with `if auth_state.is_onboarded() == Some(false) { ... }`, holding
    /// four first-run overrides: `disable_default_regex_trigger`, the Adeberry
    /// theme, the Windows 16px monospace default, and Universal input box (plus its
    /// `honor_ps1` follow-up). None of them ever ran. This fork has no first-run
    /// experience: the local placeholder user hardcodes `is_onboarded: true`
    /// (`app/src/auth/mod.rs:213`) and nothing outside tests clears it, so the
    /// predicate is a constant `Some(true)` and the block was unreachable. It was
    /// previously kept "so it works again if a real onboarding state is ever
    /// introduced"; the decision is now that the declared defaults are the whole
    /// story, so the block is gone rather than lying about what ships.
    ///
    /// **Do not re-add it.** The effective defaults are the ones declared at the
    /// settings themselves — `InputBoxType::Classic`
    /// (`app/src/settings/input.rs`), 13px monospace on every platform including
    /// Windows (`app/src/settings/font.rs`), `ThemeKind::PhosphorAmber`
    /// (`app/src/themes/theme.rs`) — and removing the block changed none of them.
    /// A first-run experience is a product decision, not a migration; building one
    /// means giving `is_onboarded` a real source first.
    /// `startup_migrations_do_not_apply_new_user_overrides` below fails if the block
    /// comes back.
    ///
    /// Audit of every migration left in here for "pin predicate reading a
    /// fork-diverged default", so it is not re-derived (the NLD one below was an
    /// instance and is fixed; the rest are recorded as checked):
    ///
    /// * NLD-in-terminal — **was** an instance; see the long comment at its call
    ///   site. `nld_in_terminal_enabled_internal` defaults `true` here vs `false` at
    ///   the pin.
    /// * `KeepThinkingExpanded` -> `ThinkingDisplayMode` — reads the two raw
    ///   preference keys directly rather than any typed default, and only writes
    ///   when the old key was explicitly `true`. Immune to default divergence.
    pub fn apply_startup_settings_migrations(&self, ctx: &mut ModelContext<Self>) {
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

    use warp_core::features::FeatureFlag;
    use warp_core::settings::Setting;
    use warpui::{App, SingletonEntity};

    use super::SettingsInitializer;
    use crate::settings::input::InputBoxType;
    use crate::settings::{AISettings, FontSettings, InputSettings, PrivacySettings, ThemeSettings};
    use crate::terminal::session_settings::SessionSettings;
    use crate::test_util::settings::initialize_settings_for_tests;

    fn run_startup_migrations(app: &mut App) {
        app.add_singleton_model(|_| SettingsInitializer::new());
        app.update(|ctx| {
            // Mirrors `settings::run_startup_settings_initialization`.
            SettingsInitializer::handle(ctx).update(ctx, |initializer, ctx| {
                initializer.apply_startup_settings_migrations(ctx);
            });
        });
    }

    /// #634: this fork has no first-run experience, so startup must leave the
    /// declared defaults exactly as declared.
    ///
    /// The migrations used to open with `if auth_state.is_onboarded() == Some(false)`,
    /// holding **all five** of the overrides asserted below. The predicate was a
    /// constant `Some(true)` — `User::test()` hardcodes `is_onboarded: true` — so
    /// none of it ran, and the block was removed rather than left implying a first
    /// run that does not exist.
    ///
    /// What makes this fail: re-adding any of those overrides, including behind a
    /// predicate that *can* be false. A first-run experience is a product decision
    /// this fork has not made — see the note on `apply_startup_settings_migrations`.
    ///
    /// **Where this test's reach ends, so nobody over-trusts it.** It can only see
    /// what `apply_startup_settings_migrations` writes. Dropping `auth_state` from
    /// that signature is what forces a would-be restorer through a visible API
    /// change; a restore that instead read `AuthStateProvider::as_ref(ctx)` off the
    /// `ModelContext` would compile, and every assertion here would still hold
    /// **as long as `is_onboarded()` stays a constant `Some(true)`** — which is the
    /// same fact that makes the block dead in the first place. The day someone gives
    /// `is_onboarded` a real source, this test starts catching that restore too.
    ///
    /// Note both halves of each assertion. The value alone is not enough: an
    /// override that writes the same value the default already has — `honor_ps1`
    /// was exactly that, already `false` — still marks the setting *explicitly set*,
    /// which pins it forever and makes the default unrecoverable even by deleting the
    /// migration again.
    #[test]
    fn startup_migrations_do_not_apply_new_user_overrides() {
        App::test((), |mut app| async move {
            initialize_settings_for_tests(&mut app);
            // Not in `initialize_settings_for_tests` (production registers it later,
            // from `initialize_app`), so the secret-redaction assertion below needs
            // it added here. Same pattern as the other tests that touch it.
            app.add_singleton_model(PrivacySettings::mock);

            let (font_size_before, input_box_type_before, theme_before) = app.read(|ctx| {
                assert!(
                    !*PrivacySettings::as_ref(ctx)
                        .has_initialized_default_secret_regexes
                        .value(),
                    "precondition: a fresh profile has not been seeded with the \
                     recommended secret-redaction regexes yet"
                );
                (
                    *FontSettings::as_ref(ctx).monospace_font_size.value(),
                    *InputSettings::as_ref(ctx).input_box_type.value(),
                    ThemeSettings::as_ref(ctx).theme_kind.value().clone(),
                )
            });
            assert_eq!(
                input_box_type_before,
                InputBoxType::Classic,
                "precondition: the declared default input box is Classic on every platform"
            );

            run_startup_migrations(&mut app);

            app.read(|ctx| {
                let input_settings = InputSettings::as_ref(ctx);
                assert_eq!(
                    *input_settings.input_box_type.value(),
                    input_box_type_before,
                    "startup must not switch the input box to Universal: that override was \
                     the dead new-user branch removed in #634"
                );
                assert!(
                    !input_settings.input_box_type.is_value_explicitly_set(),
                    "startup must not write input_box_type at all -- an explicit write pins \
                     the value and the declared default could never apply again"
                );

                // The Universal-input override's follow-up. `honor_ps1` already
                // defaults to `false`, so only the "explicitly set" half of this can
                // ever fail -- which is exactly the half that matters.
                assert!(
                    !SessionSettings::as_ref(ctx).honor_ps1.is_value_explicitly_set(),
                    "startup must not write honor_ps1: it was the Universal-input \
                     override's follow-up, and writing the value it already has would \
                     still pin it"
                );

                let font_settings = FontSettings::as_ref(ctx);
                assert_eq!(
                    *font_settings.monospace_font_size.value(),
                    font_size_before,
                    "startup must not raise the monospace font size (the removed branch set \
                     16px on Windows); the declared default applies on every platform"
                );
                assert!(
                    !font_settings.monospace_font_size.is_value_explicitly_set(),
                    "startup must not write monospace_font_size at all"
                );

                let theme_settings = ThemeSettings::as_ref(ctx);
                assert_eq!(
                    theme_settings.theme_kind.value(),
                    &theme_before,
                    "startup must not override the theme; the removed branch swapped \
                     Phenomenon for Adeberry, and PhosphorAmber is this fork's default"
                );
                assert!(
                    !theme_settings.theme_kind.is_value_explicitly_set(),
                    "startup must not write theme_kind at all"
                );

                // The one with a security consequence, and the reason this assertion
                // exists at all. The removed branch called
                // `PrivacySettings::disable_default_regex_trigger`, which sets this
                // guard to `true` WITHOUT seeding anything -- so a "new user" got a
                // redactor compiled from an empty list and no secret in terminal
                // output was ever blurred. `run_startup_settings_initialization`
                // calls `initialize_default_regexes_once` immediately after these
                // migrations and skips seeding when the guard is already set, so a
                // migration that pre-sets it silently disables redaction for
                // everyone.
                assert!(
                    !*PrivacySettings::as_ref(ctx)
                        .has_initialized_default_secret_regexes
                        .value(),
                    "startup migrations must not pre-set \
                     HasInitializedDefaultSecretRegexes: the very next step, \
                     initialize_default_regexes_once, skips seeding when it is already \
                     true, so this would leave secret redaction matching nothing"
                );
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
