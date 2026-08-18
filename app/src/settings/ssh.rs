use settings::{
    macros::define_settings_group, RespectUserSyncSetting, SupportedPlatforms, SyncToCloud,
};

define_settings_group!(SshSettings,
    settings: [
        enable_legacy_ssh_wrapper: EnableSshWrapper {
            type: bool,
            default: true,
            supported_platforms: SupportedPlatforms::ALL,
            sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
            private: false,
            storage_key: "EnableSSHWrapper",
            toml_path: "warpify.ssh.enable_legacy_ssh_wrapper",
            description: "Whether the legacy SSH wrapper is enabled for SSH sessions.",
        },
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
    ]
);
