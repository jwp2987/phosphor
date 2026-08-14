use super::is_zap_bundle;

#[test]
fn is_zap_bundle_recognises_zap_channels() {
    // OSS (Phosphor) itself.
    assert!(is_zap_bundle("dev.phosphor.Phosphor"));
    // The pre-rename bundle id. Still recognised, because a user may have an
    // older bundle installed alongside; the rename adds an identity rather
    // than replacing one.
    assert!(is_zap_bundle("dev.zap.Zap"));
    // Upstream Warp's various channels — also considered part of this app family, allowing default-app redirection.
    // `dev.warp.Warp` is upstream stable; it is the one id the pin's
    // `is_warp_bundle_recognises_warp_channels` asserts first, and it was
    // dropped when this test was adapted (`dev.warp.Zap` is not a real
    // upstream bundle id, so it covered nothing the others did not).
    assert!(is_zap_bundle("dev.warp.Warp"));
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
