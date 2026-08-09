//! Ambient agent task deserialization tests.
//!
//! Ported from Warp's `app/src/server/server_api/ai_tests.rs`. Zap dropped the hosted
//! `server_api` module (and with it `ListRunsResponse`), but `AmbientAgentTask` and its
//! `deserialize_artifacts` / unknown-state wiring are retained here, so the assertions are
//! applied to a single task record instead of a list response. The tests that only exercised
//! `ListRunsResponse`'s skip-invalid-task deserializer have no fork counterpart and are not
//! ported.

use super::*;

#[test]
fn test_deserialize_task_with_artifacts() {
    let json = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440000",
        "title": "Test Task",
        "state": "SUCCEEDED",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": true,
        "artifacts": [
            {
                "created_at": "2024-01-15T10:20:00Z",
                "artifact_type": "PLAN",
                "data": {
                    "document_uid": "doc-1",
                    "notebook_uid": "xyz1234567890123456789",
                    "title": "Plan Title"
                }
            },
            {
                "created_at": "2024-01-15T10:25:00Z",
                "artifact_type": "PULL_REQUEST",
                "data": {
                    "url": "https://github.com/org/repo/pull/1",
                    "branch": "main"
                }
            },
            {
                "created_at": "2024-01-15T10:27:00Z",
                "artifact_type": "FILE",
                "data": {
                    "artifact_uid": "artifact-file-1",
                    "filepath": "outputs/report.txt",
                    "filename": "report.txt",
                    "mime_type": "text/plain",
                    "description": "Daily summary",
                    "size_bytes": 42
                }
            }
        ]
    }"#;

    let task: AmbientAgentTask = serde_json::from_str(json).unwrap();

    assert_eq!(
        task.task_id.to_string(),
        "550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(task.artifacts.len(), 3);

    // Check first artifact (Plan)
    let Artifact::Plan {
        document_uid,
        title,
        ..
    } = &task.artifacts[0]
    else {
        panic!("expected Plan artifact");
    };
    assert_eq!(document_uid, "doc-1");
    assert_eq!(*title, Some("Plan Title".to_string()));

    // Check second artifact (PullRequest)
    let Artifact::PullRequest {
        url,
        branch,
        repo,
        number,
        ..
    } = &task.artifacts[1]
    else {
        panic!("expected PullRequest artifact");
    };
    assert_eq!(url, "https://github.com/org/repo/pull/1");
    assert_eq!(branch, "main");
    assert_eq!(*repo, Some("repo".to_string()));
    assert_eq!(*number, Some(1));

    let Artifact::File {
        artifact_uid,
        filepath,
        filename,
        mime_type,
        description,
        size_bytes,
    } = &task.artifacts[2]
    else {
        panic!("expected File artifact");
    };
    assert_eq!(artifact_uid, "artifact-file-1");
    assert_eq!(filepath, "outputs/report.txt");
    assert_eq!(filename, "report.txt");
    assert_eq!(mime_type, "text/plain");
    assert_eq!(*description, Some("Daily summary".to_string()));
    assert_eq!(*size_bytes, Some(42));
}

#[test]
fn test_deserialize_task_empty_artifacts() {
    let json = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440001",
        "title": "Test Task",
        "state": "INPROGRESS",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": true,
        "artifacts": []
    }"#;

    let task: AmbientAgentTask = serde_json::from_str(json).unwrap();

    assert!(task.artifacts.is_empty());
}

#[test]
fn test_deserialize_task_missing_artifacts_field() {
    // A record may not include the artifacts field at all.
    let json = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440002",
        "title": "Test Task",
        "state": "QUEUED",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": true
    }"#;

    let task: AmbientAgentTask = serde_json::from_str(json).unwrap();

    assert!(task.artifacts.is_empty());
}

#[test]
fn test_deserialize_artifacts_skips_invalid_items() {
    // deserialize_artifacts should skip invalid items and keep valid ones
    let json = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440000",
        "title": "Test Task",
        "state": "SUCCEEDED",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": true,
        "artifacts": [
            {
                "created_at": "2024-01-15T10:20:00Z",
                "artifact_type": "PLAN",
                "data": {
                    "document_uid": "valid-doc",
                    "notebook_uid": "validnotebook123456789",
                    "title": "Valid Plan"
                }
            },
            {
                "created_at": "2024-01-15T10:25:00Z",
                "artifact_type": "UNKNOWN_TYPE",
                "data": {
                    "some_field": "value"
                }
            },
            {
                "created_at": "2024-01-15T10:30:00Z",
                "artifact_type": "PULL_REQUEST",
                "data": {
                    "url": "https://github.com/org/repo/pull/1",
                    "branch": "main"
                }
            }
        ]
    }"#;

    let task: AmbientAgentTask = serde_json::from_str(json).unwrap();

    // Invalid artifact skipped, valid ones kept
    assert_eq!(task.artifacts.len(), 2);
    assert!(matches!(task.artifacts[0], Artifact::Plan { .. }));
    assert!(matches!(task.artifacts[1], Artifact::PullRequest { .. }));
}

