//! One ordered input queue (#1353). Phase 0 is this pure core; phase 1 wires paste ordering onto it
//! (`view/paste_queue.rs` for the pty, `input/paste_queue.rs` and `editor/view/paste_hold.rs` for
//! the editor).
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
//! One queue per pane: the terminal view's [`PaneQueue`] is adopted by the input editor, so editor
//! keys and pty keys are ordered by one instance, and EVERY key from every route lands in it.
//! A [`Host`] says which entries it can deliver ([`Host::handles`]); a drain driven from the
//! editor side stops at a pty entry and the view continues it (`pending_foreign`). The editor's
//! wait for a fresh display map (#1389) is the [`GateKind::EditorDisplayMap`] gate. Every gate
//! holds every key ([`InputQueue::holds_keys`]); an entry records its origin ([`Origin`]) and its
//! content, and where it goes is decided when it is drained.
//!
//! Consumers: paste, the subshell setup / bootstrap holds, the Enter verdict / Tab / after-Enter
//! ordering plus the paced CLI-agent submit (`input/enter_queue.rs`), the ssh connect typeahead
//! (`view/ssh_queue.rs`) and the typeahead hand-off's holds: the ConPTY prompt hold
//! (`LineEditorActive` gate, entries put back with [`DeliverOutcome::Requeue`]), a young Enter
//! (`PromptSeen`) and the keys behind an Enter deferred for the shell's report (`HandoffReport`,
//! which a young hold's gate becomes in place with [`InputQueue::retag_gate`]); see
//! `view/paste_queue.rs`. The old holders and their `PHOSPHOR_INPUT_QUEUE_*` kill switches are
//! gone; the revert path is git.
//!
//! DESTINATION TABLE (#1424, steps 1 and 2 of the approved design; NOT LIVE). Where an input goes
//! is still decided by the chain of predicates inside delivery (`route_typed`, `keydown_on_terminal`,
//! `after_paste_stage`, `deliver_replayed_entry`, ...). The replacement is one pure function,
//! [`table::decide`]`(intent, &RouteFacts, pin) -> Plan { outcome, effects, on_refused, row }`
//! (`table.rs`: every hold row H1-H10, destination row D1-D13, signal row P1-P2 and origin row, the
//! four per-class pty predicates exactly as legacy computes them, the gate x class matrix of H8
//! including #1423's editor exemption, and `signal_plan` per gate and signal). What exists
//! and what does not:
//!  - LIVE (changes no behaviour): the facts and the stamp. [`RouteFacts`] is built by
//!    `TerminalView::build_route_facts` (`view/route_facts.rs`) without a blocking lock and without
//!    `in_key_path()`, from a consistent read of the two published model cells (a `facts_gen` in
//!    both, a reader retry until they agree), the pane's [`PaneRouteState`] and live reads. Every
//!    entry carries a [`Stamp`] taken at the sink (`on_pane_key_event`) -- the model gen it saw, the
//!    focus chain, the focus epoch -- through the deferred mailbox (`Deferred::Offer { .. stamp }`)
//!    and follow-ups, so a key judged later still has the classification it had when typed (the ssh
//!    password rule, H4). [`SshPhase`], [`SubshellPhase`] and [`PsPhase`] are DERIVED read-only
//!    from the legacy fields; no refresher moved off the key path.
//!  - PURE, NOT CALLED: `table.rs` (`decide`, the predicates, `gate_holds`, `destination_for_fixed`
//!    which fixes B12, the inverted hidden-input branch of `destination_for`), the `Origin` rows
//!    (Synced, Viewer, CliSubmit, ReleasedFrom), `TypeaheadHandoff::peek_user_bytes_route`, the
//!    `HELD_SIGNAL` held class (declared, no producer or executor).
//!  - THE SWITCH: `PHOSPHOR_QUEUE_DESTINATION=legacy|shadow|table` ([`DestinationMode`], parsed like
//!    `PHOSPHOR_PANE_KEY`). Default `legacy`. Nothing reads `shadow` or `table` yet.
//!  - DELIBERATE CHANGES encoded in the table but not yet effective (the owner's decisions OD-1,
//!    OD-5, OD-7): ^Z and ^\ jump the queue like ^C with a per-gate plan; a signal typed during the
//!    ssh setup window is HELD and delivered to the REMOTE side, never to the local shell; a key
//!    stamped before the ssh prompt was published is dropped even if it is processed later.
//!  - NEXT (design steps 3-8): the shadow comparison (`kh_dest_disagree`), delivery through the
//!    table (the #1415 fix), arrival of terminal keys, editor arrival and paste, the edge-driven
//!    `SshPhase`, then `table` as the default and the deletion of `delivering`/`requeue_request`.
//!
//! BUSY QUEUE = DEFERRED OPERATION (one mechanism, #1416-#1419, #1425-#1428). The queue is held
//! while a drain delivers (and while anyone holds a peek). An operation that needs it then must
//! never be dropped, written "as typed", left half done, or consume a one-shot cap for nothing.
//! Every such branch does `PaneQueue::try_lock()`, and on `None` records a [`Deferred`] value
//! (plain data: an offer, a gate open / event, a give-up cap, a paste result, a prompt-hold
//! release, ...) with `PaneQueue::defer`, which also wakes the terminal view. The view applies
//! the mailbox, in recorded order, the moment it is outside any operation
//! (`TerminalView::flush_deferred_queue_ops`), and every top-level entry that would otherwise
//! overtake it (a typed key, the next gate operation) applies it first. Consequences:
//!  - arrival order of keys is the order of the operations that carried them;
//!  - a give-up cap that fires mid-delivery is re-run after it (the handlers are token-guarded and
//!    idempotent), so the gate is still given up or re-armed;
//!  - entries taken out of a gate with `close_gate_taking` are never re-queued: an editor entry in
//!    the taken segment is applied to the editor at once (`FollowUps::flushing_now` skips the
//!    display-map wait), because the gate it would wait behind no longer exists;
//!  - `PtyQueueHost::deliver` marks EVERY delivery (editor entries too) so a key a later stage
//!    wants held is requeued by the drain instead of finding the queue busy.
//!  - operations that RETURN what they take out of the queue (the deferred Enter's gate keys, the
//!    PowerShell held bytes, the ssh gate's inputs, the void Enter's text; #1446) are never
//!    "return empty when busy": the take and the caller's continuation (`then`) are one `Deferred`
//!    variant, and the view's state (the ssh gate key) is detached at once.
//! `PaneQueue::lock()` stays only for callers provably outside any operation.

