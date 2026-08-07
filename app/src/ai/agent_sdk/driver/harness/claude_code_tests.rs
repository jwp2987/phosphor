use std::collections::HashMap;
use std::fs;
use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use super::*;
use crate::ai::agent_events::MessageHydrator;

fn sample_parent_bridge_message(
    sequence: i64,
    message_id: &str,
    subject: &str,
    body: &str,
) -> MessageBridgeMessageRecord {
    MessageBridgeMessageRecord {
        sequence,
        message_id: message_id.to_string(),
        sender_run_id: "parent-run-456".to_string(),
        subject: subject.to_string(),
        body: body.to_string(),
        occurred_at: "2026-04-17T15:46:00Z".to_string(),
    }
}

fn sample_staged_parent_bridge_message(
    sequence: i64,
    message_id: &str,
) -> MessageBridgeMessageRecord {
    MessageBridgeMessageRecord {
        sequence,
        message_id: message_id.to_string(),
        sender_run_id: String::new(),
        subject: String::new(),
        body: String::new(),
        occurred_at: "2026-04-17T15:46:00Z".to_string(),
    }
}

fn write_surfaced_parent_bridge_message(state_dir: &Path, record: &MessageBridgeMessageRecord) {
    fs::write(
        parent_bridge_surfaced_message_path(state_dir, record.sequence, &record.message_id),
        serde_json::to_vec(record).unwrap(),
    )
    .unwrap();
}

#[test]
fn claude_command_uses_session_id_when_not_resuming() {
    let uuid = Uuid::new_v4();
    let cmd = claude_command("claude", &uuid, "/tmp/prompt.txt", None);
    assert!(
        cmd.contains(&format!("--session-id {uuid}")),
        "expected --session-id flag, got: {cmd}"
    );
    assert!(
        !cmd.contains("--resume"),
        "command should not contain --resume, got: {cmd}"
    );
}

#[test]
fn claude_command_pipes_prompt_path() {
    let uuid = Uuid::new_v4();
    let cmd = claude_command("claude", &uuid, "/tmp/prompt with spaces.txt", None);
    assert!(
        cmd.contains("< '/tmp/prompt with spaces.txt'"),
        "expected single-quoted stdin redirect of the prompt path, got: {cmd}"
    );
    assert!(
        cmd.contains("--dangerously-skip-permissions"),
        "expected --dangerously-skip-permissions, got: {cmd}"
    );
}

#[test]
#[serial_test::serial]
fn parent_bridge_root_prefers_environment_override() {
    let tmp = TempDir::new().unwrap();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var(OZ_MESSAGE_LISTENER_STATE_ROOT_ENV, tmp.path()) };
    let root = parent_bridge_root().unwrap();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(OZ_MESSAGE_LISTENER_STATE_ROOT_ENV) };

    assert_eq!(root, tmp.path());
}

#[test]
fn stage_parent_bridge_message_writes_message_record() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("session-123");
    ensure_parent_bridge_state_dir(&state_dir).unwrap();
    let record = sample_staged_parent_bridge_message(42, "msg-123");

    stage_parent_bridge_message(&state_dir, &record).unwrap();

    let staged_path = parent_bridge_staged_message_path(&state_dir, 42, "msg-123");
    let staged_record: MessageBridgeMessageRecord =
        serde_json::from_slice(&fs::read(&staged_path).unwrap()).unwrap();
    assert_eq!(staged_record.sequence, 42);
    assert_eq!(staged_record.message_id, "msg-123");
    assert!(staged_record.sender_run_id.is_empty());
}

