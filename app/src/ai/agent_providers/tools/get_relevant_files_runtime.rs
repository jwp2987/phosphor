//! Local execution logic for the BYOP `get_relevant_files` tool.
//!
//! Mirrors `codebase_runtime`'s request-snapshot split, but the actual filter is an
//! async BYOP one-shot (like the web tools do async I/O) rather than a pure CPU
//! search: it asks the user's own model which candidate files are relevant.
//!
//! The snapshot ([`RelevantFilesSnapshot`]) is built once per request in
//! `RequestParams::new` (where an `AppContext` is available) by
//! [`collect_relevant_files_snapshot`]: the candidate file+symbol list from the local
//! `RepoOutlines` index, plus the query-independent one-shot context (config + system
//! prompt) resolved via `active_ai::relevant_files::prepare_context`. The `chat_stream`
//! interceptor has no `AppContext`, so it can only pair this pre-materialized snapshot
//! with the model's query — hence the split.
//!
//! ## `_byop_intercepted` sentinel
//!
//! Every tool_result emitted here carries `"_byop_intercepted": true`, which the
//! controller consumes to trigger auto-resume so the model isn't left waiting.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ai::agent_providers::active_ai::relevant_files;

/// A request-scoped snapshot powering the `get_relevant_files` tool: the candidate
/// files (path + symbol summary) from the active repo's local outline, plus the
/// query-independent one-shot context. Carried on `RequestParams` so the
/// (AppContext-less) `chat_stream` interceptor can run the filter locally.
///
/// `Debug` is safe to derive: `FileEntry` holds no secrets, and `PreparedContext` has
/// a hand-written `Debug` that redacts the provider api_key.
#[derive(Debug, Clone)]
pub struct RelevantFilesSnapshot {
    pub candidates: Vec<relevant_files::FileEntry>,
    pub prepared: relevant_files::PreparedContext,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetRelevantFilesArgs {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GetRelevantFilesOutput {
    pub query: String,
    /// `"ok"` | `"no_context"` | `"no_matches"`.
    pub status: String,
    /// Number of candidate files the filter chose from (the search-space size).
    pub total_candidates: usize,
    /// Repository-relative paths judged relevant, a subset of the candidate list.
    pub relevant_files: Vec<String>,
}

/// Runs the local relevance filter against the request's snapshot.
///
/// Never errors: an empty query or empty candidate set yields a graceful
/// `no_context` status; a filter that selects nothing yields `no_matches`. The BYOP
/// one-shot itself degrades to an empty selection on any provider error (see
/// `relevant_files::run_with_context`).
pub async fn run_get_relevant_files(
    args: GetRelevantFilesArgs,
    snapshot: &RelevantFilesSnapshot,
) -> GetRelevantFilesOutput {
    let query = args.query.clone();
    let total_candidates = snapshot.candidates.len();

    if query.trim().is_empty() || snapshot.candidates.is_empty() {
        return GetRelevantFilesOutput {
            query,
            status: "no_context".to_owned(),
            total_candidates,
            relevant_files: Vec::new(),
        };
    }

    let relevant =
        relevant_files::run_with_context(&snapshot.prepared, &query, &snapshot.candidates).await;

    let status = if relevant.is_empty() {
        "no_matches"
    } else {
        "ok"
    };
    GetRelevantFilesOutput {
        query,
        status: status.to_owned(),
        total_candidates,
        relevant_files: relevant,
    }
}

/// Serializes the output into the JSON the upstream model sees, stamping the
/// `_byop_intercepted` sentinel every BYOP local-intercept tool_result must carry.
pub fn output_to_json(out: &GetRelevantFilesOutput) -> Value {
    let mut v = serde_json::to_value(out).unwrap_or_else(|_| json!({"status": "serialize_error"}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("_byop_intercepted".to_owned(), Value::Bool(true));
    }
    v
}

/// Error result carrying the `_byop_intercepted` sentinel (mirrors
/// `codebase_runtime::error_to_json`).
pub fn error_to_json(e: &anyhow::Error) -> Value {
    json!({
        "_byop_intercepted": true,
        "status": "error",
        "tool": super::get_relevant_files::TOOL_NAME,
        "message": format!("{e:#}"),
    })
}

/// Builds the request-scoped snapshot from the active repository's local `RepoOutlines`
/// index plus the BYOP one-shot context.
///
/// Returns `None` (the tool then reports a graceful `no_context` status, or is filtered
/// out of the tools array) when any prerequisite is missing: no BYOP active-AI one-shot
/// is configured, no local repository is detected, its outline isn't `Complete` yet, or
/// the outline has no files. Pure local: no cloud, no proto, no server API.
///
/// The active-repo outline lookup is intentionally kept inline here rather than shared
/// with `codebase_runtime::collect_codebase_symbols` — the two request-snapshot builders
/// stay decoupled, and the outline handle borrows `app` for only the duration of the call.
#[cfg(not(target_family = "wasm"))]
pub fn collect_relevant_files_snapshot(
    app: &warpui::AppContext,
    terminal_view_id: Option<warpui::EntityId>,
) -> Option<RelevantFilesSnapshot> {
    use std::path::Path;

    use repo_metadata::repositories::DetectedRepositories;
    use warpui::SingletonEntity as _;

    use crate::ai::outline::{OutlineStatus, RepoOutlines};
    use crate::workspace::ActiveSession;

    // A BYOP active-AI one-shot must be configured, or there's nothing to run the
    // relevance filter with. Resolve it first (cheap) before touching the outline.
    let prepared = relevant_files::prepare_context(app, terminal_view_id)?;

    let git_repo_path = app
        .windows()
        .state()
        .active_window
        .and_then(|window_id| ActiveSession::as_ref(app).path_if_local(window_id))
        .and_then(|current_dir| {
            DetectedRepositories::as_ref(app).get_root_for_path(Path::new(current_dir))
        })?;

    let (outline_status, _) = RepoOutlines::as_ref(app).get_outline(&git_repo_path)?;
    let OutlineStatus::Complete(outline) = outline_status else {
        return None;
    };

    let candidates: Vec<relevant_files::FileEntry> = outline
        .to_file_symbols(None)
        .into_iter()
        .map(|file| relevant_files::FileEntry {
            path: file.path,
            symbols: file.symbols,
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    Some(RelevantFilesSnapshot {
        candidates,
        prepared,
    })
}

/// WASM has no local repository / outline index; the snapshot is always absent.
#[cfg(target_family = "wasm")]
pub fn collect_relevant_files_snapshot(
    _app: &warpui::AppContext,
    _terminal_view_id: Option<warpui::EntityId>,
) -> Option<RelevantFilesSnapshot> {
    None
}

#[cfg(test)]
#[path = "get_relevant_files_runtime_tests.rs"]
mod get_relevant_files_runtime_tests;
