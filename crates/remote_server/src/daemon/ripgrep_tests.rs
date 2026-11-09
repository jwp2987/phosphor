use crate::proto::{RipgrepSearchRequest, ripgrep_search_response};

use super::*;

fn request(pattern: &str, root: &Path) -> RipgrepSearchRequest {
    RipgrepSearchRequest {
        pattern: pattern.to_string(),
        roots: vec![root.to_string_lossy().into_owned()],
        ignore_case: false,
        multiline: false,
        max_matches: 0,
    }
}

#[test]
fn validation_rejects_empty_and_relative_requests() {
    assert!(validate_request(RipgrepSearchRequest::default()).is_err());
    let mut relative = request("x", Path::new("/abs"));
    relative.roots = vec!["rel/path".to_string()];
    assert!(validate_request(relative).unwrap_err().contains("absolute"));
}

#[test]
fn match_cap_is_clamped_to_the_server_maximum() {
    let mut req = request("x", Path::new("/abs"));
    req.max_matches = 1_000_000;
    assert_eq!(
        validate_request(req).unwrap().match_cap,
        MAX_RIPGREP_SEARCH_MATCH_CAP
    );
    let mut req = request("x", Path::new("/abs"));
    req.max_matches = 7;
    assert_eq!(validate_request(req).unwrap().match_cap, 7);
}

#[test]
fn finds_lines_with_numbers_and_submatch_offsets() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "fn main() {}\nlet needle = 1;\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "nothing here\n").unwrap();
    let params = validate_request(request("needle", dir.path())).unwrap();
    let success = run_search(params, &Cancel::default()).unwrap();
    assert!(!success.capped);
    assert_eq!(success.matches.len(), 1);
    let m = &success.matches[0];
    assert!(m.file_path.ends_with("a.rs"));
    assert_eq!(m.line_number, 2);
    assert_eq!(m.line_text, "let needle = 1;\n");
    assert_eq!(m.submatches.len(), 1);
    assert_eq!(m.submatches[0].byte_start, 4);
    assert_eq!(m.submatches[0].byte_end, 10);
}

#[test]
fn a_late_submatch_in_a_long_line_keeps_the_full_line() {
    let dir = tempfile::tempdir().unwrap();
    let line = format!("{}needle\n", "x".repeat(8_000));
    std::fs::write(dir.path().join("long.rs"), &line).unwrap();
    let params = validate_request(request("needle", dir.path())).unwrap();
    let success = run_search(params, &Cancel::default()).unwrap();
    assert_eq!(success.matches[0].line_text, line);
    assert_eq!(success.matches[0].submatches[0].byte_start, 8_000);
    assert_eq!(success.matches[0].submatches[0].byte_end, 8_006);
}

#[test]
fn case_insensitive_search_matches_other_case() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "NeEdLe\n").unwrap();
    let mut req = request("needle", dir.path());
    req.ignore_case = true;
    let success = run_search(validate_request(req).unwrap(), &Cancel::default()).unwrap();
    assert_eq!(success.matches.len(), 1);
}

#[test]
fn results_beyond_the_cap_set_capped() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "hit\nhit\nhit\nhit\n").unwrap();
    let mut req = request("hit", dir.path());
    req.max_matches = 2;
    let success = run_search(validate_request(req).unwrap(), &Cancel::default()).unwrap();
    assert_eq!(success.matches.len(), 2);
    assert!(success.capped);
}

#[test]
fn a_bad_pattern_is_an_error_not_an_empty_result() {
    let dir = tempfile::tempdir().unwrap();
    let params = validate_request(request("(unclosed", dir.path())).unwrap();
    let response = search_result_to_response(run_search(params, &Cancel::default()));
    assert!(matches!(
        response.result,
        Some(ripgrep_search_response::Result::Error(_))
    ));
}

#[test]
fn a_cancelled_search_returns_without_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "hit\n").unwrap();
    let cancel = Cancel::default();
    cancel.cancel();
    let success = run_search(
        validate_request(request("hit", dir.path())).unwrap(),
        &cancel,
    )
    .unwrap();
    assert!(success.matches.is_empty());
}
