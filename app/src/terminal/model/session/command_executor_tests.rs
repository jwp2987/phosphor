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

/// #1933: the ControlMaster executor (and with it the helper and the shell-mode channel) is only
/// for an ssh session that has a ControlMaster socket. A one-connection session (Git Bash / MSYS2)
/// has none: a side command over an empty ControlPath would be a new ssh login.
#[test]
fn a_one_connection_ssh_session_never_uses_the_control_master_executor() {
    use crate::terminal::model::session::IsLegacySSHSession;
    use std::path::PathBuf;

    let multiplexed = IsLegacySSHSession::Yes {
        socket_path: PathBuf::from("/home/user/.ssh/77"),
        external_control_master: false,
        raw_bootstrap: true,
        single_connection: false,
    };
    let single = IsLegacySSHSession::Yes {
        socket_path: PathBuf::new(),
        external_control_master: false,
        raw_bootstrap: true,
        single_connection: true,
    };
    assert!(multiplexed.has_control_socket());
    assert!(uses_control_master_executor(&multiplexed));
    assert!(!single.has_control_socket());
    assert!(!uses_control_master_executor(&single));
    assert!(!IsLegacySSHSession::No.has_control_socket());
    assert!(!uses_control_master_executor(&IsLegacySSHSession::No));
    // No ControlMaster means no "fell back from the helper" report either: the helper was never
    // possible, so nothing degraded.
    assert_eq!(
        legacy_ssh_fallback_reason(single.has_control_socket(), true),
        None
    );
}
