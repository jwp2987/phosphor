//! Keys typed into the editor while a paste is still being read from the clipboard (#1300).
//!
//! The input editor's paste reads the clipboard off the UI thread, so the paste lands a moment
//! after the key that asked for it. Keys typed in that gap must apply AFTER the pasted text, in the
//! order typed (and an Enter must submit the whole line), so the editor queues them here and the
//! terminal input replays them right behind the paste. The ordering is the terminal's
//! [`PasteSequencer`], shared with the pty-bound paste path.
//!
//! #1353 phase 1: the same ordering now runs on an [`InputQueue`] (the paste is an entry held by a
//! `PasteReading` gate; the keys typed behind it are `EditorKey` entries; the reader's result
//! opens the gate; ^C cancels the paste and delivers what was typed before it). The editor's keys
//! reach `InputQueue::offer` whenever the queue is not idle; idle, the cost is one borrow-flag
//! read (no lock, no allocation). The sequencer below remains for PLAIN editors (search boxes,
//! notebooks), which have no pty and no pane queue (#1380). The queue lives here (one per editor)
//! because the editor-bound read is started by the terminal input, which has no handle on the
//! terminal view's queue; a terminal input adopts the pane's one queue ([`Self::adopt_queue`]).
//!
//! The queue can hold a password typed blind, so `Debug` shows only kinds.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use warpui::ViewContext;

use super::{EditorAction, EditorView};
use crate::terminal::input_queue::{
    DeliverOutcome, DestFacts, Destination, DiscardReason, EntryKind, FollowUps, GateKind,
    GateToken, Host, InputEntry, PaneQueue, PasteOwner, is_pty_entry,
};
use crate::terminal::paste_sequencer::PasteSequencer;

/// One key held behind a pending paste.
pub enum HeldPasteKey {
    /// Printable characters, applied as typed text.
    Text(String),
    /// An editing or cursor key, replayed through the editor's normal action handler.
    Action(EditorAction),
}

impl HeldPasteKey {
    /// Applying this key reads the display map (cursor motion, deletion by display position), so
    /// it must wait until the edits applied before it have reached the map (#1389). Typed text and
    /// the Enter family read the buffer, not the map.
    pub fn needs_fresh_display_map(&self) -> bool {
        needs_fresh_display_map(self)
    }
}

impl HeldPasteKey {
    /// Applying this key reads the soft-wrap layout the last rendered frame stored (visual line
    /// start / end, Up, Down and their selections). That layout describes the buffer as of that
    /// frame, so after an edit it is stale until a newer frame is laid out; the display map alone
    /// catching up is not enough (a large paste, #1389).
    pub fn reads_soft_wrap_layout(&self) -> bool {
        matches!(
            self,
            Self::Action(
                EditorAction::Up
                    | EditorAction::Down
                    | EditorAction::SelectUp
                    | EditorAction::SelectDown
                    | EditorAction::MoveToVisualLineStart
                    | EditorAction::MoveToVisualLineEnd
            )
        )
    }
}

impl fmt::Debug for HeldPasteKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => write!(f, "Text(<{} bytes>)", text.len()),
            Self::Action(_) => write!(f, "Action"),
        }
    }
}

