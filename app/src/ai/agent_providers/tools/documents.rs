//! The read / edit / create trio for the Zap Drive local document system.
//!
//! Difference from `read_files` / `apply_file_diffs`: these operations target
//! **documents managed by AIDocumentModel** (Drive's internal local documents,
//! referenced via `document_id`), not files on the file system. The executor goes
//! through `crate::ai::document::ai_document_model::AIDocumentModel`, entirely
//! locally, with no dependency on any server.

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use warp_multi_agent_api as api;

use super::OpenAiTool;

// ---------------------------------------------------------------------------
// Shared: DocumentContent → JSON
// ---------------------------------------------------------------------------

fn document_content_to_json(d: &api::DocumentContent) -> Value {
    let mut v = json!({
        "document_id": d.document_id,
        "content": d.content,
    });
    if let Some(lr) = &d.line_range {
        v["line_range"] = json!({ "start": lr.start, "end": lr.end });
    }
    v
}

// ---------------------------------------------------------------------------
// read_documents
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ReadArgs {
    documents: Vec<ReadDoc>,
}

#[derive(Debug, Deserialize)]
struct ReadDoc {
    document_id: String,
    #[serde(default)]
    line_ranges: Vec<LineRange>,
}

#[derive(Debug, Deserialize)]
struct LineRange {
    start: u32,
    end: u32,
}

fn read_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "documents": {
                "type": "array",
                "description": "Documents to read (each identified by document_id).",
                "items": {
                    "type": "object",
                    "properties": {
                        "document_id": { "type": "string" },
                        "line_ranges": {
                            "type": "array",
                            "description": "Optional list of 1-based inclusive line ranges; empty reads the whole document.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "start": { "type": "integer" },
                                    "end": { "type": "integer" }
                                },
                                "required": ["start", "end"]
                            }
                        }
                    },
                    "required": ["document_id"]
                }
            }
        },
        "required": ["documents"],
        "additionalProperties": false
    })
}

fn read_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: ReadArgs = serde_json::from_str(args)?;
    let docs = parsed
        .documents
        .into_iter()
        .map(|d| api::message::tool_call::read_documents::Document {
            document_id: d.document_id,
            line_ranges: d
                .line_ranges
                .into_iter()
                .map(|r| api::FileContentLineRange {
                    start: r.start,
                    end: r.end,
                })
                .collect(),
        })
        .collect();
    Ok(api::message::tool_call::Tool::ReadDocuments(
        api::message::tool_call::ReadDocuments { documents: docs },
    ))
}

fn read_result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::message::tool_call_result::Result as R;
    use api::read_documents_result::Result as DR;
    let r = match result {
        R::ReadDocuments(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(DR::Success(s)) => json!({
            "status": "ok",
            "documents": s.documents.iter().map(document_content_to_json).collect::<Vec<_>>(),
        }),
        Some(DR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static READ_DOCUMENTS: OpenAiTool = OpenAiTool {
    name: "read_documents",
    description: "Read local Zap Drive documents (referenced by document_id, not filesystem \
                  files). Returns JSON: { documents: [{document_id, content, line_range?}] }. \
                  Use when the user mentions a specific document_id or a particular Drive document.",
    parameters: read_parameters,
    from_args: read_from_args,
    result_to_json: read_result_to_json,
};

// ---------------------------------------------------------------------------
// edit_documents
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EditArgs {
    diffs: Vec<DocDiff>,
}

#[derive(Debug, Deserialize)]
struct DocDiff {
    document_id: String,
    search: String,
    replace: String,
}

fn edit_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "diffs": {
                "type": "array",
                "description": "One search→replace per target document. Each diff describes a single replacement.",
                "items": {
                    "type": "object",
                    "properties": {
                        "document_id": { "type": "string" },
                        "search": {
                            "type": "string",
                            "description": "The original text to replace (must match the document's existing content **exactly**, including whitespace and newlines)."
                        },
                        "replace": {
                            "type": "string",
                            "description": "The replacement text."
                        }
                    },
                    "required": ["document_id", "search", "replace"]
                }
            }
        },
        "required": ["diffs"],
        "additionalProperties": false
    })
}

fn edit_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: EditArgs = serde_json::from_str(args)?;
    let diffs = parsed
        .diffs
        .into_iter()
        .map(|d| api::message::tool_call::edit_documents::DocumentDiff {
            document_id: d.document_id,
            search: d.search,
            replace: d.replace,
        })
        .collect();
    Ok(api::message::tool_call::Tool::EditDocuments(
        api::message::tool_call::EditDocuments { diffs },
    ))
}

fn edit_result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::edit_documents_result::Result as ER;
    use api::message::tool_call_result::Result as R;
    let r = match result {
        R::EditDocuments(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(ER::Success(s)) => json!({
            "status": "ok",
            "updated_documents": s.updated_documents.iter().map(document_content_to_json).collect::<Vec<_>>(),
        }),
        Some(ER::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static EDIT_DOCUMENTS: OpenAiTool = OpenAiTool {
    name: "edit_documents",
    description: "String search-and-replace on existing Zap Drive documents. \
                  Similar to apply_file_diffs::edit, but targets Drive documents (referenced by \
                  document_id). search must match existing content **exactly** (including \
                  whitespace and newlines) or the edit fails.",
    parameters: edit_parameters,
    from_args: edit_from_args,
    result_to_json: edit_result_to_json,
};

// ---------------------------------------------------------------------------
// create_documents
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateArgs {
    new_documents: Vec<NewDoc>,
}

#[derive(Debug, Deserialize)]
struct NewDoc {
    title: String,
    content: String,
}

fn create_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "new_documents": {
                "type": "array",
                "description": "New documents to create.",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Document title (shown in Drive)."
                        },
                        "content": {
                            "type": "string",
                            "description": "Full document content (markdown / plain text)."
                        }
                    },
                    "required": ["title", "content"]
                }
            }
        },
        "required": ["new_documents"],
        "additionalProperties": false
    })
}

fn create_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: CreateArgs = serde_json::from_str(args)?;
    let new_documents = parsed
        .new_documents
        .into_iter()
        .map(|d| api::message::tool_call::create_documents::NewDocument {
            title: d.title,
            content: d.content,
        })
        .collect();
    Ok(api::message::tool_call::Tool::CreateDocuments(
        api::message::tool_call::CreateDocuments { new_documents },
    ))
}

fn create_result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::create_documents_result::Result as CR;
    use api::message::tool_call_result::Result as R;
    let r = match result {
        R::CreateDocuments(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(CR::Success(s)) => json!({
            "status": "ok",
            "created_documents": s.created_documents.iter().map(document_content_to_json).collect::<Vec<_>>(),
        }),
        Some(CR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static CREATE_DOCUMENTS: OpenAiTool = OpenAiTool {
    name: "create_documents",
    description: "Create one or more new documents in Zap Drive (each with title + full \
                  content). Good for persisting analysis results, notes, or todos as reusable \
                  Drive documents.",
    parameters: create_parameters,
    from_args: create_from_args,
    result_to_json: create_result_to_json,
};
