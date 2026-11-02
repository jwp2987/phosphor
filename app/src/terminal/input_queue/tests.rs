//! Tests for the input queue: unit tests for each rule, then a model-based test that drives
//! random interleavings of entries, gate events, focus flips, facts changes and signals and
//! checks every step against a small reference model plus invariants (no entry lost, duplicated
//! or reordered; a blocked head blocks everything behind it; give-up fires only for a gate whose
//! event never came, once).

use std::collections::HashMap;
use std::time::Duration;

use super::*;

// ---- mock host ------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct Delivery {
    id: EntryId,
    tag: &'static str,
    dest: Destination,
}

#[derive(Default)]
struct MockHost {
    facts: DestFacts,
    delivered: Vec<Delivery>,
    discarded: Vec<(EntryId, DiscardReason)>,
    armed: Vec<(GateToken, GateKind, Duration)>,
    warns: Vec<String>,
    /// `(first, dropped)` of every `Host::overflowed` call (#1442).
    overflows: Vec<(bool, u64)>,
    /// Enter opens a `HandoffReport` gate keyed by its id while set.
    enter_opens_gate: bool,
    ssh_signal_waits: bool,
    /// Follow-ups to request when the entry with this id is delivered.
    scripted: HashMap<EntryId, Vec<ScriptedFollow>>,
    /// The entry with this id is not delivered but taken into a hold of the later stage
    /// (`DeliverOutcome::Requeue`) the first time it comes up.
    requeue: HashMap<EntryId, GateSpec>,
    /// What goes back to the front instead of the delivered entry itself when it is requeued
    /// (`Host::requeued_entry`): the entry delivered some other way, only its held part remains.
    requeue_as: HashMap<EntryId, EntryKind>,
}

enum ScriptedFollow {
    Offer(EntryKind),
    GateEvent(GateKind, u64),
    OpenGate(GateSpec),
}

fn open_facts() -> DestFacts {
    DestFacts {
        pty_takes_input: true,
        input_box_visible: true,
        pty_open: true,
        ..DestFacts::default()
    }
}

impl MockHost {
    fn new() -> Self {
        Self {
            facts: open_facts(),
            ..Self::default()
        }
    }

    fn delivered_ids(&self) -> Vec<u64> {
        self.delivered.iter().map(|d| d.id.0).collect()
    }
}

impl Host for MockHost {
    fn facts(&self) -> DestFacts {
        self.facts
    }

    fn deliver(
        &mut self,
        entry: &InputEntry,
        dest: Destination,
        follow: &mut FollowUps,
    ) -> DeliverOutcome {
        assert!(
            !matches!(dest, Destination::Drop(_)),
            "drops never reach deliver"
        );
        if let Some(spec) = self.requeue.remove(&entry.id) {
            return DeliverOutcome::Requeue(spec);
        }
        self.delivered.push(Delivery {
            id: entry.id,
            tag: entry.kind.tag(),
            dest,
        });
        for item in self.scripted.remove(&entry.id).unwrap_or_default() {
            match item {
                ScriptedFollow::Offer(kind) => follow.offer(kind),
                ScriptedFollow::GateEvent(kind, key) => follow.gate_event(kind, key),
                ScriptedFollow::OpenGate(spec) => follow.open_gate(spec),
            }
        }
        if entry.kind == EntryKind::Enter && self.enter_opens_gate {
            return DeliverOutcome::OpensGate(GateSpec::new(GateKind::HandoffReport, entry.id.0));
        }
        DeliverOutcome::Delivered
    }

    fn discarded(&mut self, entry: &InputEntry, why: DiscardReason) {
        self.discarded.push((entry.id, why));
    }

    fn requeued_entry(&mut self, entry: &InputEntry) -> InputEntry {
        let mut remaining = entry.clone();
        if let Some(kind) = self.requeue_as.remove(&entry.id) {
            remaining.kind = kind;
        }
        remaining
    }

    fn arm_give_up(&mut self, token: GateToken, kind: GateKind, after: Duration) {
        self.armed.push((token, kind, after));
    }

    fn signal_must_wait(&self, gate: GateKind) -> bool {
        self.ssh_signal_waits && gate == GateKind::SshConnect
    }

    fn warn(&mut self, line: &str) {
        self.warns.push(line.to_owned());
    }

    fn overflowed(&mut self, first: bool, dropped: u64, queued: usize) {
        self.overflows.push((first, dropped));
        if first {
            self.warn(&format!("input queue full ({queued} entries waiting)"));
        }
    }
}

fn text(s: &str) -> EntryKind {
    EntryKind::Text(s.to_owned())
}

// ---- unit tests -----------------------------------------------------------------------------

#[test]
fn idle_queue_delivers_inline_in_order() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("a"));
    q.offer(&mut h, EntryKind::Enter);
    q.offer(&mut h, text("b"));
    assert_eq!(h.delivered_ids(), vec![1, 2, 3]);
    assert!(q.is_idle());
    assert!(h.armed.is_empty(), "no gate, no cap timer");
}

#[test]
fn a_gate_blocks_the_head_and_everything_behind_until_its_event() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("a"));
    q.open_gate(&mut h, GateSpec::new(GateKind::SshConnect, 7));
    q.offer(&mut h, text("b"));
    q.offer(&mut h, EntryKind::Enter);
    q.offer(&mut h, text("c"));
    assert_eq!(h.delivered_ids(), vec![1]);
    assert_eq!(q.queued_len(), 3);
    assert!(q.head_blocked());
    // A wrong key or kind is not the gate's event.
    assert!(!q.on_gate_event(&mut h, GateKind::SshConnect, 8));
    assert!(!q.on_gate_event(&mut h, GateKind::PasteReading, 7));
    assert_eq!(h.delivered_ids(), vec![1]);
    assert!(q.on_gate_event(&mut h, GateKind::SshConnect, 7));
    assert_eq!(h.delivered_ids(), vec![1, 2, 3, 4]);
    // A late duplicate of the event is a no-op.
    assert!(!q.on_gate_event(&mut h, GateKind::SshConnect, 7));
}

#[test]
fn give_up_is_one_shot_and_guarded_by_the_gate_token() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    let token = q.open_gate(&mut h, GateSpec::new(GateKind::EnterVerdict, 1));
    assert_eq!(h.armed.len(), 1);
    assert_eq!(h.armed[0].0, token);
    assert_eq!(h.armed[0].2, Duration::from_millis(250));
    q.offer(&mut h, EntryKind::Enter);
    // The event wins the race: the later cap timer finds nothing.
    q.on_gate_event(&mut h, GateKind::EnterVerdict, 1);
    assert_eq!(h.delivered_ids(), vec![1]);
    assert!(!q.give_up(&mut h, token));
    assert!(h.warns.is_empty());
    // Same kind and key reopened: the OLD token must not close the NEW gate.
    let token2 = q.open_gate(&mut h, GateSpec::new(GateKind::EnterVerdict, 1));
    q.offer(&mut h, text("x"));
    assert!(!q.give_up(&mut h, token));
    assert_eq!(h.delivered_ids(), vec![1]);
    assert!(q.give_up(&mut h, token2));
    assert_eq!(h.warns.len(), 1);
    assert!(!q.give_up(&mut h, token2), "one-shot");
    assert_eq!(h.warns.len(), 1);
    assert_eq!(h.delivered_ids(), vec![1, 2]);
    assert_eq!(q.give_up_count(), 1);
}