/// Owned by the editor; the terminal input starts and completes pastes on it.
#[derive(Debug)]
pub struct EditorPasteHold {
    sequencer: RefCell<PasteSequencer<HeldPasteKey>>,
    /// The pane's queue handle. A plain editor keeps its own; the terminal input swaps in the
    /// terminal view's so the pane has ONE ([`Self::adopt_queue`]).
    pane_queue: RefCell<PaneQueue>,
    /// Replayed keys that wait for the display map to catch up with the edits before them (#1389).
    replay: RefCell<VecDeque<HeldPasteKey>>,
    /// A `ReplayHeldKeys` step is already queued as an effect.
    replay_scheduled: Cell<bool>,
    /// A replayed key is being applied right now (it must not be queued again).
    replaying: Cell<bool>,
    /// #1353 merge step 4: on the pane's shared queue a key that needs a fresh display map waits
    /// behind an `EditorDisplayMap` gate instead of going to `replay`. Off (the default) keeps
    /// the replay list, the behaviour of two instances and of the sequencer path.
    display_gate_enabled: Cell<bool>,
    /// The key of the display-map gate that is open, if one is.
    display_gate: Cell<Option<u64>>,
    /// The layout generation the open display-map gate waits to be passed: its key reads the
    /// soft-wrap layout, which must be a layout newer than the edits before the key.
    layout_wait: Cell<Option<u64>>,
    next_display_gate: Cell<u64>,
    /// The step that closes the gate is running: the display map is current for the FIRST key it
    /// delivers (consumed by that key).
    map_fresh: Cell<bool>,
    /// Pokes the owner of the replay step (the terminal input) after this update's effects flush.
    display_step_waker: RefCell<Option<async_channel::Sender<()>>>,
}

/// What delivering a held editor key does about the display map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMapStep {
    /// Apply the key now.
    Apply,
    /// Put the key back under this gate; a replay step closes it once the map is current.
    Wait(crate::terminal::input_queue::GateSpec),
}

impl Default for EditorPasteHold {
    fn default() -> Self {
        Self {
            sequencer: RefCell::default(),
            pane_queue: RefCell::new(PaneQueue::new()),
            replay: RefCell::default(),
            replay_scheduled: Cell::new(false),
            replaying: Cell::new(false),
            display_gate_enabled: Cell::new(false),
            display_gate: Cell::new(None),
            layout_wait: Cell::new(None),
            next_display_gate: Cell::new(0),
            map_fresh: Cell::new(false),
            display_step_waker: RefCell::new(None),
        }
    }
}

impl EditorPasteHold {
    /// Replayed keys are waiting for the display map (see [`needs_fresh_display_map`]).
    pub fn replay_pending(&self) -> bool {
        !self.replay.borrow().is_empty()
    }

    pub fn replay_push(&self, key: HeldPasteKey) {
        self.replay.borrow_mut().push_back(key);
    }

    pub fn replay_pop(&self) -> Option<HeldPasteKey> {
        self.replay.borrow_mut().pop_front()
    }

    /// Whether the next waiting key is one that can be applied without a fresh display map.
    pub fn replay_next_needs_fresh_map(&self) -> bool {
        self.replay
            .borrow()
            .front()
            .is_some_and(needs_fresh_display_map)
    }

    /// Marks a `ReplayHeldKeys` step as queued; false if one already was.
    pub fn replay_mark_scheduled(&self) -> bool {
        !self.replay_scheduled.replace(true)
    }

    pub fn replay_clear_scheduled(&self) {
        self.replay_scheduled.set(false);
    }

    pub fn is_replaying(&self) -> bool {
        self.replaying.get()
    }

    /// Sets the "applying a replayed key" flag, returning the previous value.
    pub fn set_replaying(&self, on: bool) -> bool {
        self.replaying.replace(on)
    }

    /// The pane's queue (a cheap handle; clones share the queue).
    pub fn queue(&self) -> PaneQueue {
        self.pane_queue.borrow().clone()
    }

    /// The id of the next editor-bound queued paste. The counter belongs to the pane's queue, so
    /// it is shared with the terminal view's pastes once the queue is.
    pub fn next_queue_paste_id(&self) -> u64 {
        self.pane_queue.borrow().next_paste_id(PasteOwner::Editor)
    }

    /// Replaces this editor's private queue with the pane's shared one (#1353 step 3): the terminal
    /// view's queue, so editor keys, pty keys and the gates between them are ordered by ONE
    /// queue. Refused (false) unless the private queue is idle and not mid-drain; nothing held is
    /// ever moved or dropped.
    pub fn adopt_queue(&self, queue: PaneQueue) -> bool {
        let idle = self
            .pane_queue
            .borrow()
            .try_peek()
            .is_some_and(|current| current.is_idle());
        if !idle {
            log::warn!("input queue: the editor's queue was busy; it keeps its own");
            return false;
        }
        *self.pane_queue.borrow_mut() = queue;
        true
    }

