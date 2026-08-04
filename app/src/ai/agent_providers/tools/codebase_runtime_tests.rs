//! Unit tests for `codebase_runtime::run_search_codebase` (pure, local, no I/O).

use super::*;

fn sym(name: &str, type_prefix: Option<&str>, file: &str, line: usize) -> CodebaseSymbol {
    CodebaseSymbol {
        name: name.to_owned(),
        type_prefix: type_prefix.map(str::to_owned),
        file_path: file.to_owned(),
        line_number: line,
    }
}

fn args(query: &str, max_results: Option<usize>) -> SearchCodebaseArgs {
    SearchCodebaseArgs {
        query: query.to_owned(),
        max_results,
    }
}

fn sample() -> Vec<CodebaseSymbol> {
    vec![
        sym("run_search_codebase", Some("fn "), "src/tools/codebase_runtime.rs", 82),
        sym("collect_codebase_symbols", Some("fn "), "src/tools/codebase_runtime.rs", 160),
        sym("CodebaseSymbol", Some("struct "), "src/tools/codebase_runtime.rs", 29),
        sym("run_websearch", Some("fn "), "src/tools/web_runtime.rs", 649),
        sym("build_ssrf_safe_client", Some("fn "), "src/tools/web_runtime.rs", 227),
    ]
}

#[test]
fn empty_snapshot_reports_no_index() {
    let out = run_search_codebase(args("anything", None), &[]);
    assert_eq!(out.status, "no_index");
    assert_eq!(out.total_symbols, 0);
    assert!(out.matches.is_empty());
    assert_eq!(out.query, "anything");
}

#[test]
fn blank_query_reports_no_index() {
    let out = run_search_codebase(args("   ", None), &sample());
    assert_eq!(out.status, "no_index");
    assert!(out.matches.is_empty());
}

#[test]
fn matches_are_found_and_ranked() {
    let out = run_search_codebase(args("search_codebase", None), &sample());
    assert_eq!(out.status, "ok");
    assert_eq!(out.total_symbols, 5);
    assert!(!out.matches.is_empty());
    // The exact-substring symbol must rank first.
    assert_eq!(out.matches[0].name, "run_search_codebase");
    assert!(out.matches[0].score > 0);
    // Scores are sorted descending.
    for pair in out.matches.windows(2) {
        assert!(pair[0].score >= pair[1].score, "matches must be sorted by score desc");
    }
}

#[test]
fn no_matching_symbol_reports_no_matches() {
    let out = run_search_codebase(args("zzzqqqxxx_no_such_symbol", None), &sample());
    assert_eq!(out.status, "no_matches");
    assert!(out.matches.is_empty());
    // total_symbols still reflects the searched space.
    assert_eq!(out.total_symbols, 5);
}

#[test]
fn max_results_is_respected_and_clamped() {
    let out = run_search_codebase(args("run", Some(1)), &sample());
    assert_eq!(out.matches.len(), 1, "max_results must cap the result count");

    // 0 clamps up to 1 (never returns an unbounded / empty-by-clamp set).
    let out0 = run_search_codebase(args("run", Some(0)), &sample());
    assert_eq!(out0.matches.len(), 1);

    // Huge value clamps down to MAX_MAX_RESULTS, never panics.
    let out_big = run_search_codebase(args("run", Some(usize::MAX)), &sample());
    assert!(out_big.matches.len() <= MAX_MAX_RESULTS);
}

#[test]
fn type_prefix_participates_in_matching() {
    // Query against the "struct " prefix should still surface the struct symbol.
    let out = run_search_codebase(args("struct CodebaseSymbol", None), &sample());
    assert_eq!(out.status, "ok");
    assert!(out.matches.iter().any(|m| m.name == "CodebaseSymbol"));
}

/// The `_byop_intercepted` sentinel must be present in every result (same as the web tools),
/// so the controller triggers auto-resume — otherwise the model gets stuck waiting.
#[test]
fn search_output_carries_byop_sentinel() {
    let out = run_search_codebase(args("run", None), &sample());
    let v = search_output_to_json(&out);
    assert_eq!(v["_byop_intercepted"], true);
    assert_eq!(v["status"], "ok");
    assert!(v["matches"].is_array());
}

#[test]
fn error_json_carries_sentinel_and_tool_name() {
    let v = error_to_json(&anyhow::anyhow!("boom"));
    assert_eq!(v["_byop_intercepted"], true);
    assert_eq!(v["status"], "error");
    assert_eq!(v["tool"], "search_codebase");
    assert!(v["message"].as_str().unwrap().contains("boom"));
}

/// A default (no `max_results`) query defaults to `DEFAULT_MAX_RESULTS`.
#[test]
fn default_max_results_applies() {
    let mut symbols = Vec::new();
    for i in 0..50 {
        symbols.push(sym(&format!("run_thing_{i}"), Some("fn "), "src/a.rs", i + 1));
    }
    let out = run_search_codebase(args("run", None), &symbols);
    assert_eq!(out.matches.len(), DEFAULT_MAX_RESULTS);
}

/// `CodebaseSymbol` round-trips through serde (it is carried on `RequestParams`, which is
/// `Clone`/`Debug`; a stable serde shape keeps result JSON predictable for the model).
#[test]
fn codebase_symbol_serde_round_trip() {
    let s = sym("foo", Some("fn "), "src/x.rs", 3);
    let json = serde_json::to_string(&s).unwrap();
    let back: CodebaseSymbol = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);

    // Missing type_prefix deserializes to None (serde default).
    let no_prefix: CodebaseSymbol =
        serde_json::from_str(r#"{"name":"bar","file_path":"y.rs","line_number":1}"#).unwrap();
    assert_eq!(no_prefix.type_prefix, None);
}
