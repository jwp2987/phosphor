//! Pty-bound paste ordering (phase 1) and subshell setup / bootstrap holds (phase 2) on the one
//! input queue (#1353).
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
//!
//! Phase 2 puts the two subshell buffers on the SAME queue instance. A `SubshellSetup` gate is
//! open from the moment a warpify setup is announced until its end (`release_subshell_setup_held_bytes`:
//! the first prompt after `Bootstrapped`, abandoned, skipped, timed out); a `SubshellBootstrapWrite`
//! gate is open while the paced bootstrap lines are written, until `finish_subshell_bootstrap`.
//! Keys typed meanwhile are `PtyBytes` entries behind them, released in typed order by the single
//! drain, so the #1222 patch (`setup_held_keys_go_before`) is not needed on this path. Whether a
//! setup governs input is still decided by the old predicates (`prepare_subshell_gates`), and the
//! gates are also opened lazily by the first key, so a state set up without `write_init_subshell_
//! bytes_to_pty` behaves the same. Caps (logged when they fire): the old 90 s typed-input hold
//! limit, and N bootstrap lines x 2 s. PowerShell's line-by-line setup (#1048) stays on its own
//! state machine. `PHOSPHOR_INPUT_QUEUE_SUBSHELL=0` restores the old buffers, which stay in the
//! tree and are also the fallback for a write made while the queue is delivering.

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
    GateSpec, GateToken, Host, InputEntry, InputQueue, SignalKind, trace,
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
    /// `PHOSPHOR_INPUT_QUEUE_SUBSHELL` (#1353 phase 2); false keeps the legacy
    /// `subshell_bootstrap_in_flight.held_user_bytes` and `subshell_setup_held_bytes` buffers.
    pub(super) subshell_enabled: bool,
    /// Ctrl-C / Ctrl-D typed while the paced bootstrap lines are being written. Like the buffer it
    /// replaces they wait for the last line (an interrupt in the middle of the setup script would
    /// leave a half-run bootstrap); [`TerminalView::settle_subshell_bootstrap_gate`] sends them
    /// first, ahead of everything held, when the bootstrap ends.
    pub(super) pending_aborts: Vec<(Vec<u8>, bool)>,
}

