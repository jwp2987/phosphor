//! Pty-bound paste ordering on the one input queue (#1353, design phase 1).
//!
//! A terminal-bound paste reads the clipboard off the UI thread (#1255). Everything typed while it
//! reads must reach the pty AFTER the paste, in typed order. That used to be a `PasteSequencer`
//! that every key was offered to; now the paste is an entry of the pane's [`InputQueue`] held by a
//! `PasteReading` gate, and the keys typed behind it are entries queued behind that gate. The gate
//! is opened by the reader's result ([`InputQueue::on_paste_read`]) or closed by ^C / ^D
//! ([`InputQueue::offer`] of a `Signal`: the paste is cancelled, what was typed before the abort is
//! delivered, the abort goes last). The one timer is the give-up cap for a reader that never
//! answers (`PHOSPHOR_PASTE_BACKSTOP_MS`), armed once through [`Host::arm_give_up`], token-guarded
//! and logged when it fires.
//!
//! Delivery never goes back through the pipeline that holds: [`PtyQueueHost::deliver`] calls
//! `write_user_bytes_to_pty_after_paste_stage`, the stages AFTER the paste stage, so a delivered
//! key cannot be held a second time (the re-entry that reordered keys in #1222). Anything that
//! re-enters `write_user_bytes_to_pty_inner` while a drain is running finds the queue borrowed and
//! passes straight through, because it is part of delivering the entry being drained.
//!
//! Phase 1 keeps the destination decision (agent, hidden input box, subshell, hand-off) in the
//! stages after this one: the queue only orders. Its facts are therefore lock-free constants
//! (the pty is open and takes the bytes); the model lock is never taken here.
//!
//! `PHOSPHOR_INPUT_QUEUE_PASTE=0` restores the old `PasteSequencer` path, which stays in the tree.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use warpui::ViewContext;
use warpui::r#async::Timer;

use super::TerminalView;
use crate::terminal::input_queue::kill_switch::{self, Area};
use crate::terminal::input_queue::{
    DeliverOutcome, DestFacts, Destination, DiscardReason, EntryKind, FollowUps, GateKind,
    GateToken, Host, InputEntry, InputQueue, SignalKind, trace,
};

/// What the pane's paste ordering needs: the queue, the paste bytes waiting for their turn, and
/// the id counter.
pub(super) struct PasteQueueState {
    pub(super) queue: Rc<RefCell<InputQueue>>,
    /// Prepared pty bytes of pastes that were read and wait to be delivered, by paste id.
    pub(super) payloads: HashMap<u64, Vec<u8>>,
    next_id: u64,
    /// `PHOSPHOR_INPUT_QUEUE_PASTE`; false keeps the legacy `PasteSequencer` path.
    pub(super) enabled: bool,
}

impl PasteQueueState {
    pub(super) fn new() -> Self {
        Self {
            queue: Rc::new(RefCell::new(InputQueue::new())),
            payloads: HashMap::new(),
            next_id: 0,
            enabled: kill_switch::is_enabled(Area::Paste),
        }
    }

    fn next_paste_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

/// The signal an abort key stands for (`is_abort_key`: ^C or ^D only).
fn abort_signal(bytes: &[u8]) -> Option<SignalKind> {
    match bytes {
        [0x03] => Some(SignalKind::Interrupt),
        [0x04] => Some(SignalKind::Eof),
        _ => None,
    }
}

fn signal_bytes(kind: SignalKind) -> &'static [u8] {
    match kind {
        SignalKind::Interrupt => &[0x03],
        SignalKind::Eof => &[0x04],
        SignalKind::Suspend => &[0x1a],
        SignalKind::Quit => &[0x1c],
    }
}

const PASTE_FAILED_TOAST: &str =
    "Paste failed \u{2014} the clipboard did not answer; your typing was kept";

/// The [`Host`] of the pane's queue: delivers with the leaf writers of the terminal view.
pub(super) struct PtyQueueHost<'a, 'b> {
    view: &'a mut TerminalView,
    ctx: &'a mut ViewContext<'b, TerminalView>,
    /// What the last delivered write reported (false: the agent owned the command).
    pub(super) written: bool,
}

impl<'a, 'b> PtyQueueHost<'a, 'b> {
    pub(super) fn new(
        view: &'a mut TerminalView,
        ctx: &'a mut ViewContext<'b, TerminalView>,
    ) -> Self {
        Self {
            view,
            ctx,
            written: true,
        }
    }
}

