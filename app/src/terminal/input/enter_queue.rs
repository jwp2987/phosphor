//! The Enter verdict / Tab completion / after-Enter ordering on the editor's input queue
//! (#1353, design phase 3).
//!
//! An Enter that waits for its classification verdict, or for the Tab completion in flight, opens
//! an `EnterVerdict` gate on the editor's queue (the instance phase 1 put there; no new one).
//! Everything typed behind that Enter is an entry behind the gate: plain `Text` / `Enter` entries
//! that never enter the buffer, so the buffer-diff recovery of `keys_typed_behind_waiting_enter`
//! has nothing left to subtract. The gate is closed by the events #1351 already delivers (the
//! verdict notification, the completion result, or the compute-clock give-up that submits
//! anyway), in `Input::submit_held_enter`, which runs the Enter and then keeps the gate until the
//! route it took has settled:
//!
//! * the Enter submitted a shell command: the keys behind it are routed (editor or pty) only once
//!   the pty has taken the command. The signal for that is an event the view already receives:
//!   the pty controller's `TypeaheadHandoffChanged` (it fires when the hand-off engages) and the
//!   block-started event. Nothing is polled. The 300 ms the old 5 ms poll gave up after is now
//!   one logged give-up timer, armed once ([`TerminalView::settle_enter_queue`]).
//! * otherwise the view's `ReplayAfterDeferredEnter` handler closes the gate at once.
//!
//! Closing the gate drains it, in order. A drain driven from here stops at the first `Text` /
//! `Enter` entry (the input reaches no pty: `Host::handles`) and the terminal view continues it,
//! delivering each entry to the editor or the pty with its leaf writers, never the pipeline that
//! holds (#1222). The entries are never turned into events, so they stay in the queue in order:
//! a replayed Enter that waits again returns its new gate as the delivery's `OpensGate`
//! ([`Input::take_stashed_gate`]), and everything behind it waits for that gate.
//!
//! Timers: none here. The give-ups are the #1351 compute caps and backstops (they submit the
//! Enter, which closes the gate), the route-settle cap in the view, and a long leak net on the
//! gate itself ([`ENTER_GATE_LEAK_NET`]) so a bug can never swallow keys for good.

use std::time::Duration;

use warpui::{AppContext, ViewContext};

use warpui::SingletonEntity as _;
use warpui::r#async::Timer;

use super::paste_queue::InputQueueHost;
use super::{AfterEnterInput, Event, Input};
use crate::editor::HeldPasteKey;
use crate::terminal::input_queue::{
    EditorKeyPayload, EntryKind, GateKind, GateSpec, HELD_KEY, HELD_PASTE, HELD_SEQ, HELD_TYPED,
    InputEntry, PaneQueue, UNREADY_ENTER_KEY_FLAG,
};
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

/// The longest an Enter gate may stay closed whatever happens: a net for a bug that would leave
/// it open, far beyond every real hold (the hold backstops are 250 / 500 ms, the route-settle
/// cap 300 ms). Logged by the queue when it fires.
pub(super) const ENTER_GATE_LEAK_NET: Duration = Duration::from_secs(10);

/// The Enter gate this input has open on the queue. The gate's key is the wait's id (#1206), so a
/// late event of an earlier wait cannot close a later one.
#[derive(Clone, Copy, Debug)]
pub(super) struct EnterGate {
    pub(super) key: u64,
    /// What the Enter waits for: `EnterVerdict` or `TabCompletion`.
    pub(super) kind: GateKind,
    /// The Enter has run (`submit_held_enter`): the gate now waits for its route to settle.
    pub(super) submitted: bool,
    /// The Enter submitted a shell command, so the keys behind it wait for the pty to take it.
    pub(super) awaits_pty: bool,
    /// The view's route-settle give-up timer is armed for this gate.
    pub(super) route_cap_armed: bool,
    /// #1571: the Enter was refused because the shell could not take a command yet. It is held
    /// as the pending command and this gate keeps everything typed after it in order until the
    /// readiness event runs it ([`Input::release_unready_enter_gate_after_run`]); it is never
    /// released by the Enter's own submit.
    pub(super) held_unready: bool,
}

/// #1571: the give-up net of an Enter held because the shell was not ready. The readiness events
/// (the session's registration, its first Precmd) end the hold long before this; when it fires
/// nothing is run: the held line and what was typed after it go back to the editor as text, with
/// a toast, and it is logged. Long enough for a slow remote bootstrap (the ssh connect's own
/// ceilings are 15 s).
pub(super) const UNREADY_ENTER_GIVE_UP: Duration = Duration::from_secs(15);

const UNREADY_ENTER_GIVE_UP_TOAST: &str = "The shell was not ready, so the line was not run; it is \
     back in the input with what you typed after it";

