//! Tests of the destination table (#1424): one case per row (table driven), the 14 gates x 4
//! signals plan matrix, the gate x class matrix of H8, and the properties a pure table has.

use proptest::prelude::*;

use super::*;
use crate::terminal::input_queue::facts::{GateMask, PsPhase};
use crate::terminal::input_queue::{EntryId, SignalPlan};

// ---- builders -------------------------------------------------------------------------------

fn kb(class: Class) -> Intent {
    Intent::new(class, Origin::Keyboard)
}

fn from(class: Class, origin: Origin) -> Intent {
    Intent::new(class, origin)
}

fn at(facts_gen: u64, focus: FocusSnap) -> Stamp {
    Stamp {
        facts_gen,
        focus,
        epoch: 0,
    }
}

/// A command is running in a bootstrapped session, the input box is hidden (a full-screen-ish
/// state): every pty predicate is true.
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

/// The shell is at its prompt: bootstrapped, the input box shown, no block started: the pty does
/// not take typed text (`pty_typed` false), the editor does.
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

struct Case {
    name: &'static str,
    intent: Intent,
    facts: RouteFacts,
    outcome: Outcome,
    row: Row,
}

fn case(name: &'static str, intent: Intent, facts: RouteFacts, outcome: Outcome, row: Row) -> Case {
    Case {
        name,
        intent,
        facts,
        outcome,
        row,
    }
}

fn check(cases: Vec<Case>) {
    for case in cases {
        let plan = decide(&case.intent, &case.facts, None);
        assert_eq!(
            (plan.outcome, plan.row),
            (case.outcome, case.row),
            "{}: got {plan:?}",
            case.name
        );
    }
}

fn plan(intent: Intent, facts: RouteFacts) -> Plan {
    decide(&intent, &facts, None)
}

fn drop_of(why: DropWhy) -> Outcome {
    Outcome::Drop(why)
}

// ---- hold rows ------------------------------------------------------------------------------

#[test]
fn h1_the_ssh_choice_card_shows_its_hint_for_typed_text_and_pastes_only() {
    let card = at(1, FocusSnap::SshChoiceCard);
    check(vec![
        case(
            "typed",
            kb(Class::Typed).stamped(card),
            running(),
            Outcome::Hint,
            Row::H1,
        ),
        case(
            "paste",
            kb(Class::Paste).stamped(card),
            running(),
            Outcome::Hint,
            Row::H1,
        ),
        // The keydown path has no choice-card check.
        case(
            "control",
            kb(Class::Control).stamped(card),
            running(),
            Outcome::Pty(PtyWrite::Control),
            Row::D6,
        ),
    ]);
    assert!(
        plan(kb(Class::Typed).stamped(card), running())
            .effects
            .contains(Effects::NOTE_CHOICE_CARD_HINT)
    );
}

// No H4 test: the row (a key stamped before an ssh prompt is dropped as a possible password, and
// its #1575 shell-prompt exemption) is gone with the screen-reading prompt latch (#1611, #1612,
// #1621; `DECLINED.md`). The ssh connect reads nothing from the screen: what was held before ssh
// owned the tty is delivered to ssh, or discarded by the termios guard, at the ownership event.

#[test]
fn h2_the_setup_dialog_swallows_typing_but_not_sequences() {
    let dialog = RouteFacts {
        ssh_dialog_up: true,
        ..running()
    };
    let swallowed = Outcome::Swallow(SwallowReason::SshDialogOpen);
    check(vec![
        case("typed", kb(Class::Typed), dialog, swallowed, Row::H2),
        case("enter", kb(Class::Enter), dialog, swallowed, Row::H2),
        case("paste", kb(Class::Paste), dialog, swallowed, Row::H2),
        case("control", kb(Class::Control), dialog, swallowed, Row::H2),
        // The dialog consumes arrows and Tab itself.
        case(
            "seq",
            kb(Class::Seq),
            dialog,
            Outcome::Pty(PtyWrite::Seq),
            Row::D6,
        ),
    ]);
    let swallow = plan(kb(Class::Typed), dialog);
    assert!(swallow.effects.contains(Effects::REFOCUS_DIALOG));
    assert!(swallow.effects.contains(Effects::TOAST_ONCE));
}

#[test]
fn h3_an_ssh_hold_holds_every_content_class_and_the_editors_keys() {
    let holding = RouteFacts {
        ssh: SshPhase::Holding,
        queue_has_ssh_gate: true,
        ..running()
    };
    let held = Outcome::Hold(GateKind::SshConnect);
    for class in [
        Class::Typed,
        Class::Enter,
        Class::Control,
        Class::Seq,
        Class::Paste,
        Class::EditKey,
    ] {
        let p = plan(kb(class), holding);
        assert_eq!((p.outcome, p.row), (held, Row::H3), "{class:?}");
        assert!(p.effects.contains(Effects::NOTE_SSH_HOLDING));
        assert!(p.effects.contains(Effects::ARM_SSH_GIVE_UP));
    }
    // Raw passive traffic is not typing; signals take the P rows.
    assert_ne!(plan(kb(Class::Raw), holding).row, Row::H3);
    assert_ne!(
        plan(kb(Class::Signal(SignalKind::Suspend)), holding).row,
        Row::H3
    );
}

#[test]
fn h3b_a_stale_ssh_gate_does_not_hold_and_asks_for_the_segment_to_be_dropped() {
    // The queue has an SshConnect gate but the view's phase is not Holding (the detach window).
    let stale = RouteFacts {
        queue_has_ssh_gate: true,
        gates: GateMask::of(&[GateKind::SshConnect]),
        ssh: SshPhase::Idle,
        ..running()
    };
    let p = plan(kb(Class::Typed), stale);
    assert_eq!(p.outcome, Outcome::Pty(PtyWrite::Typed), "{p:?}");
    assert!(p.effects.contains(Effects::SSH_HOLD_DETACHED));
}

#[test]
fn h5_an_enter_verdict_or_tab_gate_holds_typed_text_and_enters_and_the_matrix_holds_the_rest() {
    for gate in [GateKind::EnterVerdict, GateKind::TabCompletion] {
        let facts = RouteFacts {
            gates: GateMask::of(&[gate]),
            ..running()
        };
        for class in [Class::Typed, Class::Enter, Class::EditKey] {
            let p = plan(kb(class), facts);
            assert_eq!(
                (p.outcome, p.row),
                (Outcome::Hold(gate), Row::H5),
                "{gate:?}/{class:?}"
            );
        }
        // Other keys are held by the matrix (H8) behind the same gate.
        let p = plan(kb(Class::Control), facts);
        assert_eq!((p.outcome, p.row), (Outcome::Hold(gate), Row::H8));
    }
}

#[test]
fn h6_a_powershell_setup_holds_every_non_signal_key_except_the_editors() {
    let holding = |ps| RouteFacts { ps, ..running() };
    for ps in [
        PsPhase::Setup { holds_keys: true },
        PsPhase::LineRunning,
        PsPhase::AfterAbort,
    ] {
        for class in [
            Class::Typed,
            Class::Enter,
            Class::Control,
            Class::Seq,
            Class::Paste,
            Class::Raw,
        ] {
            let p = plan(kb(class), holding(ps));
            assert_eq!(
                (p.outcome, p.row),
                (Outcome::Hold(GateKind::PowershellSetup), Row::H6),
                "{ps:?}/{class:?}"
            );
        }
        // The input editor's own keys (#1423) pass.
        assert_ne!(
            plan(from(Class::EditKey, Origin::Editor), holding(ps)).row,
            Row::H6
        );
    }
    // A setup past its typed-hold limit no longer holds.
    assert_ne!(
        plan(
            kb(Class::Typed),
            holding(PsPhase::Setup { holds_keys: false })
        )
        .row,
        Row::H6
    );
}

#[test]
fn h7_a_subshell_setup_holds_and_h7b_lets_the_answer_to_a_prompt_through() {
    let setup = RouteFacts {
        subshell: SubshellPhase::SetupHeld,
        gates: GateMask::of(&[GateKind::SubshellSetup]),
        ..running()
    };
    let writing = RouteFacts {
        subshell: SubshellPhase::BootstrapWriting,
        gates: GateMask::of(&[GateKind::SubshellBootstrapWrite]),
        ..running()
    };
    check(vec![
        case(
            "setup held",
            kb(Class::Typed),
            setup,
            Outcome::Hold(GateKind::SubshellSetup),
            Row::H7,
        ),
        case(
            "bootstrap lines",
            kb(Class::Control),
            writing,
            Outcome::Hold(GateKind::SubshellBootstrapWrite),
            Row::H7,
        ),
    ]);
    // H7b: the shell asks for something; only subshell gates are open; no lines in flight.
    let asked = RouteFacts {
        subshell_awaiting_answer: true,
        ..setup
    };
    let p = plan(kb(Class::Typed), asked);
    assert_eq!(p.outcome, Outcome::Pty(PtyWrite::Typed), "{p:?}");
    // ... but never while the bootstrap lines are being written, nor with another gate open.
    let asked_writing = RouteFacts {
        subshell_awaiting_answer: true,
        ..writing
    };
    assert_eq!(plan(kb(Class::Typed), asked_writing).row, Row::H7);
    let asked_with_paste = RouteFacts {
        subshell_awaiting_answer: true,
        subshell: SubshellPhase::Announced,
        gates: GateMask::of(&[GateKind::SubshellSetup, GateKind::PasteReading]),
        ..setup
    };
    assert_eq!(plan(kb(Class::Typed), asked_with_paste).row, Row::H8);
}