impl PasteQueueState {
    pub(super) fn new() -> Self {
        Self {
            queue: Rc::new(RefCell::new(InputQueue::new())),
            payloads: HashMap::new(),
            next_id: 0,
            enabled: kill_switch::is_enabled(Area::Paste),
            subshell_enabled: kill_switch::is_enabled(Area::Subshell),
            pending_aborts: Vec::new(),
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
        abort_as_entry: bool,
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
        let kind = match abort_signal(bytes).filter(|_| !abort_as_entry) {
            Some(signal) => {
                // Ctrl-C / Ctrl-D skips an announced subshell setup (the held keys follow it).
                if self.paste_queue.subshell_enabled
                    && self
                        .paste_queue
                        .queue
                        .try_borrow()
                        .is_ok_and(|queue| queue.has_gate_kind(GateKind::SubshellSetup))
                {
                    self.subshell_bootstrap_announced = None;
                }
                EntryKind::Signal(signal)
            }
            None => {
                trace::hold(self.held_trace_name(), bytes.len());
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

    /// The give-up cap of gate `token` fired. A no-op if the gate's event already closed it.
    pub(super) fn on_paste_queue_give_up(&mut self, token: GateToken, ctx: &mut ViewContext<Self>) {
        let queue = Rc::clone(&self.paste_queue.queue);
        let kind = queue
            .try_borrow()
            .ok()
            .and_then(|queue| {
                queue
                    .gates()
                    .iter()
                    .find(|gate| gate.token == token)
                    .copied()
            })
            .map(|gate| gate.kind);
        if kind == Some(GateKind::SubshellSetup) {
            // Setup is treated as over (what the legacy hold-limit timer does), before the held
            // keys are delivered, so none of them is held a second time.
            self.subshell_bootstrap_announced = None;
            self.dismiss_subshell_setup_waiting_toast(ctx);
        }
        {
            let mut host = PtyQueueHost::new(self, ctx);
            queue.borrow_mut().give_up(&mut host, token);
        }
        if kind == Some(GateKind::SubshellBootstrapWrite) {
            self.send_pending_subshell_aborts(ctx);
        }
    }

    /// The legacy buffer name of the queue holding user bytes right now, for the `kh_iq_*` trace
    /// (the names are the ones the old buffers logged, so an old and a new trace compare).
    fn held_trace_name(&self) -> &'static str {
        use crate::terminal::input_queue::trace::legacy;
        match self.paste_queue.queue.try_borrow() {
            Ok(queue) if queue.has_gate_kind(GateKind::PasteReading) => legacy::PASTE_PTY,
            Ok(queue) if queue.has_gate_kind(GateKind::SubshellBootstrapWrite) => {
                legacy::SUBSHELL_BOOTSTRAP
            }
            Ok(queue) if queue.has_gate_kind(GateKind::SubshellSetup) => legacy::SUBSHELL_SETUP,
            _ => legacy::PASTE_PTY,
        }
    }

    /// Whether the subshell setup gate is open (keys are held behind a setup that is not over).
    pub(super) fn subshell_setup_gate_open(&self) -> bool {
        self.paste_queue
            .queue
            .try_borrow()
            .is_ok_and(|queue| queue.has_gate_kind(GateKind::SubshellSetup))
    }

    /// Before a user key is written (#1353 phase 2): opens, from the SAME facts the legacy hold
    /// used, the gates that make the key wait. The legacy decisions stay the authority for "is a
    /// setup governing input"; what changed is where the held keys live (entries of the one
    /// queue, in typed order, released by the event that ends the setup).
    pub(super) fn prepare_subshell_gates(
        &mut self,
        bytes: &[u8],
        ctx: &mut ViewContext<Self>,
    ) -> SubshellPrep {
        // A queue that is mid-drain: this write is part of delivering an entry.
        let (setup_gate, bootstrap_gate, only_subshell_gates) =
            match self.paste_queue.queue.try_borrow() {
                Ok(queue) => (
                    queue.has_gate_kind(GateKind::SubshellSetup),
                    queue.has_gate_kind(GateKind::SubshellBootstrapWrite),
                    queue.gates().iter().all(|gate| {
                        matches!(
                            gate.kind,
                            GateKind::SubshellSetup | GateKind::SubshellBootstrapWrite
                        )
                    }),
                ),
                Err(_) => return SubshellPrep::Normal,
            };
        let in_flight = self
            .subshell_bootstrap_in_flight
            .as_ref()
            .map(|f| f.remaining_lines);
        // One model lock for the announcement (as the legacy hold took), none when nothing is
        // announced and nothing is held. An announcement that lapsed without a release (setup
        // finished) is the common idle case and exits here.
        let age = if self.subshell_bootstrap_announced.is_some() {
            self.subshell_bootstrap_announced_age()
        } else {
            None
        };
        if !setup_gate && !bootstrap_gate && in_flight.is_none() && age.is_none() {
            return SubshellPrep::Normal;
        }
        let abort = super::is_abort_key(bytes);
        if abort && in_flight.is_some() {
            self.chunked_clear_write_generation += 1;
            self.note_input_for_deferred_subshell_setup(bytes);
            if setup_gate || !only_subshell_gates {
                // An abort that also skips the setup goes FIRST once the last line is out, ahead
                // of what was typed (the old buffer's release order); kept until then.
                self.paste_queue.pending_aborts.push((bytes.to_vec(), true));
                return SubshellPrep::Stashed;
            }
        }
        let powershell_off = self.powershell_setup_hold == super::PowerShellSetupHold::Off;
        // The setup is over (`Bootstrapped`, abandoned, ...) while keys are still held, released
        // only at the first prompt: they go out first, then this key behind them (#1222, now by
        // construction rather than by a patch).
        if setup_gate && !abort && powershell_off && age.is_none() {
            self.release_subshell_setup_held_bytes(ctx);
            return self.prepare_subshell_gates(bytes, ctx);
        }
        // Not while the shell is asking for something: typing is the answer to it.
        let awaits_answer = !abort
            && in_flight.is_none()
            && !bootstrap_gate
            && powershell_off
            && age.is_some()
            && self.active_output_awaits_an_answer();
        if awaits_answer && only_subshell_gates {
            return SubshellPrep::Bypass;
        }
        let setup_governs = !abort
            && !awaits_answer
            && powershell_off
            && super::subshell_setup_should_hold_typed_bytes(age, abort);
        let queue = Rc::clone(&self.paste_queue.queue);
        if setup_governs && !setup_gate {
            // Cap = what is left of the old hold limit; the legacy timer is the same net.
            let left = age
                .map_or(Duration::from_secs(1), |age| {
                    super::SUBSHELL_SETUP_TYPED_HOLD_LIMIT.saturating_sub(age)
                })
                .max(Duration::from_millis(100));
            let mut host = PtyQueueHost::new(self, ctx);
            queue.borrow_mut().open_gate(
                &mut host,
                GateSpec::new(GateKind::SubshellSetup, 0).with_cap(Some(left)),
            );
        }
        if let Some(lines) = in_flight
            && !bootstrap_gate
        {
            // N lines x 2 s: the marker give-up falls back to timed pacing long before this.
            let cap = Duration::from_secs(2) * (lines as u32 + 1);
            let mut host = PtyQueueHost::new(self, ctx);
            queue.borrow_mut().open_gate(
                &mut host,
                GateSpec::new(GateKind::SubshellBootstrapWrite, 0).with_cap(Some(cap)),
            );
        }
        if abort && in_flight.is_some() {
            // No setup to skip: the abort waits for the last line in typed order, like any key.
            SubshellPrep::HeldInOrder
        } else {
            SubshellPrep::Normal
        }
    }

    /// Closes the setup gate (its event: the setup is over) and delivers what it held, in order.
    pub(super) fn close_subshell_setup_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = Rc::clone(&self.paste_queue.queue);
        let Ok(mut guard) = queue.try_borrow_mut() else {
            return;
        };
        if !guard.has_gate_kind(GateKind::SubshellSetup) {
            return;
        }
        let (chunks, bytes) = guard.queued_since_gate(GateKind::SubshellSetup);
        trace::release(trace::legacy::SUBSHELL_SETUP, chunks, bytes, "pty");
        let mut host = PtyQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, GateKind::SubshellSetup, 0);
    }

    /// The bootstrap's last line is out (or it was abandoned): the aborts typed meanwhile go
    /// first, then the gate closes and what was typed is delivered, in order.
    pub(super) fn settle_subshell_bootstrap_gate(&mut self, ctx: &mut ViewContext<Self>) {
        self.send_pending_subshell_aborts(ctx);
        let queue = Rc::clone(&self.paste_queue.queue);
        let Ok(mut guard) = queue.try_borrow_mut() else {
            return;
        };
        if !guard.has_gate_kind(GateKind::SubshellBootstrapWrite) {
            return;
        }
        let (chunks, bytes) = guard.queued_since_gate(GateKind::SubshellBootstrapWrite);
        trace::release(trace::legacy::SUBSHELL_BOOTSTRAP, chunks, bytes, "pty");
        let mut host = PtyQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, GateKind::SubshellBootstrapWrite, 0);
    }

