//! Pty-bound paste ordering (phase 1) and subshell setup / bootstrap holds (phase 2) on the one
//! input queue (#1353).
//!
//! A terminal-bound paste reads the clipboard off the UI thread (#1255). Everything typed while it
//! reads must reach the pty AFTER the paste, in typed order. That used to be a `PasteSequencer`
//! that every key was offered to; now the paste is an entry of the pane's `InputQueue` held by a
//! `PasteReading` gate, and the keys typed behind it are entries queued behind that gate. The gate
//! is opened by the reader's result (`InputQueue::on_paste_read`) or closed by ^C / ^D
//! (`InputQueue::offer` of a `Signal`: the paste is cancelled, what was typed before the abort is
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
//! state machine.

use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Duration;

use warpui::r#async::Timer;
use warpui::{SingletonEntity as _, ViewContext};

use super::{Event, TerminalView};
use crate::terminal::input_queue::{
    DeliverOutcome, DestFacts, Destination, DiscardReason, EntryKind, FollowUps, GateKind,
    GateSpec, GateToken, Host, InputEntry, PaneQueue, PasteOwner, SignalKind, is_pty_entry, trace,
};
use crate::terminal::model::typeahead_handoff::{YoungHoldEnd, YoungHoldExpiry};
use crate::terminal::paste_sequencer::PasteFailure;

/// What the pane's paste ordering needs: the queue, the paste bytes waiting for their turn, and
/// the id counter.
pub(super) struct PasteQueueState {
    pub(super) queue: PaneQueue,
    /// Prepared pty bytes of pastes that were read and wait to be delivered, by paste id.
    pub(super) payloads: HashMap<u64, Vec<u8>>,
    /// Queued pastes refused by the size cap (#1382): already told, so no generic failure toast.
    pub(super) refused: std::collections::HashSet<u64>,
    /// Pastes whose payload is TEXT that goes where a paste made at that moment would go
    /// ([`TerminalView::deliver_paste_text`]): the paste held during an ssh connect (#1292) and
    /// the CLI-agent paste. Their destination is decided when they drain, not when they were read.
    pub(super) routed_pastes: std::collections::HashSet<u64>,
    /// The subset of `routed_pastes` read for the ssh hold: cancelled with the hold (a prompt,
    /// ^C, a new command) so a late answer is never pasted anywhere.
    pub(super) ssh_pastes: std::collections::HashSet<u64>,
    /// Ctrl-C / Ctrl-D typed while the paced bootstrap lines are being written. Like the buffer it
    /// replaces they wait for the last line (an interrupt in the middle of the setup script would
    /// leave a half-run bootstrap); [`TerminalView::settle_subshell_bootstrap_gate`] sends them
    /// first, ahead of everything held, when the bootstrap ends.
    pub(super) pending_aborts: Vec<(Vec<u8>, bool)>,
    /// The key of the gate the CLI-agent submit in progress holds, if one is.
    cli_submit_gate: Option<u64>,
    next_cli_submit_key: u64,
    /// Set while the queue delivers an entry the hand-off may hold again (a key or a paste): the
    /// stage then asks the host to put the entry back instead of keeping the bytes itself.
    requeueable_delivery: bool,
    /// Set for the whole of an entry's delivery. A key the hand-off holds while it is set cannot
    /// be offered to the queue (the drain owns it): it is recorded in `requeue_request` and the
    /// host puts it back under its gate when the delivery returns, whatever the entry was.
    delivering: bool,
    /// The hand-off held the entry being delivered; the host answers `Requeue` with this.
    requeue_request: Option<RequeueRequest>,
    /// What goes back to the front of the queue after a `Requeue` when the entry delivered is not
    /// itself the entry the held bytes belong to (set by the host from the kind it delivered).
    requeue_replacement: Option<EntryKind>,
    /// The stamp for keys queued behind a prompt hold (shared with the hand-off).
    pub(super) line_stamp: Option<crate::terminal::model::typeahead_handoff::QueuedLineBytesStamp>,
}

/// What the stage tells the host when the hand-off holds the bytes of the entry it delivers.
pub(super) struct RequeueRequest {
    spec: GateSpec,
    /// The bytes the stage was handed (a paste's payload goes back to the payload map).
    bytes: Vec<u8>,
    /// Whether the bytes take agent control, for an entry made of them.
    takes_over_agent_control: bool,
}

/// How a queued prompt hold ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PromptHoldEnd {
    /// The line editor's ready signal.
    Signal,
    /// The give-up cap: no signal came.
    GaveUp,
}

/// A paced CLI-agent submit is a few hundred ms to a few seconds (a Copilot bracketed paste waits
/// 300 ms for its Enter, each image 300 ms); this is a net for a submit that never reports its end,
/// far beyond any of them. Logged when it fires.
const CLI_SUBMIT_LEAK_NET: Duration = Duration::from_secs(20);

/// Net for a PowerShell setup gate nothing ever closed: twice the typed-input hold limit that
/// releases it in every normal case (the setup's own events end it). Logged when it fires.
const POWERSHELL_GATE_LEAK_NET: Duration = Duration::from_secs(180);

impl PasteQueueState {
    pub(super) fn new() -> Self {
        Self {
            queue: PaneQueue::new(),
            payloads: HashMap::new(),
            refused: Default::default(),
            routed_pastes: Default::default(),
            ssh_pastes: Default::default(),
            pending_aborts: Vec::new(),
            cli_submit_gate: None,
            next_cli_submit_key: 0,
            requeueable_delivery: false,
            delivering: false,
            requeue_request: None,
            requeue_replacement: None,
            line_stamp: None,
        }
    }

