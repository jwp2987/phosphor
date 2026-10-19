use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::terminal::event::{Event as TerminalEvent, EvictionStamp};
use async_channel::Sender;

/// A wrapper struct that emits events which originate from the PTY event loop.
/// Instead of passing individual senders, we can pass through this struct
/// so that users have access to all of the senders in one nicely wrapped struct.
#[derive(Clone)]
pub struct ChannelEventListener {
    /// We have a dedicated channel for "wakeup"s because we throttle the receiver
    /// so that we can coalesce successive wakeup events during situations of high
    /// throughput (e.g. running `yes`).
    wakeups_tx: Sender<()>,
    terminal_events_tx: Sender<TerminalEvent>,
    pty_reads_tx: async_broadcast::Sender<Arc<Vec<u8>>>,
    /// Monotonic count of blocks evicted by `terminal.max_retained_lines` (#889), shared by every
    /// clone of this listener. See [`EvictionStamp`].
    evicted_blocks_total: Arc<AtomicUsize>,
}

impl ChannelEventListener {
    pub fn new(
        wakeups_tx: Sender<()>,
        terminal_events_tx: Sender<TerminalEvent>,
        pty_reads_tx: async_broadcast::Sender<Arc<Vec<u8>>>,
    ) -> Self {
        ChannelEventListener {
            wakeups_tx,
            terminal_events_tx,
            pty_reads_tx,
            evicted_blocks_total: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// A stamp for an index-bearing event being created right now.
    pub fn eviction_stamp(&self) -> EvictionStamp {
        EvictionStamp::new(self.evicted_blocks_total.clone())
    }

    /// Total blocks evicted so far (#889).
    pub fn evicted_blocks_total(&self) -> usize {
        self.evicted_blocks_total.load(Ordering::Acquire)
    }

    /// Records that `count` blocks were just evicted from the block list (#889). Call while still
    /// holding the model lock that performed the eviction.
    pub fn record_blocks_evicted(&self, count: usize) {
        self.evicted_blocks_total.fetch_add(count, Ordering::AcqRel);
    }

    #[cfg(any(test, feature = "integration_tests"))]
    pub fn are_any_events_pending(&self) -> bool {
        !self.wakeups_tx.is_empty()
            || !self.terminal_events_tx.is_empty()
            || !self.pty_reads_tx.is_empty()
    }

    pub fn send_wakeup_event(&self) {
        if let Err(e) = self.wakeups_tx.try_send(()) {
            log::warn!("Failed to send Wakeup event: {e:?}");
        }
    }

    pub fn send_terminal_event(&self, event: TerminalEvent) {
        if let Err(e) = self.terminal_events_tx.try_send(event) {
            let try_send_error_dbg = format!("{e:?}");
            log::warn!(
                "Failed to send Terminal event {:?}: {:?}",
                e.into_inner(),
                try_send_error_dbg
            );
        }
    }

    pub fn send_handler_event(&self, event: HandlerEvent) {
        if let Err(e) = self
            .terminal_events_tx
            .try_send(TerminalEvent::Handler(event))
        {
            log::warn!("Failed to send Terminal Handler event {e:?}");
        }
    }

    pub fn send_pty_read_event(&self, bytes: &[u8]) {
        // Don't bother sending the event if there aren't any
        // active receivers. This avoids an unnecessary allocation of the bytes vector.
        // Note that we don't simply close the sending side since receivers
        // might come alive at some point in the future.
        if self.pty_reads_tx.receiver_count() > 0 {
            if let Err(e) = self.pty_reads_tx.try_broadcast(Arc::new(bytes.to_vec())) {
                log::warn!("Failed to send pty read event: {e:?}");
            }
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
mod testing;

use crate::terminal::model::terminal_model::HandlerEvent;

#[cfg(test)]
pub use testing::*;