    /// Turns the display-map gate on (the queue is shared with the terminal view and `waker`
    /// reaches the input that runs the replay step). A plain editor keeps the replay list.
    pub fn enable_display_gate(&self, waker: async_channel::Sender<()>) {
        *self.display_step_waker.borrow_mut() = Some(waker);
        self.display_gate_enabled.set(true);
    }

    /// Held editor keys wait for the display map behind a queue gate, not in the replay list.
    pub fn display_gate_enabled(&self) -> bool {
        self.display_gate_enabled.get()
    }

    /// What delivering `payload` does about the display map. A key that reads the map waits behind
    /// a gate unless the replay step that follows the gate's creation is delivering it
    /// ([`Self::set_map_fresh`]) or a signal is flushing the queue (the signal must see everything
    /// typed before it, as `flush_held_replay_now` did). Any applied key may leave the map stale
    /// for the next one.
    pub fn display_map_step(
        &self,
        payload: &crate::terminal::input_queue::EditorKeyPayload,
        flushing: bool,
    ) -> DisplayMapStep {
        if !self.display_gate_enabled.get() {
            return DisplayMapStep::Apply;
        }
        if payload.needs_fresh_display_map() && !flushing && !self.map_fresh.get() {
            let key = self.next_display_gate.get() + 1;
            self.next_display_gate.set(key);
            self.display_gate.set(Some(key));
            return DisplayMapStep::Wait(crate::terminal::input_queue::GateSpec::new(
                GateKind::EditorDisplayMap,
                key,
            ));
        }
        self.map_fresh.set(false);
        DisplayMapStep::Apply
    }

    /// Asks the owner of the replay step to run it once this update's effects have flushed (the
    /// display map follows buffer edits only then). A closed channel means the input is gone and
    /// nothing is left to replay into.
    pub fn request_display_step(&self) {
        if let Some(waker) = self.display_step_waker.borrow().as_ref() {
            // Bounded to one: a step already requested is as good as another.
            let _ = waker.try_send(());
        }
    }

    /// Has the soft-wrap layout `soft` send the display-step waker each time a layout is stored,
    /// so a replay step that waits for a newer layout runs again the moment one lands.
    pub fn arm_layout_wake(&self, soft: &crate::editor::soft_wrap::SoftWrapState) {
        if let Some(waker) = self.display_step_waker.borrow().as_ref() {
            soft.set_layout_waker(waker.clone());
        }
    }

    /// The open gate waits for a layout newer than `generation` (see [`Self::layout_wait`]).
    pub fn set_layout_wait(&self, generation: Option<u64>) {
        self.layout_wait.set(generation);
    }

    /// The layout generation the open gate waits to pass, if its key reads the layout.
    pub fn layout_wait(&self) -> Option<u64> {
        self.layout_wait.get()
    }

    /// Puts a gate back that the replay step could not close yet (it waits for a layout).
    pub fn restore_display_gate(&self, key: u64) {
        self.display_gate.set(Some(key));
    }

    /// The open display-map gate, once: the replay step takes it to close it.
    pub fn take_display_gate(&self) -> Option<u64> {
        self.display_gate.take()
    }

    /// The replay step marks the display map current for the first key it delivers.
    pub fn set_map_fresh(&self, fresh: bool) {
        self.map_fresh.set(fresh);
    }

