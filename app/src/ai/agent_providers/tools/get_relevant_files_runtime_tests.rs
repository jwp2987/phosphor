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
        sources: vec![SOURCE_REPO_OUTLINE.to_owned()],
    };
    let v = output_to_json(&out);
    assert_eq!(v["_byop_intercepted"], serde_json::json!(true));
    assert_eq!(v["query"], "q");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["total_candidates"], 3);
    assert_eq!(v["relevant_files"], serde_json::json!(["src/a.rs", "src/b.rs"]));
    assert_eq!(v["sources"], serde_json::json!(["repo_outline"]));
}

// --- Wiring for the codebase embedding index -------------------------------------
//
// These lock the *connection* between the embedding index and the agent's tool
// result, not the retrieval API itself. The retrieval API was already unit-tested and
// still had zero callers for its entire life; a test that only called it again would
// have repeated exactly that mistake.

#[test]
fn tool_is_available_when_only_the_embedding_index_is_configured() {
    // The regression this whole change exists to prevent: a user who turns on
    // `code.indexing.agent_mode_codebase_context` pays to embed their repository, so
    // the agent must have a way to reach the result even with the per-profile
    // outline flag off.
    assert!(relevant_files_tool_available(false, true));
}

#[test]
fn tool_is_available_when_only_the_outline_filter_is_configured() {
    assert!(relevant_files_tool_available(true, false));
}

#[test]
fn tool_is_available_when_both_are_configured() {
    assert!(relevant_files_tool_available(true, true));
}

#[test]
fn tool_is_unavailable_when_neither_is_configured() {
    // The default configuration. Nothing is advertised, nothing is dispatched.
    assert!(!relevant_files_tool_available(false, false));
}

#[test]
fn index_paths_are_reported_relative_to_the_repository_root() {
    let root = std::path::Path::new("/home/u/proj");
    let paths = vec![
        std::path::PathBuf::from("/home/u/proj/src/parser.rs"),
        std::path::PathBuf::from("/home/u/proj/src/lexer.rs"),
    ];
    assert_eq!(
        relative_ranked_paths(&paths, root),
        vec!["src/parser.rs".to_owned(), "src/lexer.rs".to_owned()]
    );
}

#[test]
fn index_paths_outside_the_root_are_passed_through_not_dropped() {
    let root = std::path::Path::new("/home/u/proj");
    let paths = vec![std::path::PathBuf::from("/elsewhere/other.rs")];
    assert_eq!(
        relative_ranked_paths(&paths, root),
        vec!["/elsewhere/other.rs".to_owned()]
    );
}

#[test]
fn merge_keeps_the_embedding_rank_order_ahead_of_the_outline_selection() {
    // The reranker's whole output is an order. If merging sorted, deduplicated into a
    // set, or appended the outline results first, that ranking would be discarded --
    // which is precisely what `process_fragments` was doing before this change.
    let ranked = vec!["src/parser.rs".to_owned(), "src/lexer.rs".to_owned()];
    let outline = vec!["src/main.rs".to_owned(), "src/cli.rs".to_owned()];
    assert_eq!(
        merge_preserving_rank(ranked, &outline),
        vec![
            "src/parser.rs".to_owned(),
            "src/lexer.rs".to_owned(),
            "src/main.rs".to_owned(),
            "src/cli.rs".to_owned(),
        ]
    );
}

#[test]
fn merge_lists_a_file_both_mechanisms_found_once_at_its_embedding_rank() {
    let ranked = vec!["src/parser.rs".to_owned(), "src/lexer.rs".to_owned()];
    // `src/parser.rs` is also chosen by the outline filter, and `src/main.rs` is new.
    let outline = vec!["src/parser.rs".to_owned(), "src/main.rs".to_owned()];
    assert_eq!(
        merge_preserving_rank(ranked, &outline),
        vec![
            "src/parser.rs".to_owned(),
            "src/lexer.rs".to_owned(),
            "src/main.rs".to_owned(),
        ]
    );
}

#[test]
fn merge_with_no_embedding_results_is_the_outline_selection_unchanged() {
    let outline = vec!["src/main.rs".to_owned(), "src/cli.rs".to_owned()];
    assert_eq!(merge_preserving_rank(Vec::new(), &outline), outline);
}

#[tokio::test]
async fn no_index_and_no_snapshot_is_a_graceful_no_context() {
    // The default BYOP configuration: no embedding provider, no outline one-shot.
    // It must be a well-formed tool result, not an error and not a hang.
    let out = run_get_relevant_files_merged(
        GetRelevantFilesArgs {
            query: "where is the parser".to_owned(),
        },
        None,
        None,
    )
    .await;
    assert_eq!(out.status, "no_context");
    assert!(out.relevant_files.is_empty());
    assert!(
        out.sources.is_empty(),
        "nothing ran, so nothing is a source"
    );
    assert_eq!(out.total_candidates, 0);
}

#[tokio::test]
async fn an_empty_query_never_reaches_either_mechanism() {
    let out = run_get_relevant_files_merged(
        GetRelevantFilesArgs {
            query: "   ".to_owned(),
        },
        None,
        None,
    )
    .await;
    assert_eq!(out.status, "no_context");
    assert!(out.sources.is_empty());
}

#[test]
fn error_json_carries_sentinel_and_tool_name() {
    let v = error_to_json(&anyhow::anyhow!("boom"));
    assert_eq!(v["_byop_intercepted"], serde_json::json!(true));
    assert_eq!(v["status"], "error");
    assert_eq!(v["tool"], "get_relevant_files");
    assert!(v["message"].as_str().unwrap().contains("boom"));
}
