//! Tests ported from the pinned Warp oracle (`02b53fcd8`,
//! `app/src/ai/agent_sdk/driver_tests.rs`).
//!
//! Adaptations are called out inline. The 18 tests that are *not* here are
//! enumerated in issue #252; re-verified test-by-test against the pin again in
//! round 5 (2026-08-07) by tracing imports, not just names, with a refinement
//! over round 4's framing:
//!
//! - 17 are **cloud**, not merely "source this fork does not ship" in the
//!   abstract: 14 managed-MCP-resolution tests need
//!   `crate::server::server_api::managed_mcp::ManagedMcpClient` (dropped cloud
//!   module), 2 skill-loading tests need `crate::ai::cloud_environments`
//!   (Warp Environments, DECLINED.md), and the artifact-upload test needs
//!   `AIAgentActionResultType::UploadArtifact` (cloud artifact upload).
//! - 1 was a genuine **feature gap**, non-cloud, BYOP-relevant:
//!   `openai_api_key_exports_only_api_key_not_base_url` only needed a 5th
//!   `ManagedSecretValue` variant (`OpenaiApiKey`) alongside the 4 this fork
//!   already had; `build_secret_env_vars` (the function it calls) was already
//!   ported (#247). **Closed in #323**: the Codex harness driver needs the same
//!   variant to read `base_url` off the typed secret, so it was added to
//!   `warp_managed_secrets` and this test is now ported verbatim below.
//!
//! The two `warp_skill_dirs_env_*` tests at the end were added later, from
//! upstream `c7ab9c028` — see that section's comment.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::time::Duration;

use futures::channel::oneshot;
use tempfile::TempDir;
use warp_cli::agent::Harness;
use warp_cli::mcp::MCPSpec;
use warp_cli::{OZ_CLI_ENV, OZ_HARNESS_ENV, OZ_PARENT_RUN_ID_ENV, OZ_RUN_ID_ENV};
use warp_managed_secrets::ManagedSecretValue;
use warpui::{App, SingletonEntity as _};

use super::{
    build_secret_env_vars, AgentDriver, IdleTimeoutSender,
    LEGACY_OZ_PARENT_LISTENER_MANAGED_EXTERNALLY_ENV, LEGACY_OZ_PARENT_STATE_ROOT_ENV,
    OZ_MESSAGE_LISTENER_MANAGED_EXTERNALLY_ENV, OZ_MESSAGE_LISTENER_STATE_ROOT_ENV,
};
use crate::ai::agent::{AIAgentOutput, AIAgentOutputMessage, ArtifactCreatedData, MessageId};
use crate::ai::agent_sdk::driver::harness::task_env_vars;
use crate::ai::ambient_agents::AmbientAgentTaskId;
use crate::ai::mcp::parsing::normalize_mcp_json;
use crate::ai::mcp::JSONTransportType;
use crate::ai::skills::SkillManager;
use crate::test_util::add_window_with_terminal;
use crate::test_util::terminal::initialize_app_for_terminal_view;

#[test]
fn test_normalize_single_cli_server() {
    let input = r#"{"command": "npx", "args": ["-y", "mcp-server"]}"#;
    let result = normalize_mcp_json(input).unwrap();

    // Should wrap with a generated name
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let parsed = parsed.as_object().unwrap();
    assert_eq!(parsed.len(), 1);
    let (_name, server) = parsed.iter().next().unwrap();
    assert_eq!(server["command"].as_str().unwrap(), "npx");
}

#[test]
fn test_normalize_single_sse_server() {
    let input = r#"{"url": "http://localhost:3000/mcp", "headers": {"API_KEY": "value"}}"#;
    let result = normalize_mcp_json(input).unwrap();

    // Should wrap with a generated name
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let parsed = parsed.as_object().unwrap();
    assert_eq!(parsed.len(), 1);
    let (_name, server) = parsed.iter().next().unwrap();
    assert_eq!(server["url"].as_str().unwrap(), "http://localhost:3000/mcp");
}

