//! One ordered input queue (#1353, design phase 0: the pure core, not wired into the view yet).
//!
//! OWNER RULE: correctness never depends on timing. Everything typed or pasted is an
//! [`InputEntry`] with a monotonic id. An entry is held only by a [`Gate`], and a gate is opened
//! again by a NAMED EVENT ([`GateKind::closing_event`]). The only timers are the give-up caps a
//! gate requests through [`Host::arm_give_up`]: one-shot, token-guarded (a stale timer finds no
//! gate and does nothing), logged as a warning when they fire. The queue itself never starts a
//! timer, polls, or reads a clock for a decision.
//!
//! Model:
//! - Strict FIFO. A gate blocks the entry it was opened at and everything behind it; there is no
//!   per-queue bypass, so no overtaking by construction.
//! - The destination (editor, pty, agent input, drop) is decided when the entry is DELIVERED, from
//!   current facts, with line-run pinning (a line is not split across destinations by a focus flip).
//! - Signals (^C ^D ^Z ^\) never wait; they apply the per-gate rule of [`GateKind::signal_plan`].
//! - `drain` is the only place anything is delivered. A [`Host`] cannot call back into the queue
//!   while it delivers; it hands follow-ups (more entries, gate events) to [`FollowUps`], which
//!   the same drain applies in order. That is the re-entrancy safety: no delivery ever re-enters
//!   the pipeline and gets held a second time.
//! - Content is never logged (it can be a password): `Debug` prints kind and length only.
//!
//! Phase 0 has no production consumer: the module is exercised by its tests and by the trace hops
//! in `trace`, which record today's buffers for later comparison.

// No consumer until phase 1; the whole public surface is intentionally unused for now.
#![allow(dead_code)]

mod destination;
mod gates;
pub mod kill_switch;
pub mod trace;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

pub use destination::{DestFacts, Destination, DropReason, destination_for, pin_still_valid};
pub use gates::{Gate, GateKind, GateSpec, GateToken, SignalPlan};

/// Monotonic id assigned at arrival; it is the entry's position in the one order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(pub u64);

impl fmt::Debug for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalKind {
    /// ^C
    Interrupt,
    /// ^D
    Eof,
    /// ^Z
    Suspend,
    /// ^\
    Quit,
}

/// Destination-agnostic content, as the user produced it.
#[derive(Clone, PartialEq, Eq)]
pub enum EntryKind {
    Text(String),
    Enter,
    Control(&'static str),
    Seq(&'static str),
    /// A paste. `payload` is `None` until the reader delivers it ([`InputQueue::on_paste_read`]);
    /// a paste still `None` when it reaches the head is discarded, never delivered empty.
    Paste {
        paste_id: u64,
        payload: Option<Vec<u8>>,
    },
    Signal(SignalKind),
    /// Pty-only bytes from passive paths (mouse reports); dropped when the agent controls the
    /// command.
    Raw(Vec<u8>),
}

impl EntryKind {
    pub fn tag(&self) -> &'static str {
        match self {
            EntryKind::Text(_) => "text",
            EntryKind::Enter => "enter",
            EntryKind::Control(_) => "control",
            EntryKind::Seq(_) => "seq",
            EntryKind::Paste { .. } => "paste",
            EntryKind::Signal(_) => "signal",
            EntryKind::Raw(_) => "raw",
        }
    }

    /// Byte length of the content, for logs.
    pub fn len(&self) -> usize {
        match self {
            EntryKind::Text(text) => text.len(),
            EntryKind::Control(s) | EntryKind::Seq(s) => s.len(),
            EntryKind::Paste { payload, .. } => payload.as_ref().map_or(0, Vec::len),
            EntryKind::Raw(bytes) => bytes.len(),
            EntryKind::Enter | EntryKind::Signal(_) => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Part of a line being typed (pinned to one destination until the next Enter or signal).
    fn is_line_content(&self) -> bool {
        matches!(
            self,
            EntryKind::Text(_)
                | EntryKind::Control(_)
                | EntryKind::Seq(_)
                | EntryKind::Paste { .. }
        )
    }
}

impl fmt::Debug for EntryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryKind::Signal(kind) => write!(f, "signal({kind:?})"),
            EntryKind::Paste { paste_id, payload } => write!(
                f,
                "paste(id={paste_id}, ready={}, len={})",
                payload.is_some(),
                self.len()
            ),
            other => write!(f, "{}(len={})", other.tag(), other.len()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Keyboard,
    /// Re-offered by a delivery follow-up.
    Replay(EntryId),
    /// Synthesized (mouse report, wheel to arrow).
    Synthesized,
}

#[derive(Clone, Debug)]
pub struct InputEntry {
    pub id: EntryId,
    pub kind: EntryKind,
    /// Focus generation at arrival (design 2.6).
    pub focus_epoch: u64,
    /// Diagnostics only, never a decision input.
    pub arrived_at: Instant,
    pub origin: Origin,
}

/// What delivering one entry caused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliverOutcome {
    Delivered,
    /// The delivery (an Enter that now waits for a verdict or report) closes the queue behind it.
    OpensGate(GateSpec),
}

/// Why an entry was removed without being delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardReason {
    Dropped(DropReason),
    /// A signal discarded what was held (`DiscardThenSignal`).
    SignalDiscard(GateKind),
    /// A signal cancelled the paste.
    PasteCancelled,
    /// The paste read failed.
    PasteFailed,
    /// The paste never became ready (its gate gave up).
    PasteNotReady,
}

/// Later work a delivery asks for. Applied by the queue, in order, right after that delivery.
#[derive(Debug, Default)]
pub struct FollowUps {
    items: Vec<Follow>,
}

#[derive(Debug)]
enum Follow {
    Offer(EntryKind),
    OpenGate(GateSpec),
    GateEvent(GateKind, u64),
}

impl FollowUps {
    /// Queue another entry behind everything already queued (a replayed key).
    pub fn offer(&mut self, kind: EntryKind) {
        self.items.push(Follow::Offer(kind));
    }

