//! The smallest end-to-end cases: the rig comes up, a plain reply ends the conversation, a tool
//! call that needs approval shows a card and finishes once approved. If these fail the rest of
//! the fuzzer is meaningless: look at them first (they are the smallest cases, so their failure
//! message is the easiest to read; nothing orders them before the others).

use fake_llm::scenario::*;

use super::runner::run_case;
use crate::ai::agent::conversation::ConversationStatus;

fn text_turn(n: u32, text: &str) -> Turn {
    Turn {
        when: Match::Nth { n },
        reply: Reply::Text {
            text: text.to_owned(),
        },
        chaos: None,
    }
}

#[test]
fn a_plain_reply_ends_the_conversation() {
    let mut scenario = Scenario::new(1);
    scenario.turns.push(text_turn(0, "hello there"));
    let report = run_case(&scenario).unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(report.final_status, Some(ConversationStatus::Success));
    assert_eq!(report.requests, 1, "one request, one reply");
    assert_eq!(report.tool_messages_in_last_request, 0);
}

#[test]
fn a_command_waits_for_approval_then_runs() {
    let mut scenario = Scenario::new(2);
    scenario.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls {
            calls: vec![Call::new(
                CallId::Fresh,
                "run_shell_command",
                Args::Command {
                    cmd: "c0".to_owned(),
                },
            )],
        },
        chaos: None,
    });
    scenario.turns.push(text_turn(1, "done"));
    scenario.env.commands.insert(
        "c0".to_owned(),
        CmdBehaviour::Quick {
            out: "ok".to_owned(),
        },
    );
    scenario.actor.push(ActorStep {
        op: Op::Approve,
        timing: Timing::OnCard,
    });
    let report = run_case(&scenario).unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(report.final_status, Some(ConversationStatus::Success));
    assert!(
        report.steps[0].performed,
        "the card appeared and the user acted on it"
    );
    assert_eq!(report.steps[0].tool, Some("run_shell_command"));
    assert_eq!(
        report.requests, 2,
        "the request, then the follow-up carrying the result"
    );
    assert_eq!(
        report.tool_messages_in_last_request, 1,
        "exactly one tool message for the one call"
    );
}
