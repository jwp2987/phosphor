use settings::{Setting as _, SettingsManager};
use warp_core::features::FeatureFlag;
use warpui::{rendering::GPUPowerPreference, AppContext, SingletonEntity};
use warpui_extras::user_preferences;

use crate::{
    appearance,
    banner::BannerState,
    drive::settings::WarpDriveSettings,
    report_if_error,
    resource_center::TipsCompleted,
    search::command_search::settings::CommandSearchSettings,
    terminal::{
        alt_screen_reporting::AltScreenReporting,
        general_settings::GeneralSettings,
        keys_settings::KeysSettings,
        ligature_settings::LigatureSettings,
        safe_mode_settings::SafeModeSettings,
        session_settings::{SessionSettings, SessionSettingsChangedEvent},
        settings::TerminalSettings,
        shared_session::settings::SharedSessionSettings,
        warpify::settings::WarpifySettings,
        BlockListSettings,
    },
    undo_close::UndoCloseSettings,
    window_settings::WindowSettings,
    workflows::aliases::WorkflowAliases,
    workspace::tab_settings::TabSettings,
};

use warp_core::semantic_selection::SemanticSelection;

use super::{
    app_icon::AppIconSettings, app_installation_detection::UserAppInstallDetectionSettings,
    cloud_preferences::PreferencesSettings,
    initializer::SettingsInitializer,
    language::LanguageSettings, native_preference::NativePreferenceSettings,
    network::NetworkSettings, AISettings, AISettingsChangedEvent, AccessibilitySettings,
    AliasExpansionSettings,
    AppEditorSettings, AutoupdateSettings, BlockVisibilitySettings, CodeSettings, DebugSettings,
    EmacsBindingsSettings, FontSettings, FontSettingsChangedEvent, GPUSettings, InputBoxType,
    InputModeSettings, InputSettings, PaneSettings, SameLinePromptBlockSettings, ScrollSettings,
    SelectionSettings, SshSettings, ThemeSettings, TuiAutoupdateSettings, VimBannerSettings,
    WarpDrivePrivacySettings,
};

pub struct UserDefaultsOnStartup {
    pub should_restore_session: bool,
    pub tips_data: TipsCompleted,
    pub user_default_shell_unsupported_banner_state: BannerState,
    pub settings_file_error: Option<super::SettingsFileError>,
}

/// Registers all settings groups with the application context.
///
/// This populates the `SettingsManager` with storage keys, default values,
/// and hierarchy info for every setting. It does not set up appearance,
/// rendering config, or event subscriptions.
pub fn register_all_settings(ctx: &mut AppContext) {
    BlockListSettings::register(ctx);
    BlockVisibilitySettings::register(ctx);
    DebugSettings::register(ctx);
    SessionSettings::register(ctx);
    KeysSettings::register(ctx);
    FontSettings::register(ctx);
    TabSettings::register(ctx);
    WindowSettings::register(ctx);
    SafeModeSettings::register(ctx);
    TerminalSettings::register(ctx);
    PaneSettings::register(ctx);
    CommandSearchSettings::register(ctx);
    AliasExpansionSettings::register(ctx);
    CodeSettings::register(ctx);
    LigatureSettings::register(ctx);
    GPUSettings::register(ctx);
    GeneralSettings::register(ctx);
    AISettings::register_and_subscribe_to_events(ctx);
    // Zap Wave 7-3: `AmbientAgentSettings` was removed entirely along with the ambient-agent UI subsystem.
    ScrollSettings::register(ctx);
    SelectionSettings::register(ctx);
    InputModeSettings::register(ctx);
    ThemeSettings::register(ctx);
    AccessibilitySettings::register(ctx);
    NativePreferenceSettings::register(ctx);
    NetworkSettings::register(ctx);
    AutoupdateSettings::register(ctx);
    TuiAutoupdateSettings::register(ctx);
    PreferencesSettings::register(ctx);
    WarpDrivePrivacySettings::register(ctx);
    UserAppInstallDetectionSettings::register(ctx);
    AppIconSettings::register(ctx);
    LanguageSettings::register(ctx);
    AppEditorSettings::register(ctx);
    InputSettings::register(ctx);
    WarpifySettings::register(ctx);
    AltScreenReporting::register(ctx);
    UndoCloseSettings::register(ctx);
    SshSettings::register(ctx);
    VimBannerSettings::register(ctx);
    SharedSessionSettings::register(ctx);
    WarpDriveSettings::register(ctx);
    WorkflowAliases::register(ctx);
    EmacsBindingsSettings::register(ctx);
    SameLinePromptBlockSettings::register(ctx);
    SemanticSelection::register(ctx);

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    super::LinuxAppConfiguration::register(ctx);

    #[cfg(feature = "local_fs")]
    crate::util::file::external_editor::EditorSettings::register(ctx);
}