#[test]
fn test_normalize_already_wrapped_server() {
    let input = r#"{"my-server": {"command": "npx", "args": []}}"#;
    let result = normalize_mcp_json(input).unwrap();

    // Should return as-is (no command/url at top level)
    assert_eq!(result, input);
}

#[test]
fn test_normalize_mcp_servers_wrapper() {
    let input = r#"{"mcpServers": {"server-name": {"command": "npx", "args": []}}}"#;
    let result = normalize_mcp_json(input).unwrap();

    // Should return as-is (no command/url at top level)
    assert_eq!(result, input);
}

#[test]
fn test_normalize_servers_wrapper() {
    let input = r#"{"servers": {"server-name": {"url": "http://example.com"}}}"#;
    let result = normalize_mcp_json(input).unwrap();

    // Should return as-is (no command/url at top level)
    assert_eq!(result, input);
}

#[test]
fn test_normalize_invalid_json() {
    let input = "not valid json";
    let result = normalize_mcp_json(input);

    assert!(result.is_err());
}

#[test]
fn test_normalize_cli_server_with_env() {
    let input = r#"{"command": "npx", "args": ["-y", "mcp-server"], "env": {"API_KEY": "secret"}}"#;
    let result = normalize_mcp_json(input).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let parsed = parsed.as_object().unwrap();
    assert_eq!(parsed.len(), 1);
    let (_name, server) = parsed.iter().next().unwrap();
    assert_eq!(server["env"]["API_KEY"].as_str().unwrap(), "secret");
}

#[test]
fn test_normalize_sse_server_with_headers() {
    let input =
        r#"{"url": "http://localhost:5000/mcp", "headers": {"Authorization": "Bearer token"}}"#;
    let result = normalize_mcp_json(input).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let parsed = parsed.as_object().unwrap();
    assert_eq!(parsed.len(), 1);
    let (_name, server) = parsed.iter().next().unwrap();
    assert_eq!(
        server["headers"]["Authorization"].as_str().unwrap(),
        "Bearer token"
    );
}

// ── IdleTimeoutSender tests ──────────────────────────────────────────────────────

#[test]
fn idle_timeout_sender_send_now_delivers_value() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.end_run_now(42);
    assert_eq!(rx.try_recv().unwrap(), Some(42));
}

#[test]
fn idle_timeout_sender_send_now_only_delivers_once() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.end_run_now(1);
    idle_timeout.end_run_now(2);
    assert_eq!(rx.try_recv().unwrap(), Some(1));
}

#[test]
fn idle_timeout_sender_send_after_delivers_after_timeout() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.end_run_after(Duration::from_millis(50), 99);

    // Not yet delivered.
    assert_eq!(rx.try_recv().unwrap(), None);

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rx.try_recv().unwrap(), Some(99));
}

#[test]
fn idle_timeout_sender_cancel_prevents_delivery() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.end_run_after(Duration::from_millis(50), 99);
    idle_timeout.cancel_idle_timeout();

    std::thread::sleep(Duration::from_millis(100));
    // Sender was not consumed, so the channel is still open but empty.
    assert_eq!(rx.try_recv().unwrap(), None);
}

#[test]
fn idle_timeout_sender_cancel_then_send_now_delivers() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.end_run_after(Duration::from_millis(50), 1);
    idle_timeout.cancel_idle_timeout();
    idle_timeout.end_run_now(2);

    assert_eq!(rx.try_recv().unwrap(), Some(2));
}

#[test]
fn idle_timeout_sender_later_send_after_supersedes_earlier() {
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    // First timer: long timeout.
    idle_timeout.end_run_after(Duration::from_secs(10), 1);
    // Second timer: short timeout. The first is implicitly cancelled.
    idle_timeout.end_run_after(Duration::from_millis(50), 2);

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rx.try_recv().unwrap(), Some(2));
}

#[test]
fn idle_timeout_sender_complete_with_optional_idle_none_sends_immediately() {
    // `complete_with_optional_idle(None, value)` routes to `end_run_now` and
    // delivers `value` synchronously.
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.complete_with_optional_idle(None, 7);
    assert_eq!(rx.try_recv().unwrap(), Some(7));
}

