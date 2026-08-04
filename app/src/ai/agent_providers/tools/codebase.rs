//! `search_codebase` BYOP tool descriptor.
//!
//! Local-only codebase symbol search. The cloud `Tool::SearchCodebase` proto variant was
//! deleted from the pinned `warp_multi_agent_api`, so this tool deliberately does NOT map to a
//! protobuf executor variant. Instead it mirrors the `webfetch` / `websearch` interception
//! pattern: `chat_stream` catches it by name before `parse_incoming_tool_call` and answers it
//! from a local `RepoOutlines`-derived symbol snapshot (see [`super::codebase_runtime`]).
//!
//! ## Does not go through the protobuf executor
//!
//! `from_args` always returns `Err`, and `result_to_json` always returns `None`. The actual
//! search runs in `codebase_runtime::run_search_codebase`, invoked directly by
//! `chat_stream::dispatch_byop_codebase_tool`.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use warp_multi_agent_api as api;

use super::OpenAiTool;

pub const TOOL_NAME: &str = "search_codebase";

fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Fuzzy search query matched against code symbol names \
                    (functions, types, methods, constants, ...) indexed from the current \
                    repository."
            },
            "max_results": {
                "type": "integer",
                "description": "Maximum number of matches to return (default 20).",
                "minimum": 1,
                "maximum": 100
            }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

fn from_args(_args: &str) -> Result<api::message::tool_call::Tool> {
    Err(anyhow!(
        "search_codebase is intercepted by chat_stream's BYOP codebase tool dispatcher; \
         from_args should never be called"
    ))
}

fn result_to_json(_result: &api::message::tool_call_result::Result) -> Option<Value> {
    None
}

pub static SEARCH_CODEBASE: OpenAiTool = OpenAiTool {
    name: TOOL_NAME,
    description: "Search the LOCAL codebase for code symbols (functions, types, methods, \
        constants) relevant to a query, using the repository's on-device symbol index. Returns \
        the top matching symbols with their name, repository-relative file path, and line \
        number. Use it to quickly locate where something is defined before reading files. \
        Local-only: no network or cloud access. If the index is still building or no repository \
        is detected, it returns a graceful status instead of an error.",
    parameters,
    from_args,
    result_to_json,
};

#[cfg(test)]
#[path = "codebase_tests.rs"]
mod codebase_tests;
