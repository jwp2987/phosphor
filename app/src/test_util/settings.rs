#[cfg(test)]
use warpui::App;

#[cfg(test)]
pub fn initialize_settings_for_tests(app: &mut App) {
    use warp_core::execution_mode::ExecutionMode;
    initialize_settings_for_tests_with_mode(app, ExecutionMode::App, false);
}

/// Registers what `BlocklistAIHistoryModel::start_new_child_conversation`'s persist path
/// legitimately needs: `GeneralSettings` (it reads `persist_conversations`) and the
/// sqlite-backed `GlobalResourceHandlesProvider` the write goes through.
///
/// **Idempotent on purpose.** Tests reach this path from harnesses that already register
/// some or all of these -- `initialize_app_for_terminal_view`, `mock_pane_group`,
/// `initialize_app` -- and `add_singleton_model` debug-asserts on a second registration
/// for the same type ("was called twice for ..."). Registering blindly turned 51 green
/// tests into 12 differently-red ones, so each half is guarded by `has_singleton_model`
/// and this is safe to call before or after any other initializer.
///
/// Deliberately does NOT register `QueuedQueryModel` or `FileModel`, even though the
/// restore counterpart of this persist reaches both. Constructing them reads
/// `BlocklistAIHistoryModel`, which most callers register *after* this helper -- adding
/// them here took the suite from 8 failures to 44. They belong at the individual call
/// sites that need them, after the history model exists.
#[cfg(test)]
pub fn initialize_history_persistence_for_tests(app: &mut App) {
    use crate::terminal::general_settings::GeneralSettings;
    use crate::{GlobalResourceHandles, GlobalResourceHandlesProvider};

    if !app.read(|ctx| ctx.has_singleton_model::<GeneralSettings>()) {
        initialize_settings_for_tests(app);
    }

    if !app.read(|ctx| ctx.has_singleton_model::<GlobalResourceHandlesProvider>()) {
        let global_resource_handles = GlobalResourceHandles::mock(app);
        app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));
    }

}

#[cfg(test)]
pub fn initialize_settings_for_tests_with_mode(
    app: &mut App,
    mode: warp_core::execution_mode::ExecutionMode,
    is_sandboxed: bool,
) {
    use crate::{
        drive::settings::WarpDriveSettings,
        search::command_search::settings::CommandSearchSettings,
        settings::{
            app_icon::AppIconSettings, app_installation_detection::UserAppInstallDetectionSettings,
            init_and_register_user_preferences, manager::SettingsManager, AISettings,
            AccessibilitySettings, AliasExpansionSettings, AppEditorSettings,
            BlockVisibilitySettings, CodeSettings, DebugSettings, EmacsBindingsSettings,
            FontSettings, GPUSettings, InputModeSettings, InputSettings, LocalControlSettings,
            NativePreferenceSettings, PaneSettings, PreferencesSettings, ScrollSettings,
            SelectionSettings, SshSettings, ThemeSettings, TuiAutoupdateSettings,
            TuiThemeSettings, TuiZeroStateSettings, VimBannerSettings, WarpDrivePrivacySettings,
        },
        terminal::{
            alt_screen_reporting::AltScreenReporting, general_settings::GeneralSettings,
            keys_settings::KeysSettings, ligature_settings::LigatureSettings,
            safe_mode_settings::SafeModeSettings, session_settings::SessionSettings,
            settings::TerminalSettings, shared_session::settings::SharedSessionSettings,
            warpify::settings::WarpifySettings, BlockListSettings,
        },
        undo_close::UndoCloseSettings,
        user_config::WarpConfig,
        window_settings::WindowSettings,
        workflows::aliases::WorkflowAliases,
        workspace::tab_settings::TabSettings,
    };
    use warp_core::{execution_mode::AppExecutionMode, semantic_selection::SemanticSelection};

    // Load the i18n bundle so tests that assert on displayed text see resolved
    // English strings, not raw fluent keys. `init` is a global OnceLock, so pin
    // English here for determinism; `fallback_chain_works` uses a local loader to
    // avoid poisoning this shared global.
    crate::i18n::init(Some("en"));

    app.add_singleton_model(|ctx| AppExecutionMode::new(mode, is_sandboxed, ctx));

    app.update(init_and_register_user_preferences);
    app.add_singleton_model(|_ctx| SettingsManager::default());
    app.add_singleton_model(WarpConfig::mock);

    AccessibilitySettings::register(app);
    app.update(AISettings::register_and_subscribe_to_events);
    AliasExpansionSettings::register(app);
    // Zap Wave 7-3: `AmbientAgentSettings` was physically removed along with the
    // ambient-agent UI subsystem.
    AppEditorSettings::register(app);
    BlockVisibilitySettings::register(app);
    BlockListSettings::register(app);
    crate::settings::language::LanguageSettings::register(app);
    PreferencesSettings::register(app);
    CommandSearchSettings::register(app);
    DebugSettings::register(app);
    AppIconSettings::register(app);
    EmacsBindingsSettings::register(app);

    #[cfg(feature = "local_fs")]
    {
        crate::util::file::external_editor::EditorSettings::register(app);
    }

    FontSettings::register(app);
    GeneralSettings::register(app);
    GPUSettings::register(app);
    InputModeSettings::register(app);
    InputSettings::register(app);
    KeysSettings::register(app);
    LigatureSettings::register(app);

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        use crate::settings::LinuxAppConfiguration;
        LinuxAppConfiguration::register(app);
    }

    NativePreferenceSettings::register(app);
    SafeModeSettings::register(app);
    ScrollSettings::register(app);
    SelectionSettings::register(app);
    app.update(|ctx| {
        WarpifySettings::register(ctx);
    });
    SessionSettings::register(app);
    SshSettings::register(app);
    TabSettings::register(app);
    TerminalSettings::register(app);
    PaneSettings::register(app);
    ThemeSettings::register(app);
    UndoCloseSettings::register(app);
    VimBannerSettings::register(app);
    WarpDriveSettings::register(app);
    WindowSettings::register(app);
    SharedSessionSettings::register(app);
    CodeSettings::register(app);
    SemanticSelection::register(app);
    // Settings that `register_all_settings` registers but this test helper had
    // drifted from; needed by the workspace-view tests.
    crate::settings::network::NetworkSettings::register(app);
    crate::settings::AutoupdateSettings::register(app);

    // Settings that `register_all_settings` registers but this test helper had
    // drifted from, found by script/check_settings_registry. Each of these
    // reads only from the mocked in-memory preferences already set up above
    // (`init_and_register_user_preferences`), so registering them here is
    // safe -- unlike `LocalControlSettings` below, which reads from secure
    // storage and so cannot be registered this early.
    AltScreenReporting::register(app);
    TuiAutoupdateSettings::register(app);
    TuiThemeSettings::register(app);
    TuiZeroStateSettings::register(app);
    UserAppInstallDetectionSettings::register(app);
    WarpDrivePrivacySettings::register(app);
    WorkflowAliases::register(app);

    app.update(|ctx| {
        // Register a no-op secure storage provider for testing.
        warpui_extras::secure_storage::register_noop("test", ctx);

        // `LocalControlSettings` reads its value from secure storage (not
        // user preferences) at registration time, so it must come after the
        // no-op secure storage provider directly above, not in the drifted-
        // settings group further up. Registering it any earlier panics the
        // same way an unregistered settings model does, just for the
        // secure-storage singleton instead.
        LocalControlSettings::register(ctx);

        // Add settings models that are backed by secure storage, not user preferences.
        ctx.add_singleton_model(ai::api_keys::ApiKeyManager::new);
    });
}