/// A programmatic edit of the editor's buffer that waits for the Enter in front of it (#1434).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DeferredEdit {
    /// Insert at the cursor ([`Input::system_insert`]).
    Insert(String),
    /// Replace the whole buffer ([`Input::replace_buffer_content`]).
    Replace(String),
    /// Put the text in the buffer and submit it (an accepted inline-history entry).
    ReplaceAndSubmit(String),
    /// [`Input::resubmit_locked_to_shell`] (misroute recovery).
    ResubmitToShell(String),
    /// [`Input::send_text_to_agent`] (misroute recovery).
    SendToAgent(String),
}

impl Input {
    /// #1434: a programmatic edit (a command producer's `set_pending_command`, the local-control
    /// IPC, a slash command) is not a key and has no place in the ordering of keys, but it edits
    /// the buffer the queued keys and the waiting Enter are about:
    ///
    /// * while an Enter waits for its verdict (or a completion), the edit is kept and applied when
    ///   that Enter's gate closes. Applied at once it voided the Enter (`reconcile_verdict_wait`)
    ///   or was taken for keys typed behind it and replayed, as typeahead, to the pty;
    /// * while an editor paste is being read, an insert joins the keys queued behind the paste
    ///   (applied as typed text, in order); a replacement waits for the same Enter-style flush
    ///   since it has no key form, so it is applied right after the paste lands.
    ///
    /// Returns whether the edit was taken (the caller does nothing more). False when nothing is
    /// held, and while the queue is delivering (the edit is part of that delivery).
    pub(super) fn defer_programmatic_edit(
        &mut self,
        edit: DeferredEdit,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        // The waiting Enter's own submit (and what it triggers) is not "behind" it.
        if self.running_deferred_enter {
            return false;
        }
        if self.enter_queue_holds_keys(ctx) {
            log::info!("programmatic edit deferred behind a waiting Enter (#1434)");
            self.deferred_programmatic_edits.push(edit);
            return true;
        }
        let paste_reading = self
            .enter_queue_handle(ctx)
            .try_peek()
            .is_some_and(|queue| queue.has_editor_paste_reading());
        if !paste_reading {
            // Nothing holds the editor. Edits deferred earlier whose gate vanished without a
            // release (the leak net) go first, in order.
            if !self.deferred_programmatic_edits.is_empty() {
                self.flush_deferred_programmatic_edits(ctx);
            }
            return false;
        }
        match edit {
            DeferredEdit::Insert(text) => {
                let queue = self.enter_queue_handle(ctx);
                let Some(mut queue) = queue.try_lock() else {
                    return false;
                };
                log::info!("programmatic insert queued behind an editor paste (#1434)");
                let mut host = InputQueueHost::new(self, ctx);
                queue.offer(
                    &mut host,
                    EntryKind::EditorKey(EditorKeyPayload::new(HeldPasteKey::Text(text))),
                );
                true
            }
            other => {
                self.deferred_programmatic_edits.push(other);
                true
            }
        }
    }

