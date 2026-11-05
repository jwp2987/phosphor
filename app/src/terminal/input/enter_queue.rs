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
//!   one logged give-up timer, armed once ([`TerminalView::settle_enter_queue`]). The Enter's own
//!   `ReplayAfterDeferredEnter` is handled in the flush that carries the command to the pty
//!   controller, ahead of it, so for such a gate the view settles it only after that flush (an
//!   executor hop, not a timer, and only that gate): by then the controller has taken the command
//!   and its event has usually settled the gate, so the keys follow the command (#1630).
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
use super::{
    AfterEnterInput, EnterGuardKind, Event, GLUE_GUARD_TOAST, HANDED_BACK_GUARD_TOAST, Input,
};
use crate::editor::HeldPasteKey;
use crate::terminal::input_queue::{
    EditorKeyPayload, EntryKind, GateKind, GateSpec, HELD_KEY, HELD_PASTE, HELD_SEQ, HELD_TYPED,
    InputEntry, MULTILINE_DIALOG_KEY_FLAG, PaneQueue, UNREADY_ENTER_KEY_FLAG,
};
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

/// The longest an Enter gate may stay closed whatever happens: a net for a bug that would leave
/// it open, far beyond every real hold (the hold backstops are 2 s plus the size allowance for a
/// verdict, 500 ms for a Tab completion; the route-settle cap 300 ms). Logged by the queue when it fires.
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
    /// #1652: the Enter asked the multi-line submit dialog and waits for its answer. Everything
    /// typed after it (into the editor, or to the dialog, which hands its keys to the editor) is
    /// held behind this gate in typed order; only the answer ends it
    /// ([`Input::answer_multiline_dialog`], [`Input::cancel_multiline_dialog`]).
    pub(super) held_for_dialog: bool,
}

/// #1571: the give-up net of an Enter held because the shell was not ready. The readiness events
/// (the session's registration, its first Precmd) end the hold long before this; when it fires
/// nothing is run: the held line and what was typed after it go back to the editor as text, with
/// a toast, and it is logged. Long enough for a slow remote bootstrap (the ssh connect's own
/// ceilings are 15 s).
pub(super) const UNREADY_ENTER_GIVE_UP: Duration = Duration::from_secs(15);

/// #1577: the toast of an Enter gate that gave up without routing what was typed behind the Enter.
const ENTER_GATE_GIVE_UP_TOAST: &str = "The shell did not take the command in time; what you typed \
     after it is in the input, not sent";

/// #1577: the toast of an Enter that was not submitted because its verdict never came.
const VERDICT_GIVE_UP_TOAST: &str =
    "Couldn't tell shell from chat in time; press Enter again to send";

/// #1666/#1667: the toast of an Enter held for a Tab completion that did not land in time: the
/// uncompleted line was not run. Nothing was typed behind it, so the Enter on the line as shown
/// runs it (the glue guard holds only text typed onto it).
pub(crate) const COMPLETION_GIVE_UP_TOAST: &str = "Completion took too long, so the line was not \
     run. Press Tab to complete it, or Enter to run it as shown.";

/// #1666: the same give-up with keys typed behind the Enter: they come back after the line and the
/// next Enter is held by the guard, so this toast must not promise that Enter runs the line.
pub(crate) const COMPLETION_GIVE_UP_WITH_TYPED_TOAST: &str = "Completion took too long, so nothing \
     was run. The line and what you typed after it are in the input; check them before you press \
     Enter.";

/// #1666: the toast of the Enter the guard holds after [`COMPLETION_GIVE_UP_WITH_TYPED_TOAST`].
pub(crate) const COMPLETION_GUARD_TOAST: &str = "Enter was held back: the completion did not \
     finish, so this text was not run. Check it, then press Enter again to run it.";

const UNREADY_ENTER_GIVE_UP_TOAST: &str = "The shell was not ready, so the line was not run; it is \
     back in the input with what you typed after it";

/// #1666: the toast of the Enter the guard holds after an unready shell's give-up.
const UNREADY_GUARD_TOAST: &str = "Enter was held back: the shell was not ready, so this line did \
     not run. Check it, then press Enter to run it";