impl Host for PtyQueueHost<'_, '_> {
    fn facts(&self) -> DestFacts {
        // Lock-free by design (3.2): the stages after this one decide agent / hidden-input /
        // subshell / hand-off routing exactly as before.
        DestFacts {
            pty_takes_input: true,
            pty_open: true,
            ..DestFacts::default()
        }
    }

    fn deliver(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        _follow: &mut FollowUps,
    ) -> DeliverOutcome {
        match &entry.kind {
            EntryKind::Paste {
                paste_id,
                payload: Some(_),
            } => match self.view.paste_queue.payloads.remove(paste_id) {
                Some(bytes) => {
                    self.written = self.view.write_user_bytes_to_pty_after_paste_stage(
                        Cow::Owned(bytes),
                        true,
                        self.ctx,
                    );
                }
                None => log::warn!("input queue: ready paste {paste_id} had no bytes"),
            },
            EntryKind::PtyBytes {
                bytes,
                takes_over_agent_control,
            } => {
                self.written = self.view.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(bytes.clone()),
                    *takes_over_agent_control,
                    self.ctx,
                );
            }
            EntryKind::Signal(kind) => {
                self.written = self.view.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Borrowed(signal_bytes(*kind)),
                    true,
                    self.ctx,
                );
            }
            other => log::warn!(
                "input queue: pty paste queue was handed an entry it does not deliver: {other:?}"
            ),
        }
        DeliverOutcome::Delivered
    }

    fn discarded(&mut self, entry: &InputEntry, why: DiscardReason) {
        if let EntryKind::Paste { paste_id, .. } = &entry.kind {
            self.view.paste_queue.payloads.remove(paste_id);
        }
        match why {
            // The reader gave nothing (or never answered): say so; the keys behind it follow.
            DiscardReason::PasteFailed | DiscardReason::PasteNotReady => {
                self.view
                    .show_error_toast(PASTE_FAILED_TOAST.to_string(), self.ctx);
            }
            // ^C / ^D cancelled it: silent, as before.
            DiscardReason::PasteCancelled => {}
            other => log::warn!("input queue: pty entry discarded: {other:?}"),
        }
    }

    fn arm_give_up(&mut self, token: GateToken, _kind: GateKind, after: Duration) {
        // The ONE timer of a paste gate: a safety net for a reader that never answers. The reader's
        // result closes the gate first in every normal case and this then finds nothing to do.
        self.ctx.spawn(Timer::after(after), move |me, _, ctx| {
            me.on_paste_queue_give_up(token, ctx)
        });
    }

    fn warn(&mut self, line: &str) {
        log::warn!("{line}");
    }

    fn log_enabled(&self) -> bool {
        warpui::input_order::enabled()
    }

    fn log(&mut self, line: &str) {
        warpui::input_order::log_key_hop("kh_iq_queue", || line.to_owned());
    }
}

impl TerminalView {
    /// Whether a paste is still being read for this pane (either path).
    pub(super) fn paste_pending(&self) -> bool {
        if self.paste_queue.enabled {
            // Mid-drain (borrowed) means the paste that was pending has just been delivered.
            self.paste_queue
                .queue
                .try_borrow()
                .is_ok_and(|queue| queue.has_gate_kind(GateKind::PasteReading))
        } else {
            self.paste_sequencer.has_pending()
        }
    }

    /// A user key arrives while the queue may be holding input. Returns `None` when nothing is
    /// held (the caller writes it now: no allocation, no lock, one borrow-flag read), else the
    /// result of offering it to the queue. A queue that is mid-drain (borrowed) means this write is
    /// part of delivering an entry: it passes through.
    pub(super) fn offer_to_paste_queue(
        &mut self,
        bytes: &[u8],
        takes_over_agent_control: bool,
        ctx: &mut ViewContext<Self>,
    ) -> Option<bool> {
        if self
            .paste_queue
            .queue
            .try_borrow()
            .map_or(true, |queue| queue.is_idle())
        {
            return None;
        }
        let kind = match abort_signal(bytes) {
            Some(signal) => EntryKind::Signal(signal),
            None => {
                trace::hold(trace::legacy::PASTE_PTY, bytes.len());
                EntryKind::PtyBytes {
                    bytes: bytes.to_vec(),
                    takes_over_agent_control,
                }
            }
        };
        let queue = Rc::clone(&self.paste_queue.queue);
        let mut host = PtyQueueHost::new(self, ctx);
        queue.borrow_mut().offer(&mut host, kind);
        Some(host.written)
    }

