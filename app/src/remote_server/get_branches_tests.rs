use super::super::proto::get_branches_response;
use super::{branches_result_to_response, error_response, success_response};

#[test]
fn success_response_maps_name_and_is_main() {
    let response = success_response(vec![
        ("main".to_string(), true),
        ("feature/x".to_string(), false),
    ]);

    let Some(get_branches_response::Result::Success(success)) = response.result else {
        panic!("expected success");
    };
    assert_eq!(success.branches.len(), 2);
    assert_eq!(success.branches[0].name, "main");
    assert!(success.branches[0].is_main);
    assert_eq!(success.branches[1].name, "feature/x");
    assert!(!success.branches[1].is_main);
}

#[test]
fn error_response_carries_message() {
    let response = error_response("boom".to_string());

    let Some(get_branches_response::Result::Error(err)) = response.result else {
        panic!("expected error");
    };
    assert_eq!(err.message, "boom");
}

#[test]
fn branches_result_to_response_maps_ok_and_err() {
    let ok = branches_result_to_response(Ok(vec![("dev".to_string(), false)]));
    assert!(matches!(
        ok.result,
        Some(get_branches_response::Result::Success(_))
    ));

    let err = branches_result_to_response(Err(anyhow::anyhow!("nope")));
    let Some(get_branches_response::Result::Error(e)) = err.result else {
        panic!("expected error");
    };
    assert_eq!(e.message, "nope");
}
