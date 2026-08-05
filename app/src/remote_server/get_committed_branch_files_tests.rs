use super::super::proto::get_committed_branch_files_response;
use super::{error_response, files_result_to_response, success_response};

#[test]
fn success_response_maps_path_and_counts() {
    let response = success_response(vec![
        ("src/main.rs".to_string(), 10, 2),
        ("README.md".to_string(), 0, 5),
    ]);

    let Some(get_committed_branch_files_response::Result::Success(success)) = response.result
    else {
        panic!("expected success");
    };
    assert_eq!(success.files.len(), 2);
    assert_eq!(success.files[0].path, "src/main.rs");
    assert_eq!(success.files[0].additions, 10);
    assert_eq!(success.files[0].deletions, 2);
    assert_eq!(success.files[1].path, "README.md");
    assert_eq!(success.files[1].additions, 0);
    assert_eq!(success.files[1].deletions, 5);
}

#[test]
fn error_response_carries_message() {
    let response = error_response("boom".to_string());

    let Some(get_committed_branch_files_response::Result::Error(err)) = response.result else {
        panic!("expected error");
    };
    assert_eq!(err.message, "boom");
}

#[test]
fn files_result_to_response_maps_ok_and_err() {
    let ok = files_result_to_response(Ok(vec![("a.txt".to_string(), 1, 1)]));
    assert!(matches!(
        ok.result,
        Some(get_committed_branch_files_response::Result::Success(_))
    ));

    let err = files_result_to_response(Err(anyhow::anyhow!("nope")));
    let Some(get_committed_branch_files_response::Result::Error(e)) = err.result else {
        panic!("expected error");
    };
    assert_eq!(e.message, "nope");
}
