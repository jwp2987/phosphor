//! The reader-side barrier of #1392: whether the pty output the app has APPLIED is provably
//! everything the shell has written, so the shell-hook account the model thread keeps (Precmd,
//! Preexec, the line-editor markers) is current and may be set against a platform sample taken
//! just before.
//!
//! The pty reader thread counts the bytes it reads from the master ([`PtyStreamBarrier::begin_read`]
//! / [`PtyStreamBarrier::end_read`]) and, once a slice has been parsed into the model (every hook in
//! it applied), publishes how many bytes have been applied ([`PtyStreamBarrier::publish_applied`]).
//! Bytes buffered by the parser for a synchronized-output frame are not applied, so nothing is
//! published while such a frame is open.
//!
//! [`PtyStreamBarrier::drained_with`] answers, for a decision made now, whether nothing written
//! before the call can still be unapplied: no read was in flight across the check, the master has
//! nothing unread (the platform's check, passed in), and the applied count equals the read count.
//! bash and zsh write their prompt's Precmd before the line editor puts the tty in raw mode, so
//! under them a raw tty sampled BEFORE a drained barrier means that Precmd has been applied: a hook
//! account that still says "a command runs" is then current, not behind the shell (#933). fish
//! does the opposite (raw first, then `fish_prompt`), so the terminal manager installs this check
//! for bash and zsh only; every other shell keeps the hold.
//!
//! Lock-free: three atomics, one writer (the reader thread), any number of checkers. Every failure
//! or doubt answers "not drained", which keeps today's behaviour (the hold for the prompt).

use std::sync::atomic::{AtomicU64, Ordering};

/// See the module documentation. Shared between the pty reader thread (the only writer) and the
/// typeahead hand-off's probe (a checker).
#[derive(Debug, Default)]
pub struct PtyStreamBarrier {
    /// Bumped before and after every read of the master: odd while a read is in flight. A check
    /// that sees the same even value before and after looking at the master saw no read start or
    /// finish in between.
    read_epoch: AtomicU64,
    /// Bytes read from the master since the session's reader started.
    bytes_read: AtomicU64,
    /// Bytes fed to the parser and applied to the model (every hook in them handled), published
    /// after each parsed slice. Never more than `bytes_read`.
    bytes_applied: AtomicU64,
}

impl PtyStreamBarrier {
    /// The reader is about to read the master (reader thread).
    pub fn begin_read(&self) {
        self.read_epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// The read started by [`Self::begin_read`] returned `got` bytes (0 for an error or
    /// `WouldBlock`) (reader thread).
    pub fn end_read(&self, got: usize) {
        if got > 0 {
            self.bytes_read.fetch_add(got as u64, Ordering::SeqCst);
        }
        self.read_epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// `total` bytes read so far have been parsed into the model and applied (reader thread,
    /// after the parse, with every hook in them handled). Not called while the parser holds a
    /// synchronized-output frame: those bytes are buffered, not applied.
    pub fn publish_applied(&self, total: u64) {
        self.bytes_applied.store(total, Ordering::SeqCst);
    }

    /// Whether everything the shell wrote before this call has been read AND applied.
    /// `unread` is the platform's look at the master: `Some(false)` when it provably holds no
    /// unread byte, `Some(true)` when it does, `None` when it cannot tell. Called by a checker
    /// AFTER it took the sample it wants to set against the hook account.
    pub fn drained_with(&self, unread: impl FnOnce() -> Option<bool>) -> bool {
        let epoch_before = self.read_epoch.load(Ordering::SeqCst);
        if epoch_before % 2 == 1 {
            // A read is in flight: bytes may have left the master and not been counted yet.
            return false;
        }
        if unread() != Some(false) {
            return false;
        }
        let read = self.bytes_read.load(Ordering::SeqCst);
        let applied = self.bytes_applied.load(Ordering::SeqCst);
        let epoch_after = self.read_epoch.load(Ordering::SeqCst);
        // No read started or finished while the master was looked at, so every byte that had
        // left it was counted in `read`; and all of those are applied.
        epoch_after == epoch_before && applied == read
    }

    /// Bytes read so far.
    #[cfg(test)]
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read.load(Ordering::SeqCst)
    }

    /// Bytes applied so far.
    #[cfg(test)]
    pub fn bytes_applied(&self) -> u64 {
        self.bytes_applied.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[path = "pty_stream_barrier_tests.rs"]
mod tests;
