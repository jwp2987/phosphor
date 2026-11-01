//! Ordering for a paste whose clipboard read and byte preparation run off the main thread
//! (#1255).
//!
//! A large paste is read from the clipboard and turned into pty bytes on a background thread so
//! the UI does not freeze. Keys typed while that runs must still reach the pty AFTER the paste, in
//! the order they were typed, so they are held here and released right behind it. A Ctrl-C / Ctrl-D
//! cancels the pending paste instead: nothing of it is sent, what was typed before the abort is
//! released, and the abort goes out last.

/// The message for a paste refused by the size cap (#1382). The paste is never truncated; the
/// keys typed behind it are still delivered.
pub(crate) fn paste_too_large_message(bytes: usize) -> String {
    format!(
        "Paste too large \u{2014} the clipboard holds {} MB (limit {} MB); nothing was pasted and your typing was kept",
        bytes / (1024 * 1024),
        warpui::clipboard::MAX_PASTE_BYTES / (1024 * 1024)
    )
}

/// Why a pty-bound background paste produced no bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasteFailure {
    /// The clipboard owner never answered, or the reader died.
    NoAnswer,
    /// Refused by the size cap (this many bytes of text), never truncated (#1382).
    TooLarge(usize),
}

/// What the caller does with a key typed while a paste may be pending.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum KeyDisposition {
    /// No paste is pending: write the key now.
    PassThrough,
    /// A paste is pending and has taken the key: it is released behind the paste.
    Held,
    /// An abort key cancelled the pending paste. Write these held keys in order, then the abort.
    CancelledPaste(Vec<Vec<u8>>),
}

/// `K` is what is held behind the paste: raw pty bytes for the terminal (the default), editor keys
/// for the input editor (#1300).
#[derive(Debug)]
pub(crate) struct PasteSequencer<K = Vec<u8>> {
    next_id: u64,
    pending: Option<PendingPaste<K>>,
}

impl<K> Default for PasteSequencer<K> {
    fn default() -> Self {
        Self {
            next_id: 0,
            pending: None,
        }
    }
}

#[derive(Debug)]
struct PendingPaste<K> {
    id: u64,
    held: Vec<K>,
}

impl<K> PasteSequencer<K> {
    /// Starts a paste and returns its id. A paste already pending is superseded: its result will be
    /// ignored and its held keys carry over to the new one so none is lost or reordered.
    pub(crate) fn begin(&mut self) -> u64 {
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

    pub(crate) fn is_pending(&self, id: u64) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Holds `key` behind the pending paste. Returns false (and drops nothing) when no paste is
    /// pending: the caller applies the key itself.
    pub(crate) fn hold(&mut self, key: K) -> bool {
        match self.pending.as_mut() {
            Some(pending) => {
                pending.held.push(key);
                true
            }
            None => false,
        }
    }

    /// Cancels the pending paste and returns the keys held so far, in typing order.
    pub(crate) fn cancel(&mut self) -> Vec<K> {
        self.pending
            .take()
            .map(|pending| pending.held)
            .unwrap_or_default()
    }

    /// The paste `id` is ready. Returns the keys held behind it, in typing order, to write right
    /// after the paste; `None` if that paste was cancelled or superseded (write nothing).
    pub(crate) fn complete(&mut self, id: u64) -> Option<Vec<K>> {
        if !self.is_pending(id) {
            return None;
        }
        self.pending.take().map(|pending| pending.held)
    }
}

impl PasteSequencer<Vec<u8>> {
    /// Routes a key typed now. `is_abort` is true for Ctrl-C / Ctrl-D.
    pub(crate) fn on_key(&mut self, bytes: &[u8], is_abort: bool) -> KeyDisposition {
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
}

#[cfg(test)]
#[path = "paste_sequencer_tests.rs"]
mod tests;
