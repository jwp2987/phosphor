//! Unit tests for `get_relevant_files_runtime` serialization + argument parsing.
//!
//! The async filter itself (`run_get_relevant_files` → `relevant_files::run_with_context`)
//! makes a real BYOP one-shot call, which needs a live provider config; that path is
//! exercised at the integration level. These tests lock the pure, provider-independent
//! contract: the `_byop_intercepted` sentinel every tool_result must carry, the output
//! shape, and argument parsing.

use super::*;

#[test]
fn args_parse_from_json() {
    let args: GetRelevantFilesArgs =
        serde_json::from_str(r#"{"query":"where is the parser"}"#).unwrap();
    assert_eq!(args.query, "where is the parser");
}

#[test]
fn args_missing_query_fails() {
    assert!(serde_json::from_str::<GetRelevantFilesArgs>(r#"{}"#).is_err());
}

#[test]
fn output_json_carries_intercepted_sentinel_and_fields() {
    let out = GetRelevantFilesOutput {
        query: "q".to_owned(),
        status: "ok".to_owned(),
        total_candidates: 3,
        relevant_files: vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()],
    };
    let v = output_to_json(&out);
    assert_eq!(v["_byop_intercepted"], serde_json::json!(true));
    assert_eq!(v["query"], "q");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["total_candidates"], 3);
    assert_eq!(v["relevant_files"], serde_json::json!(["src/a.rs", "src/b.rs"]));
}

#[test]
fn error_json_carries_sentinel_and_tool_name() {
    let v = error_to_json(&anyhow::anyhow!("boom"));
    assert_eq!(v["_byop_intercepted"], serde_json::json!(true));
    assert_eq!(v["status"], "error");
    assert_eq!(v["tool"], "get_relevant_files");
    assert!(v["message"].as_str().unwrap().contains("boom"));
}