/// Key written to the platform-native store after the first successful
/// migration of public settings into `settings.toml`. Its presence prevents
/// re-migration when the user intentionally deletes the TOML file to reset.
const SETTINGS_FILE_MIGRATION_COMPLETE_KEY: &str = "SettingsFileMigrationComplete";

pub fn init(
    startup_toml_parse_error: Option<user_preferences::Error>,
    ctx: &mut AppContext,
) -> UserDefaultsOnStartup {
    ctx.add_singleton_model(|_| SettingsInitializer::new());

    register_all_settings(ctx);

    // One-time migration: copy public settings from the platform-native store
    // into the TOML file so existing users don't lose their customizations
    // when the settings file feature is first enabled.
    if needs_settings_file_migration(ctx) {
        migrate_native_settings_to_settings_file(ctx);
    }

    // Apply the persisted language setting to the i18n loader. run() already initialized it with
    // the system locale early on; this overrides it with the user's explicit choice.
    // Left alone when Language::System.
    {
        let lang = *super::language::LanguageSettings::as_ref(ctx).language;
        if let Some(locale) = lang.to_locale_str() {
            crate::i18n::set_locale(locale);
        }
    }

    let use_thin_strokes = *FontSettings::as_ref(ctx).use_thin_strokes;

    let general_settings = GeneralSettings::as_ref(ctx);
    let tips_features_used = general_settings.welcome_tips_features_used.clone();
    let tips_skipped_or_completed = *general_settings.welcome_tips_skipped_or_completed;
    let user_default_shell_unsupported_banner_state =
        *general_settings.user_default_shell_unsupported_banner_state;
    let should_restore_session = *general_settings.restore_session;

    // Validate all public settings to detect values that parsed as TOML
    // but cannot be deserialized into the expected Rust types.
    let invalid_setting_keys =
        settings::SettingsManager::as_ref(ctx).validate_all_public_settings(ctx);
    let settings_file_error = if let Some(err) = startup_toml_parse_error {
        Some(super::SettingsFileError::FileParseFailed(err.to_string()))
    } else if !invalid_setting_keys.is_empty() {
        Some(super::SettingsFileError::InvalidSettings(
            invalid_setting_keys,
        ))
    } else {
        None
    };

    let user_defaults_on_startup = UserDefaultsOnStartup {
        should_restore_session,
        tips_data: TipsCompleted::new(tips_features_used, tips_skipped_or_completed),
        user_default_shell_unsupported_banner_state,
        settings_file_error,
    };

    let gpu_settings = GPUSettings::as_ref(ctx);
    let prefer_low_power_gpu = *gpu_settings.prefer_low_power_gpu.value();
    let backend_preference = *gpu_settings.preferred_backend.value();

    // Update the rendering config.
    ctx.update_rendering_config(|config| {
        config.glyphs.use_thin_strokes = use_thin_strokes;
        config.gpu_power_preference = if prefer_low_power_gpu {
            GPUPowerPreference::LowPower
        } else {
            GPUPowerPreference::default()
        };
        config.backend_preference = backend_preference;
    });

    ctx.subscribe_to_model(&FontSettings::handle(ctx), |font_settings, event, ctx| {
        if matches!(event, FontSettingsChangedEvent::UseThinStrokes { .. }) {
            let use_thin_strokes = *font_settings.as_ref(ctx).use_thin_strokes;
            ctx.update_rendering_config(|config| {
                config.glyphs.use_thin_strokes = use_thin_strokes;
            });
        }
    });

    // Keep input_box_type in sync whenever honor_ps1 changes —
    // Classic when PS1 is honored, Universal otherwise.
    ctx.subscribe_to_model(
        &SessionSettings::handle(ctx),
        |session_settings, event, ctx| {
            if let SessionSettingsChangedEvent::HonorPS1 { .. } = event {
                let new_honor_ps1 = *session_settings.as_ref(ctx).honor_ps1;
                let new_type = if new_honor_ps1 {
                    InputBoxType::Classic
                } else {
                    InputBoxType::Universal
                };
                InputSettings::handle(ctx).update(ctx, |input_settings, ctx| {
                    report_if_error!(input_settings.input_box_type.set_value(new_type, ctx));
                });
            }
        },
    );

    appearance::register(ctx);

    // Global HTTP proxy (see Issue #72): this only reads the non-sensitive fields from
    // NetworkSettings; the password, read from the OS keychain via ProxyCredentials, is pushed
    // later once `initialize_app` has registered it.
    apply_network_settings_to_global_slots(ctx, "");
    ctx.subscribe_to_model(&NetworkSettings::handle(ctx), |_model, _event, ctx| {
        // By the time of a change, the password may already have been provided by
        // ProxyCredentials. lib.rs separately subscribes to that singleton and pushes an apply
        // that includes the password. Here we only push the non-password fields, keeping the
        // password unchanged.
        crate::settings::reapply_network_settings_preserving_password(ctx);
    });

    // Zap: hot-reload directory for the system prompt template. `prompt_renderer` is a set of
    // free functions with no access to AppContext, so like the proxy settings above, it follows
    // the "push into a global slot" pattern.
    // This must be pushed once here (not only via the settings-page subscription): the settings
    // page may never be opened during the whole session, in which case a persisted directory
    // would never take effect.
    apply_prompt_template_dir_to_global_slot(ctx);
    ctx.subscribe_to_model(&AISettings::handle(ctx), |_model, event, ctx| {
        if matches!(event, AISettingsChangedEvent::PromptTemplateDir { .. }) {
            apply_prompt_template_dir_to_global_slot(ctx);
        }
    });

    // Set up hot-reload for the settings file. When the WarpConfig watcher
    // detects a change to settings.toml, reload preferences from disk and
    // push changed values into setting models.
    #[cfg(feature = "local_fs")]
    {
        let prefs = <settings::PublicPreferences as warpui::SingletonEntity>::as_ref(ctx);
        if prefs.is_settings_file() {
            ctx.subscribe_to_model(
                &crate::user_config::WarpConfig::handle(ctx),
                handle_warp_config_change,
            );
        }
    }

    user_defaults_on_startup
}

