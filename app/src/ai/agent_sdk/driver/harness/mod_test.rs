//! Tests ported from the pinned Warp oracle (`02b53fcd8`,
//! `app/src/ai/agent_sdk/driver/harness/mod_tests.rs`).
//!
//! The oracle's four remaining un-ported tests in this file exercise
//! `ThirdPartyHarness::auth_check_command` / `auth_check_command_for`, which
//! does not exist here. At the pin it is a trait method with a trivial
//! `None` default, so porting the default alone would produce a test that
//! passes while asserting nothing about the feature. Tracked in issue #289,
//! and should land with `harness_output_monitor`, not before.
//!
//! **`runtime_error_patterns` is no longer in that boat and was ported
//! 2026-08-10**: `ClaudeHarness`/`CodexHarness` now have real per-harness
//! overrides with literal CLI-output substrings (see `claude_code.rs`,
//! `codex.rs`), so a smoke test on them exercises real, previously untested
//! data rather than a shared default. `GeminiHarness` still uses the trait
//! default (`&[]`) deliberately, mirroring the pin.

use warp_cli::agent::Harness;

use super::claude_code::ClaudeHarness;
use super::codex::CodexHarness;
use super::gemini::GeminiHarness;
use super::{ThirdPartyHarness, harness_model_env_vars, validate_cli_installed};
use crate::ai::agent_sdk::driver::AgentDriverError;
use crate::ai::ambient_agents::task::HarnessModelConfig;

/// Ported from the pin (`02b53fcd8`) in spirit: a smoke test that the trait
/// method is callable and returns real per-harness data now that
/// `ClaudeHarness::runtime_error_patterns` has real entries (`claude_code.rs`),
/// not just the trait's empty default.
#[test]
fn claude_runtime_error_patterns_returns_slice() {
    let patterns: &[&str] = ClaudeHarness.runtime_error_patterns();
    assert!(
        !patterns.is_empty(),
        "ClaudeHarness overrides runtime_error_patterns with real CLI-output substrings"
    );
}

/// Ported from the pin (`02b53fcd8`) in spirit: same smoke test as
/// `claude_runtime_error_patterns_returns_slice`, for `CodexHarness`'s
/// override (`codex.rs`).
#[test]
fn codex_runtime_error_patterns_returns_slice() {
    let patterns: &[&str] = CodexHarness.runtime_error_patterns();
    assert!(
        !patterns.is_empty(),
        "CodexHarness overrides runtime_error_patterns with real CLI-output substrings"
    );
}

/// Ported from the pin (`02b53fcd8`) verbatim: `GeminiHarness` does not
/// override `runtime_error_patterns`, so it falls back to the trait's empty
/// default -- distinct from, not an oversight relative to, Claude/Codex.
#[test]
fn gemini_runtime_error_patterns_is_empty_by_default() {
    assert!(GeminiHarness.runtime_error_patterns().is_empty());
}

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
