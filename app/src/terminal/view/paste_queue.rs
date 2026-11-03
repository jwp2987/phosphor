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

use super::dest_table::Replayed;
use super::{AfterEnterInput, Event, KeyPathScope, TerminalView, held_bytes_through_last_enter};
use crate::editor::{EditorAction, HeldPasteKey};
use crate::terminal::input_queue::shadow::{DestClass, note_leaf as note_shadow_leaf};
use crate::terminal::input_queue::table::{Class, Outcome};
use crate::terminal::input_queue::{
    Deferred, DeliverOutcome, DestFacts, Destination, DestinationMode, DiscardReason, EntryKind,
    FollowUps, GateKeysThen, GateKind, GateSpec, GateToken, Host, InputEntry, Origin, PaneQueue,
    PasteOwner, PowershellKeysThen, SignalKind, Stamp, destination_mode, is_editor_paste_id,
    is_pty_entry, trace,
};
use crate::terminal::model::typeahead_handoff::{
    CONPTY_PROMPT_HOLD_GIVE_UP, PromptHoldCap, YoungHoldEnd, YoungHoldExpiry, forces_shell_submit,
    text_for_editor,
};
use crate::terminal::paste_sequencer::PasteFailure;

/// #1577: the toast shown when a timer handed the keys held behind a gate back to the input.
const GATE_GIVE_UP_TOAST: &str = "Input was waiting too long, so it was not sent; what you typed is in the input. Check it, \
     then press Enter.";

/// A warning line for a pane closed while it still held input (#1441): entries and bytes queued
/// or held (typed keys, ssh-held typeahead, pastes), pastes still being read from the clipboard
/// and pastes read but not delivered. Counts only, never content. `None` when nothing was lost.
pub(super) fn dropped_input_summary(
    queue: &crate::terminal::input_queue::InputQueue,
    unsent_paste_payloads: usize,
) -> Option<String> {
    let (entries, bytes) = queue.queued_summary();
    let paste_reading = queue.has_pty_paste_reading() || queue.has_editor_paste_reading();
    if entries == 0 && !paste_reading && unsent_paste_payloads == 0 {
        return None;
    }
    Some(format!(
        "pane closed with input not delivered: queued_entries={entries} queued_bytes={bytes} paste_being_read={paste_reading} pastes_read_not_delivered={unsent_paste_payloads} gates={}",
        queue.gates().len()
    ))
}

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
    pub(super) delivering: bool,
    /// The stamp the entry being delivered got at its sink (#1424): the shadow comparison and the
    /// table judge a delivered entry by it. Meaningful only while `delivering`.
    pub(super) delivering_stamp: Stamp,
    /// The origin the entry being delivered is judged with (#1424): `Held` for an ssh-held input,
    /// `Replay` for text / an Enter typed behind an Enter that waited, else the entry's own.
    /// Meaningful only while `delivering`.
    pub(super) delivering_origin: Origin,
    /// The hand-off held the entry being delivered; the host answers `Requeue` with this.
    requeue_request: Option<RequeueRequest>,
    /// What goes back to the front of the queue after a `Requeue` when the entry delivered is not
    /// itself the entry the held bytes belong to (set by the host from the kind it delivered).
    requeue_replacement: Option<EntryKind>,
    /// The stamp for keys queued behind a prompt hold (shared with the hand-off).
    pub(super) line_stamp: Option<crate::terminal::model::typeahead_handoff::QueuedLineBytesStamp>,
    /// #1584: the prompt hold whose overdue keys were already announced (log + toast), so a
    /// re-armed cap does not repeat it.
    prompt_hold_waiting_told: Option<u64>,
}

/// What the stage tells the host when the hand-off holds the bytes of the entry it delivers.
pub(super) struct RequeueRequest {
    spec: GateSpec,
    /// The bytes the stage was handed (a paste's payload goes back to the payload map).
    bytes: Vec<u8>,
    /// Whether the bytes take agent control, for an entry made of them.
    takes_over_agent_control: bool,
    /// What goes back instead of an entry made of `bytes`: an ssh-held input (#1415) goes back as
    /// the `Held` entry it will be delivered as, routed again when its gate opens.
    replacement: Option<EntryKind>,
}

impl RequeueRequest {
    /// The entry that goes back to the front of the queue.
    fn into_entry(self) -> EntryKind {
        self.replacement.unwrap_or(EntryKind::PtyBytes {
            bytes: self.bytes,
            takes_over_agent_control: self.takes_over_agent_control,
        })
    }
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
            delivering_stamp: Stamp::UNSTAMPED,
            delivering_origin: Origin::Keyboard,
            requeue_request: None,
            requeue_replacement: None,
            line_stamp: None,
            prompt_hold_waiting_told: None,
        }
    }

    /// Asks the host to put `held` back at the front of the queue under `spec` when the delivery
    /// in progress returns. A second input of the same class in one delivery joins the first, in
    /// typed order. `Err(held)` hands the input back when it cannot join (another class, another
    /// kind of request): the caller defers it behind the same gate (`Deferred::OfferGated`).
    pub(super) fn request_requeue_held(
        &mut self,
        spec: GateSpec,
        held: EntryKind,
    ) -> Result<(), EntryKind> {
        let Some(request) = self.requeue_request.as_mut() else {
            self.requeue_request = Some(RequeueRequest {
                spec,
                bytes: Vec::new(),
                takes_over_agent_control: false,
                replacement: Some(held),
            });
            return Ok(());
        };
        if request.spec.kind == spec.kind && request.spec.key == spec.key {
            if let (
                Some(EntryKind::Held {
                    class: have,
                    text: have_text,
                }),
                EntryKind::Held { class, text },
            ) = (request.replacement.as_mut(), &held)
                && *class == *have
            {
                have_text.push_str(text);
                return Ok(());
            }
        }
        Err(held)
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

/// The signal `bytes` stand for, all four of them (OD-1: ^C ^D ^Z ^\).
pub(super) fn signal_of(bytes: &[u8]) -> Option<SignalKind> {
    match bytes {
        [0x03] => Some(SignalKind::Interrupt),
        [0x04] => Some(SignalKind::Eof),
        [0x1a] => Some(SignalKind::Suspend),
        [0x1c] => Some(SignalKind::Quit),
        _ => None,
    }
}

/// The signal the pane QUEUE jumps the queue for (#1424 OD-1): all four in `table` mode, where
/// ^Z and ^\ are signals like ^C (the tty flushes its pending input on INTR, QUIT and SUSP);
/// ^C and ^D only otherwise (`abort_signal`, the legacy classification).
pub(super) fn queue_signal_of(bytes: &[u8]) -> Option<SignalKind> {
    if destination_mode() == DestinationMode::Table {
        signal_of(bytes)
    } else {
        abort_signal(bytes)
    }
}

pub(super) fn signal_bytes(kind: SignalKind) -> &'static [u8] {
    match kind {
        SignalKind::Interrupt => &[0x03],
        SignalKind::Eof => &[0x04],
        SignalKind::Suspend => &[0x1a],
        SignalKind::Quit => &[0x1c],
    }
}

/// Net for the gate that holds the keys typed behind a deferred Enter: the patience timers and the
/// Enter's own events end the hold long before this; logged when it fires.
const HANDOFF_GATE_LEAK_NET: Duration = Duration::from_secs(60);