/// Reads the current `NetworkSettings` plus the externally supplied `password`, updating both
/// `http_client::set_global_proxy_config` and `websocket::set_global_proxy_config` so they stay
/// consistent with the same proxy semantics (see Issue #72).
///
/// The password is passed in as `&str` instead of being read from the `ProxyCredentials`
/// singleton in order to avoid a dependency on that singleton, which is not yet registered
/// during the settings::init phase. It's the caller's responsibility to pass an empty string
/// early during startup (subsequent UI / ProxyCredentials events will re-push it) and the real
/// password later on. Rebuilding any existing `Client` instances is also the caller's
/// responsibility.
pub(crate) fn apply_network_settings_to_global_slots(ctx: &mut AppContext, password: &str) {
    use super::network::NetworkSettings;
    let net = NetworkSettings::as_ref(ctx);
    let mode = *net.proxy_mode.value();
    let url = net.proxy_url.value().clone();
    let username = net.proxy_username.value().clone();
    let no_proxy = net.proxy_no_proxy.value().clone();

    http_client::set_global_proxy_config(http_client::ProxyConfig {
        mode: mode.to_http_client_mode(),
        url: url.clone(),
        username: username.clone(),
        password: password.to_string(),
        no_proxy: no_proxy.clone(),
    });
    websocket::set_global_proxy_config(websocket::ProxyConfig {
        mode: mode.to_websocket_mode(),
        url,
        username,
        password: password.to_string(),
        no_proxy,
    });
}

/// Zap: pushes Settings -> AI -> System prompt template directory into `prompt_renderer`'s
/// global slot.
///
/// `prompt_renderer` is a set of free functions (with no access to `AppContext`), so like the
/// global proxy settings above, it follows the "read settings -> push into a global slot"
/// pattern.
///
/// Empty string = hot-reload disabled, falling back to the built-in template compiled into the
/// binary via `include_str!`.
/// Note that the `ZAP_PROMPT_DIR` environment variable takes higher priority on the
/// `prompt_renderer` side, and will override whatever value is pushed here if set.
pub(crate) fn apply_prompt_template_dir_to_global_slot(ctx: &AppContext) {
    let dir = AISettings::as_ref(ctx).prompt_template_dir.value().clone();
    crate::ai::agent_providers::prompt_renderer::set_override_dir(
        (!dir.is_empty()).then(|| std::path::PathBuf::from(dir)),
    );
}