#[test]
fn idle_timeout_sender_complete_with_optional_idle_some_defers_then_delivers() {
    // `complete_with_optional_idle(Some(d), value)` routes to `end_run_after`
    // and defers delivery by `d`.
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.complete_with_optional_idle(Some(Duration::from_millis(50)), 7);

    // Not delivered yet.
    assert_eq!(rx.try_recv().unwrap(), None);

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rx.try_recv().unwrap(), Some(7));
}

#[test]
fn idle_timeout_sender_complete_with_optional_idle_some_then_cancel_invalidates_timer() {
    // Cross-path cancellation: the driver schedules a deferred completion via
    // `complete_with_optional_idle(Some(_), _)` in one code path, and a later
    // event invalidates the timer via `cancel_idle_timeout()` from an unrelated
    // one. The shared `Arc<AtomicUsize>` generation counter is what makes that
    // work across the two logical code paths.
    // This test exercises the same sequence in isolation: schedule via the
    // helper, then cancel via the unrelated `cancel_idle_timeout` entry point,
    // and verify the value is never delivered.
    let (tx, mut rx) = oneshot::channel::<i32>();
    let idle_timeout = IdleTimeoutSender::new(tx);
    idle_timeout.complete_with_optional_idle(Some(Duration::from_millis(50)), 7);
    idle_timeout.cancel_idle_timeout();

    std::thread::sleep(Duration::from_millis(100));
    // Sender was never consumed by the cancelled timer, so the channel is
    // still open but empty.
    assert_eq!(rx.try_recv().unwrap(), None);
}

// ── task_env_vars tests ──────────────────────────────────────────────────────
//
// Adaptation: the oracle additionally asserts on `SERVER_ROOT_URL_OVERRIDE_ENV`,
// `WS_SERVER_URL_OVERRIDE_ENV` and `SESSION_SHARING_SERVER_URL_OVERRIDE_ENV`.
// Those propagate warp-server / session-sharing URLs to child processes and are
// part of the cloud surface this fork does not ship — neither the constants nor
// `ChannelState::allows_server_url_overrides` exist here. Every other assertion
// is verbatim.

#[test]
fn task_env_vars_include_parent_run_id_when_present() {
    let task_id: AmbientAgentTaskId = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
    let env_vars = task_env_vars(Some(&task_id), Some("parent-run-123"), Harness::Claude);

    assert_eq!(
        env_vars.get(&OsString::from(OZ_RUN_ID_ENV)),
        Some(&OsString::from(task_id.to_string()))
    );
    assert_eq!(
        env_vars.get(&OsString::from(OZ_PARENT_RUN_ID_ENV)),
        Some(&OsString::from("parent-run-123"))
    );
    assert_eq!(
        env_vars.get(&OsString::from(OZ_HARNESS_ENV)),
        Some(&OsString::from("claude"))
    );
    assert_eq!(
        env_vars.get(&OsString::from(OZ_MESSAGE_LISTENER_MANAGED_EXTERNALLY_ENV)),
        Some(&OsString::from("1"))
    );
    assert_eq!(
        env_vars.get(&OsString::from(
            LEGACY_OZ_PARENT_LISTENER_MANAGED_EXTERNALLY_ENV
        )),
        Some(&OsString::from("1"))
    );
    assert!(env_vars
        .get(&OsString::from(OZ_CLI_ENV))
        .is_some_and(|value| !value.is_empty()));
}

#[test]
fn task_env_vars_omit_parent_run_id_when_absent() {
    let task_id: AmbientAgentTaskId = "550e8400-e29b-41d4-a716-446655440001".parse().unwrap();
    let env_vars = task_env_vars(Some(&task_id), None, Harness::Oz);

    assert_eq!(
        env_vars.get(&OsString::from(OZ_RUN_ID_ENV)),
        Some(&OsString::from(task_id.to_string()))
    );
    assert!(!env_vars.contains_key(&OsString::from(OZ_PARENT_RUN_ID_ENV)));
    assert_eq!(
        env_vars.get(&OsString::from(OZ_HARNESS_ENV)),
        Some(&OsString::from("oz"))
    );
    assert!(!env_vars.contains_key(&OsString::from(OZ_MESSAGE_LISTENER_MANAGED_EXTERNALLY_ENV)));
    assert!(!env_vars.contains_key(&OsString::from(
        LEGACY_OZ_PARENT_LISTENER_MANAGED_EXTERNALLY_ENV
    )));
}

