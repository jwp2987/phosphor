//! Keys the editor hands to the terminal view instead of acting on (#1292).
//!
//! While the terminal view holds the keys typed during an ssh connect, the remote shell's first
//! prompt moves focus to the input editor a moment BEFORE the held keys are delivered. Keys typed
//! in that gap must not be applied to the editor: the delivery would then land the older keys
//! after the newer ones. So while the hold is active every editing key that reaches the editor is
//! queued here, in arrival order, and the view replays the queue behind what it already holds.
//!
//! The queue can contain a password typed blind, so its `Debug` output shows only kinds and
//! counts.

use std::cell::{Cell, RefCell};
use std::fmt;

/// Most bytes queued; past this the queue is dropped (a truncated command line must never run)
/// and the view treats the whole hold as overflowed.
const MAX_QUEUED_BYTES: usize = 64 * 1024;

/// One key the editor intercepted, in the form the terminal view holds it.
#[derive(Clone, PartialEq, Eq)]
pub enum HeldEditorKey {
    /// Printable characters.
    Text(String),
    /// A control key as the pty would see it (Enter, Backspace, Ctrl-U, Ctrl-W).
    Control(&'static str),
    /// A cursor-movement control sequence.
    Sequence(&'static str),
}

impl fmt::Debug for HeldEditorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(s) => write!(f, "Text(<{} bytes>)", s.len()),
            Self::Control(_) => write!(f, "Control"),
            Self::Sequence(_) => write!(f, "Sequence"),
        }
    }
}

impl HeldEditorKey {
    fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Control(s) | Self::Sequence(s) => s.len(),
        }
    }
}

/// What [`EditorKeyHold::take`] returns.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TakenKeys {
    pub keys: Vec<HeldEditorKey>,
    /// More than the bound was queued: `keys` is empty and the whole hold must be dropped.
    pub overflowed: bool,
}

/// Shared between the terminal view (which switches it on and drains it) and the input's editor
/// (which queues into it while it is on).
#[derive(Default)]
pub struct EditorKeyHold {
    active: Cell<bool>,
    queue: RefCell<Vec<HeldEditorKey>>,
    bytes: Cell<usize>,
    overflowed: Cell<bool>,
}

impl fmt::Debug for EditorKeyHold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorKeyHold")
            .field("active", &self.active.get())
            .field("queued", &self.queue.borrow().len())
            .finish()
    }
}

impl EditorKeyHold {
    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    pub fn set_active(&self, active: bool) {
        self.active.set(active);
    }

    /// Queues one key. Adjacent printable runs are merged.
    pub fn push(&self, key: HeldEditorKey) {
        if self.overflowed.get() {
            return;
        }
        if self.bytes.get() + key.len() > MAX_QUEUED_BYTES {
            self.queue.borrow_mut().clear();
            self.bytes.set(0);
            self.overflowed.set(true);
            return;
        }
        self.bytes.set(self.bytes.get() + key.len());
        let mut queue = self.queue.borrow_mut();
        match (queue.last_mut(), key) {
            (Some(HeldEditorKey::Text(last)), HeldEditorKey::Text(more)) => last.push_str(&more),
            (_, key) => queue.push(key),
        }
    }

    /// Nothing is queued and nothing overflowed.
    pub fn is_empty(&self) -> bool {
        self.queue.borrow().is_empty() && !self.overflowed.get()
    }

    /// Empties the queue, oldest first.
    pub fn take(&self) -> TakenKeys {
        self.bytes.set(0);
        TakenKeys {
            overflowed: self.overflowed.replace(false),
            keys: std::mem::take(&mut *self.queue.borrow_mut()),
        }
    }

    /// Forgets everything queued (a prompt arrived: it was typed blind).
    pub fn clear(&self) {
        drop(self.take());
    }
}