    /// The queue holds input that keys typed into the editor must wait behind: any open gate (a
    /// paste being read, an Enter waiting for its verdict, an ssh connect, a subshell setup, ...),
    /// or entries still queued. Every gate holds every key. False while the queue is mid-drain: a
    /// key applied by a delivery is part of that delivery and must not be held again.
    pub fn queue_is_busy(&self) -> bool {
        let queue = self.pane_queue.borrow();
        // Operations waiting in the queue's mailbox come first: a key typed now is queued behind
        // them (deferred like them), not applied ahead of them.
        let busy = queue
            .try_peek()
            .is_some_and(|held| held.holds_keys() || queue.has_deferred());
        busy
    }

    /// Starts a paste; keys are held from now until it completes or is cancelled. A paste already
    /// pending is superseded and its held keys carry over.
    pub fn begin(&self) -> u64 {
        self.sequencer.borrow_mut().begin()
    }

    pub fn has_pending(&self) -> bool {
        self.pane_queue
            .borrow()
            .try_peek()
            .is_some_and(|queue| queue.has_editor_paste_reading())
    }

    /// A paste begun with [`Self::begin`] is pending, whichever path the input uses. The input's
    /// own pastes are on the queue; a plain editor's paste (search box, settings field) reads
    /// off the UI thread on the sequencer (#1380) and holds the keys typed behind it here.
    pub fn sequencer_pending(&self) -> bool {
        self.sequencer.borrow().has_pending()
    }

    pub fn is_pending(&self, id: u64) -> bool {
        self.pane_queue
            .borrow()
            .try_peek()
            .is_some_and(|queue| queue.has_gate(GateKind::PasteReading, id))
    }

    /// Holds `key` behind the pending paste; false when none is pending.
    pub fn hold(&self, key: HeldPasteKey) -> bool {
        self.sequencer.borrow_mut().hold(key)
    }

    /// Drops the pending paste and returns the keys held so far, in typing order.
    pub fn cancel(&self) -> Vec<HeldPasteKey> {
        self.sequencer.borrow_mut().cancel()
    }

    /// The paste `id` was read: the keys to replay behind it, or `None` if it was cancelled or
    /// superseded (then nothing is pasted).
    pub fn complete(&self, id: u64) -> Option<Vec<HeldPasteKey>> {
        self.sequencer.borrow_mut().complete(id)
    }
}

/// The [`Host`] of the editor's own offers (a key typed while a paste is read, or ^C): delivers
/// held keys back to the editor. It never receives a paste (those complete in the terminal input,
/// whose host delivers them) and never opens a gate.
pub(super) struct EditorQueueHost<'a, 'b> {
    pub(super) editor: &'a mut EditorView,
    pub(super) ctx: &'a mut ViewContext<'b, EditorView>,
}

impl Host for EditorQueueHost<'_, '_> {
    fn facts(&self) -> DestFacts {
        DestFacts {
            editor_focused: true,
            input_box_visible: true,
            ..DestFacts::default()
        }
    }

    /// The editor reaches no pty: on a queue shared with the terminal view, a drain driven from
    /// here stops at the first pty entry and the view continues it (#1353 step 3).
    fn handles(&self, entry: &InputEntry, _dest: Destination) -> bool {
        !is_pty_entry(&entry.kind)
    }

    fn deliver(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        _follow: &mut FollowUps,
    ) -> DeliverOutcome {
        match &entry.kind {
            EntryKind::EditorKey(payload) => {
                // Reached when a signal flushes the queue (the display map is not waited for) or
                // on a queue the display gate is off for.
                if let Some(key) = payload.take() {
                    self.editor.apply_queued_key(key, self.ctx);
                }
            }
            // The ^C itself acts through the editor's normal handler once the held keys are in.
            EntryKind::Signal(_) => {}
            // Typed behind an Enter that waited (#1353 phase 3): normally the terminal input
            // releases these (its host routes them). Reaching this host means nothing else can,
            // so type them into the editor rather than lose them.
            EntryKind::Text(text) => self
                .editor
                .apply_held_paste_key(HeldPasteKey::Text(text.clone()), self.ctx),
            EntryKind::Enter => self
                .editor
                .apply_held_paste_key(HeldPasteKey::Action(EditorAction::Enter), self.ctx),
            other => {
                log::warn!("input queue: editor was handed an entry it does not deliver: {other:?}")
            }
        }
        DeliverOutcome::Delivered
    }