#[test]
fn task_env_vars_enable_external_parent_listener_for_claude_runs_without_parent_run_id() {
    let task_id: AmbientAgentTaskId = "550e8400-e29b-41d4-a716-446655440002".parse().unwrap();
    let env_vars = task_env_vars(Some(&task_id), None, Harness::Claude);
    assert_eq!(
        env_vars.get(&OsString::from(OZ_MESSAGE_LISTENER_MANAGED_EXTERNALLY_ENV)),
        Some(&OsString::from("1"))
    );
    assert_eq!(
        env_vars.get(&OsString::from(
            LEGACY_OZ_PARENT_LISTENER_MANAGED_EXTERNALLY_ENV
        )),
        Some(&OsString::from("1"))
    );
}

#[test]
#[serial_test::serial]
fn task_env_vars_propagate_message_listener_state_root_with_legacy_alias() {
    let task_id: AmbientAgentTaskId = "550e8400-e29b-41d4-a716-446655440003".parse().unwrap();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var(
            OZ_MESSAGE_LISTENER_STATE_ROOT_ENV,
            "/tmp/message-listener-root",
        )
    };
    let env_vars = task_env_vars(Some(&task_id), None, Harness::Claude);
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(OZ_MESSAGE_LISTENER_STATE_ROOT_ENV) };

    assert_eq!(
        env_vars.get(&OsString::from(OZ_MESSAGE_LISTENER_STATE_ROOT_ENV)),
        Some(&OsString::from("/tmp/message-listener-root"))
    );
    assert_eq!(
        env_vars.get(&OsString::from(LEGACY_OZ_PARENT_STATE_ROOT_ENV)),
        Some(&OsString::from("/tmp/message-listener-root"))
    );
}

#[test]
fn task_env_vars_can_use_opencode_harness() {
    let task_id: AmbientAgentTaskId = "550e8400-e29b-41d4-a716-446655440004".parse().unwrap();
    let env_vars = task_env_vars(Some(&task_id), Some("parent-run-456"), Harness::OpenCode);

    assert_eq!(
        env_vars.get(&OsString::from(OZ_HARNESS_ENV)),
        Some(&OsString::from("opencode"))
    );
}

#[test]
fn json_format_output_includes_filename_for_file_artifact_created_event() {
    let output = AIAgentOutput {
        messages: vec![AIAgentOutputMessage::artifact_created(
            MessageId::new("message-1".to_string()),
            ArtifactCreatedData::File {
                artifact_uid: "artifact-uid".to_string(),
                filepath: "outputs/report.txt".to_string(),
                filename: "report.txt".to_string(),
                mime_type: "text/plain".to_string(),
                description: Some("Build output for the latest run".to_string()),
                size_bytes: 42,
            },
        )],
        ..Default::default()
    };

    let mut bytes = Vec::new();
    super::output::json::format_output(&output, &mut bytes).expect("json formatting should work");

    let value: serde_json::Value =
        serde_json::from_slice(&bytes).expect("output should be valid json");

    assert_eq!(value["type"], "artifact_created");
    assert_eq!(value["artifact_type"], "file");
    assert_eq!(value["artifact_uid"], "artifact-uid");
    assert_eq!(value["filepath"], "outputs/report.txt");
    assert_eq!(value["filename"], "report.txt");
    assert_eq!(value["mime_type"], "text/plain");
    assert_eq!(value["description"], "Build output for the latest run");
    assert_eq!(value["size_bytes"], 42);
}

// ── build_secret_env_vars tests ──────────────────────────────────────────────

#[test]
#[serial_test::serial]
fn raw_value_only_writes_under_secret_name() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("MY_SECRET") };
    let secrets = HashMap::from([(
        "MY_SECRET".to_string(),
        ManagedSecretValue::raw_value("s3cret"),
    )]);
    let env_vars = build_secret_env_vars(&secrets);
    assert_eq!(
        env_vars.get(&OsString::from("MY_SECRET")),
        Some(&OsString::from("s3cret"))
    );
    assert_eq!(env_vars.len(), 1);
}

