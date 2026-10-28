use settings::{
    macros::define_settings_group, RespectUserSyncSetting, SupportedPlatforms, SyncToCloud,
};

// `enable_legacy_ssh_wrapper` used to be declared here *as well as* in
// `WarpifySettings` (`terminal/warpify/settings.rs`), both against
// `toml_path: "warpify.ssh.enable_legacy_ssh_wrapper"` and
// `storage_key: "EnableSSHWrapper"` (#635). Two declarations of one key are not two
// settings: `SettingsManager::register_setting` keys every callback map by storage
// key with `HashMap::insert`, so whichever group registered last -- `SshSettings`,
// per the order in `settings/init.rs` -- silently evicted the other's entry. What
// that cost, concretely: the `update_fns`/`load_fns`/`clear_fns` that a TOML reload
// or a settings-file load dispatches were bound to the losing group's model, so
// that model never saw an update; and `inventory` still held BOTH declarations, so
// the schema and default-settings generators emitted this key twice.
//
// It also swapped the registered `SyncToCloud` (`Never` -> `Globally(Yes)`), which
// in upstream's design would defeat the warpdotdev/Warp#13228 guard on the one-time
// migration. That harm is upstream's, not this fork's: nothing here consumes
// `sync_to_cloud` for syncing -- there is no `CloudPreferencesSyncer` (see
// `DECLINED.md`), `SettingsEvent::LocalPreferencesUpdated` has no subscribers, and
// `Preference::new` is reached only from tests. The registered value is read only by
// the settings UI's local-only icon.
//
// The pin (`4111d08f9`) declares this key only in `WarpifySettings`; so does this
// fork now, and the readers that used to reach it through `SshSettings` reach
// `WarpifySettings::enable_ssh_wrapper` instead.
define_settings_group!(SshSettings,
    settings: [
        // Ported from Warp `0d24d2cf` (#12465). The pin's version also carries
        // `surface: SettingSurfaces::GUI`; this fork dropped `SettingSurfaces`
        // (see `DECLINED.md`), so that key is omitted.
        reuse_existing_control_master: ReuseExistingSshControlMaster {
            type: bool,
            default: false,
            supported_platforms: SupportedPlatforms::ALL,
            sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
            private: false,
            storage_key: "ReuseExistingSshControlMaster",
            toml_path: "warpify.ssh.reuse_existing_control_master",
            description: "Whether the legacy SSH wrapper attaches to an existing SSH ControlMaster for the destination host instead of always creating its own.",
        },
        // #1069: a default split of an SSH pane replays its `ssh` command in the new pane
        // (`pane_group::ssh_split`). Ported by content from TranscriptionFactory/warp
        // `96ad4fe1e`; the pin has no such setting.
        split_inherits_ssh: SplitInheritsSshSetting {
            type: bool,
            default: true,
            supported_platforms: SupportedPlatforms::ALL,
            sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
            private: false,
            storage_key: "SplitInheritsSsh",
            toml_path: "warpify.ssh.split_inherits_ssh",
            description: "Whether splitting a pane whose active session is an SSH session opens the new pane on the same host (it runs the same ssh command, then changes to the same remote directory).",
        },
    ]
);