    fn discarded(&mut self, _entry: &InputEntry, why: DiscardReason) {
        match why {
            DiscardReason::PasteCancelled => {}
            // ^C voided the Enter that waited: what was typed behind it goes with it, as the
            // buffer did before.
            DiscardReason::SignalDiscard(GateKind::EnterVerdict | GateKind::TabCompletion) => {}
            other => log::warn!("input queue: editor entry discarded: {other:?}"),
        }
    }

    fn arm_give_up(&mut self, token: GateToken, _kind: GateKind, after: Duration) {
        // The editor never opens a gate; the terminal input's host arms the paste's cap.
        let _ = (token, after);
    }

    fn warn(&mut self, line: &str) {
        log::warn!("{line}");
    }
}

/// Whether replaying `key` reads the display map (cursor motion, deletion by display position), so
/// it must wait until the edits replayed before it have reached the map (#1389). The map follows
/// buffer edits only when the update's effects flush, which cannot happen inside one replay.
/// Typed text and the Enter family read the buffer, not the map, and apply at once.
pub(super) fn needs_fresh_display_map(key: &HeldPasteKey) -> bool {
    match key {
        HeldPasteKey::Text(_) => false,
        HeldPasteKey::Action(action) => !matches!(
            action,
            EditorAction::Enter
                | EditorAction::Newline
                | EditorAction::CtrlEnter
                | EditorAction::ShiftEnter
                | EditorAction::AltEnter
                | EditorAction::CmdEnter
        ),
    }
}

/// Whether `action` submits the line or asks the shell to complete it: a key whose effect leaves
/// the editor, which a pty setup gate must keep holding (#1423).
pub(super) fn key_submits_or_completes(action: &EditorAction) -> bool {
    matches!(
        action,
        EditorAction::Enter
            | EditorAction::CmdEnter
            | EditorAction::CtrlEnter
            | EditorAction::ShiftEnter
            | EditorAction::AltEnter
            | EditorAction::Newline
            | EditorAction::Tab
            | EditorAction::ShiftTab
    )
}

/// The held form of `action`, if it is a key that must wait behind a pending paste. Everything
/// else (Escape, Ctrl-C, scrolling, mouse selection, copy) acts at once.
pub(super) fn held_form(action: &EditorAction) -> Option<HeldPasteKey> {
    macro_rules! replayable {
        ($($variant:ident),* $(,)?) => {
            match action {
                EditorAction::UserInsert(text) => Some(HeldPasteKey::Text(String::clone(text))),
                $(EditorAction::$variant => Some(HeldPasteKey::Action(EditorAction::$variant)),)*
                _ => None,
            }
        };
    }
    replayable!(
        Tab,
        ShiftTab,
        Cut,
        Paste,
        MiddleClickPaste,
        Yank,
        Newline,
        CtrlEnter,
        ShiftEnter,
        AltEnter,
        Enter,
        CmdEnter,
        CutWordLeft,
        CutWordRight,
        DeleteWordLeft,
        DeleteWordRight,
        CutAllLeft,
        CutAllRight,
        DeleteAllLeft,
        DeleteAllRight,
        Delete,
        Backspace,
        Up,
        Down,
        Left,
        Right,
        MoveToVisualLineStart,
        MoveToVisualLineEnd,
        MoveToLineStart,
        MoveToLineEnd,
        MoveToBufferStart,
        MoveToBufferEnd,
        MoveForwardOneWord,
        MoveBackwardOneWord,
        MoveForwardOneSubword,
        MoveBackwardOneSubword,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Undo,
        Redo,
    )
}

#[cfg(test)]
#[path = "paste_hold_tests.rs"]
mod tests;
