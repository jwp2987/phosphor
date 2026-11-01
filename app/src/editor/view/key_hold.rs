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
use std::time::{Duration, Instant};

/// Most bytes queued; past this the queue is dropped (a truncated command line must never run)
/// and the view treats the whole hold as overflowed.
const MAX_QUEUED_BYTES: usize = 64 * 1024;

/// How long after the hold ends the terminal view still takes keys that neither the editor nor
/// the live terminal path claims (#1345). Focus can still be settling into the editor when the
/// held keys are delivered; a key typed in that gap was dropped by both.
const SETTLE_WINDOW: Duration = Duration::from_millis(750);

/// Escape hatch: `PHOSPHOR_NO_SSH_SETTLE_RESCUE=1` restores the pre-#1345 behaviour.
fn settle_rescue_disabled() -> bool {
    std::env::var_os("PHOSPHOR_NO_SSH_SETTLE_RESCUE").is_some()
}

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
    /// Set when the hold ends; keys nobody claims are still taken until then (#1345).
    settle_until: Cell<Option<Instant>>,
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
        if active {
            self.settle_until.set(None);
        }
    }

    /// The hold just ended with its keys delivered (#1345): for a short window the terminal view
    /// keeps taking a key that nobody else claims, so one typed while focus settles into the
    /// editor is delivered live, in order, instead of dropped.
    pub fn begin_settle(&self, now: Instant) {
        if !settle_rescue_disabled() {
            self.settle_until.set(Some(now + SETTLE_WINDOW));
        }
    }

    /// Focus reached the editor (or the hold was reset): nothing is left to rescue.
    pub fn end_settle(&self) {
        self.settle_until.set(None);
    }

    /// Whether the terminal view may take a key nobody else claimed: the hold is active, or it
    /// ended less than the settle window ago.
    pub fn rescues_unclaimed_keys(&self, now: Instant) -> bool {
        self.active.get() || self.settle_until.get().is_some_and(|until| now < until)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #1345: a key typed after the hold ended but before focus settled was dropped by both.
    #[test]
    fn rescue_outlives_the_hold_for_the_settle_window() {
        let hold = EditorKeyHold::default();
        let t0 = Instant::now();
        assert!(!hold.rescues_unclaimed_keys(t0));
        hold.set_active(true);
        assert!(hold.rescues_unclaimed_keys(t0));
        hold.set_active(false);
        hold.begin_settle(t0);
        assert!(hold.rescues_unclaimed_keys(t0 + Duration::from_millis(100)));
        assert!(!hold.rescues_unclaimed_keys(t0 + SETTLE_WINDOW));
        hold.end_settle();
        assert!(!hold.rescues_unclaimed_keys(t0));
    }

    #[test]
    fn a_new_hold_cancels_the_settle_window() {
        let hold = EditorKeyHold::default();
        let t0 = Instant::now();
        hold.begin_settle(t0);
        hold.set_active(true);
        hold.set_active(false);
        assert!(!hold.rescues_unclaimed_keys(t0));
    }
}