#[test]
fn h8_every_gate_holds_every_class_except_the_two_exemptions() {
    let classes = [
        Class::Typed,
        Class::Enter,
        Class::Control,
        Class::Seq,
        Class::Paste,
        Class::Raw,
        Class::EditKey,
    ];
    for gate in GateKind::ALL {
        for class in classes {
            for origin in [Origin::Keyboard, Origin::Editor, Origin::Replay(EntryId(1))] {
                let facts = RouteFacts {
                    gates: GateMask::of(&[gate]),
                    queued_only_pty: true,
                    ..running()
                };
                let expected = match gate {
                    // The ssh hold holds its own keys (H3); what reaches the matrix passes.
                    GateKind::SshConnect => false,
                    // #1423: editor keys that EDIT behind only a setup gate apply at once; a key
                    // that submits or completes stays held.
                    GateKind::SubshellSetup | GateKind::PowershellSetup => {
                        !(origin == Origin::Editor
                            && matches!(class, Class::Typed | Class::EditKey | Class::Seq))
                    }
                    _ => true,
                };
                assert_eq!(
                    gate_holds(&facts, class, origin),
                    expected,
                    "{gate:?} x {class:?} x {origin:?}"
                );
            }
        }
    }
    // No gate: nothing holds.
    assert!(!gate_holds(&running(), Class::Typed, Origin::Keyboard));
}

#[test]
fn h8_the_editor_exemption_needs_only_setup_gates_and_only_pty_entries_queued() {
    let setup = GateMask::of(&[GateKind::SubshellSetup, GateKind::PowershellSetup]);
    let editor = |gates, queued_only_pty| {
        gate_holds(
            &RouteFacts {
                gates,
                queued_only_pty,
                ..running()
            },
            Class::EditKey,
            Origin::Editor,
        )
    };
    assert!(!editor(setup, true));
    assert!(editor(setup, false), "an editor entry is queued behind");
    assert!(
        editor(setup.with(GateKind::HandoffReport), true),
        "a hand-off gate holds the editor's keys"
    );
    assert!(editor(setup.with(GateKind::PasteReading), true));
    // CliSubmit's gate holds them too (it is not a setup gate).
    assert!(editor(GateMask::of(&[GateKind::CliSubmit]), true));
}

#[test]
fn h8_queued_entries_and_the_mailbox_hold_a_new_arrival_but_not_a_delivery() {
    let mailbox = RouteFacts {
        mailbox_non_empty: true,
        ..running()
    };
    let queued = RouteFacts {
        queued: true,
        ..running()
    };
    for facts in [mailbox, queued] {
        let p = plan(kb(Class::Typed), facts);
        assert_eq!((p.outcome, p.row), (Outcome::Queue, Row::H8Queue));
        // The entry being delivered already has its place.
        let delivered = plan(
            kb(Class::Typed).delivering(),
            RouteFacts {
                mailbox_non_empty: false,
                ..facts
            },
        );
        assert_eq!(delivered.outcome, Outcome::Pty(PtyWrite::Typed));
    }
    // A mailbox of earlier operations is not overtaken even by a key that otherwise passes.
    let ssh_only = RouteFacts {
        gates: GateMask::of(&[GateKind::SshConnect]),
        mailbox_non_empty: true,
        ..running()
    };
    assert_eq!(plan(kb(Class::Typed), ssh_only).outcome, Outcome::Queue);
}

#[test]
fn h10_an_editor_key_waits_for_a_fresh_display_map_unless_a_signal_flushes() {
    let mut key = from(Class::EditKey, Origin::Editor);
    key.needs_display_map = true;
    let p = plan(key, at_prompt());
    assert_eq!(
        (p.outcome, p.row),
        (Outcome::Hold(GateKind::EditorDisplayMap), Row::H10)
    );
    key.display_map_fresh = true;
    assert_eq!(plan(key, at_prompt()).outcome, Outcome::EditorAction);
    key.display_map_fresh = false;
    key.flushing = true;
    assert_eq!(plan(key, at_prompt()).outcome, Outcome::EditorAction);
}

// ---- destination rows -------------------------------------------------------------------------

#[test]
fn d1_d2_agent_input_for_typed_text_and_pastes() {
    let card = at(1, FocusSnap::ApprovalCard);
    let awaiting = at(1, FocusSnap::AiBlockAwaitingConfirm);
    let agent = RouteFacts {
        agent_owns_pty: true,
        ..running()
    };
    for class in [Class::Typed, Class::Paste] {
        for stamp in [card, awaiting] {
            let p = plan(kb(class).stamped(stamp), running());
            assert_eq!(
                (p.outcome, p.row),
                (Outcome::AgentInput, Row::D1),
                "{class:?}"
            );
            assert!(p.effects.contains(Effects::FOCUS_INPUT));
        }
        let p = plan(kb(class), agent);
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::AgentInput, Row::D2),
            "{class:?}"
        );
    }
}

#[test]
fn d2b_arrows_never_reach_a_command_the_agent_drives() {
    let agent = RouteFacts {
        agent_owns_pty: true,
        ..running()
    };
    let p = plan(kb(Class::Seq), agent);
    assert_eq!(
        (p.outcome, p.row),
        (drop_of(DropWhy::AgentControl), Row::D2b)
    );
}

#[test]
fn d2c_a_control_key_is_not_an_agent_row_it_falls_to_the_keydown_rules() {
    // OD-2: a live Enter (and any control key) while the agent owns the pty never lands in the
    // agent's input; `pty_control` excludes the agent term, so it ends in D10 (dropped, logged).
    let agent = RouteFacts {
        agent_owns_pty: true,
        ..running()
    };
    for class in [Class::Enter, Class::Control] {
        let p = plan(kb(class), agent);
        assert_eq!(
            (p.outcome, p.row),
            (drop_of(DropWhy::NotLongRunning), Row::D10),
            "{class:?}"
        );
        assert!(p.effects.contains(Effects::REFOCUS_GLOBAL), "{class:?}");
    }
    // B13: with the hand-off routing to the pty a control key reaches the pty while the agent
    // owns it. Legacy behaviour, tagged in the table, not changed here.
    let b13 = RouteFacts {
        handoff_routes: true,
        ..agent
    };
    assert_eq!(
        plan(kb(Class::Control), b13).outcome,
        Outcome::Pty(PtyWrite::Control)
    );
}

#[test]
fn d1_prime_and_d11_a_replayed_enter_goes_where_the_text_before_it_went() {
    for origin in [
        Origin::Replay(EntryId(3)),
        Origin::Held,
        Origin::ReleasedFrom(GateKind::HandoffReport),
    ] {
        // The pty took the text: the Enter goes there, and the drain yields after it.
        let p = plan(from(Class::Enter, origin), running());
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::Pty(PtyWrite::EnterReplay), Row::D11),
            "{origin:?}"
        );
        assert!(p.effects.contains(Effects::YIELD_AFTER));
        // A card, an AI block awaiting confirmation or the agent owning the pty: the editor.
        for facts in [
            RouteFacts {
                approval_card: true,
                ..running()
            },
            RouteFacts {
                ai_awaiting_confirmation: true,
                ..running()
            },
            RouteFacts {
                agent_owns_pty: true,
                ..running()
            },
        ] {
            let p = plan(from(Class::Enter, origin), facts);
            assert_eq!(
                (p.outcome, p.row),
                (Outcome::EditorSubmit, Row::D1Replay),
                "{origin:?}"
            );
            assert!(p.effects.contains(Effects::YIELD_AFTER));
        }
        // Text before it went to the editor (not bootstrapped, nothing running): so does it.
        let p = plan(from(Class::Enter, origin), at_prompt());
        assert_eq!(p.outcome, Outcome::EditorSubmit, "{origin:?}");
    }
}

#[test]
fn d3_d12_raw_bytes_drop_only_when_the_agent_is_in_control() {
    let in_control = RouteFacts {
        agent_in_control: true,
        ..running()
    };
    // OD-3: the agent driving but not in control does not stop passive bytes.
    let owns_only = RouteFacts {
        agent_owns_pty: true,
        ..running()
    };
    check(vec![
        case(
            "in control",
            kb(Class::Raw).passive(),
            in_control,
            drop_of(DropWhy::AgentControl),
            Row::D3,
        ),
        case(
            "owns the pty, not in control",
            kb(Class::Raw).passive(),
            owns_only,
            Outcome::Pty(PtyWrite::Passive),
            Row::D12,
        ),
        case(
            "plain",
            kb(Class::Raw).passive(),
            running(),
            Outcome::Pty(PtyWrite::Passive),
            Row::D12,
        ),
    ]);
}

#[test]
fn d4_keyboard_bytes_take_an_alt_screen_program_from_the_agent_pointer_bytes_do_not() {
    let alt = RouteFacts {
        alt_screen: true,
        agent_in_control: true,
        ..running()
    };
    let keyboard = plan(kb(Class::Control), alt);
    assert_eq!(keyboard.outcome, Outcome::Pty(PtyWrite::Control));
    assert!(keyboard.effects.contains(Effects::SWITCH_CONTROL_TO_USER));
    // A mouse report must not take the program over (#963): dropped.
    let pointer = plan(kb(Class::Seq).passive(), alt);
    assert_eq!(
        (pointer.outcome, pointer.row),
        (drop_of(DropWhy::AgentControl), Row::D5)
    );
    assert!(!pointer.effects.contains(Effects::SWITCH_CONTROL_TO_USER));
    // Not on the normal screen.
    let normal = RouteFacts {
        alt_screen: false,
        ..alt
    };
    assert_eq!(
        plan(kb(Class::Control), normal).outcome,
        drop_of(DropWhy::AgentControl)
    );
}