#[test]
fn enter_that_opens_a_gate_holds_what_was_already_queued_behind_it() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::PasteReading, 1).with_cap(None),
    );
    q.offer(&mut h, EntryKind::Enter);
    q.offer(&mut h, text("after"));
    h.enter_opens_gate = true;
    q.on_gate_event(&mut h, GateKind::PasteReading, 1);
    // The Enter (id 1) ran and closed the queue behind itself: "after" (id 2) was already
    // queued and must still wait for the report.
    assert_eq!(h.delivered_ids(), vec![1]);
    assert_eq!(q.queued_len(), 1);
    q.on_gate_event(&mut h, GateKind::HandoffReport, 1);
    assert_eq!(h.delivered_ids(), vec![1, 2]);
}

#[test]
fn paste_blocks_following_keys_and_delivers_before_them() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("before"));
    let (paste, _) = q.offer_gated(
        &mut h,
        EntryKind::Paste {
            paste_id: 5,
            payload: None,
        },
        GateSpec::new(GateKind::PasteReading, 5),
    );
    q.offer(&mut h, text("after"));
    assert_eq!(h.delivered_ids(), vec![1]);
    q.on_paste_read(&mut h, 5, Ok(vec![1, 2, 3]));
    assert_eq!(h.delivered_ids(), vec![1, paste.0, 3]);
    // Stale result: ignored.
    q.on_paste_read(&mut h, 5, Ok(vec![9]));
    assert_eq!(h.delivered_ids().len(), 3);
}

#[test]
fn failed_paste_is_discarded_and_keys_behind_it_are_released() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    let (paste, _) = q.offer_gated(
        &mut h,
        EntryKind::Paste {
            paste_id: 1,
            payload: None,
        },
        GateSpec::new(GateKind::PasteReading, 1),
    );
    q.offer(&mut h, text("k"));
    q.on_paste_read(&mut h, 1, Err(()));
    assert_eq!(h.delivered_ids(), vec![2]);
    assert_eq!(h.discarded, vec![(paste, DiscardReason::PasteFailed)]);
}

#[test]
fn paste_backstop_discards_the_paste_and_releases_the_keys_in_order() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    let (paste, token) = q.offer_gated(
        &mut h,
        EntryKind::Paste {
            paste_id: 1,
            payload: None,
        },
        GateSpec::new(GateKind::PasteReading, 1),
    );
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b"));
    assert!(q.give_up(&mut h, token));
    assert_eq!(h.delivered_ids(), vec![2, 3]);
    assert_eq!(h.discarded, vec![(paste, DiscardReason::PasteNotReady)]);
    // The reader finally answers: nothing left to deliver.
    q.on_paste_read(&mut h, 1, Ok(vec![1]));
    assert_eq!(h.delivered_ids(), vec![2, 3]);
}

// ---- signal rows (design 2.5) ---------------------------------------------------------------

fn gate_with_two_held(kind: GateKind) -> (InputQueue, MockHost) {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(&mut h, GateSpec::new(kind, 1));
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b"));
    (q, h)
}

#[test]
fn signal_row_paste_cancels_paste_and_flushes_keys_then_signal() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("before"));
    let (paste, _) = q.offer_gated(
        &mut h,
        EntryKind::Paste {
            paste_id: 1,
            payload: None,
        },
        GateSpec::new(GateKind::PasteReading, 1),
    );
    q.offer(&mut h, text("typed after paste"));
    let sig = q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.delivered_ids(), vec![1, 3, sig.0]);
    assert_eq!(h.discarded, vec![(paste, DiscardReason::PasteCancelled)]);
    assert!(q.is_idle());
}

#[test]
fn signal_row_enter_verdict_voids_everything_held() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer_gated(
        &mut h,
        EntryKind::Enter,
        GateSpec::new(GateKind::EnterVerdict, 1),
    );
    q.offer(&mut h, text("typed behind"));
    let sig = q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.delivered_ids(), vec![sig.0]);
    assert_eq!(h.discarded.len(), 2);
    assert!(q.is_idle());
}

#[test]
fn signal_row_young_enter_flushes_held_keys_before_the_signal() {
    let (mut q, mut h) = gate_with_two_held(GateKind::PromptSeen);
    let sig = q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.delivered_ids(), vec![1, 2, sig.0]);
    assert!(h.discarded.is_empty());
}

#[test]
fn signal_row_ssh_discards_held_keys_then_signal() {
    let (mut q, mut h) = gate_with_two_held(GateKind::SshConnect);
    let sig = q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.delivered_ids(), vec![sig.0]);
    assert_eq!(h.discarded.len(), 2);
}

#[test]
fn signal_row_ssh_signal_is_held_when_the_host_says_it_would_land_mid_bootstrap() {
    let (mut q, mut h) = gate_with_two_held(GateKind::SshConnect);
    h.ssh_signal_waits = true;
    q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert!(h.delivered_ids().is_empty());
    assert_eq!(q.queued_len(), 3);
    q.on_gate_event(&mut h, GateKind::SshConnect, 1);
    // Keys stay in order and the signal keeps its place behind them.
    assert_eq!(h.delivered_ids(), vec![1, 2, 3]);
}

#[test]
fn signal_row_subshell_setup_signal_first_then_held_keys() {
    let (mut q, mut h) = gate_with_two_held(GateKind::SubshellSetup);
    let sig = q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.delivered_ids(), vec![sig.0, 1, 2]);
    assert!(q.gates().is_empty());
}

#[test]
fn signal_row_powershell_signal_first_keys_stay_held() {
    let (mut q, mut h) = gate_with_two_held(GateKind::PowershellSetup);
    let sig = q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.delivered_ids(), vec![sig.0]);
    assert_eq!(q.queued_len(), 2);
    assert_eq!(q.gates().len(), 1);
    q.on_gate_event(&mut h, GateKind::PowershellSetup, 1);
    assert_eq!(h.delivered_ids(), vec![sig.0, 1, 2]);
}

