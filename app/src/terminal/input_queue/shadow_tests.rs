//! Tests of the shadow comparison (#1424, step 3): a constructed legacy/table mismatch is reported
//! with the right row, classes, flags and tag; agreement reports nothing; the log line never
//! carries typed text.

use super::*;
use crate::terminal::input_queue::facts::{DestinationMode, set_destination_mode_for_test};
use crate::terminal::input_queue::table::decide;
use crate::terminal::input_queue::{Origin, SshPhase, Stamp};

fn kb(class: Class) -> Intent {
    Intent::new(class, Origin::Keyboard)
}

/// The shell is at its prompt: the pty does not take typed text, the editor does.
fn at_prompt() -> RouteFacts {
    RouteFacts {
        model_consistent: true,
        handoff_known: true,
        session_pending_or_bootstrapped: true,
        is_bootstrapped: true,
        input_box_visible: true,
        ..RouteFacts::default()
    }
}

/// A command is running: the pty takes everything.
fn running() -> RouteFacts {
    RouteFacts {
        model_consistent: true,
        handoff_known: true,
        long_running: true,
        active_block_started: true,
        session_pending_or_bootstrapped: true,
        is_bootstrapped: true,
        ..RouteFacts::default()
    }
}

#[test]
fn agreement_reports_nothing() {
    let _ = take_recorded();
    let intent = kb(Class::Typed);
    let facts = at_prompt();
    let plan = decide(&intent, &facts, None);
    assert_eq!(class_of(&plan.outcome), DestClass::Editor);
    assert_eq!(check(&intent, &plan, &facts, DestClass::Editor), None);
    assert!(
        take_recorded().is_empty(),
        "nothing is logged when they agree"
    );
}

#[test]
fn a_mismatch_reports_row_both_classes_and_flags() {
    let _ = take_recorded();
    let intent = kb(Class::Typed);
    let facts = at_prompt();
    let plan = decide(&intent, &facts, None);
    // Legacy put the text on the pty while the table says the editor (row D7).
    let disagreement = check(&intent, &plan, &facts, DestClass::Pty).expect("they differ");
    assert_eq!(disagreement.row, Row::D7);
    assert_eq!(disagreement.legacy, DestClass::Pty);
    assert_eq!(disagreement.table, DestClass::Editor);
    assert_eq!(disagreement.expected, None);
    let line = disagreement.detail();
    assert!(
        line.starts_with("row=D7 legacy=pty table=editor flags=ssh=idle long=0 started=0"),
        "{line}"
    );
    assert!(
        line.contains("pty_typed=0") && line.contains("box=1"),
        "{line}"
    );
    assert!(!line.contains("expected="), "{line}");
    assert_eq!(take_recorded(), vec![line]);
}

#[test]
fn a_mismatch_with_a_different_class_of_input_names_that_row() {
    let _ = take_recorded();
    // A signal with nothing running has nowhere to go (D10); legacy wrote it to the pty.
    let intent = kb(Class::Signal(SignalKind::Interrupt));
    let facts = at_prompt();
    let plan = decide(&intent, &facts, None);
    let disagreement = check(&intent, &plan, &facts, DestClass::Pty).expect("they differ");
    assert_eq!(
        (disagreement.row, disagreement.table, disagreement.expected),
        (Row::D10, DestClass::Dropped, None)
    );
}

#[test]
fn suspend_and_quit_differences_are_tagged_od1() {
    for signal in [SignalKind::Suspend, SignalKind::Quit] {
        let intent = kb(Class::Signal(signal));
        let facts = at_prompt();
        let plan = decide(&intent, &facts, None);
        let disagreement = compare(&intent, &plan, &facts, DestClass::Pty).expect("they differ");
        assert_eq!(disagreement.expected, Some("OD1"), "{signal:?}");
        assert!(disagreement.detail().ends_with(" expected=OD1"));
    }
}

#[test]
fn a_signal_held_in_the_ssh_setup_window_is_tagged_od5() {
    let intent = kb(Class::Signal(SignalKind::Interrupt));
    let facts = RouteFacts {
        ssh_interrupt_must_wait: true,
        ..running()
    };
    let plan = decide(&intent, &facts, None);
    assert_eq!(plan.row, Row::P1a);
    // Legacy dropped it (with a toast saying it was held).
    let disagreement = compare(&intent, &plan, &facts, DestClass::Dropped).expect("they differ");
    assert_eq!(
        (disagreement.table, disagreement.expected),
        (DestClass::Held, Some("OD5"))
    );
}

#[test]
fn a_key_stamped_before_the_ssh_prompt_is_tagged_od7() {
    let intent = kb(Class::Typed).stamped(Stamp {
        facts_gen: 3,
        ..Stamp::UNSTAMPED
    });
    let facts = RouteFacts {
        ssh: SshPhase::PromptUp { prompt_gen: 5 },
        ..running()
    };
    let plan = decide(&intent, &facts, None);
    assert_eq!(plan.row, Row::H4);
    // Legacy delivered it to the prompt.
    let disagreement = compare(&intent, &plan, &facts, DestClass::Pty).expect("they differ");
    assert_eq!(
        (disagreement.table, disagreement.expected),
        (DestClass::Dropped, Some("OD7"))
    );
    assert!(disagreement.flags.starts_with("ssh=prompt"));
}