#[test]
fn d5_the_agent_still_in_control_sends_text_to_a_visible_box_and_drops_for_a_hidden_one() {
    let in_control = |input_box_visible| RouteFacts {
        agent_in_control: true,
        input_box_visible,
        ..running()
    };
    check(vec![
        case(
            "typed, box shown",
            kb(Class::Typed),
            in_control(true),
            Outcome::EditorInsert,
            Row::D5,
        ),
        case(
            "typed, box hidden",
            kb(Class::Typed),
            in_control(false),
            drop_of(DropWhy::RefusedHiddenInput),
            Row::D5,
        ),
        case(
            "paste, box shown",
            kb(Class::Paste),
            RouteFacts {
                handoff_routes: true,
                ..in_control(true)
            },
            Outcome::EditorInsert,
            Row::D5,
        ),
        case(
            "paste, box hidden",
            kb(Class::Paste),
            in_control(false),
            drop_of(DropWhy::RefusedHiddenInput),
            Row::D5,
        ),
        case(
            "control",
            kb(Class::Control),
            in_control(true),
            drop_of(DropWhy::AgentControl),
            Row::D5,
        ),
        case(
            "seq",
            kb(Class::Seq),
            in_control(true),
            drop_of(DropWhy::AgentControl),
            Row::D5,
        ),
    ]);
}

#[test]
fn d6_each_class_goes_to_the_pty_by_its_own_predicate_with_the_oracle() {
    let bracketed = RouteFacts {
        needs_bracketed_paste: true,
        input_box_visible: false,
        ..running()
    };
    check(vec![
        case(
            "typed",
            kb(Class::Typed),
            running(),
            Outcome::Pty(PtyWrite::Typed),
            Row::D6,
        ),
        case(
            "control",
            kb(Class::Control),
            running(),
            Outcome::Pty(PtyWrite::Control),
            Row::D6,
        ),
        case(
            "live enter",
            kb(Class::Enter),
            running(),
            Outcome::Pty(PtyWrite::Control),
            Row::D6,
        ),
        case(
            "seq",
            kb(Class::Seq),
            running(),
            Outcome::Pty(PtyWrite::Seq),
            Row::D6,
        ),
        case(
            "paste",
            kb(Class::Paste),
            bracketed,
            Outcome::PasteToPty { bracketed: true },
            Row::D6,
        ),
    ]);
    for class in [Class::Typed, Class::Control, Class::Seq, Class::Paste] {
        assert!(
            plan(kb(class), bracketed)
                .effects
                .contains(Effects::CONSULT_ORACLE),
            "{class:?} asks the hand-off"
        );
    }
    // Released from a hand-off gate: it has been through the oracle once.
    let released = plan(
        from(
            Class::Typed,
            Origin::ReleasedFrom(GateKind::LineEditorActive),
        ),
        running(),
    );
    assert_eq!(released.outcome, Outcome::Pty(PtyWrite::Typed));
    assert!(!released.effects.contains(Effects::CONSULT_ORACLE));
    // A paste lands in the box when it is shown and the shell is bootstrapped.
    let to_box = plan(kb(Class::Paste), at_prompt());
    assert_eq!(to_box.outcome, Outcome::EditorInsert);
}

#[test]
fn d7_d8_typed_text_the_pty_does_not_take_goes_to_the_editor_hidden_box_or_not() {
    // OD-4 / B12: `!pty_typed` always inserts; only a REFUSED write with a hidden box drops.
    for input_box_visible in [true, false] {
        let facts = RouteFacts {
            input_box_visible,
            ..at_prompt()
        };
        assert!(!pty_typed(&facts));
        let p = plan(kb(Class::Typed), facts);
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::EditorInsert, Row::D7),
            "box visible: {input_box_visible}"
        );
        assert_eq!(p.on_refused, OnRefused::NotApplicable);
    }
    // D8: the pty takes typed text, refuses the write, and the box is hidden: dropped, logged.
    let hidden = plan(kb(Class::Typed), running());
    assert_eq!(hidden.outcome, Outcome::Pty(PtyWrite::Typed));
    assert_eq!(
        hidden.on_refused,
        OnRefused::Drop(DropWhy::RefusedHiddenInput)
    );
    let shown = plan(
        kb(Class::Typed),
        RouteFacts {
            input_box_visible: true,
            ..running()
        },
    );
    assert_eq!(shown.on_refused, OnRefused::EditorInsert);
    // A settled AI target had focus: the editor gets focus with the text.
    let settled = plan(
        kb(Class::Typed).stamped(at(1, FocusSnap::SettledAiTarget)),
        at_prompt(),
    );
    assert!(settled.effects.contains(Effects::FOCUS_INPUT));
}

#[test]
fn d9_what_the_editor_produced_is_the_editors() {
    check(vec![
        case(
            "text",
            from(Class::Typed, Origin::Editor),
            running(),
            Outcome::EditorInsert,
            Row::D9,
        ),
        case(
            "key",
            from(Class::EditKey, Origin::Editor),
            running(),
            Outcome::EditorAction,
            Row::D9,
        ),
        case(
            "enter",
            from(Class::Enter, Origin::Editor),
            running(),
            Outcome::EditorSubmit,
            Row::D9,
        ),
    ]);
    assert!(
        plan(from(Class::Enter, Origin::Editor), running())
            .effects
            .contains(Effects::YIELD_AFTER)
    );
}

#[test]
fn d10_a_key_nothing_takes_goes_to_the_takeover_editor_or_is_dropped_with_a_refocus() {
    let takeover = RouteFacts {
        editor_takeover_open: true,
        buffer_empty: false,
        ..at_prompt()
    };
    check(vec![
        case(
            "control, takeover",
            kb(Class::Control),
            takeover,
            Outcome::EditorAction,
            Row::D10,
        ),
        case(
            "enter, takeover, text",
            kb(Class::Enter),
            takeover,
            Outcome::EditorSubmit,
            Row::D10,
        ),
        case(
            "enter, takeover, empty buffer",
            kb(Class::Enter),
            RouteFacts {
                buffer_empty: true,
                ..takeover
            },
            drop_of(DropWhy::NotLongRunning),
            Row::D10,
        ),
        case(
            "control, blocks selected",
            kb(Class::Control),
            RouteFacts {
                blocks_selected: true,
                ..takeover
            },
            drop_of(DropWhy::NotLongRunning),
            Row::D10,
        ),
        case(
            "control, nothing running",
            kb(Class::Control),
            at_prompt(),
            drop_of(DropWhy::NotLongRunning),
            Row::D10,
        ),
        case(
            "seq, nothing running",
            kb(Class::Seq),
            at_prompt(),
            drop_of(DropWhy::NotLongRunning),
            Row::D10,
        ),
    ]);
    let dropped = plan(kb(Class::Control), at_prompt());
    assert!(dropped.effects.contains(Effects::REFOCUS_GLOBAL));
    assert!(dropped.effects.contains(Effects::CLEAR_SELECTION));
    // A sequence only logs: no refocus.
    assert!(
        !plan(kb(Class::Seq), at_prompt())
            .effects
            .contains(Effects::REFOCUS_GLOBAL)
    );
}

#[test]
fn d13_the_agent_is_refused_while_a_setup_owns_the_pty() {
    let closed = RouteFacts {
        agent_write_gate_closed: true,
        ..running()
    };
    check(vec![
        case(
            "refused",
            from(Class::AgentWrite, Origin::Agent),
            closed,
            drop_of(DropWhy::AgentRefusedSetup),
            Row::D13,
        ),
        case(
            "open",
            from(Class::AgentWrite, Origin::Agent),
            running(),
            Outcome::Pty(PtyWrite::Agent),
            Row::D13,
        ),
    ]);
}

// ---- origin rows --------------------------------------------------------------------------------

#[test]
fn synced_bytes_need_a_bootstrapped_running_command() {
    let synced = |class| from(class, Origin::Synced);
    check(vec![
        case(
            "text, running",
            synced(Class::Typed),
            running(),
            Outcome::Pty(PtyWrite::Typed),
            Row::D6,
        ),
        case(
            "text, at prompt",
            synced(Class::Typed),
            at_prompt(),
            drop_of(DropWhy::NotLongRunning),
            Row::OSynced,
        ),
        case(
            "bytes, bootstrap script not echoed",
            synced(Class::Raw),
            RouteFacts {
                // The bootstrap script was not echoed yet (no pending or bootstrapped session).
                session_pending_or_bootstrapped: false,
                ..running()
            },
            drop_of(DropWhy::NotLongRunning),
            Row::OSynced,
        ),
        case(
            "bytes, running",
            synced(Class::Raw),
            running(),
            Outcome::Pty(PtyWrite::Passive),
            Row::D12,
        ),
    ]);
}

#[test]
fn viewer_bytes_take_the_full_rows_and_observe_ctrl_c_when_written() {
    let p = plan(from(Class::Typed, Origin::Viewer), running());
    assert_eq!(p.outcome, Outcome::Pty(PtyWrite::Typed));
    assert!(p.effects.contains(Effects::OBSERVE_CTRL_C));
    // The holds apply to a viewer like to anyone.
    let holding = RouteFacts {
        ssh: SshPhase::Holding,
        ..running()
    };
    assert_eq!(
        plan(from(Class::Typed, Origin::Viewer), holding).outcome,
        Outcome::Hold(GateKind::SshConnect)
    );
}

