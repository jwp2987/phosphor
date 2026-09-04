use crate::terminal::block_list_viewport::InputMode;
use settings::{
    macros::define_settings_group, RespectUserSyncSetting, Setting, SupportedPlatforms, SyncToCloud,
};

define_settings_group!(InputModeSettings, settings: [
    input_mode: InputModeState {
        type: InputMode,
        // The effective default, for every user. There is no override anywhere: the
        // comment here used to say "for new users, we now override this default
        // value in SettingsInitializer to set it to InputMode::Waterfall", and
        // neither half was true (#634). `SettingsInitializer` never touched
        // `input_mode` in this fork, and its new-user block has since been removed
        // outright because this fork has no first-run state. The flag that was
        // supposed to drive the override, `FeatureFlag::DefaultWaterfallMode`, had
        // no reader at all -- `is_enabled()` was never called on it -- so nothing
        // selected Waterfall; the flag has since been deleted (#638).
        default: InputMode::PinnedToBottom,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        storage_key: "InputMode",
        toml_path: "appearance.input.input_mode",
        description: "The position of the terminal input.",
    },
]);

impl InputModeSettings {
    pub fn is_pinned_to_top(&self) -> bool {
        *self.input_mode.value() == InputMode::PinnedToTop
    }
}
