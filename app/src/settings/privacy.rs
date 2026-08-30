use std::fmt::Display;
use std::sync::Arc;

use anyhow::Result;
use regex::Regex;
use warp_core::features::FeatureFlag;
use warp_core::report_if_error;
use warp_core::user_preferences::GetUserPreferences as _;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity, UpdateModel};

use crate::ai::blocklist::telemetry_banner::should_collect_ai_ugc_telemetry;
use crate::auth::AuthState;
use crate::auth::AuthStateProvider;
use crate::auth::SyncedUserSettings;
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::report_error;
// Zap Wave 3-1: the `AuthClient` trait + `MockAuthClient` were removed entirely along with
// server_api/auth.rs; `SyncedUserSettings` moved to `crate::auth`.
// Zap Wave 3-1: `ServerApiProvider` is no longer used by this file --
// all call sites of `auth_client = ServerApiProvider::as_ref(ctx).get_auth_client()`
// were removed along with the AuthClient trait.
use crate::terminal::safe_mode_settings::SafeModeSettings;

use settings::{
    macros::{define_settings_group, maybe_define_setting, register_settings_events},
    RespectUserSyncSetting, Setting, SupportedPlatforms, SyncToCloud,
};

use serde::{Deserialize, Serialize};

// Zap (localization, Phase 5): `PreferencesSyncer` has been removed entirely.
use crate::workspaces::workspace::EnterpriseSecretRegex;

pub trait RegexDisplayInfo {
    fn pattern(&self) -> &str;
    fn name(&self) -> Option<&str>;
}

pub const TELEMETRY_ENABLED_DEFAULTS_KEY: &str = "TelemetryEnabled";
pub const CRASH_REPORTING_ENABLED_DEFAULTS_KEY: &str = "CrashReportingEnabled";

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(description = "A custom regex pattern for detecting and redacting secrets.")]
pub struct CustomSecretRegex {
    #[serde(with = "serde_regex")]
    #[schemars(with = "String", description = "The regex pattern to match secrets.")]
    pub pattern: Regex,
    #[serde(default)]
    #[schemars(description = "Optional display name for this secret pattern.")]
    pub name: Option<String>,
}

impl CustomSecretRegex {
    pub fn pattern(&self) -> &Regex {
        &self.pattern
    }
}

impl RegexDisplayInfo for CustomSecretRegex {
    fn pattern(&self) -> &str {
        self.pattern.as_str()
    }

    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

impl RegexDisplayInfo for EnterpriseSecretRegex {
    fn pattern(&self) -> &str {
        &self.pattern
    }

    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

impl Display for CustomSecretRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pattern.as_str())
    }
}

impl PartialEq for CustomSecretRegex {
    /// We do not factor in the name to equality checks --
    /// if the regex is the same, then the regex is the same.
    /// This allows us to avoid adding duplicate regexes.
    fn eq(&self, other: &Self) -> bool {
        self.pattern.as_str() == other.pattern.as_str()
    }
}

impl settings_value::SettingsValue for CustomSecretRegex {}

// openWarp closed-source telemetry strip-out: the three privacy toggles' defaults changed from
// true -> false. The original Zap default of "on" was the commercial product's "opt-out" model;
// Zap has physically cut off the three outbound channels (telemetry, crash reporting, cloud
// conversation storage), so leaving the default "on" would show ON to new users while nothing
// actually goes out, which is a confusing disconnect. Changed to default OFF.
define_settings_group!(WarpDrivePrivacySettings, settings: [
    is_telemetry_enabled: IsTelemetryEnabled {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
        private: false,
        storage_key: "TelemetryEnabled",
        toml_path: "privacy.telemetry_enabled",
        description: "Whether anonymous usage telemetry is collected.",
    },
    is_crash_reporting_enabled: IsCrashReportingEnabled {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
        private: false,
        storage_key: "CrashReportingEnabled",
        toml_path: "privacy.crash_reporting_enabled",
        description: "Whether crash reports are sent.",
    },
]);

maybe_define_setting!(CustomSecretRegexList, group: PrivacySettings, {
    type: Vec<CustomSecretRegex>,
    default: Vec::new(),
    supported_platforms: SupportedPlatforms::ALL,
    sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
    private: false,
    toml_path: "privacy.custom_secret_regex_list",
    description: "Custom regex patterns for detecting and redacting secrets.",
});

maybe_define_setting!(HasInitializedDefaultSecretRegexes, group: PrivacySettings, {
    type: bool,
    default: false,
    supported_platforms: SupportedPlatforms::ALL,
    sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
    private: true,
});