/// A PACED CLI-agent submit (its own `CliSubmit` gate is open) goes straight to the stage after the
/// paste stage: every hold but the PowerShell setup is skipped, the agent check is not (#1601b).
#[test]
fn a_paced_cli_submit_is_exempt_from_every_hold_but_the_powershell_setup_and_not_from_the_agent_check()
 {
    // Every gate open, the CliSubmit gate among them (`ALL`), and every other hold fact; `ps`
    // stays Off (H6 reads `f.ps`, not the mask, which does include the PowerShell gate).
    let everything_open = RouteFacts {
        ssh: SshPhase::Holding,
        queue_has_ssh_gate: true,
        ssh_dialog_up: true,
        subshell: SubshellPhase::BootstrapWriting,
        gates: GateMask::of(&GateKind::ALL),
        queued: true,
        mailbox_non_empty: true,
        ..running()
    };
    assert!(everything_open.gates.has(GateKind::CliSubmit));
    for class in [Class::Typed, Class::Enter, Class::Control, Class::Paste] {
        let p = plan(from(class, Origin::CliSubmit), everything_open);
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::Pty(PtyWrite::CliSubmit), Row::OCliSubmit),
            "{class:?}"
        );
        // The hand-off is consulted, as for a typed key (it may hold the bytes itself).
        assert!(p.effects.contains(Effects::CONSULT_ORACLE), "{class:?}");
    }
    let in_control = RouteFacts {
        agent_in_control: true,
        ..everything_open
    };
    assert_eq!(
        plan(from(Class::Typed, Origin::CliSubmit), in_control).outcome,
        drop_of(DropWhy::AgentControl)
    );
}

/// An UNPACED CLI-agent submit (no `CliSubmit` gate) is offered to the queue by the legacy writer
/// (`write_user_bytes_to_pty` -> `prepare_subshell_gates` -> `offer_to_paste_queue`): it waits
/// behind every gate a key waits behind, with the same rows and outcomes (#1601b).
#[test]
fn an_unpaced_cli_submit_is_held_behind_the_gates_any_key_is_held_behind() {
    for class in [Class::Raw, Class::Typed, Class::Enter, Class::Paste] {
        for (gate, row) in [
            (GateKind::EnterVerdict, Row::H5),
            (GateKind::TabCompletion, Row::H5),
            (GateKind::PasteReading, Row::H8),
            (GateKind::PromptSeen, Row::H8),
            (GateKind::LineEditorActive, Row::H8),
            (GateKind::SubshellSetup, Row::H8),
            (GateKind::PowershellSetup, Row::H8),
        ] {
            let f = RouteFacts {
                gates: GateMask::of(&[gate]),
                queued: true,
                ..running()
            };
            let p = plan(from(class, Origin::CliSubmit), f);
            assert_eq!(
                (p.outcome, p.row),
                (Outcome::Hold(gate), row),
                "{class:?} behind {gate:?}"
            );
            // The same gate PACED (the submit's own gate open as well) does not hold it.
            let paced = RouteFacts {
                gates: f.gates.with(GateKind::CliSubmit),
                ..f
            };
            assert_eq!(
                plan(from(class, Origin::CliSubmit), paced).outcome,
                Outcome::Pty(PtyWrite::CliSubmit),
                "{class:?} paced, {gate:?}"
            );
        }
    }
}

/// The subshell setup: `prepare_subshell_gates` holds the unpaced submit behind the setup gate and
/// the paced bootstrap write (row H7), unless the shell asks for an answer (the bypass, H7b), which
/// writes it through; the paced submit ignores the setup.
#[test]
fn an_unpaced_cli_submit_is_held_behind_a_subshell_setup_like_a_key() {
    for (phase, gate) in [
        (SubshellPhase::SetupHeld, GateKind::SubshellSetup),
        (
            SubshellPhase::BootstrapWriting,
            GateKind::SubshellBootstrapWrite,
        ),
    ] {
        let f = RouteFacts {
            subshell: phase,
            gates: GateMask::of(&[gate]),
            ..running()
        };
        let p = plan(from(Class::Raw, Origin::CliSubmit), f);
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::Hold(gate), Row::H7),
            "{phase:?}"
        );
        let typed = plan(from(Class::Typed, Origin::Keyboard), f);
        assert_eq!(
            (p.outcome, p.row),
            (typed.outcome, typed.row),
            "a key and a submit wait the same way, {phase:?}"
        );
        let paced = RouteFacts {
            gates: f.gates.with(GateKind::CliSubmit),
            ..f
        };
        assert_eq!(
            plan(from(Class::Raw, Origin::CliSubmit), paced).outcome,
            Outcome::Pty(PtyWrite::CliSubmit),
            "{phase:?} paced"
        );
    }
    // The shell asks for an answer behind a setup gate: typing is the answer, the submit passes.
    let asked = RouteFacts {
        subshell: SubshellPhase::SetupHeld,
        subshell_awaiting_answer: true,
        gates: GateMask::of(&[GateKind::SubshellSetup]),
        ..running()
    };
    assert_eq!(
        plan(from(Class::Raw, Origin::CliSubmit), asked).outcome,
        Outcome::Pty(PtyWrite::CliSubmit)
    );
}

/// Only the ssh connect gate open: a pass-through (the ssh hold holds its own keys). Entries
/// queued with no gate, and the mailbox, hold the submit like any key (`holds_keys`).
#[test]
fn an_unpaced_cli_submit_passes_an_ssh_only_gate_set_and_waits_behind_the_queue() {
    let ssh_only = RouteFacts {
        gates: GateMask::of(&[GateKind::SshConnect]),
        ssh: SshPhase::Holding,
        queue_has_ssh_gate: true,
        ..running()
    };
    let p = plan(from(Class::Raw, Origin::CliSubmit), ssh_only);
    assert_eq!(
        (p.outcome, p.row),
        (Outcome::Pty(PtyWrite::CliSubmit), Row::OCliSubmit)
    );
    // The ssh gate with another one: held behind the queue.
    let both = RouteFacts {
        gates: GateMask::of(&[GateKind::SshConnect, GateKind::PasteReading]),
        ..ssh_only
    };
    assert!(
        plan(from(Class::Raw, Origin::CliSubmit), both)
            .outcome
            .is_hold()
    );
    let queued = RouteFacts {
        queued: true,
        ..running()
    };
    let p = plan(from(Class::Raw, Origin::CliSubmit), queued);
    assert_eq!((p.outcome, p.row), (Outcome::Queue, Row::H8Queue));
    let mailbox = RouteFacts {
        mailbox_non_empty: true,
        ..running()
    };
    assert_eq!(
        plan(from(Class::Raw, Origin::CliSubmit), mailbox).outcome,
        Outcome::Queue
    );
    // Idle: straight to the stage and the hand-off, paced or not.
    let p = plan(from(Class::Raw, Origin::CliSubmit), running());
    assert_eq!(p.outcome, Outcome::Pty(PtyWrite::CliSubmit));
    assert!(p.effects.contains(Effects::CONSULT_ORACLE));
}

/// #1601 (owner decision): an open PowerShell setup holds the CLI-agent submit like every other
/// key, in every phase that holds keys (legacy `write_user_bytes_to_pty_after_paste_stage`).
#[test]
fn cli_submit_is_held_behind_an_open_powershell_setup() {
    for ps in [
        PsPhase::Setup { holds_keys: true },
        PsPhase::LineRunning,
        PsPhase::AfterAbort,
    ] {
        let held = RouteFacts { ps, ..running() };
        for class in [
            Class::Typed,
            Class::Enter,
            Class::Control,
            Class::Paste,
            Class::Raw,
        ] {
            let p = plan(from(class, Origin::CliSubmit), held);
            assert_eq!(
                (p.outcome, p.row),
                (Outcome::Hold(GateKind::PowershellSetup), Row::H6),
                "{ps:?} {class:?}"
            );
        }
        // The hold comes before the agent check, as the legacy hold comes before D4/D5.
        let in_control = RouteFacts {
            agent_in_control: true,
            ..held
        };
        assert_eq!(
            plan(from(Class::Raw, Origin::CliSubmit), in_control).outcome,
            Outcome::Hold(GateKind::PowershellSetup)
        );
    }
    // A setup that no longer holds keys (typed-hold limit passed, or a read-only session) does not.
    let released = RouteFacts {
        ps: PsPhase::Setup { holds_keys: false },
        ..running()
    };
    assert_eq!(
        plan(from(Class::Raw, Origin::CliSubmit), released).outcome,
        Outcome::Pty(PtyWrite::CliSubmit)
    );
}

#[test]
fn a_released_entry_skips_the_oracle_and_a_follow_up_is_an_ordinary_replay() {
    let released = plan(
        from(Class::Control, Origin::ReleasedFrom(GateKind::PromptSeen)),
        running(),
    );
    assert_eq!(released.outcome, Outcome::Pty(PtyWrite::Control));
    assert!(!released.effects.contains(Effects::CONSULT_ORACLE));
    let replay = plan(from(Class::Control, Origin::Replay(EntryId(1))), running());
    assert!(replay.effects.contains(Effects::CONSULT_ORACLE));
}

// ---- the four pty predicates (B11) ------------------------------------------------------------