    /// The next paste id of this pane (one counter, shared with the editor's pastes).
    pub(super) fn next_paste_id(&mut self) -> u64 {
        self.queue.next_paste_id(PasteOwner::Pty)
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

    fn deliver_entry(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        follow: &mut FollowUps,
    ) -> DeliverOutcome {
        // The pane's queue is shared with the input editor (#1353 step 3): this host reaches both
        // destinations, so an entry that belongs to the editor side (an editor key, an editor
        // paste, text typed behind an Enter) is delivered through the input's own host.
        if !is_pty_entry(&entry.kind) && !matches!(entry.kind, EntryKind::Signal(_)) {
            return self.view.deliver_editor_entry(entry, follow, self.ctx);
        }
        // Whether the entry delivered is itself what a hold of its bytes puts back (a key or a
        // paste written as one piece); any other kind is replaced by a key of the held bytes.
        let mut requeues_itself = false;
        match &entry.kind {
            EntryKind::Paste {
                paste_id,
                payload: Some(_),
            } => match self.view.paste_queue.payloads.remove(paste_id) {
                Some(bytes) if self.view.paste_queue.routed_pastes.remove(paste_id) => {
                    // Text read for an ssh hold / a CLI agent: routed now, as a paste made now.
                    self.view.paste_queue.ssh_pastes.remove(paste_id);
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    self.view.deliver_paste_text(&text, self.ctx);
                    self.written = true;
                }
                Some(bytes) => {
                    requeues_itself = true;
                    self.view.paste_queue.requeueable_delivery = true;
                    self.written = self.view.write_user_bytes_to_pty_after_paste_stage(
                        Cow::Owned(bytes),
                        true,
                        self.ctx,
                    );
                    self.view.paste_queue.requeueable_delivery = false;
                }
                None => log::warn!("input queue: ready paste {paste_id} had no bytes"),
            },
            EntryKind::PtyBytes {
                bytes,
                takes_over_agent_control,
            } => {
                requeues_itself = true;
                self.view.paste_queue.requeueable_delivery = true;
                self.written = self.view.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(bytes.clone()),
                    *takes_over_agent_control,
                    self.ctx,
                );
                self.view.paste_queue.requeueable_delivery = false;
            }
            EntryKind::Signal(kind) => {
                self.written = self.view.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Borrowed(signal_bytes(*kind)),
                    true,
                    self.ctx,
                );
            }
            // The ssh connect typeahead (phase 4): delivered at the hand-over, routed from the
            // facts of this moment.
            EntryKind::Held { class, text } => self.view.deliver_ssh_held(*class, text, self.ctx),
            other => log::warn!(
                "input queue: pty paste queue was handed an entry it does not deliver: {other:?}"
            ),
        }
        // #1353 phase 5: the hand-off took this entry's bytes into a prompt hold (ConPTY). The
        // entry goes back to the front of the queue under the hold's gate, to be taken out again
        // when the line editor signals.
        if let Some(request) = self.view.paste_queue.requeue_request.take() {
            if requeues_itself {
                if let EntryKind::Paste { paste_id, .. } = &entry.kind {
                    self.view
                        .paste_queue
                        .payloads
                        .insert(*paste_id, request.bytes);
                }
            } else {
                // Another kind of entry (a routed paste, an ssh-held text) wrote the key that was
                // held: what goes back under the gate is that key, not the entry that wrote it.
                self.view.paste_queue.requeue_replacement = Some(EntryKind::PtyBytes {
                    bytes: request.bytes,
                    takes_over_agent_control: request.takes_over_agent_control,
                });
            }
            return DeliverOutcome::Requeue(request.spec);
        }
        DeliverOutcome::Delivered
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
        dest: Destination,
        follow: &mut FollowUps,
    ) -> DeliverOutcome {
        // Only a pty entry can write a key that the hand-off holds; an editor entry is delivered
        // by the input's own host.
        self.view.paste_queue.delivering =
            is_pty_entry(&entry.kind) || matches!(entry.kind, EntryKind::Signal(_));
        let outcome = self.deliver_entry(entry, dest, follow);
        self.view.paste_queue.delivering = false;
        outcome
    }

    fn requeued_entry(&mut self, entry: &InputEntry) -> InputEntry {
        let mut remaining = entry.clone();
        if let Some(kind) = self.view.paste_queue.requeue_replacement.take() {
            remaining.kind = kind;
        }
        remaining
    }

    fn discarded(&mut self, entry: &InputEntry, why: DiscardReason) {
        if !is_pty_entry(&entry.kind) && !matches!(entry.kind, EntryKind::Signal(_)) {
            self.view.discard_editor_entry(entry, why, self.ctx);
            return;
        }
        if matches!(entry.kind, EntryKind::Held { .. }) {
            // An ssh-typeahead entry voided by a signal: the hold already said so.
            return;
        }
        if let EntryKind::Paste { paste_id, .. } = &entry.kind {
            self.view.paste_queue.payloads.remove(paste_id);
            self.view.paste_queue.routed_pastes.remove(paste_id);
            self.view.paste_queue.ssh_pastes.remove(paste_id);
        }
        match why {
            // The reader gave nothing (or never answered): say so; the keys behind it follow.
            DiscardReason::PasteFailed | DiscardReason::PasteNotReady => {
                let refused = matches!(&entry.kind, EntryKind::Paste { paste_id, .. }
                    if self.view.paste_queue.refused.contains(paste_id));
                if !refused {
                    self.view
                        .show_error_toast(PASTE_FAILED_TOAST.to_string(), self.ctx);
                }
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
    /// Whether a paste is still being read for this pane. Mid-drain (borrowed) means the paste
    /// that was pending has just been delivered.
    pub(super) fn paste_pending(&self) -> bool {
        self.paste_queue
            .queue
            .try_peek()
            .is_some_and(|queue| queue.has_pty_paste_reading())
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
        // Nothing holds keys: the queue is idle (or mid-drain: this write is part of delivering an
        // entry). Every gate holds every key, whichever route typed it.
        if self
            .paste_queue
            .queue
            .try_peek()
            .map_or(true, |queue| !queue.holds_keys())
        {
            return None;
        }
        // Only the ssh connect hold is open: it holds its own keys (before they get here); what
        // reaches the writer now is a deliberate pass-through or passive traffic.
        if self.only_ssh_gates_open() {
            return None;
        }
        // A young Enter's hold the hand-off's state has outgrown ends before this key is queued
        // behind it (#1353 phase 5b). One borrow-flag read when no such gate is open.
        if abort_signal(bytes).is_none() {
            self.settle_stale_young_hold_before_key(ctx);
            if self
                .paste_queue
                .queue
                .try_peek()
                .map_or(true, |queue| !queue.holds_keys())
            {
                return None;
            }
        }
        let kind = match abort_signal(bytes).filter(|_| !abort_as_entry) {
            Some(signal) => {
                // Ctrl-C / Ctrl-D skips an announced subshell setup (the held keys follow it).
                if self
                    .paste_queue
                    .queue
                    .try_peek()
                    .is_some_and(|queue| queue.has_gate_kind(GateKind::SubshellSetup))
                {
                    self.subshell_bootstrap_announced = None;
                }
                EntryKind::Signal(signal)
            }
            None => {
                trace::hold(self.held_trace_name(), bytes.len());
                // A key typed for the shell's line behind a prompt hold: the ConPTY mid-line
                // guard learns of it here, without the model lock.
                if let Some(stamp) = &self.paste_queue.line_stamp
                    && self.paste_queue.queue.try_peek().is_some_and(|queue| {
                        queue.has_gate_kind(GateKind::LineEditorActive)
                            || queue.has_gate_kind(GateKind::PromptSeen)
                    })
                {
                    stamp.note_now();
                }
                EntryKind::PtyBytes {
                    bytes: bytes.to_vec(),
                    takes_over_agent_control,
                }
            }
        };
        let queue = self.paste_queue.queue.clone();
        let mut host = PtyQueueHost::new(self, ctx);
        queue.lock().offer(&mut host, kind);
        Some(host.written)
    }

    /// Delivers an entry that belongs to the editor side of the shared pane queue through the
    /// input's own host. Follow-ups the editor delivery asks for go back to the draining queue.
    fn deliver_editor_entry(
        &mut self,
        entry: &InputEntry,
        follow: &mut FollowUps,
        ctx: &mut ViewContext<Self>,
    ) -> DeliverOutcome {
        self.input.update(ctx, |input, input_ctx| {
            input.deliver_pane_queue_entry(entry, follow, input_ctx)
        })
    }

    /// The editor-side form of [`Host::discarded`], for an entry this view's drain removed.
    fn discard_editor_entry(
        &mut self,
        entry: &InputEntry,
        why: DiscardReason,
        ctx: &mut ViewContext<Self>,
    ) {
        self.input.update(ctx, |input, input_ctx| {
            input.discard_pane_queue_entry(entry, why, input_ctx)
        });
    }

    /// A drain driven from the editor side stopped at an entry only this view can deliver (a pty
    /// key, a pty paste) and poked the queue's waker (#1353 step 3). Continues the drain here,
    /// where both destinations are reachable. Idempotent: nothing pending, nothing to do.
    pub(super) fn on_pane_queue_needs_drain(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        if !queue
            .try_peek()
            .is_some_and(|queue| queue.pending_foreign())
        {
            return;
        }
        let Some(mut guard) = queue.try_lock() else {
            log::warn!("input queue: pane drain requested while the queue was busy");
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.poke(&mut host);
    }

    /// Starts the gate of a paced CLI-agent submit (#1353 phase 3): until
    /// [`Self::end_cli_submit_gate`], pty keys the user types queue behind the submit instead of
    /// landing between its writes. One submit at a time; a second begin while one is in progress is
    /// part of it.
    pub(super) fn begin_cli_submit_gate(&mut self, ctx: &mut ViewContext<Self>) {
        use crate::terminal::input_queue::GateSpec;
        if self.paste_queue.cli_submit_gate.is_some() {
            return;
        }
        self.paste_queue.next_cli_submit_key += 1;
        let key = self.paste_queue.next_cli_submit_key;
        let queue = self.paste_queue.queue.clone();
        let Some(mut queue) = queue.try_lock() else {
            log::warn!("input queue: CLI-agent submit gate not opened, the queue was busy");
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        queue.open_gate(
            &mut host,
            GateSpec::new(GateKind::CliSubmit, key).with_cap(Some(CLI_SUBMIT_LEAK_NET)),
        );
        self.paste_queue.cli_submit_gate = Some(key);
    }

    /// The paced submit has written its last piece: the keys typed behind it follow, in order.
    pub(super) fn end_cli_submit_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(key) = self.paste_queue.cli_submit_gate.take() else {
            return;
        };
        let queue = self.paste_queue.queue.clone();
        let Some(mut queue) = queue.try_lock() else {
            log::warn!(
                "input queue: CLI-agent submit gate could not be closed, the queue was busy"
            );
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        queue.on_gate_event(&mut host, GateKind::CliSubmit, key);
    }

    /// Writes a piece of a CLI-agent submit. While the submit holds the queue its own writes go
    /// straight to the stages after the paste stage (the keys the user typed behind it wait for
    /// the gate; the submit must not wait behind itself).
    pub(super) fn write_cli_submit_bytes(&mut self, bytes: Vec<u8>, ctx: &mut ViewContext<Self>) {
        if self.paste_queue.cli_submit_gate.is_some() {
            self.write_user_bytes_to_pty_after_paste_stage(Cow::Owned(bytes), true, ctx);
        } else {
            self.write_user_bytes_to_pty(bytes, ctx);
        }
    }

    /// Whether a paced CLI-agent submit holds the queue, for tests.
    #[cfg(test)]
    pub(super) fn cli_submit_gate_open_for_test(&self) -> bool {
        self.paste_queue.cli_submit_gate.is_some()
    }

    /// The give-up cap of paste `token` fired. A no-op if the reader's result already closed it.
    pub(super) fn on_paste_queue_give_up(&mut self, token: GateToken, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        let kind = queue
            .try_peek()
            .and_then(|queue| {
                queue
                    .gates()
                    .iter()
                    .find(|gate| gate.token == token)
                    .copied()
            })
            .map(|gate| gate.kind);
        if kind == Some(GateKind::LineEditorActive) {
            // The line editor never signalled (#1349): the held keys are written anyway, and the
            // hand-off stops holding for this session. The queue's own give-up would deliver them
            // through the stages again; the hold has its own release instead.
            let key = queue
                .try_peek()
                .and_then(|queue| {
                    queue
                        .gates()
                        .iter()
                        .find(|gate| gate.token == token)
                        .map(|gate| gate.key)
                })
                .unwrap_or_default();
            self.release_queued_prompt_hold(key, PromptHoldEnd::GaveUp, ctx);
            return;
        }
        if kind == Some(GateKind::PromptSeen) {
            // The prompt never came for a held young Enter (#933): the hand-off decides what the
            // give-up does (write the keys as typeahead, keep waiting for the report, or find the
            // hold already ended) and the queue's own give-up must not deliver them through the
            // stages a second time.
            let key = queue
                .try_peek()
                .and_then(|queue| {
                    queue
                        .gates()
                        .iter()
                        .find(|gate| gate.token == token)
                        .map(|gate| gate.key)
                })
                .unwrap_or_default();
            self.give_up_queued_young_hold(key, ctx);
            return;
        }
        if kind == Some(GateKind::PasteReading) {
            // #1353 phase 6 (INP-A4): the paste gave up on its clipboard read, so the reader's
            // cancel flag is signalled and the blocked-reader count / log reflect it.
            #[cfg(not(target_family = "wasm"))]
            warpui::clipboard::abandon_pending_reads();
        }
        if kind == Some(GateKind::PowershellSetup) {
            // Nothing closed the PowerShell gate: the setup is treated as over and its keys go.
            log::warn!("input queue: PowerShell setup gate never closed; releasing the held keys");
            self.release_subshell_setup_held_bytes(ctx);
            return;
        }
        if kind == Some(GateKind::SubshellSetup) {
            // Setup is treated as over (what the legacy hold-limit timer does), before the held
            // keys are delivered, so none of them is held a second time.
            self.subshell_bootstrap_announced = None;
            self.dismiss_subshell_setup_waiting_toast(ctx);
        }
        {
            let mut host = PtyQueueHost::new(self, ctx);
            queue.lock().give_up(&mut host, token);
        }
        if kind == Some(GateKind::SubshellBootstrapWrite) {
            self.send_pending_subshell_aborts(ctx);
        }
    }

    /// The legacy buffer name of the queue holding user bytes right now, for the `kh_iq_*` trace
    /// (the names are the ones the old buffers logged, so an old and a new trace compare).
    fn held_trace_name(&self) -> &'static str {
        use crate::terminal::input_queue::trace::legacy;
        match self.paste_queue.queue.try_peek() {
            Some(queue) if queue.has_gate_kind(GateKind::PasteReading) => legacy::PASTE_PTY,
            Some(queue) if queue.has_gate_kind(GateKind::CliSubmit) => legacy::PASTE_PTY,
            Some(queue) if queue.has_gate_kind(GateKind::SubshellBootstrapWrite) => {
                legacy::SUBSHELL_BOOTSTRAP
            }
            Some(queue) if queue.has_gate_kind(GateKind::SubshellSetup) => legacy::SUBSHELL_SETUP,
            Some(queue) if queue.has_gate_kind(GateKind::PowershellSetup) => legacy::SUBSHELL_SETUP,
            Some(queue) if queue.has_gate_kind(GateKind::LineEditorActive) => legacy::HANDOFF,
            Some(queue) if queue.has_gate_kind(GateKind::PromptSeen) => legacy::HANDOFF,
            _ => legacy::PASTE_PTY,
        }
    }

    /// The hand-off just held `bytes` for the prompt and told us it keeps no copy (#1353 phase 5):
    /// they become the first entry of a `LineEditorActive` gate, opened by the line editor's
    /// signal ([`Self::on_prompt_hold_released`]) with the hold's give-up cap as the only timer.
    pub(super) fn hold_prompt_keys_in_queue(
        &mut self,
        hold_id: u64,
        give_up: Duration,
        bytes: Vec<u8>,
        takes_over_agent_control: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let spec = GateSpec::new(GateKind::LineEditorActive, hold_id).with_cap(Some(give_up));
        self.hold_keys_behind_gate(spec, bytes, takes_over_agent_control, ctx);
    }

    /// The hand-off just held `bytes` for a young Enter (#933) and told us it keeps no copy
    /// (#1353 phase 5b): they become the first entry of a `PromptSeen` gate keyed by the hold's
    /// id, ended by the hand-off's decision ([`Self::drain_young_hold_ends`]), the give-up cap
    /// being the only timer.
    pub(super) fn hold_young_keys_in_queue(
        &mut self,
        hold_id: u64,
        give_up: Duration,
        bytes: Vec<u8>,
        takes_over_agent_control: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        let spec = GateSpec::new(GateKind::PromptSeen, hold_id).with_cap(Some(give_up));
        self.hold_keys_behind_gate(spec, bytes, takes_over_agent_control, ctx);
    }

    fn hold_keys_behind_gate(
        &mut self,
        spec: GateSpec,
        bytes: Vec<u8>,
        takes_over_agent_control: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.paste_queue.delivering {
            // The queue is delivering the entry that wrote this key: the host puts the key back
            // at the front of the queue under the gate when the delivery returns (the drain owns
            // the queue until then). A second held write of the same delivery joins the first, in
            // typed order.
            match self.paste_queue.requeue_request.as_mut() {
                Some(request) => request.bytes.extend_from_slice(&bytes),
                None => {
                    self.paste_queue.requeue_request = Some(RequeueRequest {
                        spec,
                        bytes,
                        takes_over_agent_control,
                    });
                }
            }
            return;
        }
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            // Nothing delivers from this queue and yet it is borrowed: the key cannot be put
            // behind its gate. It is written as typed, counted by the hand-off, never lost.
            log::warn!("input queue: a held key found the queue busy; writing it as typed");
            let write = self
                .model
                .lock()
                .typeahead_handoff_mut()
                .route_released_chunk(&bytes);
            if write {
                ctx.emit(Event::WriteHandoffUserBytesToPty {
                    bytes: Cow::Owned(bytes),
                });
            }
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.offer_gated(
            &mut host,
            EntryKind::PtyBytes {
                bytes,
                takes_over_agent_control,
            },
            spec,
        );
    }

    /// A signal or EOF is about to be written while keys are held for the prompt in the queue:
    /// the held keys come back in front of `bytes`, one write in typed order, so an Enter among
    /// them runs before a ^C reaches it (#1162), as the hand-off's own buffer did. Anything else
    /// passes through untouched.
    pub(super) fn prepend_queued_prompt_hold_to_signal(
        &mut self,
        bytes: Cow<'static, [u8]>,
        ctx: &mut ViewContext<Self>,
    ) -> Cow<'static, [u8]> {
        if !bytes.iter().any(super::is_signal_byte)
            || !self
                .paste_queue
                .queue
                .try_peek()
                .is_some_and(|queue| queue.has_gate_kind(GateKind::LineEditorActive))
        {
            return bytes;
        }
        let hold_id = {
            let mut model = self.model.lock();
            model
                .typeahead_handoff_mut()
                .take_queued_prompt_hold_for_signal(&bytes)
        };
        let Some(hold_id) = hold_id else {
            return bytes;
        };
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                return bytes;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::LineEditorActive, hold_id)
        };
        let mut combined = self.bytes_of_taken_entries(entries, ctx).concat();
        trace::release(trace::legacy::HANDOFF, 1, combined.len(), "pty");
        combined.extend_from_slice(&bytes);
        Cow::Owned(combined)
    }

    /// Whether a young Enter's gate is open.
    fn young_hold_gate_open(&self) -> bool {
        self.paste_queue
            .queue
            .try_peek()
            .is_some_and(|queue| queue.has_gate_kind(GateKind::PromptSeen))
    }

    /// A signal or EOF is about to be written while the queue holds a young Enter's keys (#933,
    /// #1353 phase 5b). The hold ends here, in typed order with the signal: the keys are void
    /// for ^C, ^Z and ^\\ (the tty flushes its typeahead), and go out in front of a lone EOF
    /// (the tty keeps typeahead across it). Anything else passes through untouched.
    pub(super) fn settle_queued_young_hold_for_signal(
        &mut self,
        bytes: Cow<'static, [u8]>,
        ctx: &mut ViewContext<Self>,
    ) -> Cow<'static, [u8]> {
        if !bytes.iter().any(super::is_signal_byte) || !self.young_hold_gate_open() {
            return bytes;
        }
        let ended = {
            let mut model = self.model.lock();
            model
                .typeahead_handoff_mut()
                .take_queued_young_hold_for_signal(&bytes)
        };
        let Some((hold_id, before_eof)) = ended else {
            return bytes;
        };
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                // Mid-drain: nothing else can end the hold, so record it for the next attempt.
                self.model
                    .lock()
                    .typeahead_handoff_mut()
                    .restore_young_hold_end(hold_id, YoungHoldEnd::Dropped);
                return bytes;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::PromptSeen, hold_id)
        };
        let chunks = self.bytes_of_taken_entries(entries, ctx);
        if !before_eof {
            return bytes;
        }
        let mut combined = chunks.concat();
        trace::release(trace::legacy::HANDOFF, 1, combined.len(), "pty");
        combined.extend_from_slice(&bytes);
        Cow::Owned(combined)
    }

    /// A key is about to be queued behind a young Enter's gate: a hold the hand-off's state has
    /// outgrown ends first (what the next routed key would have found), so the key is not
    /// swallowed by a stale gate.
    pub(super) fn settle_stale_young_hold_before_key(&mut self, ctx: &mut ViewContext<Self>) {
        if !self.young_hold_gate_open() {
            return;
        }
        let ended = {
            let mut model = self.model.lock();
            model.typeahead_handoff_mut().check_queued_young_hold()
        };
        if ended {
            self.drain_young_hold_ends(ctx);
        }
    }

    /// Applies, to the gate of the queue, every end the hand-off recorded for a young Enter's
    /// hold (a command written, a report, a signal, a state that outgrew it): the entries are
    /// dropped, written behind the command, or split at the Enter. Called from the few places a
    /// hold can end, never per key.
    pub(super) fn drain_young_hold_ends(&mut self, ctx: &mut ViewContext<Self>) {
        let ends = self
            .model
            .lock()
            .typeahead_handoff_mut()
            .take_young_hold_ends();
        for (hold_id, end) in ends {
            self.apply_young_hold_end(hold_id, end, ctx);
        }
    }

    fn apply_young_hold_end(
        &mut self,
        hold_id: u64,
        end: YoungHoldEnd,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                // Mid-drain: keep the end for the next attempt (the gate's give-up finds it too).
                self.model
                    .lock()
                    .typeahead_handoff_mut()
                    .restore_young_hold_end(hold_id, end);
                log::warn!("input queue: young hold {hold_id} end deferred, the queue was busy");
                return;
            };
            if !guard.has_gate(GateKind::PromptSeen, hold_id) {
                // Never queued (the hold's keys stayed in the hand-off's buffer) or already taken.
                if end == YoungHoldEnd::FollowCommand {
                    drop(guard);
                    self.model
                        .lock()
                        .typeahead_handoff_mut()
                        .count_followed_young_hold(hold_id, &[]);
                }
                return;
            }
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::PromptSeen, hold_id)
        };
        let chunks = self.bytes_of_taken_entries(entries, ctx);
        let held_bytes: usize = chunks.iter().map(Vec::len).sum();
        match end {
            YoungHoldEnd::Dropped => {
                trace::release(trace::legacy::HANDOFF, 0, held_bytes, "dropped");
            }
            YoungHoldEnd::FollowCommand => {
                // The command is written (or queued ahead of these): the keys typed behind the
                // Enter follow it, counted as the lines they are.
                let bytes = chunks.concat();
                self.model
                    .lock()
                    .typeahead_handoff_mut()
                    .count_followed_young_hold(hold_id, &bytes);
                trace::release(trace::legacy::HANDOFF, 1, held_bytes, "pty");
                if !bytes.is_empty() {
                    ctx.emit(Event::WriteHandoffUserBytesToPty {
                        bytes: Cow::Owned(bytes),
                    });
                }
                ctx.emit(Event::RetryTypeaheadHandoffRequest);
            }
            YoungHoldEnd::SubmitOnReport => {
                // The Enter is run in the editor; the keys typed after it are held behind it.
                let mut bytes = chunks.concat();
                if !bytes.is_empty() {
                    let rest = bytes.split_off(1);
                    if !rest.is_empty() {
                        self.model
                            .lock()
                            .typeahead_handoff_mut()
                            .prepend_held_keys(&rest);
                    }
                }
            }
        }
    }

    /// The give-up cap of the gate of young hold `hold_id` fired (no prompt came within the
    /// window, or the 2 s an event was to release it in): the hand-off says what the keys do.
    fn give_up_queued_young_hold(&mut self, hold_id: u64, ctx: &mut ViewContext<Self>) {
        // Anything the hand-off already decided wins over the give-up.
        self.drain_young_hold_ends(ctx);
        if !self
            .paste_queue
            .queue
            .try_peek()
            .is_some_and(|queue| queue.has_gate(GateKind::PromptSeen, hold_id))
        {
            return;
        }
        let expiry = self
            .model
            .lock()
            .typeahead_handoff_mut()
            .expire_queued_young_hold(hold_id);
        match expiry {
            YoungHoldExpiry::Keep => {}
            YoungHoldExpiry::Gone => {
                self.drain_young_hold_ends(ctx);
                if self.young_hold_gate_has_key(hold_id) {
                    // The gate outlived a hold with no recorded end: never lose what was typed.
                    log::warn!(
                        "input queue: young hold {hold_id} is gone but its gate is open; writing its keys"
                    );
                    self.release_young_hold_keys(hold_id, false, ctx);
                }
            }
            YoungHoldExpiry::Release => {
                self.release_young_hold_keys(hold_id, true, ctx);
            }
        }
    }

    fn young_hold_gate_has_key(&self, hold_id: u64) -> bool {
        self.paste_queue
            .queue
            .try_peek()
            .is_some_and(|queue| queue.has_gate(GateKind::PromptSeen, hold_id))
    }

    /// Takes the keys out of young hold `hold_id`'s gate and writes them to the pty as typeahead,
    /// in typed order (one write, or one per key event on ConPTY), routed and counted by the
    /// hand-off. `routed` is whether the hand-off already ended the hold.
    fn release_young_hold_keys(&mut self, hold_id: u64, routed: bool, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                log::warn!("input queue: young hold {hold_id} not released, the queue was busy");
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::PromptSeen, hold_id)
        };
        let chunks = self.bytes_of_taken_entries(entries, ctx);
        let writes = {
            let mut model = self.model.lock();
            let handoff = model.typeahead_handoff_mut();
            if routed {
                handoff.route_expired_young_chunks(chunks)
            } else {
                chunks
            }
        };
        // The give-up safety net fired: no prompt arrived in time. Ids and counts only, never bytes.
        log::warn!(
            "typeahead young hold {hold_id} expired with no prompt: releasing {} chunk(s), {} byte(s) to the pty",
            writes.len(),
            writes.iter().map(Vec::len).sum::<usize>()
        );
        self.input
            .update(ctx, |input, _| input.clear_handoff_enter_mark());
        trace::release(
            trace::legacy::HANDOFF,
            writes.len(),
            writes.iter().map(Vec::len).sum(),
            "pty",
        );
        for bytes in writes {
            ctx.emit(Event::WriteHandoffUserBytesToPty {
                bytes: Cow::Owned(bytes),
            });
        }
    }

    /// The bytes of entries taken out of a hand-off gate, one chunk per entry, in order.
    fn bytes_of_taken_entries(
        &mut self,
        entries: Vec<InputEntry>,
        ctx: &mut ViewContext<Self>,
    ) -> Vec<Vec<u8>> {
        let mut chunks = Vec::with_capacity(entries.len());
        for entry in entries {
            // An entry of the editor side that ended up in this gate's segment (the queue is
            // shared with the editor): never lost, delivered to the editor.
            if !is_pty_entry(&entry.kind) {
                self.deliver_editor_entry(&entry, &mut FollowUps::default(), ctx);
                continue;
            }
            match entry.kind {
                EntryKind::PtyBytes { bytes, .. } => chunks.push(bytes),
                EntryKind::Paste { paste_id, .. } => {
                    match self.paste_queue.payloads.remove(&paste_id) {
                        Some(bytes) => chunks.push(bytes),
                        None => log::warn!("input queue: held paste {paste_id} had no bytes"),
                    }
                }
                other => log::warn!(
                    "input queue: a prompt hold held an entry it cannot release: {other:?}"
                ),
            }
        }
        chunks
    }

    /// The line editor signalled (the controller's `PromptHoldReleased`): the keys held for the
    /// prompt go out, in typed order.
    pub(super) fn on_prompt_hold_released(&mut self, hold_id: u64, ctx: &mut ViewContext<Self>) {
        self.release_queued_prompt_hold(hold_id, PromptHoldEnd::Signal, ctx);
    }

    /// Takes the keys out of the prompt hold's gate and writes them as the separate writes they
    /// were typed in, each counted by the hand-off (so no request is decided ahead of them), then
    /// ends the hold and asks for the decision again.
    fn release_queued_prompt_hold(
        &mut self,
        hold_id: u64,
        how: PromptHoldEnd,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                // Mid-drain: the gate's own give-up cap releases the keys.
                log::warn!("input queue: prompt hold {hold_id} not released, the queue was busy");
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::LineEditorActive, hold_id)
        };
        let chunks = self.bytes_of_taken_entries(entries, ctx);
        let held_bytes: usize = chunks.iter().map(Vec::len).sum();
        if how == PromptHoldEnd::GaveUp {
            // The safety net fired: no signal came. Ids and counts only, never bytes.
            log::warn!(
                "typeahead prompt hold {hold_id} gave up with no line-editor signal: releasing {} chunk(s), {held_bytes} byte(s) to the pty",
                chunks.len()
            );
        }
        let writes = {
            let mut model = self.model.lock();
            let handoff = model.typeahead_handoff_mut();
            if how == PromptHoldEnd::GaveUp {
                handoff.expire_queued_prompt_hold(hold_id);
            }
            let mut writes = Vec::with_capacity(chunks.len());
            for chunk in chunks {
                if handoff.route_released_chunk(&chunk) {
                    writes.push(chunk);
                }
            }
            handoff.finish_queued_prompt_hold(hold_id);
            writes
        };
        trace::release(trace::legacy::HANDOFF, writes.len(), held_bytes, "pty");
        for bytes in writes {
            ctx.emit(Event::WriteHandoffUserBytesToPty {
                bytes: Cow::Owned(bytes),
            });
        }
        // The hand-off decided nothing while the keys were held: decide now.
        ctx.emit(Event::RetryTypeaheadHandoffRequest);
    }

    /// The reader's result for paste `id` (`None`: the clipboard owner never answered). Opens the
    /// paste's gate: the paste is delivered, then the keys typed behind it, in order. A cancelled
    /// or given-up paste is not in the queue any more and the late result changes nothing.
    pub(super) fn finish_queued_background_paste(
        &mut self,
        id: u64,
        bytes: Result<Vec<u8>, PasteFailure>,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.paste_queue.queue.clone();
        if queue.peek().has_gate(GateKind::PasteReading, id) {
            let (entries, held_bytes) = queue.peek().queued_summary();
            // The paste itself is one of the queued entries.
            trace::release(
                trace::legacy::PASTE_PTY,
                entries.saturating_sub(1),
                held_bytes,
                "pty",
            );
        }
        let result = match bytes {
            Ok(bytes) => {
                self.paste_queue.payloads.insert(id, bytes);
                // `Some(empty)` is the "ready" marker; the bytes wait in `payloads` so delivery
                // moves them instead of copying a multi-MB paste.
                Ok(Vec::new())
            }
            Err(failure) => {
                if let PasteFailure::TooLarge(bytes) = failure {
                    // Its own message; the gate still opens so the keys behind it follow.
                    self.paste_queue.refused.insert(id);
                    self.show_error_toast(
                        crate::terminal::paste_sequencer::paste_too_large_message(bytes),
                        ctx,
                    );
                }
                Err(())
            }
        };
        {
            let mut host = PtyQueueHost::new(self, ctx);
            queue.lock().on_paste_read(&mut host, id, result);
        }
        // A result that arrived after its paste was cancelled or given up has no entry to claim it.
        // A paste that is still queued (its own gate is closed but another gate, such as the
        // subshell bootstrap's, still holds it) keeps its bytes until it is delivered; dropping
        // them here lost the paste.
        let still_queued = queue.peek().entries().any(
            |entry| matches!(&entry.kind, EntryKind::Paste { paste_id, .. } if *paste_id == id),
        );
        if !still_queued {
            self.paste_queue.payloads.remove(&id);
            self.paste_queue.refused.remove(&id);
        }
    }

    /// Whether the PowerShell setup gate is open (#1048: its keys are entries of the queue).
    pub(super) fn powershell_gate_open(&self) -> bool {
        self.paste_queue
            .queue
            .try_peek()
            .is_some_and(|queue| queue.has_gate_kind(GateKind::PowershellSetup))
    }

    /// A key typed (or released from another gate) while a PowerShell setup governs input is held
    /// behind the `PowershellSetup` gate, in typed order. The first one opens the gate with its
    /// entry (or goes back under it when the queue is delivering it); later ones queue behind.
    pub(super) fn hold_powershell_keys(
        &mut self,
        bytes: Vec<u8>,
        takes_over_agent_control: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        if !self.paste_queue.requeueable_delivery && self.powershell_gate_open() {
            let queue = self.paste_queue.queue.clone();
            let Some(mut guard) = queue.try_lock() else {
                log::warn!("input queue: PowerShell setup key not held, the queue was busy");
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.offer(
                &mut host,
                EntryKind::PtyBytes {
                    bytes,
                    takes_over_agent_control,
                },
            );
            return;
        }
        let spec =
            GateSpec::new(GateKind::PowershellSetup, 0).with_cap(Some(POWERSHELL_GATE_LEAK_NET));
        self.hold_keys_behind_gate(spec, bytes, takes_over_agent_control, ctx);
    }

    /// Takes the keys held for the PowerShell setup out of the queue, in typed order, as bytes.
    /// The gate closes with them; the caller holds again what it does not consume.
    pub(super) fn take_powershell_held_bytes(&mut self, ctx: &mut ViewContext<Self>) -> Vec<u8> {
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                return Vec::new();
            };
            if !guard.has_gate_kind(GateKind::PowershellSetup) {
                return Vec::new();
            }
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::PowershellSetup, 0)
        };
        self.bytes_of_taken_entries(entries, ctx).concat()
    }

    /// The PowerShell setup is over: the gate closes (its event) and the queue's one drain writes
    /// what it held, in typed order.
    pub(super) fn close_powershell_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            return;
        };
        if !guard.has_gate_kind(GateKind::PowershellSetup) {
            return;
        }
        let (chunks, bytes) = guard.queued_since_gate(GateKind::PowershellSetup);
        trace::release(trace::legacy::SUBSHELL_SETUP, chunks, bytes, "pty");
        let mut host = PtyQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, GateKind::PowershellSetup, 0);
    }

    /// Whether the subshell setup gate is open (keys are held behind a setup that is not over).
    pub(super) fn subshell_setup_gate_open(&self) -> bool {
        self.paste_queue
            .queue
            .try_peek()
            .is_some_and(|queue| queue.has_gate_kind(GateKind::SubshellSetup))
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
            match self.paste_queue.queue.try_peek() {
                Some(queue) => (
                    queue.has_gate_kind(GateKind::SubshellSetup),
                    queue.has_gate_kind(GateKind::SubshellBootstrapWrite),
                    queue.gates().iter().all(|gate| {
                        matches!(
                            gate.kind,
                            GateKind::SubshellSetup
                                | GateKind::SubshellBootstrapWrite
                                | GateKind::PowershellSetup
                        )
                    }),
                ),
                None => return SubshellPrep::Normal,
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
            // The rest of a chunked line-clear write is dropped (#1354, now a writer job #1384).
            ctx.emit(super::Event::CancelPacedSetupWrites);
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
        let queue = self.paste_queue.queue.clone();
        if setup_governs && !setup_gate {
            // Cap = what is left of the old hold limit; the legacy timer is the same net.
            let left = age
                .map_or(Duration::from_secs(1), |age| {
                    super::SUBSHELL_SETUP_TYPED_HOLD_LIMIT.saturating_sub(age)
                })
                .max(Duration::from_millis(100));
            let mut host = PtyQueueHost::new(self, ctx);
            queue.lock().open_gate(
                &mut host,
                GateSpec::new(GateKind::SubshellSetup, 0).with_cap(Some(left)),
            );
        }
        if let Some(lines) = in_flight
            && !bootstrap_gate
        {
            let cap = super::subshell_bootstrap_gate_cap(lines);
            let mut host = PtyQueueHost::new(self, ctx);
            queue.lock().open_gate(
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
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
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
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
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
            let queue = self.paste_queue.queue.clone();
            let Some(mut guard) = queue.try_lock() else {
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
                    warpui::clipboard::BackgroundTextRead::TimedOut => Err(PasteFailure::NoAnswer),
                    warpui::clipboard::BackgroundTextRead::TooLarge(bytes) => {
                        Err(PasteFailure::TooLarge(bytes))
                    }
                    warpui::clipboard::BackgroundTextRead::Content(content) => {
                        let text = crate::util::clipboard::clipboard_content_with_escaped_paths(
                            content,
                            shell_dialect,
                            false,
                        );
                        Ok(super::paste_bytes_for_pty(&text, bracketed))
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
            async move { rx.await.unwrap_or(Err(PasteFailure::NoAnswer)) },
            move |me, bytes, ctx| me.finish_queued_background_paste(id, bytes, ctx),
        );
        // A subshell setup in progress holds this paste too: its gates open before the paste
        // entry exists, so the paste waits behind them like any key.
        self.prepare_subshell_gates(&[], ctx);
        let queue = self.paste_queue.queue.clone();
        {
            let mut host = PtyQueueHost::new(self, ctx);
            queue.lock().offer_gated(
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
                    .peek()
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
}