#[test]
#[serial_test::serial]
fn anthropic_api_key_writes_anthropic_env_var() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    let secrets = HashMap::from([(
        "my-custom-name".to_string(),
        ManagedSecretValue::anthropic_api_key("sk-ant-test-key"),
    )]);
    let env_vars = build_secret_env_vars(&secrets);
    assert_eq!(
        env_vars.get(&OsString::from("ANTHROPIC_API_KEY")),
        Some(&OsString::from("sk-ant-test-key"))
    );
}

#[test]
#[serial_test::serial]
fn typed_secret_overrides_raw_value_with_same_env_name() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    let typed_key = "sk-ant-typed-key-abcdef";
    let raw_key = "sk-ant-raw-key-ghijkl";
    let secrets = HashMap::from([
        (
            "my-auth".to_string(),
            ManagedSecretValue::anthropic_api_key(typed_key),
        ),
        (
            "ANTHROPIC_API_KEY".to_string(),
            ManagedSecretValue::raw_value(raw_key),
        ),
    ]);
    // Run multiple times to defeat HashMap iteration order flakiness.
    for _ in 0..20 {
        let env_vars = build_secret_env_vars(&secrets);
        assert_eq!(
            env_vars.get(&OsString::from("ANTHROPIC_API_KEY")),
            Some(&OsString::from(typed_key)),
            "Typed secret must always override RawValue with the same env name"
        );
    }
}

#[test]
#[serial_test::serial]
fn bedrock_api_key_writes_all_bedrock_env_vars() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_BEARER_TOKEN_BEDROCK") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CODE_USE_BEDROCK") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_REGION") };
    let secrets = HashMap::from([
        (
            "bedrock-secret".to_string(),
            ManagedSecretValue::anthropic_bedrock_api_key("token-123", "us-west-2"),
        ),
        (
            "AWS_REGION".to_string(),
            ManagedSecretValue::raw_value("eu-west-1"),
        ),
    ]);
    let env_vars = build_secret_env_vars(&secrets);
    assert_eq!(
        env_vars.get(&OsString::from("AWS_BEARER_TOKEN_BEDROCK")),
        Some(&OsString::from("token-123"))
    );
    assert_eq!(
        env_vars.get(&OsString::from("CLAUDE_CODE_USE_BEDROCK")),
        Some(&OsString::from("1"))
    );
    assert_eq!(
        env_vars.get(&OsString::from("AWS_REGION")),
        Some(&OsString::from("us-west-2")),
        "Typed Bedrock secret should win over RawValue for AWS_REGION"
    );
}

#[test]
#[serial_test::serial]
fn bedrock_access_key_writes_all_aws_env_vars() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_ACCESS_KEY_ID") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_SECRET_ACCESS_KEY") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_SESSION_TOKEN") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CODE_USE_BEDROCK") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_REGION") };
    let secrets = HashMap::from([(
        "bedrock-access".to_string(),
        ManagedSecretValue::anthropic_bedrock_access_key(
            "AKID",
            "secret-key",
            Some("session-tok".to_string()),
            "ap-southeast-1",
        ),
    )]);
    let env_vars = build_secret_env_vars(&secrets);
    assert_eq!(
        env_vars.get(&OsString::from("AWS_ACCESS_KEY_ID")),
        Some(&OsString::from("AKID"))
    );
    assert_eq!(
        env_vars.get(&OsString::from("AWS_SECRET_ACCESS_KEY")),
        Some(&OsString::from("secret-key"))
    );
    assert_eq!(
        env_vars.get(&OsString::from("AWS_SESSION_TOKEN")),
        Some(&OsString::from("session-tok"))
    );
    assert_eq!(
        env_vars.get(&OsString::from("CLAUDE_CODE_USE_BEDROCK")),
        Some(&OsString::from("1"))
    );
    assert_eq!(
        env_vars.get(&OsString::from("AWS_REGION")),
        Some(&OsString::from("ap-southeast-1"))
    );
}

