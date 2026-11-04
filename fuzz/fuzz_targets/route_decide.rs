//! The destination table (#1424) over arbitrary facts and intents: `table::decide` must never
//! panic, must be deterministic, and every plan must map to a row that can give it (the drop
//! reasons name their rows, a refused pty write drops only a typed write, signals never wait, the
//! hand-off oracle is consulted exactly for entries not released from one of its gates, the
//! editor never reaches the pty, the paced CLI-agent submit is never held). The same properties
//! as `input_queue/table_tests.rs` and `table_enum_tests.rs`, over inputs the fuzzer chooses.
//!
//! The input is read with `Unstructured` (no derive on the app's types): every field comes from
//! the bytes in a fixed order, so a crash artifact replays exactly. A short input reads as zeros.
#![no_main]

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use warp::dev_support::table::{
    Class, DropWhy, Effects, Intent, OnRefused, Outcome, PtyWrite, Row, decide,
};
use warp::dev_support::{
    Destination, EntryId, FocusSnap, GateKind, GateMask, Origin, PsPhase, RouteFacts, SignalKind,
    SshPhase, Stamp, SubshellPhase,
};

fn flag(u: &mut Unstructured) -> bool {
    u.arbitrary::<bool>().unwrap_or(false)
}

fn pick(u: &mut Unstructured, n: usize) -> usize {
    u.int_in_range(0..=n - 1).unwrap_or(0)
}