#[test]
fn the_four_predicates_differ_where_legacy_does() {
    // (a) a block started but not yet long running (between Enter and preexec): pty for typed text,
    // not for a control key.
    let started = RouteFacts {
        active_block_started: true,
        session_pending_or_bootstrapped: true,
        ..RouteFacts::default()
    };
    assert!(pty_typed(&started));
    assert!(!pty_control(&started, false));
    assert!(!pty_seq(&started));
    assert!(pty_enter_replay(&started));
    // (b) pre-bootstrap with no pending session: not pty for typed text even if running.
    let pre_bootstrap = RouteFacts {
        long_running: true,
        active_block_started: true,
        ..RouteFacts::default()
    };
    assert!(!pty_typed(&pre_bootstrap));
    assert!(pty_control(&pre_bootstrap, false));
    assert!(pty_seq(&pre_bootstrap));
    assert!(pty_enter_replay(&pre_bootstrap), "long running counts");
    // (c) a shared-session executor writes before the bootstrap script echoed.
    let executor = RouteFacts {
        active_block_started: true,
        shared_session_executor: true,
        ..RouteFacts::default()
    };
    assert!(pty_typed(&executor));
    // (d) a stalled subshell bootstrap takes typed text; its abort keys are forwarded at once.
    let stalled = RouteFacts {
        active_block_started: true,
        subshell_bootstrap_stalled: true,
        ..RouteFacts::default()
    };
    assert!(pty_typed(&stalled));
    let forwarding = RouteFacts {
        forwards_subshell_abort: true,
        ..RouteFacts::default()
    };
    assert!(pty_control(&forwarding, true));
    assert!(!pty_control(&forwarding, false));
    // (e) the agent owns the pty while the hand-off routes to it: Control goes to the pty, typed
    // text does not (B13).
    let b13 = RouteFacts {
        agent_owns_pty: true,
        handoff_routes: true,
        long_running: true,
        ..RouteFacts::default()
    };
    assert!(pty_control(&b13, false));
    assert!(!pty_enter_replay(&b13));
    // The agent term only applies to non-abort keys.
    let agent = RouteFacts {
        agent_owns_pty: true,
        long_running: true,
        ..RouteFacts::default()
    };
    assert!(!pty_control(&agent, false));
    assert!(pty_control(&agent, true));
    // An abort key reaches the pty when the ssh bootstrap route is Forward.
    let forward = RouteFacts {
        ssh_route_forward: true,
        ..RouteFacts::default()
    };
    assert!(pty_control(&forward, true) && !pty_control(&forward, false));
    // No block started and no hand-off: typed text is the editor's.
    assert!(!pty_typed(&RouteFacts {
        session_pending_or_bootstrapped: true,
        ..RouteFacts::default()
    }));
    assert!(pty_typed(&RouteFacts {
        handoff_routes: true,
        session_pending_or_bootstrapped: true,
        ..RouteFacts::default()
    }));
}

// ---- signals (OD-1, OD-5) -----------------------------------------------------------------------

const SIGNALS: [SignalKind; 4] = [
    SignalKind::Interrupt,
    SignalKind::Eof,
    SignalKind::Suspend,
    SignalKind::Quit,
];

#[test]
fn the_gate_by_signal_plan_matrix() {
    use GateSignalPlan::*;
    // [Interrupt, Eof, Suspend, Quit] per gate kind: the 14 x 4 plan of design revision 3.
    let expected: [(GateKind, [GateSignalPlan; 4]); 12] = [
        (GateKind::PasteReading, [CancelPasteThenFlush; 4]),
        (GateKind::CliSubmit, [CancelPasteThenFlush; 4]),
        (GateKind::EnterVerdict, [DiscardThenSignal; 4]),
        (GateKind::TabCompletion, [DiscardThenSignal; 4]),
        (GateKind::SshConnect, [DiscardThenSignal; 4]),
        (
            GateKind::HandoffReport,
            [
                CancelDeferredEnterThenSignal,
                WaitInOrder,
                CancelDeferredEnterThenSignal,
                CancelDeferredEnterThenSignal,
            ],
        ),
        (
            GateKind::PromptSeen,
            [
                VoidHeldThenSignal,
                FlushThenSignal,
                VoidHeldThenSignal,
                VoidHeldThenSignal,
            ],
        ),
        (
            GateKind::LineEditorActive,
            [
                VoidHeldThenSignal,
                FlushThenSignal,
                VoidHeldThenSignal,
                VoidHeldThenSignal,
            ],
        ),
        (GateKind::EditorDisplayMap, [FlushThenSignal; 4]),
        (GateKind::SubshellSetup, [SignalThenRelease; 4]),
        (
            GateKind::SubshellBootstrapWrite,
            [StashUntilBootstrapDone; 4],
        ),
        (GateKind::PowershellSetup, [SignalThenKeepGate; 4]),
    ];
    assert_eq!(expected.len(), GateKind::ALL.len());
    for gate in GateKind::ALL {
        assert!(
            expected.iter().any(|(kind, _)| *kind == gate),
            "{gate:?} has no row in the matrix"
        );
    }
    for (gate, plans) in expected {
        for (signal, want) in SIGNALS.into_iter().zip(plans) {
            assert_eq!(signal_plan(gate, signal), want, "{gate:?} x {signal:?}");
        }
    }
}

#[test]
fn the_interrupt_and_eof_columns_are_what_the_legacy_per_gate_plan_does() {
    // The deliberate change is the Suspend/Quit column (and the hand-off cases the legacy code
    // handled outside the queue). Where the new plan has a legacy name it must agree with it for
    // ^C, so switching callers later changes nothing for ^C.
    for gate in GateKind::ALL {
        let legacy = gate.signal_plan();
        let new = signal_plan(gate, SignalKind::Interrupt);
        let agrees = match (legacy, new) {
            (SignalPlan::CancelPasteThenFlush, GateSignalPlan::CancelPasteThenFlush)
            | (SignalPlan::FlushThenSignal, GateSignalPlan::FlushThenSignal)
            | (SignalPlan::DiscardThenSignal, GateSignalPlan::DiscardThenSignal)
            | (SignalPlan::SignalThenRelease, GateSignalPlan::SignalThenRelease)
            | (SignalPlan::SignalThenKeepGate, GateSignalPlan::SignalThenKeepGate) => true,
            // The cases the legacy code handles outside `apply_signal`.
            (SignalPlan::FlushThenSignal, GateSignalPlan::CancelDeferredEnterThenSignal)
                if gate == GateKind::HandoffReport =>
            {
                true
            }
            (SignalPlan::FlushThenSignal, GateSignalPlan::VoidHeldThenSignal)
                if matches!(gate, GateKind::PromptSeen | GateKind::LineEditorActive) =>
            {
                true
            }
            (SignalPlan::SignalThenRelease, GateSignalPlan::StashUntilBootstrapDone)
                if gate == GateKind::SubshellBootstrapWrite =>
            {
                true
            }
            _ => false,
        };
        assert!(agrees, "{gate:?}: legacy {legacy:?}, new {new:?}");
    }
}

fn signal_facts() -> RouteFacts {
    running()
}

#[test]
fn p1f_a_held_signal_is_delivered_to_the_remote_and_never_held_again() {
    for signal in SIGNALS {
        let busy = RouteFacts {
            ssh: SshPhase::Holding,
            ssh_interrupt_must_wait: true,
            gates: GateMask::of(&[GateKind::SshConnect]),
            ..signal_facts()
        };
        let p = plan(from(Class::Signal(signal), Origin::Held), busy);
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::SignalToPty(signal), Row::P1f),
            "{signal:?}"
        );
    }
}

#[test]
fn p1a_od5_a_signal_in_the_ssh_setup_window_is_held_for_the_remote_side() {
    for signal in SIGNALS {
        let window = RouteFacts {
            ssh_interrupt_must_wait: true,
            ..signal_facts()
        };
        let p = plan(kb(Class::Signal(signal)), window);
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::HoldSignal, Row::P1a),
            "{signal:?}"
        );
        assert!(
            p.effects.contains(Effects::NOTE_INTERRUPT_HELD),
            "the toast is now true"
        );
        assert!(p.effects.contains(Effects::NOTE_SSH_HOLDING));
        assert!(!p.outcome.is_pty(), "never to the local shell");
    }
    // A ^D also waits in the connect window; the other signals do not.
    let connect = RouteFacts {
        ssh_eot_must_wait: true,
        ..signal_facts()
    };
    assert_eq!(
        plan(kb(Class::Signal(SignalKind::Eof)), connect).outcome,
        Outcome::HoldSignal
    );
    for signal in [SignalKind::Interrupt, SignalKind::Suspend, SignalKind::Quit] {
        assert_ne!(
            plan(kb(Class::Signal(signal)), connect).outcome,
            Outcome::HoldSignal,
            "{signal:?}"
        );
    }
}

#[test]
fn p1b_a_passing_signal_resets_the_ssh_hold_and_goes_on() {
    for signal in SIGNALS {
        let holding = RouteFacts {
            ssh: SshPhase::Holding,
            ssh_hold_exists: true,
            queue_has_ssh_gate: true,
            gates: GateMask::of(&[GateKind::SshConnect]),
            ..signal_facts()
        };
        let p = plan(kb(Class::Signal(signal)), holding);
        assert!(p.effects.contains(Effects::RESET_SSH_HOLD), "{signal:?}");
        // The SshConnect gate meets the signal: DiscardThenSignal is the queue's job.
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::SignalAtGates, Row::P2),
            "{signal:?}"
        );
    }
}

#[test]
fn p1c_a_powershell_setup_has_its_own_abort_and_other_signals_go_straight_to_the_leaf() {
    let ps = RouteFacts {
        ps: PsPhase::Setup { holds_keys: true },
        ..signal_facts()
    };
    let p = plan(kb(Class::Signal(SignalKind::Interrupt)), ps);
    assert_eq!((p.outcome, p.row), (Outcome::PowershellAbort, Row::P1c));
    for signal in [SignalKind::Eof, SignalKind::Suspend, SignalKind::Quit] {
        let p = plan(kb(Class::Signal(signal)), ps);
        assert_eq!(
            (p.outcome, p.row),
            (Outcome::SignalToPty(signal), Row::P1c),
            "{signal:?}: not held again by the PowerShell hold"
        );
    }
}

#[test]
fn p1d_every_signal_is_stashed_during_the_bootstrap_lines() {
    for signal in SIGNALS {
        let writing = RouteFacts {
            subshell: SubshellPhase::BootstrapWriting,
            gates: GateMask::of(&[GateKind::SubshellBootstrapWrite]),
            forwards_subshell_abort: true,
            forwards_subshell_key: true,
            ..signal_facts()
        };
        let p = plan(kb(Class::Signal(signal)), writing);
        assert_eq!((p.outcome, p.row), (Outcome::Stash, Row::P1d), "{signal:?}");
        assert!(p.effects.contains(Effects::CANCEL_PACED_SETUP_WRITES));
    }
}