#[test]
fn signal_never_leaves_the_queue_wedged_behind_stacked_gates() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(&mut h, GateSpec::new(GateKind::SubshellBootstrapWrite, 1));
    q.offer(&mut h, text("a"));
    q.open_gate(&mut h, GateSpec::new(GateKind::PromptSeen, 2));
    q.offer(&mut h, text("b"));
    q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    q.offer(&mut h, text("next"));
    assert_eq!(
        q.queued_len(),
        2,
        "b (held by the prompt gate) and the key typed after the ^C"
    );
    // Whatever is left opens with its events and the next keystroke goes through.
    for gate in q.gates().to_vec() {
        q.on_gate_event(&mut h, gate.kind, gate.key);
    }
    assert!(q.is_idle());
    let before = h.delivered.len();
    q.offer(&mut h, text("probe"));
    assert_eq!(h.delivered.len(), before + 1);
}

// ---- destination and pinning ----------------------------------------------------------------

#[test]
fn destination_table_matches_typed_characters_on_terminal() {
    let t = text("x");
    let mut f = open_facts();
    assert_eq!(destination_for(&t, &f), Destination::Pty);
    f.editor_focused = true;
    assert_eq!(destination_for(&t, &f), Destination::Editor);
    f.editor_focused = false;
    f.agent_owns_pty = true;
    assert_eq!(destination_for(&t, &f), Destination::AgentInput);
    assert_eq!(
        destination_for(&EntryKind::Raw(vec![1]), &f),
        Destination::Drop(DropReason::AgentControl)
    );
    assert_eq!(
        destination_for(&EntryKind::Signal(SignalKind::Interrupt), &f),
        Destination::Pty,
        "^C keeps its route to the command"
    );
    f.agent_owns_pty = false;
    f.approval_card = true;
    assert_eq!(destination_for(&t, &f), Destination::AgentInput);
    f.approval_card = false;
    f.pty_takes_input = false;
    assert_eq!(destination_for(&t, &f), Destination::Editor);
    f.input_box_visible = false;
    assert_eq!(
        destination_for(&t, &f),
        Destination::Drop(DropReason::RefusedHiddenInput)
    );
    f.drop_reason = Some(DropReason::SshDialogOrPassword);
    assert_eq!(
        destination_for(&t, &f),
        Destination::Drop(DropReason::SshDialogOrPassword)
    );
}

#[test]
fn a_focus_flip_mid_line_does_not_split_the_line_but_the_next_line_follows_focus() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("ec"));
    h.facts.editor_focused = true;
    q.on_focus_changed(&mut h);
    q.offer(&mut h, text("ho"));
    q.offer(&mut h, EntryKind::Enter);
    q.offer(&mut h, text("next line"));
    let dests: Vec<Destination> = h.delivered.iter().map(|d| d.dest).collect();
    assert_eq!(
        dests,
        vec![
            Destination::Pty,
            Destination::Pty,
            Destination::Pty,
            Destination::Editor
        ]
    );
    assert_eq!(q.focus_epoch(), 1);
}

#[test]
fn pin_is_released_when_its_destination_disappears() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("a"));
    h.facts.pty_open = false;
    h.facts.pty_takes_input = false;
    q.offer(&mut h, text("b"));
    assert_eq!(h.delivered[1].dest, Destination::Editor);
}

#[test]
fn destination_is_decided_at_drain_not_at_arrival() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(&mut h, GateSpec::new(GateKind::EditorFocusPending, 1));
    q.offer(&mut h, text("a"));
    // While it waited, the editor took focus.
    h.facts.editor_focused = true;
    q.on_gate_event(&mut h, GateKind::EditorFocusPending, 1);
    assert_eq!(h.delivered[0].dest, Destination::Editor);
}

#[test]
fn dropped_entries_are_reported_not_delivered_and_a_dropped_enter_ends_the_line() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    h.facts.drop_reason = Some(DropReason::SshDialogOrPassword);
    let id = q.offer(&mut h, text("secret"));
    assert!(h.delivered.is_empty());
    assert_eq!(
        h.discarded,
        vec![(id, DiscardReason::Dropped(DropReason::SshDialogOrPassword))]
    );
}

// ---- re-entrancy ----------------------------------------------------------------------------

#[test]
fn follow_ups_are_applied_by_the_same_drain_in_order_and_are_not_held_twice() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    // Delivering entry 2 replays two keys and a gate event; none may re-enter the queue.
    h.scripted.insert(
        EntryId(2),
        vec![
            ScriptedFollow::Offer(text("r1")),
            ScriptedFollow::Offer(text("r2")),
        ],
    );
    q.open_gate(&mut h, GateSpec::new(GateKind::SshConnect, 1));
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b"));
    q.offer(&mut h, text("c"));
    q.on_gate_event(&mut h, GateKind::SshConnect, 1);
    // Replays go behind "c" (arrival order), once each.
    assert_eq!(h.delivered_ids(), vec![1, 2, 3, 4, 5]);
    assert!(q.is_idle());
}

#[test]
fn a_gate_event_raised_during_delivery_releases_the_queue_inside_the_same_drain() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    h.enter_opens_gate = true;
    h.scripted.insert(
        EntryId(1),
        vec![ScriptedFollow::GateEvent(GateKind::HandoffReport, 1)],
    );
    // The gate the Enter opens is pushed first, then its follow-ups are applied: the event closes
    // it inside the same drain, so the key behind goes through at once.
    q.offer(&mut h, EntryKind::Enter);
    q.offer(&mut h, text("a"));
    assert_eq!(h.delivered_ids(), vec![1, 2]);
    assert!(q.is_idle());
    assert!(!q.on_gate_event(&mut h, GateKind::HandoffReport, 1));
}

// ---- phase 3: the Enter gate's additive API ---------------------------------------------------

#[test]
fn held_behind_gate_counts_only_what_waits_behind_that_gate() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("a"));
    q.open_gate(&mut h, GateSpec::new(GateKind::EnterVerdict, 5));
    assert_eq!(q.held_behind_gate(GateKind::EnterVerdict, 5), 0);
    q.offer(&mut h, text("b"));
    q.offer(&mut h, EntryKind::Enter);
    assert_eq!(q.held_behind_gate(GateKind::EnterVerdict, 5), 2);
    assert_eq!(
        q.held_behind_gate(GateKind::EnterVerdict, 6),
        0,
        "a gate that is not open holds nothing"
    );
    assert_eq!(h.delivered_ids(), vec![1]);
}

