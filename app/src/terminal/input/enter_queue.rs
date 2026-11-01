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

use super::paste_queue::InputQueueHost;
use super::{AfterEnterInput, Event, Input};
use crate::terminal::input_queue::{EntryKind, GateKind, GateSpec, PaneQueue};

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
}

impl Input {
    fn enter_queue_handle(&self, ctx: &AppContext) -> PaneQueue {
        self.editor.as_ref(ctx).paste_hold().queue()
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
        let queue = self.enter_queue_handle(ctx);
        if queue.is_draining() {
            // This Enter is being delivered by the queue's own drain (a replayed Enter that waits
            // again): the drain cannot be re-entered. The delivering host takes the gate right
            // after this call and returns it as the delivery's `OpensGate`, which holds the
            // entries already queued behind the Enter.
            self.stashed_gate = Some(spec);
        } else {
            let Some(mut queue) = queue.try_lock() else {
                log::warn!("input queue: Enter gate not opened, the queue was busy");
                return;
            };
            let mut host = InputQueueHost::new(self, ctx);
            queue.open_gate(&mut host, spec);
        }
        self.enter_gate = Some(EnterGate {
            key: id,
            kind,
            submitted: false,
            awaits_pty: false,
            route_cap_armed: false,
        });
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
        let Some(mut queue) = queue.try_lock() else {
            return false;
        };
        let kind = match input {
            AfterEnterInput::Text(text) => EntryKind::Text(text),
            AfterEnterInput::Enter => EntryKind::Enter,
        };
        let mut host = InputQueueHost::new(self, ctx);
        queue.offer(&mut host, kind);
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
        let Some(mut queue) = queue.try_lock() else {
            log::warn!("input queue: an Enter typed behind a deferred Enter found the queue busy");
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        queue.offer(&mut host, EntryKind::Enter);
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
        let Some(mut queue) = queue.try_lock() else {
            log::warn!("input queue: voided Enter gate could not be closed, the queue was busy");
            return String::new();
        };
        let mut host = InputQueueHost::new(self, ctx);
        queue
            .close_gate_taking(&mut host, gate.kind, gate.key)
            .into_iter()
            .filter_map(|entry| match entry.kind {
                EntryKind::Text(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    /// The Enter has run. With nothing typed behind it the gate closes now; otherwise it is kept
    /// until the view has seen the route settle and asks for the release
    /// ([`Self::release_enter_gate`]). Returns whether the view has a release to do.
    pub(super) fn finish_enter_gate_after_submit(
        &mut self,
        awaits_pty: bool,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
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
        let Some(mut queue) = queue.try_lock() else {
            log::warn!("input queue: Enter gate could not be released, the queue was busy");
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        queue.on_gate_event(&mut host, gate.kind, gate.key);
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
        let queue = self.enter_queue_handle(ctx);
        let Some(mut queue) = queue.try_lock() else {
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        queue.offer(
            &mut host,
            EntryKind::Signal(crate::terminal::input_queue::SignalKind::Interrupt),
        );
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
