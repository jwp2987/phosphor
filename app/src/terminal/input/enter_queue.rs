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
//! Closing the gate drains it, in order. The host does not route anything itself (a delivery must
//! not re-enter the pipeline that holds, #1222): it emits `Event::ReplayEnterEntry` per entry and
//! the view routes each one when the event arrives. An entry that then opens a new gate (a
//! replayed Enter that waits again) holds the entries after it, which were already queued as
//! events: the view offers them back to the queue (`queue_behind_enter`), so order survives.
//!
//! Timers: none here. The give-ups are the #1351 compute caps and backstops (they submit the
//! Enter, which closes the gate), the route-settle cap in the view, and a long leak net on the
//! gate itself ([`ENTER_GATE_LEAK_NET`]) so a bug can never swallow keys for good.
//!
//! `PHOSPHOR_INPUT_QUEUE_ENTER=0` keeps the legacy path (`VerdictWait::queued`,
//! `after_enter_replay`), which stays in the tree.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use warpui::{AppContext, ViewContext};

use super::paste_queue::InputQueueHost;
use super::{AfterEnterInput, Event, Input};
use crate::terminal::input_queue::kill_switch::{self, Area};
use crate::terminal::input_queue::{EntryKind, GateKind, GateSpec, InputQueue};

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

/// Whether the Enter ordering runs on the queue by default. Tests default to the legacy path
/// (the existing suite asserts its structures) and opt in per input
/// (`set_enter_queue_for_test`) or per run (`PHOSPHOR_TEST_ENTER_QUEUE=1`).
pub(super) fn enabled_by_default() -> bool {
    if cfg!(test) {
        return std::env::var("PHOSPHOR_TEST_ENTER_QUEUE").is_ok_and(|value| value == "1");
    }
    kill_switch::is_enabled(Area::Enter)
}

impl Input {
    fn enter_queue_handle(&self, ctx: &AppContext) -> Rc<RefCell<InputQueue>> {
        let hold = self.editor.as_ref(ctx).paste_hold();
        Rc::clone(hold.queue())
    }

    /// Whether the Enter ordering runs on the queue for this input: its own switch, and the
    /// editor's queue path (the editor only offers keys to the queue when that is on).
    fn enter_queue_on(&self, ctx: &AppContext) -> bool {
        self.enter_queue_enabled && self.editor.as_ref(ctx).paste_hold().uses_queue()
    }

    #[cfg(test)]
    pub(crate) fn set_enter_queue_for_test(&mut self, on: bool) {
        self.enter_queue_enabled = on;
    }

    /// Keys typed now must queue behind an Enter: its gate is open on the queue. False while the
    /// queue is mid-drain (a key applied by a delivery is part of that delivery).
    pub(crate) fn enter_queue_holds_keys(&self, ctx: &AppContext) -> bool {
        self.enter_gate.is_some_and(|gate| {
            self.enter_queue_handle(ctx)
                .try_borrow()
                .is_ok_and(|queue| queue.has_gate(gate.kind, gate.key))
        })
    }

    /// Whether the Enter ordering is on the compute-clock / input-queue model
    /// (`PHOSPHOR_INPUT_QUEUE_ENTER`).
    pub(crate) fn enter_queue_enabled(&self) -> bool {
        self.enter_queue_enabled
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
    /// completion): keys arriving from now on queue behind it. A no-op on the legacy path.
    pub(super) fn open_enter_gate(&mut self, id: u64, kind: GateKind, ctx: &mut ViewContext<Self>) {
        if !self.enter_queue_on(ctx) {
            return;
        }
        let queue = self.enter_queue_handle(ctx);
        let Ok(mut queue) = queue.try_borrow_mut() else {
            log::warn!("input queue: Enter gate not opened, the queue was busy; legacy path used");
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        queue.open_gate(
            &mut host,
            GateSpec::new(kind, id).with_cap(Some(ENTER_GATE_LEAK_NET)),
        );
        self.enter_gate = Some(EnterGate {
            key: id,
            kind,
            submitted: false,
            awaits_pty: false,
            route_cap_armed: false,
        });
    }

    /// Queues `input` behind the Enter whose gate is open. False when no gate holds the queue (the
    /// caller then uses the legacy list).
    pub(crate) fn queue_behind_enter(
        &mut self,
        input: AfterEnterInput,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if !self.enter_queue_holds_keys(ctx) {
            return false;
        }
        let queue = self.enter_queue_handle(ctx);
        let Ok(mut queue) = queue.try_borrow_mut() else {
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

    /// An Enter typed behind the one that waits: on the queue when the gate holds it, else on the
    /// wait's own list.
    pub(super) fn queue_enter_behind_wait(&mut self, ctx: &mut ViewContext<Self>) {
        if self.queue_behind_enter(AfterEnterInput::Enter, ctx) {
            return;
        }
        if let Some(wait) = self.verdict_wait.as_mut() {
            wait.queued.push(AfterEnterInput::Enter);
        }
    }

    /// How many entries wait behind the Enter's gate.
    pub(super) fn enter_gate_held_len(&self, ctx: &AppContext) -> usize {
        let Some(gate) = self.enter_gate else {
            return 0;
        };
        self.enter_queue_handle(ctx)
            .try_borrow()
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
        let Ok(mut queue) = queue.try_borrow_mut() else {
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

    /// Closes the Enter's gate and releases what waited behind it, in order: each entry becomes an
    /// `Event::ReplayEnterEntry` for the view to route.
    pub(crate) fn release_enter_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(gate) = self.enter_gate.take() else {
            return;
        };
        let queue = self.enter_queue_handle(ctx);
        let Ok(mut queue) = queue.try_borrow_mut() else {
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
        let Ok(mut queue) = queue.try_borrow_mut() else {
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
        self.set_enter_queue_for_test(true);
        self.open_enter_gate(1, GateKind::EnterVerdict, ctx);
        let queue = self.enter_queue_handle(ctx);
        {
            let mut queue = queue.borrow_mut();
            let mut host = InputQueueHost::new(self, ctx);
            queue.offer(&mut host, EntryKind::Text("x".to_string()));
        }
        self.finish_enter_gate_after_submit(true, ctx);
    }

    /// The entries behind the Enter, for tests.
    #[cfg(test)]
    pub(crate) fn enter_gate_held_len_for_test(&self, ctx: &AppContext) -> usize {
        self.enter_gate_held_len(ctx)
    }

    /// Emits the replay request of a submitted Enter whose gate holds keys.
    pub(super) fn emit_replay_after_deferred_enter(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(Event::ReplayAfterDeferredEnter);
    }
}
