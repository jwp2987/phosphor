//! Enumerated totality of the destination table (#1424, CI review item 3 of #1531).
//!
//! `table_tests.rs` samples `RouteFacts` with proptest (2048 random cases per property). The facts
//! are a small product of booleans and enums, so this file ENUMERATES the part of that product
//! the rows branch on and adds a deterministic pseudo-random sweep over all of it:
//!
//! * `decide` is total (it never panics) and deterministic for every sample;
//! * every outcome maps to a row that can give it (the drop reasons name their rows, a refused
//!   pty write drops only a typed write, signals never wait, the oracle is consulted exactly for
//!   entries not released from a hand-off gate, the editor never reaches the pty, the paced
//!   CLI-agent submit is never held);
//! * every `Row` is reachable by some sample (a row nothing can reach is dead code in the table).
//!
//! The sample space is fixed, so a failure is reproducible from the message alone (it prints the
//! intent, the facts and the pin).

use crate::terminal::input_queue::table::{
    Class, DropWhy, Effects, Intent, OnRefused, Outcome, Plan, PtyWrite, Row, decide,
};
use crate::terminal::input_queue::{
    Destination, EntryId, FocusSnap, GateKind, GateMask, Origin, PsPhase, RouteFacts, SignalKind,
    SshPhase, Stamp, SubshellPhase,
};

/// Every row of the table, in declaration order.
const ALL_ROWS: [Row; 36] = [
    Row::OSynced,
    Row::OCliSubmit,
    Row::H1,
    Row::H2,
    Row::H3,
    Row::H3b,
    Row::H4,
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
];

const CLASSES: [Class; 12] = [
    Class::Typed,
    Class::Enter,
    Class::Control,
    Class::Seq,
    Class::Paste,
    Class::Raw,
    Class::AgentWrite,
    Class::EditKey,
    Class::Signal(SignalKind::Interrupt),
    Class::Signal(SignalKind::Eof),
    Class::Signal(SignalKind::Suspend),
    Class::Signal(SignalKind::Quit),
];

fn is_signal(class: Class) -> bool {
    matches!(class, Class::Signal(_))
}

fn all_origins() -> [Origin; 11] {
    [
        Origin::Keyboard,
        Origin::Editor,
        Origin::Replay(EntryId(1)),
        Origin::Synthesized,
        Origin::Held,
        Origin::Synced,
        Origin::Viewer,
        Origin::CliSubmit,
        Origin::Agent,
        Origin::ReleasedFrom(GateKind::LineEditorActive),
        Origin::ReleasedFrom(GateKind::HandoffReport),
    ]
}

fn ssh_of(index: usize) -> SshPhase {
    match index % 3 {
        0 => SshPhase::Idle,
        1 => SshPhase::Holding,
        _ => SshPhase::PromptUp { prompt_gen: 3 },
    }
}

fn subshell_of(index: usize) -> SubshellPhase {
    [
        SubshellPhase::Idle,
        SubshellPhase::Announced,
        SubshellPhase::SetupHeld,
        SubshellPhase::BootstrapWriting,
    ][index % 4]
}

