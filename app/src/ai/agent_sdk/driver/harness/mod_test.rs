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

use warp_cli::agent::Harness;

use super::{harness_model_env_vars, validate_cli_installed};
use crate::ai::agent_sdk::driver::AgentDriverError;
use crate::ai::ambient_agents::task::HarnessModelConfig;

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

/// #323: not ported from the pin (no isolated unit test exists there for this function --
/// only exercised indirectly via `local_harness_launch_tests.rs`'s
/// `prepare_local_claude_child_merges_anthropic_model_env_var`, which this fork doesn't port
/// since it drives the full launch pipeline and needs a real `claude` CLI on PATH).
#[test]
fn harness_model_env_vars_sets_anthropic_model_for_claude() {
    let config = HarnessModelConfig {
        model_id: "claude-opus-4".to_string(),
        reasoning_level: None,
    };
    let env_vars = harness_model_env_vars(Harness::Claude, Some(&config));
    assert_eq!(
        env_vars.get(std::ffi::OsStr::new("ANTHROPIC_MODEL")),
        Some(&std::ffi::OsString::from("claude-opus-4"))
    );
}

#[test]
fn harness_model_env_vars_empty_for_non_claude_harnesses() {
    let config = HarnessModelConfig {
        model_id: "gpt-5-codex".to_string(),
        reasoning_level: None,
    };
    assert!(harness_model_env_vars(Harness::Codex, Some(&config)).is_empty());
    assert!(harness_model_env_vars(Harness::OpenCode, Some(&config)).is_empty());
    assert!(harness_model_env_vars(Harness::Gemini, Some(&config)).is_empty());
}

#[test]
fn harness_model_env_vars_empty_when_no_config_or_empty_model_id() {
    assert!(harness_model_env_vars(Harness::Claude, None).is_empty());
    let empty_model = HarnessModelConfig {
        model_id: String::new(),
        reasoning_level: None,
    };
    assert!(harness_model_env_vars(Harness::Claude, Some(&empty_model)).is_empty());
}
