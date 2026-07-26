//! `apply_file_diffs`: write file / edit file / delete file, all in one.
//!
//! warp's protobuf `ApplyFileDiffs` contains 4 parallel vecs:
//! - `diffs`: search/replace-style string substitution
//! - `v4a_updates`: V4A-style multi-hunk patching (advanced, to be added in Phase 4)
//! - `new_files`: create new files
//! - `deleted_files`: delete files
//!
//! We give the upstream model a single aggregated `apply_file_diffs(operations)`
//! tool, distinguishing subtypes via the `op` field — this is more intuitive and
//! less error-prone than having the model return 4 parallel arrays at once.

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use warp_multi_agent_api as api;

use super::OpenAiTool;

#[derive(Debug, Deserialize)]
struct Args {
    summary: String,
    operations: Vec<Operation>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum Operation {
    /// String search-and-replace (most common, good for one or two changes).
    #[serde(rename = "edit")]
    Edit {
        file_path: String,
        search: String,
        replace: String,
    },
    /// Create a new file.
    #[serde(rename = "create")]
    Create { file_path: String, content: String },
    /// Delete an existing file.
    #[serde(rename = "delete")]
    Delete { file_path: String },
}

fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "summary": {
                "type": "string",
                "description": "Short one-sentence summary of this change, shown to the user for approval. Write it in the same language as the user's messages."
            },
            "operations": {
                "type": "array",
                "description": "All file operations to perform (may be batched). op selects the subtype: edit/create/delete.",
                "items": {
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "op": {"type": "string", "enum": ["edit"]},
                                "file_path": {"type": "string"},
                                "search": {"type": "string", "description": "The original text fragment to replace (must match the file's existing content exactly, including whitespace/newlines)."},
                                "replace": {"type": "string", "description": "The replacement text."}
                            },
                            "required": ["op", "file_path", "search", "replace"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "op": {"type": "string", "enum": ["create"]},
                                "file_path": {"type": "string"},
                                "content": {"type": "string"}
                            },
                            "required": ["op", "file_path", "content"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "op": {"type": "string", "enum": ["delete"]},
                                "file_path": {"type": "string"}
                            },
                            "required": ["op", "file_path"]
                        }
                    ]
                }
            }
        },
        "required": ["summary", "operations"],
        "additionalProperties": false
    })
}

fn from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: Args = serde_json::from_str(args)?;
    let mut diffs = Vec::new();
    let mut new_files = Vec::new();
    let mut deleted_files = Vec::new();
    for op in parsed.operations {
        match op {
            Operation::Edit {
                file_path,
                search,
                replace,
            } => diffs.push(api::message::tool_call::apply_file_diffs::FileDiff {
                file_path,
                search,
                replace,
            }),
            Operation::Create { file_path, content } => new_files
                .push(api::message::tool_call::apply_file_diffs::NewFile { file_path, content }),
            Operation::Delete { file_path } => deleted_files
                .push(api::message::tool_call::apply_file_diffs::DeleteFile { file_path }),
        }
    }
    Ok(api::message::tool_call::Tool::ApplyFileDiffs(
        api::message::tool_call::ApplyFileDiffs {
            summary: parsed.summary,
            diffs,
            v4a_updates: vec![],
            new_files,
            deleted_files,
        },
    ))
}

fn result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::apply_file_diffs_result::Result as ApplyR;
    use api::message::tool_call_result::Result as R;
    let r = match result {
        R::ApplyFileDiffs(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(ApplyR::Success(s)) => {
            let updated: Vec<&str> = s
                .updated_files_v2
                .iter()
                .filter_map(|u| u.file.as_ref().map(|f| f.file_path.as_str()))
                .collect();
            let deleted: Vec<&str> = s
                .deleted_files
                .iter()
                .map(|f| f.file_path.as_str())
                .collect();
            json!({
                "status": "ok",
                "updated_files": updated,
                "deleted_files": deleted,
            })
        }
        Some(ApplyR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled_or_rejected" }),
    };
    Some(value)
}

pub static APPLY_FILE_DIFFS: OpenAiTool = OpenAiTool {
    name: "apply_file_diffs",
    description: include_str!("../prompts/tool_descriptions/apply_file_diffs.md"),
    parameters,
    from_args,
    result_to_json,
};
