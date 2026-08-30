use settings::{macros::define_settings_group, SupportedPlatforms, SyncToCloud};

// This setting currently gates NOTHING (#630).
//
// Its only reader is `AutoupdateState::get_next_request` (app/src/autoupdate/mod.rs),
// and the polling loop that produces those requests is only started when
// `FeatureFlag::Autoupdate.is_enabled()` (mod.rs `register`). That flag is
// deliberately absent from `RELEASE_FLAGS` and is set solely by
// `#[cfg(feature = "autoupdate")]` in `enabled_features()` (app/src/lib.rs) -- and no
// bundler passes the `autoupdate` cargo feature, on any platform. So in a shipped
// build there is no background check for this to enable or disable. The About-page
// toggle that writes it is likewise hidden (`SHOW_AUTOUPDATE_UI`).
//
// The `true` default is kept rather than flipped to `false` precisely because it is
// inert: flipping it would suggest a working background updater that a user had
// switched off, and would silently change behaviour for anyone who builds with
// `--features autoupdate`, where `true` is the right default. Read it as "if this
// fork ever ships autoupdate, it is on by default", not as a description of what
// ships today.
define_settings_group!(AutoupdateSettings, settings: [
    automatic_updates_enabled: AutomaticUpdatesEnabled {
        type: bool,
        // Inert in every shipped build -- see the note above the group.
        default: true,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        storage_key: "AutomaticUpdatesEnabled",
        toml_path: "updates.automatic_updates_enabled",
        description: "Whether Phosphor automatically checks for and downloads updates in the background.",
    },
]);
