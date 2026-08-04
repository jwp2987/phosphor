//! `get_relevant_files` BYOP tool descriptor.
//!
//! Local, on-device relevance filter: given a natural-language query, it asks the
//! user's own BYOP model (a cheap one-shot) which files of the current repository are
//! relevant, judging from each file's path + symbol summary drawn from the local
//! `RepoOutlines` index. This is the BYOP realization of Warp's `GetRelevantFiles`
//! agent action — Warp ran the same outline → LLM-filter step through its cloud
//! `server_api.get_relevant_files`; here it runs through `byop_oneshot_completion`
//! (see [`super::get_relevant_files_runtime`] and
//! `crate::ai::agent_providers::active_ai::relevant_files`).
//!
//! ## Does not go through the protobuf executor
//!
//! Like `search_codebase` / the web tools, this tool is intercepted BY NAME in
//! `chat_stream` before `parse_incoming_tool_call`; it maps to no protobuf executor
//! variant. `from_args` always returns `Err`, and `result_to_json` always returns
//! `None`; the real work runs in `get_relevant_files_runtime::run_get_relevant_files`,
//! invoked directly by `chat_stream::dispatch_byop_get_relevant_files_tool`.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use warp_multi_agent_api as api;

use super::OpenAiTool;

pub const TOOL_NAME: &str = "get_relevant_files";

fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "A natural-language description of what you are looking for. \
                    The tool returns the repository files most relevant to it."
            }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

fn from_args(_args: &str) -> Result<api::message::tool_call::Tool> {
    Err(anyhow!(
        "get_relevant_files is intercepted by chat_stream's BYOP dispatcher; \
         from_args should never be called"
    ))
}

fn result_to_json(_result: &api::message::tool_call_result::Result) -> Option<Value> {
    None
}

pub static GET_RELEVANT_FILES: OpenAiTool = OpenAiTool {
    name: TOOL_NAME,
    description: "Find the files in the LOCAL repository most relevant to a natural-language \
        query. A cheap on-device model judges relevance from each file's path and symbol \
        summary (from the repository's local outline index) and returns the relevant \
        repository-relative file paths. Use it to narrow a large repo down to the handful of \
        files worth reading before you read them. Local-only: no network or cloud access. If \
        the outline is still building, no repository is detected, or no local model is \
        configured, it returns a graceful status instead of an error.",
    parameters,
    from_args,
    result_to_json,
};

#[cfg(test)]
#[path = "get_relevant_files_tests.rs"]
mod get_relevant_files_tests;
