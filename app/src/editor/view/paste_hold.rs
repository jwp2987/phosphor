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
//! read (no lock, no allocation). `PHOSPHOR_INPUT_QUEUE_PASTE=0` restores the sequencer path
//! below, which stays in the tree. The queue lives here (one per editor) because the editor-bound
//! read is started by the terminal input, which has no handle on the terminal view's queue.
//!
//! The queue can hold a password typed blind, so `Debug` shows only kinds.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use warpui::ViewContext;

use super::{EditorAction, EditorView};
use crate::terminal::input_queue::kill_switch::{self, Area};
use crate::terminal::input_queue::{
    DeliverOutcome, DestFacts, Destination, DiscardReason, EntryKind, FollowUps, GateKind,
    GateToken, Host, InputEntry, PaneQueue, PasteOwner,
};
use crate::terminal::paste_sequencer::PasteSequencer;

/// One key held behind a pending paste.
pub enum HeldPasteKey {
    /// Printable characters, applied as typed text.
    Text(String),
    /// An editing or cursor key, replayed through the editor's normal action handler.
    Action(EditorAction),
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
    /// Paste ordering runs on `queue` (`PHOSPHOR_INPUT_QUEUE_PASTE`, default on).
    queue_enabled: Cell<bool>,
    /// Replayed keys that wait for the display map to catch up with the edits before them (#1389).
    replay: RefCell<VecDeque<HeldPasteKey>>,
    /// A `ReplayHeldKeys` step is already queued as an effect.
    replay_scheduled: Cell<bool>,
    /// A replayed key is being applied right now (it must not be queued again).
    replaying: Cell<bool>,
}

impl Default for EditorPasteHold {
    fn default() -> Self {
        Self {
            sequencer: RefCell::default(),
            pane_queue: RefCell::new(PaneQueue::new()),
            queue_enabled: Cell::new(kill_switch::is_enabled(Area::Paste)),
            replay: RefCell::default(),
            replay_scheduled: Cell::new(false),
            replaying: Cell::new(false),
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

    /// Whether paste ordering runs on the input queue instead of the sequencer.
    pub fn uses_queue(&self) -> bool {
        self.queue_enabled.get()
    }

    /// Tests flip the path per hold (the kill switch itself is read once per process).
    #[cfg(test)]
    pub fn set_uses_queue(&self, on: bool) {
        self.queue_enabled.set(on);
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

    /// The queue holds input right now (a paste is being read). False while it is mid-drain: a
    /// key applied by a delivery is part of that delivery and must not be held again.
    pub fn queue_is_busy(&self) -> bool {
        self.pane_queue
            .borrow()
            .try_peek()
            .is_some_and(|queue| !queue.is_idle())
    }

    /// Starts a paste; keys are held from now until it completes or is cancelled. A paste already
    /// pending is superseded and its held keys carry over.
    pub fn begin(&self) -> u64 {
        self.sequencer.borrow_mut().begin()
    }

    pub fn has_pending(&self) -> bool {
        if self.queue_enabled.get() {
            return self
                .pane_queue
                .borrow()
                .try_peek()
                .is_some_and(|queue| queue.has_gate_kind(GateKind::PasteReading));
        }
        self.sequencer.borrow().has_pending()
    }

    /// A paste begun with [`Self::begin`] is pending, whichever path the input uses. The input's
    /// own pastes are on the queue; a plain editor's paste (search box, settings field) reads
    /// off the UI thread on the sequencer (#1380) and holds the keys typed behind it here.
    pub fn sequencer_pending(&self) -> bool {
        self.sequencer.borrow().has_pending()
    }

    pub fn is_pending(&self, id: u64) -> bool {
        if self.queue_enabled.get() {
            return self
                .pane_queue
                .borrow()
                .try_peek()
                .is_some_and(|queue| queue.has_gate(GateKind::PasteReading, id));
        }
        self.sequencer.borrow().is_pending(id)
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

    fn deliver(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        _follow: &mut FollowUps,
    ) -> DeliverOutcome {
        match &entry.kind {
            EntryKind::EditorKey(payload) => {
                if let Some(key) = payload.take() {
                    self.editor.apply_held_paste_key(key, self.ctx);
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
            DiscardReason::SignalDiscard(GateKind::EnterVerdict) => {}
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
