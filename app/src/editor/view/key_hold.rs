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
    /// Set when the hold ends; keys nobody claims are still taken until focus is in the editor
    /// (#1345). Ended by an event (the editor gaining focus, a new hold, a reset), never a clock
    /// (#1348).
    settling: Cell<bool>,
    queue: RefCell<Vec<HeldEditorKey>>,
    bytes: Cell<usize>,
    overflowed: Cell<bool>,
    /// The typed-characters event (`warpui::input_order::typed_dispatch_seq`) and text already
    /// offered to the hold by one route (#1400). The terminal element and the focused editor both
    /// see the same event; whichever offers first claims it and the other must not offer it again
    /// (#1339), whatever the order they run in.
    claimed: RefCell<Option<(u64, String)>>,
    /// Wakes the terminal view when a key was queued here, so it joins the ordered ssh hold at
    /// once instead of waiting for the next key or the hand-over (#1400).
    waker: RefCell<Option<async_channel::Sender<()>>>,
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
            self.settling.set(false);
        }
    }

    /// The hold just ended with its keys delivered (#1345): until focus reaches the editor the
    /// terminal view keeps taking a key that nobody else claims (it only does so while focus is
    /// on the input but not the editor), so one typed while focus settles is delivered live, in
    /// order, instead of dropped.
    pub fn begin_settle(&self) {
        self.settling.set(true);
    }

    /// Focus reached the editor (or the hold was reset): nothing is left to rescue.
    pub fn end_settle(&self) {
        self.settling.set(false);
    }

    /// Whether the terminal view may take a key nobody else claimed: the hold is active, or it
    /// ended and focus has not reached the editor since.
    pub fn rescues_unclaimed_keys(&self) -> bool {
        self.active.get() || self.settling.get()
    }

    /// The terminal element offered typed-characters event `seq` (text `text`) to the hold. The
    /// same event is also dispatched to the focused editor as an action, after the element pass;
    /// [`Self::take_offered`] makes that copy a no-op (offered exactly once, #1339/#1400).
    pub fn mark_offered(&self, seq: u64, text: &str) {
        *self.claimed.borrow_mut() = Some((seq, text.to_owned()));
    }

    /// Whether this event was already offered by the terminal element (then the editor drops its
    /// copy). Consumes the mark.
    pub fn take_offered(&self, seq: u64, text: &str) -> bool {
        let mut claimed = self.claimed.borrow_mut();
        if claimed
            .as_ref()
            .is_some_and(|(s, t)| *s == seq && t == text)
        {
            *claimed = None;
            return true;
        }
        false
    }

    /// The terminal view's wake-up channel (bounded to one: a wake pending is as good as two).
    pub fn set_waker(&self, waker: async_channel::Sender<()>) {
        *self.waker.borrow_mut() = Some(waker);
    }

    /// Queues one key. Adjacent printable runs are merged.
    pub fn push(&self, key: HeldEditorKey) {
        self.push_inner(key);
        if let Some(waker) = self.waker.borrow().as_ref() {
            let _ = waker.try_send(());
        }
    }

    fn push_inner(&self, key: HeldEditorKey) {
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

    /// #1345 / #1348: a key typed after the hold ended but before focus settled was dropped by
    /// both. The rescue outlives the hold until an event (focus in the editor) ends it, with no
    /// clock involved.
    #[test]
    fn rescue_outlives_the_hold_until_focus_settles() {
        let hold = EditorKeyHold::default();
        assert!(!hold.rescues_unclaimed_keys());
        hold.set_active(true);
        assert!(hold.rescues_unclaimed_keys());
        hold.set_active(false);
        hold.begin_settle();
        assert!(hold.rescues_unclaimed_keys());
        hold.end_settle();
        assert!(!hold.rescues_unclaimed_keys());
    }

    /// #1339 / #1400: one typed-characters event is offered once, whichever route is first.
    #[test]
    fn a_typed_event_offered_by_the_element_is_dropped_by_the_editor_once() {
        let hold = EditorKeyHold::default();
        assert!(
            !hold.take_offered(5, "a"),
            "nothing offered: the editor offers its own"
        );
        hold.mark_offered(5, "a");
        assert!(!hold.take_offered(6, "a"));
        assert!(hold.take_offered(5, "a"));
        assert!(!hold.take_offered(5, "a"), "the mark is consumed");
    }

    #[test]
    fn a_new_hold_cancels_the_settle() {
        let hold = EditorKeyHold::default();
        hold.begin_settle();
        hold.set_active(true);
        hold.set_active(false);
        assert!(!hold.rescues_unclaimed_keys());
    }
}
