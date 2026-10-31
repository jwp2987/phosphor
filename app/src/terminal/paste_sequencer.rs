//! Ordering for a paste whose clipboard read and byte preparation run off the main thread
//! (#1255).
//!
//! A large paste is read from the clipboard and turned into pty bytes on a background thread so
//! the UI does not freeze. Keys typed while that runs must still reach the pty AFTER the paste, in
//! the order they were typed, so they are held here and released right behind it. A Ctrl-C / Ctrl-D
//! cancels the pending paste instead: nothing of it is sent, what was typed before the abort is
//! released, and the abort goes out last.

/// What the caller does with a key typed while a paste may be pending.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum KeyDisposition {
    /// No paste is pending: write the key now.
    PassThrough,
    /// A paste is pending and has taken the key: it is released behind the paste.
    Held,
    /// An abort key cancelled the pending paste. Write these held keys in order, then the abort.
    CancelledPaste(Vec<Vec<u8>>),
}

#[derive(Debug, Default)]
pub(super) struct PasteSequencer {
    next_id: u64,
    pending: Option<PendingPaste>,
}

#[derive(Debug)]
struct PendingPaste {
    id: u64,
    held: Vec<Vec<u8>>,
}

impl PasteSequencer {
    /// Starts a paste and returns its id. A paste already pending is superseded: its result will be
    /// ignored and its held keys carry over to the new one so none is lost or reordered.
    pub(super) fn begin(&mut self) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let held = self
            .pending
            .take()
            .map(|pending| pending.held)
            .unwrap_or_default();
        self.pending = Some(PendingPaste { id, held });
        id
    }

    pub(super) fn is_pending(&self, id: u64) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Routes a key typed now. `is_abort` is true for Ctrl-C / Ctrl-D.
    pub(super) fn on_key(&mut self, bytes: &[u8], is_abort: bool) -> KeyDisposition {
        let Some(pending) = self.pending.as_mut() else {
            return KeyDisposition::PassThrough;
        };
        if is_abort {
            let held = std::mem::take(&mut pending.held);
            self.pending = None;
            return KeyDisposition::CancelledPaste(held);
        }
        pending.held.push(bytes.to_vec());
        KeyDisposition::Held
    }

    /// The paste `id` is ready. Returns the keys held behind it, in typing order, to write right
    /// after the paste; `None` if that paste was cancelled or superseded (write nothing).
    pub(super) fn complete(&mut self, id: u64) -> Option<Vec<Vec<u8>>> {
        if !self.is_pending(id) {
            return None;
        }
        self.pending.take().map(|pending| pending.held)
    }
}

#[cfg(test)]
#[path = "paste_sequencer_tests.rs"]
mod tests;
