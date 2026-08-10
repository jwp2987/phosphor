use settings::{
    macros::define_settings_group, RespectUserSyncSetting, SupportedPlatforms, SyncToCloud,
};

define_settings_group!(CodeSettings, settings: [
    code_as_default_editor: CodeAsDefaultEditor {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "code.editor.use_warp_as_default_editor",
        description: "Whether Zap is used as the default code editor.",
    }

    // Whether or not the user has manually dismissed the code toolbelt new feature popup.
    dismissed_code_toolbelt_new_feature_popup: DismissedCodeToolbeltNewFeaturePopup {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
    // Controls whether the project explorer / file tree appears in the tools panel.
    // Controls whether the language server reformats the file on save.
    // Restored with LSP: format-on-save is an LSP request, so it went out with
    // `efcaa42b8` and comes back with it. The pin's `surface:` key is omitted --
    // this fork dropped `SettingSurfaces`.
    format_on_save: FormatOnSave {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "code.editor.format_on_save",
        description: "Whether the language server automatically formats the file on save. Other LSP features (hover, go-to-definition, references, diagnostics) are unaffected.",
    },
    show_project_explorer: ShowProjectExplorer {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "code.editor.show_project_explorer",
        description: "Whether the project explorer is shown in the tools panel.",
    },
    // Controls whether global file search appears in the tools panel.
    show_global_search: ShowGlobalSearch {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "code.editor.show_global_search",
        description: "Whether global file search is shown in the tools panel.",
    },
    // Whether the AI agent may use the codebase embedding index as context.
    //
    // Restored from the pin (`02b53fcd8:app/src/settings/code.rs`) along with the
    // index itself, minus one thing: the pin also consulted an organization-level
    // `AdminEnablementSetting` that could force this on or off for a whole team.
    // That override arrived from Warp's server and has no local equivalent, so
    // `UserWorkspaces::is_codebase_context_enabled` reduces to this setting plus
    // the global AI toggle.
    codebase_context_enabled: CodebaseContextEnabled {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "code.indexing.agent_mode_codebase_context",
        description: "Whether codebase context is provided to the AI agent.",
    },
    // Whether repositories are indexed automatically as they are opened, rather
    // than only on explicit request. Restored from the pin; default off there
    // too, because indexing spends the user's embedding provider quota.
    auto_indexing_enabled: AutoIndexingEnabled {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "code.indexing.agent_mode_codebase_context_auto_indexing",
        description: "Whether automatic codebase indexing is enabled.",
    },
    // Controls whether hidden files (dotfiles) are shown in the project explorer.
    show_hidden_files: ShowHiddenFiles {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "code.editor.show_hidden_files",
        description: "Whether hidden files (dotfiles) are shown in the project explorer.",
    },
]);