#[test]
fn p1e_a_ctrl_c_while_ssh_connects_aborts_signals_or_writes_the_byte() {
    let ctrl_c = kb(Class::Signal(SignalKind::Interrupt));
    check(vec![
        case(
            "hook seen: ~. abort",
            ctrl_c,
            RouteFacts {
                ssh_pre_init_abort: true,
                ssh_sigint_applies: true,
                ..signal_facts()
            },
            Outcome::AbortBytes,
            Row::P1e,
        ),
        case(
            "still connecting: SIGINT",
            ctrl_c,
            RouteFacts {
                ssh_sigint_applies: true,
                ..signal_facts()
            },
            Outcome::SigintForeground,
            Row::P1e,
        ),
        case(
            "otherwise the byte",
            ctrl_c,
            signal_facts(),
            Outcome::SignalToPty(SignalKind::Interrupt),
            Row::D6,
        ),
        // The ssh rules are ^C's: a ^D or ^Z does not abort the connect.
        case(
            "^Z is not an abort",
            kb(Class::Signal(SignalKind::Suspend)),
            RouteFacts {
                ssh_pre_init_abort: true,
                ssh_sigint_applies: true,
                ..signal_facts()
            },
            Outcome::SignalToPty(SignalKind::Suspend),
            Row::D6,
        ),
    ]);
    assert!(
        plan(
            ctrl_c,
            RouteFacts {
                ssh_sigint_applies: true,
                ..signal_facts()
            }
        )
        .effects
        .contains(Effects::LEAVE_USER_WROTE_CTRL_C)
    );
}

#[test]
fn p2_and_d6_a_signal_meets_open_gates_or_goes_to_the_pty_for_all_four() {
    for signal in SIGNALS {
        check(vec![
            case(
                "free",
                kb(Class::Signal(signal)),
                signal_facts(),
                Outcome::SignalToPty(signal),
                Row::D6,
            ),
            case(
                "gates",
                kb(Class::Signal(signal)),
                RouteFacts {
                    gates: GateMask::of(&[GateKind::PasteReading]),
                    ..signal_facts()
                },
                Outcome::SignalAtGates,
                Row::P2,
            ),
        ]);
    }
}

#[test]
fn a_keyboard_signal_nothing_takes_is_dropped_but_a_queue_signal_is_not() {
    for signal in SIGNALS {
        let p = plan(kb(Class::Signal(signal)), at_prompt());
        assert_eq!(
            (p.outcome, p.row),
            (drop_of(DropWhy::NotLongRunning), Row::D10),
            "{signal:?}"
        );
        // A signal the queue replays (or an editor-side signal) is already past the keydown test.
        let replayed = plan(
            from(Class::Signal(signal), Origin::Replay(EntryId(1))),
            at_prompt(),
        );
        assert_eq!(replayed.outcome, Outcome::SignalToPty(signal), "{signal:?}");
    }
}

#[test]
fn d5_a_signal_is_refused_while_the_agent_is_in_control() {
    for signal in SIGNALS {
        let p = plan(
            kb(Class::Signal(signal)),
            RouteFacts {
                agent_in_control: true,
                ..signal_facts()
            },
        );
        assert_eq!(
            (p.outcome, p.row),
            (drop_of(DropWhy::AgentControl), Row::D5),
            "{signal:?}"
        );
    }
}

// ---- pinning and B12 --------------------------------------------------------------------------

#[test]
fn a_line_pinned_to_the_editor_stays_there_while_the_pin_is_valid() {
    let pty_now = RouteFacts {
        input_box_visible: true,
        ..running()
    };
    let pinned = |class, facts| decide(&kb(class), &facts, Some(Destination::Editor));
    let p = pinned(Class::Typed, pty_now);
    assert_eq!((p.outcome, p.row), (Outcome::EditorInsert, Row::Pin));
    assert!(!p.effects.contains(Effects::CONSULT_ORACLE));
    assert_eq!(
        pinned(Class::Control, pty_now).outcome,
        Outcome::EditorAction
    );
    // Not once the hand-off routes to the pty, with a card up, or the agent owning the pty.
    for facts in [
        RouteFacts {
            handoff_routes: true,
            ..pty_now
        },
        RouteFacts {
            approval_card: true,
            ..pty_now
        },
        RouteFacts {
            input_box_visible: false,
            ..running()
        },
    ] {
        assert_eq!(
            pinned(Class::Typed, facts).outcome,
            Outcome::Pty(PtyWrite::Typed)
        );
    }
    // A pin to the pty changes nothing (the predicates already say pty).
    assert_eq!(
        decide(&kb(Class::Typed), &pty_now, Some(Destination::Pty)).outcome,
        Outcome::Pty(PtyWrite::Typed)
    );
}

#[test]
fn b12_the_old_destination_for_dropped_text_the_pty_does_not_take_the_fixed_one_inserts() {
    let hidden = DestFacts {
        pty_takes_input: false,
        input_box_visible: false,
        pty_open: true,
        ..DestFacts::default()
    };
    let text = EntryKind::Text("a".to_owned());
    // The old function drops; legacy (`after_pty_attempt`) never does: it inserts.
    assert_eq!(
        super::super::destination_for(&text, &hidden),
        Destination::Drop(DropReason::RefusedHiddenInput)
    );
    assert_eq!(destination_for_fixed(&text, &hidden), Destination::Editor);
    // Where the old function was right it is unchanged.
    let takes = DestFacts {
        pty_takes_input: true,
        ..hidden
    };
    assert_eq!(destination_for_fixed(&text, &takes), Destination::Pty);
    let visible = DestFacts {
        input_box_visible: true,
        ..hidden
    };
    assert_eq!(destination_for_fixed(&text, &visible), Destination::Editor);
    let card = DestFacts {
        approval_card: true,
        ..hidden
    };
    assert_eq!(destination_for_fixed(&text, &card), Destination::AgentInput);
    let raw = EntryKind::Raw(vec![0x1b]);
    assert_eq!(
        destination_for_fixed(&raw, &hidden),
        super::super::destination_for(&raw, &hidden),
        "raw bytes are not text: unchanged"
    );
}

#[test]
fn effects_are_a_set() {
    let both = Effects::FOCUS_INPUT | Effects::TOAST_ONCE;
    assert!(both.contains(Effects::FOCUS_INPUT) && both.contains(Effects::TOAST_ONCE));
    assert!(!both.contains(Effects::REFOCUS_GLOBAL));
    assert!(
        !Effects::NONE.contains(Effects::NONE),
        "the empty set contains nothing"
    );
    assert!(Effects::NONE.is_empty() && !both.is_empty());
}

// ---- properties -------------------------------------------------------------------------------

fn origin_of(index: usize) -> Origin {
    match index % 11 {
        0 => Origin::Keyboard,
        1 => Origin::Editor,
        2 => Origin::Replay(EntryId(1)),
        3 => Origin::Synthesized,
        4 => Origin::Held,
        5 => Origin::Synced,
        6 => Origin::Viewer,
        7 => Origin::CliSubmit,
        8 => Origin::Agent,
        9 => Origin::ReleasedFrom(GateKind::LineEditorActive),
        _ => Origin::ReleasedFrom(GateKind::HandoffReport),
    }
}

fn class_of(index: usize) -> Class {
    match index % 12 {
        0 => Class::Typed,
        1 => Class::Enter,
        2 => Class::Control,
        3 => Class::Seq,
        4 => Class::Paste,
        5 => Class::Raw,
        6 => Class::AgentWrite,
        7 => Class::EditKey,
        8 => Class::Signal(SignalKind::Interrupt),
        9 => Class::Signal(SignalKind::Eof),
        10 => Class::Signal(SignalKind::Suspend),
        _ => Class::Signal(SignalKind::Quit),
    }
}

fn focus_of(index: usize) -> FocusSnap {
    [
        FocusSnap::Other,
        FocusSnap::Terminal,
        FocusSnap::Editor,
        FocusSnap::SshChoiceCard,
        FocusSnap::ApprovalCard,
        FocusSnap::AiBlockAwaitingConfirm,
        FocusSnap::SettledAiTarget,
    ][index % 7]
}

