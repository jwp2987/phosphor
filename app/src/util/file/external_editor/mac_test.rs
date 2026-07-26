use super::is_zap_bundle;

#[test]
fn is_zap_bundle_recognises_zap_channels() {
    // OSS (Zap) itself.
    assert!(is_zap_bundle("dev.zap.Zap"));
    // Upstream Warp's various channels — also considered part of this app family, allowing default-app redirection.
    assert!(is_zap_bundle("dev.warp.Zap"));
    assert!(is_zap_bundle("dev.warp.WarpDev"));
    assert!(is_zap_bundle("dev.warp.WarpPreview"));
    assert!(is_zap_bundle("dev.warp.WarpOss"));
}

#[test]
fn is_zap_bundle_rejects_other_apps() {
    assert!(!is_zap_bundle("com.microsoft.VSCode"));
    assert!(!is_zap_bundle("com.apple.TextEdit"));
    assert!(!is_zap_bundle("dev.zed.Zed"));
    assert!(!is_zap_bundle("invalid"));
    assert!(!is_zap_bundle(""));
}
