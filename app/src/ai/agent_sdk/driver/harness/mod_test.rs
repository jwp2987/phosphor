//! Tests ported from the pinned Warp oracle (`02b53fcd8`,
//! `app/src/ai/agent_sdk/driver/harness/mod_tests.rs`).
//!
//! The oracle's other seven tests in this file exercise
//! `ThirdPartyHarness::auth_check_command` / `auth_check_command_for` and
//! `ThirdPartyHarness::runtime_error_patterns`, neither of which exists here.
//! At the pin both are trait methods with trivial defaults (`None` / `&[]`), so
//! porting the defaults alone would produce tests that pass while asserting
//! nothing about the feature. They are tracked in issue #289 and should land
//! with the per-harness overrides and `harness_output_monitor`, not before.
//! Re-verified against the pin again in round 5 (2026-08-07): still a feature
//! gap, still deliberately not stubbed.

use super::validate_cli_installed;
use crate::ai::agent_sdk::driver::AgentDriverError;

fn assert_harness_setup_failed(err: &AgentDriverError) -> (&str, &str) {
    match err {
        AgentDriverError::HarnessSetupFailed { harness, reason } => (harness, reason),
        other => panic!("expected HarnessSetupFailed, got: {other}"),
    }
}

#[cfg(not(windows))]
#[test]
fn validate_cli_installed_succeeds_for_known_binary() {
    assert!(validate_cli_installed("ls", None).is_ok());
}

#[test]
fn validate_cli_installed_fails_for_missing_binary() {
    let err = validate_cli_installed("__nonexistent_cli_abc123__", None).unwrap_err();
    let (harness, reason) = assert_harness_setup_failed(&err);
    assert_eq!(harness, "__nonexistent_cli_abc123__");
    assert!(reason.contains("not found"));
    assert!(!reason.contains("Install it first"));
}

#[test]
fn validate_cli_installed_includes_docs_url_in_error() {
    let url = "https://example.com/install";
    let err = validate_cli_installed("__nonexistent_cli_abc123__", Some(url)).unwrap_err();
    let (_, reason) = assert_harness_setup_failed(&err);
    assert!(reason.contains(url));
    assert!(reason.contains("Install it first"));
}