#[test]
fn test_deserialize_artifacts_all_invalid_returns_empty() {
    // When all artifacts are invalid, result should be empty vec
    let json = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440000",
        "title": "Test Task",
        "state": "SUCCEEDED",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": true,
        "artifacts": [
            {
                "created_at": "2024-01-15T10:20:00Z",
                "artifact_type": "UNKNOWN_TYPE",
                "data": {}
            }
        ]
    }"#;

    let task: AmbientAgentTask = serde_json::from_str(json).unwrap();

    assert!(task.artifacts.is_empty());
}

#[test]
fn test_deserialize_task_error_and_blocked_states() {
    let errored = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440000",
        "title": "Errored Task",
        "state": "ERROR",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": false
    }"#;
    let blocked = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440001",
        "title": "Blocked Task",
        "state": "BLOCKED",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": false
    }"#;

    let errored: AmbientAgentTask = serde_json::from_str(errored).unwrap();
    let blocked: AmbientAgentTask = serde_json::from_str(blocked).unwrap();
    assert_eq!(errored.state, AmbientAgentTaskState::Error);
    assert_eq!(blocked.state, AmbientAgentTaskState::Blocked);
}

#[test]
fn test_deserialize_task_invalid_state_enum() {
    // Task with an unknown state enum value
    let json = r#"{
        "task_id": "550e8400-e29b-41d4-a716-446655440001",
        "title": "Task with Invalid State",
        "state": "INVALID_STATE",
        "prompt": "test prompt",
        "created_at": "2024-01-15T10:00:00Z",
        "updated_at": "2024-01-15T10:30:00Z",
        "is_sandbox_running": true
    }"#;

    let task: AmbientAgentTask = serde_json::from_str(json).unwrap();

    // Unknown states should deserialize to AmbientAgentTaskState::Unknown.
    assert_eq!(task.title, "Task with Invalid State");
    assert_eq!(task.state, AmbientAgentTaskState::Unknown);
}

// `AmbientAgentTask::display_name()` tests. Built via a struct literal (not the
// JSON style above) since these only need the two fields that feed the
// fallback chain; the rest can take their `Default`/dummy values.
fn make_display_name_task(snapshot_name: Option<&str>, title: &str) -> AmbientAgentTask {
    let now = chrono::Utc::now();
    let agent_config_snapshot = snapshot_name.map(|name| AgentConfigSnapshot {
        name: Some(name.to_string()),
        ..Default::default()
    });
    AmbientAgentTask {
        task_id: "11111111-1111-1111-1111-111111111111".parse().unwrap(),
        parent_run_id: None,
        title: title.to_string(),
        state: AmbientAgentTaskState::InProgress,
        prompt: String::new(),
        created_at: now,
        started_at: Some(now),
        updated_at: now,
        status_message: None,
        source: None,
        session_id: None,
        session_link: None,
        creator: None,
        conversation_id: None,
        request_usage: None,
        is_sandbox_running: false,
        agent_config_snapshot,
        artifacts: vec![],
        last_event_sequence: None,
        children: vec![],
    }
}

#[test]
fn display_name_prefers_agent_config_snapshot_name_over_title() {
    let task = make_display_name_task(Some("frontend-tests"), "Long descriptive task title");
    assert_eq!(task.display_name(), "frontend-tests");
}

#[test]
fn display_name_falls_back_to_title_when_snapshot_name_is_missing() {
    let task = make_display_name_task(None, "Long descriptive task title");
    assert_eq!(task.display_name(), "Long descriptive task title");
}

#[test]
fn display_name_falls_back_to_title_when_snapshot_name_is_whitespace() {
    let task = make_display_name_task(Some("   "), "Long descriptive task title");
    assert_eq!(task.display_name(), "Long descriptive task title");
}

#[test]
fn display_name_returns_literal_agent_when_both_sources_are_empty() {
    let task = make_display_name_task(None, "");
    assert_eq!(task.display_name(), "Agent");
}

#[test]
fn display_name_returns_literal_agent_for_whitespace_only_title() {
    let task = make_display_name_task(None, "   \t\n  ");
    assert_eq!(task.display_name(), "Agent");
}

#[test]
fn display_name_trims_whitespace_at_each_layer() {
    let task = make_display_name_task(Some("  frontend-tests  "), "  Long descriptive title  ");
    assert_eq!(task.display_name(), "frontend-tests");

    let task = make_display_name_task(None, "  Long descriptive title  ");
    assert_eq!(task.display_name(), "Long descriptive title");
}