#[test]
fn close_gate_taking_returns_what_waited_without_delivering_it() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer(&mut h, text("a"));
    q.open_gate(&mut h, GateSpec::new(GateKind::EnterVerdict, 5));
    q.offer(&mut h, text("b"));
    q.offer(&mut h, EntryKind::Enter);
    q.offer(&mut h, text("c"));
    let taken = q.close_gate_taking(&mut h, GateKind::EnterVerdict, 5);
    assert_eq!(
        taken.iter().map(|entry| entry.id.0).collect::<Vec<_>>(),
        vec![2, 3, 4],
        "in typed order"
    );
    assert_eq!(
        h.delivered_ids(),
        vec![1],
        "what was taken is the caller's to place; nothing of it is delivered"
    );
    assert!(q.is_idle());
    // A second close finds no gate and no entries.
    assert!(
        q.close_gate_taking(&mut h, GateKind::EnterVerdict, 5)
            .is_empty()
    );
}

#[test]
fn close_gate_taking_leaves_what_a_later_gate_holds() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(&mut h, GateSpec::new(GateKind::EnterVerdict, 1));
    q.offer(&mut h, text("b"));
    q.open_gate(&mut h, GateSpec::new(GateKind::PasteReading, 9));
    q.offer(&mut h, text("c"));
    let taken = q.close_gate_taking(&mut h, GateKind::EnterVerdict, 1);
    assert_eq!(taken.len(), 1, "only the entry behind the Enter's own gate");
    assert_eq!(q.queued_len(), 1, "the later gate still holds its entry");
    assert!(h.delivered.is_empty());
    q.on_gate_event(&mut h, GateKind::PasteReading, 9);
    assert_eq!(h.delivered_ids(), vec![2]);
}

#[test]
fn enter_gate_releases_keys_in_typed_order_on_its_event() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::EnterVerdict, 1).with_cap(None),
    );
    for key in ["a", "b"] {
        q.offer(&mut h, text(key));
    }
    q.offer(&mut h, EntryKind::Enter);
    q.offer(&mut h, text("c"));
    assert!(h.delivered.is_empty());
    assert!(q.on_gate_event(&mut h, GateKind::EnterVerdict, 1));
    assert_eq!(h.delivered_ids(), vec![1, 2, 3, 4]);
    assert!(
        !q.on_gate_event(&mut h, GateKind::EnterVerdict, 1),
        "a late duplicate event is a no-op"
    );
}

#[test]
fn debug_output_never_contains_typed_text() {
    let kinds = [
        text("hunter2"),
        EntryKind::Raw(b"hunter2".to_vec()),
        EntryKind::Paste {
            paste_id: 1,
            payload: Some(b"hunter2".to_vec()),
        },
    ];
    for kind in kinds {
        assert!(!format!("{kind:?}").contains("hunter2"));
    }
}

// ---- model-based test -----------------------------------------------------------------------

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

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

#[derive(Clone, Debug)]
struct RItem {
    id: EntryId,
    kind: EntryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RGate {
    kind: GateKind,
    key: u64,
    barrier: EntryId,
    seq: u64,
}

/// The reference: recompute-everything, written from the spec, not from the queue's code.
#[derive(Default)]
struct Model {
    next_id: u64,
    next_seq: u64,
    pending: Vec<RItem>,
    gates: Vec<RGate>,
    pin: Option<Destination>,
    delivered: Vec<Delivery>,
    discarded: Vec<(EntryId, DiscardReason)>,
    /// Entries the host turns into a hold the first time they are delivered.
    requeue: HashMap<EntryId, (GateKind, u64)>,
}

struct World {
    facts: DestFacts,
    enter_opens_gate: bool,
    ssh_signal_waits: bool,
}

impl Model {
    fn new() -> Self {
        Self {
            next_id: 1,
            next_seq: 1,
            ..Self::default()
        }
    }

    fn blocked(&self, id: EntryId) -> bool {
        self.gates.iter().any(|g| g.barrier <= id)
    }

    fn open_gate(&mut self, kind: GateKind, key: u64, barrier: EntryId) {
        self.gates.push(RGate {
            kind,
            key,
            barrier,
            seq: self.next_seq,
        });
        self.next_seq += 1;
    }

    fn deliver(&mut self, item: RItem, w: &World) {
        if matches!(item.kind, EntryKind::Paste { payload: None, .. }) {
            self.discarded.push((item.id, DiscardReason::PasteNotReady));
            return;
        }
        let computed = destination_for(&item.kind, &w.facts);
        // Pty bytes and editor keys know where they go; the line-run pin is for the rest.
        let intrinsic = intrinsic_destination(&item.kind);
        let line_part = item.kind.is_line_content();
        let mut dest = computed;
        if let Some(own) = intrinsic {
            dest = own;
        } else if !matches!(computed, Destination::Drop(_))
            && (line_part || item.kind == EntryKind::Enter)
            && let Some(pin) = self.pin
            && pin_still_valid(pin, &w.facts)
        {
            dest = pin;
        }
        if let Destination::Drop(reason) = dest {
            self.discarded
                .push((item.id, DiscardReason::Dropped(reason)));
            if item.kind == EntryKind::Enter {
                self.pin = None;
            }
            return;
        }
        if item.kind == EntryKind::Enter || matches!(item.kind, EntryKind::Signal(_)) {
            self.pin = None;
        } else if line_part {
            self.pin = Some(dest);
        }
        if let Some((kind, key)) = self.requeue.remove(&item.id) {
            // The later stage took the entry into a hold: back to the front under the new gate.
            let barrier = item.id;
            self.pending.insert(0, item);
            self.open_gate(kind, key, barrier);
            return;
        }
        self.delivered.push(Delivery {
            id: item.id,
            tag: item.kind.tag(),
            dest,
        });
        if item.kind == EntryKind::Enter && w.enter_opens_gate {
            self.open_gate(GateKind::HandoffReport, item.id.0, EntryId(item.id.0 + 1));
        }
    }

    fn drain(&mut self, w: &World) {
        while !self.pending.is_empty() && !self.blocked(self.pending[0].id) {
            let item = self.pending.remove(0);
            self.deliver(item, w);
        }
    }

    fn offer(&mut self, kind: EntryKind, gate: Option<(GateKind, u64)>, w: &World) -> EntryId {
        let id = EntryId(self.next_id);
        self.next_id += 1;
        let item = RItem { id, kind };
        if matches!(item.kind, EntryKind::Signal(_)) {
            self.signal(item, w);
            return id;
        }
        self.pending.push(item);
        if let Some((kind, key)) = gate {
            self.open_gate(kind, key, id);
        }
        self.drain(w);
        id
    }

    fn earliest(&self) -> Option<RGate> {
        self.gates
            .iter()
            .copied()
            .min_by_key(|g| (g.barrier, g.seq))
    }

