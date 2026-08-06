//! Tests for the local `read_files` result mapping.
//!
//! Warp keeps the equivalent coverage in
//! `app/src/ai/blocklist/action_model/execute_tests.rs::read_file_failures`,
//! which asserts that a failed read is reported with its own reason and that a
//! mixed batch still returns the files that were read. The fork's
//! `read_local_file_context` cannot name the individual reason (see
//! `FAILED_FILE_REASON`), so the reason-accuracy assertions are expressed here
//! as "the message must not claim the file is merely absent".

use warp_multi_agent_api as api;

use super::{FAILED_FILE_REASON, FAILED_FILES_ENTRY_NAME, local_read_files_result};
use crate::ai::{
    agent::{
        AIAgentActionResultType, AnyFileContent, FileContext, MarkdownActionResult, ReadFilesResult,
    },
    agent_providers::tools::serialize_result,
    blocklist::ReadFileContextResult,
};

fn text_file(name: &str, content: &str) -> FileContext {
    FileContext::new(
        name.to_string(),
        AnyFileContent::StringContent(content.to_string()),
        None,
        None,
    )
}

fn read_result(file_contexts: Vec<FileContext>, missing_files: &[&str]) -> ReadFilesResult {
    local_read_files_result(ReadFileContextResult {
        file_contexts,
        missing_files: missing_files.iter().map(|p| p.to_string()).collect(),
    })
}

fn expect_success(result: ReadFilesResult) -> Vec<FileContext> {
    match result {
        ReadFilesResult::Success { files } => files,
        other => panic!("expected success, got: {other:?}"),
    }
}

fn expect_error(result: ReadFilesResult) -> String {
    match result {
        ReadFilesResult::Error(message) => message,
        other => panic!("expected error, got: {other:?}"),
    }
}

fn failure_report(files: &[FileContext]) -> String {
    let entry = files
        .iter()
        .find(|file| file.file_name == FAILED_FILES_ENTRY_NAME)
        .unwrap_or_else(|| panic!("no `{FAILED_FILES_ENTRY_NAME}` entry in {files:?}"));
    match &entry.content {
        AnyFileContent::StringContent(text) => text.clone(),
        AnyFileContent::BinaryContent(_) => panic!("failure report must be text"),
    }
}

/// The BYOP model receives the proto-derived `role=tool` JSON, so this is what
/// actually reaches it — not the in-process enum.
fn byop_tool_json(result: ReadFilesResult) -> String {
    let converted = api::request::input::tool_call_result::Result::try_from(result)
        .expect("read files result converts to the API type");
    let read_files = match converted {
        api::request::input::tool_call_result::Result::ReadFiles(read_files) => read_files,
        other => panic!("expected a read files result, got: {other:?}"),
    };
    serialize_result(&api::message::ToolCallResult {
        result: Some(api::message::tool_call_result::Result::ReadFiles(
            read_files,
        )),
        ..Default::default()
    })
}

#[test]
fn all_files_readable_returns_every_file() {
    let files = expect_success(read_result(
        vec![text_file("/repo/a.rs", "a"), text_file("/repo/b.rs", "b")],
        &[],
    ));

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].file_name, "/repo/a.rs");
    assert_eq!(files[1].file_name, "/repo/b.rs");
    assert!(
        files
            .iter()
            .all(|file| file.file_name != FAILED_FILES_ENTRY_NAME),
        "a clean batch must not report failures: {files:?}"
    );
}

#[test]
fn nothing_requested_returns_an_empty_success() {
    let files = expect_success(read_result(Vec::new(), &[]));

    assert!(files.is_empty(), "got: {files:?}");
}

/// Regression for issue #136: nine readable files were discarded because a
/// tenth path could not be read.
#[test]
fn partial_failure_keeps_the_files_that_were_read() {
    let readable: Vec<_> = (0..9)
        .map(|index| text_file(&format!("/repo/file{index}.rs"), "content"))
        .collect();

    let files = expect_success(read_result(readable, &["/repo/missing.rs"]));

    for index in 0..9 {
        let expected = format!("/repo/file{index}.rs");
        assert!(
            files.iter().any(|file| file.file_name == expected),
            "{expected} was discarded: {files:?}"
        );
    }
}

#[test]
fn partial_failure_reports_every_path_that_failed() {
    let files = expect_success(read_result(
        vec![text_file("/repo/a.rs", "a")],
        &["/repo/missing.rs", "/repo/huge.png"],
    ));

    let report = failure_report(&files);
    assert!(report.contains("/repo/missing.rs"), "got: {report}");
    assert!(report.contains("/repo/huge.png"), "got: {report}");
}

/// The fork cannot tell a missing file from an oversized or unprocessable one,
/// so the reason it reports must cover all three rather than asserting the file
/// is absent — the old "These files do not exist" was wrong for a large binary.
#[test]
fn failure_reason_does_not_claim_the_file_is_merely_absent() {
    let files = expect_success(read_result(
        vec![text_file("/repo/a.rs", "a")],
        &["/repo/huge.png"],
    ));

    let report = failure_report(&files);
    assert!(report.contains("too large"), "got: {report}");
    assert!(report.contains("unprocessable"), "got: {report}");
}

#[test]
fn no_files_readable_returns_an_error_naming_each_path() {
    let message = expect_error(read_result(
        Vec::new(),
        &["/repo/missing.rs", "/repo/huge.png"],
    ));

    assert!(
        message.starts_with("Failed to read files: "),
        "got: {message}"
    );
    assert!(message.contains("/repo/missing.rs"), "got: {message}");
    assert!(message.contains("/repo/huge.png"), "got: {message}");
    assert!(message.contains(FAILED_FILE_REASON), "got: {message}");
}

/// Warp renders a `**Files Failed:**` section for the same case; the fork's
/// synthetic entry must survive into the markdown the agent sees.
#[test]
fn failures_reach_the_model_through_the_rendered_action_result() {
    let result = read_result(vec![text_file("/repo/a.rs", "a")], &["/repo/missing.rs"]);

    let rendered = MarkdownActionResult(&AIAgentActionResultType::ReadFiles(result)).to_string();

    assert!(rendered.contains("/repo/a.rs"), "got: {rendered}");
    assert!(
        rendered.contains(FAILED_FILES_ENTRY_NAME),
        "got: {rendered}"
    );
    assert!(rendered.contains("/repo/missing.rs"), "got: {rendered}");
}

#[test]
fn failures_reach_the_model_through_the_byop_tool_result() {
    let result = read_result(vec![text_file("/repo/a.rs", "a")], &["/repo/missing.rs"]);

    let json = byop_tool_json(result);

    assert!(json.contains(r#""status":"ok""#), "got: {json}");
    assert!(json.contains("/repo/a.rs"), "got: {json}");
    assert!(json.contains(FAILED_FILES_ENTRY_NAME), "got: {json}");
    assert!(json.contains("/repo/missing.rs"), "got: {json}");
}

#[test]
fn total_failure_reaches_the_model_as_a_tool_error() {
    let json = byop_tool_json(read_result(Vec::new(), &["/repo/missing.rs"]));

    assert!(json.contains(r#""status":"error""#), "got: {json}");
    assert!(json.contains("/repo/missing.rs"), "got: {json}");
}