/// Ported verbatim from the pin (`02b53fcd8`) now that the `OpenaiApiKey` variant
/// this test needed exists here — see the note in this file's header (#323).
#[test]
#[serial_test::serial]
fn openai_api_key_exports_only_api_key_not_base_url() {
    // The OpenAI typed secret should only export OPENAI_API_KEY as an env var.
    // base_url is piped through the structured secret to the harness instead.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("OPENAI_API_KEY") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("OPENAI_BASE_URL") };
    let secrets = HashMap::from([(
        "openai-key".to_string(),
        ManagedSecretValue::openai_api_key(
            "sk-test-key",
            Some("https://us.api.openai.com/v1".to_string()),
        ),
    )]);
    let env_vars = build_secret_env_vars(&secrets);
    assert_eq!(
        env_vars.get(&OsString::from("OPENAI_API_KEY")),
        Some(&OsString::from("sk-test-key")),
        "OPENAI_API_KEY should be exported from the typed secret"
    );
    assert!(
        !env_vars.contains_key(&OsString::from("OPENAI_BASE_URL")),
        "OPENAI_BASE_URL should NOT be exported as an env var"
    );
}

#[test]
#[serial_test::serial]
fn raw_value_skipped_when_process_env_already_set() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("WORKER_TOKEN", "injected-value") };
    let secrets = HashMap::from([(
        "WORKER_TOKEN".to_string(),
        ManagedSecretValue::raw_value("managed-value"),
    )]);
    let env_vars = build_secret_env_vars(&secrets);
    // The worker-injected env var wins; env_vars should NOT contain it
    // because the child inherits the process env directly.
    assert!(!env_vars.contains_key(&OsString::from("WORKER_TOKEN")));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("WORKER_TOKEN") };
}

#[test]
#[serial_test::serial]
fn worker_injected_env_wins_over_typed_secret() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "worker-key") };
    let secrets = HashMap::from([(
        "my-auth".to_string(),
        ManagedSecretValue::anthropic_api_key("managed-key"),
    )]);
    let env_vars = build_secret_env_vars(&secrets);
    // The typed secret should be skipped entirely; the child inherits
    // ANTHROPIC_API_KEY from the process env.
    assert!(!env_vars.contains_key(&OsString::from("ANTHROPIC_API_KEY")));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
}

#[test]
#[serial_test::serial]
fn worker_injected_env_skips_entire_bedrock_secret() {
    // Only AWS_REGION is worker-injected; the entire Bedrock secret should
    // be atomically skipped — no partial insertion.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("AWS_REGION", "us-east-1") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_BEARER_TOKEN_BEDROCK") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CODE_USE_BEDROCK") };
    let secrets = HashMap::from([(
        "bedrock-secret".to_string(),
        ManagedSecretValue::anthropic_bedrock_api_key("token-456", "eu-central-1"),
    )]);
    let env_vars = build_secret_env_vars(&secrets);
    assert!(
        !env_vars.contains_key(&OsString::from("AWS_BEARER_TOKEN_BEDROCK")),
        "Entire Bedrock secret must be skipped when any field is worker-injected"
    );
    assert!(!env_vars.contains_key(&OsString::from("CLAUDE_CODE_USE_BEDROCK")));
    assert!(!env_vars.contains_key(&OsString::from("AWS_REGION")));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("AWS_REGION") };
}

