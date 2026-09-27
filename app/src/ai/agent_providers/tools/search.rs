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
    /// Accepted and clamped, never silently dropped — see [`GLOB_RESULT_LIMIT`].
    ///
    /// Kept in the schema for two reasons. Models in the wild send it: the recovery
    /// fixture at `chat_stream.rs`'s `recovers_the_observed_run_shell_log_call_to_file_glob`
    /// is a verbatim call from `zap.log` carrying `"limit":10`, and
    /// `recover_tool_by_arg_shape` requires **every** sent key to exist in the schema, so
    /// removing it made that observed call unrecoverable. And rejecting an argument a model
    /// reasonably supplies costs a round trip to teach it nothing.
    #[serde(default)]
    limit: Option<usize>,
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
            },
            "limit": {
                "type": "integer",
                "description": "Maximum matches to return. Results are always capped at 200 regardless of this value; a smaller value is honoured.",
                "default": 200
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
/// **Two layers now enforce it, deliberately redundant.** [`glob_from_args`] clamps the
/// model's `limit` to this constant and writes it into the proto's `max_matches`.
/// `AIAgentActionType::FileGlobV2::result_limit` (`crates/ai/src/agent/action/mod.rs`)
/// carries that already-clamped value, and the executor
/// (`app/src/ai/blocklist/action_model/execute/file_glob.rs`) truncates the match list to
/// it before the result is even built. [`glob_result_to_json`] then applies this same
/// constant again as a second, independent backstop — so a bug in the executor's
/// truncation (or a `result_limit` of `None`, the case for anything not built from a real
/// tool call: a persisted pre-limit action, or a value built directly in a test) still
/// cannot let an unbounded match list reach the model. Before the executor honoured it,
/// this was the *only* enforcement point, and the only backstop above it was
/// `chat_stream`'s 40,000-character truncation, which slices the serialized JSON mid-array
/// and mid-path — hence keeping this layer even though it is now usually a no-op.
///
/// A smaller `limit` than this constant is honoured (via `result_limit`); a larger one is
/// silently held to it, exactly as the schema's `limit` description says. That distinction
/// — a documented ceiling versus a silently-dropped argument — is what keeps this from
/// being the `grep.md` phantom-`include` defect, which advertised an argument as working
/// and dropped it in silence.
///
/// `limit` was briefly deleted from the schema instead, on 2026-08-21, and that was wrong
/// in a way worth recording: `recover_tool_by_arg_shape` (`chat_stream.rs`) requires every
/// key a model sent to exist in the tool's schema, and its `file_glob` fixture is a
/// verbatim call captured from `zap.log` carrying `"limit":10`. Removing the key made that
/// real observed call unrecoverable — a round trip and a red row for arguments that were
/// already correct.
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
            // Clamped here, once, to `GLOB_RESULT_LIMIT`: `AIAgentActionType::FileGlobV2`'s
            // `result_limit` (populated from this field by `convert.rs`) trusts it as
            // already-clamped and applies it verbatim in the executor
            // (`app/src/ai/blocklist/action_model/execute/file_glob.rs`). A model asking for
            // fewer than the cap is honoured; asking for more is silently held to the cap,
            // exactly as the schema's `limit` description promises. `glob_result_to_json`'s
            // own cap stays as a second, independent backstop.
            max_matches: parsed
                .limit
                .map_or(GLOB_RESULT_LIMIT, |limit| limit.min(GLOB_RESULT_LIMIT))
                as i32,
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

    /// A `limit` smaller than [`GLOB_RESULT_LIMIT`] is forwarded verbatim into the proto's
    /// `max_matches`, which `crates/ai/src/agent/action/convert.rs` carries into
    /// `AIAgentActionType::FileGlobV2::result_limit` for the executor to honour.
    #[test]
    fn glob_from_args_forwards_a_smaller_limit_into_max_matches() {
        let tool = glob_from_args(r#"{"patterns":["*.rs"],"limit":10}"#).expect("valid args");
        let api::message::tool_call::Tool::FileGlobV2(glob) = tool else {
            panic!("expected FileGlobV2");
        };
        assert_eq!(
            glob.max_matches, 10,
            "a limit smaller than the cap must be forwarded verbatim"
        );
    }

    /// A `limit` larger than the cap is held to it, not forwarded verbatim -- the model
    /// cannot use `limit` to request more than the tool ever hands back.
    #[test]
    fn glob_from_args_clamps_a_larger_limit_to_the_cap() {
        let tool = glob_from_args(r#"{"patterns":["*.rs"],"limit":100000}"#).expect("valid args");
        let api::message::tool_call::Tool::FileGlobV2(glob) = tool else {
            panic!("expected FileGlobV2");
        };
        assert_eq!(
            glob.max_matches, GLOB_RESULT_LIMIT as i32,
            "a limit larger than the cap must be held to it, not forwarded verbatim"
        );
    }

    /// No `limit` at all -- `#[serde(default)]` -- must produce the same `max_matches` as
    /// before this field was honoured: the cap itself, not `0` or some other default that
    /// would silently narrow every un-limited call.
    #[test]
    fn glob_from_args_defaults_max_matches_to_the_cap_when_no_limit_is_sent() {
        let tool = glob_from_args(r#"{"patterns":["*.rs"]}"#).expect("valid args");
        let api::message::tool_call::Tool::FileGlobV2(glob) = tool else {
            panic!("expected FileGlobV2");
        };
        assert_eq!(glob.max_matches, GLOB_RESULT_LIMIT as i32);
    }
}
