//! #2029: the turn's reported state around an automatic follow-up, end to end on the L1 rig.
//!
//! When the model's tool call is rejected by the app itself (arguments that do not parse), the
//! app answers the call with an error result and asks the model again. That follow-up is
//! decided a moment after the reply's stream finished, and the conversation used to read
//! `Success` in between (1 to 4 ms in the app; `hop=turn ... state=success`, then
//! `in_progress`, then `success` again). Anything that acts on "the turn succeeded" could act
//! in that window, next to the automatic request.
//!
//! The states are read from `conversation::status_trail_for_test`: one entry per `hop=turn`
//! line the app would log, on this test's thread.

use fake_llm::scenario::*;

use super::runner::run_case;
use crate::ai::agent::conversation::{ConversationStatus, status_trail_for_test};

/// The reply is one `ask_user_question` whose arguments the app rejects (`questions: []`, the
/// shape of app case `agent_question_empty`); the model's answer to the error result is plain
/// text. Two requests, and the turn reads `success` exactly once: at its real end.
///
/// Red on the old code: the trail held `success` twice, the first between the two requests.
#[test]
fn a_rejected_tool_call_does_not_read_success_before_its_follow_up() {
    let mut scenario = Scenario::new(2029);
    scenario.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls {
            calls: vec![Call::new(
                CallId::Fresh,
                "ask_user_question",
                Args::AskQuestionsEmpty,
            )],
        },
        chaos: None,
    });
    scenario.turns.push(Turn {
        when: Match::Default,
        reply: Reply::Text {
            text: "done".to_owned(),
        },
        chaos: None,
    });

    let _ = status_trail_for_test::take();
    let report = run_case(&scenario).unwrap_or_else(|failure| panic!("{failure}"));
    let trail = status_trail_for_test::take();

    assert_eq!(
        report.requests, 2,
        "the request, then the automatic follow-up with the error result"
    );
    assert_eq!(report.final_status, Some(ConversationStatus::Success));
    let successes = trail.iter().filter(|state| **state == "success").count();
    assert_eq!(
        successes, 1,
        "the turn is reported finished once, not before its follow-up: {trail:?}"
    );
    assert_eq!(
        trail.last(),
        Some(&"success"),
        "and that is its last state: {trail:?}"
    );
}

/// A plain reply has no follow-up to wait for: it reads `success` at once, as before.
#[test]
fn a_plain_reply_still_reads_success_at_its_end() {
    let mut scenario = Scenario::new(2030);
    scenario.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::Text {
            text: "hello".to_owned(),
        },
        chaos: None,
    });

    let _ = status_trail_for_test::take();
    let report = run_case(&scenario).unwrap_or_else(|failure| panic!("{failure}"));
    let trail = status_trail_for_test::take();

    assert_eq!(report.requests, 1);
    assert_eq!(report.final_status, Some(ConversationStatus::Success));
    assert_eq!(trail.last(), Some(&"success"), "{trail:?}");
}