/// The spec of the `HandoffReport` gate named `id`.
fn handoff_report_spec(id: u64) -> GateSpec {
    GateSpec::new(GateKind::HandoffReport, id).with_cap(Some(HANDOFF_GATE_LEAK_NET))
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

    /// Delivers text or an Enter typed behind an Enter that waited: to the editor or the pty, from
    /// the facts of this moment. An Enter that waits again returns its gate as `OpensGate`; a key
    /// the hand-off holds goes back under its gate (`Requeue`).
    fn deliver_replayed_entry(&mut self, entry: &InputEntry) -> DeliverOutcome {
        let input = match &entry.kind {
            EntryKind::Text(text) => AfterEnterInput::Text(text.clone()),
            _ => AfterEnterInput::Enter,
        };
        self.view
            .input
            .as_ref(self.ctx)
            .note_replayed_after_enter(&input);
        super::dest_table::note_legacy_chain_delivery();
        self.view.route_after_enter_input(&input, self.ctx);
        self.finish_replayed_delivery()
    }

    /// The same delivery decided by the destination table (`PHOSPHOR_QUEUE_DESTINATION=table`,
    /// #1424 step 4): the input is judged once, on the facts of this moment and the stamp the
    /// entry got at its sink, and executed with the leaf writers; the legacy chain is not called.
    fn deliver_replayed_entry_table(&mut self, entry: &InputEntry) -> DeliverOutcome {
        let input = match &entry.kind {
            EntryKind::Text(text) => AfterEnterInput::Text(text.clone()),
            _ => AfterEnterInput::Enter,
        };
        self.view
            .input
            .as_ref(self.ctx)
            .note_replayed_after_enter(&input);
        let origin = Origin::Replay(entry.id);
        match &input {
            AfterEnterInput::Text(text) => self.view.deliver_via_table(
                super::dest_table::Replayed::Typed(text),
                origin,
                entry.stamp,
                self.ctx,
            ),
            AfterEnterInput::Enter => self.view.deliver_via_table(
                super::dest_table::Replayed::Key("\r"),
                origin,
                entry.stamp,
                self.ctx,
            ),
        }
        self.finish_replayed_delivery()
    }

    /// What a delivered text / Enter caused: a key the hand-off held goes back under its gate, an
    /// Enter that waits again hands its gate to the drain.
    fn finish_replayed_delivery(&mut self) -> DeliverOutcome {
        if let Some(request) = self.view.paste_queue.requeue_request.take() {
            let spec = request.spec;
            self.view.paste_queue.requeue_replacement = Some(request.into_entry());
            return DeliverOutcome::Requeue(spec);
        }
        match self
            .view
            .input
            .update(self.ctx, |input, _| input.take_stashed_gate())
        {
            Some(spec) => DeliverOutcome::OpensGate(spec),
            None => DeliverOutcome::Delivered,
        }
    }

    fn deliver_entry(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        follow: &mut FollowUps,
    ) -> DeliverOutcome {
        // Text and Enters typed behind an Enter that waited are routed here, by the view that
        // reaches both destinations and decides where each goes when it is delivered (#1353).
        if matches!(entry.kind, EntryKind::Text(_) | EntryKind::Enter) {
            if destination_mode() == DestinationMode::Table {
                return self.deliver_replayed_entry_table(entry);
            }
            return self.deliver_replayed_entry(entry);
        }
        // An editor paste queued behind an Enter gate follows the line it belongs to (#1585): the
        // table decides it like the Text and Enter entries around it. Where the table says editor
        // it keeps the editor's own delivery (rich content, paste sources) below.
        if destination_mode() == DestinationMode::Table
            && let EntryKind::Paste {
                paste_id,
                payload: Some(_),
            } = &entry.kind
            && is_editor_paste_id(*paste_id)
            && let Some(text) = self.view.input.read(self.ctx, |input, _| {
                input.peek_paste_payload_text(*paste_id)
            })
        {
            let _key_path = KeyPathScope::enter(&self.view.key_path_depth);
            let (plan, facts) = self.view.decide_delivery(
                Replayed::Paste(&text),
                Origin::Replay(entry.id),
                entry.stamp,
                self.ctx,
            );
            if !matches!(plan.outcome, Outcome::EditorInsert | Outcome::AgentInput) {
                // The line this paste belongs to went elsewhere (a connect hold, the pty): follow it.
                let taken = self
                    .view
                    .input
                    .update(self.ctx, |input, _| {
                        input.take_paste_payload_text(*paste_id)
                    })
                    .unwrap_or(text);
                warpui::input_order::log_key_hop("kh_editor_paste_followed_line", || {
                    format!("outcome={:?} chars={}", plan.outcome, taken.chars().count())
                });
                self.view
                    .execute_table_plan(Replayed::Paste(&taken), &plan, &facts, self.ctx);
                return self.finish_replayed_delivery();
            }
        }
        // The pane's queue is shared with the input editor (#1353 step 3): this host reaches both
        // destinations, so an entry that belongs to the editor side (an editor key, an editor
        // paste) is delivered through the input's own host.
        if !is_pty_entry(&entry.kind) && !matches!(entry.kind, EntryKind::Signal(_)) {
            let outcome = self.view.deliver_editor_entry(entry, follow, self.ctx);
            // An editor delivery (an Enter that submits and writes the command) can write a key
            // the hand-off holds (#1418): what goes back under the gate is that key, not the
            // editor entry that was applied.
            if let Some(request) = self.view.paste_queue.requeue_request.take() {
                if outcome != DeliverOutcome::Delivered {
                    log::warn!(
                        "input queue: an editor delivery both held a key and asked for {outcome:?}; the held key wins"
                    );
                }
                let spec = request.spec;
                self.view.paste_queue.requeue_replacement = Some(request.into_entry());
                return DeliverOutcome::Requeue(spec);
            }
            return outcome;
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
            EntryKind::Held { class, text } => {
                if destination_mode() == DestinationMode::Table {
                    self.view
                        .deliver_held_via_table(*class, text, entry.stamp, self.ctx);
                } else {
                    super::dest_table::note_legacy_chain_delivery();
                    self.view.deliver_ssh_held(*class, text, self.ctx);
                }
                // #1571: a held Enter that reached a shell not ready yet opened the gate of that
                // Enter while this drain delivered it: what is still queued behind it waits for
                // that gate, in order, instead of being delivered into the held line.
                if self.view.paste_queue.requeue_request.is_none()
                    && let Some(spec) = self
                        .view
                        .input
                        .update(self.ctx, |input, _| input.take_stashed_gate())
                {
                    return DeliverOutcome::OpensGate(spec);
                }
            }
            other => log::warn!(
                "input queue: pty paste queue was handed an entry it does not deliver: {other:?}"
            ),
        }
        // #1353 phase 5: the hand-off took this entry's bytes into a prompt hold (ConPTY). The
        // entry goes back to the front of the queue under the hold's gate, to be taken out again
        // when the line editor signals.
        if let Some(request) = self.view.paste_queue.requeue_request.take() {
            let spec = request.spec;
            if requeues_itself && request.replacement.is_none() {
                if let EntryKind::Paste { paste_id, .. } = &entry.kind {
                    self.view
                        .paste_queue
                        .payloads
                        .insert(*paste_id, request.bytes);
                }
            } else {
                // Another kind of entry (a routed paste, an ssh-held text) wrote the key that was
                // held: what goes back under the gate is that key, not the entry that wrote it.
                self.view.paste_queue.requeue_replacement = Some(request.into_entry());
            }
            return DeliverOutcome::Requeue(spec);
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
        // EVERY delivery can write a key that the hand-off holds (an editor Enter writes the
        // command it submitted, #1418): the key goes back under its gate when this returns, never
        // "as typed" because the queue is held by this very drain.
        self.view.paste_queue.delivering = true;
        self.view.paste_queue.delivering_stamp = entry.stamp;
        self.view.paste_queue.delivering_origin = match &entry.kind {
            EntryKind::Held { .. } => Origin::Held,
            EntryKind::Text(_) | EntryKind::Enter => Origin::Replay(entry.id),
            _ => entry.origin,
        };
        let outcome = self.deliver_entry(entry, dest, follow);
        self.view.paste_queue.delivering = false;
        outcome
    }

    /// A ^D (^Z, ^\) typed behind an Enter deferred for the shell's report is typed after the keys
    /// held there and goes out after them, in typed order: it waits behind the gate like a key
    /// (^C cancels the deferred Enter first, `cancel_deferred_enter`).
    fn signal_must_wait(&self, gate: GateKind) -> bool {
        gate == GateKind::HandoffReport
    }

    /// An Enter delivered from the ssh hand-over may have submitted a command (a second `ssh`)
    /// whose connect gate is armed by the event that submit emits, after this call returns: the
    /// held input behind it is judged on a later update, with that gate in place (#1421).
    fn yield_after(&mut self, entry: &InputEntry) -> bool {
        // Only an Enter can submit a command; a Backspace or Ctrl-U cannot arm a gate, so the
        // drain goes on through them in the same turn (keys behind them must not wait a turn).
        matches!(
            &entry.kind,
            EntryKind::Held { class, text }
                if *class == crate::terminal::input_queue::HELD_KEY
                    && matches!(text.as_str(), "\r" | "\n" | "\r\n")
        )
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
            DiscardReason::PasteCancelled => {
                // A pending command that waited for this paste goes now (#1435).
                self.view
                    .paste_queue
                    .queue
                    .defer(Deferred::FlushProgrammaticEdits);
            }
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

    fn overflowed(&mut self, first: bool, dropped: u64, queued: usize) {
        if first || dropped % 4096 == 0 {
            log::warn!(
                "input queue full ({queued} entries waiting): dropped {dropped} newest arrival(s)"
            );
        }
        if first {
            self.view.show_ssh_typing_toast(
                "Too much input is waiting to be sent to this terminal; the newest keys were discarded, not run",
                self.ctx,
            );
        }
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
    #[cfg(test)]
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
        // Operations that found the queue busy come before this key (typed order).
        self.flush_deferred_queue_ops(ctx);
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
        if queue_signal_of(bytes).is_none() {
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
        let kind = match queue_signal_of(bytes).filter(|_| !abort_as_entry) {
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
        // The shadow comparison (#1424): a key the pane queue holds is `Held`, not `Pty` (a signal
        // jumps the queue, it is not held).
        if !matches!(kind, EntryKind::Signal(_)) {
            note_shadow_leaf(DestClass::Held);
        }
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            // Became busy since the checks above: the key waits its turn, in order.
            queue.defer(Deferred::Offer {
                kind,
                origin: Origin::Keyboard,
                stamp: queue.stamp_now(),
            });
            return Some(false);
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.offer(&mut host, kind);
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
        // The deferred operations first (the same wake serves both), then a stopped drain.
        self.flush_deferred_queue_ops(ctx);
        let queue = self.paste_queue.queue.clone();
        let pending = match queue.try_peek() {
            Some(queue) => queue.pending_foreign(),
            None => {
                // Busy right now (#1428): the wake must not be lost. Look again once free.
                log::warn!("input queue: pane drain requested while the queue was busy; deferred");
                queue.defer(Deferred::Poke);
                return;
            }
        };
        if !pending {
            return;
        }
        let Some(mut guard) = queue.try_lock() else {
            queue.defer(Deferred::Poke);
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.resume_after_yield(&mut host);
    }

    /// #1580 (F2): the agent write's version of [`Self::on_pane_queue_needs_drain`]: continues a
    /// drain that STOPPED at a foreign entry, but never a YIELDED one (`poke` returns at once
    /// while the queue is yielded). Only the waker's handler resumes a yield, after the yielding
    /// entry's effects (the submit's `ExecuteCommand`, which arms the inner connect window) were
    /// applied; resuming in the agent write's own update would re-open #1421. The caller has
    /// already flushed the deferred operations.
    pub(super) fn continue_stopped_drain(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        let pending = match queue.try_peek() {
            Some(queue) => queue.pending_foreign(),
            None => {
                log::warn!("input queue: agent drain requested while the queue was busy; deferred");
                queue.defer(Deferred::Poke);
                return;
            }
        };
        if !pending {
            return;
        }
        let Some(mut guard) = queue.try_lock() else {
            queue.defer(Deferred::Poke);
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
        self.flush_deferred_queue_ops(ctx);
        let queue = self.paste_queue.queue.clone();
        let spec = GateSpec::new(GateKind::CliSubmit, key).with_cap(Some(CLI_SUBMIT_LEAK_NET));
        // The submit owns the gate from here on, whether the queue is free or busy.
        self.paste_queue.cli_submit_gate = Some(key);
        let Some(mut queue) = queue.try_lock() else {
            // Opened by the next operation, in order with the end below (#1419).
            queue_defer_open_gate(&self.paste_queue.queue, spec);
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        queue.open_gate(&mut host, spec);
    }

    /// The paced submit has written its last piece: the keys typed behind it follow, in order.
    pub(super) fn end_cli_submit_gate(&mut self, ctx: &mut ViewContext<Self>) {
        self.flush_deferred_queue_ops(ctx);
        let Some(key) = self.paste_queue.cli_submit_gate.take() else {
            return;
        };
        let queue = self.paste_queue.queue.clone();
        let Some(mut queue) = queue.try_lock() else {
            // The close is recorded, never lost (#1419): the keys typed behind the submit follow
            // as soon as the queue is free, not at the 20 s leak net.
            queue.defer(Deferred::GateEvent {
                kind: GateKind::CliSubmit,
                key,
            });
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        queue.on_gate_event(&mut host, GateKind::CliSubmit, key);
    }

    /// Writes a piece of a CLI-agent submit. While the submit holds the queue its own writes go
    /// straight to the stages after the paste stage (the keys the user typed behind it wait for
    /// the gate; the submit must not wait behind itself).
    pub(super) fn write_cli_submit_bytes(&mut self, bytes: Vec<u8>, ctx: &mut ViewContext<Self>) {
        // #1424: the CLI-submit origin is exempt from every hold (row O-cli-submit); the table
        // logs its plan in `table` mode and shadow compares it with what the write did.
        let row = self.begin_origin_row(Class::Raw, Origin::CliSubmit, ctx);
        let written = if self.paste_queue.cli_submit_gate.is_some() {
            self.write_user_bytes_to_pty_after_paste_stage(Cow::Owned(bytes), true, ctx)
        } else {
            self.write_user_bytes_to_pty(bytes, ctx)
        };
        self.end_origin_row(row, written);
    }

    /// Whether a paced CLI-agent submit holds the queue, for tests.
    #[cfg(test)]
    pub(super) fn cli_submit_gate_open_for_test(&self) -> bool {
        self.paste_queue.cli_submit_gate.is_some()
    }

    /// The give-up cap of paste `token` fired. A no-op if the reader's result already closed it.
    pub(super) fn on_paste_queue_give_up(&mut self, token: GateToken, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        // A cap is one-shot: firing while the queue is busy must not consume it (#1417). It runs
        // again, whole, once the queue is free (the handler is token-guarded).
        if queue.try_peek().is_none() {
            queue.defer(Deferred::GiveUpCap(token));
            return;
        }
        self.flush_deferred_queue_ops(ctx);
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
            // #1573: the cap is shared by two waits; the hand-off says whether this is a give-up
            // at all. While the command the enter window's Enter started is running, or while the
            // precmd that followed it is younger than the cap, nothing is overdue: the cap is
            // armed again and nothing is written. Only the events (the line editor's signal, the
            // command becoming interactive, a continuation prompt, a signal key) release the keys.
            let decision = self
                .handoff
                .read()
                .prompt_hold_cap_fired(key, instant::Instant::now());
            if let PromptHoldCap::NotYet(after) = decision {
                warpui::input_order::log_hop("prompt_hold_cap_not_due", || {
                    format!("hold={key} rearm_ms={}", after.as_millis())
                });
                self.rearm_prompt_hold_cap(token, after, ctx);
                return;
            }
            // #1584: the cap is a give-up, and a timer never writes to the pty. The line editor
            // never signalled (#1349), or the window's Enter was never acted on:
            // * keys with no Enter, at a prompt whose line holds nothing typed, go back to the
            //   editor as shell-routed text (visible, logged with counts), and the editor takes
            //   input over -- with an Enter too when the shell has never signalled;
            // * a segment with an Enter (or one that cannot go back in order) stays held: the cap
            //   is armed again, a toast says input is waiting for the shell, and the events
            //   release it (the signal, the precmd's wait, a command becoming interactive, a
            //   continuation prompt, ^C or another signal key).
            // The queue's own give-up would deliver them through the stages; the hold has its own
            // ends instead.
            if decision == PromptHoldCap::Gone {
                self.hand_back_prompt_hold_keys(key, token, true, ctx);
                return;
            }
            // A shell that has never signalled will not signal for this prompt either: a segment
            // with an Enter kept for it would be kept forever, so at a clean prompt it goes back
            // too (the Enter as a newline in the editor, never run). Only an enter window's
            // segment (the shell's state unknown) stays held for its events.
            let (clean_prompt, never_signalled) = {
                let handoff = self.handoff.read();
                (
                    handoff.prompt_hold_can_hand_back(key),
                    handoff.prompt_hold_shell_never_signalled(key),
                )
            };
            let can_hand_back =
                clean_prompt && (never_signalled || !self.prompt_hold_segment_has_enter(key));
            if can_hand_back {
                self.hand_back_prompt_hold_keys(key, token, false, ctx);
            } else {
                self.keep_overdue_prompt_hold(key, token, ctx);
            }
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
        // #1577: a cap is a give-up, and a timer never writes user bytes to a pty. For every gate
        // kind below, what waited behind the gate goes back to the editor as visible text (the
        // Enter a newline, the next Enter consumed once with a toast) instead of being delivered
        // through the stages, which wrote it to the pty.
        let gate_key = queue
            .try_peek()
            .and_then(|queue| {
                queue
                    .gates()
                    .iter()
                    .find(|gate| gate.token == token)
                    .map(|gate| gate.key)
            })
            .unwrap_or_default();
        match kind {
            Some(kind @ (GateKind::EnterVerdict | GateKind::TabCompletion)) => {
                // The Input owns these gates: its own hand-back also voids the held Enter.
                let handled = self.input.update(ctx, |input, ctx| {
                    input.give_up_enter_gate_by_key(kind, gate_key, "leak_net", ctx)
                });
                if !handled {
                    self.hand_back_gate_keys(kind, gate_key, "leak_net", ctx);
                }
                return;
            }
            Some(GateKind::PasteReading) => {
                // #1353 phase 6 (INP-A4): the paste gave up on its clipboard read, so the reader's
                // cancel flag is signalled and the blocked-reader count / log reflect it.
                warpui::clipboard::abandon_pending_reads();
                self.hand_back_gate_keys(GateKind::PasteReading, gate_key, "paste_cap", ctx);
                return;
            }
            Some(GateKind::CliSubmit) => {
                // The paced submit's own gate: its pieces are protocol writes and are not held
                // here; the keys typed behind it come back.
                self.hand_back_gate_keys(GateKind::CliSubmit, gate_key, "cli_submit_cap", ctx);
                return;
            }
            Some(GateKind::PowershellSetup) => {
                // Nothing closed the PowerShell gate: the setup is treated as over and its keys
                // go back to the editor.
                log::warn!(
                    "input queue: PowerShell setup gate never closed; handing the held keys back"
                );
                self.give_up_setup_gates_to_editor("powershell_setup_cap", ctx);
                return;
            }
            _ => {}
        }
        if kind == Some(GateKind::SubshellBootstrapWrite)
            && self.subshell_bootstrap_in_flight.is_some()
        {
            // The cap passed while the paced lines are still being written: releasing the keys
            // now would land them among the setup lines (#1399). The gate stays; its event (the
            // last line written, or the bootstrap abandoned) delivers them in order.
            log::warn!(
                "input queue: bootstrap-write gate cap passed with lines still to write; \
                 the held keys wait for the last line"
            );
            // Re-armed for what is left (#1427): the same token stays valid while the gate is open.
            let lines = self
                .subshell_bootstrap_in_flight
                .as_ref()
                .map_or(1, |flight| flight.remaining_lines);
            let after = super::subshell_bootstrap_gate_cap(lines);
            ctx.spawn(Timer::after(after), move |me, _, ctx| {
                me.on_paste_queue_give_up(token, ctx)
            });
            return;
        }
        if kind == Some(GateKind::SubshellSetup) {
            // Setup is treated as over (what the legacy hold-limit timer does), before the held
            // keys are handed back, so none of them is held a second time.
            self.give_up_setup_gates_to_editor("subshell_setup_cap", ctx);
            return;
        }
        if kind == Some(GateKind::SubshellBootstrapWrite) {
            // The paced lines are out: the aborts typed meanwhile go first (protocol), then the
            // keys typed behind the write come back to the editor.
            self.send_pending_subshell_aborts(ctx);
            self.hand_back_gate_keys(
                GateKind::SubshellBootstrapWrite,
                gate_key,
                "bootstrap_write_cap",
                ctx,
            );
            return;
        }
        {
            let Some(mut guard) = queue.try_lock() else {
                queue.defer(Deferred::GiveUpCap(token));
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.give_up(&mut host, token);
        }
    }

    /// #1577: a setup hold (PowerShell or subshell) gave up on a timer: setup is treated as over
    /// and the keys held behind its gates come back to the editor as text, never written.
    pub(super) fn give_up_setup_gates_to_editor(
        &mut self,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.powershell_setup_hold = super::PowerShellSetupHold::Off;
        self.subshell_bootstrap_announced = None;
        self.dismiss_subshell_setup_waiting_toast(ctx);
        for kind in [GateKind::PowershellSetup, GateKind::SubshellSetup] {
            let key = self.paste_queue.queue.try_peek().and_then(|queue| {
                queue
                    .gates()
                    .iter()
                    .find(|gate| gate.kind == kind)
                    .map(|gate| gate.key)
            });
            if let Some(key) = key {
                self.hand_back_gate_keys(kind, key, why, ctx);
            }
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
            Some(queue) if queue.has_gate_kind(GateKind::HandoffReport) => legacy::HANDOFF,
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
                        replacement: None,
                    });
                }
            }
            return;
        }
        self.flush_deferred_queue_ops(ctx);
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            // The queue is held by something that is not delivering this key (a peek, an
            // operation above us): the key is recorded to go behind its gate, in order, when the
            // queue is free. Never written ahead of the line editor (lost on ConPTY, #1162).
            queue.defer(Deferred::OfferGated {
                kind: EntryKind::PtyBytes {
                    bytes,
                    takes_over_agent_control,
                },
                spec,
            });
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        let kind = EntryKind::PtyBytes {
            bytes,
            takes_over_agent_control,
        };
        if guard.has_gate(spec.kind, spec.key) {
            // The gate is open already (a key the queue did not take reached the hand-off):
            // the key joins what it holds, in arrival order, instead of opening a second gate.
            guard.offer(&mut host, kind);
        } else {
            guard.offer_gated(&mut host, kind, spec);
        }
    }

    /// A signal or EOF is about to be written while keys are held for the prompt in the queue.
    /// For a lone EOF (^D) the held keys come back in front of `bytes`, one write in typed order:
    /// the tty keeps typeahead across EOF. For an interrupt (^C, ^Z, ^\\) the held pty keys are
    /// VOID (#1584, the table's `VoidHeldThenSignal`): a real tty flushes its typeahead on them, and
    /// prepending them made `ls\r` + ^C run `ls`. They are dropped with a counted log; editor entries
    /// in the segment still reach the editor. Anything else passes through untouched.
    pub(super) fn prepend_queued_prompt_hold_to_signal(
        &mut self,
        bytes: Cow<'static, [u8]>,
        ctx: &mut ViewContext<Self>,
    ) -> Cow<'static, [u8]> {
        if !bytes.iter().any(super::is_signal_byte) {
            return bytes;
        }
        let Some(next_id) = self
            .paste_queue
            .queue
            .try_peek()
            .filter(|queue| queue.has_gate_kind(GateKind::LineEditorActive))
            .map(|queue| queue.peek_next_id())
        else {
            return bytes;
        };
        let voids = bytes.iter().any(|byte| matches!(byte, 0x03 | 0x1a | 0x1c));
        let hold_id = {
            self.handoff
                .write()
                .take_queued_prompt_hold_for_signal(&bytes)
        };
        let Some(hold_id) = hold_id else {
            return bytes;
        };
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                // The hold was already taken from the hand-off: its keys are released (EOF) or
                // voided (an interrupt) by the next operation instead of waiting for the cap.
                if voids {
                    queue.defer(Deferred::VoidPromptHold {
                        hold_id,
                        before: next_id,
                    });
                } else {
                    queue.defer(Deferred::ReleasePromptHold {
                        hold_id,
                        gave_up: false,
                    });
                }
                return bytes;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::LineEditorActive, hold_id)
        };
        if voids {
            self.void_prompt_hold_entries(hold_id, entries, None, ctx);
            return bytes;
        }
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
            self.handoff
                .write()
                .take_queued_young_hold_for_signal(&bytes)
        };
        let Some((hold_id, before_eof)) = ended else {
            return bytes;
        };
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                // Mid-drain: nothing else can end the hold, so record it for the next attempt.
                self.handoff
                    .write()
                    .restore_young_hold_end(hold_id, YoungHoldEnd::Dropped);
                queue.defer(Deferred::ApplyYoungHoldEnds);
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
        let ended = { self.handoff.write().check_queued_young_hold() };
        if ended {
            self.drain_young_hold_ends(ctx);
        }
    }

    /// Applies, to the gate of the queue, every end the hand-off recorded for a young Enter's
    /// hold (a command written, a report, a signal, a state that outgrew it): the entries are
    /// dropped, written behind the command, or split at the Enter. Called from the few places a
    /// hold can end, never per key.
    pub(super) fn drain_young_hold_ends(&mut self, ctx: &mut ViewContext<Self>) {
        let ends = self.handoff.write().take_young_hold_ends();
        for (hold_id, end) in ends {
            self.apply_young_hold_end(hold_id, end, ctx);
        }
    }

    /// Applies one recorded end to the gate named `hold_id`: a young Enter's `PromptSeen` gate, or
    /// the `HandoffReport` gate of the keys held behind a deferred Enter (one id counter names
    /// both).
    fn apply_young_hold_end(
        &mut self,
        hold_id: u64,
        end: YoungHoldEnd,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.paste_queue.queue.clone();
        let (kind, entries) = {
            let Some(mut guard) = queue.try_lock() else {
                // Mid-drain: keep the end for the next attempt (the gate's give-up finds it too).
                self.handoff.write().restore_young_hold_end(hold_id, end);
                log::warn!("input queue: young hold {hold_id} end deferred, the queue was busy");
                queue.defer(Deferred::ApplyYoungHoldEnds);
                return;
            };
            if let YoungHoldEnd::SubmitOnReport { deferred_gate } = end {
                // The report ran the held Enter in the editor: the hold's gate becomes the deferred
                // Enter's gate in place (the Enter's first byte leaves it), so the keys behind it
                // are never without a gate. A hold whose keys never reached the queue just opens
                // the deferred Enter's gate.
                let mut host = PtyQueueHost::new(self, ctx);
                let spec = handoff_report_spec(deferred_gate);
                if !guard.retag_gate(&mut host, GateKind::PromptSeen, hold_id, spec, true) {
                    guard.open_gate(&mut host, spec);
                }
                return;
            }
            let kind = if guard.has_gate(GateKind::PromptSeen, hold_id) {
                GateKind::PromptSeen
            } else if guard.has_gate(GateKind::HandoffReport, hold_id) {
                GateKind::HandoffReport
            } else {
                // Never queued or already taken.
                if end == YoungHoldEnd::FollowCommand {
                    drop(guard);
                    self.handoff.write().count_followed_young_hold(hold_id, &[]);
                }
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            (kind, guard.close_gate_taking(&mut host, kind, hold_id))
        };
        let chunks = if kind == GateKind::HandoffReport && end == YoungHoldEnd::FollowCommand {
            self.pty_bytes_of_taken_entries(entries, ctx)
        } else {
            self.bytes_of_taken_entries(entries, ctx)
        };
        let held_bytes: usize = chunks.iter().map(Vec::len).sum();
        match end {
            YoungHoldEnd::Dropped => {
                trace::release(trace::legacy::HANDOFF, 0, held_bytes, "dropped");
            }
            YoungHoldEnd::FollowCommand => {
                // The command is written (or queued ahead of these): the keys typed behind the
                // Enter follow it, counted as the lines they are.
                let bytes = chunks.concat();
                self.handoff
                    .write()
                    .count_followed_young_hold(hold_id, &bytes);
                trace::release(trace::legacy::HANDOFF, 1, held_bytes, "pty");
                if !bytes.is_empty() {
                    ctx.emit(Event::WriteHandoffUserBytesToPty {
                        bytes: Cow::Owned(bytes),
                    });
                }
                ctx.emit(Event::RetryTypeaheadHandoffRequest);
            }
            // Handled above, in place.
            YoungHoldEnd::SubmitOnReport { .. } => {}
        }
    }

    /// Opens the `HandoffReport` gate for the Enter the hand-off just deferred (#1016): every key
    /// typed from now on, in the editor or on the terminal, is an entry behind it, so arrival
    /// order is the order and nothing has to be recovered from the editor's buffer. A gate that is
    /// open already (the report turned a young hold's gate into this one) is left alone.
    pub(super) fn open_deferred_enter_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(id) = self.handoff.read().deferred_gate() else {
            return;
        };
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            queue_defer_open_gate(&queue, handoff_report_spec(id));
            return;
        };
        if guard.has_gate(GateKind::HandoffReport, id) {
            return;
        }
        let mut host = PtyQueueHost::new(self, ctx);
        guard.open_gate(&mut host, handoff_report_spec(id));
    }

    /// A key reached the hand-off while an Enter is deferred and was not queued behind its gate:
    /// it goes behind the gate now (the gate is opened with it if it is not open).
    pub(super) fn hold_deferred_key_in_queue(
        &mut self,
        gate_id: u64,
        bytes: Vec<u8>,
        takes_over_agent_control: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        self.hold_keys_behind_gate(
            handoff_report_spec(gate_id),
            bytes,
            takes_over_agent_control,
            ctx,
        );
    }

    /// Closes the `HandoffReport` gate `gate_id` and hands the keys held behind it, as bytes in
    /// typed order, to `then` (the deferred Enter ran and wrote no command, or its report never
    /// came). Nothing is delivered by the take itself. When the queue is busy (mid-drain) or
    /// earlier operations wait in the mailbox, the take AND `then` are one deferred operation
    /// (#1446): they run, in order, when the queue is free.
    pub(super) fn take_deferred_gate_keys_then(
        &mut self,
        gate_id: u64,
        then: GateKeysThen,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.paste_queue.queue.clone();
        if !queue.is_draining() {
            // Earlier operations that waited for the queue go first (the keys they carry are
            // behind this gate).
            self.flush_deferred_queue_ops(ctx);
        }
        if queue.has_deferred() || queue.is_draining() {
            queue.defer(Deferred::TakeGateKeys { gate_id, then });
            return;
        }
        match self.take_deferred_gate_bytes_now(gate_id, ctx) {
            Some(held) => self.finish_taken_gate_keys(held, then, ctx),
            None => queue.defer(Deferred::TakeGateKeys { gate_id, then }),
        }
    }

    /// The continuation of [`Self::take_deferred_gate_keys_then`].
    fn finish_taken_gate_keys(
        &mut self,
        held: Vec<u8>,
        then: GateKeysThen,
        ctx: &mut ViewContext<Self>,
    ) {
        match then {
            GateKeysThen::Release => self.release_held_keys(held, ctx),
            GateKeysThen::GiveUpToEditor => self.give_up_deferred_enter_keys_to_editor(held, ctx),
            GateKeysThen::GiveUpHandBack(why) => {
                self.hand_back_deferred_enter_keys_to_editor(held, why, ctx)
            }
        }
    }

    /// The take itself; `None` when the queue could not be locked (nothing was changed).
    fn take_deferred_gate_bytes_now(
        &mut self,
        gate_id: u64,
        ctx: &mut ViewContext<Self>,
    ) -> Option<Vec<u8>> {
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let mut guard = queue.try_lock()?;
            let mut host = PtyQueueHost::new(self, ctx);
            let mut entries = Vec::new();
            // A key held mid-delivery may have opened a second gate under the same id.
            while guard.has_gate(GateKind::HandoffReport, gate_id) {
                entries.extend(guard.close_gate_taking(
                    &mut host,
                    GateKind::HandoffReport,
                    gate_id,
                ));
            }
            entries
        };
        Some(self.pty_bytes_of_taken_entries(entries, ctx).concat())
    }

    /// As [`Self::bytes_of_taken_entries`], for keys that go to the pty as typed or come back as
    /// editor text: an editor key that is plain typing (text, Enter, Backspace) joins the bytes in
    /// typed order; one that is not (an arrow, a selection) acts on the editor.
    fn pty_bytes_of_taken_entries(
        &mut self,
        entries: Vec<InputEntry>,
        ctx: &mut ViewContext<Self>,
    ) -> Vec<Vec<u8>> {
        let mut chunks = Vec::with_capacity(entries.len());
        for entry in entries {
            match entry.kind {
                EntryKind::Text(text) => chunks.push(text.into_bytes()),
                EntryKind::Enter => chunks.push(b"\r".to_vec()),
                EntryKind::Signal(kind) => chunks.push(signal_bytes(kind).to_vec()),
                EntryKind::EditorKey(payload) => match payload.take() {
                    Some(HeldPasteKey::Text(text)) => chunks.push(text.into_bytes()),
                    Some(HeldPasteKey::Action(EditorAction::Enter)) => chunks.push(b"\r".to_vec()),
                    Some(HeldPasteKey::Action(EditorAction::Backspace)) => chunks.push(vec![0x7f]),
                    Some(key) => {
                        self.input.update(ctx, |input, ctx| {
                            input
                                .editor()
                                .update(ctx, |editor, ctx| editor.apply_queued_key(key, ctx))
                        });
                    }
                    None => {}
                },
                kind => {
                    let rest = InputEntry { kind, ..entry };
                    chunks.extend(self.bytes_of_taken_entries(vec![rest], ctx));
                }
            }
        }
        chunks
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
        let expiry = self.handoff.write().expire_queued_young_hold(hold_id);
        match expiry {
            YoungHoldExpiry::Keep => {}
            YoungHoldExpiry::Gone => {
                self.drain_young_hold_ends(ctx);
                if self.young_hold_gate_has_key(hold_id) {
                    // The gate outlived a hold with no recorded end: never lose what was typed.
                    log::warn!(
                        "input queue: young hold {hold_id} is gone but its gate is open; handing \
                         its keys back to the editor"
                    );
                    self.hand_back_young_hold_keys(hold_id, "young_hold_gone", ctx);
                }
            }
            // #1577: the cap is a give-up, and a timer never writes user bytes to the pty: the
            // held Enter and the keys behind it come back to the editor as text (the Enter a
            // newline, the next Enter consumed once with a toast). The event-driven ends
            // (`drain_young_hold_ends`: a command, the report) still write them as typeahead.
            YoungHoldExpiry::Release => {
                self.hand_back_young_hold_keys(hold_id, "young_hold_cap", ctx);
            }
        }
    }

    /// The give-up of young hold `hold_id` as a hand-back (#1577), then the hand-off's own
    /// bookkeeping: the hold ended without a write.
    fn hand_back_young_hold_keys(
        &mut self,
        hold_id: u64,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.handoff.write().finish_young_hold_handed_back(hold_id);
        self.hand_back_gate_keys(GateKind::PromptSeen, hold_id, why, ctx);
        self.input
            .update(ctx, |input, _| input.clear_handoff_enter_hold());
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
                queue.defer(Deferred::ReleaseYoungHoldKeys { hold_id, routed });
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::PromptSeen, hold_id)
        };
        let chunks = self.bytes_of_taken_entries(entries, ctx);
        let writes = {
            let mut handoff = self.handoff.write();
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
            .update(ctx, |input, _| input.clear_handoff_enter_hold());
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
            if let Some(bytes) = self.take_entry_bytes(entry, ctx) {
                chunks.push(bytes);
            }
        }
        chunks
    }

    /// One entry taken out of a hand-off gate: an entry of the editor side that ended up in this
    /// gate's segment (the queue is shared with the editor) is delivered to the editor now, never
    /// lost, and gives `None`; a pty entry gives its bytes.
    fn take_entry_bytes(
        &mut self,
        entry: InputEntry,
        ctx: &mut ViewContext<Self>,
    ) -> Option<Vec<u8>> {
        if !is_pty_entry(&entry.kind) {
            // The gate this entry waited behind is gone, so it must not wait for anything else
            // either: `flushing_now` applies a key that reads the display map at once instead of
            // answering `Requeue` for a gate that does not exist (#1416).
            let outcome = self.deliver_editor_entry(&entry, &mut FollowUps::flushing_now(), ctx);
            if outcome != DeliverOutcome::Delivered {
                log::warn!("input queue: an editor entry taken out of a gate answered {outcome:?}");
            }
            return None;
        }
        match entry.kind {
            EntryKind::PtyBytes { bytes, .. } => Some(bytes),
            EntryKind::Paste { paste_id, .. } => {
                let bytes = self.paste_queue.payloads.remove(&paste_id);
                if bytes.is_none() {
                    log::warn!("input queue: held paste {paste_id} had no bytes");
                }
                bytes
            }
            other => {
                log::warn!("input queue: a prompt hold held an entry it cannot release: {other:?}");
                None
            }
        }
    }

    /// #1584: the entries of prompt hold `hold_id`, taken because an interrupt voided it. The pty
    /// keys held before the signal (all of them, or the ids below `before` when the void ran
    /// deferred) are dropped, as the tty flushes its typeahead on the interrupt, with a counted
    /// log; editor entries are delivered in order; pty keys that arrived after the signal go out,
    /// in order, counted by the hand-off (the hold is over, they are live keys).
    fn void_prompt_hold_entries(
        &mut self,
        hold_id: u64,
        entries: Vec<InputEntry>,
        before: Option<crate::terminal::input_queue::EntryId>,
        ctx: &mut ViewContext<Self>,
    ) {
        let mut dropped_chunks = 0usize;
        let mut dropped_bytes = 0usize;
        let mut after: Vec<Vec<u8>> = Vec::new();
        for entry in entries {
            let held_before_signal = before.is_none_or(|before| entry.id < before);
            if let Some(bytes) = self.take_entry_bytes(entry, ctx) {
                if held_before_signal {
                    dropped_chunks += 1;
                    dropped_bytes += bytes.len();
                } else {
                    after.push(bytes);
                }
            }
        }
        log::info!(
            "typeahead prompt hold {hold_id}: an interrupt voided {dropped_chunks} held chunk(s), \
             {dropped_bytes} byte(s) (the tty flushes typeahead on it); none reached the pty"
        );
        warpui::input_order::log_hop("prompt_hold_voided", || {
            format!(
                "hold={hold_id} chunks={dropped_chunks} bytes={dropped_bytes} after={}",
                after.len()
            )
        });
        let mut absorbed: Vec<(u64, Vec<u8>)> = Vec::new();
        let mut writes: Vec<Vec<u8>> = Vec::new();
        {
            let mut handoff = self.handoff.write();
            handoff.finish_queued_prompt_hold(hold_id);
            for chunk in after {
                if handoff.route_released_chunk(&chunk) {
                    writes.push(chunk);
                } else if let Some(gate) = handoff.take_deferred_key_started() {
                    // An Enter was deferred meanwhile: the key is typed behind it.
                    absorbed.push((gate, chunk));
                }
            }
        }
        for (gate, chunk) in absorbed {
            self.hold_deferred_key_in_queue(gate, chunk, true, ctx);
        }
        for bytes in writes {
            ctx.emit(Event::WriteHandoffUserBytesToPty {
                bytes: Cow::Owned(bytes),
            });
        }
    }

    /// #1584: whether the keys held behind prompt hold gate `hold_id` include an Enter (a held
    /// paste whose payload is not known counts as one). A busy queue answers yes: the caller then
    /// keeps the hold, which is always safe.
    fn prompt_hold_segment_has_enter(&self, hold_id: u64) -> bool {
        let queue = self.paste_queue.queue.clone();
        let Some(peek) = queue.try_peek() else {
            return true;
        };
        let Some(barrier) = peek
            .gates()
            .iter()
            .find(|gate| gate.kind == GateKind::LineEditorActive && gate.key == hold_id)
            .map(|gate| gate.barrier)
        else {
            return false;
        };
        let upper = peek
            .gates()
            .iter()
            .map(|gate| gate.barrier)
            .filter(|other| *other > barrier)
            .min();
        let is_enter = |byte: &u8| matches!(byte, b'\r' | b'\n');
        peek.entries()
            .filter(|entry| entry.id >= barrier && upper.is_none_or(|upper| entry.id < upper))
            .any(|entry| match &entry.kind {
                EntryKind::PtyBytes { bytes, .. } => bytes.iter().any(is_enter),
                EntryKind::Paste { paste_id, .. } => self
                    .paste_queue
                    .payloads
                    .get(paste_id)
                    .is_none_or(|payload| payload.iter().any(is_enter)),
                _ => false,
            })
    }

    /// #1584: the give-up of prompt hold `hold_id` with keys that may go back to the editor: the
    /// gate is closed, the hand-off finishes the hold and hands input to the editor, and the keys
    /// become editor text, routed to the shell on the next Enter. Nothing is written to the pty.
    /// `gone`: the hand-off no longer knows the hold (a stale gate); its keys go to the editor as
    /// text, newlines included, and nothing is submitted.
    fn hand_back_prompt_hold_keys(
        &mut self,
        hold_id: u64,
        token: GateToken,
        gone: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        self.hand_back_gate_keys_inner(
            GateKind::LineEditorActive,
            hold_id,
            Some(token),
            gone,
            "prompt_hold_cap",
            ctx,
        );
    }

    /// #1577 (GROUP 2): a timer gave up on gate `kind`/`key`. The gate is closed and what waited
    /// behind it comes back to the editor as visible text (the Enter a newline), the next Enter is
    /// consumed once with the "check the line" toast (the ssh hand-back's guard), and the editor
    /// takes input over. Nothing is written to the pty and nothing is run. Logged with counts only.
    pub(super) fn hand_back_gate_keys(
        &mut self,
        kind: GateKind,
        key: u64,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.hand_back_gate_keys_inner(kind, key, None, false, why, ctx);
    }

    fn hand_back_gate_keys_inner(
        &mut self,
        kind: GateKind,
        key: u64,
        retry_token: Option<GateToken>,
        gone: bool,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let hold_id = key;
        let prompt_hold = kind == GateKind::LineEditorActive;
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let Some(mut guard) = queue.try_lock() else {
                // Busy: decided again shortly, never written meanwhile.
                match retry_token {
                    Some(token) => {
                        self.rearm_prompt_hold_cap(token, Duration::from_millis(100), ctx)
                    }
                    // Busy: asked again shortly (a hand-back, never a write).
                    None => {
                        ctx.spawn(
                            Timer::after(Duration::from_millis(100)),
                            move |me, _, ctx| me.hand_back_gate_keys(kind, key, why, ctx),
                        );
                    }
                }
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, kind, key)
        };
        let committed = if prompt_hold {
            let mut handoff = self.handoff.write();
            handoff.expire_queued_prompt_hold(hold_id);
            handoff.finish_queued_prompt_hold(hold_id);
            !gone && handoff.try_commit_at_prompt()
        } else {
            false
        };
        if committed {
            self.handle_typeahead_handoff_changed(ctx);
        }
        // In typed order across both sides of the queue: pty keys accumulate as one run of text;
        // an editor entry first puts the run before it into the editor, then is delivered itself.
        let mut run: Vec<u8> = Vec::new();
        let (mut chunks, mut bytes, mut chars) = (0usize, 0usize, 0usize);
        let mut inserted = false;
        for entry in entries {
            if matches!(entry.kind, EntryKind::Paste { payload: None, .. }) {
                // A paste whose read never finished: it is discarded with its toast (as the
                // queue's own give-up does); what was typed behind it comes back below.
                let mut host = PtyQueueHost::new(self, ctx);
                host.discarded(&entry, DiscardReason::PasteNotReady);
                continue;
            }
            if !is_pty_entry(&entry.kind) {
                self.insert_handed_back_run(&mut run, &mut chars, &mut inserted, ctx);
            }
            if let Some(chunk) = self.take_entry_bytes(entry, ctx) {
                chunks += 1;
                bytes += chunk.len();
                run.extend_from_slice(&chunk);
            }
        }
        self.insert_handed_back_run(&mut run, &mut chars, &mut inserted, ctx);
        if inserted {
            // The text was typed for the shell: the next Enter runs it as a shell command, for a
            // stale gate (`gone`) too.
            self.input
                .update(ctx, |input, _| input.submit_next_enter_as_shell_command());
            if !prompt_hold {
                // A timer handed several keys back as one buffer: the first Enter on it is held
                // with the "check the line" toast, never run (#1577).
                self.input.update(ctx, |input, ctx| {
                    let buffer = input.editor().as_ref(ctx).buffer_text(ctx);
                    input.arm_ssh_handover_enter_guard(buffer);
                });
            }
        }
        log::warn!(
            "input queue: gate {kind:?} {hold_id} gave up ({why}): {chunks} chunk(s), {bytes} \
             byte(s) handed back to the editor as text (gone={gone}, committed={committed}); \
             nothing written to the pty"
        );
        warpui::input_order::log_hop("prompt_hold_handed_back", || {
            format!(
                "hold={hold_id} kind={kind:?} why={why} chunks={chunks} bytes={bytes} \
                 chars={chars} gone={gone} committed={committed}"
            )
        });
        if !prompt_hold && (inserted || chunks > 0) {
            let toast = match why {
                "stall_ceiling" => "Shell setup stalled; what you typed is in the input",
                _ => GATE_GIVE_UP_TOAST,
            };
            self.show_error_toast(toast.to_string(), ctx);
        }
        self.note_editor_took_over();
        self.redetermine_terminal_focus(ctx);
        ctx.emit(Event::RetryTypeaheadHandoffRequest);
    }

    /// Puts the run of pty keys handed back so far into the editor as text (#1584), and empties it.
    fn insert_handed_back_run(
        &mut self,
        run: &mut Vec<u8>,
        chars: &mut usize,
        inserted: &mut bool,
        ctx: &mut ViewContext<Self>,
    ) {
        if run.is_empty() {
            return;
        }
        let text = text_for_editor(run);
        run.clear();
        if !forces_shell_submit(&text) {
            return;
        }
        *chars += text.chars().count();
        *inserted = true;
        self.input
            .update(ctx, |input, ctx| input.system_insert(&text, ctx));
    }

    /// #1584: the give-up of prompt hold `hold_id` whose keys must not go back to the editor (they
    /// hold an Enter, or the shell's state is unknown): nothing is written and nothing is run on a
    /// timer. The gate stays, the cap is armed again, and the first time a log line and a toast say
    /// input is waiting for the shell. The events release the keys in order.
    fn keep_overdue_prompt_hold(
        &mut self,
        hold_id: u64,
        token: GateToken,
        ctx: &mut ViewContext<Self>,
    ) {
        let live = self.handoff.write().keep_overdue_prompt_hold(hold_id);
        if live && self.paste_queue.prompt_hold_waiting_told != Some(hold_id) {
            self.paste_queue.prompt_hold_waiting_told = Some(hold_id);
            let held = self.paste_queue.queue.try_peek().map_or(0, |queue| {
                queue.held_behind_gate(GateKind::LineEditorActive, hold_id)
            });
            log::warn!(
                "typeahead prompt hold {hold_id}: no line-editor ready event within the cap; \
                 {held} held entr(ies) wait for the shell (an Enter among them is never sent by a \
                 timer)"
            );
            warpui::input_order::log_hop("prompt_hold_kept_overdue", || {
                format!("hold={hold_id} entries={held}")
            });
            if ctx.has_singleton_model::<crate::workspace::ToastStack>() {
                self.show_ssh_typing_toast(
                    "Your typed input is waiting for the shell to be ready; it is sent when the \
                     prompt is ready (Ctrl-C discards it)",
                    ctx,
                );
            }
        }
        self.rearm_prompt_hold_cap(token, CONPTY_PROMPT_HOLD_GIVE_UP, ctx);
    }

    /// Arms the give-up cap of prompt hold gate `token` again (#1573): the hand-off found the wait
    /// not overdue yet. Token-guarded like the first arming: a gate that closed meanwhile finds
    /// nothing to do. Writes nothing.
    fn rearm_prompt_hold_cap(
        &mut self,
        token: GateToken,
        after: Duration,
        ctx: &mut ViewContext<Self>,
    ) {
        ctx.spawn(Timer::after(after), move |me, _, ctx| {
            me.on_paste_queue_give_up(token, ctx)
        });
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
                // Mid-drain: released by the next operation, in order (#1417). The gate's cap is
                // only the net behind this.
                log::warn!(
                    "input queue: prompt hold {hold_id} release deferred, the queue was busy"
                );
                queue.defer(Deferred::ReleasePromptHold {
                    hold_id,
                    gave_up: how == PromptHoldEnd::GaveUp,
                });
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
        let mut absorbed: Vec<(u64, Vec<u8>)> = Vec::new();
        let writes = {
            let mut handoff = self.handoff.write();
            if how == PromptHoldEnd::GaveUp {
                handoff.expire_queued_prompt_hold(hold_id);
            }
            let mut writes = Vec::with_capacity(chunks.len());
            for chunk in chunks {
                if handoff.route_released_chunk(&chunk) {
                    writes.push(chunk);
                } else if let Some(gate) = handoff.take_deferred_key_started() {
                    // An Enter was deferred meanwhile: the key is typed behind it.
                    absorbed.push((gate, chunk));
                }
            }
            handoff.finish_queued_prompt_hold(hold_id);
            writes
        };
        for (gate, chunk) in absorbed {
            self.hold_deferred_key_in_queue(gate, chunk, true, ctx);
        }
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
        if let Some(held) = queue.try_peek()
            && held.has_gate(GateKind::PasteReading, id)
        {
            let (entries, held_bytes) = held.queued_summary();
            // The paste itself is one of the queued entries.
            trace::release(
                trace::legacy::PASTE_PTY,
                entries.saturating_sub(1),
                held_bytes,
                "pty",
            );
        }
        let result: Result<Vec<u8>, ()> = match bytes {
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
        let ok = result.is_ok();
        if !self.complete_queued_paste_read(id, ok, ctx) {
            // Busy: the result (its bytes are stored) is applied by the next operation.
            queue.defer(Deferred::PasteRead { id, ok });
        }
    }

    /// Applies the reader's result for paste `id` to the queue. False (nothing done) if the queue
    /// is busy.
    fn complete_queued_paste_read(
        &mut self,
        id: u64,
        ok: bool,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let queue = self.paste_queue.queue.clone();
        {
            let Some(mut guard) = queue.try_lock() else {
                return false;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.on_paste_read(&mut host, id, if ok { Ok(Vec::new()) } else { Err(()) });
        }
        // A pending command waits for a paste being read (#1435); it goes now.
        self.execute_pending_command((), ctx);
        // A result that arrived after its paste was cancelled or given up has no entry to claim it.
        // A paste that is still queued (its own gate is closed but another gate, such as the
        // subshell bootstrap's, still holds it) keeps its bytes until it is delivered; dropping
        // them here lost the paste. Unreadable queue: assume it is still queued.
        let still_queued = queue.try_peek().is_none_or(|held| {
            held.entries().any(
                |entry| matches!(&entry.kind, EntryKind::Paste { paste_id, .. } if *paste_id == id),
            )
        });
        if !still_queued {
            self.paste_queue.payloads.remove(&id);
            self.paste_queue.refused.remove(&id);
        }
        true
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
                // Held behind the gate by the next operation, in typed order (#1426).
                queue.defer(Deferred::Offer {
                    kind: EntryKind::PtyBytes {
                        bytes,
                        takes_over_agent_control,
                    },
                    origin: Origin::Keyboard,
                    stamp: queue.stamp_now(),
                });
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

    /// Takes the keys held for the PowerShell setup out of the queue, in typed order, as bytes,
    /// and hands them to `then`. The gate closes with them; `then` holds again what it does not
    /// consume. When the queue is busy (mid-drain) or earlier operations wait in the mailbox, the
    /// take AND `then` are one deferred operation (#1446), run in order when the queue is free.
    pub(super) fn take_powershell_held_then(
        &mut self,
        then: PowershellKeysThen,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.paste_queue.queue.clone();
        if !queue.is_draining() {
            // Earlier operations that waited for the queue go first (the keys they carry are
            // behind this gate).
            self.flush_deferred_queue_ops(ctx);
        }
        if queue.has_deferred() || queue.is_draining() {
            queue.defer(Deferred::TakePowershellKeys { then });
            return;
        }
        match self.take_powershell_held_bytes_now(ctx) {
            Some(held) => self.finish_taken_powershell_keys(held, then, ctx),
            None => queue.defer(Deferred::TakePowershellKeys { then }),
        }
    }

    fn finish_taken_powershell_keys(
        &mut self,
        held: Vec<u8>,
        then: PowershellKeysThen,
        ctx: &mut ViewContext<Self>,
    ) {
        match then {
            PowershellKeysThen::Deliver => self.deliver_powershell_held_bytes(held, ctx),
            PowershellKeysThen::CancelHeldLine => {
                let mut held = held;
                held.truncate(held_bytes_through_last_enter(&held));
                if !held.is_empty() {
                    self.hold_powershell_keys(held, true, ctx);
                }
            }
        }
    }

    /// The take itself; `None` when the queue could not be locked (nothing was changed). Empty
    /// when no PowerShell gate is open.
    fn take_powershell_held_bytes_now(&mut self, ctx: &mut ViewContext<Self>) -> Option<Vec<u8>> {
        let queue = self.paste_queue.queue.clone();
        let entries = {
            let mut guard = queue.try_lock()?;
            if !guard.has_gate_kind(GateKind::PowershellSetup) {
                return Some(Vec::new());
            }
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_taking(&mut host, GateKind::PowershellSetup, 0)
        };
        Some(self.bytes_of_taken_entries(entries, ctx).concat())
    }

    /// The PowerShell setup is over: the gate closes (its event) and the queue's one drain writes
    /// what it held, in typed order.
    pub(super) fn close_powershell_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            queue.defer(Deferred::GateEvent {
                kind: GateKind::PowershellSetup,
                key: 0,
            });
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
        let abort = queue_signal_of(bytes).is_some();
        if abort && in_flight.is_some() {
            // The rest of a chunked line-clear write is dropped (#1354, now a writer job #1384).
            ctx.emit(super::Event::CancelPacedSetupWrites);
            self.note_input_for_deferred_subshell_setup(bytes);
            if setup_gate || !only_subshell_gates {
                // An abort that also skips the setup goes FIRST once the last line is out, ahead
                // of what was typed (the old buffer's release order); kept until then.
                self.paste_queue.pending_aborts.push((bytes.to_vec(), true));
                note_shadow_leaf(DestClass::Held);
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
            let spec = GateSpec::new(GateKind::SubshellSetup, 0).with_cap(Some(left));
            match queue.try_lock() {
                Some(mut guard) => {
                    let mut host = PtyQueueHost::new(self, ctx);
                    guard.open_gate(&mut host, spec);
                }
                None => queue_defer_open_gate(&queue, spec),
            }
        }
        if let Some(lines) = in_flight
            && !bootstrap_gate
        {
            let cap = super::subshell_bootstrap_gate_cap(lines);
            let spec = GateSpec::new(GateKind::SubshellBootstrapWrite, 0).with_cap(Some(cap));
            match queue.try_lock() {
                Some(mut guard) => {
                    let mut host = PtyQueueHost::new(self, ctx);
                    guard.open_gate(&mut host, spec);
                }
                None => queue_defer_open_gate(&queue, spec),
            }
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
            queue.defer(Deferred::GateEvent {
                kind: GateKind::SubshellSetup,
                key: 0,
            });
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
            queue.defer(Deferred::GateEvent {
                kind: GateKind::SubshellBootstrapWrite,
                key: 0,
            });
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
            let Some(signal) = queue_signal_of(&bytes) else {
                continue;
            };
            if self.subshell_setup_gate_open() {
                self.subshell_bootstrap_announced = None;
            }
            let queue = self.paste_queue.queue.clone();
            let Some(mut guard) = queue.try_lock() else {
                queue.defer(Deferred::Offer {
                    kind: EntryKind::Signal(signal),
                    origin: Origin::Keyboard,
                    stamp: queue.stamp_now(),
                });
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

/// Records the opening of gate `spec` for the next operation (a gate that is open already, with
/// the same kind and key, is left alone when it is applied).
fn queue_defer_open_gate(queue: &PaneQueue, spec: GateSpec) {
    queue.defer(Deferred::OpenGate(spec));
}

impl TerminalView {
    /// Applies, in the order they were recorded, the operations that found the queue busy
    /// ([`Deferred`], the design note in `input_queue/mod.rs`). A no-op when nothing waits. While
    /// the queue is held (this runs under a drain, or a peek is out) nothing is applied: the
    /// view is woken again, so the mailbox is never stranded.
    pub(super) fn flush_deferred_queue_ops(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        if !queue.has_deferred() {
            return;
        }
        if queue.try_peek().is_none() {
            queue.poke_defer_waker();
            return;
        }
        if !queue.begin_deferred_flush() {
            return;
        }
        while let Some(op) = queue.pop_deferred() {
            if queue.try_peek().is_none() {
                queue.unpop_deferred(op);
                queue.poke_defer_waker();
                break;
            }
            if let Err(op) = self.apply_deferred_op(op, ctx) {
                queue.unpop_deferred(op);
                queue.poke_defer_waker();
                break;
            }
        }
        queue.end_deferred_flush();
    }

    /// Applies one deferred operation; gives it back if the queue could not be locked.
    fn apply_deferred_op(
        &mut self,
        op: Deferred,
        ctx: &mut ViewContext<Self>,
    ) -> Result<(), Deferred> {
        let queue = self.paste_queue.queue.clone();
        match op {
            Deferred::Offer {
                kind,
                origin,
                stamp,
            } => {
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::Offer {
                        kind,
                        origin,
                        stamp,
                    });
                };
                let mut host = PtyQueueHost::new(self, ctx);
                // The stamp from when the key was typed, not the pane's state now.
                guard.offer_stamped(&mut host, kind, origin, stamp);
            }
            Deferred::OfferGated { kind, spec } => {
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::OfferGated { kind, spec });
                };
                let mut host = PtyQueueHost::new(self, ctx);
                if guard.has_gate(spec.kind, spec.key) {
                    // The gate is open already: the key joins what it holds, in arrival order.
                    guard.offer(&mut host, kind);
                } else {
                    guard.offer_gated(&mut host, kind, spec);
                }
            }
            Deferred::OpenGate(spec) => {
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::OpenGate(spec));
                };
                if !guard.has_gate(spec.kind, spec.key) {
                    let mut host = PtyQueueHost::new(self, ctx);
                    guard.open_gate(&mut host, spec);
                }
            }
            Deferred::GateEvent { kind, key } => {
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::GateEvent { kind, key });
                };
                let mut host = PtyQueueHost::new(self, ctx);
                guard.on_gate_event(&mut host, kind, key);
            }
            Deferred::GiveUpCap(token) => self.on_paste_queue_give_up(token, ctx),
            Deferred::PasteRead { id, ok } => {
                if !self.complete_queued_paste_read(id, ok, ctx) {
                    return Err(Deferred::PasteRead { id, ok });
                }
            }
            Deferred::ReleasePromptHold { hold_id, gave_up } => {
                let how = if gave_up {
                    PromptHoldEnd::GaveUp
                } else {
                    PromptHoldEnd::Signal
                };
                self.release_queued_prompt_hold(hold_id, how, ctx);
            }
            Deferred::VoidPromptHold { hold_id, before } => {
                let queue = self.paste_queue.queue.clone();
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::VoidPromptHold { hold_id, before });
                };
                let entries = {
                    let mut host = PtyQueueHost::new(self, ctx);
                    guard.close_gate_taking(&mut host, GateKind::LineEditorActive, hold_id)
                };
                drop(guard);
                self.void_prompt_hold_entries(hold_id, entries, Some(before), ctx);
            }
            Deferred::ReleaseYoungHoldKeys { hold_id, routed } => {
                self.release_young_hold_keys(hold_id, routed, ctx);
            }
            Deferred::ApplyYoungHoldEnds => self.drain_young_hold_ends(ctx),
            Deferred::TakeGateKeys { gate_id, then } => {
                let Some(held) = self.take_deferred_gate_bytes_now(gate_id, ctx) else {
                    return Err(Deferred::TakeGateKeys { gate_id, then });
                };
                self.finish_taken_gate_keys(held, then, ctx);
            }
            Deferred::TakePowershellKeys { then } => {
                let Some(held) = self.take_powershell_held_bytes_now(ctx) else {
                    return Err(Deferred::TakePowershellKeys { then });
                };
                self.finish_taken_powershell_keys(held, then, ctx);
            }
            Deferred::VoidEnterGate { kind, key } => {
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::VoidEnterGate { kind, key });
                };
                let mut host = PtyQueueHost::new(self, ctx);
                let taken = guard.close_gate_taking(&mut host, kind, key);
                drop(guard);
                // #1571 / #1446: everything typed behind the void Enter comes back as text: its
                // Enters as line breaks, the editing keys' effect, pastes (never only the text
                // entries, which lost the rest).
                let text = crate::terminal::input::text_of_entries_typed_after(&taken, |id| {
                    self.paste_queue.routed_pastes.remove(&id);
                    self.paste_queue.ssh_pastes.remove(&id);
                    self.paste_queue
                        .payloads
                        .remove(&id)
                        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                        .or_else(|| {
                            self.input
                                .update(ctx, |input, _| input.take_paste_payload_text(id))
                        })
                });
                if !text.is_empty() {
                    self.input.update(ctx, |input, ctx| {
                        input.system_insert(&text, ctx);
                        if text.contains('\n') {
                            // Several lines in one buffer submit as one command: the first Enter
                            // on them is held with the "check the line" toast.
                            let buffer = input.buffer_text(ctx);
                            input.arm_ssh_handover_enter_guard(buffer);
                        }
                    });
                }
            }
            Deferred::DropSshGate {
                gate,
                paste_ids,
                then,
            } => {
                if queue.try_peek().is_none() {
                    return Err(Deferred::DropSshGate {
                        gate,
                        paste_ids,
                        then,
                    });
                }
                let held = self.ssh_drop_detached(gate, &paste_ids);
                self.finish_dropped_ssh_gate(held, then, ctx);
            }
            Deferred::FlushProgrammaticEdits => {
                self.input.update(ctx, |input, ctx| {
                    input.flush_deferred_programmatic_edits(ctx)
                });
            }
            Deferred::FocusChanged => {
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::FocusChanged);
                };
                let mut host = PtyQueueHost::new(self, ctx);
                guard.on_focus_changed(&mut host);
            }
            Deferred::Poke => {
                let Some(mut guard) = queue.try_lock() else {
                    return Err(Deferred::Poke);
                };
                if guard.pending_foreign() {
                    let mut host = PtyQueueHost::new(self, ctx);
                    guard.resume_after_yield(&mut host);
                }
            }
        }
        Ok(())
    }
}

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
            let kind = EntryKind::Paste {
                paste_id: id,
                payload: None,
            };
            let spec = GateSpec::new(GateKind::PasteReading, id).with_cap(backstop);
            match queue.try_lock() {
                Some(mut guard) => {
                    let mut host = PtyQueueHost::new(self, ctx);
                    guard.offer_gated(&mut host, kind, spec);
                }
                None => queue.defer(Deferred::OfferGated { kind, spec }),
            }
        }
        // Say what is happening when the read takes noticeable time (a big paste). Time is the
        // requirement here (user feedback), not correctness.
        ctx.spawn(
            Timer::after(super::PASTE_PROGRESS_TOAST_DELAY),
            move |me, _, ctx| {
                if me
                    .paste_queue
                    .queue
                    .try_peek()
                    .is_some_and(|queue| queue.has_gate(GateKind::PasteReading, id))
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
