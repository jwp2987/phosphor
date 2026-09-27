//! Unit tests for `legacy_ssh_fallback_reason`'s pure decision logic: which
//! `LegacySshFallbackReason` (if any) a session's command-executor
//! construction should report, without constructing a `Sessions` model, a
//! `RemoteServerManager`, or real feature-flag state. See TODO.md
//! "Remote-session setup degrades silently", item 1.

use super::*;

#[test]
fn a_non_ssh_session_never_reports_a_fallback() {
    assert_eq!(legacy_ssh_fallback_reason(false, true), None);
    assert_eq!(legacy_ssh_fallback_reason(false, false), None);
}

#[test]
fn a_legacy_ssh_session_with_the_flag_off_reports_feature_disabled() {
    assert_eq!(
        legacy_ssh_fallback_reason(true, false),
        Some(LegacySshFallbackReason::FeatureDisabled)
    );
}

#[test]
fn a_legacy_ssh_session_with_the_flag_on_reports_no_connected_client() {
    // The caller only reaches this decision after the "found a connected
    // client" case has already returned early with a
    // `RemoteServerCommandExecutor`, so `remote_server_flag_enabled: true`
    // here always means "on, but no client was found" -- see the doc on
    // `legacy_ssh_fallback_reason`.
    assert_eq!(
        legacy_ssh_fallback_reason(true, true),
        Some(LegacySshFallbackReason::NoConnectedClient)
    );
}