    /// Sends the stashed aborts as signals: they meet the gates still open (setup: the abort goes
    /// first and releases the held keys behind it; bootstrap: same).
    fn send_pending_subshell_aborts(&mut self, ctx: &mut ViewContext<Self>) {
        let aborts = std::mem::take(&mut self.paste_queue.pending_aborts);
        for (bytes, _takes_over) in aborts {
            let Some(signal) = abort_signal(&bytes) else {
                continue;
            };
            if self.subshell_setup_gate_open() {
                self.subshell_bootstrap_announced = None;
            }
            let queue = Rc::clone(&self.paste_queue.queue);
            let Ok(mut guard) = queue.try_borrow_mut() else {
                continue;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.offer(&mut host, EntryKind::Signal(signal));
        }
    }
}

/// What [`TerminalView::prepare_subshell_gates`] decided for a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SubshellPrep {
    /// Offer it to the queue like any key (which delivers it now if nothing is held).
    Normal,
    /// A shell prompt is asking for an answer: the key goes straight to the pty.
    Bypass,
    /// An abort typed during the bootstrap lines that also skips the setup; kept until the last
    /// line, then sent first.
    Stashed,
    /// An abort typed during the bootstrap lines with no setup to skip: an ordinary entry, so it
    /// reaches the pty at its place in what was typed.
    HeldInOrder,
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
        // A subshell setup in progress holds this paste too: its gates open before the paste
        // entry exists, so the paste waits behind them like any key.
        if self.paste_queue.subshell_enabled {
            self.prepare_subshell_gates(&[], ctx);
        }
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