    fn signal(&mut self, item: RItem, w: &World) {
        while let Some(g) = self.earliest() {
            if w.ssh_signal_waits && g.kind == GateKind::SshConnect {
                self.pending.push(item);
                return;
            }
            self.gates.retain(|x| x.seq != g.seq);
            match g.kind.signal_plan() {
                SignalPlan::CancelPasteThenFlush => {
                    let (gone, kept): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|p| {
                        matches!(&p.kind, EntryKind::Paste { paste_id, .. } if *paste_id == g.key)
                    });
                    self.pending = kept;
                    for p in gone {
                        self.discarded.push((p.id, DiscardReason::PasteCancelled));
                    }
                    self.drain(w);
                }
                SignalPlan::FlushThenSignal => self.drain(w),
                SignalPlan::DiscardThenSignal => {
                    let upper = self
                        .gates
                        .iter()
                        .map(|x| x.barrier)
                        .filter(|b| *b > g.barrier)
                        .min();
                    let (gone, kept): (Vec<_>, Vec<_>) = self
                        .pending
                        .drain(..)
                        .partition(|p| p.id >= g.barrier && upper.is_none_or(|u| p.id < u));
                    self.pending = kept;
                    for p in gone {
                        self.discarded
                            .push((p.id, DiscardReason::SignalDiscard(g.kind)));
                    }
                    self.drain(w);
                }
                SignalPlan::SignalThenRelease => {
                    self.deliver(item, w);
                    self.drain(w);
                    return;
                }
                SignalPlan::SignalThenKeepGate => {
                    // Gate stays: put it back.
                    self.gates.push(g);
                    self.deliver(item, w);
                    self.drain(w);
                    return;
                }
            }
        }
        self.deliver(item, w);
        self.drain(w);
    }

    fn close(&mut self, kind: GateKind, key: u64, w: &World) -> bool {
        let before = self.gates.len();
        self.gates.retain(|g| !(g.kind == kind && g.key == key));
        let closed = self.gates.len() != before;
        self.drain(w);
        closed
    }
}

fn random_kind(rng: &mut Rng) -> EntryKind {
    match rng.below(10) {
        0..=3 => EntryKind::Text("x".repeat(1 + rng.below(3))),
        4 | 5 => EntryKind::Enter,
        6 => EntryKind::Control("backspace"),
        7 => EntryKind::Seq("left"),
        8 => EntryKind::Raw(vec![0x1b, b'[']),
        _ => EntryKind::Text("y".to_owned()),
    }
}

fn random_signal(rng: &mut Rng) -> EntryKind {
    EntryKind::Signal(match rng.below(4) {
        0 | 1 => SignalKind::Interrupt,
        2 => SignalKind::Eof,
        _ => SignalKind::Suspend,
    })
}

fn check_equal(seed: u64, step: usize, op: &str, q: &InputQueue, h: &MockHost, m: &Model) {
    let ctx = format!("seed={seed} step={step} op={op}");
    assert_eq!(h.delivered, m.delivered, "delivered mismatch: {ctx}");
    assert_eq!(h.discarded, m.discarded, "discarded mismatch: {ctx}");
    let real_pending: Vec<EntryId> = q.queued_ids().collect();
    let model_pending: Vec<EntryId> = m.pending.iter().map(|p| p.id).collect();
    assert_eq!(real_pending, model_pending, "queued mismatch: {ctx}");
    let mut real_gates: Vec<(u64, GateKind, u64, EntryId)> = q
        .gates()
        .iter()
        .map(|g| (g.token.0, g.kind, g.key, g.barrier))
        .collect();
    real_gates.sort_by_key(|g| g.0);
    let mut model_gates: Vec<(u64, GateKind, u64, EntryId)> = m
        .gates
        .iter()
        .map(|g| (g.seq, g.kind, g.key, g.barrier))
        .collect();
    model_gates.sort_by_key(|g| g.0);
    assert_eq!(real_gates, model_gates, "gates mismatch: {ctx}");

    // Invariants that do not depend on the model.
    // 1. A non-empty queue has a blocked head: nothing waits behind an open path.
    assert_eq!(
        q.head_blocked(),
        !real_pending.is_empty(),
        "head not blocked: {ctx}"
    );
    // 2. Non-signal entries are delivered/discarded in arrival order: no reordering.
    let mut last = 0;
    for d in h.delivered.iter().filter(|d| d.tag != "signal") {
        assert!(
            d.id.0 > last,
            "reordered delivery {:?} after #{last}: {ctx}",
            d.id
        );
        last = d.id.0;
    }
    // 3. Nothing delivered while a gate held it: every queued id is >= the head's barrier path,
    //    i.e. no queued entry has a smaller id than a delivered non-signal one.
    if let Some(first_queued) = real_pending.first() {
        for d in h.delivered.iter().filter(|d| d.tag != "signal") {
            assert!(
                d.id < *first_queued,
                "delivered {:?} past queued head: {ctx}",
                d.id
            );
        }
    }
    // 4. Conservation: every issued id is exactly one of delivered, discarded or queued.
    let mut seen: HashMap<u64, u32> = HashMap::new();
    for d in &h.delivered {
        *seen.entry(d.id.0).or_default() += 1;
    }
    for (id, _) in &h.discarded {
        *seen.entry(id.0).or_default() += 1;
    }
    for id in &real_pending {
        *seen.entry(id.0).or_default() += 1;
    }
    for id in 1..q.peek_next_id().0 {
        assert_eq!(
            seen.get(&id).copied(),
            Some(1),
            "entry #{id} lost or duplicated: {ctx}"
        );
    }
}

