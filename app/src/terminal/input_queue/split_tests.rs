//! The destination-split drain of the shared pane queue (#1353 step 2/3).
//!
//! The editor and the terminal view drain ONE queue, but only the view can reach both
//! destinations. A drain driven by the editor side stops at the first entry it cannot deliver
//! (`Host::handles`), leaves it at the head, and pokes the waker; the view continues the drain.
//! These tests drive two hosts over one queue the way the wiring does and check the invariant
//! that matters: per destination, and overall, delivery order is arrival order, every entry is
//! delivered exactly once, by a host that handles it, and a stop is never left without a wake.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

type Log = Rc<RefCell<Vec<(u64, &'static str, Destination)>>>;

/// A host that delivers `Editor` entries only (`editor_only`) or everything.
struct SideHost {
    name: &'static str,
    editor_only: bool,
    log: Log,
    /// Entries delivered to a destination this host does not handle (must stay empty).
    violations: Rc<RefCell<Vec<u64>>>,
}

impl SideHost {
    fn editor(log: &Log, violations: &Rc<RefCell<Vec<u64>>>) -> Self {
        Self {
            name: "editor",
            editor_only: true,
            log: Rc::clone(log),
            violations: Rc::clone(violations),
        }
    }

    fn view(log: &Log, violations: &Rc<RefCell<Vec<u64>>>) -> Self {
        Self {
            name: "view",
            editor_only: false,
            log: Rc::clone(log),
            violations: Rc::clone(violations),
        }
    }
}

impl Host for SideHost {
    fn facts(&self) -> DestFacts {
        DestFacts {
            pty_takes_input: true,
            pty_open: true,
            input_box_visible: true,
            ..DestFacts::default()
        }
    }

    fn handles(&self, entry: &InputEntry, _dest: Destination) -> bool {
        !self.editor_only || !is_pty_entry(&entry.kind)
    }

    fn deliver(
        &mut self,
        entry: &InputEntry,
        dest: Destination,
        _follow: &mut FollowUps,
    ) -> DeliverOutcome {
        if !self.handles(entry, dest) {
            self.violations.borrow_mut().push(entry.id.0);
        }
        self.log.borrow_mut().push((entry.id.0, self.name, dest));
        DeliverOutcome::Delivered
    }

    fn discarded(&mut self, entry: &InputEntry, why: DiscardReason) {
        panic!(
            "nothing is discarded in these scripts: {:?} {why:?}",
            entry.id
        );
    }

    fn arm_give_up(&mut self, _token: GateToken, _kind: GateKind, _after: std::time::Duration) {}

    fn warn(&mut self, line: &str) {
        panic!("unexpected warning: {line}");
    }
}

fn editor_key(text: &str) -> EntryKind {
    EntryKind::EditorKey(EditorKeyPayload::new(HeldPasteKey::Text(text.to_owned())))
}

fn pty_key(text: &str) -> EntryKind {
    EntryKind::PtyBytes {
        bytes: text.as_bytes().to_vec(),
        takes_over_agent_control: true,
    }
}

struct Rig {
    queue: InputQueue,
    log: Log,
    violations: Rc<RefCell<Vec<u64>>>,
    wake: async_channel::Receiver<()>,
}

impl Rig {
    fn new() -> Self {
        let (tx, rx) = async_channel::bounded::<()>(1);
        let mut queue = InputQueue::new();
        queue.set_foreign_waker(Some(tx));
        Self {
            queue,
            log: Rc::default(),
            violations: Rc::default(),
            wake: rx,
        }
    }

    fn editor(&self) -> SideHost {
        SideHost::editor(&self.log, &self.violations)
    }

    fn view(&self) -> SideHost {
        SideHost::view(&self.log, &self.violations)
    }

    fn woken(&self) -> bool {
        self.wake.try_recv().is_ok()
    }

    fn delivered(&self) -> Vec<(u64, &'static str)> {
        self.log
            .borrow()
            .iter()
            .map(|(id, host, _)| (*id, *host))
            .collect()
    }
}

#[test]
fn an_editor_drain_stops_at_a_pty_entry_and_the_view_continues_in_order() {
    let mut rig = Rig::new();
    let mut ed = rig.editor();
    let mut view = rig.view();
    rig.queue
        .open_gate(&mut ed, GateSpec::new(GateKind::EnterVerdict, 1));
    // Arrival order: editor key, pty key, editor key. The pty key comes from the view.
    rig.queue.offer(&mut ed, editor_key("a"));
    rig.queue.offer(&mut view, pty_key("p"));
    rig.queue.offer(&mut ed, editor_key("b"));
    assert!(rig.delivered().is_empty(), "the gate holds all three");
    assert!(!rig.queue.pending_foreign());

    // The editor side closes the gate: it delivers what it can and stops at the pty entry.
    assert!(rig.queue.on_gate_event(&mut ed, GateKind::EnterVerdict, 1));
    assert_eq!(rig.delivered(), vec![(1, "editor")]);
    assert!(
        rig.queue.pending_foreign(),
        "the pty entry waits for the view"
    );
    assert!(rig.woken(), "a stop pokes the waker");
    assert_eq!(
        rig.queue.queued_len(),
        2,
        "the pty entry and the key behind it"
    );
    assert!(
        !rig.queue.head_blocked(),
        "the head is deliverable, only the wrong host drove"
    );

    // The view continues: the pty entry, then the editor key behind it (the view reaches both).
    rig.queue.poke(&mut view);
    assert_eq!(
        rig.delivered(),
        vec![(1, "editor"), (2, "view"), (3, "view")]
    );
    assert!(!rig.queue.pending_foreign());
    assert!(rig.queue.is_idle());
    assert!(rig.violations.borrow().is_empty());
}

#[test]
fn nothing_overtakes_a_foreign_stop() {
    let mut rig = Rig::new();
    let mut ed = rig.editor();
    let mut view = rig.view();
    rig.queue
        .open_gate(&mut view, GateSpec::new(GateKind::SubshellSetup, 0));
    rig.queue.offer(&mut view, pty_key("p"));
    // The gate opens through the editor-side host (any host can close a gate).
    rig.queue.on_gate_event(&mut ed, GateKind::SubshellSetup, 0);
    assert!(rig.queue.pending_foreign());
    assert!(rig.delivered().is_empty());
    // A key typed in the editor meanwhile queues behind the stopped pty entry, even though the
    // editor host could deliver it: strict FIFO.
    rig.queue.offer(&mut ed, editor_key("late"));
    assert!(
        rig.delivered().is_empty(),
        "the editor key must not overtake"
    );
    assert_eq!(rig.queue.queued_len(), 2);
    rig.queue.poke(&mut view);
    assert_eq!(rig.delivered(), vec![(1, "view"), (2, "view")]);
    assert!(rig.violations.borrow().is_empty());
}

#[test]
fn a_blocked_head_is_not_a_foreign_stop() {
    let mut rig = Rig::new();
    let mut ed = rig.editor();
    let mut view = rig.view();
    rig.queue
        .open_gate(&mut view, GateSpec::new(GateKind::SubshellSetup, 0));
    rig.queue.offer(&mut view, pty_key("p"));
    // A drain by the editor host over a gate-held pty entry just stops at the gate.
    rig.queue.poke(&mut ed);
    assert!(
        !rig.queue.pending_foreign(),
        "the gate holds it, no hand-over needed"
    );
    assert!(!rig.woken());
}

#[test]
fn the_next_drain_clears_the_stop() {
    let mut rig = Rig::new();
    let mut ed = rig.editor();
    let mut view = rig.view();
    rig.queue
        .open_gate(&mut ed, GateSpec::new(GateKind::EnterVerdict, 1));
    rig.queue.offer(&mut view, pty_key("p"));
    rig.queue.on_gate_event(&mut ed, GateKind::EnterVerdict, 1);
    assert!(rig.queue.pending_foreign());
    // A second editor drain stops again (and pokes again), still one entry, still the head.
    rig.queue.poke(&mut ed);
    assert!(rig.queue.pending_foreign());
    assert_eq!(rig.queue.queued_len(), 1);
    rig.queue.poke(&mut view);
    assert!(!rig.queue.pending_foreign());
    assert!(rig.queue.is_idle());
}

// ---- model-based --------------------------------------------------------------------------

/// Deterministic xorshift so a failing seed reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Random interleavings of editor keys, pty keys, gates of both sides and drains by either host.
/// Invariants after EVERY step:
/// - the entries delivered so far are exactly the arrival-order prefix (ids 1..=n in order), and
///   each was delivered once, by a host that handles its destination;
/// - an editor-driven stop always has a wake behind it (nothing is left waiting unannounced);
/// and at the end, after every gate is closed and the view drains: the queue is idle and every
/// entry was delivered.
#[test]
fn two_destination_drain_keeps_arrival_order_and_never_strands_a_stop() {
    for seed in 1..=300u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut rig = Rig::new();
        let mut ed = rig.editor();
        let mut view = rig.view();
        let mut offered = 0u64;
        // Open gates: (kind, key). Editor-side kinds are closed by the editor host, pty-side by
        // the view, as in the wiring.
        let mut open: Vec<(GateKind, u64)> = Vec::new();
        let mut next_key = 0u64;
        let mut wake_seen = false;

        for step in 0..60 {
            let op: &str;
            match rng.below(8) {
                0 | 1 => {
                    op = "editor_key";
                    rig.queue.offer(&mut ed, editor_key("e"));
                    offered += 1;
                }
                2 | 3 => {
                    op = "pty_key";
                    rig.queue.offer(&mut view, pty_key("p"));
                    offered += 1;
                }
                4 => {
                    op = "open_editor_gate";
                    next_key += 1;
                    rig.queue
                        .open_gate(&mut ed, GateSpec::new(GateKind::EnterVerdict, next_key));
                    open.push((GateKind::EnterVerdict, next_key));
                }
                5 => {
                    op = "open_pty_gate";
                    next_key += 1;
                    rig.queue
                        .open_gate(&mut view, GateSpec::new(GateKind::SubshellSetup, next_key));
                    open.push((GateKind::SubshellSetup, next_key));
                }
                6 => {
                    op = "close_gate";
                    if !open.is_empty() {
                        let (kind, key) = open.remove(rng.below(open.len()));
                        if kind == GateKind::EnterVerdict {
                            rig.queue.on_gate_event(&mut ed, kind, key);
                        } else {
                            rig.queue.on_gate_event(&mut view, kind, key);
                        }
                    }
                }
                _ => {
                    op = "view_poke";
                    rig.queue.poke(&mut view);
                    wake_seen = false;
                }
            }
            while rig.woken() {
                wake_seen = true;
            }
            let ctx = format!("seed={seed} step={step} op={op}");
            if rig.queue.pending_foreign() {
                assert!(wake_seen, "a foreign stop without a wake: {ctx}");
            }
            let ids: Vec<u64> = rig.delivered().iter().map(|(id, _)| *id).collect();
            let expected: Vec<u64> = (1..=ids.len() as u64).collect();
            assert_eq!(ids, expected, "delivered out of arrival order: {ctx}");
            assert_eq!(
                ids.len() + rig.queue.queued_len(),
                offered as usize,
                "an entry was lost or duplicated: {ctx}"
            );
            assert!(
                rig.violations.borrow().is_empty(),
                "wrong host delivered: {ctx}"
            );
        }

        // Close everything and let the view finish.
        for (kind, key) in open.drain(..) {
            if kind == GateKind::EnterVerdict {
                rig.queue.on_gate_event(&mut ed, kind, key);
            } else {
                rig.queue.on_gate_event(&mut view, kind, key);
            }
        }
        rig.queue.poke(&mut view);
        assert!(rig.queue.is_idle(), "seed={seed}: the queue did not empty");
        assert!(!rig.queue.pending_foreign(), "seed={seed}");
        assert_eq!(rig.delivered().len() as u64, offered, "seed={seed}");
        assert!(rig.violations.borrow().is_empty(), "seed={seed}");
    }
}