mod destination;
// #1424 steps 1-2 (facts + the pure table) are not called yet: step 3 (shadow) wires them in, then remove these allows.
#[allow(dead_code)]
pub mod facts;
mod gates;
mod pane;
#[allow(dead_code)]
pub mod table;
pub mod trace;

#[cfg(test)]
mod split_tests;
#[cfg(any(test, feature = "dev-support"))]
pub(crate) mod tests;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::editor::HeldPasteKey;

pub use destination::{
    DestFacts, Destination, DropReason, destination_for, intrinsic_destination, is_pty_entry,
    pin_still_valid,
};
pub use facts::{
    DestinationMode, FocusSnap, GateMask, PaneRouteState, PsPhase, QueueFacts, RouteFacts,
    SshPhase, Stamp, SubshellPhase, destination_mode,
};
pub use gates::{Gate, GateKind, GateSignalPlan, GateSpec, GateToken, SignalPlan, signal_plan};
pub use pane::{
    Deferred, GateKeysThen, PaneQueue, PasteOwner, PowershellKeysThen, SshDropThen,
    is_editor_paste_id,
};

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

/// An editor key (`HeldPasteKey`) travelling through the queue (phase 1). The editor action is
/// not `Clone`, so the key sits in a shared cell and the one delivery takes it out. Equality is
/// identity: two payloads are equal only if they are the same key.
#[derive(Clone)]
pub struct EditorKeyPayload(Rc<RefCell<Option<HeldPasteKey>>>);

impl EditorKeyPayload {
    pub fn new(key: HeldPasteKey) -> Self {
        Self(Rc::new(RefCell::new(Some(key))))
    }

    /// Delivering this key reads the editor's display map (cursor motion, deletion by display
    /// position), so it must wait until the map has caught up with the edits before it (#1389).
    /// False once the key was delivered.
    pub fn needs_fresh_display_map(&self) -> bool {
        self.0
            .borrow()
            .as_ref()
            .is_some_and(HeldPasteKey::needs_fresh_display_map)
    }

    /// Delivering this key reads the soft-wrap LAYOUT of the last rendered frame (visual line
    /// start / end, Up, Down), which only a render after the edits before it brings up to date.
    pub fn reads_soft_wrap_layout(&self) -> bool {
        self.0
            .borrow()
            .as_ref()
            .is_some_and(HeldPasteKey::reads_soft_wrap_layout)
    }

    /// The key, once. `None` if it was already delivered.
    pub fn take(&self) -> Option<HeldPasteKey> {
        self.0.borrow_mut().take()
    }

    fn len(&self) -> usize {
        match self.0.borrow().as_ref() {
            Some(HeldPasteKey::Text(text)) => text.len(),
            Some(HeldPasteKey::Action(_)) | None => 0,
        }
    }
}

