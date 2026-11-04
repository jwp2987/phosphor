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

/// The editor's own hold decision (#1423) against the H8 matrix: agreement logs nothing, a
/// difference is a row-H8 line carrying the class and the gate mask, never content.
#[test]
fn the_editor_matrix_comparison_reports_only_a_difference() {
    use crate::terminal::input_queue::GateKind;
    let _ = take_recorded();
    let gates = GateMask::of(&[GateKind::SubshellSetup]);
    assert_eq!(
        check_editor_matrix(Class::EditKey, gates, true, true),
        None,
        "agreement"
    );
    assert_eq!(
        check_editor_matrix(Class::EditKey, gates, false, false),
        None,
        "agreement"
    );
    assert!(take_recorded().is_empty());
    let disagreement = check_editor_matrix(Class::Enter, gates, true, false).expect("they differ");
    assert_eq!(disagreement.row, Row::H8);
    assert_eq!(disagreement.legacy, DestClass::Editor);
    assert_eq!(disagreement.table, DestClass::Held);
    let line = disagreement.detail();
    assert!(
        line.starts_with("row=H8 legacy=editor table=held flags=editor_key class=Enter gates=0x"),
        "{line}"
    );
    assert_eq!(take_recorded(), vec![line]);
}

/// B14: text the legacy terminal path put straight into the editor while a gate held every other
/// key is a known, tagged difference (the table holds it, strict FIFO); the same difference with
/// the pty as legacy's destination is not.
#[test]
fn editor_text_past_a_gate_is_the_tagged_difference_b14() {
    use crate::terminal::input_queue::GateKind;
    let _ = take_recorded();
    let intent = kb(Class::Typed);
    let facts = RouteFacts {
        gates: GateMask::of(&[GateKind::PasteReading]),
        ..at_prompt()
    };
    let plan = decide(&intent, &facts, None);
    assert_eq!(
        (plan.row, class_of(&plan.outcome)),
        (Row::H8, DestClass::Held)
    );
    let tagged = check(&intent, &plan, &facts, DestClass::Editor).expect("they differ");
    assert_eq!(tagged.expected, Some("B14"));
    assert!(
        tagged.detail().ends_with(" expected=B14"),
        "{}",
        tagged.detail()
    );
    let untagged = check(&intent, &plan, &facts, DestClass::Pty).expect("they differ");
    assert_eq!(untagged.expected, None);
}

/// #1528: shadow mode's consumer, at the pure level. When legacy reaches the destination class the
/// table's plan has, NOTHING is reported, whatever the facts, so a run whose legacy chain agrees
/// with the table has zero disagreements by the counter (`take_untagged`); and when it does not,
/// exactly one line is reported, tagged or not.
#[test]
fn a_legacy_that_mirrors_the_table_reports_zero_disagreements_over_the_facts() {
    use crate::terminal::input_queue::GateMask;
    let _ = take_recorded();
    let mut compared = 0u32;
    let mut disagreed = 0u32;
    // The ssh dimension is the phase (idle / holding) and the setup dialog (row H2): the prompt
    // phase that was the third value here is gone with the screen latch (#1611, #1612, #1621),
    // and the dialog keeps the fact space as wide as the `compared > 1000` floor demands.
    for facts in [at_prompt(), running(), RouteFacts::default()] {
        for (ssh, dialog_up) in [
            (SshPhase::Idle, false),
            (SshPhase::Holding, false),
            (SshPhase::Idle, true),
        ] {
            for mask in [0u16, 1, 1 << 4, 1 << 10, (1 << 12) - 1] {
                let mut facts = facts;
                facts.ssh = ssh;
                facts.ssh_dialog_up = dialog_up;
                facts.gates = GateMask(mask);
                for class in [
                    Class::Typed,
                    Class::Enter,
                    Class::Control,
                    Class::Seq,
                    Class::Paste,
                    Class::Raw,
                    Class::EditKey,
                    Class::Signal(SignalKind::Interrupt),
                    Class::Signal(SignalKind::Eof),
                ] {
                    for origin in [
                        Origin::Keyboard,
                        Origin::Replay(crate::terminal::input_queue::EntryId(1)),
                        Origin::Held,
                    ] {
                        let intent = Intent::new(class, origin).stamped(Stamp::UNSTAMPED);
                        let plan = decide(&intent, &facts, None);
                        let table = class_of(&plan.outcome);
                        assert_eq!(check(&intent, &plan, &facts, table), None);
                        compared += 1;
                        // Any other class is a disagreement, reported exactly once.
                        let other = if table == DestClass::Dropped {
                            DestClass::Pty
                        } else {
                            DestClass::Dropped
                        };
                        let found = check(&intent, &plan, &facts, other).expect("they differ");
                        assert_eq!((found.legacy, found.table), (other, table));
                        disagreed += 1;
                    }
                }
            }
        }
    }
    assert!(compared > 1000, "{compared}");
    let lines = take_recorded();
    assert_eq!(
        lines.len() as u32,
        disagreed,
        "only the constructed differences were reported"
    );
}

/// The shadow counter: `take_untagged` returns only lines without an `expected=` tag, and
/// drains the record.
#[test]
fn take_untagged_counts_only_the_lines_with_no_known_cause() {
    let _ = take_recorded();
    let intent = kb(Class::Typed);
    let facts = at_prompt();
    let plan = decide(&intent, &facts, None);
    // D7, no tag.
    check(&intent, &plan, &facts, DestClass::Pty).expect("they differ");
    // ^Z is tagged OD1.
    let suspend = kb(Class::Signal(SignalKind::Suspend));
    let plan = decide(&suspend, &running(), None);
    let other = if class_of(&plan.outcome) == DestClass::Dropped {
        DestClass::Pty
    } else {
        DestClass::Dropped
    };
    let tagged = check(&suspend, &plan, &running(), other).expect("they differ");
    assert_eq!(tagged.expected, Some("OD1"));
    let untagged_lines = take_untagged();
    assert_eq!(untagged_lines.len(), 1, "{untagged_lines:?}");
    assert!(
        untagged_lines[0].starts_with("row=D7 "),
        "{untagged_lines:?}"
    );
    assert!(take_recorded().is_empty(), "drained");
}
