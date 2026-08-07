use super::*;

// The pinned oracle (`02b53fcd8`) has 2 tests here. The second,
// `read_files_partial_success_converts_failed_files`, is blocked: it builds a
// `ReadFilesResult::Success { files, failed_files }`, and this fork's variant is
// `Success { files }` only. The `failed_reads` field *does* exist on the wire
// (`convert.rs` sets it to an empty vec), so the gap is the client-side result
// type, not the proto. Tracked as #136; the sanctioned divergence is documented
// on `describe_failed_files` in
// `app/src/ai/blocklist/action_model/execute/read_files.rs`.

#[test]
fn ask_user_question_skipped_by_auto_approve_converts_to_skipped_answers() {
    let result = api::request::input::tool_call_result::Result::from(
        AskUserQuestionResult::SkippedByAutoApprove {
            question_ids: vec!["q1".to_string(), "q2".to_string()],
        },
    );

    let api::request::input::tool_call_result::Result::AskUserQuestion(result) = result else {
        panic!("expected ask_user_question result");
    };

    let Some(api::ask_user_question_result::Result::Success(success)) = result.result else {
        panic!("expected success result");
    };

    assert_eq!(success.answers.len(), 2);
    assert_eq!(success.answers[0].question_id, "q1");
    assert_eq!(success.answers[1].question_id, "q2");
    assert!(matches!(
        success.answers[0].answer,
        Some(AskUserQuestionAnswer::Skipped(()))
    ));
    assert!(matches!(
        success.answers[1].answer,
        Some(AskUserQuestionAnswer::Skipped(()))
    ));
}