fn ps_of(index: usize) -> PsPhase {
    [
        PsPhase::Off,
        PsPhase::Setup { holds_keys: true },
        PsPhase::Setup { holds_keys: false },
        PsPhase::LineRunning,
        PsPhase::AfterAbort,
    ][index % 5]
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

fn pin_of(index: usize) -> Option<Destination> {
    [
        None,
        Some(Destination::Editor),
        Some(Destination::Pty),
        Some(Destination::AgentInput),
    ][index % 4]
}

/// Checks every property of one decision and returns the plan.
fn check(intent: &Intent, facts: &RouteFacts, pin: Option<Destination>) -> Plan {
    let plan = decide(intent, facts, pin);
    assert_eq!(
        plan,
        decide(intent, facts, pin),
        "not deterministic: {intent:?} / {facts:?} / {pin:?}"
    );
    assert!(
        !plan.row.name().is_empty(),
        "a plan with an unnamed row: {plan:?}"
    );

    // Every drop names its reason, and the reason names a row that can drop for it.
    if let Outcome::Drop(why) = plan.outcome {
        let allowed: &[Row] = match why {
            DropWhy::StaleBeforePrompt => &[Row::H4],
            DropWhy::AgentRefusedSetup => &[Row::D13],
            DropWhy::AgentControl => &[Row::D2b, Row::D3, Row::D5],
            DropWhy::NotLongRunning => &[Row::D10, Row::OSynced],
            DropWhy::RefusedHiddenInput => &[Row::D5],
        };
        assert!(
            allowed.contains(&plan.row),
            "{why:?} dropped by {:?}: {intent:?} / {facts:?}",
            plan.row
        );
    }
    if let OnRefused::Drop(_) = plan.on_refused {
        assert_eq!(
            plan.outcome,
            Outcome::Pty(PtyWrite::Typed),
            "only a typed pty write can be refused into a drop: {intent:?} / {facts:?}"
        );
    }

    // Signals never wait.
    if is_signal(intent.class) {
        assert!(
            !matches!(plan.outcome, Outcome::Hold(_) | Outcome::Queue),
            "a signal waited: {plan:?} for {intent:?} / {facts:?}"
        );
        if plan.outcome == Outcome::HoldSignal {
            assert_eq!(plan.row, Row::P1a, "{intent:?} / {facts:?}");
        }
        if intent.origin == Origin::Held {
            assert_eq!(plan.row, Row::P1f, "{intent:?} / {facts:?}");
        }
    }

    // The properties below are stated for an unpinned line (the pin only rewrites D6).
    if pin.is_none() {
        let pty_write = matches!(
            plan.outcome,
            Outcome::Pty(_) | Outcome::PasteToPty { .. } | Outcome::SignalToPty(_)
        );
        if pty_write && plan.row != Row::Pin {
            assert_eq!(
                plan.effects.contains(Effects::CONSULT_ORACLE),
                !matches!(intent.origin, Origin::ReleasedFrom(_)),
                "oracle consultation: {plan:?} for {intent:?} / {facts:?}"
            );
        }
        if intent.origin == Origin::CliSubmit
            && !is_signal(intent.class)
            && intent.class != Class::AgentWrite
        {
            assert!(
                !plan.outcome.is_hold()
                    && plan.outcome != Outcome::Hint
                    && !matches!(plan.outcome, Outcome::Swallow(_)),
                "the CLI-agent submit was held: {plan:?} for {intent:?} / {facts:?}"
            );
        }
        if intent.origin == Origin::Editor
            && !is_signal(intent.class)
            && intent.class != Class::AgentWrite
        {
            assert!(
                !plan.outcome.is_pty(),
                "the editor reached the pty: {plan:?} for {intent:?} / {facts:?}"
            );
        }
    }
    plan
}

/// Facts from 11 enumerated booleans and the three phases. The rest stays at its default (an
/// idle shell), the random sweep varies it. Built by assignment so a field added to
/// `RouteFacts` does not break this file.
fn facts_from(bits: u32, ssh: usize, subshell: usize, ps: usize, gates: GateMask) -> RouteFacts {
    let bit = |n: u32| bits & (1 << n) != 0;
    let mut f = RouteFacts::default();
    f.model_consistent = true;
    f.handoff_known = true;
    f.alt_screen = bit(0);
    f.agent_in_control = bit(1);
    f.agent_owns_pty = bit(2);
    f.long_running = bit(3);
    f.active_block_started = bit(4);
    f.session_pending_or_bootstrapped = bit(5);
    f.handoff_routes = bit(6);
    f.input_box_visible = bit(7);
    f.editor_focused = bit(8);
    f.queued = bit(9);
    f.ssh_hold_exists = bit(10);
    f.ssh = ssh_of(ssh);
    f.subshell = subshell_of(subshell);
    f.ps = ps_of(ps);
    f.gates = gates;
    f
}

/// xorshift64*: a fixed-seed sweep, so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        ((self.next() >> 33) as usize) % n
    }

    fn flag(&mut self) -> bool {
        self.next() >> 63 == 1
    }
}