fn arb_facts() -> impl Strategy<Value = RouteFacts> {
    (
        any::<[bool; 32]>(),
        any::<[bool; 16]>(),
        0usize..2,
        0usize..4,
        0usize..5,
        0u16..(1 << 14),
        0u64..8,
    )
        .prop_map(|(a, b, ssh, subshell, ps, gates, generation)| RouteFacts {
            facts_gen: generation,
            model_consistent: a[0],
            alt_screen: a[1],
            agent_in_control: a[2],
            agent_in_control_exact: a[3],
            agent_owns_pty: a[4],
            long_running: a[5],
            read_only: a[6],
            is_bootstrapped: a[7],
            needs_bracketed_paste: a[8],
            active_block_started: a[9],
            shared_session_executor: a[10],
            session_pending_or_bootstrapped: a[11],
            has_pending_ssh_command: a[12],
            handoff_routes: a[13],
            handoff_known: a[14],
            subshell: [
                SubshellPhase::Idle,
                SubshellPhase::Announced,
                SubshellPhase::SetupHeld,
                SubshellPhase::BootstrapWriting,
            ][subshell],
            forwards_subshell_key: a[15],
            forwards_subshell_abort: a[16],
            subshell_awaiting_answer: a[17],
            subshell_bootstrap_stalled: a[18],
            ps: [
                PsPhase::Off,
                PsPhase::Setup { holds_keys: true },
                PsPhase::Setup { holds_keys: false },
                PsPhase::LineRunning,
                PsPhase::AfterAbort,
            ][ps],
            ssh: [SshPhase::Idle, SshPhase::Holding][ssh],
            ssh_dialog_up: a[19],
            ssh_setup_in_flight: a[20],
            ssh_bootstrapping: a[21],
            ssh_connecting_silently: a[22],
            ssh_interrupt_must_wait: a[23],
            ssh_eot_must_wait: a[24],
            ssh_hold_exists: a[25],
            queue_has_ssh_gate: a[26],
            ssh_route_forward: a[27],
            ssh_pre_init_abort: a[28],
            ssh_sigint_applies: a[29],
            focus: focus_of(b[0] as usize),
            approval_card: b[1],
            ai_awaiting_confirmation: b[2],
            settled_ai_target: b[3],
            input_box_visible: b[4],
            editor_focused: b[5],
            editor_takeover_open: b[6],
            blocks_selected: b[7],
            buffer_empty: b[8],
            cli_agent_session: b[9],
            gates: GateMask(gates),
            queued: b[10],
            queued_only_pty: b[11],
            mailbox_non_empty: b[12],
            agent_write_gate_closed: b[13],
        })
}

fn arb_intent() -> impl Strategy<Value = Intent> {
    (
        0usize..12,
        0usize..11,
        0u64..8,
        0usize..7,
        any::<[bool; 4]>(),
    )
        .prop_map(|(class, origin, generation, focus, flags)| Intent {
            class: class_of(class),
            origin: origin_of(origin),
            stamp: at(generation, focus_of(focus)),
            takes_over_agent: flags[0],
            delivering: flags[1],
            needs_display_map: flags[2],
            display_map_fresh: flags[3],
            flushing: flags[0] && flags[3],
        })
}