/// Singleton model for managing the user's privacy settings (whether the user has enabled crash
/// reporting and/or telemetry).
pub struct PrivacySettings {
    auth_state: Arc<AuthState>,
    // Zap Wave 3-1: the `auth_client: Arc<dyn AuthClient>` field was removed along with the
    // AuthClient trait. It was originally used to sync to the server when telemetry / crash
    // reporting settings changed; Zap no longer syncs any server-side settings.
    pub is_telemetry_enabled: bool,
    pub is_crash_reporting_enabled: bool,
    pub has_initialized_default_secret_regexes: HasInitializedDefaultSecretRegexes,
    /// List of user defined secret regexes.
    /// Enterprise-level secret regexes will always take precedence over user-level secrets,
    /// but they both used to support additive behavior.
    /// It's a [Vec<CustomSecretRegex>], but also a user setting.
    pub user_secret_regex_list: CustomSecretRegexList,
    /// List of enterprise-level secret regexes provided by the organization.
    /// These are kept separate from user-level secrets to support additive behavior.
    pub enterprise_secret_regex_list: Vec<CustomSecretRegex>,
    /// Whether or not the user's organization has forced telemetry on, in which case we ignore any
    /// user local/cloud settings. If false, we fall back to the user's settings.
    /// This is populated by the server when teams data is fetched.
    pub is_telemetry_force_enabled: bool,
    /// Whether or not the user's organization has enabled enterprise secret redaction.
    /// This is populated by the server when teams data is fetched.
    pub is_enterprise_secret_redaction_enabled: bool,
}

/// A snapshot of a user's [`PrivacySettings`] settings at some point in time.
#[derive(Clone, Copy)]
pub struct PrivacySettingsSnapshot {
    is_telemetry_enabled: bool,
    is_crash_reporting_enabled: bool,
    is_telemetry_force_enabled: bool,
    should_collect_ai_ugc_telemetry: bool,
}

impl PrivacySettingsSnapshot {
    pub fn is_telemetry_enabled(&self) -> bool {
        self.is_telemetry_enabled
    }

    pub fn is_crash_reporting_enabled(&self) -> bool {
        self.is_crash_reporting_enabled
    }

    pub fn is_telemetry_force_enabled(&self) -> bool {
        self.is_telemetry_force_enabled
    }

    pub fn should_disable_telemetry(&self) -> bool {
        // If a user has opted in to the agent mode analytics experiment, telemetry must be enabled.
        !self.is_telemetry_enabled
            && !self.is_telemetry_force_enabled
            && !FeatureFlag::AgentModeAnalytics.is_enabled()
    }

    pub fn should_collect_ai_ugc_telemetry(&self) -> bool {
        self.should_collect_ai_ugc_telemetry
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn mock() -> Self {
        Self {
            is_telemetry_enabled: true,
            is_crash_reporting_enabled: true,
            is_telemetry_force_enabled: true,
            should_collect_ai_ugc_telemetry: true,
        }
    }
}

impl PrivacySettings {
    /// Registers a singleton PrivacySettings model on `app`.
    ///
    /// We expose this function publicly (while keeping the constructor private) to prevent
    /// instantiation another PrivacySettings struct, in the case where a developer might be
    /// unaware that it is registered as a singleton model.
    pub fn register_singleton(ctx: &mut AppContext) {
        let handle = ctx.add_singleton_model(PrivacySettings::new);

        register_settings_events!(
            PrivacySettings,
            user_secret_regex_list,
            CustomSecretRegexList,
            handle,
            ctx
        );
    }