impl PartialEq for EditorKeyPayload {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for EditorKeyPayload {}

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
    /// User bytes already encoded for the pty (what `write_user_bytes_to_pty` is handed). Phase 1
    /// carries the terminal's keys in this form because the later stages (subshell, hand-off,
    /// ssh) still consume bytes. `takes_over_agent_control` is false for pointer-driven bytes.
    PtyBytes {
        bytes: Vec<u8>,
        takes_over_agent_control: bool,
    },
    /// A key for the input editor, held behind a paste (phase 1).
    EditorKey(EditorKeyPayload),
    /// Input whose form the host owns (phase 4: the ssh connect typeahead keeps typed text, keys,
    /// pastes and escape sequences as they were produced, `class` says which). The queue only
    /// orders it; the host routes it at delivery from the facts of that moment.
    Held {
        class: u8,
        text: String,
    },
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
            EntryKind::PtyBytes { .. } => "pty_bytes",
            EntryKind::EditorKey(_) => "editor_key",
            EntryKind::Held { .. } => "held",
        }
    }

    /// Byte length of the content, for logs.
    pub fn len(&self) -> usize {
        match self {
            EntryKind::Text(text) => text.len(),
            EntryKind::Control(s) | EntryKind::Seq(s) => s.len(),
            EntryKind::Paste { payload, .. } => payload.as_ref().map_or(0, Vec::len),
            EntryKind::Raw(bytes) => bytes.len(),
            EntryKind::PtyBytes { bytes, .. } => bytes.len(),
            EntryKind::EditorKey(key) => key.len(),
            EntryKind::Held { text, .. } => text.len(),
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
                | EntryKind::PtyBytes { .. }
                | EntryKind::EditorKey(_)
                | EntryKind::Held { .. }
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

/// The `class` of an [`EntryKind::Held`] entry: printable characters, a control key, a paste, a
/// control sequence (arrows, ...).
pub const HELD_TYPED: u8 = 0;
pub const HELD_KEY: u8 = 1;
pub const HELD_PASTE: u8 = 2;
pub const HELD_SEQ: u8 = 3;
/// A signal (^C ^D ^Z ^\) typed during the ssh setup window (#1424, OD-5): held in typed order with
/// the keys, carried as the byte, and delivered to the REMOTE side at the hand-over by the
/// table's `Origin::Held` + signal row. Never delivered locally: when the connect ends without a
/// remote shell the held signal is discarded (logged, count only). Declared here; no producer or
/// executor uses it yet (steps 1 and 2 change no behaviour).
pub const HELD_SIGNAL: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Keyboard,
    /// Offered by the input editor's key handler (the editor route), as opposed to the terminal
    /// element's (`Keyboard`). The content is what was typed; the destination is decided at drain.
    Editor,
    /// Re-offered by a delivery follow-up.
    Replay(EntryId),
    /// Synthesized (mouse report, wheel to arrow).
    Synthesized,
    /// Held during an ssh connect and handed over to the remote shell (#1292).
    Held,
    /// Bytes relayed from a synced pane (`write_to_pty_for_syncing_long_running_commands`).
    Synced,
    /// Bytes a shared-session viewer wrote (`write_viewer_bytes_to_pty`).
    Viewer,
    /// The paced CLI-agent submit's own pieces (`write_cli_submit_bytes`): exempt from the holds.
    CliSubmit,
    /// The agent wrote it (`write_agent_bytes_to_pty`).
    Agent,
    /// Released by a hand-off gate (`LineEditorActive`, `PromptSeen`, ...): it has been through
    /// the hand-off oracle once and must not re-enter it.
    ReleasedFrom(GateKind),
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
    /// What was true when the key reached the pane's sink (#1424): the model gen it saw, the
    /// focus chain, the focus epoch. Created at the sink and carried through the deferred
    /// mailbox; a decision made later still judges the key by it (H4).
    pub stamp: Stamp,
}

/// What delivering one entry caused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliverOutcome {
    Delivered,
    /// The delivery (an Enter that now waits for a verdict or report) closes the queue behind it.
    OpensGate(GateSpec),
    /// The delivery did NOT happen: a later stage (the typeahead hand-off, #1353 phase 5) took
    /// the entry's bytes into a hold of its own. The entry goes back to the front of the queue,
    /// held by the new gate together with everything behind it, and is taken out again by the
    /// gate's event ([`InputQueue::close_gate_taking`]).
    Requeue(GateSpec),
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
    /// The delivery is part of a signal's flush (^C met a gate and released what it held). The
    /// queue sets it; hosts that would otherwise wait (the editor's display-map gate) apply at
    /// once, because the signal must see everything typed before it.
    flushing: bool,
}