#[tokio::test]
async fn prepare_parent_bridge_hook_output_moves_selected_messages_to_surfaced_dir() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("session-123");
    ensure_parent_bridge_state_dir(&state_dir).unwrap();

    let first = sample_parent_bridge_message(
        42,
        "msg-123",
        "Please pivot",
        "Inspect the failing tests first.",
    );
    stage_parent_bridge_message(&state_dir, &first).unwrap();
    stage_parent_bridge_message(
        &state_dir,
        &sample_staged_parent_bridge_message(43, "msg-456"),
    )
    .unwrap();

    let hydrator = MessageHydrator::new();

    let max_context_chars = parent_bridge_char_count(MESSAGE_BRIDGE_CONTEXT_PREAMBLE)
        + parent_bridge_char_count(&render_parent_bridge_message_block(&first));
    prepare_parent_bridge_hook_output(&hydrator, &state_dir, max_context_chars)
        .await
        .unwrap();

    let hook_output: MessageBridgeHookOutput =
        serde_json::from_slice(&fs::read(parent_bridge_hook_output_file(&state_dir)).unwrap())
            .unwrap();
    assert_eq!(hook_output.surfaced_count, 1);
    assert_eq!(hook_output.remaining_staged_count, 1);
    assert!(hook_output.additional_context.contains("Please pivot"));
    assert!(!hook_output.additional_context.contains("Second update"));
    let surfaced_path = parent_bridge_surfaced_message_path(&state_dir, 42, "msg-123");
    assert!(surfaced_path.exists());
    assert!(parent_bridge_staged_message_path(&state_dir, 43, "msg-456").exists());
    assert!(!parent_bridge_staged_message_path(&state_dir, 42, "msg-123").exists());
    let surfaced_record: MessageBridgeMessageRecord =
        serde_json::from_slice(&fs::read(&surfaced_path).unwrap()).unwrap();
    assert_eq!(surfaced_record.subject, first.subject);
    assert_eq!(surfaced_record.body, first.body);
}

#[tokio::test]
async fn acknowledge_parent_bridge_hook_output_marks_messages_delivered_and_clears_state() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("session-123");
    ensure_parent_bridge_state_dir(&state_dir).unwrap();

    let record = sample_parent_bridge_message(
        42,
        "msg-123",
        "Please pivot",
        "Inspect the failing tests first.",
    );
    write_surfaced_parent_bridge_message(&state_dir, &record);
    fs::write(
        parent_bridge_hook_output_file(&state_dir),
        serde_json::to_vec(&MessageBridgeHookOutput {
            additional_context: "context".to_string(),
            remaining_staged_count: 0,
            surfaced_count: 1,
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(parent_bridge_hook_output_ack_file(&state_dir), "").unwrap();

    let hydrator = MessageHydrator::new();

    acknowledge_parent_bridge_hook_output(&hydrator, &state_dir)
        .await
        .unwrap();

    assert!(!parent_bridge_surfaced_message_path(&state_dir, 42, "msg-123").exists());
    assert!(!parent_bridge_hook_output_file(&state_dir).exists());
    assert!(!parent_bridge_hook_output_ack_file(&state_dir).exists());
}

#[tokio::test]
async fn prepare_parent_bridge_hook_output_reuses_surfaced_records_without_rehydrating() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("session-123");
    ensure_parent_bridge_state_dir(&state_dir).unwrap();

    let record = sample_parent_bridge_message(
        42,
        "msg-123",
        "Please pivot",
        "Inspect the failing tests first.",
    );
    write_surfaced_parent_bridge_message(&state_dir, &record);

    let hydrator = MessageHydrator::new();

    let max_context_chars = parent_bridge_char_count(MESSAGE_BRIDGE_CONTEXT_PREAMBLE)
        + parent_bridge_char_count(&render_parent_bridge_message_block(&record));
    prepare_parent_bridge_hook_output(&hydrator, &state_dir, max_context_chars)
        .await
        .unwrap();

    let hook_output: MessageBridgeHookOutput =
        serde_json::from_slice(&fs::read(parent_bridge_hook_output_file(&state_dir)).unwrap())
            .unwrap();
    assert_eq!(hook_output.surfaced_count, 1);
    assert_eq!(hook_output.remaining_staged_count, 0);
    assert!(hook_output.additional_context.contains(&record.subject));
}

#[tokio::test]
async fn prepare_parent_bridge_hook_output_truncates_single_large_message() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("session-123");
    ensure_parent_bridge_state_dir(&state_dir).unwrap();
    let long_body = "x".repeat(200);
    stage_parent_bridge_message(
        &state_dir,
        &sample_parent_bridge_message(42, "msg-123", "Please pivot", &long_body),
    )
    .unwrap();
    let hydrator = MessageHydrator::new();

    let max_context_chars = parent_bridge_char_count(MESSAGE_BRIDGE_CONTEXT_PREAMBLE) + 48;
    prepare_parent_bridge_hook_output(&hydrator, &state_dir, max_context_chars)
        .await
        .unwrap();

    let hook_output: MessageBridgeHookOutput =
        serde_json::from_slice(&fs::read(parent_bridge_hook_output_file(&state_dir)).unwrap())
            .unwrap();
    assert_eq!(hook_output.surfaced_count, 1);
    assert!(
        hook_output.additional_context.ends_with("..."),
        "expected truncated context, got: {}",
        hook_output.additional_context
    );
    assert!(parent_bridge_char_count(&hook_output.additional_context) <= max_context_chars);
}