#[test]
fn a_silent_drop_of_hidden_input_is_tagged_b3() {
    // The agent still owns the command, the box is hidden: the table drops with a reason where
    // legacy lost the text without one.
    let intent = kb(Class::Typed);
    let facts = RouteFacts {
        agent_in_control: true,
        agent_in_control_exact: true,
        input_box_visible: false,
        ..running()
    };
    let plan = decide(&intent, &facts, None);
    assert_eq!(plan.outcome, Outcome::Drop(DropWhy::RefusedHiddenInput));
    let disagreement = compare(&intent, &plan, &facts, DestClass::Editor).expect("they differ");
    assert_eq!(disagreement.expected, Some("B3"));
}

#[test]
fn the_log_line_has_no_content_of_the_input() {
    // The comparison never sees the text: an intent has a class and an origin, nothing else.
    let intent = kb(Class::Typed);
    let facts = at_prompt();
    let plan = decide(&intent, &facts, None);
    let line = compare(&intent, &plan, &facts, DestClass::Held)
        .expect("they differ")
        .detail();
    assert!(line.is_ascii());
    assert_eq!(
        line.split_whitespace()
            .map(|word| word.split('=').next().unwrap_or(""))
            .collect::<Vec<_>>(),
        vec![
            "row",
            "legacy",
            "table",
            "flags",
            "long",
            "started",
            "agent_owns",
            "agent_ctl",
            "handoff",
            "box",
            "pty_typed",
            "gates",
            "model_ok"
        ]
    );
}

#[test]
fn every_outcome_has_a_class() {
    use crate::terminal::input_queue::GateKind;
    use crate::terminal::input_queue::table::{PtyWrite, SwallowReason};
    let cases = [
        (Outcome::Pty(PtyWrite::Typed), DestClass::Pty),
        (Outcome::PasteToPty { bracketed: true }, DestClass::Pty),
        (Outcome::SignalToPty(SignalKind::Quit), DestClass::Pty),
        (Outcome::SigintForeground, DestClass::Pty),
        (Outcome::AbortBytes, DestClass::Pty),
        (Outcome::PowershellAbort, DestClass::Pty),
        (Outcome::SignalAtGates, DestClass::Pty),
        (Outcome::EditorInsert, DestClass::Editor),
        (Outcome::EditorAction, DestClass::Editor),
        (Outcome::EditorSubmit, DestClass::Editor),
        (Outcome::AgentInput, DestClass::Editor),
        (Outcome::Hold(GateKind::SshConnect), DestClass::Held),
        (Outcome::HoldSignal, DestClass::Held),
        (Outcome::Queue, DestClass::Held),
        (Outcome::Stash, DestClass::Held),
        (
            Outcome::Swallow(SwallowReason::SshDialogOpen),
            DestClass::Swallowed,
        ),
        (Outcome::Drop(DropWhy::AgentControl), DestClass::Dropped),
        (Outcome::Hint, DestClass::Hint),
    ];
    for (outcome, class) in cases {
        assert_eq!(class_of(&outcome), class, "{outcome:?}");
    }
}

#[test]
fn the_leaf_is_the_first_one_noted_and_is_taken_once() {
    begin_leaf();
    assert_eq!(take_leaf(), None);
    note_leaf(DestClass::Held);
    note_leaf(DestClass::Pty);
    assert_eq!(
        take_leaf(),
        Some(DestClass::Held),
        "a nested write does not overwrite it"
    );
    assert_eq!(take_leaf(), None, "taken once");
    note_leaf(DestClass::Editor);
    begin_leaf();
    assert_eq!(take_leaf(), None, "a new entry point forgets the old leaf");
}

#[test]
fn the_comparison_runs_only_in_shadow_mode_when_forced_by_a_test() {
    {
        let _mode = set_destination_mode_for_test(DestinationMode::Shadow);
        assert!(wanted());
    }
    {
        let _mode = set_destination_mode_for_test(DestinationMode::Table);
        assert!(!wanted());
    }
    {
        let _mode = set_destination_mode_for_test(DestinationMode::Legacy);
        assert!(!wanted());
    }
}

#[test]
fn the_test_override_nests_and_restores() {
    use crate::terminal::input_queue::facts::mode_forced_for_test;
    assert_eq!(mode_forced_for_test(), None);
    let outer = set_destination_mode_for_test(DestinationMode::Shadow);
    assert_eq!(destination_mode(), DestinationMode::Shadow);
    {
        let _inner = set_destination_mode_for_test(DestinationMode::Table);
        assert_eq!(destination_mode(), DestinationMode::Table);
    }
    assert_eq!(destination_mode(), DestinationMode::Shadow);
    drop(outer);
    assert_eq!(mode_forced_for_test(), None);
}
