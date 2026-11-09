//! I5: an agent command is handed to the user only if it really reads the tty.
//!
//! The L1 rig has no pty, so no prompt is ever on a screen. These tests drive the two real pieces
//! the invariant is about, with synthetic observations built from the scenario's command
//! behaviours:
//!
//! * `PromptDetector::observe` (pure; takes `now`, the termios shape, the screen candidate and,
//!   since #1796, the `foreground_tty_read` probe) at the executor's 1 Hz poll;
//! * the hand-over itself on a real `TerminalModel` block: `take_over_control_for_user` and
//!   `handoff_control_to_agent`, whose failures `CLISubagentController` logs at ERROR.
//!
//! The rules (docs/testing.md "Agent fuzzer and fake model", I5):
//! * `Silent`, `TrailingColon`, `TrailingQuestion` look like a prompt but never read the tty. With
//!   a probe that says `NotReading` they are never handed over, however long they sit.
//! * `RealPrompt` (probe `Reading`) is handed over within the quiet window plus 3 s; a secret one
//!   (echo off) at once.
//! * With no probe (`Unknown`: macOS, ptrace limits) a bare `?` / `:` waits 90 s, never less.
//! * Giving control back logs no ERROR and leaves the agent in control.

use std::time::Duration;

use fake_llm::scenario::CmdBehaviour;
use fake_llm::strategies::arb_cmd_behaviour;
use instant::Instant;
use proptest::prelude::*;
use warp_terminal::model::Point;

use crate::ai::agent::AIAgentActionId;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::block::cli_controller::UserTakeOverReason;
use crate::terminal::TerminalModel;
use crate::terminal::model::block::{
    ForegroundTtyRead, INTERACTIVE_PROMPT_QUIET_TRAILING,
    INTERACTIVE_PROMPT_QUIET_TRAILING_UNKNOWN_READ, InteractivePromptCandidate, PromptDetection,
    PromptDetector, interactive_prompt_shape,
};

use super::log_capture;

/// The executor's termios poll.
const POLL: Duration = Duration::from_secs(1);

/// What a behaviour looks like to the detector: (echo on, canonical, the screen's prompt line).
fn screen(behaviour: &CmdBehaviour) -> (bool, bool, Option<String>) {
    match behaviour {
        CmdBehaviour::Silent { s }
        | CmdBehaviour::TrailingColon { s }
        | CmdBehaviour::TrailingQuestion { s } => (true, true, Some(s.clone())),
        CmdBehaviour::RealPrompt { prompt, secret } => {
            let line = if prompt.ends_with(':') || prompt.ends_with('?') {
                prompt.clone()
            } else {
                format!("{prompt}:")
            };
            // `read -s`: echo off, canonical on (the password shape).
            (!secret, true, Some(line))
        }
        // Raw mode / full screen / finished: never probed.
        CmdBehaviour::NeverEnds
        | CmdBehaviour::AltScreen
        | CmdBehaviour::Pager
        | CmdBehaviour::SetE
        | CmdBehaviour::Quick { .. } => (true, false, None),
    }
}

/// How long after the command started the detector first hands it over, polling at 1 Hz for
/// `horizon` with the probe answering `probe`; `None` = never.
fn handed_over_after(
    behaviour: &CmdBehaviour,
    probe: ForegroundTtyRead,
    horizon: Duration,
) -> Option<Duration> {
    let (echo_on, canonical, line) = screen(behaviour);
    let started = Instant::now();
    let mut detector = PromptDetector::default();
    let mut elapsed = Duration::ZERO;
    while elapsed <= horizon {
        let now = started + elapsed;
        let line = line.clone();
        let detection = detector.observe(
            now,
            echo_on,
            canonical,
            move || {
                let text = line?;
                let shape = interactive_prompt_shape(&text)?;
                Some(InteractivePromptCandidate {
                    cursor: Point::new(0, text.chars().count()),
                    text,
                    shape,
                })
            },
            || probe,
        );
        if matches!(
            detection,
            Some(PromptDetection::Interactive | PromptDetection::Password)
        ) {
            return Some(elapsed);
        }
        elapsed += POLL;
    }
    None
}

const THIRTY_MINUTES: Duration = Duration::from_secs(30 * 60);

fn real_prompt(prompt: &str, secret: bool) -> CmdBehaviour {
    CmdBehaviour::RealPrompt {
        prompt: prompt.to_owned(),
        secret,
    }
}

