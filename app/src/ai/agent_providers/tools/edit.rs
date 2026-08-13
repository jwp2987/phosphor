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
    /// Purely human-facing (shown to the user for approval) — it must never be able to sink
    /// the whole file operation. `parameters()` below still marks it `required`, so a
    /// well-behaved model sends a good one; the parser is deliberately more forgiving than the
    /// advertised schema, because smaller local BYOP models drop purely descriptive fields far
    /// more often than they drop operational ones (a missing `summary` used to make the entire
    /// call fail `serde_json::from_str` before any file logic ever ran — no file, no error, no
    /// prompt). When absent, `from_args` derives a fallback from `operations`.
    #[serde(default)]
    summary: Option<String>,
    operations: Vec<Operation>,
}

/// The aliases below are not speculative — each is a spelling a BYOP model actually sent,
/// taken from `from_args failed` lines in the app log:
///
/// ```text
/// missing field `file_path` … args_str={"operations":[{"content":…,"op":"create",
///                                        "path":"docker-compose.yml",…}]}
/// missing field `content`   … args_str={"operations":[{"contents":"Hello world!\n",…}]}
/// ```
///
/// A synonym costs the entire call: `serde_json::from_str` fails before any file logic runs,
/// so the user gets no file and (before `ce097bad`) no error either. Accepting the synonym is
/// strictly better than losing the operation, and it cannot introduce ambiguity — serde
/// rejects an alias that collides with another field in the same struct.
///
/// `parameters()` still advertises only the canonical names, so well-behaved models are
/// unaffected. Add to this list only from observed payloads, never from guesswork: an
/// unobserved alias is an untested code path, and a *wrong* guess silently accepts a field
/// that means something else.
#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum Operation {
    /// String search-and-replace (most common, good for one or two changes).
    #[serde(rename = "edit")]
    Edit {
        #[serde(alias = "path")]
        file_path: String,
        search: String,
        replace: String,
    },
    /// Create a new file.
    #[serde(rename = "create")]
    Create {
        #[serde(alias = "path")]
        file_path: String,
        #[serde(alias = "contents")]
        content: String,
    },
    /// Delete an existing file.
    #[serde(rename = "delete")]
    Delete {
        #[serde(alias = "path")]
        file_path: String,
    },
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
        // `summary` is `required` here so a well-behaved model still sends one — but the
        // parser (`Args::summary`, above) accepts its absence and synthesizes a fallback.
        // The schema is guidance for good models; the parser must be forgiving of bad ones.
        "required": ["summary", "operations"],
        "additionalProperties": false
    })
}

/// Fallback used when the model omits (or blanks out) `summary`. Derived purely from the
/// operation list, so the user still gets a meaningful one-line description — e.g. "Create
/// hi.txt" or "Edit 2 files, delete 1 file" — instead of losing the whole batch to a field
/// that only ever mattered for display.
fn fallback_summary(operations: &[Operation]) -> String {
    if let [only] = operations {
        return match only {
            Operation::Edit { file_path, .. } => format!("Edit {file_path}"),
            Operation::Create { file_path, .. } => format!("Create {file_path}"),
            Operation::Delete { file_path } => format!("Delete {file_path}"),
        };
    }
    let (mut edits, mut creates, mut deletes) = (0usize, 0usize, 0usize);
    for op in operations {
        match op {
            Operation::Edit { .. } => edits += 1,
            Operation::Create { .. } => creates += 1,
            Operation::Delete { .. } => deletes += 1,
        }
    }
    let plural = |n: usize, noun: &str| format!("{n} {noun}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if creates > 0 {
        parts.push(format!("create {}", plural(creates, "file")));
    }
    if edits > 0 {
        parts.push(format!("edit {}", plural(edits, "file")));
    }
    if deletes > 0 {
        parts.push(format!("delete {}", plural(deletes, "file")));
    }
    let Some((first, rest)) = parts.split_first() else {
        return "Apply file changes".to_owned();
    };
    let mut summary = first.clone();
    for part in rest {
        summary.push_str(", ");
        summary.push_str(part);
    }
    // Capitalize the leading verb ("create"/"edit"/"delete") for a sentence-like summary.
    let mut chars = summary.chars();
    match chars.next() {
        Some(first_char) => first_char.to_uppercase().collect::<String>() + chars.as_str(),
        None => summary,
    }
}

fn from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: Args = serde_json::from_str(args)?;
    let summary = parsed
        .summary
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| fallback_summary(&parsed.operations));
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
            summary,
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

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
