//! The pane key sink (#1404): one entry point per pane for every key event.
//!
//! Before this, a key reached a terminal pane as several copies of one event (the block list
//! element, the alt-screen element, the focused editor's element), each deciding for itself
//! whether it "claims" the key, with guards (an identity seq, rescue rules) to stop two of them
//! delivering it. A view that is a *pane key sink* instead receives the event exactly once, from
//! the window's key dispatch, after the keybinding system has had its chance at a chord and before
//! any element sees the event, and decides the destination itself. Elements keep rendering,
//! hit-testing, mouse and selection.
//!
//! The sink is found by walking the focus chain from the focused view up: the nearest ancestor
//! whose [`crate::View::is_pane_key_sink`] is true. A key with no sink in the chain (a modal, the
//! command palette, another window) takes the old per-view dispatch unchanged. A sink answers
//! [`PaneKeyOutcome::NotMine`] for what it does not own (a text field inside the pane, say), and
//! the event then continues to the elements as before.

use std::ops::Range;

use crate::EntityId;
use crate::keymap::Keystroke;

/// What a key event is.
#[derive(Clone, Debug)]
pub enum PaneKeyKind {
    /// A key press. `will_type` says the platform will follow it with a [`PaneKeyKind::Typed`]
    /// event for the same physical key when nothing consumes this one (a printable key without
    /// cmd): the sink acts on the typed event, so each physical key is decided once. A key press
    /// that is all control characters is the key itself (Enter, Backspace, Ctrl-C) and the sink
    /// acts on it here.
    KeyDown {
        keystroke: Keystroke,
        chars: String,
        is_composing: bool,
        will_type: bool,
    },
    /// Printable text. `ime_commit` marks the commit of an IME composition (it follows the
    /// [`PaneKeyKind::ClearMarkedText`] that ends the composition), as opposed to a plain key.
    Typed { chars: String, ime_commit: bool },
    /// The IME's composition text changed.
    SetMarkedText {
        marked_text: String,
        selected_range: Range<usize>,
    },
    /// The IME's composition ended (before its commit, when it commits).
    ClearMarkedText,
}

/// One key event offered to a sink.
#[derive(Clone, Debug)]
pub struct PaneKey {
    /// The number of the physical key ([`crate::input_order::current_key_seq`]); a keydown and the
    /// typed event it causes carry the same number. For logs: nothing compares it.
    pub seq: u64,
    pub kind: PaneKeyKind,
}

/// The sink's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneKeyOutcome {
    /// The sink delivered the key; no element sees the event.
    Consumed,
    /// Not the sink's: the event is dispatched to the elements as before.
    NotMine,
}

/// The live focus at the moment the key arrives (no render snapshot, #934).
#[derive(Clone, Debug, Default)]
pub struct PaneKeyContext {
    /// Root first, the focused view last.
    pub focus_chain: Vec<EntityId>,
    /// The focused view takes typed text itself ([`crate::View::takes_typed_text`]): an editor.
    pub leaf_takes_typed_text: bool,
}

impl PaneKeyContext {
    /// The focused view.
    pub fn leaf(&self) -> Option<EntityId> {
        self.focus_chain.last().copied()
    }

    /// Whether `view_id` is the focused view.
    pub fn is_focused_exactly(&self, view_id: EntityId) -> bool {
        self.leaf() == Some(view_id)
    }

    /// Whether `view_id` is the focused view or one of its ancestors.
    pub fn in_focus_chain(&self, view_id: EntityId) -> bool {
        self.focus_chain.contains(&view_id)
    }
}

use std::cell::Cell;

thread_local! {
    /// The previous key event was the end of an IME composition: a typed event right after it is
    /// the composition's commit. Key events are dispatched on the main thread, in order.
    static AFTER_IME_CLEAR: Cell<bool> = const { Cell::new(false) };
}

/// Records that an event of kind `kind` went by and returns whether a typed event now is an IME
/// commit. Called once per key event, in order.
pub(crate) fn note_event_and_is_ime_commit(clear_marked_text: bool, typed: bool) -> bool {
    if clear_marked_text {
        AFTER_IME_CLEAR.with(|flag| flag.set(true));
        false
    } else if typed {
        AFTER_IME_CLEAR.with(|flag| flag.replace(false))
    } else {
        AFTER_IME_CLEAR.with(|flag| flag.set(false));
        false
    }
}

#[cfg(test)]
#[path = "pane_key_tests.rs"]
mod tests;
