//! Pure-function-level unit tests for `manager.rs`.
//!
//! This only covers pure-function helpers — it doesn't touch
//! `RemoteServerManager` itself, since the latter depends on
//! `warpui::Entity` / `ModelContext` and needs a full App context, which is a
//! better fit for the integration testing framework.

use super::*;

// ---------------------------------------------------------------------------
// version_is_compatible
// ---------------------------------------------------------------------------

#[test]
fn version_compat_both_tagged_and_equal() {
    assert!(version_is_compatible(
        Some("v0.2026.05.10.stable"),
        "v0.2026.05.10.stable",
    ));
}

#[test]
fn version_compat_both_tagged_and_different() {
    assert!(!version_is_compatible(
        Some("v0.2026.05.10.stable"),
        "v0.2026.05.10.preview",
    ));
}

#[test]
fn version_compat_both_untagged() {
    // Client has no GIT_RELEASE_TAG (cargo run), and the server also reports
    // an empty string (`script/deploy_remote_server` dev deployment): treated
    // as compatible, keeping the local dev loop unaffected.
    assert!(version_is_compatible(None, ""));
}

#[test]
fn version_compat_client_tagged_server_untagged() {
    // Client is a release build, server is a dev deployment → treated as
    // incompatible, normally triggering the reinstall flow.
    assert!(!version_is_compatible(Some("v0.2026.05.10.stable"), ""));
}

#[test]
fn version_compat_client_untagged_server_tagged() {
    // **Key scenario**: the Zap client has no tag (cargo build), and the
    // server is a release downloaded from the official CDN (with a tag). The
    // original helper judges this incompatible, triggering
    // `remove_remote_server_binary` → an infinite loop. This test only
    // documents that `version_is_compatible`'s own behavior is unchanged; the
    // actual "skip checking" logic is handled by
    // [`should_enforce_remote_version_check`].
    assert!(!version_is_compatible(None, "v0.2026.05.10.stable"));
}

// ---------------------------------------------------------------------------
// should_enforce_remote_version_check
// ---------------------------------------------------------------------------

#[test]
fn enforce_version_check_skipped_on_oss() {
    // When Zap temporarily reuses the official release binary, the client and
    // server versions never match, so strict checking must be skipped.
    assert!(!should_enforce_remote_version_check(Channel::Oss));
}

#[test]
fn enforce_version_check_kept_on_official_channels() {
    // On official channels, the client and server either both come from the
    // same release CI run, or both come from a local
    // `script/deploy_remote_server` deployment, so strict checking is still
    // necessary — preserving the original stale-binary self-healing path.
    for channel in [
        Channel::Stable,
        Channel::Preview,
        Channel::Dev,
        Channel::Local,
        Channel::Integration,
    ] {
        assert!(
            should_enforce_remote_version_check(channel),
            "channel {channel:?} should still enforce version check"
        );
    }
}