    /// Returns a new PrivacySettings object initialized from locally cached values. Server-side
    /// settings are fetched later via `fetch_or_update_settings`, which is called from
    /// `on_user_fetched` after the user's auth state is established.
    fn new(ctx: &mut ModelContext<Self>) -> Self {
        let auth_state = AuthStateProvider::as_ref(ctx).get().clone();

        // openWarp closed-source telemetry strip-out: also defaults to false when user_preferences
        // has no value, matching the setting macro's default.
        let is_telemetry_enabled: bool = ctx
            .private_user_preferences()
            .read_value(TELEMETRY_ENABLED_DEFAULTS_KEY)
            .unwrap_or_default()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(false);

        let is_crash_reporting_enabled: bool = ctx
            .private_user_preferences()
            .read_value(CRASH_REPORTING_ENABLED_DEFAULTS_KEY)
            .unwrap_or_default()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(false);

        // Make sure the user-preferences stores match what's in memory.
        // Needed for zap drive preferences to work and no harm in doing in general.
        let _ = ctx.private_user_preferences().write_value(
            TELEMETRY_ENABLED_DEFAULTS_KEY,
            serde_json::to_string(&is_telemetry_enabled)
                .expect("is_telemetry_enabled is a boolean."),
        );
        let _ = ctx.private_user_preferences().write_value(
            CRASH_REPORTING_ENABLED_DEFAULTS_KEY,
            serde_json::to_string(&is_crash_reporting_enabled)
                .expect("is_crash_reporting_enabled is a boolean."),
        );

        // Listen for changes to the object store and update ourselves when they happen.
        ctx.subscribe_to_model(&WarpDrivePrivacySettings::handle(ctx), |me, event, ctx| {
            let privacy_settings = WarpDrivePrivacySettings::as_ref(ctx);
            match event {
                WarpDrivePrivacySettingsChangedEvent::IsTelemetryEnabled { .. } => {
                    me.set_is_telemetry_enabled(
                        *privacy_settings.is_telemetry_enabled.value(),
                        ctx,
                    );
                }
                WarpDrivePrivacySettingsChangedEvent::IsCrashReportingEnabled { .. } => {
                    me.set_is_crash_reporting_enabled(
                        *privacy_settings.is_crash_reporting_enabled.value(),
                        ctx,
                    );
                }
            }
        });

        let user_secret_regex_list: CustomSecretRegexList =
            CustomSecretRegexList::new_from_storage(ctx);
        let has_initialized_default_secret_regexes: HasInitializedDefaultSecretRegexes =
            HasInitializedDefaultSecretRegexes::new_from_storage(ctx);

        Self {
            auth_state,
            is_crash_reporting_enabled,
            is_telemetry_enabled,
            user_secret_regex_list,
            has_initialized_default_secret_regexes,
            is_telemetry_force_enabled: false,
            is_enterprise_secret_redaction_enabled: false,
            enterprise_secret_regex_list: Vec::new(),
        }
    }

    pub fn is_telemetry_force_enabled(&self) -> bool {
        self.is_telemetry_force_enabled
    }

    pub fn set_is_telemetry_force_enabled(&mut self, is_telemetry_force_enabled: bool) {
        self.is_telemetry_force_enabled = is_telemetry_force_enabled;
    }

    pub fn is_enterprise_secret_redaction_enabled(&self) -> bool {
        self.is_enterprise_secret_redaction_enabled
    }

    pub fn set_enterprise_secret_redaction_settings(
        &mut self,
        enabled: bool,
        enterprise_regexes: Vec<EnterpriseSecretRegex>,
        change_event_reason: ChangeEventReason,
        ctx: &mut ModelContext<Self>,
    ) {
        if enabled {
            // First time: Force enable secret redaction setting (safe mode).
            if !self.is_enterprise_secret_redaction_enabled {
                let safe_mode_settings = SafeModeSettings::handle(ctx);
                ctx.update_model(&safe_mode_settings, |safe_mode_settings, ctx| {
                    let _ = safe_mode_settings.safe_mode_enabled.set_value(true, ctx);
                });
            }

            // Convert EnterpriseSecretRegex to CustomSecretRegex for internal use
            let mut enterprise_secrets = Vec::new();
            for enterprise_regex in enterprise_regexes {
                match Regex::new(&enterprise_regex.pattern) { Ok(regex) => {
                    enterprise_secrets.push(CustomSecretRegex {
                        pattern: regex,
                        name: enterprise_regex.name,
                    });
                } _ => {
                    log::error!(
                        "Invalid enterprise secret regex pattern: {}",
                        enterprise_regex.pattern
                    );
                }}
            }
            self.enterprise_secret_regex_list = enterprise_secrets;
        } else {
            // Clear enterprise secrets when disabled
            self.enterprise_secret_regex_list.clear();
        }

        self.is_enterprise_secret_redaction_enabled = enabled;

        ctx.emit(PrivacySettingsChangedEvent::CustomSecretRegexList {
            change_event_reason,
        });
        ctx.notify();
    }

    pub fn refresh_to_default(&mut self) {
        // TODO(zach): this seems incorrect - should we also update the values on disk?
        self.is_telemetry_enabled = true;
        self.is_crash_reporting_enabled = true;
        self.is_telemetry_force_enabled = false;
        self.is_enterprise_secret_redaction_enabled = false;
    }

