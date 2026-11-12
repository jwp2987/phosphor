//! Tests for the git failure text that may reach the app log (#2191).

use super::*;

const DIFF_BODY: &str = "diff --git a/secrets.env b/secrets.env\n--- a/secrets.env\n+++ b/secrets.env\n@@ -1 +1,2 @@\n+API_KEY=hunter2-not-for-logs\n 2026-11-12T19:59:50Z [DEBUG] input_order hop=view_mode mode=terminal\n";

#[test]
fn failure_log_message_carries_no_diff_body() {
    let error = format!("Git command failed: , {DIFF_BODY}");
    let message = git_error_for_log(&error);
    assert!(message.starts_with("kind=git_failed "), "{message}");
    for leaked in ["hunter2", "API_KEY", "diff --git", "hop=view_mode", "@@"] {
        assert!(!message.contains(leaked), "{leaked} leaked: {message}");
    }
    assert!(
        message.contains(&format!("bytes={}", error.len())),
        "{message}"
    );
    assert!(message.contains("lines=6"), "{message}");
}

#[test]
fn failure_log_message_keeps_gits_own_first_stderr_line() {
    let error = format!("Git command failed: fatal: bad revision 'HEAD'\n, {DIFF_BODY}");
    let message = git_error_for_log(&error);
    assert!(message.contains("fatal: bad revision 'HEAD'"), "{message}");
    assert!(
        !message.contains("hunter2") && !message.contains("diff --git"),
        "{message}"
    );
}

#[test]
fn spawn_failure_keeps_its_first_line_only() {
    let message = git_error_for_log(
        "Failed to execute git command: no out-of-band channel: the ssh session's ControlMaster is gone\nsecond line",
    );
    assert!(message.starts_with("kind=spawn_failed "), "{message}");
    assert!(message.contains("ControlMaster is gone"), "{message}");
    assert!(!message.contains("second line"), "{message}");
}

#[test]
fn unknown_error_is_kind_and_sizes_only() {
    let message = git_error_for_log("something +API_KEY=hunter2 happened");
    assert_eq!(message, "kind=other bytes=35 lines=1");
}

#[test]
fn long_detail_is_cut() {
    let error = format!("Failed to execute git command: {}", "x".repeat(1000));
    let message = git_error_for_log(&error);
    assert!(message.len() < 260, "{message}");
}
