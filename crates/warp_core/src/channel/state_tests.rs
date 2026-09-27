use super::ChannelState;

// Zap Wave 5-5: the `derive_http_origin_from_ws_url` call and its 3 wss/ws path
// tests were physically removed together with `ChannelState::rtc_http_url()`.

/// `ChannelState::init()` (the static default for OSS builds) must satisfy
/// the cloud-disabled predicate; the cloud-removal plan's Phase 5 short-circuit
/// depends on this invariant.
#[test]
fn default_oss_state_is_cloud_disabled() {
    assert!(ChannelState::is_cloud_disabled());
}

/// `display_version` falls back to the caller's `dev_version` when no
/// `GIT_RELEASE_TAG` was baked in -- the untagged-build case for `--version`
/// and the About page (issue #640). This file requires `not(feature =
/// "test-util")`, so `app_version()` here reads `option_env!("GIT_RELEASE_TAG")`
/// directly with no mock able to intercept it, and a plain test build never
/// has that env var set.
#[test]
fn display_version_falls_back_to_dev_version_without_a_release_tag() {
    assert_eq!(ChannelState::display_version("v0.1.7-dev"), "v0.1.7-dev");
}
