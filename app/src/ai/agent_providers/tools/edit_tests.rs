//! Unit tests for `apply_file_diffs`'s `from_args`.
//!
//! The point of this file: a `summary`-less payload must still parse and produce a sensible
//! generated summary (a model that drops a purely human-facing field must never lose the
//! whole file operation — see `hi.txt`'s write-up), while a genuinely malformed payload must
//! still fail loudly enough for the caller to surface it to the user.

use warp_multi_agent_api as api;

use super::*;

fn apply_file_diffs(args: &str) -> api::message::tool_call::ApplyFileDiffs {
    match (APPLY_FILE_DIFFS.from_args)(args).expect("from_args should accept this call") {
        api::message::tool_call::Tool::ApplyFileDiffs(a) => a,
        other => panic!("expected Tool::ApplyFileDiffs, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// summary is optional in the parser
// ---------------------------------------------------------------------------

#[test]
fn missing_summary_still_parses_and_gets_a_fallback() {
    let result = apply_file_diffs(
        r#"{
            "operations": [
                {"op": "create", "file_path": "hi.txt", "content": "hello"}
            ]
        }"#,
    );
    assert_eq!(result.summary, "Create hi.txt");
    assert_eq!(result.new_files.len(), 1);
    assert_eq!(result.new_files[0].file_path, "hi.txt");
}

#[test]
fn blank_summary_is_treated_the_same_as_absent() {
    let result = apply_file_diffs(
        r#"{
            "summary": "   ",
            "operations": [
                {"op": "delete", "file_path": "old.txt"}
            ]
        }"#,
    );
    assert_eq!(result.summary, "Delete old.txt");
}

#[test]
fn a_provided_summary_is_kept_verbatim() {
    let result = apply_file_diffs(
        r#"{
            "summary": "Fix the typo in the README",
            "operations": [
                {"op": "edit", "file_path": "README.md", "search": "teh", "replace": "the"}
            ]
        }"#,
    );
    assert_eq!(result.summary, "Fix the typo in the README");
}

#[test]
fn fallback_summary_for_a_mixed_batch_covers_every_op_kind() {
    let result = apply_file_diffs(
        r#"{
            "operations": [
                {"op": "create", "file_path": "a.txt", "content": "a"},
                {"op": "create", "file_path": "b.txt", "content": "b"},
                {"op": "edit", "file_path": "c.txt", "search": "x", "replace": "y"},
                {"op": "delete", "file_path": "d.txt"}
            ]
        }"#,
    );
    assert_eq!(result.summary, "Create 2 files, edit 1 file, delete 1 file");
}

// ---------------------------------------------------------------------------
// operations are still required — a missing/malformed payload must fail loudly
// ---------------------------------------------------------------------------

#[test]
fn missing_operations_field_still_fails_to_parse() {
    let err = (APPLY_FILE_DIFFS.from_args)(r#"{"summary": "do something"}"#)
        .expect_err("operations is not optional; from_args must reject this");
    assert!(
        err.to_string().to_lowercase().contains("operations")
            || err.to_string().to_lowercase().contains("missing field"),
        "error should point at the missing field: {err}"
    );
}

#[test]
fn completely_malformed_json_still_fails_to_parse() {
    (APPLY_FILE_DIFFS.from_args)("not json at all")
        .expect_err("garbage input must not silently produce a tool call");
}

// ---------------------------------------------------------------------------
// observed field-name synonyms
//
// Both payloads below are copied from `from_args failed` lines in the app log. A synonym
// killed the entire call in `serde_json::from_str`, so the file was never written — the same
// user-visible outcome as the `hi.txt` incident, by a different route.
// ---------------------------------------------------------------------------

#[test]
fn create_accepts_the_path_synonym() {
    // args_str={"operations":[{"content":…,"op":"create","path":"docker-compose.yml",…}]}
    let result = apply_file_diffs(
        r#"{
            "summary": "Create docker-compose.yml",
            "operations": [
                {"op": "create", "path": "docker-compose.yml", "content": "version: \"2\"\n"}
            ]
        }"#,
    );
    assert_eq!(result.new_files.len(), 1);
    assert_eq!(result.new_files[0].file_path, "docker-compose.yml");
}

#[test]
fn create_accepts_the_contents_synonym() {
    // args_str={"operations":[{"contents":"Hello world!\n","file_path":"hi.txt",…}]}
    let result = apply_file_diffs(
        r#"{
            "operations": [
                {"op": "create", "file_path": "hi.txt", "contents": "Hello world!\n"}
            ]
        }"#,
    );
    assert_eq!(result.new_files.len(), 1);
    assert_eq!(result.new_files[0].content, "Hello world!\n");
}

#[test]
fn both_synonyms_together_still_parse() {
    let result = apply_file_diffs(
        r#"{"operations":[{"op":"create","path":"a.txt","contents":"x"}]}"#,
    );
    assert_eq!(result.new_files[0].file_path, "a.txt");
    assert_eq!(result.new_files[0].content, "x");
}

#[test]
fn edit_and_delete_accept_the_path_synonym() {
    let result = apply_file_diffs(
        r#"{
            "operations": [
                {"op": "edit", "path": "a.txt", "search": "x", "replace": "y"},
                {"op": "delete", "path": "b.txt"}
            ]
        }"#,
    );
    assert_eq!(result.diffs.len(), 1);
    assert_eq!(result.diffs[0].file_path, "a.txt");
    assert_eq!(result.deleted_files.len(), 1);
    assert_eq!(result.deleted_files[0].file_path, "b.txt");
}

#[test]
fn the_canonical_names_still_win_and_are_unaffected() {
    let result = apply_file_diffs(
        r#"{
            "operations": [
                {"op": "create", "file_path": "canonical.txt", "content": "still works"}
            ]
        }"#,
    );
    assert_eq!(result.new_files[0].file_path, "canonical.txt");
    assert_eq!(result.new_files[0].content, "still works");
}
