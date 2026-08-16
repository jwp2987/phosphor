use settings::{macros::define_settings_group, SupportedPlatforms, SyncToCloud};

// Zap-adapted from warp/master `app/src/settings/tui_autoupdate.rs`. Warp's
// newer `surface: SettingSurfaces::TUI` marker is dropped — Zap's settings macro
// predates the surface concept — so this key lives in the shared settings file
// rather than a TUI-only surface. Behaviour is otherwise identical: read once at
// TUI startup, and `WARP_TUI_DISABLE_AUTOUPDATE` also disables updates per launch.
define_settings_group!(TuiAutoupdateSettings, settings: [
    // Whether the `warp-tui` background auto-updater is enabled. TUI-only; the
    // GUI has its own autoupdater and update preferences (`AutoupdateSettings`).
    autoupdate_enabled: TuiAutoupdateEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        storage_key: "TuiAutoupdateEnabled",
        toml_path: "general.autoupdate_enabled",
        description: "Whether Phosphor automatically installs TUI updates in the background.",
    },
]);