/// What a hand-back of a held Enter tells the user and which guard it arms (#1666): each give-up
/// says its own reason, and the guard's toast agrees with the give-up's.
#[derive(Clone, Copy, Debug)]
struct HandBack {
    /// The give-up's toast when only the line comes back.
    toast: &'static str,
    /// The give-up's toast when keys typed behind the Enter come back after the line (the next
    /// Enter is then held by the guard).
    toast_with_typed: &'static str,
    /// The toast of the Enter the guard holds when keys came back with the line.
    guard_toast: &'static str,
    /// Whether the line alone arms the [`EnterGuardKind::Glue`] guard (#1666). The Tab-completion
    /// give-up does; the verdict give-up does not (yet: the owner deferred it to the design pass).
    glue_guard_on_line_alone: bool,
}

impl HandBack {
    /// A give-up whose toast is the same with or without keys behind it, and no glue guard.
    const fn plain(toast: &'static str, guard_toast: &'static str) -> Self {
        Self {
            toast,
            toast_with_typed: toast,
            guard_toast,
            glue_guard_on_line_alone: false,
        }
    }

    /// The hand-back of the gate give-up named `why`.
    fn for_give_up(why: &str) -> Self {
        match why {
            "verdict_cap" => Self::plain(VERDICT_GIVE_UP_TOAST, HANDED_BACK_GUARD_TOAST),
            "completion_cap" | "completion_backstop" => Self::COMPLETION,
            _ => Self::plain(ENTER_GATE_GIVE_UP_TOAST, HANDED_BACK_GUARD_TOAST),
        }
    }

