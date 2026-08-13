//! Unit tests for `create_documents`'s `from_args`.
//!
//! The point of this file: a `content`-as-`contents` payload (the same synonym a BYOP model
//! sent for `apply_file_diffs`'s identically-named-and-shaped field, per the app log) must
//! still parse instead of losing the whole call in `serde_json::from_str` before any document
//! is created. `title` has no such alias -- a `name`-only payload must still fail loudly,
//! since no synonym for it has ever been observed.

use warp_multi_agent_api as api;

use super::*;

fn create_documents(args: &str) -> api::message::tool_call::CreateDocuments {
    match (CREATE_DOCUMENTS.from_args)(args).expect("from_args should accept this call") {
        api::message::tool_call::Tool::CreateDocuments(c) => c,
        other => panic!("expected Tool::CreateDocuments, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// `content` accepts the `contents` synonym (mirrors the observed apply_file_diffs payload)
// ---------------------------------------------------------------------------

#[test]
fn contents_synonym_is_accepted_in_place_of_content() {
    let result = create_documents(
        r#"{
            "new_documents": [
                {"title": "Notes", "contents": "Hello world!\n"}
            ]
        }"#,
    );
    assert_eq!(result.new_documents.len(), 1);
    assert_eq!(result.new_documents[0].title, "Notes");
    assert_eq!(result.new_documents[0].content, "Hello world!\n");
}

#[test]
fn canonical_content_field_still_works() {
    let result = create_documents(
        r#"{
            "new_documents": [
                {"title": "Notes", "content": "Hello world!\n"}
            ]
        }"#,
    );
    assert_eq!(result.new_documents[0].content, "Hello world!\n");
}

#[test]
fn multiple_documents_with_mixed_content_spelling_all_parse() {
    let result = create_documents(
        r#"{
            "new_documents": [
                {"title": "A", "content": "a body"},
                {"title": "B", "contents": "b body"}
            ]
        }"#,
    );
    assert_eq!(result.new_documents.len(), 2);
    assert_eq!(result.new_documents[0].content, "a body");
    assert_eq!(result.new_documents[1].content, "b body");
}

// ---------------------------------------------------------------------------
// `title` has no observed synonym -- a payload missing it must still fail loudly
// ---------------------------------------------------------------------------

#[test]
fn missing_title_still_fails_to_parse() {
    let err = (CREATE_DOCUMENTS.from_args)(
        r#"{"new_documents": [{"name": "Notes", "content": "Hello world!\n"}]}"#,
    )
    .expect_err("title is not optional and has no alias; from_args must reject this");
    assert!(
        err.to_string().to_lowercase().contains("title")
            || err.to_string().to_lowercase().contains("missing field"),
        "error should point at the missing field: {err}"
    );
}

#[test]
fn missing_both_content_spellings_still_fails_to_parse() {
    (CREATE_DOCUMENTS.from_args)(r#"{"new_documents": [{"title": "Notes"}]}"#)
        .expect_err("neither content nor contents is present; from_args must reject this");
}

#[test]
fn completely_malformed_json_still_fails_to_parse() {
    (CREATE_DOCUMENTS.from_args)("not json at all")
        .expect_err("garbage input must not silently produce a tool call");
}