fn class(u: &mut Unstructured) -> Class {
    match pick(u, 12) {
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

fn origin(u: &mut Unstructured) -> Origin {
    match pick(u, 11) {
        0 => Origin::Keyboard,
        1 => Origin::Editor,
        2 => Origin::Replay(EntryId(u.arbitrary::<u64>().unwrap_or(0))),
        3 => Origin::Synthesized,
        4 => Origin::Held,
        5 => Origin::Synced,
        6 => Origin::Viewer,
        7 => Origin::CliSubmit,
        8 => Origin::Agent,
        9 => Origin::ReleasedFrom(GateKind::ALL[pick(u, GateKind::ALL.len())]),
        _ => Origin::ReleasedFrom(GateKind::HandoffReport),
    }
}

fn focus(u: &mut Unstructured) -> FocusSnap {
    [
        FocusSnap::Other,
        FocusSnap::Terminal,
        FocusSnap::Editor,
        FocusSnap::SshChoiceCard,
        FocusSnap::ApprovalCard,
        FocusSnap::AiBlockAwaitingConfirm,
        FocusSnap::SettledAiTarget,
    ][pick(u, 7)]
}

fn intent(u: &mut Unstructured) -> Intent {
    let mut intent = Intent::new(class(u), origin(u));
    intent.stamp = Stamp {
        facts_gen: u.arbitrary::<u64>().unwrap_or(0),
        focus: focus(u),
        epoch: u.arbitrary::<u64>().unwrap_or(0),
    };
    intent.takes_over_agent = flag(u);
    intent.delivering = flag(u);
    intent.needs_display_map = flag(u);
    intent.display_map_fresh = flag(u);
    intent.flushing = flag(u);
    intent
}

fn facts(u: &mut Unstructured) -> RouteFacts {
    let mut f = RouteFacts::default();
    f.facts_gen = u.arbitrary::<u64>().unwrap_or(0);
    f.model_consistent = flag(u);
    f.alt_screen = flag(u);
    f.agent_in_control = flag(u);
    f.agent_in_control_exact = flag(u);
    f.agent_owns_pty = flag(u);
    f.long_running = flag(u);
    f.read_only = flag(u);
    f.is_bootstrapped = flag(u);
    f.needs_bracketed_paste = flag(u);
    f.active_block_started = flag(u);
    f.shared_session_executor = flag(u);
    f.session_pending_or_bootstrapped = flag(u);
    f.has_pending_ssh_command = flag(u);
    f.handoff_routes = flag(u);
    f.handoff_known = flag(u);
    f.subshell = [
        SubshellPhase::Idle,
        SubshellPhase::Announced,
        SubshellPhase::SetupHeld,
        SubshellPhase::BootstrapWriting,
    ][pick(u, 4)];
    f.forwards_subshell_key = flag(u);
    f.forwards_subshell_abort = flag(u);
    f.subshell_awaiting_answer = flag(u);
    f.subshell_bootstrap_stalled = flag(u);
    f.ps = [
        PsPhase::Off,
        PsPhase::Setup { holds_keys: true },
        PsPhase::Setup { holds_keys: false },
        PsPhase::LineRunning,
        PsPhase::AfterAbort,
    ][pick(u, 5)];
    f.ssh = match pick(u, 2) {
        0 => SshPhase::Idle,
        _ => SshPhase::Holding,
    };
    f.ssh_dialog_up = flag(u);
    f.ssh_setup_in_flight = flag(u);
    f.ssh_bootstrapping = flag(u);
    f.ssh_connecting_silently = flag(u);
    f.ssh_interrupt_must_wait = flag(u);
    f.ssh_eot_must_wait = flag(u);
    f.ssh_hold_exists = flag(u);
    f.queue_has_ssh_gate = flag(u);
    f.ssh_route_forward = flag(u);
    f.ssh_pre_init_abort = flag(u);
    f.ssh_sigint_applies = flag(u);
    f.focus = focus(u);
    f.approval_card = flag(u);
    f.ai_awaiting_confirmation = flag(u);
    f.settled_ai_target = flag(u);
    f.input_box_visible = flag(u);
    f.editor_focused = flag(u);
    f.editor_takeover_open = flag(u);
    f.blocks_selected = flag(u);
    f.buffer_empty = flag(u);
    f.cli_agent_session = flag(u);
    f.gates = GateMask(u.arbitrary::<u16>().unwrap_or(0) & ((1 << GateKind::ALL.len()) - 1));
    f.queued = flag(u);
    f.queued_only_pty = flag(u);
    f.mailbox_non_empty = flag(u);
    f.agent_write_gate_closed = flag(u);
    f
}

fn is_signal(class: Class) -> bool {
    matches!(class, Class::Signal(_))
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let intent = intent(&mut u);
    let facts = facts(&mut u);
    let pin = [
        None,
        Some(Destination::Editor),
        Some(Destination::Pty),
        Some(Destination::AgentInput),
    ][pick(&mut u, 4)];

    let plan = decide(&intent, &facts, pin);
    assert_eq!(plan, decide(&intent, &facts, pin), "not deterministic");
    assert!(!plan.row.name().is_empty(), "a plan with an unnamed row");

    if let Outcome::Drop(why) = plan.outcome {
        let allowed: &[Row] = match why {
            DropWhy::AgentRefusedSetup => &[Row::D13],
            DropWhy::AgentControl => &[Row::D2b, Row::D3, Row::D5],
            DropWhy::NotLongRunning => &[Row::D10, Row::OSynced],
            DropWhy::RefusedHiddenInput => &[Row::D5],
        };
        assert!(
            allowed.contains(&plan.row),
            "{why:?} dropped by {:?}",
            plan.row
        );
    }
    if let OnRefused::Drop(_) = plan.on_refused {
        assert_eq!(plan.outcome, Outcome::Pty(PtyWrite::Typed));
    }
    if is_signal(intent.class) {
        assert!(
            !matches!(plan.outcome, Outcome::Hold(_) | Outcome::Queue),
            "a signal waited: {plan:?}"
        );
        if plan.outcome == Outcome::HoldSignal {
            assert_eq!(plan.row, Row::P1a);
        }
        if intent.origin == Origin::Held {
            assert_eq!(plan.row, Row::P1f);
        }
    }
    if pin.is_none() {
        let pty_write = matches!(
            plan.outcome,
            Outcome::Pty(_) | Outcome::PasteToPty { .. } | Outcome::SignalToPty(_)
        );
        if pty_write && plan.row != Row::Pin {
            assert_eq!(
                plan.effects.contains(Effects::CONSULT_ORACLE),
                !matches!(intent.origin, Origin::ReleasedFrom(_)),
                "oracle consultation: {plan:?}"
            );
        }
        // #1601b: held by an open PowerShell setup (paced or not); a PACED submit (its `CliSubmit`
        // gate is open) by nothing else; an UNPACED one passes only an idle queue (or an ssh-only
        // gate set with nothing queued) outside a subshell setup, and waits behind any other gate.
        if intent.origin == Origin::CliSubmit
            && !is_signal(intent.class)
            && intent.class != Class::AgentWrite
        {
            let paced = facts.gates.has(GateKind::CliSubmit);
            let ssh_only = facts.gates.all_within(&[GateKind::SshConnect]);
            let setup_held = matches!(
                facts.subshell,
                SubshellPhase::SetupHeld | SubshellPhase::BootstrapWriting
            );
            let not_held = !plan.outcome.is_hold()
                && plan.outcome != Outcome::Hint
                && !matches!(plan.outcome, Outcome::Swallow(_));
            if facts.ps.holds_keys() {
                assert_eq!(
                    plan.outcome,
                    Outcome::Hold(GateKind::PowershellSetup),
                    "{plan:?}"
                );
            } else if paced {
                assert!(not_held, "the paced CLI-agent submit was held: {plan:?}");
            } else if (facts.gates.is_empty() || ssh_only)
                && !facts.queued
                && !facts.mailbox_non_empty
                && !setup_held
            {
                assert!(not_held, "the idle CLI-agent submit was held: {plan:?}");
            } else if !facts.subshell_awaiting_answer
                && ((!facts.gates.is_empty() && !ssh_only) || setup_held)
            {
                assert!(
                    plan.outcome.is_hold(),
                    "the unpaced CLI-agent submit passed a gate: {plan:?}"
                );
            }
        }
        if intent.origin == Origin::Editor
            && !is_signal(intent.class)
            && intent.class != Class::AgentWrite
        {
            assert!(
                !plan.outcome.is_pty(),
                "the editor reached the pty: {plan:?}"
            );
        }
    }
});
