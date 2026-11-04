use settings::{
    RespectUserSyncSetting, SupportedPlatforms, SyncToCloud, macros::define_settings_group,
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
        // `96ad4fe1e`; the pin has no such setting. Opt-in (TF defaults it on): a split that
        // starts an ssh connection by itself is a behaviour change nobody asked for.
        split_inherits_ssh: SplitInheritsSshSetting {
            type: bool,
            default: false,
            supported_platforms: SupportedPlatforms::ALL,
            sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
            private: false,
            storage_key: "SplitInheritsSsh",
            toml_path: "warpify.ssh.split_inherits_ssh",
            description: "Whether splitting a pane whose active session is an SSH session opens the new pane on the same host (it runs the same ssh command, then changes to the same remote directory). Off by default.",
        },
        // #1171: without ServerAlive* a silently dead link (laptop sleep, wifi handover) is
        // never noticed while idle and takes ~15 min to notice with unacked data. The shell
        // wrapper adds `ServerAliveInterval=30 ServerAliveCountMax=6` (180 s tolerance) ONLY
        // when `ssh -G` reports the interval as 0, so a user's own ssh config always wins.
        keepalive: SshKeepalive {
            type: bool,
            default: true,
            supported_platforms: SupportedPlatforms::ALL,
            sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
            private: false,
            storage_key: "SshKeepalive",
            toml_path: "warpify.ssh.keepalive",
            description: "Whether the SSH wrapper adds ServerAliveInterval=30 / ServerAliveCountMax=6 to connections whose ssh configuration does not set ServerAliveInterval, so a silently dead connection is detected within about 3 minutes. Your own ssh configuration always wins. New tabs only.",
        },
        // #1243: OpenSSH >= 9.5 sends interactive keystrokes on a 20 ms tick
        // (`ObscureKeystrokeTiming yes`), measured as +11 ms median / +21 ms max per key
        // over plain ssh. It is a privacy default (it hides typing cadence from a network
        // observer), so it is never changed silently: this is opt-in, and the wrapper passes
        // `-o ObscureKeystrokeTiming=no` ONLY when `ssh -G` reports the default `yes`, so a
        // user's own ssh config always wins. A session with the remote-server proxy channel is
        // already un-obscured (OpenSSH only obscures with exactly one session channel open).
        obscure_keystroke_timing: SshObscureKeystrokeTiming {
            type: bool,
            default: true,
            supported_platforms: SupportedPlatforms::ALL,
            sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
            private: false,
            storage_key: "SshObscureKeystrokeTiming",
            toml_path: "warpify.ssh.obscure_keystroke_timing",
            description: "Whether managed SSH sessions keep OpenSSH's ObscureKeystrokeTiming (the default since OpenSSH 9.5): keystrokes are sent on a 20 ms tick, adding up to 20 ms to each key, so that a network observer cannot read your typing rhythm (for example while you type a password). Turning this off passes -o ObscureKeystrokeTiming=no to the connections the SSH wrapper starts, only when your ssh configuration does not set the option itself. New tabs only.",
        },
    ]
);