    /// Close the queue behind the entry being delivered.
    pub fn open_gate(&mut self, spec: GateSpec) {
        self.items.push(Follow::OpenGate(spec));
    }

    /// A gate-closing event that happened while delivering.
    pub fn gate_event(&mut self, kind: GateKind, key: u64) {
        self.items.push(Follow::GateEvent(kind, key));
    }
}

/// Everything the queue needs from the outside world. Implemented by the terminal view (later
/// phases) and by a mock in the tests.
pub trait Host {
    /// Current facts for the destination decision. Plain data; must not block.
    fn facts(&self) -> DestFacts;

    /// Deliver `entry` to `dest` using the LEAF writers (never the pipeline that holds). The
    /// destination is never `Drop`; drops go to [`Host::discarded`].
    fn deliver(
        &mut self,
        entry: &InputEntry,
        dest: Destination,
        follow: &mut FollowUps,
    ) -> DeliverOutcome;

    /// An entry was removed without delivery.
    fn discarded(&mut self, entry: &InputEntry, why: DiscardReason);

    /// Request ONE timer that calls [`InputQueue::give_up`] with `token` after `after`. This is
    /// the only way the queue touches time. The host must not re-arm it.
    fn arm_give_up(&mut self, token: GateToken, kind: GateKind, after: Duration);

    /// Whether a signal meeting `gate` must wait like any entry (an interrupt must not land in
    /// the middle of the remote bootstrap, #1001).
    fn signal_must_wait(&self, _gate: GateKind) -> bool {
        false
    }

    /// A give-up cap fired for a gate whose event never came; always logged, at warning level.
    fn warn(&mut self, line: &str);

    /// Cheap check so the queue only formats trace lines when someone listens.
    fn log_enabled(&self) -> bool {
        false
    }

