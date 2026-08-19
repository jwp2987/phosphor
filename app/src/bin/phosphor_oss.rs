// On Windows, we don't want to display a console window when the application is running in release
// builds. See https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute.
#![cfg_attr(feature = "release_bundle", windows_subsystem = "windows")]

use anyhow::Result;
use warp_core::{
    channel::{Channel, ChannelConfig, ChannelState},
    features::DEBUG_FLAGS,
    AppId,
};

#[cfg(all(target_os = "windows", feature = "windows_high_performance_gpu_default"))]
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
#[used]
pub static NvOptimusEnablement: u32 = 1;

#[cfg(all(target_os = "windows", feature = "windows_high_performance_gpu_default"))]
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
#[used]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

// Entry point for the Zap OSS build; a thin wrapper around warp::run().
fn main() -> Result<()> {
    let mut state = ChannelState::new(
        Channel::Oss,
        ChannelConfig {
            // Keep in sync with `ChannelState::init`'s default app id.
            app_id: AppId::new("dev", "phosphor", "Phosphor"),
            display_name: "Phosphor".into(),
            // Rebranded from "zap.log" 2026-08-19. A log filename is not one of the
            // load-bearing "zap" compatibility surfaces (persistence keys, X-Zap-*
            // headers, the DCS reply, Software\Zap, the paths.rs legacy arm) -- nothing
            // reads it but this app's own rotation, which derives every path from this
            // string. Old zap.log* files are simply no longer collected, matching the
            // no-migration decision recorded in README.md's storage-identity note.
            logfile_name: "phosphor.log".into(),
            autoupdate_config: None,
            mcp_static_config: None,
        },
    );
    if cfg!(debug_assertions) {
        state = state.with_additional_features(DEBUG_FLAGS);
    }
    // Always enable IME marked-text rendering: winit's IME path is supported
    // on both macOS / Windows, but if not explicitly enabled here, Zap
    // discards preedit / input-composition updates entirely, leaving only
    // the OS's candidate window visible — on Windows this amounts to
    // substantive breakage for Japanese / Chinese / Korean input.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        use warp_core::features::FeatureFlag;
        state = state.with_additional_features(&[FeatureFlag::ImeMarkedText]);
    }
    ChannelState::set(state);

    warp::run()
}

// If we're not using an external plist, embed the following as the Info.plist.
//
// `CFBundleIdentifier` here is not decoration: on macOS
// `warp_core::channel::state::app_id_from_bundle()` reads it back at runtime and
// *overrides* the app id configured above. It must therefore stay in sync with
// `AppId::new` above, `identifier` in `app/Cargo.toml`, and `BUNDLE_ID` in
// `script/macos/bundle` — otherwise a bundled build and `cargo run` silently use
// different data directories.
//
// `CFBundleURLSchemes` must match `ChannelState::url_scheme()`, which is now
// `phosphor`. `zap` is kept alongside it so links already in the wild still
// open; the app only ever *emits* the first.
#[cfg(all(not(feature = "extern_plist"), target_os = "macos"))]
embed_plist::embed_info_plist_bytes!(r#"
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleDisplayName</key>
    <string>Phosphor</string>
    <key>CFBundleExecutable</key>
    <string>phosphor-oss</string>
    <key>CFBundleIdentifier</key>
    <string>dev.phosphor.Phosphor</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleLocalizations</key>
    <array>
    <string>en</string>
    <string>ja</string>
    <string>zh-CN</string>
    </array>
    <key>CFBundleName</key>
    <string>Phosphor</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>UIDesignRequiresCompatibility</key>
    <true/>
    <key>CFBundleURLTypes</key>
    <array><dict><key>CFBundleURLName</key><string>Custom App</string><key>CFBundleURLSchemes</key><array><string>phosphor</string><string>zap</string></array></dict></array>
    <key>NSHumanReadableCopyright</key>
    <string>© 2026, Phosphor</string>
    </dict>
    </plist>
"#.as_bytes());
