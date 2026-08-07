//! Tests for the local `read_files` result mapping.
//!
//! Warp keeps the reason-accuracy coverage in
//! `app/src/ai/blocklist/action_model/execute_tests.rs::read_file_failures`,
//! which is now ported verbatim (#369). These tests cover the layer above it:
//! that a mixed batch keeps the files it did read, and that the per-file reasons
//! survive every path the model actually sees — the rendered markdown and the
//! BYOP `role=tool` JSON.
//!
//! These previously asserted the shape of a fork-local workaround: the failure
//! list travelled as a synthetic `Files Failed` file entry because the proto was
//! believed to have no `failed_reads` field. It does have one, so the workaround
//! is gone and these now assert the real structure. The assertions are strictly
//! stronger than before — they name the exact reason per file instead of
//! requiring a catch-all phrase that covered all three causes at once.

use warp_multi_agent_api as api;

use super::local_read_files_result;
use crate::ai::{
    agent::{
        AIAgentActionResultType, AnyFileContent, FileContext, MarkdownActionResult,
        ReadFilesFailedFile, ReadFilesResult,
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

/// A failure carrying a real, cause-specific reason — the thing this module
/// exists to prove reaches the model.
fn failure(path: &str, message: &str) -> ReadFilesFailedFile {
    ReadFilesFailedFile {
        path: path.to_string(),
        message: message.to_string(),
    }
}

fn read_result(
    file_contexts: Vec<FileContext>,
    failed_files: Vec<ReadFilesFailedFile>,
) -> ReadFilesResult {
    local_read_files_result(ReadFileContextResult {
        file_contexts,
        failed_files,
    })
}

fn expect_success(result: ReadFilesResult) -> (Vec<FileContext>, Vec<ReadFilesFailedFile>) {
    match result {
        ReadFilesResult::Success {
            files,
            failed_files,
        } => (files, failed_files),
        other => panic!("expected success, got: {other:?}"),
    }
}

fn expect_error(result: ReadFilesResult) -> String {
    match result {
        ReadFilesResult::Error(message) => message,
        other => panic!("expected error, got: {other:?}"),
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
    let (files, failed_files) = expect_success(read_result(
        vec![text_file("/repo/a.rs", "a"), text_file("/repo/b.rs", "b")],
        Vec::new(),
    ));

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].file_name, "/repo/a.rs");
    assert_eq!(files[1].file_name, "/repo/b.rs");
    assert!(
        failed_files.is_empty(),
        "a clean batch must not report failures: {failed_files:?}"
    );
}

#[test]
fn nothing_requested_returns_an_empty_success() {
    let (files, failed_files) = expect_success(read_result(Vec::new(), Vec::new()));

    assert!(files.is_empty(), "got: {files:?}");
    assert!(failed_files.is_empty(), "got: {failed_files:?}");
}

/// Regression for issue #136: nine readable files were discarded because a
/// tenth path could not be read.
#[test]
fn partial_failure_keeps_the_files_that_were_read() {
    let readable: Vec<_> = (0..9)
        .map(|index| text_file(&format!("/repo/file{index}.rs"), "content"))
        .collect();

    let (files, _) = expect_success(read_result(
        readable,
        vec![failure("/repo/missing.rs", "File does not exist")],
    ));

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
    let (_, failed_files) = expect_success(read_result(
        vec![text_file("/repo/a.rs", "a")],
        vec![
            failure("/repo/missing.rs", "File does not exist"),
            failure("/repo/huge.png", "File is too large to read"),
        ],
    ));

    let paths: Vec<_> = failed_files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"/repo/missing.rs"), "got: {paths:?}");
    assert!(paths.contains(&"/repo/huge.png"), "got: {paths:?}");
}

/// The whole point of #369: each failure keeps its own cause. Previously every
/// reason collapsed into one catch-all string, so an oversized binary was
/// reported as "does not exist".
#[test]
fn each_failure_keeps_its_own_reason() {
    let (_, failed_files) = expect_success(read_result(
        vec![text_file("/repo/a.rs", "a")],
        vec![
            failure("/repo/gone.txt", "File does not exist"),
            failure(
                "/repo/huge.png",
                "File is too large to read (1.5 MB > 1.0 MB limit).",
            ),
            failure(
                "/repo/broken.png",
                "File could not be processed as an image: bad header",
            ),
        ],
    ));

    let by_path = |path: &str| {
        failed_files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} missing from {failed_files:?}"))
            .message
            .clone()
    };

    assert_eq!(by_path("/repo/gone.txt"), "File does not exist");
    assert!(
        by_path("/repo/huge.png").contains("too large to read"),
        "got: {}",
        by_path("/repo/huge.png")
    );
    assert!(
        !by_path("/repo/huge.png").contains("does not exist"),
        "an oversized file must not be reported as absent: {}",
        by_path("/repo/huge.png")
    );
    assert!(
        by_path("/repo/broken.png").contains("could not be processed as an image"),
        "got: {}",
        by_path("/repo/broken.png")
    );
}

#[test]
fn no_files_readable_returns_an_error_naming_each_path() {
    let message = expect_error(read_result(
        Vec::new(),
        vec![
            failure("/repo/missing.rs", "File does not exist"),
            failure("/repo/huge.png", "File is too large to read"),
        ],
    ));

    assert!(
        message.starts_with("Failed to read files: "),
        "got: {message}"
    );
    assert!(
        message.contains("/repo/missing.rs: File does not exist"),
        "got: {message}"
    );
    assert!(
        message.contains("/repo/huge.png: File is too large to read"),
        "got: {message}"
    );
}

/// Warp renders a `**Files Failed:**` section for this case; the fork now does
/// the same rather than smuggling the list through a synthetic file entry.
#[test]
fn failures_reach_the_model_through_the_rendered_action_result() {
    let result = read_result(
        vec![text_file("/repo/a.rs", "a")],
        vec![failure("/repo/missing.rs", "File does not exist")],
    );

    let rendered = MarkdownActionResult(&AIAgentActionResultType::ReadFiles(result)).to_string();

    assert!(rendered.contains("/repo/a.rs"), "got: {rendered}");
    assert!(rendered.contains("**Files Failed:**"), "got: {rendered}");
    assert!(rendered.contains("/repo/missing.rs"), "got: {rendered}");
    assert!(rendered.contains("File does not exist"), "got: {rendered}");
}

#[test]
fn failures_reach_the_model_through_the_byop_tool_result() {
    let result = read_result(
        vec![text_file("/repo/a.rs", "a")],
        vec![failure("/repo/missing.rs", "File does not exist")],
    );

    let json = byop_tool_json(result);

    assert!(json.contains(r#""status":"ok""#), "got: {json}");
    assert!(json.contains("/repo/a.rs"), "got: {json}");
    // The failure now travels in the proto's own `failed_reads` field, carrying
    // its reason, instead of as a fake file named "Files Failed".
    assert!(json.contains("/repo/missing.rs"), "got: {json}");
    assert!(json.contains("File does not exist"), "got: {json}");
}

#[test]
fn total_failure_reaches_the_model_as_a_tool_error() {
    let json = byop_tool_json(read_result(
        Vec::new(),
        vec![failure("/repo/missing.rs", "File does not exist")],
    ));

    assert!(json.contains(r#""status":"error""#), "got: {json}");
    assert!(json.contains("/repo/missing.rs"), "got: {json}");
}
