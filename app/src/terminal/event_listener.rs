use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::terminal::event::Event as TerminalEvent;
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
    /// Whether anything the terminal view renders has changed since the view last drew
    /// (#812). Shared by every clone of this listener, so the model, its blocks and the
    /// background timers all write the same bit.
    ///
    /// A wakeup used to mean "redraw": `TerminalView::handle_terminal_wakeup` ended in an
    /// unconditional `ctx.notify()`, so every pty read cost a full-window layout+paint even
    /// when the bytes changed nothing on screen. Now the model's `ansi::Handler` marks this
    /// bit as it mutates render state, every terminal event and explicit
    /// [`send_wakeup_event`](Self::send_wakeup_event) marks it too, and the view only
    /// notifies when [`take_render_dirty`](Self::take_render_dirty) returns `true`. The pty
    /// event loops send their post-read wakeup with
    /// [`send_byte_processing_wakeup_event`](Self::send_byte_processing_wakeup_event), which
    /// does not mark, so a read whose bytes touched nothing visible is the one case that no
    /// longer forces a frame.
    render_dirty: Arc<AtomicBool>,
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
            // Start dirty: the first wakeup after construction always draws.
            render_dirty: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Records that something the terminal view renders has changed, so the next wakeup
    /// the view handles produces a frame. See the field docs on `render_dirty`.
    pub fn mark_render_dirty(&self) {
        self.render_dirty.store(true, Ordering::Release);
    }

    /// Returns whether a render-affecting change is pending, and clears the flag. The view
    /// calls this once per handled wakeup; a `false` means the wakeup can skip its notify.
    ///
    /// A mark that lands after this take is never lost: every marker either sends its own
    /// wakeup afterwards or runs inside a pty read that does, so a later wakeup is already
    /// queued and will observe the flag.
    pub fn take_render_dirty(&self) -> bool {
        self.render_dirty.swap(false, Ordering::AcqRel)
    }

    /// Whether a render-affecting change is pending, without clearing it.
    #[cfg(test)]
    pub fn is_render_dirty(&self) -> bool {
        self.render_dirty.load(Ordering::Acquire)
    }

    #[cfg(any(test, feature = "integration_tests"))]
    pub fn are_any_events_pending(&self) -> bool {
        !self.wakeups_tx.is_empty()
            || !self.terminal_events_tx.is_empty()
            || !self.pty_reads_tx.is_empty()
    }

    /// Asks the view to redraw: marks the render-dirty flag and queues a wakeup. Every caller
    /// outside the pty event loops has just changed something visible (a block finished, a
    /// render delay elapsed, scrollback loaded, synchronized output ended, the shell exited),
    /// so this keeps their frame unconditional.
    pub fn send_wakeup_event(&self) {
        self.mark_render_dirty();
        self.send_byte_processing_wakeup_event();
    }

    /// Queues a wakeup after a pty read without marking the render-dirty flag (#812).
    ///
    /// Only the pty event loops call this, right after `Processor::parse_bytes`: whatever the
    /// bytes changed has already been marked by the model's `ansi::Handler`, so the view can
    /// tell a read that changed the screen from one that did not (captured in-band command
    /// or completions output, a terminal query such as `CSI 6 n`, or the leading bytes of a
    /// sequence that completes in a later read).
    pub fn send_byte_processing_wakeup_event(&self) {
        if let Err(e) = self.wakeups_tx.try_send(()) {
            log::warn!("Failed to send Wakeup event: {e:?}");
        }
    }

    pub fn send_terminal_event(&self, event: TerminalEvent) {
        // The view may change what it renders in response to any terminal event, and before
        // #812 the wakeup that followed the same bytes drew that change; keep that frame.
        self.mark_render_dirty();
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
        self.mark_render_dirty();
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

#[cfg(test)]
#[path = "event_listener_tests.rs"]
mod tests;