    /// Fetch the user's privacy settings from the server if any or update the server settings.
    pub fn fetch_or_update_settings(&self, _ctx: &mut ModelContext<Self>) {
        // Zap Wave 3-1: the original call to `auth_client.get_user_settings().await` was
        // removed along with the entire AuthClient trait. After Zap's localization, privacy
        // settings are only saved locally; this entry point is a no-op.
    }

    /// Initializes state from the [`SyncedUserSettings`] fetched from the server, if any.
    /// If there are no settings from the server, updates the server settings with local settings.
    /// TODO: Make this a server-side db transaction.
    fn initialize_from_fetched_settings_or_update_settings(
        &mut self,
        fetched_settings: Result<Option<SyncedUserSettings>>,
        ctx: &mut ModelContext<PrivacySettings>,
    ) {
        match fetched_settings {
            Ok(Some(fetched_settings)) => {
                // Until the login experience stops hiding the telemetry settings,
                // we assume that locally enabled telemetry is unintentional.
                // As such, where settings differ, we respect whichever setting that is disabled.
                self.overwrite_local_settings_if_cloud_disabled(fetched_settings, ctx);
                // If any local setting is disabled, we have to update the server.
                if !self.is_telemetry_enabled || !self.is_crash_reporting_enabled {
                    self.update_server_with_local_settings(ctx);
                }
            }
            Ok(None) => {
                // This indicates the user had not logged in before.
                log::info!("User has no synced privacy settings.");
                self.update_server_with_local_settings(ctx);
            }
            Err(err) => {
                report_error!(err.context("Failed to fetch user settings."));
            }
        }

        self.maybe_sync_with_warp_drive_prefs(ctx);
    }

    fn overwrite_local_settings_if_cloud_disabled(
        &mut self,
        fetched_settings: SyncedUserSettings,
        ctx: &mut ModelContext<Self>,
    ) {
        // For now, only overwrite the user's locally stored setting if the cloud version
        // has is_crash_reporting disabled. Until we implement a more reliable retry
        // mechanism for update settings requests, in addition to possibly a UI for the
        // user to resolve the conflicting settings themselves, default to "safe" behavior.
        // Namely, we want to avoid incidentally overwriting is_crash_reporting_enabled to
        // `true`.
        if self.is_crash_reporting_enabled && !fetched_settings.is_crash_reporting_enabled {
            self.set_is_crash_reporting_enabled(fetched_settings.is_crash_reporting_enabled, ctx);
        }

        // For now, only overwrite the user's locally stored setting if the cloud version
        // has is_telemetry_enabled disabled. Until we implement a more reliable retry
        // mechanism for update settings requests, in addition to possibly a UI for the
        // user to resolve the conflicting settings themselves, default to "safe" behavior.
        // Namely, we want to avoid incidentally overwriting is_telemetry_enabled to
        // `true`.
        if self.is_telemetry_enabled && !fetched_settings.is_telemetry_enabled {
            self.set_is_telemetry_enabled(fetched_settings.is_telemetry_enabled, ctx);
        }
    }

    /// Constructor for tests only.
    #[cfg(any(test, feature = "test-util"))]
    pub fn mock(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            auth_state: Arc::new(AuthState::new_for_test()),
            is_crash_reporting_enabled: true,
            is_telemetry_enabled: true,
            user_secret_regex_list: CustomSecretRegexList::new(None),
            has_initialized_default_secret_regexes: HasInitializedDefaultSecretRegexes::new(None),
            is_telemetry_force_enabled: false,
            is_enterprise_secret_redaction_enabled: false,
            enterprise_secret_regex_list: Vec::new(),
        }
    }

    /// Returns a snapshot of the user's privacy settings.
    ///
    /// The returned snapshot is not stateful, thus its values should be used shortly after the
    /// snapshot is returned.
    pub fn get_snapshot(&self, app: &AppContext) -> PrivacySettingsSnapshot {
        PrivacySettingsSnapshot {
            is_telemetry_enabled: self.is_telemetry_enabled,
            is_crash_reporting_enabled: self.is_crash_reporting_enabled,
            is_telemetry_force_enabled: self.is_telemetry_force_enabled,
            should_collect_ai_ugc_telemetry: should_collect_ai_ugc_telemetry(
                app,
                self.is_telemetry_enabled,
            ),
        }
    }

