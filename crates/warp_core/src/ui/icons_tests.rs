use std::path::{Path, PathBuf};

use super::Icon;

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// The root of the Cargo workspace. `warp_core` lives at `crates/warp_core`.
fn workspace_root() -> &'static Path {
    Path::new(MANIFEST_DIR)
        .parent()
        .and_then(Path::parent)
        .expect("Tests run from the warp_core crate root")
}

/// Icon asset paths are relative to `app/assets`.
fn asset_path(icon: Icon) -> PathBuf {
    workspace_root()
        .join("app/assets")
        .join(<&'static str>::from(icon))
}

#[test]
fn pin_icons_map_to_distinct_assets() {
    assert_eq!(
        <&'static str>::from(Icon::PinFilledDiagonal),
        "bundled/svg/pin-filled-diagonal.svg"
    );
    // The diagonal pin is a separate glyph from the upright one; sharing the
    // asset would silently ship the wrong icon in the tab bar.
    assert_ne!(
        <&'static str>::from(Icon::PinFilledDiagonal),
        <&'static str>::from(Icon::PinFilled)
    );
}

// An `Icon` arm pointing at a file that was never bundled renders as nothing at
// runtime, which no compile-time check catches.
#[test]
fn pin_icon_assets_exist_on_disk() {
    for icon in [Icon::Pin, Icon::PinFilled, Icon::PinFilledDiagonal] {
        let path = asset_path(icon);
        assert!(
            path.is_file(),
            "missing bundled asset for {icon:?}: {}",
            path.display()
        );
    }
}