/// Called after `initialize_app` (once `ProxyCredentials` is registered): reads the current
/// password and re-pushes the global proxy settings. Also used by the NetworkSettings change
/// subscription to keep the password from being lost.
pub(crate) fn reapply_network_settings_preserving_password(ctx: &mut AppContext) {
    use super::network_secrets::ProxyCredentials;
    let password = ProxyCredentials::as_ref(ctx).password().to_string();
    apply_network_settings_to_global_slots(ctx, &password);
}

/// Handles a `WarpConfig` change event, reloading settings from disk when
/// the settings file is modified, created, or deleted.
#[cfg(feature = "local_fs")]
fn handle_warp_config_change(
    _: warpui::ModelHandle<crate::user_config::WarpConfig>,
    event: &crate::user_config::WarpConfigUpdateEvent,
    ctx: &mut AppContext,
) {
    use crate::user_config::{WarpConfig, WarpConfigUpdateEvent};

    if !matches!(event, WarpConfigUpdateEvent::Settings) {
        return;
    }
    let prefs = <settings::PublicPreferences as warpui::SingletonEntity>::as_ref(ctx);
    if let Err(err) = prefs.reload_from_disk() {
        log::warn!("Settings file reload failed: {err}");
        WarpConfig::handle(ctx).update(ctx, |_, ctx| {
            ctx.emit(WarpConfigUpdateEvent::SettingsErrors(
                super::SettingsFileError::FileParseFailed(err.to_string()),
            ));
        });
        return;
    }
    let failed_keys = settings::SettingsManager::handle(ctx)
        .update(ctx, |manager, ctx| manager.reload_all_public_settings(ctx));
    WarpConfig::handle(ctx).update(ctx, |_, ctx| {
        if failed_keys.is_empty() {
            ctx.emit(WarpConfigUpdateEvent::SettingsErrorsCleared);
        } else {
            ctx.emit(WarpConfigUpdateEvent::SettingsErrors(
                super::SettingsFileError::InvalidSettings(failed_keys),
            ));
        }
    });
}
/// Returns the platform-native preferences backend.
///
/// Used directly for private settings, and also as the fallback for public
/// settings when the settings file feature flag is disabled.
fn init_platform_native_preferences() -> user_preferences::Model {
    cfg_if::cfg_if! {
        if #[cfg(test)] {
            Box::<user_preferences::in_memory::InMemoryPreferences>::default()
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd", feature = "integration_tests"))] {
            match user_preferences::file_backed::FileBackedUserPreferences::new(super::user_preferences_file_path()) {
                Ok(prefs) => Box::new(prefs) as user_preferences::Model,
                Err(err) => {
                    crate::report_error!(anyhow::anyhow!(err));
                    Box::<user_preferences::in_memory::InMemoryPreferences>::default()
                }
            }
        } else if #[cfg(target_os = "windows")] {
            let app_id = warp_core::channel::ChannelState::app_id();
            Box::new(user_preferences::registry_backed::RegistryBackedPreferences::new(app_id.application_name()))
        } else if #[cfg(target_os = "macos")] {
            Box::new(user_preferences::user_defaults::UserDefaultsPreferencesStorage::new(
                warp_core::channel::ChannelState::data_domain_if_not_default()
            ))
        } else if #[cfg(target_family = "wasm")] {
            Box::<user_preferences::local_storage::LocalStoragePreferences>::default()
        } else {
            unreachable!("Unspecified user preferences implementation for current platform!");
        }
    }
}

/// Creates the platform-native preferences backend for private settings.
///
/// Private settings are always stored in the platform-native store (e.g.
/// UserDefaults on macOS) and never appear in the user-visible TOML file.
pub fn init_private_user_preferences() -> settings::PrivatePreferences {
    settings::PrivatePreferences::new(init_platform_native_preferences())
}

/// Initializes the public UserPreferences provider.
///
/// When the `SettingsFile` feature flag is enabled, public settings are stored
/// in `settings.toml` so they are user-visible and editable. When the flag is
/// disabled, this falls back to the platform-native store (same as private
/// settings), so all settings live in the same place.
/// Returns `(preferences_backend, optional_parse_error)`. The parse error
/// is `Some` only when the TOML settings file existed but could not be
/// parsed; it should be propagated to the UI so the user sees a banner.
pub fn init_public_user_preferences() -> (user_preferences::Model, Option<user_preferences::Error>)
{
    cfg_if::cfg_if! {
        if #[cfg(test)] {
            (Box::<user_preferences::in_memory::InMemoryPreferences>::default(), None)
        } else if #[cfg(target_family = "wasm")] {
            (Box::<user_preferences::local_storage::LocalStoragePreferences>::default(), None)
        } else {
            if warp_core::features::FeatureFlag::SettingsFile.is_enabled() {
                let (prefs, parse_error) =
                    user_preferences::toml_backed::TomlBackedUserPreferences::new(
                        super::user_preferences_toml_file_path(),
                    );
                if let Some(err) = &parse_error {
                    log::warn!("Settings file has syntax errors and could not be parsed: {err}");
                }
                (Box::new(prefs) as user_preferences::Model, parse_error)
            } else {
                (init_platform_native_preferences(), None)
            }
        }
    }
}