#[tokio::test]
async fn acknowledge_parent_bridge_hook_output_ignores_missing_ack_marker() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("session-123");
    ensure_parent_bridge_state_dir(&state_dir).unwrap();

    let record = sample_parent_bridge_message(
        42,
        "msg-123",
        "Please pivot",
        "Inspect the failing tests first.",
    );
    write_surfaced_parent_bridge_message(&state_dir, &record);

    let hydrator = MessageHydrator::new();

    acknowledge_parent_bridge_hook_output(&hydrator, &state_dir)
        .await
        .unwrap();

    assert!(parent_bridge_surfaced_message_path(&state_dir, 42, "msg-123").exists());
}

#[test]
fn prepare_claude_config_creates_config_file_without_api_suffix() {
    let tmp = TempDir::new().unwrap();
    let claude_json_path = tmp.path().join(".claude.json");
    let working_dir = tmp.path().join("workspace/project");

    prepare_claude_config(&claude_json_path, &working_dir, None).unwrap();

    let claude_config: Value =
        serde_json::from_slice(&fs::read(claude_json_path).unwrap()).unwrap();
    assert_eq!(claude_config["hasCompletedOnboarding"], Value::Bool(true));
    assert_eq!(
        claude_config["lspRecommendationDisabled"],
        Value::Bool(true)
    );
    let working_dir_key = working_dir.to_string_lossy().to_string();
    assert_eq!(
        claude_config["projects"][working_dir_key]["hasTrustDialogAccepted"],
        Value::Bool(true)
    );
    assert_eq!(claude_config.get("customApiKeyResponses"), None);
}

#[test]
fn prepare_claude_config_creates_config_file_with_api_suffix() {
    let tmp = TempDir::new().unwrap();
    let claude_json_path = tmp.path().join(".claude.json");
    let working_dir = tmp.path().join("workspace/project");

    prepare_claude_config(
        &claude_json_path,
        &working_dir,
        Some("QLWn-dUnuwQ-hIhDiAAA"),
    )
    .unwrap();

    let claude_config: Value =
        serde_json::from_slice(&fs::read(claude_json_path).unwrap()).unwrap();
    assert_eq!(
        claude_config["customApiKeyResponses"]["approved"],
        serde_json::json!(["QLWn-dUnuwQ-hIhDiAAA"]),
    );
}

