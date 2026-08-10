use settings::{
    macros::define_settings_group, RespectUserSyncSetting, SupportedPlatforms, SyncToCloud,
};

use super::DriveSortOrder;

pub const HAS_AUTO_OPENED_WELCOME_FOLDER: &str = "HasAutoOpenedWelcomeFolder";

define_settings_group!(WarpDriveSettings, settings: [
    sorting_choice: WarpDriveSortingChoice {
        type: DriveSortOrder,
        default: DriveSortOrder::ByObjectType,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "warp_drive.sorting_choice",
        description: "The sort order for items in Phosphor Drive.",
    },
    sharing_onboarding_block_shown: WarpDriveSharingOnboardingBlockShown {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: true,
    },
    // 2026-08-10: `enable_warp_drive` / `warp_drive.enabled` was removed, together with
    // its settings page and `is_warp_drive_enabled`. It was a toggle whose only "off"
    // switch lived on a settings page that was never in `nav_items` -- so a user who
    // turned Drive off could not turn it back on outside `settings.toml`. Every surface
    // it gated is now unconditional. See DECLINED.md.
    //
    // A stale `warp_drive.enabled = false` left in an existing `settings.toml` is inert:
    // `SettingsManager::validate_all_public_settings` only walks *registered* settings,
    // so an unregistered key is never read and never reported as invalid. No migration
    // is needed and startup is unaffected.
]);