#[derive(Debug)]
enum Follow {
    Offer(EntryKind),
    OpenGate(GateSpec),
    GateEvent(GateKind, u64),
}

impl FollowUps {
    /// Follow-ups for a delivery made OUTSIDE a drain that must apply at once (a key taken out of
    /// a gate that is being voided: hosts that would wait, like the editor's display-map gate,
    /// do not).
    pub fn flushing_now() -> Self {
        Self {
            items: Vec::new(),
            flushing: true,
        }
    }

    /// Whether this delivery is part of a signal's flush.
    pub fn flushing(&self) -> bool {
        self.flushing
    }

    /// Queue another entry behind everything already queued (a replayed key).
    pub fn offer(&mut self, kind: EntryKind) {
        self.items.push(Follow::Offer(kind));
    }

    /// Close the queue behind the entry being delivered.
    pub fn open_gate(&mut self, spec: GateSpec) {
        self.items.push(Follow::OpenGate(spec));
    }

    /// A gate-closing event that happened while delivering.
    #[cfg(any(test, feature = "dev-support"))]
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

    /// Whether this host can deliver `entry` to `dest`. A queue shared by the editor and the
    /// terminal view (#1353 step 3) is drained by whichever of them is running; an entry its
    /// driving host cannot deliver is NOT delivered: the drain stops with the entry still at the
    /// head (no gate, nothing behind it overtakes) and asks, through the queue's waker, for the
    /// drain to be continued by a host that can ([`InputQueue::pending_foreign`]). Drops and
    /// signals are always handled. Hosts of a queue with one destination keep the default.
    fn handles(&self, _entry: &InputEntry, _dest: Destination) -> bool {
        true
    }

    /// Whether the drain should hand control back to the event loop after delivering `entry`,
    /// when more is queued behind it: the entry's effects (an Enter that submitted a command)
    /// are processed as events after this call returns, and what follows must be judged with
    /// them applied (#1421). The drain stops as a foreign stop does and the waker continues it.
    fn yield_after(&mut self, _entry: &InputEntry) -> bool {
        false
    }

    /// What goes back to the front of the queue after [`DeliverOutcome::Requeue`]: by default the
    /// entry that was delivered (a key or a paste whose bytes a later stage took into a hold). A
    /// host whose delivery had other effects (it wrote part of the entry, or delivered an entry
    /// of another shape) says what is left: the same entry id, so the gate's barrier holds
    /// everything behind it, with the remaining content.
    fn requeued_entry(&mut self, entry: &InputEntry) -> InputEntry {
        entry.clone()
    }

    /// Whether a signal meeting `gate` must wait like any entry (an interrupt must not land in
    /// the middle of the remote bootstrap, #1001).
    fn signal_must_wait(&self, _gate: GateKind) -> bool {
        false
    }

    /// A give-up cap fired for a gate whose event never came; always logged, at warning level.
    fn warn(&mut self, line: &str);

    /// The queue is full ([`MAX_QUEUED_ENTRIES`]) and refused an arrival (#1442). `first` is true
    /// for the first refusal since the queue was last idle: tell the user then (once), and log
    /// the running `dropped` count sparingly. The default logs the first and every 4096th.
    fn overflowed(&mut self, first: bool, dropped: u64, queued: usize) {
        if first || dropped % 4096 == 0 {
            self.warn(&format!(
                "input queue full ({queued} entries waiting): dropped {dropped} newest arrival(s)"
            ));
        }
    }

    /// Cheap check so the queue only formats trace lines when someone listens.
    fn log_enabled(&self) -> bool {
        false
    }

    /// Trace line (`input_order`, debug). Counts and ids only.
    fn log(&mut self, _line: &str) {}
}

/// The most entries a pane's queue keeps waiting (#1442). Reached only by a gate that stays shut
/// while keys keep arriving: the longest is the PowerShell connect net of 180 s, and 180 s of the
/// fastest key repeat (about 60 Hz, one entry per key) is ~11,000 entries, so the bound is six
/// times that; it equals the ssh hold's 64 KiB (one byte per key at the very least). Past it the
/// NEWEST non-signal arrival is dropped, logged as a warning and reported to the host
/// ([`Host::overflowed`]) so the user is told; a signal (^C) is never dropped.
pub const MAX_QUEUED_ENTRIES: usize = 64 * 1024;