fn run_seed(seed: u64, steps: usize) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    let mut m = Model::new();
    let mut w = World {
        facts: open_facts(),
        enter_opens_gate: false,
        ssh_signal_waits: false,
    };
    let mut next_paste = 1u64;
    let mut dead_tokens: Vec<GateToken> = Vec::new();
    let mut live_give_ups = 0u64;
    for step in 0..steps {
        let op;
        match rng.below(100) {
            0..=39 => {
                op = "offer";
                let kind = random_kind(&mut rng);
                let a = q.offer(&mut h, kind.clone());
                let b = m.offer(kind, None, &w);
                assert_eq!(a, b);
            }
            40..=47 => {
                op = "paste";
                let id = next_paste;
                next_paste += 1;
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
            48..=54 => {
                op = "paste_result";
                // Sometimes a stale or never-issued id.
                let id = 1 + rng.below(next_paste as usize + 1) as u64;
                let ok = rng.chance(70);
                q.on_paste_read(&mut h, id, if ok { Ok(vec![7]) } else { Err(()) });
                // Model: same rules written out.
                let pos = m.pending.iter().position(
                    |p| matches!(&p.kind, EntryKind::Paste { paste_id, .. } if *paste_id == id),
                );
                if let Some(pos) = pos {
                    if ok {
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
            55..=62 => {
                op = "gate_event";
                let (kind, key) = if !m.gates.is_empty() && rng.chance(80) {
                    let g = m.gates[rng.below(m.gates.len())];
                    (g.kind, g.key)
                } else {
                    (
                        GateKind::ALL[rng.below(GateKind::ALL.len())],
                        rng.below(4) as u64,
                    )
                };
                let a = q.on_gate_event(&mut h, kind, key);
                let b = m.close(kind, key, &w);
                assert_eq!(a, b);
            }
            63..=69 => {
                op = "open_gate";
                let kind = GateKind::ALL[rng.below(GateKind::ALL.len())];
                let key = rng.below(4) as u64;
                q.open_gate(&mut h, GateSpec::new(kind, key));
                let barrier = EntryId(m.next_id);
                m.open_gate(kind, key, barrier);
            }
            70..=75 => {
                op = "give_up_live";
                if !q.gates().is_empty() {
                    let g = q.gates()[rng.below(q.gates().len())];
                    assert!(q.give_up(&mut h, g.token));
                    live_give_ups += 1;
                    dead_tokens.push(g.token);
                    // Model: the gate closes; a paste gate also discards its unready paste.
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
            76..=79 => {
                op = "give_up_stale";
                // A timer for a gate that already closed: no effect at all.
                let token = if dead_tokens.is_empty() || rng.chance(30) {
                    GateToken(10_000 + rng.below(100) as u64)
                } else {
                    dead_tokens[rng.below(dead_tokens.len())]
                };
                let warns = h.warns.len();
                assert!(
                    !q.give_up(&mut h, token),
                    "stale cap fired at seed={seed} step={step}"
                );
                assert_eq!(h.warns.len(), warns);
            }
            80..=86 => {
                op = "signal";
                let kind = random_signal(&mut rng);
                let a = q.offer(&mut h, kind.clone());
                let b = m.offer(kind, None, &w);
                assert_eq!(a, b);
            }
            87..=89 => {
                op = "focus";
                w.facts.editor_focused = !w.facts.editor_focused;
                h.facts = w.facts;
                q.on_focus_changed(&mut h);
                m.drain(&w);
            }
            90..=95 => {
                op = "facts";
                match rng.below(6) {
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
            96..=97 => {
                op = "enter_opens";
                w.enter_opens_gate = !w.enter_opens_gate;
                h.enter_opens_gate = w.enter_opens_gate;
            }
            _ => {
                op = "ssh_wait";
                w.ssh_signal_waits = !w.ssh_signal_waits;
                h.ssh_signal_waits = w.ssh_signal_waits;
            }
        }
        h.facts = w.facts;
        // Track tokens that closed by event too, so stale timers get exercised.
        for (token, _, _) in &h.armed {
            if !q.gates().iter().any(|g| g.token == *token) && !dead_tokens.contains(token) {
                dead_tokens.push(*token);
            }
        }
        check_equal(seed, step, op, &q, &h, &m);
    }
    // Give-up fired exactly once per live gate that was closed that way, and nothing else.
    assert_eq!(q.give_up_count(), live_give_ups);
    assert_eq!(h.warns.len() as u64, live_give_ups);
    // Never wedged: every remaining gate opens by its event, then the next key goes through.
    for gate in q.gates().to_vec() {
        q.on_gate_event(&mut h, gate.kind, gate.key);
        m.close(gate.kind, gate.key, &w);
    }
    // Events can open new gates (Enter delivery); close until quiet.
    for _ in 0..1000 {
        let Some(gate) = q.gates().first().copied() else {
            break;
        };
        q.on_gate_event(&mut h, gate.kind, gate.key);
        m.close(gate.kind, gate.key, &w);
    }
    assert!(q.gates().is_empty(), "seed={seed}");
    // A paste still unready at this point was never answered; its gate is gone, so the entry was
    // delivered/discarded by now.
    check_equal(seed, steps, "final", &q, &h, &m);
    assert_eq!(q.queued_len(), 0, "seed={seed}: queue wedged");
    h.enter_opens_gate = false;
    h.facts = open_facts();
    let before = h.delivered.len();
    q.offer(&mut h, text("probe"));
    assert_eq!(
        h.delivered.len(),
        before + 1,
        "seed={seed}: next key must go through"
    );
}

#[test]
fn model_random_interleavings_hold_the_invariants() {
    for seed in 1..=400u64 {
        run_seed(seed, 250);
    }
}

#[test]
fn model_long_runs() {
    for seed in 1000..1003u64 {
        run_seed(seed, 3000);
    }
}

// ---- phase 4: the ssh connect hold ----------------------------------------------------------

fn held(class: u8, s: &str) -> EntryKind {
    EntryKind::Held {
        class,
        text: s.to_owned(),
    }
}

#[test]
fn held_entries_wait_behind_the_ssh_gate_and_the_close_delivers_them_in_order() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 3).with_cap(None),
    );
    q.offer(&mut h, held(0, "echo "));
    q.offer(&mut h, held(1, "\r"));
    q.offer(&mut h, held(3, "\u{1b}[A"));
    assert!(h.delivered.is_empty());
    assert!(h.armed.is_empty(), "the ssh gate carries no cap of its own");
    assert_eq!(q.entries().count(), 3);
    assert!(q.on_gate_event(&mut h, GateKind::SshConnect, 3));
    assert_eq!(h.delivered_ids(), vec![1, 2, 3]);
    assert!(q.is_idle());
    assert!(
        !q.on_gate_event(&mut h, GateKind::SshConnect, 3),
        "a second hand-over finds nothing"
    );
    assert_eq!(h.delivered.len(), 3);
}

#[test]
fn drop_gate_removes_the_gate_and_its_entries_without_delivering() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 1).with_cap(None),
    );
    q.offer(&mut h, held(0, "blind"));
    q.offer(&mut h, held(1, "\r"));
    let removed = q.drop_gate(GateKind::SshConnect, 1);
    assert_eq!(removed.len(), 2);
    assert!(matches!(&removed[0].kind, EntryKind::Held { class: 0, text } if text == "blind"));
    assert!(q.is_idle());
    assert!(h.delivered.is_empty());
    // Keys typed after the drop pass straight through.
    q.offer(&mut h, text("yes"));
    assert_eq!(h.delivered.len(), 1);
    assert!(
        q.drop_gate(GateKind::SshConnect, 1).is_empty(),
        "no gate, nothing to drop"
    );
}

#[test]
fn drop_gate_leaves_entries_behind_a_later_gate_waiting() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 1).with_cap(None),
    );
    q.offer(&mut h, held(0, "a"));
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SubshellSetup, 0).with_cap(None),
    );
    q.offer(&mut h, text("b"));
    let removed = q.drop_gate(GateKind::SshConnect, 1);
    assert_eq!(removed.len(), 1);
    assert_eq!(
        q.entries().count(),
        1,
        "b still waits for the subshell gate"
    );
}

// ---- #1353 phase 5: Requeue ------------------------------------------------------------------
// Not compiled/run when written.

