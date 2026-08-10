//! Local execution logic for the BYOP `get_relevant_files` tool.
//!
//! ## Two mechanisms, two settings
//!
//! This tool is answered by two independent mechanisms, which are easy to confuse
//! because both have historically been called "codebase context":
//!
//! | mechanism | gated by | what it is |
//! |---|---|---|
//! | outline filter | `AIExecutionProfile::codebase_context_enabled` (per profile) | the `RepoOutlines` symbol index, filtered by a BYOP one-shot |
//! | embedding index | `code.indexing.agent_mode_codebase_context` (settings) | the vector index `CodebaseIndexManager` builds against the user's `/embeddings` endpoint |
//!
//! They share no code and neither reads the other's setting. `search_codebase` is
//! outline-only and is unaffected by the embedding index.
//!
//! [`run_get_relevant_files`] is the outline half alone;
//! [`run_get_relevant_files_merged`] is what the `chat_stream` interceptor calls, and
//! runs whichever mechanisms are available. Until the embedding index was wired up,
//! only the outline half existed and the index — which costs real money to build and
//! maintain — was never queried by anything.
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
    /// Which mechanisms produced `relevant_files`, in the order they contributed:
    /// `"codebase_index"` (the embedding index) and/or `"repo_outline"` (the
    /// outline-based one-shot filter). Empty when neither could run.
    ///
    /// Present so the two same-named codebase-context mechanisms are distinguishable
    /// in the tool result rather than silently indistinguishable — which is how the
    /// embedding index went unqueried for so long.
    #[serde(default)]
    pub sources: Vec<String>,
}

/// The name [`GetRelevantFilesOutput::sources`] uses for the embedding index.
pub const SOURCE_CODEBASE_INDEX: &str = "codebase_index";
/// The name [`GetRelevantFilesOutput::sources`] uses for the outline-based filter.
pub const SOURCE_REPO_OUTLINE: &str = "repo_outline";

/// Whether the `get_relevant_files` tool can be answered at all this turn.
///
/// The single source of truth for that question: `build_tools_array` and
/// `available_tool_names` use it to decide whether to advertise the tool, and the
/// `chat_stream` dispatcher uses it to decide whether to reject a call. Those three
/// must agree — a tool advertised but rejected, or rejected but reachable, is the same
/// class of bug as an index nothing queries.
///
/// The two arguments are the two independent settings:
/// * `codebase_context_enabled` — the per-profile `AIExecutionProfile` flag, which
///   gates the outline mechanism (and `search_codebase`, which this does not affect).
/// * `has_codebase_index` — whether an embedding index is available for this session,
///   which already implies `code.indexing.agent_mode_codebase_context` is on.
pub fn relevant_files_tool_available(
    codebase_context_enabled: bool,
    has_codebase_index: bool,
) -> bool {
    codebase_context_enabled || has_codebase_index
}

/// Rewrites absolute index paths as repository-relative ones, preserving rank order.
///
/// The index stores absolute paths; the outline half returns repository-relative ones,
/// and repository-relative is what the model is shown everywhere else. A path outside
/// `repo_root` (which should not happen) is passed through unchanged rather than
/// dropped, so a surprise is visible instead of silent.
pub(crate) fn relative_ranked_paths(
    paths: &[std::path::PathBuf],
    repo_root: &std::path::Path,
) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| {
            path.strip_prefix(repo_root)
                .unwrap_or(path.as_path())
                .to_str()
                .map(str::to_owned)
        })
        .collect()
}

/// Appends the outline filter's selection after the ranked embedding hits, dropping
/// duplicates.
///
/// Order matters and is the point: `ranked` arrives in relevance order from the
/// reranker, while the outline filter returns an unordered selection. A file both
/// mechanisms found keeps its embedding rank instead of being listed twice.
pub(crate) fn merge_preserving_rank(mut ranked: Vec<String>, outline: &[String]) -> Vec<String> {
    let already: std::collections::HashSet<&str> = ranked.iter().map(String::as_str).collect();
    let extra: Vec<String> = outline
        .iter()
        .filter(|path| !already.contains(path.as_str()))
        .cloned()
        .collect();
    ranked.extend(extra);
    ranked
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
            sources: Vec::new(),
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
        sources: vec![SOURCE_REPO_OUTLINE.to_owned()],
    }
}

/// Runs `get_relevant_files` over **both** codebase-context mechanisms and merges them.
///
/// This is the function the `chat_stream` interceptor calls; [`run_get_relevant_files`]
/// remains the outline-only half.
///
/// * `retrieval` is the codebase **embedding** index (semantic, ranked by the reranker,
///   gated on `code.indexing.agent_mode_codebase_context`).
/// * `snapshot` is the **outline** candidate list run through a BYOP one-shot filter
///   (gated on the per-profile `AIExecutionProfile::codebase_context_enabled`).
///
/// Either, both, or neither may be available; each is queried only if present. Results
/// are merged with the embedding hits first, because they carry a real relevance
/// ranking and the one-shot filter returns an unordered selection. Paths are
/// deduplicated, so a file both mechanisms find is listed once, at its embedding rank.
///
/// Never errors. When neither mechanism can answer, the status explains which state the
/// index is in (`no_index`, `index_syncing`, ...) rather than reporting a failure — in
/// a BYOP fork "no embedding provider configured" is the default, not a fault.
pub async fn run_get_relevant_files_merged(
    args: GetRelevantFilesArgs,
    snapshot: Option<&RelevantFilesSnapshot>,
    retrieval: Option<&crate::ai::codebase_retrieval::CodebaseRetrievalHandle>,
) -> GetRelevantFilesOutput {
    let query = args.query.clone();

    if query.trim().is_empty() {
        return GetRelevantFilesOutput {
            query,
            status: "no_context".to_owned(),
            total_candidates: snapshot.map_or(0, |s| s.candidates.len()),
            relevant_files: Vec::new(),
            sources: Vec::new(),
        };
    }

    // The embedding index first: it is the ranked source, and its failure modes are
    // the ones worth reporting when nothing else can answer either.
    let mut embedding_failure = None;
    let mut merged: Vec<String> = Vec::new();
    let mut sources: Vec<String> = Vec::new();

    if let Some(handle) = retrieval {
        match handle.retrieve(&query).await {
            Ok(paths) => {
                merged = relative_ranked_paths(&paths, handle.repo_root());
                sources.push(SOURCE_CODEBASE_INDEX.to_owned());
            }
            Err(failure) => {
                // Expected states, not faults: `debug` only. This runs on ordinary
                // agent queries now, so anything louder would be log spam for every
                // user who has not configured an embedding provider.
                log::debug!(
                    "codebase index answered no relevant files ({}): {failure:?}",
                    failure.status()
                );
                embedding_failure = Some(failure);
            }
        }
    }

    if let Some(snapshot) = snapshot {
        let outline = run_get_relevant_files(args, snapshot).await;
        merged = merge_preserving_rank(merged, &outline.relevant_files);
        if outline.status != "no_context" {
            sources.push(SOURCE_REPO_OUTLINE.to_owned());
        }
    }

    let status = if !merged.is_empty() {
        "ok".to_owned()
    } else if !sources.is_empty() {
        // Something ran and found nothing, which is an answer.
        "no_matches".to_owned()
    } else if let Some(failure) = embedding_failure {
        // Only the embedding index was available and it could not answer; say why.
        failure.status().to_owned()
    } else {
        "no_context".to_owned()
    };

    GetRelevantFilesOutput {
        query,
        status,
        total_candidates: snapshot.map_or(0, |s| s.candidates.len()),
        relevant_files: merged,
        sources,
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
