//! The handle every consumer of a pane's input queue goes through (#1353 step 3, design (a1)/(a3)).
//!
//! Until the editor and the terminal view share one [`InputQueue`] each pane has two of them, and
//! the "is a drain running right now" question was answered by the `RefCell`'s borrow state
//! (`try_borrow` failing). With one queue shared by several views that signal has to be explicit,
//! so this wrapper owns:
//!
//! * the queue itself, reached only through [`PaneQueue::lock`] / [`PaneQueue::try_lock`] (mutable,
//!   and the only way a drain can run) and [`PaneQueue::peek`] / [`PaneQueue::try_peek`] (shared,
//!   inspection); `script/check_queue_access` bans raw `.borrow()` / `.borrow_mut()` on the queue
//!   in production code,
//! * an explicit `draining` flag: true while a [`QueueGuard`] is alive. Anything that asks "is the
//!   queue busy delivering?" reads it, instead of inferring it from a failed borrow,
//! * the "ssh hold is active" fact the pane's key sink reads to route a key (#1404). There is no
//!   identity guard for a key two routes could both offer (#1339/#1400): the sink receives each
//!   key once,
//! * the ONE counter of paste ids, so an editor paste and a pty paste of one pane can never share
//!   an id (`has_gate(PasteReading, id)` would alias). The high bit tags the owner; see
//!   [`PasteOwner`].
//!
//! * the DEFERRED-OPERATION mailbox ([`Deferred`], see the design note in `input_queue/mod.rs`):
//!   an operation that needs the queue while it is busy (a drain is delivering, or a peek is
//!   held) records itself here instead of being dropped, written as typed or left half done. The
//!   terminal view applies the mailbox, in order, as soon as it is outside any operation.
//!
//! Nothing here delivers, times or decides anything: it is the shape of access, not behaviour.

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::terminal::model::typeahead_handoff::ViewKeysHeld;

use super::{
    Destination, EntryId, EntryKind, GateKind, GateSpec, GateToken, InputQueue, Origin,
    PaneRouteState, QueueFacts, Stamp, intrinsic_destination,
};

/// Paste ids owned by the input editor have this bit set; the pty's do not. One counter hands out
/// both (the low bits), the tag only says who delivers the paste.
const EDITOR_PASTE_BIT: u64 = 1 << 63;

/// Who delivers a paste entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteOwner {
    /// The input editor (an editor-bound paste, read by the terminal `Input`).
    Editor,
    /// The pty (a terminal-bound paste, read by the terminal view).
    Pty,
}