    /// Puts `text` in the buffer and submits it, behind a waiting Enter or a paste being read
    /// (#1434).
    pub(crate) fn accept_history_text_and_submit(
        &mut self,
        text: &str,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.defer_programmatic_edit(DeferredEdit::ReplaceAndSubmit(text.to_owned()), ctx) {
            return;
        }
        self.editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(text, ctx);
        });
        self.input_enter(ctx);
    }

    /// Applies the deferred programmatic edits in order, then retries the pending command that
    /// was waiting behind them (#1434 / #1435).
    pub(crate) fn flush_deferred_programmatic_edits(&mut self, ctx: &mut ViewContext<Self>) {
        for edit in std::mem::take(&mut self.deferred_programmatic_edits) {
            match edit {
                DeferredEdit::Insert(text) => {
                    self.system_insert(&text, ctx);
                }
                DeferredEdit::Replace(text) => self.replace_buffer_content(&text, ctx),
                DeferredEdit::ReplaceAndSubmit(text) => {
                    self.accept_history_text_and_submit(&text, ctx)
                }
                DeferredEdit::ResubmitToShell(text) => self.resubmit_locked_to_shell(&text, ctx),
                DeferredEdit::SendToAgent(text) => self.send_text_to_agent(&text, ctx),
            }
        }
        if self.has_pending_command {
            self.execute_pending_command(ctx);
        }
    }

    /// Whether the pending command must wait: an Enter waits for its verdict, or a paste is being
    /// read for the editor or the pty (#1435).
    pub(super) fn pending_command_must_wait_for_queue(&self, ctx: &AppContext) -> bool {
        self.enter_queue_holds_keys(ctx)
            || self
                .enter_queue_handle(ctx)
                .try_peek()
                .is_some_and(|queue| {
                    queue.has_editor_paste_reading() || queue.has_pty_paste_reading()
                })
    }

    fn enter_queue_handle(&self, ctx: &AppContext) -> PaneQueue {
        self.editor.as_ref(ctx).paste_hold().queue()
    }

    /// Whether the pane queue is mid-drain (an entry it delivers is being handled).
    pub(super) fn enter_queue_handle_is_draining(&self, ctx: &AppContext) -> bool {
        self.enter_queue_handle(ctx).is_draining()
    }

    /// Keys typed now must queue behind an Enter: its gate is open on the queue. False while the
    /// queue is mid-drain (a key applied by a delivery is part of that delivery).
    pub(crate) fn enter_queue_holds_keys(&self, ctx: &AppContext) -> bool {
        self.enter_gate.is_some_and(|gate| {
            self.enter_queue_handle(ctx)
                .try_peek()
                .is_some_and(|queue| queue.has_gate(gate.kind, gate.key))
        })
    }

    /// Whether a stage of the current input-type detection is running, and the classifier compute
    /// it has spent (#1351): what a young line's Enter measures its give-up on.
    pub(crate) fn detection_compute(&self, ctx: &AppContext) -> (bool, Duration) {
        let clock = self.ai_input_model.as_ref(ctx).detection_clock();
        (clock.is_running(), clock.spent())
    }

    /// Sends a unit on `tx` each time a stage of the current detection begins running.
    pub(crate) fn notify_on_detection_stage_start(
        &self,
        tx: async_channel::Sender<()>,
        ctx: &AppContext,
    ) {
        self.ai_input_model
            .as_ref(ctx)
            .detection_clock()
            .notify_on_stage_start(tx);
    }

    /// The Enter has run and its gate is open: the keys behind it wait for the route to settle.
    pub(crate) fn enter_queue_awaits_route(&self, ctx: &AppContext) -> bool {
        self.enter_gate.is_some_and(|gate| gate.submitted) && self.enter_queue_holds_keys(ctx)
    }

    /// The submitted Enter's command must be taken by the pty before the keys behind it route.
    pub(crate) fn enter_queue_awaits_pty(&self) -> bool {
        self.enter_gate
            .is_some_and(|gate| gate.submitted && gate.awaits_pty)
    }

    /// The key of the gate whose route-settle give-up timer the view should arm now: `Some` once
    /// per gate (the timer is one-shot), `None` when it is armed already or there is no gate.
    pub(crate) fn take_enter_route_cap_arm(&mut self) -> Option<u64> {
        let gate = self.enter_gate.as_mut()?;
        if gate.route_cap_armed {
            return None;
        }
        gate.route_cap_armed = true;
        Some(gate.key)
    }

    /// Opens the gate of the wait `id` that just started (an Enter held for its verdict or a Tab
    /// completion): keys arriving from now on queue behind it.
    pub(super) fn open_enter_gate(&mut self, id: u64, kind: GateKind, ctx: &mut ViewContext<Self>) {
        let spec = GateSpec::new(kind, id).with_cap(Some(ENTER_GATE_LEAK_NET));
        self.open_enter_gate_with(spec, ctx);
    }

    /// [`Self::open_enter_gate`] for a spec the caller built (#1571: the unready Enter's gate,
    /// whose give-up is the input's own hand-back timer, not the queue's release).
    fn open_enter_gate_with(&mut self, spec: GateSpec, ctx: &mut ViewContext<Self>) {
        if let Some(existing) = self.enter_gate.filter(|gate| gate.held_unready) {
            // Never expected: every Enter behind a held unready one queues behind its gate. The
            // new gate replaces it in the input's bookkeeping; the old one stays on the queue.
            log::error!(
                "input queue: an Enter gate opened while an unready Enter's gate (key {}) is held \
                 (#1571)",
                existing.key
            );
        }
        let (id, kind) = (spec.key, spec.kind);
        let queue = self.enter_queue_handle(ctx);
        if queue.is_draining() {
            // This Enter is being delivered by the queue's own drain (a replayed Enter that waits
            // again): the drain cannot be re-entered. The delivering host takes the gate right
            // after this call and returns it as the delivery's `OpensGate`, which holds the
            // entries already queued behind the Enter.
            self.stashed_gate = Some(spec);
        } else {
            match queue.try_lock() {
                Some(mut guard) => {
                    let mut host = InputQueueHost::new(self, ctx);
                    guard.open_gate(&mut host, spec);
                }
                // Opened by the next operation, in order (the busy-queue mailbox).
                None => queue.defer(crate::terminal::input_queue::Deferred::OpenGate(spec)),
            }
        }
        self.enter_gate = Some(EnterGate {
            key: id,
            kind,
            submitted: false,
            awaits_pty: false,
            route_cap_armed: false,
            held_unready: false,
        });
    }

    /// #1571: an Enter the shell could not take yet (`hold_refused_unready_enter`) becomes a GATE.
    /// Everything typed after it queues behind it in typed order (text, Enters, editing keys), so
    /// the editor never merges it into the held line and a second Enter is an entry of its own,
    /// never a submit of a merged buffer. The gate is the readiness event's: the held line runs
    /// once ([`Self::execute_pending_command`]) and the gate then releases the keys behind it as
    /// for any Enter that ran ([`Self::release_unready_enter_gate_after_run`]). The only timer
    /// is the hand-back net ([`UNREADY_ENTER_GIVE_UP`]), which never runs anything.
    ///
    /// A deferred Enter (verdict / completion wait) already has its gate, holding what was typed
    /// while it waited: that gate is turned into this one in place (same barrier, nothing leaves
    /// the queue in between). A direct Enter opens it.
    pub(super) fn gate_unready_enter(&mut self, ctx: &mut ViewContext<Self>) {
        if self.enter_gate.is_some_and(|gate| gate.held_unready) {
            // Already gated (a retry that found the shell still unready): the same gate waits.
            return;
        }
        self.next_verdict_wait_id = self.next_verdict_wait_id.wrapping_add(1);
        let key = UNREADY_ENTER_KEY_FLAG | (self.next_verdict_wait_id & !UNREADY_ENTER_KEY_FLAG);
        // No queue cap: the queue's give-up would RELEASE the keys behind the line (an Enter among
        // them would then submit a merged buffer). The give-up is this input's own hand-back.
        let spec = GateSpec::new(GateKind::EnterVerdict, key).with_cap(None);
        let gated = match self.enter_gate {
            Some(gate) => self.retag_enter_gate(gate, spec, ctx),
            None => {
                self.open_enter_gate_with(spec, ctx);
                true
            }
        };
        if let Some(gate) = self.enter_gate.as_mut() {
            gate.held_unready = true;
            gate.submitted = false;
            gate.awaits_pty = false;
        }
        let gate_key = self.enter_gate.map(|gate| gate.key);
        let held_behind = self.enter_gate_held_len(ctx);
        warpui::input_order::log_hop("enter_held_gate", || {
            format!(
                "key={} gated={gated} held_behind={held_behind}",
                gate_key.unwrap_or_default()
            )
        });
        ctx.spawn(Timer::after(UNREADY_ENTER_GIVE_UP), move |me, _, ctx| {
            if gate_key.is_some() && me.enter_gate.map(|gate| gate.key) == gate_key {
                me.give_up_unready_enter("give_up_cap", ctx);
            }
        });
    }

    /// Turns the deferred Enter's open gate into the unready Enter's `spec` in place (#1571).
    /// Returns whether the gate now carries `spec`. The queue busy (this Enter is delivered by its
    /// drain): the stashed gate the delivery is about to return is replaced instead; with neither,
    /// the gate keeps its identity (logged) and is still held for the readiness event, only with
    /// the verdict gate's own net.
    fn retag_enter_gate(
        &mut self,
        gate: EnterGate,
        spec: GateSpec,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if self
            .stashed_gate
            .is_some_and(|stashed| stashed.kind == gate.kind && stashed.key == gate.key)
        {
            self.stashed_gate = Some(spec);
            self.enter_gate = Some(EnterGate {
                key: spec.key,
                kind: spec.kind,
                ..gate
            });
            return true;
        }
        let queue = self.enter_queue_handle(ctx);
        let retagged = match queue.try_lock_ordered() {
            Some(mut guard) => {
                let mut host = InputQueueHost::new(self, ctx);
                guard.retag_gate(&mut host, gate.kind, gate.key, spec, false)
            }
            None => false,
        };
        if retagged {
            self.enter_gate = Some(EnterGate {
                key: spec.key,
                kind: spec.kind,
                ..gate
            });
        } else {
            log::warn!(
                "input queue: the gate of an Enter refused by an unready shell could not be \
                 retagged; it is held under its verdict gate (#1571)"
            );
        }
        retagged
    }

    /// An Enter held because the shell was not ready gates the queue (#1571).
    pub(crate) fn unready_enter_gate_open(&self) -> bool {
        self.enter_gate.is_some_and(|gate| gate.held_unready)
    }

    /// #1571: the held line has run. The gate is now that of an Enter that ran: the keys behind it
    /// are released, in order, once the pty has taken the command (`settle_enter_queue`, the same
    /// events as for any Enter), to wherever they belong at that moment: the editor of the
    /// now-ready shell, or the program the line started (a nested ssh's hold).
    pub(super) fn release_unready_enter_gate_after_run(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(gate) = self.enter_gate.as_mut() else {
            return;
        };
        gate.held_unready = false;
        let held_behind = self.enter_gate_held_len(ctx);
        warpui::input_order::log_hop("enter_held_ran", || format!("held_behind={held_behind}"));
        self.last_enter_submitted_shell_command = true;
        if self.finish_enter_gate_after_submit(true, ctx) {
            self.emit_replay_after_deferred_enter(ctx);
        }
    }

    /// #1571: ^C met the Enter held for an unready shell: the line is void (the tty flushes what
    /// is typed but not read on INTR) and nothing runs at the readiness event. What was typed
    /// behind it was discarded by the signal; a gate the signal did not reach is closed here.
    pub(crate) fn cancel_unready_enter(&mut self, why: &'static str, ctx: &mut ViewContext<Self>) {
        let Some(gate) = self.enter_gate.filter(|gate| gate.held_unready) else {
            return;
        };
        warpui::input_order::log_hop("enter_held_cancelled", || format!("by={why}"));
        self.clear_held_unready_enter();
        self.enter_gate = None;
        let queue = self.enter_queue_handle(ctx);
        match queue.try_lock_ordered() {
            Some(mut guard) => {
                let mut host = InputQueueHost::new(self, ctx);
                let dropped = guard
                    .close_gate_taking(&mut host, gate.kind, gate.key)
                    .len();
                if dropped > 0 {
                    log::info!(
                        "input queue: {dropped} entr(ies) typed behind a cancelled unready Enter \
                         were dropped with it (#1571)"
                    );
                }
            }
            None => queue.defer(crate::terminal::input_queue::Deferred::VoidEnterGate {
                kind: gate.kind,
                key: gate.key,
            }),
        }
    }

    /// The pending-command state of an Enter held for an unready shell, cleared.
    fn clear_held_unready_enter(&mut self) {
        if self.held_unready_enter {
            self.has_pending_command = false;
            self.pending_command_text = None;
        }
        self.held_unready_enter = false;
    }

    /// #1571: the give-up of an Enter held for an unready shell (its net fired, or its gate is
    /// gone). NOTHING runs: the held line stays in the editor as text and what was typed after it
    /// follows it on the next line(s), visible and editable, with a toast. Logged.
    pub(crate) fn give_up_unready_enter(&mut self, why: &'static str, ctx: &mut ViewContext<Self>) {
        let Some(gate) = self.enter_gate.filter(|gate| gate.held_unready) else {
            return;
        };
        log::warn!(
            "input: an Enter held because the shell was not ready gave up ({why}); the line is \
             handed back to the input, not run (#1571)"
        );
        warpui::input_order::log_hop("enter_held_give_up", || format!("by={why}"));
        let held_behind = self.enter_gate_held_len(ctx);
        self.clear_held_unready_enter();
        self.enter_gate = None;
        let queue = self.enter_queue_handle(ctx);
        let taken = match queue.try_lock_ordered() {
            Some(mut guard) => {
                let mut host = InputQueueHost::new(self, ctx);
                Some(guard.close_gate_taking(&mut host, gate.kind, gate.key))
            }
            None => {
                // Busy: the take and the insert are one deferred operation (the text typed behind
                // the gate comes back into the editor, #1446), at the cursor: put it at the end of
                // the held line, on a line of its own.
                if held_behind > 0 {
                    let line = self.editor.as_ref(ctx).buffer_text(ctx);
                    if !line.is_empty() {
                        self.editor.update(ctx, |editor, ctx| {
                            editor.set_buffer_text(&format!("{line}\n"), ctx)
                        });
                    }
                }
                queue.defer(crate::terminal::input_queue::Deferred::VoidEnterGate {
                    kind: gate.kind,
                    key: gate.key,
                });
                None
            }
        };
        let typed_after = taken.map_or_else(String::new, |taken| {
            text_of_entries_typed_after(&taken, |id| self.take_paste_payload_text(id))
        });
        if !typed_after.is_empty() {
            let line = self.editor.as_ref(ctx).buffer_text(ctx);
            let text = if line.is_empty() {
                typed_after
            } else {
                format!("{line}\n{typed_after}")
            };
            self.editor
                .update(ctx, |editor, ctx| editor.set_buffer_text(&text, ctx));
            // Several lines in one buffer submit as ONE command: the first Enter on the handed-back
            // text is held with the "check the line" toast (the ssh hand-back's guard).
            self.arm_ssh_handover_enter_guard(text);
        } else {
            let line = self.editor.as_ref(ctx).buffer_text(ctx);
            self.arm_ssh_handover_enter_guard(line);
        }
        if ctx.has_singleton_model::<ToastStack>() {
            let window_id = ctx.window_id();
            ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                toast_stack.add_ephemeral_toast(
                    DismissibleToast::error(UNREADY_ENTER_GIVE_UP_TOAST.to_string()),
                    window_id,
                    ctx,
                );
            });
        }
        // Edits that waited behind the line apply now.
        self.flush_deferred_programmatic_edits(ctx);
    }

    /// The text of the editor paste `id` whose content waits here, left in place (#1585: the
    /// view asks the table where the paste goes before it takes it).
    pub(crate) fn peek_paste_payload_text(&self, id: u64) -> Option<String> {
        self.paste_payloads
            .get(&id)
            .map(|(_, content)| content.plain_text.clone())
    }

    /// The text of the editor paste `id` whose content waits here (a ready paste taken out of the
    /// queue without being delivered, #1446 / #1571), removed.
    pub(crate) fn take_paste_payload_text(&mut self, id: u64) -> Option<String> {
        self.paste_refused.remove(&id);
        self.paste_payloads
            .remove(&id)
            .map(|(_, content)| content.plain_text)
    }

    /// The gate [`Self::open_enter_gate`] could not open because the queue was delivering the
    /// Enter that needed it; taken once by the delivering host.
    pub(crate) fn take_stashed_gate(&mut self) -> Option<GateSpec> {
        self.stashed_gate.take()
    }

    /// Records that the view routed `input` after an Enter that waited, for tests.
    pub(crate) fn note_replayed_after_enter(&self, input: &AfterEnterInput) {
        #[cfg(test)]
        if let Some(sink) = &self.replayed_sink_for_test {
            sink.borrow_mut().push(input.clone());
        }
        #[cfg(not(test))]
        let _ = input;
    }

    /// Queues `input` behind the Enter whose gate is open. False when no gate holds the queue.
    pub(crate) fn queue_behind_enter(
        &mut self,
        input: AfterEnterInput,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if !self.enter_queue_holds_keys(ctx) {
            return false;
        }
        let queue = self.enter_queue_handle(ctx);
        let kind = match input {
            AfterEnterInput::Text(text) => EntryKind::Text(text),
            AfterEnterInput::Enter => EntryKind::Enter,
        };
        let Some(mut guard) = queue.try_lock_ordered() else {
            queue.defer(crate::terminal::input_queue::Deferred::Offer {
                kind,
                origin: crate::terminal::input_queue::Origin::Keyboard,
                stamp: queue.stamp_now(),
            });
            return true;
        };
        let mut host = InputQueueHost::new(self, ctx);
        guard.offer(&mut host, kind);
        true
    }

    /// Queues an Enter that reached the editor while another is deferred behind the hand-off's
    /// report, behind that Enter's `HandoffReport` gate. The gate is open from the moment the
    /// Enter is deferred (the view opens it on `TypeaheadHandoffEnterDeferred`); without it the
    /// Enter has nowhere to wait and is logged.
    pub(super) fn queue_enter_behind_deferred_enter(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = self.enter_queue_handle(ctx);
        if !queue
            .try_peek()
            .is_some_and(|queue| queue.has_gate_kind(GateKind::HandoffReport))
        {
            log::warn!("input queue: an Enter typed behind a deferred Enter found no gate");
            return;
        }
        let Some(mut guard) = queue.try_lock_ordered() else {
            queue.defer(crate::terminal::input_queue::Deferred::Offer {
                kind: EntryKind::Enter,
                origin: crate::terminal::input_queue::Origin::Keyboard,
                stamp: queue.stamp_now(),
            });
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        guard.offer(&mut host, EntryKind::Enter);
    }

    /// An Enter typed behind the one that waits, on the queue behind the Enter's gate. The queue
    /// not holding it (mid-drain) is not reachable from a user key; the Enter is then dropped
    /// with a warning (it has nothing to submit: the line it follows is the waiting one).
    pub(super) fn queue_enter_behind_wait(&mut self, ctx: &mut ViewContext<Self>) {
        if !self.queue_behind_enter(AfterEnterInput::Enter, ctx) {
            log::warn!("input queue: an Enter typed behind a waiting Enter could not be queued");
        }
    }

    /// How many entries wait behind the Enter's gate.
    pub(super) fn enter_gate_held_len(&self, ctx: &AppContext) -> usize {
        let Some(gate) = self.enter_gate else {
            return 0;
        };
        self.enter_queue_handle(ctx)
            .try_peek()
            .map_or(0, |queue| queue.held_behind_gate(gate.kind, gate.key))
    }

    /// The waiting Enter is void (^C, an edit of its line): closes the gate and hands back the text
    /// that was typed behind it, which stays in the editor rather than being lost (Enters typed
    /// behind it have nothing to submit and go).
    pub(super) fn void_enter_gate(&mut self, ctx: &mut ViewContext<Self>) -> String {
        let Some(gate) = self.enter_gate.take() else {
            return String::new();
        };
        let queue = self.enter_queue_handle(ctx);
        let Some(mut guard) = queue.try_lock_ordered() else {
            // Busy (mid-drain) or earlier operations wait: the take and the editor insert are one
            // deferred operation the view runs when the queue is free (#1446); the text typed
            // behind the void Enter is neither lost nor left in the queue.
            queue.defer(crate::terminal::input_queue::Deferred::VoidEnterGate {
                kind: gate.kind,
                key: gate.key,
            });
            queue.defer(crate::terminal::input_queue::Deferred::FlushProgrammaticEdits);
            return String::new();
        };
        let mut host = InputQueueHost::new(self, ctx);
        let typed_behind: String = guard
            .close_gate_taking(&mut host, gate.kind, gate.key)
            .into_iter()
            .filter_map(|entry| match entry.kind {
                EntryKind::Text(text) => Some(text),
                _ => None,
            })
            .collect();
        drop(guard);
        // The Enter is void: the programmatic edits that waited behind it apply now (#1434).
        self.flush_deferred_programmatic_edits(ctx);
        typed_behind
    }

    /// Test seam for [`Self::void_enter_gate`].
    #[cfg(test)]
    pub(crate) fn void_enter_gate_for_test(&mut self, ctx: &mut ViewContext<Self>) -> String {
        self.void_enter_gate(ctx)
    }

    /// The Enter has run. With nothing typed behind it the gate closes now; otherwise it is kept
    /// until the view has seen the route settle and asks for the release
    /// ([`Self::release_enter_gate`]). Returns whether the view has a release to do.
    pub(super) fn finish_enter_gate_after_submit(
        &mut self,
        awaits_pty: bool,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if self.unready_enter_gate_open() {
            // #1571: the Enter did not run (the shell was not ready): its gate waits for the
            // readiness event, not for this submit's route.
            return false;
        }
        if self.enter_gate_held_len(ctx) == 0 {
            self.release_enter_gate(ctx);
            return false;
        }
        if let Some(gate) = self.enter_gate.as_mut() {
            gate.submitted = true;
            gate.awaits_pty = awaits_pty;
        }
        true
    }

    /// Closes the Enter's gate and releases what waited behind it, in order: the drain stops at
    /// the first entry only the terminal view routes and the view continues it.
    pub(crate) fn release_enter_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(gate) = self.enter_gate.take() else {
            return;
        };
        let queue = self.enter_queue_handle(ctx);
        let Some(mut guard) = queue.try_lock_ordered() else {
            queue.defer(crate::terminal::input_queue::Deferred::GateEvent {
                kind: gate.kind,
                key: gate.key,
            });
            queue.defer(crate::terminal::input_queue::Deferred::FlushProgrammaticEdits);
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, gate.kind, gate.key);
        drop(guard);
        // The Enter ran and its gate is closed: the programmatic edits that waited behind it
        // apply now, and the pending command they were holding back is retried (#1434, #1435).
        self.flush_deferred_programmatic_edits(ctx);
    }

    /// The route-settle give-up of gate `key` fired: the event that should have opened it (the pty
    /// took the command) never came. Releases the gate and says so.
    pub(crate) fn give_up_enter_route_wait(&mut self, key: u64, ctx: &mut ViewContext<Self>) {
        if self.enter_gate.map(|gate| gate.key) != Some(key) {
            return;
        }
        log::warn!(
            "input queue: keys typed behind an Enter waited for the pty to take its command and \
             gave up; routing them now"
        );
        self.release_enter_gate(ctx);
    }

    /// ^C / ^D reached the terminal while keys wait behind an Enter: what was typed behind it is
    /// dropped (the tty flushes its input on ^C) and the gate closes. The signal itself is written
    /// by the caller.
    pub(crate) fn abort_enter_queue(&mut self, ctx: &mut ViewContext<Self>) {
        if !self.enter_queue_holds_keys(ctx) {
            return;
        }
        if self.unready_enter_gate_open() {
            // #1571: the held line goes with what was typed behind it.
            self.cancel_unready_enter("terminal_interrupt", ctx);
            return;
        }
        let queue = self.enter_queue_handle(ctx);
        let kind = EntryKind::Signal(crate::terminal::input_queue::SignalKind::Interrupt);
        let Some(mut guard) = queue.try_lock_ordered() else {
            queue.defer(crate::terminal::input_queue::Deferred::Offer {
                kind,
                origin: crate::terminal::input_queue::Origin::Keyboard,
                stamp: queue.stamp_now(),
            });
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        guard.offer(&mut host, kind);
    }

    /// Offers pty bytes to the pane queue through the EDITOR host, whose drain stops at them as a
    /// foreign entry (#1580 test): the view has to continue the drain.
    #[cfg(test)]
    pub(crate) fn offer_pty_bytes_through_editor_host_for_test(
        &mut self,
        bytes: Vec<u8>,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.enter_queue_handle(ctx);
        let mut guard = queue.lock();
        let mut host = InputQueueHost::new(self, ctx);
        guard.offer(
            &mut host,
            EntryKind::PtyBytes {
                bytes,
                takes_over_agent_control: true,
            },
        );
    }

    /// The gate of an Enter that waits for its input-type verdict (nothing submitted, nothing
    /// queued behind it yet), for tests of what must wait for it.
    #[cfg(test)]
    pub(crate) fn open_waiting_enter_gate_for_test(&mut self, ctx: &mut ViewContext<Self>) {
        self.open_enter_gate(1, GateKind::EnterVerdict, ctx);
    }

    /// A submitted shell-command Enter whose gate holds one key and waits for the pty to take its
    /// command, for tests of the events that release it.
    #[cfg(test)]
    pub(crate) fn open_submitted_enter_gate_awaiting_pty_for_test(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) {
        self.open_submitted_enter_gate_for_test(
            vec![AfterEnterInput::Text("x".to_string())],
            true,
            ctx,
        );
    }

    /// The gate of an Enter that has run, holding `queued` behind it (what the after-Enter replay
    /// list used to be opened with in tests); `awaits_pty` as for a submitted shell command.
    #[cfg(test)]
    pub(crate) fn open_submitted_enter_gate_for_test(
        &mut self,
        queued: Vec<AfterEnterInput>,
        awaits_pty: bool,
        ctx: &mut ViewContext<Self>,
    ) {
        self.open_enter_gate(1, GateKind::EnterVerdict, ctx);
        for item in queued {
            assert!(self.queue_behind_enter(item, ctx));
        }
        self.finish_enter_gate_after_submit(awaits_pty, ctx);
    }

    /// The entries behind the Enter, for tests.
    #[cfg(test)]
    pub(crate) fn enter_gate_held_len_for_test(&self, ctx: &AppContext) -> usize {
        self.enter_gate_held_len(ctx)
    }

    /// The `Held` entries (class, text) the Enter gate holds, in order, for tests (#1586).
    #[cfg(test)]
    pub(crate) fn queued_held_for_test(&self, ctx: &AppContext) -> Vec<(u8, String)> {
        let queue = self.enter_queue_handle(ctx);
        let queue = queue.peek();
        queue
            .entries()
            .filter_map(|entry| match &entry.kind {
                EntryKind::Held { class, text } => Some((*class, text.clone())),
                _ => None,
            })
            .collect()
    }

    /// The text and Enters queued behind the Enter, in order, adjacent text merged: what the
    /// wait's own list and the after-Enter replay list used to hold, for tests.
    #[cfg(test)]
    pub(crate) fn enter_gate_held_for_test(&self, ctx: &AppContext) -> Vec<AfterEnterInput> {
        let mut held: Vec<AfterEnterInput> = Vec::new();
        let queue = self.enter_queue_handle(ctx);
        let queue = queue.peek();
        for entry in queue.entries() {
            match &entry.kind {
                EntryKind::Text(text) => match held.last_mut() {
                    Some(AfterEnterInput::Text(last)) => last.push_str(text),
                    _ => held.push(AfterEnterInput::Text(text.clone())),
                },
                EntryKind::Enter => held.push(AfterEnterInput::Enter),
                _ => {}
            }
        }
        held
    }

    /// Emits the replay request of a submitted Enter whose gate holds keys.
    pub(super) fn emit_replay_after_deferred_enter(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(Event::ReplayAfterDeferredEnter);
    }
}

