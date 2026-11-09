//! The safe read-only allowlist under the fuzzer (it ships OFF; the owner enables it after the
//! suite's first green run with it OFF).
//!
//! The allowlist must only ever REMOVE approvals; it must never hide a stuck state. So:
//!
//! * `allowlist_removes_the_approval_for_a_listed_line_and_only_that` pins the effect: with it ON
//!   `echo hi` runs with no card, a `fake-cmd` line (not listed) still asks, with it OFF both ask.
//! * `allowlist_on_and_off_reach_the_same_terminal_state` (proptest, differential): the same
//!   scenario with a user who only approves / answers ends in the same status, the same number of
//!   requests and the same tool messages whether the allowlist is ON or OFF. The only permitted
//!   difference is the skipped approvals themselves.
//! * the ON runs of every sweep (`matrix`, corpus replay, the paced proptest) check I1-I7 with it ON.

use fake_llm::scenario::*;
use proptest::prelude::*;
use proptest::test_runner::{
    Config, FailurePersistence, RngAlgorithm, TestError, TestRng, TestRunner,
};

use super::runner::{CaseReport, run_case_with};
use crate::ai::agent::conversation::ConversationStatus;

fn one_command(args: Args, key: &str, behaviour: CmdBehaviour) -> Scenario {
    let mut s = Scenario::new(77);
    s.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls {
            calls: vec![Call::new(CallId::Fresh, "run_shell_command", args)],
        },
        chaos: None,
    });
    s.turns.push(Turn {
        when: Match::Nth { n: 1 },
        reply: Reply::Text {
            text: "done".to_owned(),
        },
        chaos: None,
    });
    s.env.commands.insert(key.to_owned(), behaviour);
    s.actor.push(ActorStep {
        op: Op::Approve,
        timing: Timing::OnCard,
    });
    s
}

fn quick() -> CmdBehaviour {
    CmdBehaviour::Quick {
        out: "ok".to_owned(),
    }
}

fn run(scenario: &Scenario, allowlist: bool) -> CaseReport {
    run_case_with(scenario, allowlist).unwrap_or_else(|failure| panic!("{failure}"))
}

#[test]
fn allowlist_removes_the_approval_for_a_listed_line_and_only_that() {
    // `echo hi` is listed when the allowlist is ON.
    let listed = one_command(Args::Valid, "unused", quick());
    let off = run(&listed, false);
    assert!(
        off.steps[0].performed,
        "OFF: the card appears and is approved"
    );
    assert_eq!(off.steps[0].tool, Some("run_shell_command"));
    let on = run(&listed, true);
    assert!(
        !on.steps[0].performed,
        "ON: a listed read-only line runs with no approval card"
    );
    for report in [&off, &on] {
        assert_eq!(report.final_status, Some(ConversationStatus::Success));
        assert_eq!(report.requests, 2);
        assert_eq!(report.tool_messages_in_last_request, 1);
    }

    // A `fake-cmd` line is not on the list: it asks ON as well as OFF.
    let unlisted = one_command(
        Args::Command {
            cmd: "c0".to_owned(),
        },
        "c0",
        quick(),
    );
    for allowlist in [false, true] {
        let report = run(&unlisted, allowlist);
        assert!(
            report.steps[0].performed,
            "allowlist {allowlist}: an unlisted command must still ask"
        );
        assert_eq!(report.final_status, Some(ConversationStatus::Success));
    }
}

/// The user in these scenarios only approves / answers: the ops whose outcome does not depend on
/// whether a card was there to act on.
fn approving_user() -> impl Strategy<Value = Vec<ActorStep>> {
    prop::collection::vec(
        prop_oneof![
            3 => Just(Op::Approve),
            1 => (0usize..3).prop_map(|choice| Op::AnswerQuestion { choice }),
            1 => Just(Op::SkipQuestion),
            1 => Just(Op::Wait { ms: 5 }),
        ]
        .prop_map(|op| ActorStep {
            op,
            timing: Timing::OnCard,
        }),
        0..5,
    )
}

fn differential_scenarios() -> impl Strategy<Value = Scenario> {
    // No chaos: retries add timing, not state, and the differential is about states.
    (fake_llm::strategies::arb_scenario(), approving_user()).prop_map(|(mut scenario, actor)| {
        for turn in &mut scenario.turns {
            turn.chaos = None;
        }
        scenario.actor = actor;
        scenario
    })
}

fn differential(scenario: Scenario) -> Result<(), TestCaseError> {
    let off = run_case_with(&scenario, false).map_err(|f| TestCaseError::fail(f.to_string()))?;
    let on = run_case_with(&scenario, true).map_err(|f| TestCaseError::fail(f.to_string()))?;
    let same = off.final_status == on.final_status
        && off.requests == on.requests
        && off.tool_messages_in_last_request == on.tool_messages_in_last_request;
    if same {
        Ok(())
    } else {
        Err(TestCaseError::fail(format!(
            "the allowlist changed where the conversation ends (only skipped approvals may differ)\n\
             OFF: status {:?}, {} requests, {} tool messages\n\
             ON:  status {:?}, {} requests, {} tool messages\nscenario:\n{}",
            off.final_status,
            off.requests,
            off.tool_messages_in_last_request,
            on.final_status,
            on.requests,
            on.tool_messages_in_last_request,
            scenario.to_json()
        )))
    }
}

#[test]
fn allowlist_on_and_off_reach_the_same_terminal_state() {
    let mut config = Config::default();
    config.cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12);
    config.max_shrink_iters = 128;
    config.max_shrink_time = 120_000;
    config.failure_persistence = None::<Box<dyn FailurePersistence>>;
    let mut runner = TestRunner::new_with_rng(
        config,
        TestRng::from_seed(RngAlgorithm::ChaCha, &[0x3c; 32]),
    );
    if let Err(error) = runner.run(&differential_scenarios(), differential) {
        match error {
            TestError::Fail(reason, scenario) => {
                panic!("{reason}\n\nshrunk scenario:\n{}", scenario.to_json())
            }
            TestError::Abort(reason) => panic!("aborted: {reason}"),
        }
    }
}