/// An operation that needed the queue while it was busy, recorded as plain data and applied by
/// the terminal view (`TerminalView::flush_deferred_queue_ops`) in the order it was recorded,
/// the next time the queue is free. One mechanism for every "busy" branch: never a dropped key,
/// never a key written as typed, never a one-shot cap consumed for nothing.
#[derive(Debug)]
pub enum Deferred {
    /// `InputQueue::offer_with_origin`. `stamp` is what the key had at the sink (#1424), taken
    /// before the `try_lock` that failed: applying the offer later does not re-stamp it.
    Offer {
        kind: EntryKind,
        origin: Origin,
        stamp: Stamp,
    },
    /// A key held behind `spec`: joins the gate if it is open already, else opens it with the
    /// entry (`offer_gated`).
    OfferGated { kind: EntryKind, spec: GateSpec },
    /// `InputQueue::open_gate`.
    OpenGate(GateSpec),
    /// `InputQueue::on_gate_event`.
    GateEvent { kind: GateKind, key: u64 },
    /// A give-up cap fired for `token` while the queue was busy: run the cap's handler again
    /// (it is token-guarded and idempotent).
    GiveUpCap(GateToken),
    /// The reader's result for paste `id` (the payload is already stored by the caller).
    PasteRead { id: u64, ok: bool },
    /// The line editor signalled (or the hold gave up) while the queue was busy.
    ReleasePromptHold { hold_id: u64, gave_up: bool },
    /// An interrupt (^C, ^Z, ^\\) voided the prompt hold `hold_id` while the queue was busy
    /// (#1584): the pty keys held before the signal (ids below `before`) are dropped, as the tty
    /// flushes its typeahead; editor entries are delivered; keys that arrived after the signal go
    /// out in order.
    VoidPromptHold { hold_id: u64, before: EntryId },
    /// A young hold's keys are to be written (its give-up found the queue busy).
    ReleaseYoungHoldKeys { hold_id: u64, routed: bool },
    /// The hand-off recorded young-hold ends that could not be applied while the queue was busy.
    ApplyYoungHoldEnds,
    /// A drain stopped at an entry only the view can deliver, and the view was busy.
    Poke,
    /// Focus moved while the queue was busy: advance the focus epoch and drain once the queue is
    /// free (`InputQueue::on_focus_changed`, #1424 step 7).
    FocusChanged,
    /// Programmatic editor edits (and the pending command waiting behind them) that waited for
    /// an Enter's gate: apply them, in order, now that the gate operation ahead of this one ran
    /// (#1434, #1435).
    FlushProgrammaticEdits,
    /// Operations that RETURN what they take out of the queue (#1446): the caller could not get
    /// the bytes (the queue was busy), so the take and the caller's continuation (`then`) are one
    /// deferred operation, run in order once the queue is free. Nothing is returned empty.
    /// Closes `HandoffReport` gate `gate_id` and hands the keys held behind it to `then`.
    TakeGateKeys { gate_id: u64, then: GateKeysThen },
    /// Closes the PowerShell setup gate and hands the keys it held to `then`.
    TakePowershellKeys { then: PowershellKeysThen },
    /// The waiting Enter's gate is void (a give-up, ^C, an edit of its line): closes it and takes
    /// what was typed behind it, as `report` says: a give-up or an edit WRITES it to the pty in
    /// typed order (owner decision 2026-11-09, reversing #1661); ^C and the multi-line dialog's
    /// answer discard it (#1571, #1652). #1768: the busy queue gives the free one's result.
    VoidEnterGate {
        kind: GateKind,
        key: u64,
        report: crate::terminal::input::EnterDiscard,
    },
    /// `TerminalView::give_up_gate_keys_to_pty` on a busy queue: closes gate `(kind, key)`, whose
    /// timer gave up, and WRITES what it held to the pty in typed order (owner decision
    /// 2026-11-09, reversing #1661). Applied in FIFO order, so a key typed after the give-up
    /// (deferred behind it) is offered once the gate is gone and follows what the gate held.
    GiveUpGateKeysToPty {
        kind: GateKind,
        key: u64,
        why: &'static str,
    },
    /// Drops the ssh connect gate `gate` and the pastes still reading behind it (`paste_ids`),
    /// then does `then` with what they held. The view's own state was already detached.
    DropSshGate {
        gate: Option<u64>,
        paste_ids: Vec<u64>,
        then: SshDropThen,
    },
}

/// What happens to the keys of a `Deferred::TakeGateKeys` once taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateKeysThen {
    /// The deferred Enter wrote no command: the keys are released (sent on, in order).
    Release,
    /// A timer gave up on the deferred Enter (the reason names it: the report patience, the
    /// uncovered-report patience, a verdict cap, the gate's leak net). The Enter is not run and
    /// its line stays in the editor; the keys typed behind it are WRITTEN TO THE PTY in typed
    /// order (owner decision 2026-11-09, reversing #1661). A bare Enter (an empty editor line)
    /// goes to the pty in front of them: it was a key too.
    GiveUp(&'static str),
    /// #2000 (owner decision 2026-11-11): an EVENT ended the deferred Enter without running it
    /// (the reason names it: the editor refused the report as another line's text, the young line
    /// cannot be classified, the young wait keeps the line in the editor) and its line stays in
    /// the editor. The keys typed behind it stay WITH that line, as text on the lines after it,
    /// in typed order, under the hand-back guard, as the timer paths place them; nothing reaches
    /// the pty (a later line never runs without the line typed before it). With an EMPTY editor
    /// line (a bare Enter) there is no unrun line to keep them with: they are released exactly
    /// as [`Self::Release`] releases them.
    KeepWithLine(&'static str),
}

/// What happens to the keys of a `Deferred::TakePowershellKeys` once taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowershellKeysThen {
    /// The setup is over: the held lines are delivered.
    Deliver,
    /// Ctrl-C discarded the line being typed: what was held through the last Enter stays held.
    CancelHeldLine,
}

