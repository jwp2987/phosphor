//! OSS-channel `warp-tui` binary and `default-run` target.
//!
//! This is what bare `cargo run -p warp_tui` builds, so it hand-builds a
//! production config and needs no internal `warp-channel-config` generator
//! (mirrors `app/src/bin/oss.rs`). It is a console application (no GUI window,
//! no app bundle), so unlike the GUI binaries it sets no `windows_subsystem`
//! attribute and embeds no `Info.plist`.

use anyhow::Result;
use warp_core::AppId;
use warp_core::channel::{Channel, ChannelConfig, ChannelState};

fn main() -> Result<()> {
    let mut state = ChannelState::new(
        Channel::Oss,
        ChannelConfig {
            // Share the GUI's application identity ("Zap"), NOT a separate
            // "ZapTui", so the TUI reads the same config/secrets: BYOP
            // providers, models, and API keys live in the app-id-based
            // user_preferences.json + keychain. A distinct app_id would point
            // the TUI at an empty config dir, so `/model` would show none of
            // the providers configured in the GUI. The log stays separate via
            // `logfile_name` (a distinct field from the app id).
            app_id: AppId::new("dev", "zap", "Zap"),
            display_name: "Phosphor".into(),
            logfile_name: "zap-tui.log".into(),
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
