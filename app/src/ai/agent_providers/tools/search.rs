//! Search tools: `Grep` (line-by-line matching) + `FileGlobV2` (filename globbing).

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use warp_multi_agent_api as api;

use super::OpenAiTool;

// ---------------------------------------------------------------------------
// Grep
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GrepArgs {
    queries: Vec<String>,
    #[serde(default)]
    path: String,
}

fn grep_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "queries": {
                "type": "array",
                "description": "Keywords/regex patterns to search for (each element is an independent query; any hit counts as a match).",
                "items": {"type": "string"}
            },
            "path": {
                "type": "string",
                "description": "Relative path to search (file or directory). Empty string or \".\" means the current working directory.",
                "default": "."
            }
        },
        "required": ["queries"],
        "additionalProperties": false
    })
}

fn grep_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: GrepArgs = serde_json::from_str(args)?;
    Ok(api::message::tool_call::Tool::Grep(
        api::message::tool_call::Grep {
            queries: parsed.queries,
            path: if parsed.path.is_empty() {
                ".".to_owned()
            } else {
                parsed.path
            },
        },
    ))
}

fn grep_result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::grep_result::Result as GR;
    use api::message::tool_call_result::Result as R;
    let r = match result {
        R::Grep(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(GR::Success(s)) => {
            let files: Vec<Value> = s
                .matched_files
                .iter()
                .map(|f| {
                    json!({
                        "path": f.file_path,
                        "lines": f.matched_lines.iter().map(|l| l.line_number).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({ "status": "ok", "files": files })
        }
        Some(GR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static GREP: OpenAiTool = OpenAiTool {
    name: "grep",
    description: include_str!("../prompts/tool_descriptions/grep.md"),
    parameters: grep_parameters,
    from_args: grep_from_args,
    result_to_json: grep_result_to_json,
};

// ---------------------------------------------------------------------------
// FileGlobV2
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GlobArgs {
    patterns: Vec<String>,
    #[serde(default)]
    search_dir: String,
}

fn glob_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "patterns": {
                "type": "array",
                "description": "Filename glob patterns (supports ?, *, […]). E.g. [\"**/*.rs\", \"src/**/*.toml\"].",
                "items": {"type": "string"}
            },
            "search_dir": {
                "type": "string",
                "description": "Relative path of the directory to search; empty means the current working directory.",
                "default": "."
            }
        },
        "required": ["patterns"],
        "additionalProperties": false
    })
}

/// Result-count cap on the match list handed to the model.
///
/// Small models like to run `patterns=["*.sh"], search_dir="."` across an entire home
/// directory, and thousands of paths in one tool result blow past what a small-context
/// local model (e.g. 32K) can hold, cutting the stream off instantly.
///
/// **Enforced in [`glob_result_to_json`], not on the request.** The obvious place for it
/// is the request — and `glob_from_args` used to write a `max_matches` into the proto and
/// call it done — but that value is dropped one layer below:
/// `crates/ai/src/agent/action/convert.rs`'s `From<FileGlobV2>` builds
/// `AIAgentActionType::FileGlobV2 { patterns, search_dir }` from just two of the proto's
/// five fields, because the internal enum has nowhere to put a limit (see its standing
/// upstream `TODO: Maybe implement client side depth and result limits`). The executor
/// therefore applies no count cap at all, and the only backstop was `chat_stream`'s
/// 40,000-character truncation, which slices the serialized JSON mid-array and mid-path.
///
/// Capping here — in fork-original BYOP code — keeps the enum at parity with the pin and
/// puts the cut at the one layer that still knows both the full match count and the shape
/// being serialized, so the model can be told what was left out.
///
/// This is also why the tool takes no `limit` parameter. It used to advertise one; nothing
/// downstream could honour it, which is the same defect as `grep.md`'s phantom `include`
/// argument. A knob that cannot be enforced is worse than no knob, because the model
/// believes it narrowed the search.
const GLOB_RESULT_LIMIT: usize = 200;

fn glob_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: GlobArgs = serde_json::from_str(args)?;
    Ok(api::message::tool_call::Tool::FileGlobV2(
        api::message::tool_call::FileGlobV2 {
            patterns: parsed.patterns,
            search_dir: if parsed.search_dir.is_empty() {
                ".".to_owned()
            } else {
                parsed.search_dir
            },
            // Sent for the day the internal enum grows a slot for it (see
            // `GLOB_RESULT_LIMIT`); currently discarded by `convert.rs`, so the cap that
            // actually runs is the one in `glob_result_to_json`.
            max_matches: GLOB_RESULT_LIMIT as i32,
            max_depth: 0, // unlimited depth
            min_depth: 0,
        },
    ))
}

fn glob_result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::file_glob_v2_result::Result as GR;
    use api::message::tool_call_result::Result as R;
    let r = match result {
        R::FileGlobV2(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(GR::Success(s)) => {
            let total_matches = s.matched_files.len();
            let files: Vec<&str> = s
                .matched_files
                .iter()
                .take(GLOB_RESULT_LIMIT)
                .map(|f| f.file_path.as_str())
                .collect();
            let mut value = json!({ "status": "ok", "files": files });
            if total_matches > files.len() {
                // A shortened list must never be presented as the whole answer: the model
                // would conclude the remaining files do not exist. Say what was cut and
                // what to do about it, in the result itself rather than only in
                // `chat_stream`'s character-level truncation notice.
                value["truncated"] = json!(true);
                value["total_matches"] = json!(total_matches);
                value["note"] = json!(format!(
                    "Only the first {} of {total_matches} matching files are listed. \
                     Narrow `patterns` or `search_dir` and search again for the rest.",
                    files.len()
                ));
            }
            // In protobuf, Success.warnings: String is stderr warning text (e.g. permission
            // errors). Only emitted when non-empty, to avoid noise for the model.
            if !s.warnings.is_empty() {
                value["warnings"] = json!(s.warnings);
            }
            value
        }
        Some(GR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static FILE_GLOB_V2: OpenAiTool = OpenAiTool {
    name: "file_glob",
    description: include_str!("../prompts/tool_descriptions/file_glob.md"),
    parameters: glob_parameters,
    from_args: glob_from_args,
    result_to_json: glob_result_to_json,
};

#[cfg(test)]
mod glob_result_tests {
    use super::*;

    fn success_result(count: usize) -> api::message::tool_call_result::Result {
        let matched_files = (0..count)
            .map(|i| api::file_glob_v2_result::success::FileGlobMatch {
                file_path: format!("/repo/src/file{i}.rs"),
            })
            .collect();
        api::message::tool_call_result::Result::FileGlobV2(api::FileGlobV2Result {
            result: Some(api::file_glob_v2_result::Result::Success(
                api::file_glob_v2_result::Success {
                    matched_files,
                    warnings: String::new(),
                },
            )),
        })
    }

    #[test]
    fn glob_result_under_the_cap_is_reported_whole() {
        let value = glob_result_to_json(&success_result(5)).expect("file_glob result");

        assert_eq!(value["files"].as_array().expect("files").len(), 5);
        assert!(
            value.get("truncated").is_none(),
            "a complete list must not be flagged as truncated"
        );
        assert!(value.get("total_matches").is_none());
    }

    #[test]
    fn glob_result_over_the_cap_is_capped_and_says_so() {
        let total_matches = GLOB_RESULT_LIMIT + 37;
        let value = glob_result_to_json(&success_result(total_matches)).expect("file_glob result");

        assert_eq!(
            value["files"].as_array().expect("files").len(),
            GLOB_RESULT_LIMIT,
            "the match list handed to the model must be capped"
        );
        // Of the two ways to get this wrong, reporting a shortened list as the complete
        // answer is the worse one: the model concludes the missing files do not exist.
        assert_eq!(value["truncated"], json!(true));
        assert_eq!(value["total_matches"], json!(total_matches));
        assert!(
            value["note"]
                .as_str()
                .expect("note")
                .contains(&total_matches.to_string()),
            "the note must state the true total: {value}"
        );
    }
}
