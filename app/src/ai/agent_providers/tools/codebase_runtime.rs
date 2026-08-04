//! Local execution logic for the BYOP `search_codebase` tool.
//!
//! Mirrors `web_runtime`, but is a pure, local, CPU-only search: no network, no protobuf
//! executor, no server API, no team gating. `chat_stream`'s codebase interceptor calls
//! [`run_search_codebase`] directly against a `RepoOutlines`-derived symbol snapshot carried on
//! `RequestParams`.
//!
//! The snapshot is built once per request in `RequestParams::new` (where an `AppContext` is
//! available) by [`collect_codebase_symbols`], reusing the same outline → symbol projection and
//! fuzzy match the interactive code search (`ai_context_menu::code::data_source`) uses. The
//! `chat_stream` interceptor has no `AppContext`, so it can only search this pre-materialized
//! snapshot — hence the split.
//!
//! ## `_byop_intercepted` sentinel
//!
//! Like the web tools, every tool_result emitted here carries `"_byop_intercepted": true`; the
//! controller (`controller.rs:2693+`) consumes it to trigger auto-resume so the model isn't left
//! waiting for a result.

use fuzzy_match::match_indices_case_insensitive_ignore_spaces;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const DEFAULT_MAX_RESULTS: usize = 20;
pub const MAX_MAX_RESULTS: usize = 100;

/// A lightweight, request-scoped snapshot of one code symbol. Carried on `RequestParams` so the
/// `chat_stream` interceptor (which has no `AppContext`) can search it locally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodebaseSymbol {
    pub name: String,
    /// Language-specific type prefix, e.g. `fn ` in Rust. May be absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_prefix: Option<String>,
    /// Repository-relative path of the file the symbol lives in.
    pub file_path: String,
    /// 1-indexed line number of the symbol.
    pub line_number: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchCodebaseArgs {
    pub query: String,
    #[serde(default)]
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodebaseMatch {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_prefix: Option<String>,
    pub file_path: String,
    pub line_number: usize,
    pub score: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchCodebaseOutput {
    pub query: String,
    /// `"ok"` | `"no_index"` | `"no_matches"`.
    pub status: String,
    /// Total number of symbols in the request's snapshot (the search space size).
    pub total_symbols: usize,
    pub matches: Vec<CodebaseMatch>,
}

/// Fuzzy-matches a symbol name (with its language type prefix, when present) against `query`,
/// applying the same 3x symbol-score weighting the interactive code search uses. Returns the
/// score only on an actual match.
fn fuzzy_match_symbol(symbol: &CodebaseSymbol, query: &str) -> Option<i64> {
    let search_text = match &symbol.type_prefix {
        Some(prefix) => format!("{prefix}{}", symbol.name),
        None => symbol.name.clone(),
    };
    match_indices_case_insensitive_ignore_spaces(&search_text, query).map(|m| m.score * 3)
}

/// Runs a pure, local fuzzy search over the request's symbol snapshot.
///
/// Never errors:
/// - an empty snapshot yields a graceful `no_index` status (the outline is still building, the
///   `codebase_context_enabled` setting is off, or no repository was detected),
/// - a query that matches nothing yields `no_matches`.
pub fn run_search_codebase(
    args: SearchCodebaseArgs,
    symbols: &[CodebaseSymbol],
) -> SearchCodebaseOutput {
    let query = args.query.clone();
    let max_results = args
        .max_results
        .unwrap_or(DEFAULT_MAX_RESULTS)
        .clamp(1, MAX_MAX_RESULTS);
    let total_symbols = symbols.len();

    if symbols.is_empty() || query.trim().is_empty() {
        return SearchCodebaseOutput {
            query,
            status: "no_index".to_owned(),
            total_symbols,
            matches: Vec::new(),
        };
    }

    let mut scored: Vec<CodebaseMatch> = symbols
        .iter()
        .filter_map(|s| {
            fuzzy_match_symbol(s, &query).map(|score| CodebaseMatch {
                name: s.name.clone(),
                type_prefix: s.type_prefix.clone(),
                file_path: s.file_path.clone(),
                line_number: s.line_number,
                score,
            })
        })
        .collect();

    // Highest score first; ties broken deterministically by path then line for stable output.
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.line_number.cmp(&b.line_number))
    });
    scored.truncate(max_results);

    let status = if scored.is_empty() {
        "no_matches"
    } else {
        "ok"
    };
    SearchCodebaseOutput {
        query,
        status: status.to_owned(),
        total_symbols,
        matches: scored,
    }
}

/// Serializes a `SearchCodebaseOutput` into the JSON the upstream model sees, stamping the
/// `_byop_intercepted` sentinel every BYOP local-intercept tool_result must carry.
pub fn search_output_to_json(out: &SearchCodebaseOutput) -> Value {
    let mut v = serde_json::to_value(out).unwrap_or_else(|_| json!({"status": "serialize_error"}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("_byop_intercepted".to_owned(), Value::Bool(true));
    }
    v
}

/// Error result carrying the `_byop_intercepted` sentinel (mirrors `web_runtime::error_to_json`).
pub fn error_to_json(e: &anyhow::Error) -> Value {
    json!({
        "_byop_intercepted": true,
        "status": "error",
        "tool": crate::ai::agent_providers::tools::codebase::TOOL_NAME,
        "message": format!("{e:#}"),
    })
}

/// Builds the request-scoped symbol snapshot from the active repository's local
/// `RepoOutlines` index.
///
/// Reuses the exact outline → symbol projection of
/// `ai_context_menu::code::data_source::CodeSymbolCache::ensure_symbols_cached`. Returns an
/// empty vec when no repository is detected or its outline isn't `Complete` yet (the tool then
/// reports a graceful `no_index` status). Pure local: no cloud, no proto, no server API.
#[cfg(not(target_family = "wasm"))]
pub fn collect_codebase_symbols(app: &warpui::AppContext) -> Vec<CodebaseSymbol> {
    use std::path::Path;

    use repo_metadata::repositories::DetectedRepositories;
    use warpui::SingletonEntity as _;

    use crate::ai::outline::{OutlineStatus, RepoOutlines};
    use crate::workspace::ActiveSession;

    let Some(git_repo_path) = app
        .windows()
        .state()
        .active_window
        .and_then(|window_id| ActiveSession::as_ref(app).path_if_local(window_id))
        .and_then(|current_dir| {
            DetectedRepositories::as_ref(app).get_root_for_path(Path::new(current_dir))
        })
    else {
        return Vec::new();
    };

    let Some((outline_status, _)) = RepoOutlines::as_ref(app).get_outline(&git_repo_path) else {
        return Vec::new();
    };
    let OutlineStatus::Complete(outline) = outline_status else {
        return Vec::new();
    };

    outline
        .to_symbols_by_file(None)
        .into_iter()
        .flat_map(|(file_path, file_outline)| {
            let prefix = git_repo_path.clone();
            file_outline
                .symbols()
                .into_iter()
                .flatten()
                .map(move |symbol| CodebaseSymbol {
                    name: symbol.name.clone(),
                    type_prefix: symbol.type_prefix.clone(),
                    file_path: file_path
                        .strip_prefix(&prefix)
                        .unwrap_or(&file_path)
                        .to_string_lossy()
                        .into_owned(),
                    line_number: symbol.line_number,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// WASM has no local repository / outline index; the snapshot is always empty.
#[cfg(target_family = "wasm")]
pub fn collect_codebase_symbols(_app: &warpui::AppContext) -> Vec<CodebaseSymbol> {
    Vec::new()
}

#[cfg(test)]
#[path = "codebase_runtime_tests.rs"]
mod codebase_runtime_tests;