    /// Sets `is_crash_reporting_enabled` to the given value.
    ///
    /// Additionally, this writes the given value to the user's local defaults, and additionally
    /// sends a request to update the user's `is_crash_reporting_enabled` value stored server-side.
    /// Finally, emits a `PrivacySettingsEvent::UpdateIsCrashReportingEnabled` event.
    pub fn set_is_crash_reporting_enabled(
        &mut self,
        new_value: bool,
        ctx: &mut ModelContext<PrivacySettings>,
    ) {
        let old_value = self.is_crash_reporting_enabled;
        if new_value != old_value {
            self.is_crash_reporting_enabled = new_value;

            WarpDrivePrivacySettings::handle(ctx).update(ctx, |settings, ctx| {
                log::info!("Setting is_crash_reporting_enabled to {new_value}");
                let _ = settings
                    .is_crash_reporting_enabled
                    .set_value(new_value, ctx);
            });

            if self.auth_state.is_logged_in() {
                // Zap Wave 3-1: the original call to `auth_client.set_is_crash_reporting_enabled(new_value)`
                // was removed along with the AuthClient trait. Zap only updates local state now.
                log::debug!(
                    "set_is_crash_reporting_enabled remote sync has been localized, new_value={new_value}"
                );
            }
            ctx.emit(PrivacySettingsChangedEvent::UpdateIsCrashReportingEnabled {
                old_value,
                new_value,
            });
            ctx.notify();
        }
    }

    /// Sets `is_telemetry_enabled` to the given value.
    ///
    /// Additionally, this writes the given value to the user's local defaults, and additionally
    /// sends a request to update the user's `is_telemetry_enabled` value stored server-side.
    /// Finally, emits a `PrivacySettingsEvent::UpdateIsTelemetryEnabled` event.
    pub fn set_is_telemetry_enabled(
        &mut self,
        new_value: bool,
        ctx: &mut ModelContext<PrivacySettings>,
    ) {
        let old_value = self.is_telemetry_enabled;
        if new_value != old_value {
            self.is_telemetry_enabled = new_value;

            WarpDrivePrivacySettings::handle(ctx).update(ctx, |settings, ctx| {
                log::info!("Setting is_telemetry_enabled to {new_value}");
                let _ = settings.is_telemetry_enabled.set_value(new_value, ctx);
            });

            if self.auth_state.is_logged_in() {
                // Zap Wave 3-1: same as above.
                log::debug!("set_is_telemetry_enabled remote sync has been localized, new_value={new_value}");
            }
            ctx.emit(PrivacySettingsChangedEvent::UpdateIsTelemetryEnabled {
                old_value,
                new_value,
            });
            ctx.notify();
        }
    }

    pub fn remove_user_secret_regex(&mut self, idx: &usize, ctx: &mut ModelContext<Self>) {
        let mut new_user_secret_regex_list = self.user_secret_regex_list.to_vec();
        new_user_secret_regex_list.remove(*idx);
        if self
            .user_secret_regex_list
            .set_value(new_user_secret_regex_list, ctx)
            .is_err()
        {
            log::error!("Custom Secret Regex List failed to serialize")
        }
    }

    /// Initializes the custom secret regex list with the default regexes, provided
    /// non matches can be found.
    /// This can be called when a user first enables secret redaction.
    ///
    /// Returns whether the recommended regexes are now durably stored — `true`
    /// both when the write succeeded and when there was nothing to add. A
    /// `false` return means the list on disk does *not* contain them, and the
    /// caller must not record that seeding has happened; see
    /// [`Self::initialize_default_regexes_once`].
    pub fn add_all_recommended_regex(&mut self, ctx: &mut ModelContext<Self>) -> bool {
        let mut new_user_secret_regex_list = self.user_secret_regex_list.to_vec();
        let num_existing_regexes = new_user_secret_regex_list.len();

        // Add all the default regexes if they don't already exist
        for default_regex in crate::terminal::model::secrets::regexes::DEFAULT_REGEXES_WITH_NAMES {
            match Regex::new(default_regex.pattern) { Ok(regex) => {
                let custom_regex = CustomSecretRegex {
                    pattern: regex,
                    name: Some(default_regex.name.to_string()),
                };
                if !new_user_secret_regex_list.contains(&custom_regex) {
                    new_user_secret_regex_list.push(custom_regex);
                }
            } _ => {
                log::error!("Failed to compile default regex: {}", default_regex.pattern);
            }}
        }

        if num_existing_regexes == new_user_secret_regex_list.len() {
            // Nothing to add: every recommended regex is already present, so the
            // stored list is already in the desired state.
            return true;
        }

        if let Err(err) = self
            .user_secret_regex_list
            .set_value(new_user_secret_regex_list, ctx)
        {
            // The write did not reach durable storage (a broken `settings.toml`
            // inhibits writes, and I/O errors are now propagated rather than
            // discarded by `Setting::write_to_preferences`). Report `false` so
            // the caller does not persist a "seeded" marker for a list that was
            // never seeded.
            log::error!("Failed to persist the recommended secret regexes: {err:?}");
            return false;
        }

        // `Ok(())` is not the same claim as "it is on disk". If
        // `privacy.custom_secret_regex_list` is present in `settings.toml` as
        // valid TOML but holds a value that will not deserialize -- one pattern
        // that does not compile is enough, in a list users are explicitly
        // invited to hand-edit -- then `read_from_preferences` inhibits writes
        // for that one key, the setting falls back to the empty default, and
        // the seeding write is accepted and silently dropped. Whole-file
        // inhibition errors; this per-key case deliberately does not, because
        // making it error would break resetting exactly those settings whose
        // stored value is broken. So the write cannot report the failure and
        // only a read-back can see it. Without this check the guard below is
        // written `true` into the *native* store -- which is not inhibited --
        // and the user has no secret redaction, permanently, with no signal.
        if !CustomSecretRegexList::is_value_durably_stored(
            self.user_secret_regex_list.value(),
            CustomSecretRegexList::preferences_for_setting(ctx),
        ) {
            log::error!(
                "The recommended secret regexes were accepted in memory but are not in \
                 durable storage; writes for privacy.custom_secret_regex_list are most \
                 likely inhibited by an unparseable value in settings.toml"
            );
            ctx.notify();
            return false;
        }

        ctx.notify();
        true
    }

