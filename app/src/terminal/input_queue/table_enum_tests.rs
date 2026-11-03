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
//!
//! #1530 adds `exhaustive_*` below: enumerations, not samples. `RouteFacts` has ~42 booleans, four
//! small enums and a 12-bit gate mask (about 2^60 combinations), so the full product cannot be
//! walked. The reduction is stated in each test: (1) EVERY gate mask x ssh phase x every class x
//! every origin; (2) EVERY subshell x ps x ssh phase x all 11 core booleans x every class; (3)
//! EVERY PAIR of values of ANY two dimensions (all booleans, all enum values, all gate bits, the
//! intent flags, four stamps) on three base fact sets, x every class x origin x two pins. The
//! rows branch on at most a handful of facts each, so two-way coverage plus the exhaustive
//! blocks for the dimensions that interact most (gates, phases) is the decisive reduction; rows
//! that need three simultaneous odd facts are reached by the existing samples above, which the
//! reachability test unions in.

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

    // 4. The rows the products above cannot reach (a random sweep reaches them too rarely to
    //    count on): H10 (an editor key that reads the display map, behind no gate), D1 (the
    //    approval card or a confirming AI block had focus at the sink), D11 (a replayed or held
    //    Enter), and the pin (a line pinned to the editor). No gate is open and the subshell is
    //    idle, so the holds before the destination rows do not take the sample.
    let at_approval_card = Stamp {
        facts_gen: 0,
        focus: FocusSnap::ApprovalCard,
        epoch: 0,
    };
    let content_origins = [
        Origin::Keyboard,
        Origin::Editor,
        Origin::Replay(EntryId(1)),
        Origin::Held,
    ];
    for bits in 0..(1u32 << 11) {
        for ssh in 0..3 {
            let facts = facts_from(bits, ssh, 0, 0, GateMask::EMPTY);
            for class in [Class::Typed, Class::Paste, Class::Enter, Class::EditKey] {
                // The three display-map flags only matter for an editor key.
                let flag_sets = if class == Class::EditKey { 8 } else { 1 };
                for origin in content_origins {
                    for flags in 0..flag_sets {
                        for stamp in [Stamp::UNSTAMPED, at_approval_card] {
                            let mut intent = Intent::new(class, origin).stamped(stamp);
                            intent.needs_display_map = flags & 1 != 0;
                            intent.display_map_fresh = flags & 2 != 0;
                            intent.flushing = flags & 4 != 0;
                            visit(&intent, &facts, None);
                        }
                    }
                }
            }
            for class in [Class::Typed, Class::Paste, Class::Control, Class::Seq] {
                visit(
                    &Intent::new(class, Origin::Keyboard),
                    &facts,
                    Some(Destination::Editor),
                );
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

/// The rows of the matrix that are effects or bypasses on another row's plan, never the row of a
/// plan themselves (the table's own doc comment and `table_tests.rs` say so): `H3b` and `H7b` are
/// skipped holds, `P1b` an effect of the interrupt rows, `D4` the take-over effect of D5/D6.
const EFFECT_ONLY_ROWS: [Row; 4] = [Row::H3b, Row::H7b, Row::P1b, Row::D4];

/// Every row of the table is decided by some sample, except the effect-only rows, which no sample
/// may ever decide. A row nothing can reach is dead: either the row is unreachable by
/// construction (remove it) or the facts that should reach it are never built (a bug in what
/// feeds the table).
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
        .filter(|row| !EFFECT_ONLY_ROWS.contains(row) && !seen.contains(row))
        .map(|row| row.name())
        .collect();
    assert!(
        missing.is_empty(),
        "rows no sample reaches: {missing:?} (see {} rows)",
        ALL_ROWS.len()
    );
    let decided: Vec<&'static str> = EFFECT_ONLY_ROWS
        .iter()
        .filter(|row| seen.contains(row))
        .map(|row| row.name())
        .collect();
    assert!(
        decided.is_empty(),
        "effect-only rows that a plan names as its row: {decided:?}"
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

// ---- #1530: enumerated, not sampled ------------------------------------------------------------

/// One point of the enumeration: the facts, the five intent flags and the stamp.
#[derive(Clone, Copy)]
struct Point {
    facts: RouteFacts,
    /// takes_over_agent, delivering, needs_display_map, display_map_fresh, flushing.
    flags: [bool; 5],
    stamp: Stamp,
}

impl Point {
    fn intent(&self, class: Class, origin: Origin) -> Intent {
        let mut intent = Intent::new(class, origin).stamped(self.stamp);
        intent.takes_over_agent = self.flags[0];
        intent.delivering = self.flags[1];
        intent.needs_display_map = self.flags[2];
        intent.display_map_fresh = self.flags[3];
        intent.flushing = self.flags[4];
        intent
    }
}

/// One enumerated dimension: `values` settings, applied by `set`.
struct Dim {
    values: usize,
    set: fn(&mut Point, usize),
}

macro_rules! bool_dim {
    ($($field:ident),* $(,)?) => {
        [$(Dim { values: 2, set: |p, v| p.facts.$field = v == 1 }),*]
    };
}

fn stamp_of(index: usize) -> Stamp {
    match index % 4 {
        0 => Stamp::UNSTAMPED,
        1 => Stamp {
            facts_gen: 5,
            focus: FocusSnap::Terminal,
            epoch: 0,
        },
        2 => Stamp {
            facts_gen: 0,
            focus: FocusSnap::ApprovalCard,
            epoch: 0,
        },
        _ => Stamp {
            facts_gen: 0,
            focus: FocusSnap::Editor,
            epoch: 0,
        },
    }
}

/// Every dimension the table can branch on: all 42 boolean facts, the four enum facts, the 12 gate
/// bits, the five intent flags and the stamp.
fn all_dims() -> Vec<Dim> {
    let mut dims: Vec<Dim> = Vec::new();
    dims.extend(bool_dim![
        model_consistent,
        alt_screen,
        agent_in_control,
        agent_in_control_exact,
        agent_owns_pty,
        long_running,
        read_only,
        is_bootstrapped,
        needs_bracketed_paste,
        active_block_started,
        shared_session_executor,
        session_pending_or_bootstrapped,
        has_pending_ssh_command,
        handoff_routes,
        handoff_known,
        forwards_subshell_key,
        forwards_subshell_abort,
        subshell_awaiting_answer,
        subshell_bootstrap_stalled,
        ssh_dialog_up,
        ssh_setup_in_flight,
        ssh_bootstrapping,
        ssh_connecting_silently,
        ssh_interrupt_must_wait,
        ssh_eot_must_wait,
        ssh_hold_exists,
        queue_has_ssh_gate,
        ssh_route_forward,
        ssh_pre_init_abort,
        ssh_sigint_applies,
        approval_card,
        ai_awaiting_confirmation,
        settled_ai_target,
        input_box_visible,
        editor_focused,
        editor_takeover_open,
        blocks_selected,
        buffer_empty,
        cli_agent_session,
        queued,
        queued_only_pty,
        mailbox_non_empty,
        agent_write_gate_closed,
    ]);
    dims.push(Dim {
        values: 3,
        set: |p, v| p.facts.ssh = ssh_of(v),
    });
    dims.push(Dim {
        values: 4,
        set: |p, v| p.facts.subshell = subshell_of(v),
    });
    dims.push(Dim {
        values: 5,
        set: |p, v| p.facts.ps = ps_of(v),
    });
    dims.push(Dim {
        values: 7,
        set: |p, v| p.facts.focus = focus_of(v),
    });
    dims.push(Dim {
        values: 4,
        set: |p, v| p.stamp = stamp_of(v),
    });
    dims.push(Dim {
        values: 2,
        set: |p, v| p.flags[0] = v == 1,
    });
    dims.push(Dim {
        values: 2,
        set: |p, v| p.flags[1] = v == 1,
    });
    dims.push(Dim {
        values: 2,
        set: |p, v| p.flags[2] = v == 1,
    });
    dims.push(Dim {
        values: 2,
        set: |p, v| p.flags[3] = v == 1,
    });
    dims.push(Dim {
        values: 2,
        set: |p, v| p.flags[4] = v == 1,
    });
    // One dimension per gate kind (the mask bit), so every pair of gates is covered too.
    macro_rules! gate_dim {
        ($index:expr) => {
            dims.push(Dim {
                values: 2,
                set: |p, v| {
                    let kind = GateKind::ALL[$index];
                    p.facts.gates = if v == 1 {
                        p.facts.gates.with(kind)
                    } else {
                        GateMask(p.facts.gates.0 & !GateMask::EMPTY.with(kind).0)
                    };
                },
            });
        };
    }
    gate_dim!(0);
    gate_dim!(1);
    gate_dim!(2);
    gate_dim!(3);
    gate_dim!(4);
    gate_dim!(5);
    gate_dim!(6);
    gate_dim!(7);
    gate_dim!(8);
    gate_dim!(9);
    gate_dim!(10);
    gate_dim!(11);
    assert_eq!(GateKind::ALL.len(), 12, "add a gate_dim! for the new kind");
    dims
}

/// The three base points the pairs are applied to: a shell with nothing known, a running command
/// the pty takes input for, and an idle prompt with the input box up.
fn base_points() -> [Point; 3] {
    let point = |facts: RouteFacts| Point {
        facts,
        flags: [true, false, false, false, false],
        stamp: Stamp::UNSTAMPED,
    };
    let running = RouteFacts {
        model_consistent: true,
        handoff_known: true,
        long_running: true,
        active_block_started: true,
        session_pending_or_bootstrapped: true,
        is_bootstrapped: true,
        ..RouteFacts::default()
    };
    let prompt = RouteFacts {
        model_consistent: true,
        handoff_known: true,
        session_pending_or_bootstrapped: true,
        is_bootstrapped: true,
        input_box_visible: true,
        editor_focused: true,
        ..RouteFacts::default()
    };
    [point(RouteFacts::default()), point(running), point(prompt)]
}

/// Which rows the enumeration decided, by their position in `ALL_ROWS`.
struct Tally {
    seen: [bool; 36],
    decisions: u64,
}

impl Tally {
    fn new() -> Self {
        Self {
            seen: [false; 36],
            decisions: 0,
        }
    }

    /// Checks one decision and counts its row. Every plan carries exactly one row of the table.
    fn decide(&mut self, intent: &Intent, facts: &RouteFacts, pin: Option<Destination>) {
        let plan = check(intent, facts, pin);
        let position = ALL_ROWS
            .iter()
            .position(|row| *row == plan.row)
            .unwrap_or_else(|| panic!("a plan names a row outside the table: {plan:?}"));
        self.seen[position] = true;
        self.decisions += 1;
    }

    fn rows(&self) -> impl Iterator<Item = Row> + '_ {
        ALL_ROWS
            .iter()
            .zip(self.seen)
            .filter(|(_, seen)| *seen)
            .map(|(row, _)| *row)
    }
}

/// Every class x the given origins for one point (and the given pins).
fn decide_all_intents(
    tally: &mut Tally,
    point: &Point,
    origins: &[Origin],
    pins: &[Option<Destination>],
) {
    for class in CLASSES {
        for origin in origins.iter().copied() {
            let intent = point.intent(class, origin);
            for pin in pins {
                tally.decide(&intent, &point.facts, *pin);
            }
        }
    }
}

/// Runs the three exhaustive blocks and returns what they decided.
fn run_exhaustive() -> Tally {
    let mut tally = Tally::new();
    let bases = base_points();
    let origins = all_origins();

    // 1. EVERY gate mask (2^12) x every ssh phase x every class x every origin, on the running and
    //    the idle-prompt base. Gates are what holds content, so they get the full product.
    for base in &bases[1..] {
        for mask in 0..(1u16 << GateKind::ALL.len()) {
            for ssh in 0..3 {
                let mut point = *base;
                point.facts.gates = GateMask(mask);
                point.facts.ssh = ssh_of(ssh);
                for class in CLASSES {
                    for origin in origins {
                        tally.decide(&point.intent(class, origin), &point.facts, None);
                    }
                }
            }
        }
    }

    // 2. EVERY subshell x ps x ssh phase x the 11 core booleans of `facts_from`, every class, for
    //    the keyboard (the editor's origin is covered by block 1 and the pairs).
    for bits in 0..(1u32 << 11) {
        for ssh in 0..3 {
            for subshell in 0..4 {
                for ps in 0..5 {
                    let facts = facts_from(bits, ssh, subshell, ps, GateMask::EMPTY);
                    for class in CLASSES {
                        tally.decide(&Intent::new(class, Origin::Keyboard), &facts, None);
                    }
                }
            }
        }
    }

    // 3. EVERY PAIR of values of any two dimensions, on each base, every class x origin x pin.
    //    The pins only rewrite D6 (a keyboard line), so they run for the keyboard origin only.
    let dims = all_dims();
    let all = all_origins();
    let pinned = [
        Some(Destination::Editor),
        Some(Destination::Pty),
        Some(Destination::AgentInput),
    ];
    let both = |tally: &mut Tally, point: &Point| {
        decide_all_intents(tally, point, &all, &[None]);
        decide_all_intents(tally, point, &[Origin::Keyboard], &pinned);
    };
    for base in &bases {
        both(&mut tally, base);
        for (i, first) in dims.iter().enumerate() {
            for second in dims.iter().skip(i + 1) {
                for a in 0..first.values {
                    for b in 0..second.values {
                        let mut point = *base;
                        (first.set)(&mut point, a);
                        (second.set)(&mut point, b);
                        both(&mut tally, &point);
                    }
                }
            }
        }
    }
    tally
}

/// `decide` is total and satisfies every table property over the exhaustive blocks (see the module
/// docs for what is enumerated), every plan carries exactly one row, and no sample decides an
/// effect-only row.
#[test]
fn exhaustive_enumeration_is_total_and_never_decides_an_effect_only_row() {
    let tally = run_exhaustive();
    assert!(
        tally.decisions > 5_000_000,
        "the enumeration shrank: {}",
        tally.decisions
    );
    let decided: Vec<&'static str> = tally
        .rows()
        .filter(|row| EFFECT_ONLY_ROWS.contains(row))
        .map(|row| row.name())
        .collect();
    assert!(
        decided.is_empty(),
        "effect-only rows that a plan names as its row: {decided:?}"
    );
}

/// Every row except the effect-only ones (H3b, H7b, P1b, D4) is decided by the exhaustive blocks
/// together with the existing enumerated samples: no row is unreachable.
#[test]
fn exhaustive_enumeration_reaches_every_row_but_the_effect_only_ones() {
    let mut tally = run_exhaustive();
    for_each_sample(|intent, facts, pin| {
        let position = ALL_ROWS
            .iter()
            .position(|row| *row == decide(intent, facts, pin).row)
            .expect("a plan names a row of the table");
        tally.seen[position] = true;
    });
    let missing: Vec<&'static str> = ALL_ROWS
        .iter()
        .zip(tally.seen)
        .filter(|(row, seen)| !seen && !EFFECT_ONLY_ROWS.contains(*row))
        .map(|(row, _)| row.name())
        .collect();
    assert!(
        missing.is_empty(),
        "rows no enumeration reaches: {missing:?}"
    );
}