    /// The Tab-completion give-up (#1666/#1667).
    const COMPLETION: Self = Self {
        toast: COMPLETION_GIVE_UP_TOAST,
        toast_with_typed: COMPLETION_GIVE_UP_WITH_TYPED_TOAST,
        guard_toast: COMPLETION_GUARD_TOAST,
        glue_guard_on_line_alone: true,
    };
}

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
        // #1652: the queue is free again: a multi-line dialog answer that waited for it goes
        // first (it flushes the edits itself once what it holds is placed).
        if self.apply_pending_multiline_dialog_answer(ctx) {
            return;
        }
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

    /// The wall time the current input-type detection's stages have run (#1651 review): a young
    /// line's Enter gives up at the detection's wall ceiling too, for a classifier that waits off
    /// the CPU.
    pub(crate) fn detection_stage_wall(&self, ctx: &AppContext) -> Duration {
        self.ai_input_model
            .as_ref(ctx)
            .detection_clock()
            .stage_wall()
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

    /// The key of the Enter gate this input has open (#1630: a deferred settle names its gate).
    pub(crate) fn enter_gate_key(&self) -> Option<u64> {
        self.enter_gate.map(|gate| gate.key)
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
            held_for_dialog: false,
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
            gate.held_for_dialog = false;
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

    /// #1652: an Enter that asked the multi-line submit dialog gates the queue until the answer.
    pub(crate) fn multiline_dialog_gate_open(&self) -> bool {
        self.enter_gate.is_some_and(|gate| gate.held_for_dialog)
    }

    /// #1652: the Enter asked the multi-line submit dialog (the workspace showed it). Its gate holds
    /// everything typed after it, in typed order, until the answer: the gate of an Enter held for
    /// an unready shell in all but its ending (the same key flag, so the editor holds its editing
    /// keys as keys of the NEXT line and an Enter as an entry of its own, never a submit of the
    /// waiting buffer). A deferred Enter's open gate (verdict / completion) is turned into this one
    /// in place, so the typeahead already behind it stays behind it. No timer: the dialog is on
    /// screen and only its answer ends the gate (or ^C, which voids the Enter).
    ///
    /// Returns whether the gate holds: false when a deferred Enter's gate could not be retagged
    /// (the queue was busy), and the caller then closes the dialog (L1: a dialog is never open
    /// without its gate).
    pub(super) fn gate_enter_for_multiline_dialog(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        if self.multiline_dialog_gate_open() {
            return true;
        }
        self.next_verdict_wait_id = self.next_verdict_wait_id.wrapping_add(1);
        let flags = UNREADY_ENTER_KEY_FLAG | MULTILINE_DIALOG_KEY_FLAG;
        let key = flags | (self.next_verdict_wait_id & !flags);
        // No cap: a queue give-up would RELEASE the keys behind the waiting buffer.
        let spec = GateSpec::new(GateKind::EnterVerdict, key).with_cap(None);
        let gated = match self.enter_gate {
            Some(gate) => self.retag_enter_gate(gate, spec, ctx),
            None => {
                self.open_enter_gate_with(spec, ctx);
                true
            }
        };
        if gated && let Some(gate) = self.enter_gate.as_mut() {
            gate.held_for_dialog = true;
            gate.held_unready = false;
            gate.submitted = false;
            gate.awaits_pty = false;
        }
        let held_behind = self.enter_gate_held_len(ctx);
        warpui::input_order::log_hop("enter_dialog_gate", || {
            format!("gated={gated} held_behind={held_behind}")
        });
        gated
    }

    /// #1652: the dialog was answered (`confirm`). What was typed while it waited is taken out of
    /// the gate and DISCARDED (owner: "if you type something in there that isn't part of the
    /// approval throw it away"), with a log line and a toast that say how many. Confirm then runs
    /// the buffer as the waiting Enter's own submit, once; Cancel leaves it exactly as it was.
    ///
    /// A paste behind the gate still being read, or a busy queue, defers the answer to the
    /// queue's next free moment (`flush_deferred_programmatic_edits`, which the read's result,
    /// its give-up and a deferred flush all reach). Never a timer.
    pub(super) fn answer_multiline_dialog(&mut self, confirm: bool, ctx: &mut ViewContext<Self>) {
        let Some(gate) = self.enter_gate.filter(|gate| gate.held_for_dialog) else {
            // No gate (an answer arriving after ^C voided the Enter, or a direct call): Confirm
            // runs the buffer as before; Cancel has nothing to do.
            if confirm {
                self.run_confirmed_multiline_submit(ctx);
            }
            return;
        };
        let queue = self.enter_queue_handle(ctx);
        let paste_reading = queue
            .try_peek()
            .is_some_and(|held| held.has_editor_paste_reading());
        let taken = if paste_reading {
            None
        } else {
            match queue.try_lock_ordered() {
                Some(mut guard) => {
                    let mut host = InputQueueHost::new(self, ctx);
                    Some(guard.close_gate_taking(&mut host, gate.kind, gate.key))
                }
                None => None,
            }
        };
        let Some(taken) = taken else {
            warpui::input_order::log_hop("enter_dialog_answer_waits", || {
                format!("confirm={confirm} paste_reading={paste_reading}")
            });
            self.pending_multiline_dialog_answer = Some(confirm);
            if !paste_reading {
                // The queue is busy: its next free moment runs the flush that applies the answer.
                // (A paste's read result, or its give-up, flushes by itself.)
                queue.defer(crate::terminal::input_queue::Deferred::FlushProgrammaticEdits);
            }
            return;
        };
        self.enter_gate = None;
        self.pending_multiline_dialog_answer = None;
        warpui::input_order::log_hop("enter_dialog_answered", || {
            format!("confirm={confirm} held_behind={}", taken.len())
        });
        // Owner rule: "if you type something in there that isn't part of the approval throw it
        // away". What was typed while the dialog was open (anything but the dialog's own Enter,
        // Escape and buttons) is DISCARDED on Confirm and on Cancel alike, with a log line and a
        // toast; it never runs and never reaches the editor.
        let discarded = self.discard_dialog_typeahead(&taken);
        self.report_discarded_dialog_typeahead(
            discarded,
            if confirm { "confirm" } else { "cancel" },
            ctx,
        );
        if confirm {
            // The confirmed buffer runs exactly as it would have, once.
            self.run_confirmed_multiline_submit(ctx);
            if self.enter_gate.is_some() {
                // An unready shell's refusal made its own gate: its event releases the edits.
                return;
            }
        }
        // Cancel: the buffer stays exactly as it was.
        self.flush_deferred_programmatic_edits(ctx);
    }

    /// #1652: says (log and toast) how many keys typed while the dialog was open were discarded.
    fn report_discarded_dialog_typeahead(
        &mut self,
        discarded: usize,
        by: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        if discarded == 0 {
            return;
        }
        log::warn!(
            "input: {discarded} key(s) typed while the multi-line dialog was open were discarded \
             ({by}) (#1652)"
        );
        warpui::input_order::log_hop("enter_dialog_typeahead_discarded", || {
            format!("keys={discarded} by={by}")
        });
        if ctx.has_singleton_model::<ToastStack>() {
            let window_id = ctx.window_id();
            let text = dialog_typeahead_discarded_toast(discarded);
            ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                toast_stack.add_ephemeral_toast(DismissibleToast::default(text), window_id, ctx);
            });
        }
    }

    /// #1652: the held entries are discarded: the content of a paste among them that waits in this
    /// input's map goes with them. Returns how many keys they were.
    fn discard_dialog_typeahead(&mut self, taken: &[InputEntry]) -> usize {
        for entry in taken {
            if let EntryKind::Paste { paste_id, .. } = &entry.kind {
                let _ = self.take_paste_payload_text(*paste_id);
            }
        }
        taken
            .iter()
            .filter(|entry| !matches!(entry.kind, EntryKind::Signal(_)))
            .count()
    }

    /// #1652: the confirmed buffer runs as the waiting Enter's own submit (never queued behind its
    /// own gate). Returns whether a shell command was written.
    fn run_confirmed_multiline_submit(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        self.multiline_submit_confirmed = true;
        let was_deferred = std::mem::replace(&mut self.running_deferred_enter, true);
        let submitted = self.input_enter_as_shell_command(ctx);
        self.running_deferred_enter = was_deferred;
        self.multiline_submit_confirmed = false;
        submitted
    }

    /// #1652: an answer that waited for a paste's read (or a busy queue) is applied now, when the
    /// queue is free again. Returns whether one was applied.
    pub(crate) fn apply_pending_multiline_dialog_answer(
        &mut self,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let Some(confirm) = self.pending_multiline_dialog_answer.take() else {
            return false;
        };
        self.answer_multiline_dialog(confirm, ctx);
        true
    }

    /// #1652: ^C met the Enter that waits for the dialog: the Enter is void and what was typed
    /// behind it goes with it (the tty flushes typed-ahead input on INTR), as for an Enter held for
    /// an unready shell. The dialog is closed. Returns whether a dialog gate was open.
    pub(crate) fn cancel_multiline_dialog(
        &mut self,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let Some(gate) = self.enter_gate.filter(|gate| gate.held_for_dialog) else {
            return false;
        };
        warpui::input_order::log_hop("enter_dialog_cancelled", || format!("by={why}"));
        self.enter_gate = None;
        self.pending_multiline_dialog_answer = None;
        let queue = self.enter_queue_handle(ctx);
        match queue.try_lock_ordered() {
            Some(mut guard) => {
                let mut host = InputQueueHost::new(self, ctx);
                let taken = guard.close_gate_taking(&mut host, gate.kind, gate.key);
                drop(guard);
                let dropped = self.discard_dialog_typeahead(&taken);
                self.report_discarded_dialog_typeahead(dropped, why, ctx);
            }
            None => queue.defer(crate::terminal::input_queue::Deferred::VoidEnterGate {
                kind: gate.kind,
                key: gate.key,
            }),
        }
        self.end_multiline_dialog_wait(ctx);
        // L3: edits that waited behind the void Enter apply now.
        self.flush_deferred_programmatic_edits(ctx);
        true
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
        self.hand_back_enter_gate(
            gate,
            HandBack::plain(UNREADY_ENTER_GIVE_UP_TOAST, UNREADY_GUARD_TOAST),
            ctx,
        );
    }

    /// #1577 (A2): a timer gave up on the Enter gate (the route-settle net, the verdict cap, the
    /// generic leak net). NOTHING is routed or run: what was typed behind the Enter comes back to
    /// the editor as text, the next Enter is consumed once with the "check the line" toast, and it
    /// is logged with counts only. An Enter held for an unready shell has its own give-up.
    pub(crate) fn give_up_enter_gate_to_editor(
        &mut self,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.give_up_enter_gate_with(why, HandBack::for_give_up(why), ctx);
    }

    /// [`Self::give_up_enter_gate_to_editor`] with the hand-back's texts and guard given.
    fn give_up_enter_gate_with(
        &mut self,
        why: &'static str,
        hand_back: HandBack,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(gate) = self.enter_gate else {
            return;
        };
        if gate.held_unready {
            self.give_up_unready_enter(why, ctx);
            return;
        }
        if gate.held_for_dialog {
            // #1652: the dialog is on screen and its answer ends this gate. A net that fires here
            // (the verdict gate's, kept when its retag found the queue busy) must not hand the
            // keys back into the buffer the dialog asks about.
            log::warn!(
                "input: an Enter gate net fired while the multi-line dialog waits ({why}); kept (#1652)"
            );
            return;
        }
        let held_behind = self.enter_gate_held_len(ctx);
        if !gate.submitted {
            // The Enter is still waiting for its verdict / completion: it is void now, so a late
            // result cannot submit a buffer that holds the handed-back text.
            self.void_verdict_wait_for_give_up();
        }
        log::warn!(
            "input: the Enter gate gave up ({why}) with {held_behind} entr(ies) behind it; they \
             are handed back to the input, nothing is routed (#1577)"
        );
        warpui::input_order::log_key_hop("kh_enter_gate_give_up", || {
            format!("by={why} behind={held_behind} submitted={}", gate.submitted)
        });
        self.hand_back_enter_gate(gate, hand_back, ctx);
    }

    /// #1577: an Enter whose verdict / completion never came within the cap is not submitted by
    /// the clock. With its gate open the text typed behind it comes back with it
    /// ([`Self::give_up_enter_gate_to_editor`]); otherwise the wait is dropped and the line stays
    /// in the editor. The next Enter submits it, with the verdict by then in.
    pub(super) fn hand_back_unsubmitted_enter(
        &mut self,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        warpui::input_order::log_key_hop("enter_verdict_give_up", || format!("editor {why}"));
        // The late verdict is dropped (as a submit would have): the next Enter, the user's own
        // say-so after the toast, submits on the Enter-time rules instead of waiting again for a
        // classifier that may be stuck for good. A large buffer's verdict is kept instead (#1604):
        // its job is linear and lands, and the next Enter waits for it or submits on it.
        self.ai_input_model.update(ctx, |model, _| {
            model.abandon_pending_detection_after_give_up()
        });
        self.hand_back_held_enter(
            why,
            HandBack::plain(VERDICT_GIVE_UP_TOAST, HANDED_BACK_GUARD_TOAST),
            ctx,
        );
    }

    /// #1666/#1667: an Enter held for a Tab completion that did not land within its cap or
    /// backstop is not submitted by the clock (the visible line is not the one the user meant to
    /// run). It is handed back exactly like a verdict give-up, with its own toast; the partial
    /// line's verdict is kept, since the classifier is not what was slow.
    pub(super) fn hand_back_completion_held_enter(
        &mut self,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        warpui::input_order::log_key_hop("enter_completion_give_up", || format!("editor {why}"));
        // A Tab-completion hold's gate is never the multi-line dialog's or an unready shell's,
        // whose give-ups would keep or re-route it.
        debug_assert!(
            self.enter_gate
                .is_none_or(|gate| !gate.held_for_dialog && !gate.held_unready),
            "a Tab-completion give-up found a dialog / unready gate"
        );
        self.hand_back_held_enter(why, HandBack::COMPLETION, ctx);
    }

    /// The shared body of the held-Enter hand-backs: with its gate open, the text typed behind
    /// the Enter comes back with the line ([`Self::give_up_enter_gate_with`]); otherwise the wait
    /// is dropped, the line stays in the editor (with the glue guard if `hand_back` asks for it),
    /// and its toast is shown.
    fn hand_back_held_enter(
        &mut self,
        why: &'static str,
        hand_back: HandBack,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.enter_gate.is_some() {
            self.give_up_enter_gate_with(why, hand_back, ctx);
            return;
        }
        self.void_verdict_wait_for_give_up();
        if hand_back.glue_guard_on_line_alone {
            let line = self.editor.as_ref(ctx).buffer_text(ctx);
            self.arm_enter_guard(line, EnterGuardKind::Glue, GLUE_GUARD_TOAST);
        }
        if ctx.has_singleton_model::<ToastStack>() {
            let window_id = ctx.window_id();
            ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                toast_stack.add_ephemeral_toast(
                    DismissibleToast::error(hand_back.toast.to_string()),
                    window_id,
                    ctx,
                );
            });
        }
    }

    /// [`Self::give_up_enter_gate_to_editor`] for the gate `(kind, key)` a cap named: only when it
    /// is this input's own Enter gate. Returns whether it was.
    pub(crate) fn give_up_enter_gate_by_key(
        &mut self,
        kind: GateKind,
        key: u64,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if !self
            .enter_gate
            .is_some_and(|gate| gate.kind == kind && gate.key == key)
        {
            return false;
        }
        self.give_up_enter_gate_to_editor(why, ctx);
        true
    }

    /// The shared body of the Enter-gate give-ups: the gate is closed taking what waited behind
    /// it and that text goes back into the editor, with the Enter guard armed and a toast. Both
    /// texts, and whether the line alone arms the glue guard, come from `hand_back` (#1666).
    ///
    /// An Enter that was already SUBMITTED (the command is in flight; the buffer still holds the
    /// submitted line until its block completes) is not part of the hand-back: the text follows
    /// the submitted line directly, which is where typing it would have put it, so the block's
    /// completion keeps exactly the text typed after it.
    fn hand_back_enter_gate(
        &mut self,
        gate: EnterGate,
        hand_back: HandBack,
        ctx: &mut ViewContext<Self>,
    ) {
        let submitted = gate.submitted && !gate.held_unready;
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
                if held_behind > 0 && !submitted {
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
        // Whether keys typed behind the Enter come back after the line (now, or with the deferred
        // take when the queue was busy).
        let deferred_typed = taken.is_none() && held_behind > 0;
        let typed_after = taken.map_or_else(String::new, |taken| {
            text_of_entries_typed_after(&taken, |id| self.take_paste_payload_text(id))
        });
        let toast = if !submitted && (deferred_typed || !typed_after.is_empty()) {
            hand_back.toast_with_typed
        } else {
            hand_back.toast
        };
        if !typed_after.is_empty() {
            let line = self.editor.as_ref(ctx).buffer_text(ctx);
            let (text, baseline) = if submitted {
                (format!("{line}{typed_after}"), typed_after)
            } else if line.is_empty() {
                (typed_after.clone(), typed_after)
            } else {
                let text = format!("{line}\n{typed_after}");
                (text.clone(), text)
            };
            self.editor
                .update(ctx, |editor, ctx| editor.set_buffer_text(&text, ctx));
            // Several lines in one buffer submit as ONE command: the first Enter on the handed-back
            // text is held with the give-up's own "check the line" toast.
            self.arm_enter_guard(
                baseline,
                EnterGuardKind::HandedBackText,
                hand_back.guard_toast,
            );
        } else if !submitted && gate.held_unready {
            // The held line alone (an unready shell's Enter): its next Enter is held with the
            // toast. A line given up for a verdict is what the user typed: Enter sends it.
            let line = self.editor.as_ref(ctx).buffer_text(ctx);
            self.arm_enter_guard(line, EnterGuardKind::HandedBackText, hand_back.guard_toast);
        } else if !submitted && hand_back.glue_guard_on_line_alone {
            // #1666: the line alone, which the user may run as shown: only text typed onto its end
            // holds the next Enter (`cd proj` handed back, then `ls` Enter must not run
            // `cd projls`).
            let line = self.editor.as_ref(ctx).buffer_text(ctx);
            let line = line.strip_suffix('\n').unwrap_or(&line).to_string();
            self.arm_enter_guard(line, EnterGuardKind::Glue, GLUE_GUARD_TOAST);
        }
        if ctx.has_singleton_model::<ToastStack>() {
            let window_id = ctx.window_id();
            ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                toast_stack.add_ephemeral_toast(
                    DismissibleToast::error(toast.to_string()),
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

    /// The Enter has run. With nothing typed behind it the gate closes now; otherwise it is kept
    /// until the view has seen the route settle and asks for the release
    /// ([`Self::release_enter_gate`]). Returns whether the view has a release to do.
    pub(super) fn finish_enter_gate_after_submit(
        &mut self,
        awaits_pty: bool,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if self.unready_enter_gate_open() || self.multiline_dialog_gate_open() {
            // #1571: the Enter did not run (the shell was not ready): its gate waits for the
            // readiness event, not for this submit's route. #1652: the Enter asked the multi-line
            // dialog: its gate waits for the answer.
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
             gave up; handing them back to the input (#1577)"
        );
        self.give_up_enter_gate_to_editor("route_wait", ctx);
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
        if self.multiline_dialog_gate_open() {
            // #1652: as for the unready line: the waiting Enter is void, with what followed it.
            self.cancel_multiline_dialog("terminal_interrupt", ctx);
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

/// #1652: the toast of an answer that discarded the keys typed while the dialog was open.
pub(crate) fn dialog_typeahead_discarded_toast(keys: usize) -> String {
    if keys == 1 {
        "1 key typed while the confirmation was open was discarded".to_string()
    } else {
        format!("{keys} keys typed while the confirmation was open were discarded")
    }
}