    /// Disables the default regex trigger, so that it will not be executed.
    ///
    /// **No caller since #634.** Its only one was the first-run branch of
    /// `SettingsInitializer::apply_startup_settings_migrations`, which suppressed the
    /// recommended secret-redaction regexes for a brand-new user so they were never
    /// added without an explicit action. That branch was unreachable in this fork
    /// (`is_onboarded()` is a constant `Some(true)`) and has been removed, so every
    /// user is seeded — which is the fork's intent: the redactor otherwise compiles a
    /// match-nothing regex and blurs no secrets at all. Kept because it is the only
    /// way to set the guard without also writing the list, and a real first-run state
    /// would need it back.
    pub fn disable_default_regex_trigger(&mut self, ctx: &mut ModelContext<Self>) {
        if self
            .has_initialized_default_secret_regexes
            .set_value(true, ctx)
            .is_err()
        {
            log::error!("Failed to disable default regex trigger");
        }
    }

    /// Initializes the custom secret regex list with the default regexes.
    /// This will only be executed once per user, and only if they haven't already initialized.
    ///
    /// Zap: called from `settings::run_startup_settings_initialization` on every
    /// launch. That is safe to repeat, and specifically cannot resurrect patterns
    /// the user removed: the guard is the persisted private setting
    /// `HasInitializedDefaultSecretRegexes`, not the contents of the list, and it is
    /// set the first time seeding runs. So an empty list means "seeded, then emptied
    /// on purpose" once the flag is set, and only a user who has never been seeded
    /// gets the defaults.
    ///
    /// The guard is set **only** on a confirmed durable write of the regex list.
    /// The two settings live in different stores — `CustomSecretRegexList` is
    /// public, so it goes to `settings.toml`, while
    /// `HasInitializedDefaultSecretRegexes` is private, so it goes to the
    /// platform-native store — and only the first can be inhibited by a syntax
    /// error in `settings.toml`. Setting the guard unconditionally therefore
    /// cached a failure as a permanent success: a single launch with a broken
    /// settings file left the redaction list empty and the guard `true`, so
    /// seeding never ran again and the user had no secret redaction at all, with
    /// no signal, recoverable only by finding Settings > Privacy > "Add all
    /// recommended".
    pub fn initialize_default_regexes_once(&mut self, ctx: &mut ModelContext<Self>) {
        // Only initialize if we haven't done so before
        if !*self.has_initialized_default_secret_regexes.value() {
            if !self.add_all_recommended_regex(ctx) {
                // Leave the guard clear so the next launch retries once the
                // user has fixed their settings file.
                log::error!(
                    "Not marking the recommended secret regexes as initialized: the list \
                     could not be persisted. Seeding will be retried on the next launch."
                );
                return;
            }

            // Mark as initialized
            if self
                .has_initialized_default_secret_regexes
                .set_value(true, ctx)
                .is_err()
            {
                log::error!("Failed to set has_initialized_default_secret_regexes flag");
            }
        }
    }