#[derive(Debug)]
pub struct InputQueue {
    /// The cap on `entries` ([`MAX_QUEUED_ENTRIES`]; tests lower it).
    max_entries: usize,
    /// Arrivals dropped for the cap since the queue last went idle.
    overflow_dropped: u64,
    next_id: u64,
    next_token: u64,
    focus_epoch: u64,
    /// The stamp new arrivals get when the caller did not bring one (see [`Self::set_stamp`]).
    current_stamp: Stamp,
    entries: VecDeque<InputEntry>,
    gates: Vec<Gate>,
    /// Destination of the line being typed.
    line_pin: Option<Destination>,
    give_ups: u64,
    /// The last drain stopped at an entry its host does not handle (see [`Host::handles`]); the
    /// entry is still at the head. Cleared by the next drain.
    foreign_stop: bool,
    /// A signal is being applied (its gates resolved, what they held delivered): deliveries made
    /// meanwhile are a flush ([`FollowUps::flushing`]).
    in_signal: bool,
    /// Poked (once, idempotent: a bounded channel of one) when a drain stops at a foreign entry,
    /// so the host that CAN deliver it continues the drain. The view listens; nothing polls.
    foreign_waker: Option<async_channel::Sender<()>>,
}

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl InputQueue {
    pub fn new() -> Self {
        Self {
            max_entries: MAX_QUEUED_ENTRIES,
            overflow_dropped: 0,
            next_id: 1,
            next_token: 1,
            focus_epoch: 0,
            current_stamp: Stamp::UNSTAMPED,
            entries: VecDeque::new(),
            gates: Vec::new(),
            line_pin: None,
            give_ups: 0,
            foreign_stop: false,
            in_signal: false,
            foreign_waker: None,
        }
    }

    // ---- inspection -------------------------------------------------------------------------

    /// Lowers (or raises) the cap on queued entries; for tests of the overflow behaviour.
    #[cfg(any(test, feature = "dev-support"))]
    pub fn set_max_entries(&mut self, max_entries: usize) {
        self.max_entries = max_entries;
    }

    /// Arrivals dropped for the cap since the queue last went idle.
    pub fn overflow_dropped(&self) -> u64 {
        self.overflow_dropped
    }

    #[cfg(any(test, feature = "dev-support"))]
    pub fn queued_len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(any(test, feature = "dev-support"))]
    pub fn queued_ids(&self) -> impl Iterator<Item = EntryId> + '_ {
        self.entries.iter().map(|entry| entry.id)
    }

    pub fn gates(&self) -> &[Gate] {
        &self.gates
    }

    pub fn focus_epoch(&self) -> u64 {
        self.focus_epoch
    }

    /// The stamp [`Self::offer`] and the other arrivals without an explicit stamp give their
    /// entries. [`PaneQueue::try_lock`] sets it from the pane's route state, so every arrival
    /// through a guard is stamped with what the sink noted.
    pub fn set_stamp(&mut self, stamp: Stamp) {
        self.current_stamp = stamp;
    }

    /// Nothing queued and no gate open: the fast path applies.
    pub fn is_idle(&self) -> bool {
        self.entries.is_empty() && self.gates.is_empty()
    }

    /// The last drain stopped at an entry its host does not handle and nobody has continued it
    /// yet. While this is true the head is deliverable but waiting for the other host.
    pub fn pending_foreign(&self) -> bool {
        self.foreign_stop
    }

    /// Where to poke when a drain stops at a foreign entry (the terminal view's receiver).
    pub fn set_foreign_waker(&mut self, waker: Option<async_channel::Sender<()>>) {
        self.foreign_waker = waker;
    }

    /// The id the next arrival will get.
    #[cfg(any(test, feature = "dev-support"))]
    pub fn peek_next_id(&self) -> EntryId {
        EntryId(self.next_id)
    }

    /// Number of caps that fired for a gate that was still open.
    #[cfg(any(test, feature = "dev-support"))]
    pub fn give_up_count(&self) -> u64 {
        self.give_ups
    }

    /// Whether a gate of `kind` with `key` is still open (e.g. "is paste 7 still being read").
    pub fn has_gate(&self, kind: GateKind, key: u64) -> bool {
        self.gates
            .iter()
            .any(|gate| gate.kind == kind && gate.key == key)
    }

    /// How many entries wait behind the gate `(kind, key)`: from its barrier to the next gate's
    /// (those further on are held by that gate). 0 when the gate is not open (additive, #1353
    /// phase 3: the Enter that is released asks whether anything was typed behind it).
    pub fn held_behind_gate(&self, kind: GateKind, key: u64) -> usize {
        let Some(gate) = self
            .gates
            .iter()
            .find(|gate| gate.kind == kind && gate.key == key)
        else {
            return 0;
        };
        let upper = self.next_barrier_after(gate.barrier);
        self.entries
            .iter()
            .filter(|entry| entry.id >= gate.barrier && upper.is_none_or(|upper| entry.id < upper))
            .count()
    }

    fn next_barrier_after(&self, barrier: EntryId) -> Option<EntryId> {
        self.gates
            .iter()
            .map(|gate| gate.barrier)
            .filter(|other| *other > barrier)
            .min()
    }

    /// A key typed now, from any route and for any destination, waits in the queue: some gate is
    /// open, or entries are still queued (a stopped drain), so arrival order is kept. Every gate
    /// holds every key; where a key goes is decided when it drains.
    pub fn holds_keys(&self) -> bool {
        !self.is_idle()
    }

    /// Whether an editing key typed in the input editor may be applied at once (#1423) instead of
    /// waiting in the queue. True only when every open gate is a setup gate that governs the
    /// pty's bytes and never the editor's buffer (`SubshellSetup`, `PowershellSetup`), and
    /// everything queued is pty bytes: no earlier editor-bound entry (a held key, text, Enter or
    /// paste) exists that this key would overtake. A key that submits (an Enter) is not decided
    /// here; the editor keeps those held. `CliSubmit` is deliberately not in the set: the rich
    /// input the submit reads from is closed or cleared when the submit ends, so a key applied
    /// meanwhile would be lost with it. Every other gate (hand-off, Enter verdict, ssh, paste,
    /// display map, ...) holds the editor's keys, as before.
    pub fn editor_keys_pass_gates(&self) -> bool {
        !self.gates.is_empty()
            && self.gates.iter().all(|gate| {
                matches!(
                    gate.kind,
                    GateKind::SubshellSetup | GateKind::PowershellSetup
                )
            })
            && self
                .entries
                .iter()
                .all(|entry| matches!(entry.kind, EntryKind::PtyBytes { .. }))
    }

    /// What the destination table needs to know about the queue itself (#1424): the open gate
    /// kinds, whether anything is queued, and whether everything queued is pty-bound bytes.
    pub fn queue_facts(&self) -> QueueFacts {
        QueueFacts {
            gates: GateMask::of(&self.gates.iter().map(|gate| gate.kind).collect::<Vec<_>>()),
            queued: !self.entries.is_empty(),
            queued_only_pty: self
                .entries
                .iter()
                .all(|entry| matches!(entry.kind, EntryKind::PtyBytes { .. })),
        }
    }

    /// A paste read for the input editor is pending.
    pub fn has_editor_paste_reading(&self) -> bool {
        self.gates
            .iter()
            .any(|gate| gate.kind == GateKind::PasteReading && is_editor_paste_id(gate.key))
    }

    /// A paste read for the pty is pending.
    pub fn has_pty_paste_reading(&self) -> bool {
        self.gates
            .iter()
            .any(|gate| gate.kind == GateKind::PasteReading && !is_editor_paste_id(gate.key))
    }

    /// Whether any gate of `kind` is open.
    pub fn has_gate_kind(&self, kind: GateKind) -> bool {
        self.gates.iter().any(|gate| gate.kind == kind)
    }

    /// Entries and content bytes queued, for trace lines (counts only, never content). A paste
    /// that has not been read yet counts as an entry with no bytes.
    pub fn queued_summary(&self) -> (usize, usize) {
        (
            self.entries.len(),
            self.entries.iter().map(|entry| entry.kind.len()).sum(),
        )
    }

    /// Entries and content bytes queued at or behind the first gate of `kind` (what that gate's
    /// release will let go, give or take entries held again by a later gate), for trace lines.
    pub fn queued_since_gate(&self, kind: GateKind) -> (usize, usize) {
        let Some(barrier) = self
            .gates
            .iter()
            .filter(|gate| gate.kind == kind)
            .map(|gate| gate.barrier)
            .min()
        else {
            return (0, 0);
        };
        self.entries
            .iter()
            .filter(|entry| entry.id >= barrier)
            .fold((0, 0), |(count, bytes), entry| {
                (count + 1, bytes + entry.kind.len())
            })
    }

    /// The queued entries in order (inspection; content is never to be logged).
    pub fn entries(&self) -> impl Iterator<Item = &InputEntry> + '_ {
        self.entries.iter()
    }

    /// Removes the gate `(kind, key)` WITHOUT delivering what it held, and returns those entries
    /// (from its barrier up to the next gate's) in order. For a hold that is cancelled by an event
    /// that also voids its keys (^C flushes the tty's input, a password prompt appeared). Needs no
    /// host: nothing is delivered, and entries behind a later gate keep waiting for it.
    pub fn drop_gate(&mut self, kind: GateKind, key: u64) -> Vec<InputEntry> {
        let Some(index) = self
            .gates
            .iter()
            .position(|gate| gate.kind == kind && gate.key == key)
        else {
            return Vec::new();
        };
        let gate = self.gates.remove(index);
        let upper = self
            .gates
            .iter()
            .map(|g| g.barrier)
            .filter(|barrier| *barrier > gate.barrier)
            .min();
        let mut kept = VecDeque::with_capacity(self.entries.len());
        let mut removed = Vec::new();
        for entry in self.entries.drain(..) {
            if entry.id >= gate.barrier && upper.is_none_or(|upper| entry.id < upper) {
                removed.push(entry);
            } else {
                kept.push_back(entry);
            }
        }
        self.entries = kept;
        removed
    }

    /// Whether the head entry waits behind a gate. Invariant: a non-empty queue always has a
    /// blocked head (otherwise the last drain would have delivered it).
    #[cfg(any(test, feature = "dev-support"))]
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
        let stamp = self.current_stamp;
        self.offer_stamped(host, kind, origin, stamp)
    }

    /// [`Self::offer_with_origin`] with the stamp the key got at the sink (a key that went
    /// through the deferred mailbox keeps the stamp it had when typed).
    pub fn offer_stamped<H: Host>(
        &mut self,
        host: &mut H,
        kind: EntryKind,
        origin: Origin,
        stamp: Stamp,
    ) -> EntryId {
        let entry = self.make_entry(kind, origin, stamp);
        let id = entry.id;
        if matches!(entry.kind, EntryKind::Signal(_)) {
            self.handle_signal(host, entry);
        } else if self.is_idle() {
            // Fast path: nothing held, nothing queued; deliver inline without touching the deque.
            self.deliver_entry(host, &entry);
            self.drain(host);
        } else if self.entries.len() >= self.max_entries {
            // #1442: nothing drains and keys keep coming. Refuse the newest instead of growing
            // without bound; the host logs it and tells the user. The first drop of an episode
            // is `first`; the count restarts when the queue goes idle.
            self.overflow_dropped += 1;
            let first = self.overflow_dropped == 1;
            host.overflowed(first, self.overflow_dropped, self.entries.len());
        } else {
            self.entries.push_back(entry);
            self.drain(host);
        }
        if self.is_idle() {
            self.overflow_dropped = 0;
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
        let stamp = self.current_stamp;
        let entry = self.make_entry(kind, Origin::Keyboard, stamp);
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

    /// Turns the open gate `(kind, key)` into `spec` in place: the same barrier, so everything it
    /// holds stays held with no window in which it is not (a take and a re-offer would have one),
    /// under a new token, so the cap timer armed for the old gate finds nothing. With
    /// `drop_first_byte` the Enter that starts the first pty bytes entry the gate holds is removed
    /// (the Enter of a young hold that now runs in the editor: one byte, or both of a `\r\n`
    /// pair; the rest of a multi-byte write stays held); an entry left empty is removed.
    /// Returns whether the gate was open.
    pub fn retag_gate<H: Host>(
        &mut self,
        host: &mut H,
        kind: GateKind,
        key: u64,
        spec: GateSpec,
        drop_first_byte: bool,
    ) -> bool {
        let Some(index) = self
            .gates
            .iter()
            .position(|gate| gate.kind == kind && gate.key == key)
        else {
            return false;
        };
        let barrier = self.gates.remove(index).barrier;
        if drop_first_byte
            && let Some(position) = self.entries.iter().position(|entry| entry.id >= barrier)
        {
            let emptied = match &mut self.entries[position].kind {
                EntryKind::PtyBytes { bytes, .. } if !bytes.is_empty() => {
                    // One Enter leaves: a `\r\n` pair is a single Enter (#1420), so its `\n`
                    // does not stay behind as a second one. The rest of the write is typed
                    // content behind the Enter and stays held, in order.
                    let enter_len = if bytes.starts_with(b"\r\n") { 2 } else { 1 };
                    bytes.drain(..enter_len);
                    bytes.is_empty()
                }
                _ => false,
            };
            if emptied {
                self.entries.remove(position);
            }
        }
        self.push_gate(host, spec, barrier);
        true
    }

    fn make_entry(&mut self, kind: EntryKind, origin: Origin, stamp: Stamp) -> InputEntry {
        let id = EntryId(self.next_id);
        self.next_id += 1;
        InputEntry {
            id,
            kind,
            focus_epoch: self.focus_epoch,
            arrived_at: Instant::now(),
            origin,
            stamp,
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

    /// Closes the gate `(kind, key)` and hands back, without delivering them, the entries that
    /// waited behind it (in order); whatever else the gate was holding back is then drained as
    /// usual. For a gate whose owner voids it and wants what was typed behind it back (additive,
    /// #1353 phase 3). Empty when the gate is not open.
    pub fn close_gate_taking<H: Host>(
        &mut self,
        host: &mut H,
        kind: GateKind,
        key: u64,
    ) -> Vec<InputEntry> {
        let Some(gate) = self
            .gates
            .iter()
            .copied()
            .find(|gate| gate.kind == kind && gate.key == key)
        else {
            return Vec::new();
        };
        let upper = self.next_barrier_after(gate.barrier);
        let mut taken = Vec::new();
        let mut kept = VecDeque::with_capacity(self.entries.len());
        for entry in self.entries.drain(..) {
            if entry.id >= gate.barrier && upper.is_none_or(|upper| entry.id < upper) {
                taken.push(entry);
            } else {
                kept.push_back(entry);
            }
        }
        self.entries = kept;
        self.close_gates(host, kind, key, "taken");
        self.drain(host);
        taken
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
        self.foreign_stop = false;
        while let Some(head) = self.entries.front() {
            if self.is_blocked(head.id) {
                break;
            }
            if !self.head_is_handled(host, head) {
                // The entry stays where it is: everything behind it waits for it (FIFO), and the
                // host that can deliver it is asked to continue.
                self.foreign_stop = true;
                if host.log_enabled() {
                    host.log(&format!(
                        "drain_foreign_stop id={:?} kind={}",
                        head.id,
                        head.kind.tag()
                    ));
                }
                if let Some(waker) = &self.foreign_waker {
                    // Bounded to one: a wake already pending is as good as another.
                    let _ = waker.try_send(());
                }
                break;
            }
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.deliver_entry(host, &entry);
            if !self.entries.is_empty() && host.yield_after(&entry) {
                self.foreign_stop = true;
                if let Some(waker) = &self.foreign_waker {
                    let _ = waker.try_send(());
                }
                break;
            }
        }
    }

    /// Whether the driving host can deliver the (unblocked) head. An unready paste is discarded by
    /// whoever drains it, and a drop needs no destination.
    fn head_is_handled<H: Host>(&self, host: &H, head: &InputEntry) -> bool {
        if matches!(head.kind, EntryKind::Paste { payload: None, .. }) {
            return true;
        }
        let dest = self.route(host, head);
        matches!(dest, Destination::Drop(_)) || host.handles(head, dest)
    }

    /// The destination of `entry` from the facts of this moment, honouring the line-run pin.
    fn route<H: Host>(&self, host: &H, entry: &InputEntry) -> Destination {
        // Pty bytes and editor keys know where they go; the pin is for text whose form the
        // destination decides (a shared pane queue carries both kinds).
        if let Some(dest) = intrinsic_destination(&entry.kind) {
            return dest;
        }
        let facts = host.facts();
        let computed = destination_for(&entry.kind, &facts);
        match (computed, self.line_pin) {
            (Destination::Drop(_), _) => computed,
            (_, Some(pin)) if entry.kind.is_line_content() || entry.kind == EntryKind::Enter => {
                if pin_still_valid(pin, &facts) {
                    pin
                } else {
                    computed
                }
            }
            _ => computed,
        }
    }

    /// Decide the destination NOW and hand the entry to the host.
    fn deliver_entry<H: Host>(&mut self, host: &mut H, entry: &InputEntry) {
        if let EntryKind::Paste { payload: None, .. } = &entry.kind {
            host.discarded(entry, DiscardReason::PasteNotReady);
            return;
        }
        let dest = self.route(host, entry);
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
        let mut follow = FollowUps {
            flushing: self.in_signal,
            ..FollowUps::default()
        };
        let outcome = host.deliver(entry, dest, &mut follow);
        match outcome {
            DeliverOutcome::Delivered => {}
            DeliverOutcome::OpensGate(spec) => {
                // Behind the delivered entry: whatever already queued after it waits too.
                self.push_gate(host, spec, EntryId(entry.id.0 + 1));
            }
            DeliverOutcome::Requeue(spec) => {
                // Not delivered: back to the front, held by the new gate along with the rest.
                self.entries.push_front(host.requeued_entry(entry));
                self.push_gate(host, spec, entry.id);
            }
        }
        self.apply_follow_ups(host, entry.id, entry.stamp, follow);
    }

    fn apply_follow_ups<H: Host>(
        &mut self,
        host: &mut H,
        from: EntryId,
        stamp: Stamp,
        follow: FollowUps,
    ) {
        for item in follow.items {
            match item {
                Follow::Offer(kind) => {
                    // A follow-up takes the stamp of the entry it follows (H4).
                    let entry = self.make_entry(kind, Origin::Replay(from), stamp);
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
        self.in_signal = true;
        self.apply_signal(host, entry);
        self.in_signal = false;
    }

    fn apply_signal<H: Host>(&mut self, host: &mut H, entry: InputEntry) {
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

#[cfg(test)]
mod paste_tests;