/// What happens to the inputs of a `Deferred::DropSshGate` once taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshDropThen {
    /// Forgotten.
    Discard,
    /// Shown in the editor as unexecuted text (the connect ended without a remote shell).
    HandBackText,
    /// Written to the pty once, in typed order, for the LOCAL shell: a managed ssh that failed or
    /// ended with no wrapper verdict, as plain ssh in a plain terminal leaves the typeahead in the
    /// tty for the shell (#1641, owner decision after r18u16).
    WriteToLocalPty,
    /// A connect give-up (the gate's ceilings, the first-Precmd wait cap) fired: written to the
    /// pty once, in typed order, NOW, for whatever reads the tty at this moment (the ssh process
    /// while it is still the foreground program), as a plain terminal would have sent the keys
    /// all along (owner decision 2026-11-09, reversing #1661). Never kept for the local shell
    /// that follows a later ssh exit.
    GiveUpToPty,
}

impl Deferred {
    /// #2182: whether this waiting operation counts as "the view holds keys" for the pty
    /// controller's in-band wait. A deferred offer is judged by its entry, exactly as a queued
    /// one ([`InputQueue::holds_entry_not_bound_for_editor`]): an editor key or an editor paste
    /// can only reach the editor. Every other operation counts as held: it may carry, release or
    /// re-route keys for the pty, and what it does is only known once it has run.
    fn may_hold_keys_not_bound_for_editor(&self) -> bool {
        match self {
            Deferred::Offer { kind, .. } | Deferred::OfferGated { kind, .. } => {
                intrinsic_destination(kind) != Some(Destination::Editor)
            }
            Deferred::OpenGate(_)
            | Deferred::GateEvent { .. }
            | Deferred::GiveUpCap(_)
            | Deferred::PasteRead { .. }
            | Deferred::ReleasePromptHold { .. }
            | Deferred::VoidPromptHold { .. }
            | Deferred::ReleaseYoungHoldKeys { .. }
            | Deferred::ApplyYoungHoldEnds
            | Deferred::Poke
            | Deferred::FocusChanged
            | Deferred::FlushProgrammaticEdits
            | Deferred::TakeGateKeys { .. }
            | Deferred::TakePowershellKeys { .. }
            | Deferred::VoidEnterGate { .. }
            | Deferred::GiveUpGateKeysToPty { .. }
            | Deferred::DropSshGate { .. } => true,
        }
    }
}

/// #2182: the fact the pane publishes to [`ViewKeysHeld`]: the queue holds an entry, or the
/// mailbox an operation, that is not provably bound for the input editor. Keys that can only
/// reach the editor (held behind a `RemoteTabCompletion` gate, say) are not in the way of the
/// app's own in-band command: its kill prefix cannot touch them and no write can overtake them.
/// Publishing them as held made the completer's own in-band listing wait for the keys that were
/// waiting for the completer.
fn holds_keys_not_bound_for_editor(queue: &InputQueue, deferred: &VecDeque<Deferred>) -> bool {
    queue.holds_entry_not_bound_for_editor()
        || deferred
            .iter()
            .any(Deferred::may_hold_keys_not_bound_for_editor)
}

/// Whether paste `id` belongs to the input editor.
pub fn is_editor_paste_id(id: u64) -> bool {
    id & EDITOR_PASTE_BIT != 0
}