#[test]
fn prepare_claude_config_merges_existing_config() {
    let tmp = TempDir::new().unwrap();
    let claude_json_path = tmp.path().join(".claude.json");
    fs::write(
        &claude_json_path,
        r#"{"theme":"dark","projects":{"/existing/project":{"allowedTools":["Bash"],"nested":{"value":2}}},"customApiKeyResponses":{"approved":["existing-suffix-12345"]}}"#,
    )
    .unwrap();

    let working_dir = tmp.path().join("workspace/project");
    prepare_claude_config(
        &claude_json_path,
        &working_dir,
        Some("new-suffix-1234567890"),
    )
    .unwrap();

    let claude_config: Value =
        serde_json::from_slice(&fs::read(claude_json_path).unwrap()).unwrap();
    assert_eq!(claude_config["theme"], "dark");
    assert_eq!(
        claude_config["lspRecommendationDisabled"],
        Value::Bool(true)
    );
    assert_eq!(
        claude_config["projects"]["/existing/project"]["allowedTools"],
        serde_json::json!(["Bash"])
    );
    assert_eq!(
        claude_config["projects"]["/existing/project"]["nested"]["value"],
        2
    );
    // Both existing and new suffixes should be present.
    assert_eq!(
        claude_config["customApiKeyResponses"]["approved"],
        serde_json::json!(["existing-suffix-12345", "new-suffix-1234567890"]),
    );
    let working_dir_key = working_dir.to_string_lossy().to_string();
    assert_eq!(
        claude_config["projects"][working_dir_key]["hasTrustDialogAccepted"],
        Value::Bool(true)
    );
}

#[test]
fn prepare_claude_config_no_duplicate_suffix() {
    let tmp = TempDir::new().unwrap();
    let claude_json_path = tmp.path().join(".claude.json");
    fs::write(
        &claude_json_path,
        r#"{"customApiKeyResponses":{"approved":["QLWn-dUnuwQ-hIhDiAAA"]}}"#,
    )
    .unwrap();

    let working_dir = tmp.path().join("workspace/project");
    prepare_claude_config(
        &claude_json_path,
        &working_dir,
        Some("QLWn-dUnuwQ-hIhDiAAA"),
    )
    .unwrap();

    let claude_config: Value =
        serde_json::from_slice(&fs::read(claude_json_path).unwrap()).unwrap();
    assert_eq!(
        claude_config["customApiKeyResponses"]["approved"],
        serde_json::json!(["QLWn-dUnuwQ-hIhDiAAA"]),
    );
}

#[test]
fn prepare_claude_config_none_suffix_preserves_existing_responses() {
    let tmp = TempDir::new().unwrap();
    let claude_json_path = tmp.path().join(".claude.json");
    fs::write(
        &claude_json_path,
        r#"{"customApiKeyResponses":{"approved":["existing-suffix-12345"],"rejected":["bad-key"]}}"#,
    )
    .unwrap();

    let working_dir = tmp.path().join("workspace/project");
    prepare_claude_config(&claude_json_path, &working_dir, None).unwrap();

    let claude_config: Value =
        serde_json::from_slice(&fs::read(claude_json_path).unwrap()).unwrap();
    assert_eq!(
        claude_config["customApiKeyResponses"]["approved"],
        serde_json::json!(["existing-suffix-12345"]),
    );
    assert_eq!(
        claude_config["customApiKeyResponses"]["rejected"],
        serde_json::json!(["bad-key"]),
    );
}

#[test]
#[serial_test::serial]
fn resolve_suffix_from_raw_value_secret() {
    // Hermeticity: the resolver now prefers a key already present in the
    // process environment, so clear it before exercising the secrets map.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
    let key = "sk-ant-api03-abcdefghij1234567890ABCDEFGHIJ1234567890abcdefghij1234567890QLWn-dUnuwQ-hIhDiAAA";
    let secrets = HashMap::from([(
        "ANTHROPIC_API_KEY".to_string(),
        ManagedSecretValue::raw_value(key),
    )]);
    let suffix = resolve_anthropic_api_key_suffix(&secrets);
    assert_eq!(suffix.as_deref(), Some("QLWn-dUnuwQ-hIhDiAAA"));
}

#[test]
#[serial_test::serial]
fn resolve_suffix_from_anthropic_api_key_secret() {
    // Hermeticity: the resolver now prefers a key already present in the
    // process environment, so clear it before exercising the secrets map.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
    let key = "sk-ant-api03-abcdefghij1234567890ABCDEFGHIJ1234567890abcdefghij1234567890QLWn-dUnuwQ-hIhDiAAA";
    let secrets = HashMap::from([(
        "ANTHROPIC_API_KEY".to_string(),
        ManagedSecretValue::anthropic_api_key(key),
    )]);
    let suffix = resolve_anthropic_api_key_suffix(&secrets);
    assert_eq!(suffix.as_deref(), Some("QLWn-dUnuwQ-hIhDiAAA"));
}