    /// openWarp closed-source telemetry strip-out P3: privacy settings are no longer synced to
    /// upstream cloud settings.
    /// After strip-out, this is purely local persistence (the caller still writes to
    /// settings.toml + the warp_drive local cache), with no outbound sync.
    fn update_server_with_local_settings(&self, _ctx: &mut ModelContext<Self>) {}

    /// We wait until zap drive prefs have loaded and then either
    /// 1) use them as the data store for is_telemetry_enabled and is_crash_reporting_enabled, if those
    ///    values are set in zap drive, or
    /// 2) update the zap drive prefs to match the values from the legacy user_settings endpoint so
    ///    that we can use zap drive prefs going forward.
    pub fn maybe_sync_with_warp_drive_prefs(&mut self, ctx: &mut ModelContext<Self>) {
        // Wait for cloud objects to load, and, if telemetry & crash reporting are synced to zap drive
        // initialize from the zap drive values.
        ctx.spawn(
            ObjectStoreModel::as_ref(ctx).initial_load_complete(),
            Self::handle_warp_drive_objects_loaded,
        );
    }

    fn handle_warp_drive_objects_loaded(&mut self, _: (), ctx: &mut ModelContext<Self>) {
        self.initialize_default_regexes_once(ctx);
        // Check if the zap drive preferences are set. If they are, and telemetry and crash reporting
        // are set as zap drive prefs, then use those.  Otherwise, update the zap drive prefs to match
        // the values from the legacy user_settings endpoint so that we can use zap drive prefs going forward.
        let object_store_model = ObjectStoreModel::as_ref(ctx);
        let cloud_prefs = object_store_model.get_all_preferences_by_storage_key();
        let cloud_telemetry_value =
            cloud_prefs
                .get(IsTelemetryEnabled::storage_key())
                .map(|pref| {
                    pref.model()
                        .string_model
                        .value
                        .as_bool()
                        .unwrap_or_default()
                });
        let cloud_crash_reporting_value = cloud_prefs
            .get(IsCrashReportingEnabled::storage_key())
            .map(|pref| {
                pref.model()
                    .string_model
                    .value
                    .as_bool()
                    .unwrap_or_default()
            });

        match (cloud_telemetry_value, cloud_crash_reporting_value) {
            (Some(is_telemetry_enabled), Some(is_crash_reporting_enabled)) => {
                log::info!(
                    "Phosphor Drive privacy preferences are set, using those for telemetry={is_telemetry_enabled}, \
                    crash_reporting={is_crash_reporting_enabled}"
                );
                self.set_is_telemetry_enabled(is_telemetry_enabled, ctx);
                self.set_is_crash_reporting_enabled(is_crash_reporting_enabled, ctx);
            }
            _ => {
                log::info!(
                    "Phosphor Drive privacy preferences are not set, syncing local PrivacySettings values to \
                    WarpDrivePrivacySettings and cloud. telemetry={}, crash_reporting={}",
                    self.is_telemetry_enabled,
                    self.is_crash_reporting_enabled
                );
                WarpDrivePrivacySettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings
                        .is_telemetry_enabled
                        .set_value(self.is_telemetry_enabled, ctx));
                    report_if_error!(settings
                        .is_crash_reporting_enabled
                        .set_value(self.is_crash_reporting_enabled, ctx));
                });
                // Zap (localization, Phase 5): the original `PreferencesSyncer::maybe_sync_local_prefs_to_cloud`,
                // which synced local privacy settings to the cloud, was removed along with the syncer.
                // Local settings are now only written to sqlite.
            }
        }
    }
}

/// Events emitted when PrivacySettings is updated.
#[derive(Clone, Copy)]
pub enum PrivacySettingsChangedEvent {
    UpdateIsTelemetryEnabled {
        old_value: bool,
        new_value: bool,
    },
    UpdateIsCrashReportingEnabled {
        old_value: bool,
        new_value: bool,
    },
    CustomSecretRegexList {
        change_event_reason: ChangeEventReason,
    },
    HasInitializedDefaultSecretRegexes {
        change_event_reason: ChangeEventReason,
    },
}

impl Entity for PrivacySettings {
    type Event = PrivacySettingsChangedEvent;
}

impl SingletonEntity for PrivacySettings {}

#[cfg(test)]
mod default_regex_seeding_tests {
    //! Inline because there is no `privacy_tests.rs` sibling. These cover the
    //! one-shot guard around the recommended secret-redaction regexes, which
    //! used to record a failed write as a permanent success.