fn random_facts(rng: &mut Rng) -> RouteFacts {
    let generation = rng.below(8) as u64;
    let mut f = RouteFacts::default();
    f.facts_gen = generation;
    f.model_consistent = rng.flag();
    f.alt_screen = rng.flag();
    f.agent_in_control = rng.flag();
    f.agent_in_control_exact = rng.flag();
    f.agent_owns_pty = rng.flag();
    f.long_running = rng.flag();
    f.read_only = rng.flag();
    f.is_bootstrapped = rng.flag();
    f.needs_bracketed_paste = rng.flag();
    f.active_block_started = rng.flag();
    f.shared_session_executor = rng.flag();
    f.session_pending_or_bootstrapped = rng.flag();
    f.has_pending_ssh_command = rng.flag();
    f.handoff_routes = rng.flag();
    f.handoff_known = rng.flag();
    f.subshell = subshell_of(rng.below(4));
    f.forwards_subshell_key = rng.flag();
    f.forwards_subshell_abort = rng.flag();
    f.subshell_awaiting_answer = rng.flag();
    f.subshell_bootstrap_stalled = rng.flag();
    f.ps = ps_of(rng.below(5));
    f.ssh = match rng.below(3) {
        0 => SshPhase::Idle,
        1 => SshPhase::Holding,
        _ => SshPhase::PromptUp {
            prompt_gen: generation,
        },
    };
    f.ssh_dialog_up = rng.flag();
    f.ssh_setup_in_flight = rng.flag();
    f.ssh_bootstrapping = rng.flag();
    f.ssh_connecting_silently = rng.flag();
    f.ssh_interrupt_must_wait = rng.flag();
    f.ssh_eot_must_wait = rng.flag();
    f.ssh_hold_exists = rng.flag();
    f.queue_has_ssh_gate = rng.flag();
    f.ssh_route_forward = rng.flag();
    f.ssh_pre_init_abort = rng.flag();
    f.ssh_sigint_applies = rng.flag();
    f.focus = focus_of(rng.below(7));
    f.approval_card = rng.flag();
    f.ai_awaiting_confirmation = rng.flag();
    f.settled_ai_target = rng.flag();
    f.input_box_visible = rng.flag();
    f.editor_focused = rng.flag();
    f.editor_takeover_open = rng.flag();
    f.blocks_selected = rng.flag();
    f.buffer_empty = rng.flag();
    f.cli_agent_session = rng.flag();
    // One bit per gate kind; the mask is never wider than the kinds there are.
    f.gates = GateMask((rng.next() as u16) & ((1 << GateKind::ALL.len()) - 1));
    f.queued = rng.flag();
    f.queued_only_pty = rng.flag();
    f.mailbox_non_empty = rng.flag();
    f.agent_write_gate_closed = rng.flag();
    f
}

fn random_intent(rng: &mut Rng) -> Intent {
    let origins = all_origins();
    let mut intent = Intent::new(
        CLASSES[rng.below(CLASSES.len())],
        origins[rng.below(origins.len())],
    );
    intent.stamp = Stamp {
        facts_gen: rng.below(8) as u64,
        focus: focus_of(rng.below(7)),
        epoch: 0,
    };
    intent.takes_over_agent = rng.flag();
    intent.delivering = rng.flag();
    intent.needs_display_map = rng.flag();
    intent.display_map_fresh = rng.flag();
    intent.flushing = rng.flag();
    intent
}

