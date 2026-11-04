use std::collections::HashSet;

use crate::{banner::BannerState, resource_center::Tip};
use warp_core::settings::{
    RespectUserSyncSetting, SupportedPlatforms, SyncToCloud, macros::define_settings_group,
};

/// Default for `general.quit_on_last_window_closed`. On (#1154): the app must not be
/// left running with no window; macOS users can opt into Mac-style behaviour.
pub const QUIT_ON_LAST_WINDOW_CLOSED_DEFAULT: bool = true;

define_settings_group!(GeneralSettings, settings: [
    show_warning_before_quitting: ShowWarningBeforeQuitting {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "general.show_warning_before_quitting",
        description: "Whether to show a warning dialog before quitting Phosphor.",
    },
    quit_on_last_window_closed: QuitOnLastWindowClosed {
        type: bool,
        default: QUIT_ON_LAST_WINDOW_CLOSED_DEFAULT,
        supported_platforms: SupportedPlatforms::MAC,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "general.quit_on_last_window_closed",
        description: "Whether to quit Phosphor when the last window is closed (default: on).",
    },
    restore_session: RestoreSession {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "general.restore_session",
        description: "Whether to restore the previous session when Phosphor starts up.",
    },
    persist_conversations: PersistConversations {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "general.persist_conversations",
        description: "Whether to persist agent conversations to the local database so they appear in /conversations history. Independent of session restore.",
    },
    // Opt-in (#1073), unlike Warp: the app must not add itself to the OS
    // startup items without being asked. Installs that were registered under
    // the old default and never set this explicitly are unregistered on the
    // next launch by `login_item::maybe_register_app_as_login_item`.
    add_app_as_login_item: LoginItem {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::OR(
            Box::new(SupportedPlatforms::MAC),
            Box::new(SupportedPlatforms::WINDOWS),
        ),
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "general.login_item",
        description: "Whether to launch the app automatically when you log in. Off by default.",
    },
    // Records whether the app has been added as a login item.
    // If it has, we don't try to add it again unless the user explicitly
    // retoggles the setting. This is to allow a user to remove the login item
    // directly from their OS's startup UI and not have it re-added when they
    // next start the app.
    app_added_as_login_item: AppAddedAsLoginItem {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::OR(
            Box::new(SupportedPlatforms::MAC),
            Box::new(SupportedPlatforms::WINDOWS),
        ),
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    },
    link_tooltip: LinkTooltip {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "general.link_tooltip",
        description: "Whether to show a tooltip when hovering over links.",
    },
    welcome_tips_features_used: WelcomeTipsFeaturesUsed {
        type: HashSet<Tip>,
        default: HashSet::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    },
    welcome_tips_skipped_or_completed: WelcomeTipsCompleted {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
    agent_mode_onboarding_block_shown: AgentModeOnboardingBlockShown {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
    user_default_shell_unsupported_banner_state: UserDefaultShellUnsupportedBannerState {
        type: BannerState,
        default: BannerState::default(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
    open_in_warp_banner_dismissed_for_markdown: OpenInWarpBannerDismissedMarkdown {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    },
    open_in_warp_banner_dismissed_for_code_and_text: OpenInWarpBannerDismissedCode {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    },
    did_non_anonymous_user_log_in: DidNonAnonymousUserLogIn {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
    },
    // One-time flag tracking whether the Zap launch modal has already been
    // shown to the user. Not user-visible; modeled as a setting so it's only
    // shown once per user regardless of the number of devices they use.
    did_check_to_trigger_zap_launch_modal: DidShowZapLaunchModal {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::No),
        private: true,
    },
    auto_open_code_review_pane_on_first_agent_change: AutoOpenCodeReviewPaneOnFirstAgentChange {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "code.editor.auto_open_code_review_pane_on_first_agent_change",
        description: "Whether to automatically open the code review pane when the agent makes its first change.",
    },
    bonus_grants_shown: BonusGrantsShown {
        type: HashSet<String>,
        default: HashSet::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
]);