#[derive(Debug)]
struct Inner {
    queue: RefCell<InputQueue>,
    /// True exactly while a [`QueueGuard`] is alive: some operation (and so possibly a drain) holds
    /// the queue.
    draining: Cell<bool>,
    next_paste_id: Cell<u64>,
    /// The view is holding the keys typed during an ssh connect (set by the view, read by the
    /// pane's key sink).
    ssh_hold_active: Cell<bool>,
    /// What the destination table decides on that lives at the pane level (#1424): the stamp's
    /// parts and the phases the view derives. Written by the view, read by anyone.
    route_state: PaneRouteState,
    /// Operations that found the queue busy, in the order they were recorded.
    deferred: RefCell<VecDeque<Deferred>>,
    /// Wakes the terminal view to apply [`Self::deferred`] (a bounded channel of one).
    defer_waker: RefCell<Option<async_channel::Sender<()>>>,
    /// The view is applying the deferred operations (no nested flush).
    flushing_deferred: Cell<bool>,
    /// #2182: where "the view holds user keys" is published (the hand-off's cell), if wired.
    keys_held: RefCell<Option<ViewKeysHeld>>,
}

/// A cheap, clonable handle on the pane's queue (clones share the queue).
#[derive(Clone, Debug)]
pub struct PaneQueue {
    inner: Rc<Inner>,
}