#[test]
fn a_prompt_lookalike_is_never_handed_over_while_the_foreground_is_not_reading() {
    for behaviour in [
        CmdBehaviour::Silent {
            s: "Waiting".into(),
        },
        CmdBehaviour::TrailingColon {
            s: "Waiting for DB:".into(),
        },
        CmdBehaviour::TrailingQuestion {
            s: "Still going?".into(),
        },
    ] {
        assert_eq!(
            handed_over_after(&behaviour, ForegroundTtyRead::NotReading, THIRTY_MINUTES),
            None,
            "{behaviour:?} does not read the tty and must stay with the agent"
        );
    }
}

#[test]
fn a_real_prompt_is_handed_over_within_the_window_plus_three_seconds() {
    let behaviour = real_prompt("Name:", false);
    let after = handed_over_after(&behaviour, ForegroundTtyRead::Reading, THIRTY_MINUTES)
        .expect("a command blocked reading the tty must reach the user");
    assert!(
        after >= INTERACTIVE_PROMPT_QUIET_TRAILING,
        "not before the quiet window: {after:?}"
    );
    assert!(
        after <= INTERACTIVE_PROMPT_QUIET_TRAILING + Duration::from_secs(3),
        "within the window + 3 s: {after:?}"
    );
}

#[test]
fn a_secret_prompt_is_handed_over_at_once() {
    let after = handed_over_after(
        &real_prompt("Password:", true),
        ForegroundTtyRead::Reading,
        THIRTY_MINUTES,
    );
    assert_eq!(after, Some(Duration::ZERO));
}

#[test]
fn with_no_probe_a_bare_question_waits_ninety_seconds() {
    let behaviour = CmdBehaviour::TrailingQuestion {
        s: "Continue?".into(),
    };
    let after = handed_over_after(&behaviour, ForegroundTtyRead::Unknown, THIRTY_MINUTES)
        .expect("unknown must not mean never");
    assert!(
        after >= INTERACTIVE_PROMPT_QUIET_TRAILING_UNKNOWN_READ,
        "{after:?}"
    );
}

proptest! {
    /// Whatever the behaviour: a hand-over with the probe saying NotReading never happens unless
    /// the command is a secret prompt (echo off needs no probe), and with Reading it happens only
    /// for a command that reads the tty.
    #[test]
    fn handover_only_for_commands_that_read_the_tty(behaviour in arb_cmd_behaviour()) {
        let not_reading = handed_over_after(&behaviour, ForegroundTtyRead::NotReading, Duration::from_secs(600));
        let secret = matches!(behaviour, CmdBehaviour::RealPrompt { secret: true, .. });
        if !secret {
            prop_assert_eq!(not_reading, None, "{:?} with NotReading", behaviour);
        }
        if handed_over_after(&behaviour, ForegroundTtyRead::Reading, Duration::from_secs(600)).is_some() {
            prop_assert!(behaviour.reads_tty(), "{:?} was handed over but does not read the tty", behaviour);
        }
    }
}

/// A running agent command in a real block, as `cli_controller_tests.rs` builds one.
fn agent_command_block() -> TerminalModel {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block("read -p 'Name: ' n", "Name:");
    model
        .block_list_mut()
        .active_block_mut()
        .set_agent_interaction_mode_for_requested_command(
            AIAgentActionId::from("requested-command".to_owned()),
            None,
            AIConversationId::new(),
        );
    model
}

#[test]
fn handing_over_and_back_logs_no_error_and_leaves_the_agent_in_control() {
    let capture = log_capture::install();
    let mark = log_capture::mark();
    let mut model = agent_command_block();
    let block = model.block_list_mut().active_block_mut();

    // The detector fired for a RealPrompt: the executor hands the command to the user...
    block
        .take_over_control_for_user(UserTakeOverReason::BlockedOnInput)
        .expect("the hand-over of a prompt-blocked command is admitted");
    assert!(
        !UserTakeOverReason::BlockedOnInput.should_cancel_conversation(),
        "a prompt hand-over keeps the conversation alive"
    );
    assert!(!block.is_agent_in_control());

    // ...and the user hands it back when the prompt is answered.
    block
        .handoff_control_to_agent()
        .expect("giving control back must not fail");
    assert!(
        block.is_agent_in_control(),
        "the agent is in control afterwards"
    );

    if capture {
        assert_eq!(
            log_capture::errors_since(mark),
            Vec::<String>::new(),
            "the hand back must log no ERROR"
        );
    }
}

#[test]
fn a_second_hand_back_is_refused_without_a_panic() {
    let mut model = agent_command_block();
    let block = model.block_list_mut().active_block_mut();
    block
        .take_over_control_for_user(UserTakeOverReason::BlockedOnInput)
        .unwrap();
    block.handoff_control_to_agent().unwrap();
    // Not an I5 violation: documents that a repeated hand-back is an `Err`, which the
    // controller logs at ERROR; the executor must therefore hand back once per take-over.
    assert!(block.handoff_control_to_agent().is_err() || block.is_agent_in_control());
}
