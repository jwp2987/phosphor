//! A Ctrl-C typed into a running agent command interrupts the command and the turn goes on
//! (#850; owner decision 2026-11-10, which closed the question left open on #1942), end to end
//! on the L1 rig.
//!
//! The rig has no pty, so the ^C reaches the command through `Rig::interrupt_commands` (what the
//! pty does with the byte the view writes: the command dies, the executor reports exit 130,
//! interrupted). Everything after that is the app: the action model moves on to the next action
//! of the reply, and the controller's end-of-step rule decides what the model is told.

use fake_llm::scenario::*;

use super::runner::run_case;
use crate::ai::agent::conversation::ConversationStatus;

fn hung_command_then(rest: Vec<Call>) -> Scenario {
    let mut scenario = Scenario::new(7);
    let mut calls = vec![Call::new(
        CallId::Fresh,
        "run_shell_command",
        Args::Command {
            cmd: "c0".to_owned(),
        },
    )];
    calls.extend(rest);
    scenario.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls { calls },
        chaos: None,
    });
    scenario.turns.push(Turn {
        when: Match::Default,
        reply: Reply::Text {
            text: "done".to_owned(),
        },
        chaos: None,
    });
    scenario
        .env
        .commands
        .insert("c0".to_owned(), CmdBehaviour::NeverEnds);
    // Approve the command, then Ctrl-C about 80 ms later, while it runs (a never-ending command
    // is reported as stalled only after 300 ms).
    scenario.actor.push(ActorStep {
        op: Op::Approve,
        timing: Timing::OnCard,
    });
    scenario.actor.push(ActorStep {
        op: Op::CtrlC,
        timing: Timing::Fast,
    });
    scenario
}

/// The reply asks for a never-ending command and then a question. The user approves the
/// command and presses Ctrl-C while it runs. The command stops; the question's card comes up
/// (I2c: it was not cancelled with the command) and is answered; both results go back to the
/// model in one request, and the turn ends with the model's answer.
///
/// A Ctrl-C that stopped the whole turn would fail I2c here ("also dropped ..."), and one that
/// was counted as a user stop would end `Cancelled` with a single request.
#[test]
fn a_ctrl_c_into_a_running_command_interrupts_it_and_the_turn_goes_on() {
    let scenario = hung_command_then(vec![Call::new(
        CallId::Fresh,
        "ask_user_question",
        Args::Valid,
    )]);
    let report = run_case(&scenario).unwrap_or_else(|failure| panic!("{failure}"));
    assert!(
        report.steps.iter().all(|step| step.performed),
        "the card appeared, the command was approved and the Ctrl-C was pressed: {:?}",
        report.steps
    );
    assert_eq!(
        report.requests, 2,
        "the request, then one follow-up with the step's results: the turn went on"
    );
    assert_eq!(
        report.tool_messages_in_last_request, 2,
        "the interrupted command's result and the question's answer, one tool message each"
    );
    assert_eq!(report.final_status, Some(ConversationStatus::Success));
}

/// The command is the only call of the reply. The Ctrl-C interrupts it and the invariants hold
/// (the command stops, I2c; the turn ends, I2a; no ERROR is logged, I6). How the step ends is
/// the ordinary end-of-step rule and is not pinned here: today a step whose only result is the
/// interrupted command ends `Cancelled` with the reason shown and sends nothing (#1178).
#[test]
fn a_ctrl_c_into_the_only_command_of_a_reply_holds_the_invariants() {
    let scenario = hung_command_then(Vec::new());
    let report = run_case(&scenario).unwrap_or_else(|failure| panic!("{failure}"));
    assert!(
        report.steps.iter().all(|step| step.performed),
        "{:?}",
        report.steps
    );
}