    use settings::{
        is_settings_file_enabled, set_settings_file_enabled, PrivatePreferences, PublicPreferences,
        Setting, SettingsManager,
    };
    use warpui::{App, SingletonEntity};
    use warpui_extras::user_preferences::{
        in_memory::InMemoryPreferences, toml_backed::TomlBackedUserPreferences,
    };

    use super::PrivacySettings;

    /// `set_settings_file_enabled` is process-global; restore it even on panic.
    struct SettingsFileEnabledGuard(bool);

    impl SettingsFileEnabledGuard {
        fn new(enabled: bool) -> Self {
            let previous = is_settings_file_enabled();
            set_settings_file_enabled(enabled);
            Self(previous)
        }
    }

    impl Drop for SettingsFileEnabledGuard {
        fn drop(&mut self) {
            set_settings_file_enabled(self.0);
        }
    }

    /// `CustomSecretRegexList` is public, so it is written to `settings.toml`;
    /// `HasInitializedDefaultSecretRegexes` is private, so it is written to the
    /// platform-native store. Only the first can be inhibited by a syntax error
    /// in `settings.toml`.
    ///
    /// The two stores are deliberately given different backends here: the public
    /// one is a real TOML store over an unparseable file (so its writes fail),
    /// and the private one is in-memory (so the guard write *would* succeed).
    /// That is what makes this test non-vacuous — if the guard stays clear it is
    /// because we declined to set it, not because setting it was impossible.
    #[test]
    #[serial_test::serial]
    fn guard_is_not_set_when_the_regex_list_write_fails() {
        let _settings_file = SettingsFileEnabledGuard::new(true);

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("settings.toml");
        std::fs::write(&file_path, "this is [not valid toml =").unwrap();

        App::test((), |mut app| async move {
            app.add_singleton_model(move |_| {
                let (prefs, parse_error) = TomlBackedUserPreferences::new(file_path);
                assert!(
                    parse_error.is_some(),
                    "precondition: the settings file must fail to parse"
                );
                PublicPreferences::new(Box::new(prefs))
            });
            app.add_singleton_model(|_| {
                PrivatePreferences::new(Box::<InMemoryPreferences>::default())
            });
            app.add_singleton_model(|_| SettingsManager::default());
            app.add_singleton_model(PrivacySettings::mock);

            app.update(|ctx| {
                PrivacySettings::handle(ctx).update(ctx, |privacy_settings, ctx| {
                    privacy_settings.initialize_default_regexes_once(ctx);
                });
            });

            app.read(|ctx| {
                let privacy_settings = PrivacySettings::as_ref(ctx);
                assert!(
                    privacy_settings.user_secret_regex_list.value().is_empty(),
                    "precondition: the seeding write could not reach the broken settings file"
                );
                assert!(
                    !*privacy_settings
                        .has_initialized_default_secret_regexes
                        .value(),
                    "the one-shot guard must not be set when the regex list was never \
                     persisted -- setting it strands the user with an empty redaction list \
                     forever, because seeding never runs again"
                );
            });
        });
    }

    /// The complement: with a writable settings file the seeding does happen and
    /// the guard *is* set, so the fix above cannot be satisfied by simply never
    /// setting the guard.
    #[test]
    #[serial_test::serial]
    fn guard_is_set_when_the_regex_list_write_succeeds() {
        let _settings_file = SettingsFileEnabledGuard::new(true);

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("settings.toml");

        App::test((), |mut app| async move {
            app.add_singleton_model(move |_| {
                let (prefs, parse_error) = TomlBackedUserPreferences::new(file_path);
                assert!(parse_error.is_none());
                PublicPreferences::new(Box::new(prefs))
            });
            app.add_singleton_model(|_| {
                PrivatePreferences::new(Box::<InMemoryPreferences>::default())
            });
            app.add_singleton_model(|_| SettingsManager::default());
            app.add_singleton_model(PrivacySettings::mock);

            app.update(|ctx| {
                PrivacySettings::handle(ctx).update(ctx, |privacy_settings, ctx| {
                    privacy_settings.initialize_default_regexes_once(ctx);
                });
            });

            app.read(|ctx| {
                let privacy_settings = PrivacySettings::as_ref(ctx);
                assert!(
                    !privacy_settings.user_secret_regex_list.value().is_empty(),
                    "the recommended regexes should have been seeded"
                );
                assert!(
                    *privacy_settings
                        .has_initialized_default_secret_regexes
                        .value(),
                    "the guard should be set once seeding actually persisted"
                );
            });
        });
    }
}
