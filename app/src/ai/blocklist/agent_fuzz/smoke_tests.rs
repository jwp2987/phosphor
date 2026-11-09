//! The smallest end-to-end cases: the rig comes up, a plain reply ends the conversation, a tool
//! call that needs approval shows a card and finishes once approved. If these fail the rest of
//! the fuzzer is meaningless, so they run first (alphabetically `smoke_tests`).

use fake_llm::scenario::*;

use super::runner::run_case;

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
    if let Err(failure) = run_case(&scenario) {
        panic!("{failure}");
    }
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
    if let Err(failure) = run_case(&scenario) {
        panic!("{failure}");
    }
}