fn arb_pin() -> impl Strategy<Value = Option<Destination>> {
    prop_oneof![
        Just(None),
        Just(Some(Destination::Editor)),
        Just(Some(Destination::Pty)),
        Just(Some(Destination::AgentInput)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Total and deterministic: every combination of facts, intent and pin has a plan, and asking
    /// twice gives the same one.
    #[test]
    fn decide_is_total_and_deterministic(i in arb_intent(), f in arb_facts(), pin in arb_pin()) {
        let first = decide(&i, &f, pin);
        let second = decide(&i, &f, pin);
        prop_assert_eq!(first, second);
    }

    /// Every drop names its reason, and the reason names a row that can drop for it.
    #[test]
    fn every_drop_has_a_reason_and_a_row_that_gives_it(i in arb_intent(), f in arb_facts(), pin in arb_pin()) {
        let plan = decide(&i, &f, pin);
        if let Outcome::Drop(why) = plan.outcome {
            let allowed: &[Row] = match why {
                DropWhy::AgentRefusedSetup => &[Row::D13],
                DropWhy::AgentControl => &[Row::D2b, Row::D3, Row::D5],
                DropWhy::NotLongRunning => &[Row::D10, Row::OSynced],
                DropWhy::RefusedHiddenInput => &[Row::D5],
            };
            prop_assert!(allowed.contains(&plan.row), "{:?} dropped by {:?}", why, plan.row);
        }
        if let OnRefused::Drop(_) = plan.on_refused {
            prop_assert_eq!(plan.outcome, Outcome::Pty(PtyWrite::Typed));
        }
    }

    /// Nothing but an abort key (and the legacy B13 case, a control key while the hand-off routes
    /// to the pty) reaches the pty of a command the agent owns.
    #[test]
    fn nothing_reaches_the_pty_while_the_agent_owns_it_except_aborts(i in arb_intent(), mut f in arb_facts()) {
        f.agent_owns_pty = true;
        let user_origin = matches!(
            i.origin,
            Origin::Keyboard | Origin::Editor | Origin::Replay(_) | Origin::Synthesized | Origin::Held
        );
        let plan = decide(&i, &f, None);
        if !user_origin || !plan.outcome.is_pty() {
            return Ok(());
        }
        match i.class {
            Class::Typed | Class::Paste | Class::Seq => {
                prop_assert!(false, "{:?} reached the pty: {:?}", i.class, plan);
            }
            Class::Enter if i.origin != Origin::Keyboard && i.origin != Origin::Synthesized => {
                prop_assert!(false, "a replayed Enter reached the pty: {:?}", plan);
            }
            Class::Control | Class::Enter => {
                prop_assert!(
                    f.handoff_routes || f.forwards_subshell_key,
                    "a control key reached the pty of the agent without B13: {:?}", plan
                );
            }
            Class::Signal(_) | Class::Raw | Class::AgentWrite | Class::EditKey => {}
        }
    }

    /// Typed text the pty does not take is never dropped for a hidden input box (B12, OD-4), and
    /// a hidden box only ever drops text the pty refused.
    #[test]
    fn a_hidden_box_never_drops_text_the_pty_does_not_take(i in arb_intent(), mut f in arb_facts(), pin in arb_pin()) {
        f.input_box_visible = false;
        let mut i = i;
        i.class = Class::Typed;
        let plan = decide(&i, &f, pin);
        if !pty_typed(&f) {
            prop_assert_ne!(plan.outcome, Outcome::Drop(DropWhy::RefusedHiddenInput));
            prop_assert_eq!(plan.on_refused, OnRefused::NotApplicable);
        }
    }

    /// Signals never wait: no hold, no queue. The only thing that holds one is P1a, and only a
    /// keyboard signal (OD-5); a held signal is never delivered locally.
    #[test]
    fn signals_never_wait_behind_a_gate(i in arb_intent(), f in arb_facts()) {
        if !i.class.is_signal() {
            return Ok(());
        }
        let plan = decide(&i, &f, None);
        prop_assert!(
            !matches!(plan.outcome, Outcome::Hold(_) | Outcome::Queue),
            "{:?}", plan
        );
        if plan.outcome == Outcome::HoldSignal {
            prop_assert_eq!(plan.row, Row::P1a);
            prop_assert!(matches!(i.origin, Origin::Keyboard | Origin::Synthesized));
            prop_assert!(f.ssh_interrupt_must_wait || f.ssh_eot_must_wait);
            prop_assert!(!plan.outcome.is_pty());
        }
        if i.origin == Origin::Held {
            prop_assert_eq!(plan.row, Row::P1f);
        }
    }

    /// Every write to the pty asks the hand-off, except entries released from one of its gates.
    #[test]
    fn pty_writes_consult_the_oracle_unless_released(i in arb_intent(), f in arb_facts()) {
        let plan = decide(&i, &f, None);
        let pty_write = matches!(
            plan.outcome,
            Outcome::Pty(_) | Outcome::PasteToPty { .. } | Outcome::SignalToPty(_)
        );
        if pty_write && !matches!(plan.row, Row::Pin) {
            prop_assert_eq!(
                plan.effects.contains(Effects::CONSULT_ORACLE),
                !matches!(i.origin, Origin::ReleasedFrom(_))
            );
        }
    }

    /// The CLI-agent submit (#1601b), as the legacy writer treats its bytes. An open PowerShell
    /// setup holds it behind `PowershellSetup` and nothing else, paced or not. A PACED submit (its
    /// `CliSubmit` gate is open) is exempt from every other hold. An UNPACED one waits behind any
    /// open gate but an ssh-only set, behind a subshell setup, and behind the queue; otherwise it
    /// is not held. Nothing the editor produced goes to the pty.
    #[test]
    fn origin_exemptions_hold(i in arb_intent(), f in arb_facts()) {
        let plan = decide(&i, &f, None);
        if i.origin == Origin::CliSubmit && !i.class.is_signal() && i.class != Class::AgentWrite {
            let paced = f.gates.has(GateKind::CliSubmit);
            let setup_held = matches!(
                f.subshell,
                SubshellPhase::SetupHeld | SubshellPhase::BootstrapWriting
            );
            if f.ps.holds_keys() {
                prop_assert_eq!(plan.outcome, Outcome::Hold(GateKind::PowershellSetup), "{:?}", plan);
            } else if paced {
                prop_assert!(!plan.outcome.is_hold() && plan.outcome != Outcome::Hint
                    && !matches!(plan.outcome, Outcome::Swallow(_)), "{:?}", plan);
            } else {
                // The shell asking for an answer lets a key past the setup gates (H7b).
                if !f.subshell_awaiting_answer
                    && ((!f.gates.is_empty() && !f.gates.all_within(&[GateKind::SshConnect]))
                        || setup_held
                        // A delivering entry is never held behind the queue it is being drained from.
                        || (!i.delivering && (f.mailbox_non_empty || (f.queued && f.gates.is_empty()))))
                {
                    prop_assert!(plan.outcome.is_hold(), "{:?}", plan);
                }
                if f.gates.is_empty() || f.gates.all_within(&[GateKind::SshConnect]) {
                    if !setup_held && !f.mailbox_non_empty && !(f.queued && f.gates.is_empty()) {
                        prop_assert!(!plan.outcome.is_hold() && plan.outcome != Outcome::Hint
                            && !matches!(plan.outcome, Outcome::Swallow(_)), "{:?}", plan);
                    }
                }
            }
        }
        if i.origin == Origin::Editor && !i.class.is_signal() && i.class != Class::AgentWrite {
            prop_assert!(!plan.outcome.is_pty(), "{:?}", plan);
        }
    }

    /// The matrix and the plan agree about which gates hold: with an arbitrary open gate the
    /// content class is held, unless one of the two exemptions applies.
    #[test]
    fn the_matrix_is_what_holds_content(i in arb_intent(), f in arb_facts()) {
        let plain = matches!(
            i.class,
            Class::Typed | Class::Enter | Class::Control | Class::Seq | Class::Paste | Class::Raw
        ) && matches!(i.origin, Origin::Keyboard | Origin::Replay(_) | Origin::Synthesized);
        let f = RouteFacts { ssh: SshPhase::Idle, ps: PsPhase::Off, subshell: SubshellPhase::Idle, ssh_dialog_up: false, queue_has_ssh_gate: false, ..f };
        if plain && !f.gates.is_empty() && !f.gates.all_within(&[GateKind::SshConnect])
            && !f.subshell_awaiting_answer
        {
            let plan = decide(&i, &f, None);
            prop_assert!(
                matches!(plan.outcome, Outcome::Hold(_) | Outcome::Hint | Outcome::Drop(_)),
                "an open gate did not hold {:?}: {:?}", i.class, plan
            );
        }
    }
}

// ---- enumerated totality (#1424, Fable review item 3) -------------------------------------------

/// Facts from an index of the enumeration: 10 flags, 4 ssh states, 5 gate states.
fn enum_facts(bits: u32, ssh: u8, gate: u8) -> RouteFacts {
    let b = |n: u32| bits & (1 << n) != 0;
    let mut facts = RouteFacts {
        model_consistent: true,
        handoff_known: true,
        long_running: b(0),
        active_block_started: b(1),
        session_pending_or_bootstrapped: b(2),
        handoff_routes: b(3),
        agent_owns_pty: b(4),
        agent_in_control: b(5),
        input_box_visible: b(6),
        editor_takeover_open: b(7),
        ps: if b(8) {
            PsPhase::Setup { holds_keys: true }
        } else {
            PsPhase::Off
        },
        queued: b(9),
        agent_write_gate_closed: b(8),
        ..RouteFacts::default()
    };
    // State 2 was the prompt phase, gone with the screen latch (#1611, #1612, #1621); it now
    // repeats the idle state.
    match ssh {
        1 => facts.ssh = SshPhase::Holding,
        3 => facts.ssh_dialog_up = true,
        _ => {}
    }
    match gate {
        1 => facts.gates = GateMask::of(&[GateKind::PasteReading]),
        2 => {
            facts.gates = GateMask::of(&[GateKind::SubshellSetup]);
            facts.subshell = SubshellPhase::SetupHeld;
            facts.forwards_subshell_abort = true;
        }
        3 => {
            facts.gates = GateMask::of(&[GateKind::SubshellBootstrapWrite]);
            facts.subshell = SubshellPhase::BootstrapWriting;
            facts.forwards_subshell_abort = true;
        }
        4 => facts.gates = GateMask::of(&[GateKind::EnterVerdict]),
        _ => {}
    }
    facts
}

/// Every combination of the decisive facts (about 20,000), against every class and origin:
/// `decide` is total (it never panics, whatever the combination), every row a plan can carry is
/// reached by some combination, and every outcome comes with a row. The four rows that are only
/// ever EFFECTS or a bypass of the hold rows are named: they are never the row of a plan.
#[test]
fn enumerated_facts_make_decide_total_and_reach_every_row_and_outcome() {
    use std::collections::{HashMap, HashSet};
    use std::mem::{Discriminant, discriminant};

    let mut rows: HashSet<&'static str> = HashSet::new();
    let mut outcomes: HashMap<Discriminant<Outcome>, HashSet<&'static str>> = HashMap::new();
    let mut decisions = 0u64;
    let mut note = |plan: Plan| {
        decisions += 1;
        rows.insert(plan.row.name());
        outcomes
            .entry(discriminant(&plan.outcome))
            .or_default()
            .insert(plan.row.name());
    };

    let content = [
        Class::Typed,
        Class::Enter,
        Class::Control,
        Class::Seq,
        Class::Paste,
        Class::Raw,
    ];
    let signals = [
        Class::Signal(SignalKind::Interrupt),
        Class::Signal(SignalKind::Eof),
        Class::Signal(SignalKind::Suspend),
        Class::Signal(SignalKind::Quit),
    ];
    let origins = [Origin::Keyboard, Origin::Editor, Origin::Held];
    let stamp = at(1, FocusSnap::Other);

    for bits in 0u32..1024 {
        for ssh in 0u8..4 {
            for gate in 0u8..5 {
                let facts = enum_facts(bits, ssh, gate);
                for origin in origins {
                    for class in content {
                        note(decide(
                            &Intent::new(class, origin).stamped(stamp),
                            &facts,
                            None,
                        ));
                    }
                    // An editor key may wait for the display map or not.
                    for needs_map in [false, true] {
                        let mut intent = Intent::new(Class::EditKey, origin).stamped(stamp);
                        intent.needs_display_map = needs_map;
                        note(decide(&intent, &facts, None));
                    }
                }
                // Signals: the ssh setup window's three bits only matter for a keydown.
                for class in signals {
                    for window in 0u8..8 {
                        let facts = RouteFacts {
                            ssh_interrupt_must_wait: window & 1 != 0,
                            ssh_pre_init_abort: window & 2 != 0,
                            ssh_sigint_applies: window & 4 != 0,
                            ssh_hold_exists: ssh == 1,
                            ..facts
                        };
                        note(decide(
                            &Intent::new(class, Origin::Keyboard).stamped(stamp),
                            &facts,
                            None,
                        ));
                    }
                    note(decide(
                        &Intent::new(class, Origin::Held).stamped(stamp),
                        &facts,
                        None,
                    ));
                }
                // The agent's writes, and the origins with rows of their own.
                note(decide(
                    &Intent::new(Class::AgentWrite, Origin::Agent).stamped(stamp),
                    &facts,
                    None,
                ));
                for origin in [Origin::Synced, Origin::CliSubmit, Origin::Viewer] {
                    for class in [Class::Typed, Class::Raw, Class::Control, Class::Paste] {
                        note(decide(
                            &Intent::new(class, origin).stamped(stamp),
                            &facts,
                            None,
                        ));
                    }
                }
                // The cards: what had focus at the sink.
                for focus in [
                    FocusSnap::SshChoiceCard,
                    FocusSnap::ApprovalCard,
                    FocusSnap::AiBlockAwaitingConfirm,
                    FocusSnap::SettledAiTarget,
                ] {
                    let facts = RouteFacts {
                        approval_card: focus == FocusSnap::ApprovalCard,
                        ai_awaiting_confirmation: focus == FocusSnap::AiBlockAwaitingConfirm,
                        settled_ai_target: focus == FocusSnap::SettledAiTarget,
                        focus,
                        ..facts
                    };
                    for class in [Class::Typed, Class::Paste] {
                        note(decide(
                            &Intent::new(class, Origin::Keyboard).stamped(at(1, focus)),
                            &facts,
                            None,
                        ));
                    }
                }
                // A line pinned to the editor.
                for class in [Class::Typed, Class::Paste, Class::Control, Class::Seq] {
                    note(decide(
                        &Intent::new(class, Origin::Keyboard).stamped(stamp),
                        &facts,
                        Some(Destination::Editor),
                    ));
                }
            }
        }
    }

    // The rows that are effects or bypasses, never the row of a plan.
    let effects_only = ["H3b", "H7b", "P1b", "D4"];
    for row in [
        Row::OSynced,
        Row::OCliSubmit,
        Row::H1,
        Row::H2,
        Row::H3,
        Row::H3b,
        Row::H5,
        Row::H6,
        Row::H7,
        Row::H7b,
        Row::H8,
        Row::H8Queue,
        Row::H10,
        Row::P1a,
        Row::P1b,
        Row::P1c,
        Row::P1d,
        Row::P1e,
        Row::P1f,
        Row::P2,
        Row::D1,
        Row::D1Replay,
        Row::D2,
        Row::D2b,
        Row::D3,
        Row::D4,
        Row::D5,
        Row::D6,
        Row::D7,
        Row::D9,
        Row::D10,
        Row::D11,
        Row::D12,
        Row::D13,
        Row::Pin,
    ] {
        let name = row.name();
        assert_eq!(
            rows.contains(name),
            !effects_only.contains(&name),
            "row {name}: reached={} over {decisions} decisions",
            rows.contains(name)
        );
    }
    // Every outcome comes with a row, and each of the 18 kinds is produced by some combination.
    let kinds = [
        discriminant(&Outcome::Pty(PtyWrite::Typed)),
        discriminant(&Outcome::PasteToPty { bracketed: false }),
        discriminant(&Outcome::SignalToPty(SignalKind::Interrupt)),
        discriminant(&Outcome::SigintForeground),
        discriminant(&Outcome::AbortBytes),
        discriminant(&Outcome::EditorInsert),
        discriminant(&Outcome::EditorAction),
        discriminant(&Outcome::EditorSubmit),
        discriminant(&Outcome::AgentInput),
        discriminant(&Outcome::Hold(GateKind::SshConnect)),
        discriminant(&Outcome::HoldSignal),
        discriminant(&Outcome::Queue),
        discriminant(&Outcome::SignalAtGates),
        discriminant(&Outcome::Stash),
        discriminant(&Outcome::PowershellAbort),
        discriminant(&Outcome::Hint),
        discriminant(&Outcome::Swallow(SwallowReason::SshDialogOpen)),
        discriminant(&Outcome::Drop(DropWhy::AgentControl)),
    ];
    for kind in kinds {
        let rows_of_kind = outcomes.get(&kind);
        assert!(
            rows_of_kind.is_some_and(|rows| !rows.is_empty()),
            "an outcome kind was never produced, or came with no row: {kind:?}"
        );
    }
    assert_eq!(
        outcomes.len(),
        kinds.len(),
        "an outcome kind this test does not know"
    );
}