/// A delivery that the later stage turned into a hold puts the entry back at the front under the
/// new gate: nothing is delivered, nothing behind it overtakes, and the gate's event takes the
/// entry out again, in typed order, together with everything typed since.
#[test]
fn a_requeued_entry_holds_its_place_and_everything_behind_it() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    h.requeue
        .insert(EntryId(2), GateSpec::new(GateKind::LineEditorActive, 9));
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b")); // requeued under the gate
    q.offer(&mut h, text("c"));
    q.offer(&mut h, EntryKind::Enter);
    assert_eq!(
        h.delivered_ids(),
        vec![1],
        "only the entry before the hold went out"
    );
    assert!(q.has_gate(GateKind::LineEditorActive, 9));
    assert_eq!(
        q.queued_ids().map(|id| id.0).collect::<Vec<_>>(),
        vec![2, 3, 4],
        "the held entry is first, the keys behind it in typed order"
    );
    assert_eq!(q.held_behind_gate(GateKind::LineEditorActive, 9), 3);
    // The event takes them out in order; nothing is delivered by the queue itself.
    let taken = q.close_gate_taking(&mut h, GateKind::LineEditorActive, 9);
    assert_eq!(
        taken.iter().map(|entry| entry.id.0).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    assert!(q.is_idle());
    assert_eq!(h.delivered_ids(), vec![1]);
    // Later keys flow again.
    q.offer(&mut h, text("d"));
    assert_eq!(h.delivered_ids(), vec![1, 5]);
}

/// A delivery that wrote part of its entry (a routed paste, an ssh-held text) and had the rest held
/// by the later stage puts back what is left, under the same id: the gate still holds everything
/// behind it, and the event takes out the remainder, not the entry that was delivered.
#[test]
fn a_requeued_entry_can_come_back_as_what_the_hold_kept() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    h.requeue
        .insert(EntryId(1), GateSpec::new(GateKind::LineEditorActive, 4));
    h.requeue_as.insert(EntryId(1), pty_bytes(b"kept"));
    q.offer(&mut h, text("whole line"));
    q.offer(&mut h, text("next"));
    assert!(q.has_gate(GateKind::LineEditorActive, 4));
    let taken = q.close_gate_taking(&mut h, GateKind::LineEditorActive, 4);
    assert_eq!(
        taken.iter().map(|entry| entry.id.0).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        taken[0].kind,
        pty_bytes(b"kept"),
        "the remainder, not the delivered text"
    );
    assert_eq!(taken[1].kind, text("next"));
}

/// The hold's cap is the gate's: armed once with the spec's value, and a given-up gate delivers
/// what it held through the ordinary stages, in order (the host decides to release it itself
/// before that; this is the net under it).
#[test]
fn a_requeued_gate_arms_its_cap_and_gives_up_in_order() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    let spec =
        GateSpec::new(GateKind::LineEditorActive, 4).with_cap(Some(Duration::from_millis(1500)));
    h.requeue.insert(EntryId(1), spec);
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b"));
    assert_eq!(h.armed.len(), 1);
    assert_eq!(h.armed[0].1, GateKind::LineEditorActive);
    assert_eq!(h.armed[0].2, Duration::from_millis(1500));
    assert!(h.delivered.is_empty());
    let token = h.armed[0].0;
    assert!(q.give_up(&mut h, token));
    assert_eq!(h.delivered_ids(), vec![1, 2]);
    assert!(h.warns.len() == 1);
    assert!(!q.give_up(&mut h, token), "one-shot");
}

/// A signal meets the hold's gate: held keys are delivered, in order, ahead of the signal
/// (`FlushThenSignal`), as for every gate of this family.
#[test]
fn a_signal_flushes_a_requeued_hold_in_typed_order() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    h.requeue
        .insert(EntryId(1), GateSpec::new(GateKind::LineEditorActive, 1));
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b"));
    q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.delivered_ids(), vec![1, 2, 3]);
    assert!(q.is_idle());
}

// ---- #1353 phase 5b: gate kinds of their own -------------------------------------------------

/// A paced CLI-agent submit holds the keys typed behind it on a `CliSubmit` gate: its key can
/// equal a paste's id without the two touching, only its own event opens it, and a signal flushes
/// what was typed before it (no paste entry exists to cancel).
#[test]
fn a_cli_submit_gate_is_its_own_kind_and_never_a_paste() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(&mut h, GateSpec::new(GateKind::CliSubmit, 1));
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b"));
    assert!(h.delivered.is_empty());
    // A paste read finishing under the same number is not the submit's event.
    q.on_paste_read(&mut h, 1, Ok(b"p".to_vec()));
    assert!(h.delivered.is_empty());
    assert!(q.has_gate(GateKind::CliSubmit, 1));
    assert!(q.on_gate_event(&mut h, GateKind::CliSubmit, 1));
    assert_eq!(h.delivered_ids(), vec![1, 2]);
    assert!(q.is_idle());

    // Its cap is the gate's: the keys come out, in order, and nothing is reported as a paste.
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::CliSubmit, 2).with_cap(Some(Duration::from_secs(20))),
    );
    q.offer(&mut h, text("c"));
    let token = h.armed.last().unwrap().0;
    assert_eq!(h.armed.last().unwrap().1, GateKind::CliSubmit);
    assert!(q.give_up(&mut h, token));
    assert_eq!(h.delivered_ids(), vec![1, 2, 3]);
    assert!(h.discarded.is_empty());
}

/// An Enter waiting for a Tab completion is a `TabCompletion` gate, distinct from one waiting
/// for the verdict, with the same signal rule: what was typed behind the Enter is dropped with it.
#[test]
fn a_tab_completion_gate_is_distinct_from_the_verdict_gate_and_drops_its_keys_on_a_signal() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(&mut h, GateSpec::new(GateKind::TabCompletion, 7));
    q.open_gate(&mut h, GateSpec::new(GateKind::EnterVerdict, 7));
    assert!(q.has_gate(GateKind::TabCompletion, 7));
    assert!(q.has_gate(GateKind::EnterVerdict, 7));
    assert_eq!(
        GateKind::TabCompletion.signal_plan(),
        GateKind::EnterVerdict.signal_plan()
    );
    // The verdict's event does not open the Tab completion's gate.
    assert!(q.on_gate_event(&mut h, GateKind::EnterVerdict, 7));
    assert!(q.has_gate(GateKind::TabCompletion, 7));
    q.offer(&mut h, text("a"));
    assert!(h.delivered.is_empty());
    q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(h.discarded.len(), 1);
    assert_eq!(
        h.discarded[0].1,
        DiscardReason::SignalDiscard(GateKind::TabCompletion)
    );
    assert!(q.is_idle());
}