/// Runs `visit` over every sample: the enumerated products, then the sweep.
fn for_each_sample(mut visit: impl FnMut(&Intent, &RouteFacts, Option<Destination>)) {
    // 1. No gate: 11 booleans x ssh x (idle, setup-held subshell), every class, for the two
    //    origins that carry most rows.
    let origins = [Origin::Keyboard, Origin::Editor];
    for bits in 0..(1u32 << 11) {
        for ssh in 0..3 {
            for subshell in [0, 2] {
                let facts = facts_from(bits, ssh, subshell, 0, GateMask::EMPTY);
                for class in CLASSES {
                    for origin in origins {
                        visit(&Intent::new(class, origin), &facts, None);
                    }
                }
            }
        }
    }

    // 2. Every gate kind, alone and next to the ssh gate: a smaller fact product, four origins.
    let gate_origins = [
        Origin::Keyboard,
        Origin::Editor,
        Origin::Held,
        Origin::ReleasedFrom(GateKind::HandoffReport),
    ];
    let mut gate_sets = vec![GateMask::EMPTY];
    for kind in GateKind::ALL {
        gate_sets.push(GateMask::EMPTY.with(kind));
        gate_sets.push(GateMask::EMPTY.with(kind).with(GateKind::SshConnect));
    }
    for gates in gate_sets {
        // agent_owns_pty, queued, input_box_visible, editor_focused, queued_only_pty.
        for small in 0..32u32 {
            let bit = |n: u32| (small >> n) & 1;
            let bits = (bit(0) << 2) | (bit(1) << 9) | (bit(2) << 7) | (bit(3) << 8);
            for ssh in 0..3 {
                for ps in [0, 1] {
                    let mut facts = facts_from(bits, ssh, 0, ps, gates);
                    facts.queued_only_pty = bit(4) == 1;
                    for class in CLASSES {
                        for origin in gate_origins {
                            visit(&Intent::new(class, origin), &facts, None);
                        }
                    }
                }
            }
        }
    }

    // 3. The sweep over everything, including pins and every intent flag.
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..300_000 {
        let facts = random_facts(&mut rng);
        let intent = random_intent(&mut rng);
        let pin = pin_of(rng.below(4));
        visit(&intent, &facts, pin);
    }
}

/// `decide` has a plan for every sample, and the plan satisfies every table property.
#[test]
fn decide_is_total_and_consistent_over_the_enumerated_facts() {
    let mut samples = 0u64;
    for_each_sample(|intent, facts, pin| {
        check(intent, facts, pin);
        samples += 1;
    });
    assert!(samples > 800_000, "the enumeration shrank: {samples}");
}

/// Every row of the table is decided by some sample. A row nothing can reach is dead: either the
/// row is unreachable by construction (remove it) or the facts that should reach it are never
/// built (a bug in what feeds the table).
#[test]
fn every_row_of_the_table_is_reachable() {
    let mut seen: Vec<Row> = Vec::new();
    for_each_sample(|intent, facts, pin| {
        let row = decide(intent, facts, pin).row;
        if !seen.contains(&row) {
            seen.push(row);
        }
    });
    let missing: Vec<&'static str> = ALL_ROWS
        .iter()
        .filter(|row| !seen.contains(row))
        .map(|row| row.name())
        .collect();
    assert!(
        missing.is_empty(),
        "rows no sample reaches: {missing:?} (see {} rows)",
        ALL_ROWS.len()
    );
}

/// The row names are unique (the shadow log and the matrix address rows by name) and the list
/// above is the whole enum: a row added to `Row` without being added to `ALL_ROWS` is found by
/// the exhaustive match, which fails to compile.
#[test]
fn row_names_are_unique_and_the_list_is_complete() {
    fn index(row: Row) -> usize {
        match row {
            Row::OSynced => 0,
            Row::OCliSubmit => 1,
            Row::H1 => 2,
            Row::H2 => 3,
            Row::H3 => 4,
            Row::H3b => 5,
            Row::H4 => 6,
            Row::H5 => 7,
            Row::H6 => 8,
            Row::H7 => 9,
            Row::H7b => 10,
            Row::H8 => 11,
            Row::H8Queue => 12,
            Row::H10 => 13,
            Row::P1a => 14,
            Row::P1b => 15,
            Row::P1c => 16,
            Row::P1d => 17,
            Row::P1e => 18,
            Row::P1f => 19,
            Row::P2 => 20,
            Row::D1 => 21,
            Row::D1Replay => 22,
            Row::D2 => 23,
            Row::D2b => 24,
            Row::D3 => 25,
            Row::D4 => 26,
            Row::D5 => 27,
            Row::D6 => 28,
            Row::D7 => 29,
            Row::D9 => 30,
            Row::D10 => 31,
            Row::D11 => 32,
            Row::D12 => 33,
            Row::D13 => 34,
            Row::Pin => 35,
        }
    }
    for (position, row) in ALL_ROWS.iter().enumerate() {
        assert_eq!(index(*row), position, "ALL_ROWS is out of order at {row:?}");
    }
    let mut names: Vec<&str> = ALL_ROWS.iter().map(|row| row.name()).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "two rows share a name");
}