impl Default for PaneQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl PaneQueue {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                queue: RefCell::new(InputQueue::new()),
                draining: Cell::new(false),
                next_paste_id: Cell::new(0),
                ssh_hold_active: Cell::new(false),
                route_state: PaneRouteState::default(),
                deferred: RefCell::new(VecDeque::new()),
                defer_waker: RefCell::new(None),
                flushing_deferred: Cell::new(false),
                keys_held: RefCell::new(None),
            }),
        }
    }

    /// Whether `other` is the same queue (two handles of one pane).
    pub fn ptr_eq(&self, other: &PaneQueue) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }

    /// #2182: where this queue publishes whether it holds user keys that are not provably bound
    /// for the input editor (entries queued, or an operation waiting for the queue; see
    /// [`holds_keys_not_bound_for_editor`]), every time an operation on it ends. The keys stay in
    /// the queue; the cell only tells the pty controller to wait with the app's own in-band
    /// command.
    pub fn set_keys_held_sink(&self, sink: Option<ViewKeysHeld>) {
        *self.inner.keys_held.borrow_mut() = sink;
    }

    /// Publishes the held fact when no operation holds the queue (a guard publishes its own on
    /// drop); a no-op while one does.
    fn refresh_keys_held(&self) {
        let Some(sink) = self.inner.keys_held.borrow().clone() else {
            return;
        };
        let Ok(queue) = self.inner.queue.try_borrow() else {
            return;
        };
        sink.publish(holds_keys_not_bound_for_editor(
            &queue,
            &self.inner.deferred.borrow(),
        ));
    }

    /// The view holds (or stops holding) the keys typed during an ssh connect.
    pub fn set_ssh_hold_active(&self, active: bool) {
        self.inner.ssh_hold_active.set(active);
    }

    /// The view is holding the keys typed during an ssh connect.
    pub fn ssh_hold_active(&self) -> bool {
        self.inner.ssh_hold_active.get()
    }

    /// The pane-level route state (stamp parts and derived phases).
    pub fn route_state(&self) -> &PaneRouteState {
        &self.inner.route_state
    }

    /// A stamp from the pane's route state: what the sink noted for the key being handled.
    /// Plain `Cell` reads; callable while the queue is busy, which is the point (an offer that
    /// must be deferred is stamped BEFORE it is deferred).
    pub fn stamp_now(&self) -> Stamp {
        self.inner.route_state.stamp()
    }

    /// [`InputQueue::queue_facts`], or `None` while an operation holds the queue (a caller inside
    /// a drain gets the facts from the queue itself).
    pub fn try_queue_facts(&self) -> Option<QueueFacts> {
        self.try_peek().map(|queue| queue.queue_facts())
    }

    /// An operation holds the queue right now (so a delivery that asks is part of it).
    pub fn is_draining(&self) -> bool {
        self.inner.draining.get()
    }

    /// Where [`Self::defer`] pokes: the terminal view's drain receiver.
    pub fn set_defer_waker(&self, waker: Option<async_channel::Sender<()>>) {
        *self.inner.defer_waker.borrow_mut() = waker;
    }

    /// Records `op` for the next time the queue is free and wakes the view to apply it. Always
    /// available (it never touches the queue itself), so a busy branch can call it.
    pub fn defer(&self, op: Deferred) {
        // #2182: an operation waits for the queue: keys may be behind it. An editor key's offer
        // changes nothing: it can only reach the editor.
        let holds_keys = op.may_hold_keys_not_bound_for_editor();
        self.inner.deferred.borrow_mut().push_back(op);
        if holds_keys && let Some(sink) = self.inner.keys_held.borrow().as_ref() {
            sink.publish(true);
        }
        self.poke_defer_waker();
    }

    /// Asks the view to look at the deferred operations again (idempotent).
    pub fn poke_defer_waker(&self) {
        if let Some(waker) = self.inner.defer_waker.borrow().as_ref() {
            let _ = waker.try_send(());
        }
    }

    /// Whether an operation waits for the queue.
    pub fn has_deferred(&self) -> bool {
        !self.inner.deferred.borrow().is_empty()
    }

    /// Whether an `OpenGate` / `OfferGated` for the gate `(kind, key)` waits in the mailbox.
    pub fn has_deferred_gate_open(&self, kind: GateKind, key: u64) -> bool {
        self.inner.deferred.borrow().iter().any(|op| match op {
            Deferred::OpenGate(spec) | Deferred::OfferGated { spec, .. } => {
                spec.kind == kind && spec.key == key
            }
            _ => false,
        })
    }

    /// The oldest deferred operation.
    pub fn pop_deferred(&self) -> Option<Deferred> {
        let op = self.inner.deferred.borrow_mut().pop_front();
        self.refresh_keys_held();
        op
    }

    /// Puts an operation back at the FRONT (it could not run yet; order is kept).
    pub fn unpop_deferred(&self, op: Deferred) {
        let holds_keys = op.may_hold_keys_not_bound_for_editor();
        self.inner.deferred.borrow_mut().push_front(op);
        if holds_keys {
            if let Some(sink) = self.inner.keys_held.borrow().as_ref() {
                sink.publish(true);
            }
        } else {
            // An editor key's offer went back: the fact is whatever the rest says.
            self.refresh_keys_held();
        }
    }

    /// Claims the right to apply the deferred operations; false if that is already going on.
    pub fn begin_deferred_flush(&self) -> bool {
        !self.inner.flushing_deferred.replace(true)
    }

    pub fn end_deferred_flush(&self) {
        self.inner.flushing_deferred.set(false);
    }

    /// The next paste id of this pane. One counter for both owners.
    pub fn next_paste_id(&self, owner: PasteOwner) -> u64 {
        let id = self.inner.next_paste_id.get() + 1;
        self.inner.next_paste_id.set(id);
        match owner {
            PasteOwner::Editor => id | EDITOR_PASTE_BIT,
            PasteOwner::Pty => id,
        }
    }

    /// Mutable access, for callers that are PROVABLY outside any operation (a test, a top-level
    /// event handler that checked [`Self::try_peek`] a line before). Anything reachable from a
    /// delivery uses [`Self::try_lock`] and [`Self::defer`]s on `None`. Panics in debug builds if
    /// the queue is held; in release it also panics (a re-entrant lock is a logic error), which is
    /// why production code does not call it.
    pub fn lock(&self) -> QueueGuard<'_> {
        match self.try_lock() {
            Some(guard) => guard,
            None => panic!("input queue: locked while it is already held (re-entrant access)"),
        }
    }

    /// Mutable access, or `None` while the queue is held (a drain is delivering, or somebody holds
    /// a [`Self::peek`]).
    pub fn try_lock(&self) -> Option<QueueGuard<'_>> {
        if self.inner.draining.get() {
            return None;
        }
        let mut guard = self.inner.queue.try_borrow_mut().ok()?;
        self.inner.draining.set(true);
        // Arrivals through this guard that bring no stamp of their own get the pane's current
        // one; the focus epoch is the queue's own.
        let mut stamp = self.inner.route_state.stamp();
        stamp.epoch = guard.focus_epoch();
        guard.set_stamp(stamp);
        // An owner-change edge asked for the line pin to go (#1424 step 7).
        if self.inner.route_state.take_pin_clear() {
            guard.clear_line_pin();
        }
        Some(QueueGuard {
            guard,
            draining: &self.inner.draining,
            route_state: &self.inner.route_state,
            keys_held: &self.inner.keys_held,
            deferred: &self.inner.deferred,
        })
    }

    /// As [`Self::try_lock`], but `None` as well while earlier operations wait in the mailbox:
    /// an arrival that took the lock then would overtake them. For the callers that cannot apply
    /// the mailbox themselves (the input editor); they [`Self::defer`] on `None`.
    pub fn try_lock_ordered(&self) -> Option<QueueGuard<'_>> {
        if self.has_deferred() {
            return None;
        }
        self.try_lock()
    }

    /// Shared access for inspection. Only for callers that know no operation is running; the
    /// ones that may run mid-drain use [`Self::try_peek`].
    pub fn peek(&self) -> Ref<'_, InputQueue> {
        self.inner.queue.borrow()
    }

    /// Shared access, or `None` while an operation holds the queue (mid-drain: what a caller would
    /// read is in flux, and a delivery is part of the operation).
    pub fn try_peek(&self) -> Option<Ref<'_, InputQueue>> {
        if self.inner.draining.get() {
            return None;
        }
        self.inner.queue.try_borrow().ok()
    }

    /// Tests read the queue freely.
    #[cfg(test)]
    pub fn borrow(&self) -> Ref<'_, InputQueue> {
        self.peek()
    }

    /// Tests drive the queue freely.
    #[cfg(test)]
    pub fn borrow_mut(&self) -> QueueGuard<'_> {
        self.lock()
    }
}

