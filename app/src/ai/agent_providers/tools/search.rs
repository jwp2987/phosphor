//! 搜索类工具:`Grep`(逐行匹配)+ `FileGlobV2`(文件名通配)。

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
    #[serde(default)]
    limit: i32,
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
                "description": "Maximum number of results; 0 or omitted uses the default cap (200). Values above 2000 are clamped. Prefer narrow patterns/search_dir over raising the limit.",
                "default": 200
            }
        },
        "required": ["patterns"],
        "additionalProperties": false
    })
}

/// `limit` 缺省/0 时的结果条数上限。此前 0 = 无限制:小模型爱用
/// `patterns=["*.sh"], search_dir="."` 扫全家目录,几千条路径进 tool result
/// 后请求直接超出小上下文本地模型(如 32K)的承受范围,流被瞬时掐断。
const DEFAULT_GLOB_LIMIT: i32 = 200;
/// 显式传入 limit 的硬上限。
const MAX_GLOB_LIMIT: i32 = 2000;

fn glob_from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: GlobArgs = serde_json::from_str(args)?;
    let max_matches = if parsed.limit <= 0 {
        DEFAULT_GLOB_LIMIT
    } else {
        parsed.limit.min(MAX_GLOB_LIMIT)
    };
    Ok(api::message::tool_call::Tool::FileGlobV2(
        api::message::tool_call::FileGlobV2 {
            patterns: parsed.patterns,
            search_dir: if parsed.search_dir.is_empty() {
                ".".to_owned()
            } else {
                parsed.search_dir
            },
            max_matches,
            max_depth: 0, // 不限深度
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
            let files: Vec<&str> = s
                .matched_files
                .iter()
                .map(|f| f.file_path.as_str())
                .collect();
            // protobuf 中 Success.warnings: String 是 stderr 警告文本(如权限错误)。
            // 仅在非空时输出,避免给模型噪声。
            let mut value = json!({ "status": "ok", "files": files });
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
