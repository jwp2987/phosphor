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
//! No further porting opportunity in this file.

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::oneshot;
use warp_cli::agent::Harness;
use warp_cli::mcp::MCPSpec;
use warp_cli::{OZ_CLI_ENV, OZ_HARNESS_ENV, OZ_PARENT_RUN_ID_ENV, OZ_RUN_ID_ENV};
use warp_managed_secrets::ManagedSecretValue;

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
use crate::terminal::cli_agent_sessions::plugin_manager::{
    CliAgentPluginManager, PluginInstallError, PluginInstructions,
};

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

/// A `CliAgentPluginManager` whose capability and on-disk state are fixed by
/// the test, recording whether `setup_notification_plugin` reached `install`
/// or `update`.
///
/// There is no pin test to port here. At `02b53fcd8`,
/// `setup_notification_plugin` and its caller `setup_harness_plugins` appear
/// in exactly one file -- `app/src/ai/agent_sdk/driver.rs`, their own
/// definition -- and the pin's `driver_tests.rs` does not mention either. That
/// is how the fork lost the capability check silently in the first place, so
/// #601 adds the coverage along with the code.
///
/// `has_local_marketplace_override` is deliberately left at its trait default:
/// this call site never consults it, and #600's
/// `ensure_local_claude_child_plugins` is the only place that does.
struct FakePluginManager {
    can_auto_install: bool,
    needs_update: bool,
    is_installed: bool,
    installs: AtomicUsize,
    updates: AtomicUsize,
}

impl FakePluginManager {
    fn new(can_auto_install: bool, needs_update: bool, is_installed: bool) -> Self {
        Self {
            can_auto_install,
            needs_update,
            is_installed,
            installs: AtomicUsize::new(0),
            updates: AtomicUsize::new(0),
        }
    }

    /// `(installs, updates)` observed so far.
    fn calls(&self) -> (usize, usize) {
        (
            self.installs.load(Ordering::SeqCst),
            self.updates.load(Ordering::SeqCst),
        )
    }
}

static FAKE_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: "",
    subtitle: "",
    steps: Vec::new(),
    post_install_notes: Vec::new(),
});

#[async_trait]
impl CliAgentPluginManager for FakePluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        "1.0.0"
    }

    fn can_auto_install(&self) -> bool {
        self.can_auto_install
    }

    fn is_installed(&self) -> bool {
        self.is_installed
    }

    fn needs_update(&self) -> bool {
        self.needs_update
    }

    async fn install(&self) -> Result<(), PluginInstallError> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn update(&self) -> Result<(), PluginInstallError> {
        self.updates.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &FAKE_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &FAKE_INSTRUCTIONS
    }
}

/// The check #601 restores. `plugin_manager_for` returns managers for agents
/// whose plugin was never auto-installable -- OpenCode and DeepSeek always,
/// Codex when its feature flag is off -- and those do not override `install`,
/// so calling it reaches the trait default and returns "Auto-install not
/// supported for this agent". That is the agent declining, not a failure, and
/// warning about it on every launch is what made a real install failure
/// unreadable. Note this manager is deliberately also `needs_update` *and*
/// not installed: the state that most strongly invites a call.
#[tokio::test]
async fn notification_plugin_setup_touches_nothing_when_the_agent_cannot_auto_install() {
    let manager = FakePluginManager::new(false, true, false);

    AgentDriver::setup_notification_plugin(&manager).await;

    assert_eq!(manager.calls(), (0, 0));
}

#[tokio::test]
async fn notification_plugin_setup_installs_when_the_plugin_is_absent() {
    let manager = FakePluginManager::new(true, false, false);

    AgentDriver::setup_notification_plugin(&manager).await;

    assert_eq!(manager.calls(), (1, 0));
}

/// An outdated plugin takes the update path, not the install path: `update()`
/// refreshes the marketplace clone first, which a plain `install()` does not.
#[tokio::test]
async fn notification_plugin_setup_updates_when_the_plugin_is_outdated() {
    let manager = FakePluginManager::new(true, true, true);

    AgentDriver::setup_notification_plugin(&manager).await;

    assert_eq!(manager.calls(), (0, 1));
}

/// The second half of #601. Before it, every third-party-harness launch shelled
/// out to `plugin marketplace add` + `plugin install` even with the plugin
/// already current.
#[tokio::test]
async fn notification_plugin_setup_touches_nothing_when_the_plugin_is_current() {
    let manager = FakePluginManager::new(true, false, true);

    AgentDriver::setup_notification_plugin(&manager).await;

    assert_eq!(manager.calls(), (0, 0));
}
