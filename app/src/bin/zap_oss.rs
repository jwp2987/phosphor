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
#[no_mangle]
#[used]
pub static NvOptimusEnablement: u32 = 1;

#[cfg(all(target_os = "windows", feature = "windows_high_performance_gpu_default"))]
#[allow(non_upper_case_globals)]
#[no_mangle]
#[used]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

// Entry point for the Zap OSS build; a thin wrapper around warp::run().
fn main() -> Result<()> {
    let mut state = ChannelState::new(
        Channel::Oss,
        ChannelConfig {
            app_id: AppId::new("dev", "zap", "Zap"),
            logfile_name: "zap.log".into(),
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
#[cfg(all(not(feature = "extern_plist"), target_os = "macos"))]
embed_plist::embed_info_plist_bytes!(r#"
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleDisplayName</key>
    <string>Zap</string>
    <key>CFBundleExecutable</key>
    <string>zap-oss</string>
    <key>CFBundleIdentifier</key>
    <string>dev.zap.Zap</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleLocalizations</key>
    <array>
    <string>en</string>
    <string>ja</string>
    <string>zh-CN</string>
    </array>
    <key>CFBundleName</key>
    <string>Zap</string>
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
    <array><dict><key>CFBundleURLName</key><string>Custom App</string><key>CFBundleURLSchemes</key><array><string>zap</string></array></dict></array>
    <key>NSHumanReadableCopyright</key>
    <string>© 2026, Zap</string>
    </dict>
    </plist>
"#.as_bytes());