/// The fork-local half of the pin's managed-MCP rendering tests
/// (`driver_tests.rs:301-420`, `02b53fcd8`): those all start from
/// `installations_from_managed_client_config_json`, which is the dropped managed-MCP
/// (cloud) source. An inline `--mcp '<json>'` spec is the local source that reaches the
/// same `mcp_installations_to_json` rendering, so it is what these exercise instead.
///
/// This is the resolution third-party harnesses now consume: `AgentDriver::prepare_harness`
/// feeds the result to `ThirdPartyHarness::prepare_environment_config`.
#[test]
fn inline_mcp_spec_renders_to_harness_native_json() {
    let (uuids, installations) = AgentDriver::resolve_mcp_specs(&[MCPSpec::Json(
        r#"{"mcpServers":{"docs":{"command":"npx","args":["-y","docs-mcp"]}}}"#.to_string(),
    )])
    .unwrap();
    assert!(uuids.is_empty());

    let rendered = AgentDriver::mcp_installations_to_json(installations, &HashMap::new()).unwrap();

    match &rendered["docs"].transport_type {
        JSONTransportType::CLIServer { command, args, .. } => {
            assert_eq!(command.as_str(), "npx");
            assert_eq!(args, &vec!["-y".to_string(), "docs-mcp".to_string()]);
        }
        other => panic!("expected CLI server, got {other:?}"),
    }
}

/// Secret placeholders in an inline spec are resolved before the servers reach a harness,
/// so a harness config file never receives an unsubstituted `{{VAR}}`.
#[test]
fn inline_mcp_spec_resolves_secret_placeholders_before_rendering() {
    let (_uuids, installations) = AgentDriver::resolve_mcp_specs(&[MCPSpec::Json(
        r#"{"mcpServers":{"github":{"command":"npx","env":{"API_TOKEN":"{{API_TOKEN}}"}}}}"#
            .to_string(),
    )])
    .unwrap();

    let secrets = HashMap::from([(
        "API_TOKEN".to_string(),
        ManagedSecretValue::RawValue {
            value: "real-token".to_string(),
        },
    )]);
    let rendered = AgentDriver::mcp_installations_to_json(installations, &secrets).unwrap();

    match &rendered["github"].transport_type {
        JSONTransportType::CLIServer { env, .. } => {
            assert_eq!(env.get("API_TOKEN").map(String::as_str), Some("real-token"));
        }
        other => panic!("expected CLI server, got {other:?}"),
    }
}

// ── WARP_SKILL_DIRS (upstream c7ab9c028) ─────────────────────────────────────
//
// The unit tests in `crates/ai/src/skills/read_skills_test.rs` cover
// `resolve_skills_dirs` in isolation. These two cover the part only the driver
// can answer: that `load_skills_dirs` passes `me.working_dir` — and not the
// process cwd, which environment preparation may have changed — into that
// resolution, and that what comes back lands in the home/personal bucket.
//
// This fork's driver has no skill-loading *phase* to hook (`load_global_skills`
// was never ported, see DECLINED.md), so `load_skills_dirs` is called directly,
// exactly as upstream's tests do — upstream's tests never go through
// `run_internal` either, so the placement divergence does not reach them.

/// Write a minimal SKILL.md at `{skills_dir}/{name}/SKILL.md`.
/// This is the flat layout expected by `WARP_SKILL_DIRS` (no `.agents/skills` wrapper).
fn write_flat_skill(skills_dir: &Path, name: &str) {
    let skill_dir = skills_dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Skill {name}.\n---\n\n# {name}\n"),
    )
    .unwrap();
}

