//! Keys typed into the editor while a paste is still being read from the clipboard (#1300).
//!
//! The input editor's paste reads the clipboard off the UI thread, so the paste lands a moment
//! after the key that asked for it. Keys typed in that gap must apply AFTER the pasted text, in the
//! order typed (and an Enter must submit the whole line), so the editor queues them here and the
//! terminal input replays them right behind the paste. The ordering is the terminal's
//! [`PasteSequencer`], shared with the pty-bound paste path.
//!
//! The queue can hold a password typed blind, so `Debug` shows only kinds.

use std::cell::RefCell;
use std::fmt;

use super::EditorAction;
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
#[derive(Debug, Default)]
pub struct EditorPasteHold {
    sequencer: RefCell<PasteSequencer<HeldPasteKey>>,
}

impl EditorPasteHold {
    /// Starts a paste; keys are held from now until it completes or is cancelled. A paste already
    /// pending is superseded and its held keys carry over.
    pub fn begin(&self) -> u64 {
        self.sequencer.borrow_mut().begin()
    }

    pub fn has_pending(&self) -> bool {
        self.sequencer.borrow().has_pending()
    }

    pub fn is_pending(&self, id: u64) -> bool {
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