/// #1571: what the entries typed behind an Enter held for an unready shell spell, as editor text:
/// text as typed, each Enter a line break, the tty editing keys applied to the text before them
/// (Backspace, ^U, ^W). Cursor keys have no text form and are left out. Nothing here is run.
/// Also what the view's deferred `VoidEnterGate` gives back (#1446): every queued kind with a text
/// form keeps it, pastes included. A ready paste's content usually waits in its owner's map, not
/// in the entry (`Some(empty)` is the "read" marker): `paste_text` takes it from there by id.
pub(crate) fn text_of_entries_typed_after(
    entries: &[InputEntry],
    mut paste_text: impl FnMut(u64) -> Option<String>,
) -> String {
    let mut text = String::new();
    for entry in entries {
        match &entry.kind {
            EntryKind::Text(typed) => text.push_str(typed),
            EntryKind::Paste {
                paste_id,
                payload: Some(bytes),
            } => match paste_text(*paste_id) {
                Some(pasted) => text.push_str(&pasted),
                None => text.push_str(&String::from_utf8_lossy(bytes)),
            },
            EntryKind::Enter => text.push('\n'),
            EntryKind::Held { class, text: held } => match *class {
                HELD_TYPED | HELD_PASTE => text.push_str(held),
                HELD_KEY => match held.as_str() {
                    "\r" | "\n" | "\r\n" => text.push('\n'),
                    "\u{7f}" | "\u{8}" => {
                        if !text.ends_with('\n') {
                            text.pop();
                        }
                    }
                    "\u{15}" => {
                        let line_start = text.rfind('\n').map_or(0, |at| at + 1);
                        text.truncate(line_start);
                    }
                    "\u{17}" => {
                        let line_start = text.rfind('\n').map_or(0, |at| at + 1);
                        let line = &text[line_start..];
                        let kept = line.trim_end_matches(' ');
                        let word_start = kept.rfind(' ').map_or(0, |at| at + 1);
                        text.truncate(line_start + word_start);
                    }
                    _ => {}
                },
                HELD_SEQ => {}
                _ => {}
            },
            EntryKind::EditorKey(payload) => {
                if let Some(HeldPasteKey::Text(typed)) = payload.take() {
                    text.push_str(&typed);
                }
            }
            _ => {}
        }
    }
    text.trim_end_matches('\n').to_owned()
}