/// Verifies that `load_skills_dirs` reads skills from the `WARP_SKILL_DIRS` environment
/// variable and registers them in the personal (home) bucket so they are always in scope,
/// regardless of the current working directory.
#[test]
#[serial_test::serial]
fn warp_skill_dirs_env_loads_skills_as_home_tier() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let temp = TempDir::new().unwrap();
        let working_dir = dunce::canonicalize(temp.path()).unwrap();

        // Create two separate flat skills directories (no .agents/skills prefix).
        let skills_dir_a = working_dir.join("extra-skills-a");
        let skills_dir_b = working_dir.join("extra-skills-b");
        write_flat_skill(&skills_dir_a, "env-skill-a1");
        write_flat_skill(&skills_dir_a, "env-skill-a2");
        write_flat_skill(&skills_dir_b, "env-skill-b1");

        // Point WARP_SKILL_DIRS at both directories.
        let skills_dirs_value = format!("{},{}", skills_dir_a.display(), skills_dir_b.display());
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("WARP_SKILL_DIRS", &skills_dirs_value) };

        let terminal_view = add_window_with_terminal(&mut app, None);
        let driver_handle = app.add_model(|ctx| {
            let terminal_driver =
                super::terminal::TerminalDriver::create_from_existing_view(terminal_view, ctx);
            AgentDriver::new_for_test(working_dir.clone(), terminal_driver, ctx)
        });

        let (done_tx, done_rx) = oneshot::channel::<()>();
        driver_handle.update(&mut app, |_, ctx| {
            let spawner = ctx.spawner();
            ctx.spawn(
                async move {
                    AgentDriver::load_skills_dirs(&spawner).await;
                    let _ = done_tx.send(());
                },
                |_, _, _| {},
            );
        });
        done_rx.await.expect("loading task should complete");

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("WARP_SKILL_DIRS") };

        // Skills from WARP_SKILL_DIRS are home-tier, so they appear for any working directory.
        // Use None cwd — home skills are included regardless of is_cloud_environment.
        let skill_names = SkillManager::handle(&app).read(&app, |manager: &SkillManager, ctx| {
            manager
                .get_skills_for_working_directory(None, ctx)
                .into_iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
        });

        assert!(
            skill_names.contains(&"env-skill-a1".to_string()),
            "'env-skill-a1' from WARP_SKILL_DIRS should be loaded; got: {skill_names:?}"
        );
        assert!(
            skill_names.contains(&"env-skill-a2".to_string()),
            "'env-skill-a2' from WARP_SKILL_DIRS should be loaded; got: {skill_names:?}"
        );
        assert!(
            skill_names.contains(&"env-skill-b1".to_string()),
            "'env-skill-b1' from WARP_SKILL_DIRS should be loaded; got: {skill_names:?}"
        );

        // Verify the skills have Home scope (personal tier).
        let scope_check = SkillManager::handle(&app).read(&app, |manager: &SkillManager, ctx| {
            use ai::skills::SkillScope;
            manager
                .get_skills_for_working_directory(None, ctx)
                .into_iter()
                .filter(|s| s.name.starts_with("env-skill-"))
                .all(|s| s.scope == SkillScope::Home)
        });
        assert!(
            scope_check,
            "all WARP_SKILL_DIRS skills must have SkillScope::Home"
        );
    });
}

/// Verifies that relative `WARP_SKILL_DIRS` entries are resolved against the driver's
/// working directory rather than the process's current working directory (which
/// `prepare_environment` may have changed).
#[test]
#[serial_test::serial]
fn warp_skill_dirs_env_relative_entries_resolve_against_working_dir() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);

        let temp = TempDir::new().unwrap();
        let working_dir = dunce::canonicalize(temp.path()).unwrap();

        // Create a flat skills directory inside the working dir and reference it by
        // relative path only. No `rel-skills` directory exists under the process cwd,
        // so this only loads if resolution is anchored at the driver's working dir.
        write_flat_skill(&working_dir.join("rel-skills"), "env-skill-rel");

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("WARP_SKILL_DIRS", "rel-skills") };

        let terminal_view = add_window_with_terminal(&mut app, None);
        let driver_handle = app.add_model(|ctx| {
            let terminal_driver =
                super::terminal::TerminalDriver::create_from_existing_view(terminal_view, ctx);
            AgentDriver::new_for_test(working_dir.clone(), terminal_driver, ctx)
        });

        let (done_tx, done_rx) = oneshot::channel::<()>();
        driver_handle.update(&mut app, |_, ctx| {
            let spawner = ctx.spawner();
            ctx.spawn(
                async move {
                    AgentDriver::load_skills_dirs(&spawner).await;
                    let _ = done_tx.send(());
                },
                |_, _, _| {},
            );
        });
        done_rx.await.expect("loading task should complete");

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("WARP_SKILL_DIRS") };

        let skill_names = SkillManager::handle(&app).read(&app, |manager: &SkillManager, ctx| {
            manager
                .get_skills_for_working_directory(None, ctx)
                .into_iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
        });

        assert!(
            skill_names.contains(&"env-skill-rel".to_string()),
            "'env-skill-rel' should load via a relative WARP_SKILL_DIRS entry resolved against the driver's working dir; got: {skill_names:?}"
        );
    });
}
