//! OSS-channel `warp-tui` binary and `default-run` target.
//!
//! This is what bare `cargo run -p warp_tui` builds, so it hand-builds a
//! production config and needs no internal `warp-channel-config` generator
//! (mirrors `app/src/bin/phosphor_oss.rs`). It is a console application (no
//! GUI window, no app bundle), so unlike the GUI binaries it sets no
//! `windows_subsystem` attribute and embeds no `Info.plist`.

use anyhow::Result;
use warp_core::AppId;
use warp_core::channel::{Channel, ChannelConfig, ChannelState};

fn main() -> Result<()> {
    let mut state = ChannelState::new(
        Channel::Oss,
        ChannelConfig {
            // Share the GUI's application identity ("Phosphor"), NOT a
            // separate "PhosphorTui", so the TUI reads the same config and
            // secrets: BYOP providers, models, and API keys live in the
            // app-id-based user_preferences.json + keychain. A distinct app_id
            // would point the TUI at an empty config dir, so `/model` would
            // show none of the providers configured in the GUI. The log stays
            // separate via `logfile_name` (a distinct field from the app id).
            //
            // Keep in sync with `app/src/bin/phosphor_oss.rs` and with
            // `ChannelState::init`'s default. This is the THIRD `AppId::new`
            // site; the layer-3 identity rename (`874c2f43d`) moved only the
            // other two, because `specs/phosphor-rebrand/LAYER3-PLAN.md` §1
            // recorded that "`AppId` is set in two places". The miss shipped in
            // `v2026.08.14.1-beta` as issue #585: the GUI on
            // `dev.phosphor.Phosphor` and the TUI still on `dev.zap.Zap`, i.e.
            // separate config dirs and separate keyring service names, so a key
            // saved in one surface was invisible to the other — exactly the
            // failure the paragraph above exists to prevent.
            //
            // As with that rename there is NO data migration: anything the TUI
            // wrote under the old identity stays on disk and is simply no
            // longer read. See README.md's storage-identity note.
            app_id: AppId::new("dev", "phosphor", "Phosphor"),
            display_name: "Phosphor".into(),
            // Rebranded from "zap-tui.log" 2026-08-19; same no-migration rationale as
            // the app id above.
            logfile_name: "phosphor-tui.log".into(),
            autoupdate_config: None,
            mcp_static_config: None,
        },
    );
    if cfg!(debug_assertions) {
        state = state.with_additional_features(warp_core::features::DEBUG_FLAGS);
    }
    ChannelState::set(state);

    warp_tui::run()
}