    /// Trace line (`input_order`, debug). Counts and ids only.
    fn log(&mut self, _line: &str) {}
}

#[derive(Debug)]
pub struct InputQueue {
    next_id: u64,
    next_token: u64,
    focus_epoch: u64,
    entries: VecDeque<InputEntry>,
    gates: Vec<Gate>,
    /// Destination of the line being typed.
    line_pin: Option<Destination>,
    give_ups: u64,
}

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl InputQueue {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            next_token: 1,
            focus_epoch: 0,
            entries: VecDeque::new(),
            gates: Vec::new(),
            line_pin: None,
            give_ups: 0,
        }
    }

    // ---- inspection -------------------------------------------------------------------------

    pub fn queued_len(&self) -> usize {
        self.entries.len()
    }

    pub fn queued_ids(&self) -> impl Iterator<Item = EntryId> + '_ {
        self.entries.iter().map(|entry| entry.id)
    }

    pub fn gates(&self) -> &[Gate] {
        &self.gates
    }

    pub fn focus_epoch(&self) -> u64 {
        self.focus_epoch
    }

    /// Nothing queued and no gate open: the fast path applies.
    pub fn is_idle(&self) -> bool {
        self.entries.is_empty() && self.gates.is_empty()
    }

    /// The id the next arrival will get.
    pub fn peek_next_id(&self) -> EntryId {
        EntryId(self.next_id)
    }

    /// Number of caps that fired for a gate that was still open.
    pub fn give_up_count(&self) -> u64 {
        self.give_ups
    }

    /// Whether the head entry waits behind a gate. Invariant: a non-empty queue always has a
    /// blocked head (otherwise the last drain would have delivered it).
    pub fn head_blocked(&self) -> bool {
        self.entries
            .front()
            .is_some_and(|head| self.is_blocked(head.id))
    }

    fn is_blocked(&self, id: EntryId) -> bool {
        self.gates.iter().any(|gate| gate.barrier <= id)
    }

    // ---- arrivals ---------------------------------------------------------------------------

    /// A new entry arrives. Signals apply their gate rule; everything else is delivered now if
    /// nothing holds it, else queued behind the gate. Returns the entry's id.
    pub fn offer<H: Host>(&mut self, host: &mut H, kind: EntryKind) -> EntryId {
        self.offer_with_origin(host, kind, Origin::Keyboard)
    }

    pub fn offer_with_origin<H: Host>(
        &mut self,
        host: &mut H,
        kind: EntryKind,
        origin: Origin,
    ) -> EntryId {
        let entry = self.make_entry(kind, origin);
        let id = entry.id;
        if matches!(entry.kind, EntryKind::Signal(_)) {
            self.handle_signal(host, entry);
        } else if self.is_idle() {
            // Fast path: nothing held, nothing queued; deliver inline without touching the deque.
            self.deliver_entry(host, &entry);
            self.drain(host);
        } else {
            self.entries.push_back(entry);
            self.drain(host);
        }
        id
    }

    /// An entry that is held by its own gate (a paste: the entry sits in the queue as pending
    /// until its read result arrives). Never takes the fast path.
    pub fn offer_gated<H: Host>(
        &mut self,
        host: &mut H,
        kind: EntryKind,
        spec: GateSpec,
    ) -> (EntryId, GateToken) {
        let entry = self.make_entry(kind, Origin::Keyboard);
        let id = entry.id;
        self.entries.push_back(entry);
        let token = self.push_gate(host, spec, id);
        self.drain(host);
        (id, token)
    }

    /// A gate opens from now on: entries arriving after this call wait; what is already queued
    /// keeps its place and is not newly held.
    pub fn open_gate<H: Host>(&mut self, host: &mut H, spec: GateSpec) -> GateToken {
        let barrier = EntryId(self.next_id);
        self.push_gate(host, spec, barrier)
    }

    fn make_entry(&mut self, kind: EntryKind, origin: Origin) -> InputEntry {
        let id = EntryId(self.next_id);
        self.next_id += 1;
        InputEntry {
            id,
            kind,
            focus_epoch: self.focus_epoch,
            arrived_at: Instant::now(),
            origin,
        }
    }

    fn push_gate<H: Host>(&mut self, host: &mut H, spec: GateSpec, barrier: EntryId) -> GateToken {
        let token = GateToken(self.next_token);
        self.next_token += 1;
        self.gates.push(Gate {
            kind: spec.kind,
            key: spec.key,
            token,
            barrier,
            cap: spec.cap,
        });
        if host.log_enabled() {
            host.log(&format!(
                "gate_open kind={} key={} token={} barrier={:?} opened_by={} closes_on={}",
                spec.kind.name(),
                spec.key,
                token.0,
                barrier,
                spec.kind.opened_by(),
                spec.kind.closing_event()
            ));
        }
        if let Some(cap) = spec.cap {
            host.arm_give_up(token, spec.kind, cap);
        }
        token
    }

    // ---- events -----------------------------------------------------------------------------

    /// The named closing event for `(kind, key)` happened. Returns whether a gate was open for it
    /// (an event for a gate that is already gone is a no-op, as a late duplicate must be).
    pub fn on_gate_event<H: Host>(&mut self, host: &mut H, kind: GateKind, key: u64) -> bool {
        let closed = self.close_gates(host, kind, key, "event");
        self.drain(host);
        closed
    }

    /// The paste reader finished. `Ok` fills the payload and closes the paste's gate; `Err`
    /// removes the paste entry (reported as discarded) and closes the gate. Stale ids are ignored.
    pub fn on_paste_read<H: Host>(
        &mut self,
        host: &mut H,
        paste_id: u64,
        result: Result<Vec<u8>, ()>,
    ) {
        let position = self.entries.iter().position(
            |entry| matches!(&entry.kind, EntryKind::Paste { paste_id: id, .. } if *id == paste_id),
        );
        if let Some(position) = position {
            match result {
                Ok(bytes) => {
                    if let EntryKind::Paste { payload, .. } = &mut self.entries[position].kind {
                        *payload = Some(bytes);
                    }
                }
                Err(()) => {
                    if let Some(entry) = self.entries.remove(position) {
                        host.discarded(&entry, DiscardReason::PasteFailed);
                    }
                }
            }
        }
        self.close_gates(host, GateKind::PasteReading, paste_id, "paste_read");
        self.drain(host);
    }

    /// A give-up cap fired. ONE-SHOT and id-guarded: only the gate that still carries `token` is
    /// closed; a timer for a gate that an event already closed returns `false` and changes
    /// nothing. A paste that gives up is discarded (the held keys behind it are released in order),
    /// matching today's backstop.
    pub fn give_up<H: Host>(&mut self, host: &mut H, token: GateToken) -> bool {
        let Some(index) = self.gates.iter().position(|gate| gate.token == token) else {
            return false;
        };
        let gate = self.gates.remove(index);
        self.give_ups += 1;
        host.warn(&format!(
            "input queue gate gave up: kind={} key={} token={} cap={:?}; the closing event ({}) \
             never came",
            gate.kind.name(),
            gate.key,
            gate.token.0,
            gate.cap,
            gate.kind.closing_event()
        ));
        if gate.kind == GateKind::PasteReading {
            self.discard_where(host, DiscardReason::PasteNotReady, |entry| {
                matches!(&entry.kind, EntryKind::Paste { paste_id, payload: None } if *paste_id == gate.key)
            });
        }
        self.drain(host);
        true
    }

    /// Focus moved. The epoch is stamped on later arrivals; entries already queued are NOT
    /// re-targeted (line-run pinning). A re-drain is harmless and lets a head that waited on
    /// destination facts go.
    pub fn on_focus_changed<H: Host>(&mut self, host: &mut H) {
        self.focus_epoch += 1;
        self.drain(host);
    }

    /// Facts changed in a way that can unblock nothing by itself but that the host wants applied
    /// to a head that is not gated (kept for symmetry; a no-op when the queue is empty).
    pub fn poke<H: Host>(&mut self, host: &mut H) {
        self.drain(host);
    }

    fn close_gates<H: Host>(
        &mut self,
        host: &mut H,
        kind: GateKind,
        key: u64,
        how: &'static str,
    ) -> bool {
        let before = self.gates.len();
        let logging = host.log_enabled();
        let mut closed_lines = Vec::new();
        self.gates.retain(|gate| {
            let matches = gate.kind == kind && gate.key == key;
            if matches && logging {
                closed_lines.push(format!(
                    "gate_close kind={} key={} token={} by={how}",
                    gate.kind.name(),
                    gate.key,
                    gate.token.0
                ));
            }
            !matches
        });
        for line in closed_lines {
            host.log(&line);
        }
        self.gates.len() != before
    }

    // ---- the one drain ----------------------------------------------------------------------

    /// Deliver, in arrival order, every entry that no gate holds. The ONLY place queued input is
    /// delivered. Triggered by an arrival, a gate-closing event, a give-up, a focus change or a
    /// signal; never polled.
    fn drain<H: Host>(&mut self, host: &mut H) {
        while let Some(head) = self.entries.front() {
            if self.is_blocked(head.id) {
                break;
            }
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.deliver_entry(host, &entry);
        }
    }

    /// Decide the destination NOW and hand the entry to the host.
    fn deliver_entry<H: Host>(&mut self, host: &mut H, entry: &InputEntry) {
        if let EntryKind::Paste { payload: None, .. } = &entry.kind {
            host.discarded(entry, DiscardReason::PasteNotReady);
            return;
        }
        let facts = host.facts();
        let computed = destination_for(&entry.kind, &facts);
        let dest = match (computed, self.line_pin) {
            (Destination::Drop(_), _) => computed,
            (_, Some(pin)) if entry.kind.is_line_content() || entry.kind == EntryKind::Enter => {
                if pin_still_valid(pin, &facts) {
                    pin
                } else {
                    computed
                }
            }
            _ => computed,
        };
        if host.log_enabled() {
            host.log(&format!(
                "deliver id={:?} kind={} len={} dest={dest:?} epoch={}",
                entry.id,
                entry.kind.tag(),
                entry.kind.len(),
                entry.focus_epoch
            ));
        }
        // Pin bookkeeping: content pins the line to where it went; Enter and signals end the line.
        match (&entry.kind, dest) {
            (_, Destination::Drop(reason)) => {
                host.discarded(entry, DiscardReason::Dropped(reason));
                if entry.kind == EntryKind::Enter {
                    self.line_pin = None;
                }
                return;
            }
            (EntryKind::Enter | EntryKind::Signal(_), _) => self.line_pin = None,
            (kind, dest) if kind.is_line_content() => self.line_pin = Some(dest),
            _ => {}
        }
        let mut follow = FollowUps::default();
        let outcome = host.deliver(entry, dest, &mut follow);
        if let DeliverOutcome::OpensGate(spec) = outcome {
            // Behind the delivered entry: whatever already queued after it waits too.
            self.push_gate(host, spec, EntryId(entry.id.0 + 1));
        }
        self.apply_follow_ups(host, entry.id, follow);
    }

    fn apply_follow_ups<H: Host>(&mut self, host: &mut H, from: EntryId, follow: FollowUps) {
        for item in follow.items {
            match item {
                Follow::Offer(kind) => {
                    let entry = self.make_entry(kind, Origin::Replay(from));
                    self.entries.push_back(entry);
                }
                Follow::OpenGate(spec) => {
                    self.push_gate(host, spec, EntryId(from.0 + 1));
                }
                Follow::GateEvent(kind, key) => {
                    self.close_gates(host, kind, key, "follow_up_event");
                }
            }
        }
    }

    // ---- signals ----------------------------------------------------------------------------

    fn earliest_gate(&self) -> Option<Gate> {
        self.gates
            .iter()
            .copied()
            .min_by_key(|gate| (gate.barrier, gate.token))
    }

    /// Apply the gate rules (design 2.5) to a signal. Each pass resolves the earliest gate; a
    /// signal always resolves the gates it passes (so the queue cannot stay wedged behind one),
    /// except `SignalThenKeepGate`, which leaves its gate for its own event.
    fn handle_signal<H: Host>(&mut self, host: &mut H, entry: InputEntry) {
        while let Some(gate) = self.earliest_gate() {
            if host.signal_must_wait(gate.kind) {
                // Waits like any entry, behind the gate.
                self.entries.push_back(entry);
                return;
            }
            let held_before = self.entries.len();
            match gate.kind.signal_plan() {
                SignalPlan::CancelPasteThenFlush => {
                    self.gates.retain(|g| g.token != gate.token);
                    self.discard_where(host, DiscardReason::PasteCancelled, |held| {
                        matches!(&held.kind, EntryKind::Paste { paste_id, .. } if *paste_id == gate.key)
                    });
                    self.drain(host);
                }
                SignalPlan::FlushThenSignal => {
                    self.gates.retain(|g| g.token != gate.token);
                    self.drain(host);
                }
                SignalPlan::DiscardThenSignal => {
                    self.gates.retain(|g| g.token != gate.token);
                    // Entries of this gate's segment: from its barrier to the next gate's.
                    let upper = self
                        .gates
                        .iter()
                        .map(|g| g.barrier)
                        .filter(|barrier| *barrier > gate.barrier)
                        .min();
                    self.discard_where(host, DiscardReason::SignalDiscard(gate.kind), |held| {
                        held.id >= gate.barrier && upper.is_none_or(|upper| held.id < upper)
                    });
                    self.drain(host);
                }
                SignalPlan::SignalThenRelease => {
                    self.deliver_entry(host, &entry);
                    self.gates.retain(|g| g.token != gate.token);
                    self.drain(host);
                    return;
                }
                SignalPlan::SignalThenKeepGate => {
                    self.deliver_entry(host, &entry);
                    self.drain(host);
                    return;
                }
            }
            if host.log_enabled() {
                host.log(&format!(
                    "signal gate={} held_before={held_before} held_after={}",
                    gate.kind.name(),
                    self.entries.len()
                ));
            }
        }
        self.deliver_entry(host, &entry);
        self.drain(host);
    }

    fn discard_where<H: Host>(
        &mut self,
        host: &mut H,
        why: DiscardReason,
        mut pick: impl FnMut(&InputEntry) -> bool,
    ) {
        let mut kept = VecDeque::with_capacity(self.entries.len());
        let mut removed = Vec::new();
        for entry in self.entries.drain(..) {
            if pick(&entry) {
                removed.push(entry);
            } else {
                kept.push_back(entry);
            }
        }
        self.entries = kept;
        for entry in removed {
            host.discarded(&entry, why);
        }
    }
}
