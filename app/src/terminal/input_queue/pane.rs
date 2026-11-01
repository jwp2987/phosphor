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
//! Nothing here delivers, times or decides anything: it is the shape of access, not behaviour.

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::InputQueue;

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
            }),
        }
    }

    /// Whether `other` is the same queue (two handles of one pane).
    pub fn ptr_eq(&self, other: &PaneQueue) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }

    /// The view holds (or stops holding) the keys typed during an ssh connect.
    pub fn set_ssh_hold_active(&self, active: bool) {
        self.inner.ssh_hold_active.set(active);
    }

    /// The view is holding the keys typed during an ssh connect.
    pub fn ssh_hold_active(&self) -> bool {
        self.inner.ssh_hold_active.get()
    }

    /// An operation holds the queue right now (so a delivery that asks is part of it).
    pub fn is_draining(&self) -> bool {
        self.inner.draining.get()
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

    /// Mutable access, for the operations that arrive, close a gate or drain. Panics if the queue
    /// is already held (a delivery must hand follow-ups to `FollowUps`, never re-enter).
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
        let guard = self.inner.queue.try_borrow_mut().ok()?;
        self.inner.draining.set(true);
        Some(QueueGuard {
            guard,
            draining: &self.inner.draining,
        })
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

    /// #1339 / #1400: one typed-characters event is offered once, whichever route is first.
    #[test]
    fn a_typed_event_offered_by_the_element_is_dropped_by_the_editor_once() {
        let pane = PaneQueue::new();
        assert!(
            !pane.take_typed_offered(5, "a"),
            "nothing offered: the editor offers its own"
        );
        pane.mark_typed_offered(5, "a");
        assert!(!pane.take_typed_offered(6, "a"));
        assert!(pane.take_typed_offered(5, "a"));
        assert!(!pane.take_typed_offered(5, "a"), "the mark is consumed");
    }

    /// A new hold forgets a mark left over from an earlier one.
    #[test]
    fn a_new_ssh_hold_forgets_a_stale_mark() {
        let pane = PaneQueue::new();
        pane.mark_typed_offered(9, "x");
        pane.set_ssh_hold_active(true);
        assert!(pane.ssh_hold_active());
        assert!(!pane.take_typed_offered(9, "x"));
        pane.set_ssh_hold_active(false);
        assert!(!pane.ssh_hold_active());
    }

    /// The view re-asserts the hold on every held key; that is not a new hold.
    #[test]
    fn re_asserting_an_active_hold_keeps_the_fresh_mark() {
        let pane = PaneQueue::new();
        pane.set_ssh_hold_active(true);
        pane.mark_typed_offered(3, "k");
        pane.set_ssh_hold_active(true);
        assert!(pane.take_typed_offered(3, "k"));
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