/// Returns `true` when we should migrate public settings from the
/// platform-native store into the TOML settings file.
///
/// Migration is needed when all of the following are true:
/// 1. The `SettingsFile` feature flag is enabled.
/// 2. The `settings.toml` file does not yet exist on disk.
/// 3. The migration-complete marker is absent from the native store
///    (handles the case where a user deletes `settings.toml` to reset).
fn needs_settings_file_migration(ctx: &AppContext) -> bool {
    if !FeatureFlag::SettingsFile.is_enabled() {
        return false;
    }

    if super::user_preferences_toml_file_path().exists() {
        return false;
    }

    use warp_core::user_preferences::GetUserPreferences as _;
    ctx.private_user_preferences()
        .read_value(SETTINGS_FILE_MIGRATION_COMPLETE_KEY)
        .unwrap_or_default()
        .as_deref()
        != Some("true")
}

/// Performs a one-time migration of public settings from the platform-native
/// store (e.g. NSUserDefaults on macOS) into the TOML settings file.
///
/// For each public storage key registered with the `SettingsManager`, this
/// reads the value from the native store and, if present, feeds it through
/// `update_setting_with_storage_key` — which deserializes, validates, updates
/// the in-memory setting, and writes to the TOML file with the correct
/// hierarchy, `serialize_for_file` transforms, and `max_table_depth`.
fn migrate_native_settings_to_settings_file(ctx: &mut AppContext) {
    use warp_core::user_preferences::GetUserPreferences as _;

    log::info!("Migrating public settings from native store to settings.toml");

    // Collect the storage keys for all public settings.
    let storage_keys: Vec<String> = SettingsManager::as_ref(ctx)
        .public_storage_keys()
        .map(str::to_owned)
        .collect();

    // Read each public setting's value from the native store.
    let native_prefs = ctx.private_user_preferences();
    let values_to_migrate: Vec<(String, String)> = storage_keys
        .into_iter()
        .filter_map(|key| {
            let value = native_prefs.read_value(&key).unwrap_or_default()?;
            Some((key, value))
        })
        .collect();

    let mut migrated_count = 0;
    let mut failed_count = 0;
    let mut last_error: Option<anyhow::Error> = None;

    // Write each value through the SettingsManager so the in-memory state
    // and the TOML file are both updated correctly.
    SettingsManager::handle(ctx).update(ctx, |manager, ctx| {
        for (key, value) in values_to_migrate {
            match manager.update_setting_with_storage_key(&key, value, false, ctx) {
                Ok(()) => migrated_count += 1,
                Err(err) => {
                    log::warn!("Failed to migrate setting {key}: {err}");
                    failed_count += 1;
                    last_error = Some(err);
                }
            }
        }
    });

    if let Some(err) = last_error {
        report_if_error!(Err::<(), _>(err.context(format!(
            "Settings file migration: {failed_count} of {} settings failed to migrate",
            migrated_count + failed_count
        ))));
    }

    log::info!("Settings file migration complete — migrated {migrated_count} settings, {failed_count} failed");

    // Record the migration so it won't re-run if the user deletes the TOML
    // file. This marker is written unconditionally — for new users the native
    // store is empty so the migration is a no-op, but the marker still gets
    // written to indicate that migration was attempted.
    report_if_error!(ctx
        .private_user_preferences()
        .write_value(SETTINGS_FILE_MIGRATION_COMPLETE_KEY, "true".to_owned())
        .map_err(|err| anyhow::anyhow!(err)));
}

#[cfg(any(test, feature = "test-util"))]
pub fn init_and_register_user_preferences(ctx: &mut AppContext) {
    let (public_prefs, _parse_error) = init_public_user_preferences();
    ctx.add_singleton_model(move |_| settings::PublicPreferences::new(public_prefs));
    ctx.add_singleton_model(move |_| init_private_user_preferences());
}

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