/// Every gate kind has a distinct name (the trace and the logs key on it).
#[test]
fn gate_kind_names_are_unique() {
    let mut names: Vec<&str> = GateKind::ALL.iter().map(|kind| kind.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), GateKind::ALL.len());
}

// ---- retag_gate (a young hold's gate becomes a deferred Enter's, #1353 / #1403) --------------

fn pty_bytes(bytes: &[u8]) -> EntryKind {
    EntryKind::PtyBytes {
        bytes: bytes.to_vec(),
        takes_over_agent_control: true,
    }
}

fn held_bytes(entries: &[InputEntry]) -> Vec<Vec<u8>> {
    entries
        .iter()
        .map(|entry| match &entry.kind {
            EntryKind::PtyBytes { bytes, .. } => bytes.clone(),
            other => panic!("unexpected entry {other:?}"),
        })
        .collect()
}

#[test]
fn retag_gate_keeps_everything_held_under_a_new_token_and_drops_the_enters_first_byte() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer_gated(
        &mut h,
        pty_bytes(b"\rx"),
        GateSpec::new(GateKind::PromptSeen, 5).with_cap(None),
    );
    q.offer(&mut h, pty_bytes(b"y"));
    let old = q.gates()[0].token;
    let spec = GateSpec::new(GateKind::HandoffReport, 9).with_cap(Some(Duration::from_secs(60)));
    assert!(q.retag_gate(&mut h, GateKind::PromptSeen, 5, spec, true));
    assert!(!q.has_gate(GateKind::PromptSeen, 5));
    assert!(q.has_gate(GateKind::HandoffReport, 9));
    assert_ne!(
        q.gates()[0].token,
        old,
        "a new token: the old cap finds nothing"
    );
    assert_eq!(
        h.armed.last().map(|armed| armed.1),
        Some(GateKind::HandoffReport)
    );
    assert!(!q.give_up(&mut h, old));
    assert!(h.delivered.is_empty(), "nothing was released by the retag");
    assert_eq!(q.queued_len(), 2);
    let taken = q.close_gate_taking(&mut h, GateKind::HandoffReport, 9);
    assert_eq!(held_bytes(&taken), vec![b"x".to_vec(), b"y".to_vec()]);
}

#[test]
fn retag_gate_removes_an_entry_the_enter_emptied_and_keeps_the_barrier() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.offer_gated(
        &mut h,
        pty_bytes(b"\r"),
        GateSpec::new(GateKind::PromptSeen, 1).with_cap(None),
    );
    q.offer(&mut h, pty_bytes(b"a"));
    assert!(q.retag_gate(
        &mut h,
        GateKind::PromptSeen,
        1,
        GateSpec::new(GateKind::HandoffReport, 2).with_cap(None),
        true,
    ));
    assert_eq!(q.queued_len(), 1);
    // A key typed after the retag still queues behind the gate.
    q.offer(&mut h, pty_bytes(b"b"));
    assert!(h.delivered.is_empty());
    let taken = q.close_gate_taking(&mut h, GateKind::HandoffReport, 2);
    assert_eq!(held_bytes(&taken), vec![b"a".to_vec(), b"b".to_vec()]);
    assert!(q.is_idle());
}

#[test]
fn retagging_a_gate_that_is_not_open_changes_nothing() {
    let mut q = InputQueue::new();
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 3).with_cap(None),
    );
    assert!(!q.retag_gate(
        &mut h,
        GateKind::PromptSeen,
        3,
        GateSpec::new(GateKind::HandoffReport, 4).with_cap(None),
        true,
    ));
    assert!(q.has_gate(GateKind::SshConnect, 3));
    assert!(!q.has_gate(GateKind::HandoffReport, 4));
}

// ---- the cap on queued entries (#1442) ------------------------------------------------------

/// A gate that stays shut while keys keep arriving: the queue stops growing at its cap, drops the
/// NEWEST arrivals (the older ones keep their order), and tells the host once.
#[test]
fn a_full_queue_drops_the_newest_arrivals_and_tells_the_host_once() {
    let mut q = InputQueue::new();
    q.set_max_entries(4);
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 1).with_cap(None),
    );
    for key in [b"a", b"b", b"c", b"d", b"e", b"f", b"g"] {
        q.offer(&mut h, pty_bytes(key));
    }
    assert_eq!(q.queued_len(), 4, "bounded at the cap");
    assert_eq!(q.overflow_dropped(), 3);
    assert_eq!(h.overflows, vec![(true, 1), (false, 2), (false, 3)]);
    assert!(
        h.warns.iter().any(|w| w.contains("input queue full")),
        "the first refusal is logged as a warning: {:?}",
        h.warns
    );
    // What was kept is the OLDEST four, in typed order.
    let kept = q.close_gate_taking(&mut h, GateKind::SshConnect, 1);
    assert_eq!(
        held_bytes(&kept),
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]
    );
}

/// A signal is never dropped for the cap: ^C must always get through.
#[test]
fn a_full_queue_still_takes_a_signal() {
    let mut q = InputQueue::new();
    q.set_max_entries(2);
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 1).with_cap(None),
    );
    for key in ["a", "b", "c"] {
        q.offer(&mut h, text(key));
    }
    assert_eq!(q.overflow_dropped(), 1);
    q.offer(&mut h, EntryKind::Signal(SignalKind::Interrupt));
    assert_eq!(q.overflow_dropped(), 1, "the signal was not refused");
}

/// The episode ends when the queue goes idle: the next overflow is `first` again (the user is told
/// again), and the count restarts.
#[test]
fn the_overflow_episode_resets_when_the_queue_is_idle() {
    let mut q = InputQueue::new();
    q.set_max_entries(1);
    let mut h = MockHost::new();
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 1).with_cap(None),
    );
    q.offer(&mut h, text("a"));
    q.offer(&mut h, text("b"));
    assert_eq!(h.overflows, vec![(true, 1)]);
    q.close_gate_taking(&mut h, GateKind::SshConnect, 1);
    // Idle again: an offer delivers inline and the episode count restarts.
    q.offer(&mut h, text("c"));
    assert_eq!(q.overflow_dropped(), 0);
    q.open_gate(
        &mut h,
        GateSpec::new(GateKind::SshConnect, 2).with_cap(None),
    );
    q.offer(&mut h, text("d"));
    q.offer(&mut h, text("e"));
    assert_eq!(h.overflows.last(), Some(&(true, 1)));
}

/// The real default bound: 180 s of the fastest key repeat (about 60 Hz) is far below it.
#[test]
fn the_default_cap_is_far_above_a_long_gate_at_key_repeat_rate() {
    assert!(MAX_QUEUED_ENTRIES >= 180 * 60 * 5);
}

#[cfg(test)]
mod prop_model;
pub mod ops;