#[test]
#[serial_test::serial]
fn resolve_suffix_from_anthropic_api_key_with_different_secret_name() {
    // Hermeticity: the resolver now prefers a key already present in the
    // process environment, so clear it before exercising the secrets map.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
    let key = "sk-ant-api03-abcdefghij1234567890ABCDEFGHIJ1234567890abcdefghij1234567890QLWn-dUnuwQ-hIhDiAAA";
    // Secret name doesn't match the env var, but the AnthropicApiKey variant
    // should still be found by iterating all secrets.
    let secrets = HashMap::from([(
        "my-anthropic-key".to_string(),
        ManagedSecretValue::anthropic_api_key(key),
    )]);
    let suffix = resolve_anthropic_api_key_suffix(&secrets);
    assert_eq!(suffix.as_deref(), Some("QLWn-dUnuwQ-hIhDiAAA"));
}

#[test]
#[serial_test::serial]
fn resolve_suffix_prefers_anthropic_api_key_variant_over_raw_value() {
    // Hermeticity: the resolver now prefers a key already present in the
    // process environment, so clear it before exercising the secrets map.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
    let anthropic_key = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-anthropic-suffix";
    let raw_key = "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB-raw-suffix";
    let secrets = HashMap::from([
        (
            "my-anthropic-key".to_string(),
            ManagedSecretValue::anthropic_api_key(anthropic_key),
        ),
        (
            "ANTHROPIC_API_KEY".to_string(),
            ManagedSecretValue::raw_value(raw_key),
        ),
    ]);
    let suffix = resolve_anthropic_api_key_suffix(&secrets);
    // AnthropicApiKey variant should be preferred.
    assert_eq!(suffix.as_deref(), Some("AAA-anthropic-suffix"));
}

#[test]
#[serial_test::serial]
fn resolve_suffix_returns_none_for_short_key() {
    // Hermeticity: the resolver now prefers a key already present in the
    // process environment, so clear it before exercising the secrets map.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
    let secrets = HashMap::from([(
        "ANTHROPIC_API_KEY".to_string(),
        ManagedSecretValue::raw_value("short"),
    )]);
    assert_eq!(resolve_anthropic_api_key_suffix(&secrets), None);
}

#[test]
#[serial_test::serial]
fn resolve_suffix_returns_none_for_short_anthropic_api_key() {
    // Hermeticity: the resolver now prefers a key already present in the
    // process environment, so clear it before exercising the secrets map.
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
    let secrets = HashMap::from([(
        "ANTHROPIC_API_KEY".to_string(),
        ManagedSecretValue::anthropic_api_key("short"),
    )]);
    assert_eq!(resolve_anthropic_api_key_suffix(&secrets), None);
}

#[test]
fn prepare_claude_settings_creates_settings_file() {
    let tmp = TempDir::new().unwrap();
    let claude_settings_path = tmp.path().join(".claude/settings.json");

    prepare_claude_settings(&claude_settings_path).unwrap();

    let claude_settings: Value =
        serde_json::from_slice(&fs::read(claude_settings_path).unwrap()).unwrap();
    assert_eq!(
        claude_settings["skipDangerousModePermissionPrompt"],
        Value::Bool(true)
    );
}

#[test]
fn prepare_claude_settings_merges_existing_settings() {
    let tmp = TempDir::new().unwrap();
    let claude_settings_path = tmp.path().join("settings.json");
    fs::write(
        &claude_settings_path,
        r#"{"editor":"vim","nested":{"value":1}}"#,
    )
    .unwrap();

    prepare_claude_settings(&claude_settings_path).unwrap();

    let claude_settings: Value =
        serde_json::from_slice(&fs::read(claude_settings_path).unwrap()).unwrap();
    assert_eq!(claude_settings["editor"], "vim");
    assert_eq!(claude_settings["nested"]["value"], 1);
    assert_eq!(
        claude_settings["skipDangerousModePermissionPrompt"],
        Value::Bool(true)
    );
}