    /// The give-up cap of paste `token` fired. A no-op if the reader's result already closed it.
    pub(super) fn on_paste_queue_give_up(&mut self, token: GateToken, ctx: &mut ViewContext<Self>) {
        let queue = Rc::clone(&self.paste_queue.queue);
        let mut host = PtyQueueHost::new(self, ctx);
        queue.borrow_mut().give_up(&mut host, token);
    }
}

#[cfg(not(target_family = "wasm"))]
impl TerminalView {
    /// Starts a terminal-bound paste on the queue: reads the clipboard and builds the pty bytes
    /// on a worker thread; the paste is an entry held by a `PasteReading` gate until the result.
    pub(super) fn start_queued_background_paste(
        &mut self,
        reader: warpui::clipboard::BackgroundTextReader,
        shell_dialect: Option<warp_util::path::ShellDialect>,
        bracketed: bool,
        backstop: Option<Duration>,
        ctx: &mut ViewContext<Self>,
    ) {
        use crate::terminal::input_queue::GateSpec;
        let id = self.paste_queue.next_paste_id();
        let (tx, rx) = futures::channel::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("paste-read".into())
            .spawn(move || {
                let result = match reader() {
                    warpui::clipboard::BackgroundTextRead::TimedOut => None,
                    warpui::clipboard::BackgroundTextRead::Content(content) => {
                        let text = crate::util::clipboard::clipboard_content_with_escaped_paths(
                            content,
                            shell_dialect,
                            false,
                        );
                        Some(super::paste_bytes_for_pty(&text, bracketed))
                    }
                };
                let _ = tx.send(result);
            });
        if spawned.is_err() {
            self.show_error_toast(
                "Paste failed \u{2014} could not start the clipboard read".to_string(),
                ctx,
            );
            return;
        }
        ctx.spawn(
            async move { rx.await.ok().flatten() },
            move |me, bytes, ctx| me.finish_queued_background_paste(id, bytes, ctx),
        );
        let queue = Rc::clone(&self.paste_queue.queue);
        {
            let mut host = PtyQueueHost::new(self, ctx);
            queue.borrow_mut().offer_gated(
                &mut host,
                EntryKind::Paste {
                    paste_id: id,
                    payload: None,
                },
                GateSpec::new(GateKind::PasteReading, id).with_cap(backstop),
            );
        }
        // Say what is happening when the read takes noticeable time (a big paste). Time is the
        // requirement here (user feedback), not correctness.
        ctx.spawn(
            Timer::after(super::PASTE_PROGRESS_TOAST_DELAY),
            move |me, _, ctx| {
                if me
                    .paste_queue
                    .queue
                    .borrow()
                    .has_gate(GateKind::PasteReading, id)
                {
                    if !ctx.has_singleton_model::<crate::workspace::ToastStack>() {
                        return;
                    }
                    let window_id = ctx.window_id();
                    crate::workspace::ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                        let toast = crate::view_components::DismissibleToast::success(
                            "Pasting from the clipboard\u{2026} (Ctrl-C cancels)".to_string(),
                        );
                        toast_stack.add_ephemeral_toast(toast, window_id, ctx);
                    });
                }
            },
        );
    }

    /// The reader's result for paste `id` (`None`: the clipboard owner never answered). Opens the
    /// paste's gate: the paste is delivered, then the keys typed behind it, in order. A cancelled
    /// or given-up paste is not in the queue any more and the late result changes nothing.
    fn finish_queued_background_paste(
        &mut self,
        id: u64,
        bytes: Option<Vec<u8>>,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = Rc::clone(&self.paste_queue.queue);
        if queue.borrow().has_gate(GateKind::PasteReading, id) {
            let (entries, held_bytes) = queue.borrow().queued_summary();
            // The paste itself is one of the queued entries.
            trace::release(
                trace::legacy::PASTE_PTY,
                entries.saturating_sub(1),
                held_bytes,
                "pty",
            );
        }
        let result = match bytes {
            Some(bytes) => {
                self.paste_queue.payloads.insert(id, bytes);
                // `Some(empty)` is the "ready" marker; the bytes wait in `payloads` so delivery
                // moves them instead of copying a multi-MB paste.
                Ok(Vec::new())
            }
            None => Err(()),
        };
        {
            let mut host = PtyQueueHost::new(self, ctx);
            queue.borrow_mut().on_paste_read(&mut host, id, result);
        }
        // A result that arrived after its paste was cancelled or given up has no entry to claim it.
        self.paste_queue.payloads.remove(&id);
    }
}
