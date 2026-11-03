//! The op sequence the queue's model-based tests drive, and the runner that checks every step
//! against the reference model. Shared by the proptest strategies (`prop_model.rs`) and, behind
//! the `dev-support` feature, the cargo-fuzz input-queue target (`fuzz/`), where `Op` derives
//! `arbitrary::Arbitrary`.

use super::*;

/// One generated step. Indices are reduced modulo what exists when the step runs, so a shrunk
/// sequence stays meaningful.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "dev-support", derive(arbitrary::Arbitrary))]
pub enum Op {
    /// Text typed into the input editor (destination decided at drain).
    Text(u8),
    Enter,
    Control,
    Seq,
    Raw,
    /// A key the editor offered behind a gate (carries the editor origin).
    EditorKey(u8),
    /// Bytes the terminal element offered (carries the pty origin).
    PtyKey(u8),
    /// A paste read for the editor (`true`) or the pty (`false`).
    Paste {
        editor: bool,
    },
    PasteResult {
        pick: u8,
        ok: bool,
    },
    /// The event of an open gate (`pick`), or a made-up (kind, key) that may match nothing.
    GateEvent {
        pick: u8,
        made_up: Option<(u8, u8)>,
    },
    OpenGate {
        kind: u8,
        key: u8,
    },
    GiveUpLive {
        pick: u8,
    },
    GiveUpStale {
        pick: u8,
    },
    Signal(u8),
    Focus,
    Facts(u8),
    EnterOpens,
    SshWait,
    /// The entry offered next is turned into a hold by the later stage when first delivered.
    RequeueNext {
        kind: u8,
        key: u8,
    },
    /// Asks the destination table (#1424) what it would do with an input of `class` and `origin`
    /// RIGHT NOW, on facts that carry the model's open gates and queued entries. Decides nothing
    /// and changes nothing; checks that the table agrees with the queue about what holds.
    Decide {
        class: u8,
        origin: u8,
    },
    /// Calls `table::decide` on the model's CURRENT destination facts and checks it against the
    /// legacy `destination_for`, allowing only the differences listed in `table_equiv_tests`
    /// (#1529). The equivalence space has no open gate and nothing queued, so it is checked only
    /// at such points. Test builds only: the allow-list lives in a test module.
    DecideEquiv {
        kind: u8,
    },
}

/// The input class a [`Op::Decide`] asks about.
fn decide_class(n: u8) -> table::Class {
    use table::Class;
    match n % 12 {
        0 => Class::Typed,
        1 => Class::Enter,
        2 => Class::Control,
        3 => Class::Seq,
        4 => Class::Paste,
        5 => Class::Raw,
        6 => Class::EditKey,
        7 => Class::AgentWrite,
        8 => Class::Signal(SignalKind::Interrupt),
        9 => Class::Signal(SignalKind::Eof),
        10 => Class::Signal(SignalKind::Suspend),
        _ => Class::Signal(SignalKind::Quit),
    }
}

/// The origin a [`Op::Decide`] asks about.
fn decide_origin(n: u8) -> Origin {
    match n % 8 {
        0 | 1 => Origin::Keyboard,
        2 => Origin::Editor,
        3 => Origin::Held,
        4 => Origin::Synced,
        5 => Origin::CliSubmit,
        6 => Origin::Viewer,
        _ => Origin::Agent,
    }
}

fn editor_key_kind(n: u8) -> EntryKind {
    let key = if n % 3 == 0 {
        HeldPasteKey::Action(crate::editor::EditorAction::Left)
    } else {
        HeldPasteKey::Text(format!("e{}", n % 5))
    };
    EntryKind::EditorKey(EditorKeyPayload::new(key))
}

fn pty_key_kind(n: u8) -> EntryKind {
    EntryKind::PtyBytes {
        bytes: vec![b'p', b'0' + n % 10],
        takes_over_agent_control: n % 4 != 0,
    }
}