/// Mutable access to the queue; `draining` is true while it lives.
pub struct QueueGuard<'a> {
    guard: RefMut<'a, InputQueue>,
    draining: &'a Cell<bool>,
    route_state: &'a PaneRouteState,
    keys_held: &'a RefCell<Option<ViewKeysHeld>>,
    deferred: &'a RefCell<VecDeque<Deferred>>,
}

impl Deref for QueueGuard<'_> {
    type Target = InputQueue;

    fn deref(&self) -> &InputQueue {
        &self.guard
    }
}

impl DerefMut for QueueGuard<'_> {
    fn deref_mut(&mut self) -> &mut InputQueue {
        &mut self.guard
    }
}

impl Drop for QueueGuard<'_> {
    fn drop(&mut self) {
        // The epoch only changes under a guard; mirror it so a stamp taken while the queue is busy
        // (the editor's deferred offers) carries the current one.
        self.route_state.note_epoch(self.guard.focus_epoch());
        // #2182: whether the view holds user keys now, for the pty controller's in-band wait.
        if let Some(sink) = self.keys_held.borrow().as_ref() {
            sink.publish(holds_keys_not_bound_for_editor(
                &self.guard,
                &self.deferred.borrow(),
            ));
        }
        self.draining.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draining_is_explicit_and_follows_the_guard() {
        let pane = PaneQueue::new();
        assert!(!pane.is_draining());
        {
            let guard = pane.lock();
            assert!(pane.is_draining());
            assert!(pane.try_peek().is_none(), "nothing reads a queue in flux");
            assert!(pane.try_lock().is_none(), "no second holder");
            assert!(guard.is_idle());
        }
        assert!(!pane.is_draining());
        assert!(pane.try_peek().is_some_and(|queue| queue.is_idle()));
    }

    /// The pane records whether an ssh connect hold is active.
    #[test]
    fn the_ssh_hold_flag_follows_set_and_clear() {
        let pane = PaneQueue::new();
        assert!(!pane.ssh_hold_active());
        pane.set_ssh_hold_active(true);
        assert!(pane.ssh_hold_active());
        pane.set_ssh_hold_active(false);
        assert!(!pane.ssh_hold_active());
    }

    /// #1578: only an op that opens THE gate counts as the gate being alive.
    #[test]
    fn has_deferred_gate_open_matches_only_the_gates_own_ops() {
        let pane = PaneQueue::new();
        pane.defer(Deferred::FocusChanged);
        assert!(pane.has_deferred());
        assert!(!pane.has_deferred_gate_open(GateKind::SshConnect, 1));
        pane.defer(Deferred::OpenGate(
            GateSpec::new(GateKind::SshConnect, 1).with_cap(None),
        ));
        assert!(pane.has_deferred_gate_open(GateKind::SshConnect, 1));
        assert!(!pane.has_deferred_gate_open(GateKind::SshConnect, 2));
        assert!(!pane.has_deferred_gate_open(GateKind::PromptSeen, 1));
    }

    #[test]
    fn deferred_operations_keep_their_order_and_wake_the_view() {
        let pane = PaneQueue::new();
        let (tx, rx) = async_channel::bounded::<()>(1);
        pane.set_defer_waker(Some(tx));
        assert!(!pane.has_deferred());
        let _held = pane.lock();
        pane.defer(Deferred::Poke);
        pane.defer(Deferred::ApplyYoungHoldEnds);
        assert!(rx.try_recv().is_ok(), "the view is woken");
        assert!(pane.has_deferred());
        assert!(matches!(pane.pop_deferred(), Some(Deferred::Poke)));
        pane.unpop_deferred(Deferred::Poke);
        assert!(matches!(pane.pop_deferred(), Some(Deferred::Poke)));
        assert!(matches!(
            pane.pop_deferred(),
            Some(Deferred::ApplyYoungHoldEnds)
        ));
        assert!(pane.pop_deferred().is_none());
        assert!(pane.begin_deferred_flush());
        assert!(!pane.begin_deferred_flush(), "no nested flush");
        pane.end_deferred_flush();
        assert!(pane.begin_deferred_flush());
    }

    #[test]
    fn a_shared_peek_blocks_the_lock_without_looking_like_a_drain() {
        let pane = PaneQueue::new();
        let peek = pane.peek();
        assert!(pane.try_lock().is_none());
        assert!(!pane.is_draining(), "a reader is not a drain");
        drop(peek);
        assert!(pane.try_lock().is_some());
    }

    #[test]
    fn clones_share_one_queue_and_one_paste_counter() {
        let a = PaneQueue::new();
        let b = a.clone();
        assert!(a.ptr_eq(&b));
        let first = a.next_paste_id(PasteOwner::Pty);
        let second = b.next_paste_id(PasteOwner::Editor);
        let third = a.next_paste_id(PasteOwner::Pty);
        assert!(!is_editor_paste_id(first));
        assert!(is_editor_paste_id(second));
        assert!(!is_editor_paste_id(third));
        assert_eq!(first & !EDITOR_PASTE_BIT, 1);
        assert_eq!(second & !EDITOR_PASTE_BIT, 2);
        assert_eq!(third & !EDITOR_PASTE_BIT, 3);
        assert_ne!(first, second);
        assert!(!a.ptr_eq(&PaneQueue::new()));
    }
}