// ── Tests ported from the pinned Warp oracle (`02b53fcd8`) ───────────────────
//
// Source: `app/src/ai/agent_sdk/driver/harness/claude_code_tests.rs` at the pin.
// The remaining oracle tests in that file need source this fork does not ship
// (`claude_transcript`, `serialize_claude_mcp_config`, the parent-bridge event
// cursor, `MessageBridgeCleanupDisposition`, `--resume`, `prepare_local_wake_command`).

#[test]
#[serial_test::serial]
fn prepare_claude_environment_config_without_config_dir_uses_home_global_config() {
    let home_dir = TempDir::new().unwrap();
    let old_home = std::env::var_os("HOME");
    let old_config_dir = std::env::var_os("CLAUDE_CONFIG_DIR");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("HOME", home_dir.path()) };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

    let working_dir = home_dir.path().join("workspace/project");
    // Adaptation: the oracle's `prepare_claude_environment_config` takes the
    // already-resolved env vars; this fork's takes the managed-secret map.
    prepare_claude_environment_config(&working_dir, &HashMap::new()).unwrap();

    assert!(home_dir.path().join(CLAUDE_JSON_FILE_NAME).exists());
    assert!(home_dir
        .path()
        .join(".claude")
        .join(CLAUDE_SETTINGS_FILE_NAME)
        .exists());
    assert!(!home_dir
        .path()
        .join(".claude")
        .join(CLAUDE_JSON_FILE_NAME)
        .exists());

    match old_home {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(home) => unsafe { std::env::set_var("HOME", home) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("HOME") },
    }
    match old_config_dir {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(dir) => unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") },
    }
}

#[test]
#[serial_test::serial]
fn prepare_claude_environment_config_with_config_dir_uses_dir_global_config() {
    let home_dir = TempDir::new().unwrap();
    let claude_config_dir = TempDir::new().unwrap();
    let old_home = std::env::var_os("HOME");
    let old_config_dir = std::env::var_os("CLAUDE_CONFIG_DIR");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("HOME", home_dir.path()) };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", claude_config_dir.path()) };

    let working_dir = home_dir.path().join("workspace/project");
    // Adaptation: see the sibling test — secrets map instead of resolved env vars.
    prepare_claude_environment_config(&working_dir, &HashMap::new()).unwrap();

    assert!(claude_config_dir
        .path()
        .join(CLAUDE_JSON_FILE_NAME)
        .exists());
    assert!(claude_config_dir
        .path()
        .join(CLAUDE_SETTINGS_FILE_NAME)
        .exists());
    assert!(!home_dir.path().join(CLAUDE_JSON_FILE_NAME).exists());

    match old_home {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(home) => unsafe { std::env::set_var("HOME", home) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("HOME") },
    }
    match old_config_dir {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(dir) => unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") },
    }
}

#[test]
#[serial_test::serial]
fn resolve_suffix_returns_none_when_empty() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
    assert_eq!(resolve_anthropic_api_key_suffix(&HashMap::new()), None);
}

#[test]
#[serial_test::serial]
fn suffix_uses_worker_injected_env_when_present() {
    let worker_key = "sk-ant-api03-WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW-worker-suffix!";
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var(ANTHROPIC_API_KEY_ENV, worker_key) };
    // Even when the secrets map carries a different value, the worker env wins.
    // Adaptation: the oracle passes the resolved env-var map here; this fork
    // resolves from the managed-secret map, so the competing value is supplied
    // as an `AnthropicApiKey` secret.
    let secrets = HashMap::from([(
        "my-auth".to_string(),
        ManagedSecretValue::anthropic_api_key(
            "sk-ant-api03-RRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRR-resolved-val!",
        ),
    )]);
    let suffix = resolve_anthropic_api_key_suffix(&secrets);
    let expected = &worker_key[worker_key.len() - 20..];
    assert_eq!(suffix.as_deref(), Some(expected));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ANTHROPIC_API_KEY_ENV) };
}