fn signal_kind(n: u8) -> EntryKind {
    EntryKind::Signal(match n % 4 {
        0 | 1 => SignalKind::Interrupt,
        2 => SignalKind::Eof,
        _ => SignalKind::Suspend,
    })
}

/// Runs `ops` against the queue and the reference model, panicking on a violated invariant.
pub fn run_ops(ops: &[Op]) {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    let mut m = Model::new();
    let mut w = World {
        facts: open_facts(),
        enter_opens_gate: false,
        ssh_signal_waits: false,
    };
    // One id source for both owners, as on the pane.
    let ids = PaneQueue::new();
    let mut pastes: Vec<u64> = Vec::new();
    let mut dead_tokens: Vec<GateToken> = Vec::new();
    let mut live_give_ups = 0u64;

    for (step, op) in ops.iter().enumerate() {
        let opname = format!("{op:?}");
        let offer =
            |q: &mut InputQueue, h: &mut MockHost, m: &mut Model, w: &World, kind: EntryKind| {
                let a = q.offer(h, kind.clone());
                let b = m.offer(kind, None, w);
                assert_eq!(a, b, "entry ids diverged");
            };
        match op {
            Op::Text(n) => offer(
                &mut q,
                &mut h,
                &mut m,
                &w,
                EntryKind::Text("t".repeat(1 + usize::from(*n % 3))),
            ),
            Op::Enter => offer(&mut q, &mut h, &mut m, &w, EntryKind::Enter),
            Op::Control => offer(&mut q, &mut h, &mut m, &w, EntryKind::Control("backspace")),
            Op::Seq => offer(&mut q, &mut h, &mut m, &w, EntryKind::Seq("left")),
            Op::Raw => offer(&mut q, &mut h, &mut m, &w, EntryKind::Raw(vec![0x1b, b'['])),
            Op::EditorKey(n) => offer(&mut q, &mut h, &mut m, &w, editor_key_kind(*n)),
            Op::PtyKey(n) => offer(&mut q, &mut h, &mut m, &w, pty_key_kind(*n)),
            Op::Paste { editor } => {
                let id = ids.next_paste_id(if *editor {
                    PasteOwner::Editor
                } else {
                    PasteOwner::Pty
                });
                pastes.push(id);
                let kind = EntryKind::Paste {
                    paste_id: id,
                    payload: None,
                };
                let (a, _) = q.offer_gated(
                    &mut h,
                    kind.clone(),
                    GateSpec::new(GateKind::PasteReading, id),
                );
                let b = m.offer(kind, Some((GateKind::PasteReading, id)), &w);
                assert_eq!(a, b);
            }
            Op::PasteResult { pick, ok } => {
                // Sometimes an id that was never issued.
                let slot = usize::from(*pick) % (pastes.len() + 1);
                let id = pastes.get(slot).copied().unwrap_or(9_999);
                q.on_paste_read(&mut h, id, if *ok { Ok(vec![7]) } else { Err(()) });
                let pos = m.pending.iter().position(
                    |p| matches!(&p.kind, EntryKind::Paste { paste_id, .. } if *paste_id == id),
                );
                if let Some(pos) = pos {
                    if *ok {
                        if let EntryKind::Paste { payload, .. } = &mut m.pending[pos].kind {
                            *payload = Some(vec![7]);
                        }
                    } else {
                        let p = m.pending.remove(pos);
                        m.discarded.push((p.id, DiscardReason::PasteFailed));
                    }
                }
                m.close(GateKind::PasteReading, id, &w);
            }
            Op::GateEvent { pick, made_up } => {
                let (kind, key) = match (made_up, m.gates.is_empty()) {
                    (Some((k, key)), _) => (
                        GateKind::ALL[usize::from(*k) % GateKind::ALL.len()],
                        u64::from(*key % 4),
                    ),
                    (None, false) => {
                        let g = m.gates[usize::from(*pick) % m.gates.len()];
                        (g.kind, g.key)
                    }
                    (None, true) => (GateKind::SshConnect, 0),
                };
                let a = q.on_gate_event(&mut h, kind, key);
                let b = m.close(kind, key, &w);
                assert_eq!(a, b, "gate event result diverged");
            }
            Op::OpenGate { kind, key } => {
                let kind = GateKind::ALL[usize::from(*kind) % GateKind::ALL.len()];
                let key = u64::from(*key % 4);
                q.open_gate(&mut h, GateSpec::new(kind, key));
                let barrier = EntryId(m.next_id);
                m.open_gate(kind, key, barrier);
            }
            Op::GiveUpLive { pick } => {
                if !q.gates().is_empty() {
                    let g = q.gates()[usize::from(*pick) % q.gates().len()];
                    assert!(q.give_up(&mut h, g.token), "a live gate's cap must fire");
                    live_give_ups += 1;
                    dead_tokens.push(g.token);
                    m.gates.retain(|x| x.seq != g.token.0);
                    if g.kind == GateKind::PasteReading {
                        let (gone, kept): (Vec<_>, Vec<_>) = m.pending.drain(..).partition(|p| {
                            matches!(&p.kind, EntryKind::Paste { paste_id, payload: None } if *paste_id == g.key)
                        });
                        m.pending = kept;
                        for p in gone {
                            m.discarded.push((p.id, DiscardReason::PasteNotReady));
                        }
                    }
                    m.drain(&w);
                }
            }
            Op::GiveUpStale { pick } => {
                // A cap for a gate whose event already came (or that never existed): no effect.
                let token = if dead_tokens.is_empty() {
                    GateToken(10_000 + u64::from(*pick))
                } else {
                    dead_tokens[usize::from(*pick) % dead_tokens.len()]
                };
                let warns = h.warns.len();
                assert!(!q.give_up(&mut h, token), "a stale cap fired");
                assert_eq!(h.warns.len(), warns, "a stale cap logged");
            }
            Op::Signal(n) => {
                // A requeue scheduled for a signal's id would only be consumed by the signal.
                m.requeue.remove(&EntryId(m.next_id));
                h.requeue.remove(&EntryId(m.next_id));
                offer(&mut q, &mut h, &mut m, &w, signal_kind(*n));
            }
            Op::Focus => {
                w.facts.editor_focused = !w.facts.editor_focused;
                h.facts = w.facts;
                q.on_focus_changed(&mut h);
                m.drain(&w);
            }
            Op::Facts(n) => {
                match n % 6 {
                    0 => w.facts.approval_card = !w.facts.approval_card,
                    1 => w.facts.agent_owns_pty = !w.facts.agent_owns_pty,
                    2 => w.facts.pty_takes_input = !w.facts.pty_takes_input,
                    3 => w.facts.input_box_visible = !w.facts.input_box_visible,
                    4 => w.facts.pty_open = !w.facts.pty_open,
                    _ => {
                        w.facts.drop_reason = if w.facts.drop_reason.is_some() {
                            None
                        } else {
                            Some(DropReason::SshDialogOrPassword)
                        }
                    }
                }
                h.facts = w.facts;
            }
            Op::EnterOpens => {
                w.enter_opens_gate = !w.enter_opens_gate;
                h.enter_opens_gate = w.enter_opens_gate;
            }
            Op::SshWait => {
                w.ssh_signal_waits = !w.ssh_signal_waits;
                h.ssh_signal_waits = w.ssh_signal_waits;
            }
            Op::Decide { class, origin } => {
                let class = decide_class(*class);
                let origin = decide_origin(*origin);
                let kinds: Vec<GateKind> = m.gates.iter().map(|g| g.kind).collect();
                let facts = RouteFacts {
                    model_consistent: true,
                    handoff_known: true,
                    long_running: true,
                    active_block_started: true,
                    session_pending_or_bootstrapped: true,
                    is_bootstrapped: true,
                    gates: GateMask::of(&kinds),
                    queued: !m.pending.is_empty(),
                    queued_only_pty: m
                        .pending
                        .iter()
                        .all(|p| matches!(p.kind, EntryKind::PtyBytes { .. })),
                    ..RouteFacts::default()
                };
                let intent = table::Intent::new(class, origin);
                // Total: any class, origin and queue state has a plan (no panic).
                let plan = table::decide(&intent, &facts, None);
                let signal = matches!(class, table::Class::Signal(_));
                if signal {
                    assert!(
                        !matches!(
                            plan.outcome,
                            table::Outcome::Hold(_) | table::Outcome::Queue
                        ),
                        "a signal never waits behind a gate: {plan:?} with gates {kinds:?}"
                    );
                }
                let plain = matches!(
                    class,
                    table::Class::Typed
                        | table::Class::Enter
                        | table::Class::Control
                        | table::Class::Seq
                        | table::Class::Paste
                ) && origin == Origin::Keyboard;
                if plain {
                    let only_ssh = facts.gates.all_within(&[GateKind::SshConnect]);
                    if !facts.gates.is_empty() && !only_ssh {
                        // The queue holds every key behind an open gate: the table must agree.
                        assert!(
                            matches!(
                                plan.outcome,
                                table::Outcome::Hold(_)
                                    | table::Outcome::Drop(_)
                                    | table::Outcome::Hint
                            ),
                            "the queue holds {class:?} behind {kinds:?} but the table says {plan:?}"
                        );
                    }
                    if facts.gates.is_empty() && !facts.queued {
                        assert!(
                            !matches!(
                                plan.outcome,
                                table::Outcome::Hold(_) | table::Outcome::Queue
                            ),
                            "nothing is held or queued, yet the table holds {class:?}: {plan:?}"
                        );
                    }
                }
            }
            Op::DecideEquiv { kind } => {
                #[cfg(test)]
                if m.gates.is_empty() && m.pending.is_empty() {
                    if let Err(diff) =
                        super::super::facts::table_enum_tests::equiv::check_live_facts(
                            &w.facts, *kind,
                        )
                    {
                        panic!(
                            "table::decide and destination_for differ off the ALLOWED list: {diff}"
                        );
                    }
                }
                #[cfg(not(test))]
                let _ = kind;
            }
            Op::RequeueNext { kind, key } => {
                let kind = GateKind::ALL[usize::from(*kind) % GateKind::ALL.len()];
                let key = u64::from(*key % 4);
                let id = EntryId(m.next_id);
                h.requeue.insert(id, GateSpec::new(kind, key));
                m.requeue.insert(id, (kind, key));
            }
        }
        h.facts = w.facts;
        // Gates that closed by their event leave a cap timer behind: it must now be inert.
        for (token, _, _) in &h.armed {
            if !q.gates().iter().any(|g| g.token == *token) && !dead_tokens.contains(token) {
                dead_tokens.push(*token);
            }
        }
        check_equal(0, step, &opname, &q, &h, &m);
    }

    // A cap fired exactly once per live gate that was closed that way, and nothing else.
    assert_eq!(q.give_up_count(), live_give_ups);
    assert_eq!(h.warns.len() as u64, live_give_ups);
    // Never wedged: every remaining gate opens by its event (events can open new gates, so close
    // until quiet).
    for _ in 0..2000 {
        let Some(gate) = q.gates().first().copied() else {
            break;
        };
        q.on_gate_event(&mut h, gate.kind, gate.key);
        m.close(gate.kind, gate.key, &w);
    }
    assert!(q.gates().is_empty(), "gates left after their events");
    check_equal(0, ops.len(), "final", &q, &h, &m);
    assert_eq!(q.queued_len(), 0, "queue wedged");
    h.enter_opens_gate = false;
    h.facts = open_facts();
    // A requeue scheduled for an id no entry reached yet would turn the probe into a hold; it is
    // a scripted later-stage decision, not a queue wedge (shrunk case: `[RequeueNext]` alone).
    h.requeue.clear();
    m.requeue.clear();
    let before = h.delivered.len();
    q.offer(&mut h, text("probe"));
    assert_eq!(
        h.delivered.len(),
        before + 1,
        "the next key must go through"
    );
}
