//! The view's side of the edge-driven ssh phase (#1424, design step 7).
//!
//! `input_queue/ssh_phase.rs` is the pure machine. This file is where the edges come from. Each
//! one is sent by the event that is the edge, never by a key:
//!
//! | edge | sent from |
//! |---|---|
//! | `Submitted` | `note_command_submitted_for_ssh_gate` (the user's Enter) |
//! | `ConnectStart` | `refresh_ssh_connect_gate` arming the window for an ssh recognised at preexec only (run at the model event, `ssh_arm_gate_at_event`) |
//! | `TtyReclaimed` | `ssh_apply_wrapper_announcement`: the wrapper announced itself after `TtyOwned` for the window (#1641) |
//! | `TtyOwned` | `ssh_on_tty_owned`: `CommandBecameInteractive` for the window's block (ssh owns the tty; the held keys are delivered to it, or discarded by the termios guard, there) (#1611, #1612, #1621); under the ssh wrapper deferred to its `PlainSSHFallback` hook and never sent for a managed session (`ssh_on_wrapper_verdict`, #1641) |
//! | `Observed` | [`TerminalView::ssh_observe`]: every model event and output wakeup while a window is open or a session is pending, the remote-setup `SessionsEvent`, and the choice dialog being added or removed |
//! | `BlockCompleted` | `apply_block_completed_to_ssh_gate` |
//! | `BootstrapFailed` | `on_bootstrap_failed_timer_complete` (#1494's path) |
//! | `SwallowGiveUp` | the one-shot swallow ceiling timer ([`TerminalView::ssh_swallow_give_up`]) |
//! | `HoldStarted` / `HoldEnded` | `set_ssh_typeahead_holding`, the one place the hold flag changes |
//! | `Released` | `refresh_ssh_connect_gate` releasing the gate (an event, or a logged give-up), and a non-ssh command starting |
//!
//! In `shadow` mode the machine runs next to the legacy derivation and a difference is reported
//! as `kh_ssh_phase_disagree` (debug, `input_order` target) and counted. In `table` mode the
//! machine's phase is the one the facts carry, and the key path no longer refreshes the connect
//! gate: the work those refreshers did on a key (arming the gate lazily, releasing it past its
//! ceiling, starting the swallow clock, handing over a hold whose gate is gone) is done by the
//! events above and by the give-up timers, each of which is LOGGED ([`TerminalView::
//! note_ssh_give_up`]).
//!
//! Timers that remain, and why (every one is a logged give-up, none is the release):
//!  * the connect gate's ceiling and its post-bootstrap / post-block graces
//!    (`schedule_ssh_typeahead_give_up`, armed from events): a remote that never prints, never
//!    bootstraps or never shows a prompt must not own the keyboard;
//!  * the bootstrap swallow ceiling ([`TerminalView::ssh_swallow_give_up`]);
//!  * the bootstrap-failed timer (`on_bootstrap_failed_timer_complete`, #1494);
//!  * the first-Precmd wait cap (`SSH_TYPEAHEAD_PRECMD_WAIT_CAP`).
//! Never logged: the text of a key. Counts and reasons only.
//!
//! Nothing here reads the screen: no edge is "visible text was printed" or "the line looks like a
//! prompt" any more (the latch those fed failed review four times, #1611, #1612, #1621; see
//! `DECLINED.md`).

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Local;
use instant::Instant;
use warpui::ViewContext;
use warpui::r#async::Timer;

use super::{
    SSH_BOOTSTRAP_SWALLOW_CEILING, SSH_CONNECT_SILENCE_CEILING, TerminalView,
    choice_block_owns_input, ssh_bootstrap_swallow_active,
};
use crate::features::FeatureFlag;
use crate::terminal::input_queue::{
    DestinationMode, SshEdge, SshObservation, SshPhase, SshRelease, destination_mode,
};
use crate::terminal::model::key_facts::KeyModelSnapshot;
use crate::terminal::model::session::SessionId;
use crate::terminal::model_events::{AnsiHandlerEvent, ModelEvent};
use crate::terminal::view::rich_content::RichContentMetadata;

/// How often the machine and the legacy derivation gave different phases (shadow mode). A test
/// reads the difference around the code it runs; the log line carries the detail.
static SSH_PHASE_DISAGREEMENTS: AtomicU64 = AtomicU64::new(0);

/// Disagreements counted so far (process wide, like the other shadow counters).
#[cfg(test)]
pub(super) fn ssh_phase_disagreements() -> u64 {
    SSH_PHASE_DISAGREEMENTS.load(Ordering::Relaxed)
}

impl TerminalView {
    /// The machine is fed in every mode but `legacy` (the kill switch turns all of the table off).
    fn ssh_machine_runs() -> bool {
        destination_mode() != DestinationMode::Legacy
    }

    /// Whether the key path is free of the ssh refreshers: `table` mode only.
    pub(super) fn ssh_events_own_the_gate() -> bool {
        destination_mode() == DestinationMode::Table
    }

    /// Feeds one edge.
    pub(super) fn ssh_edge(&mut self, edge: SshEdge) {
        if Self::ssh_machine_runs() {
            self.ssh_phase.apply(edge);
            // The ssh phase moved: the line pinned to a destination is over.
            if !matches!(
                edge,
                SshEdge::Observed(_) | SshEdge::BootstrapFailed | SshEdge::SwallowGiveUp
            ) {
                self.request_queue_pin_clear();
            }
        }
    }

    /// Whether the remote-setup choice dialog belongs to the pending session (#976): the blocks of
    /// abandoned connects never own input.
    pub(super) fn ssh_dialog_up_in(
        &self,
        pending: Option<SessionId>,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        choice_block_owns_input(
            self.rich_content_views
                .iter()
                .filter_map(|view| match view.metadata() {
                    Some(RichContentMetadata::SshRemoteServerChoiceBlock { handle }) => {
                        Some(handle.as_ref(ctx).session_id())
                    }
                    _ => None,
                }),
            pending,
        )
    }

    /// Whether the pending session's remote-server setup is in flight (the state the
    /// `SessionsEvent` announces).
    pub(super) fn ssh_setup_in_flight_in(
        &self,
        pending: Option<SessionId>,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        FeatureFlag::SshRemoteServer.is_enabled()
            && pending.is_some_and(|sid| {
                self.sessions
                    .as_ref(ctx)
                    .remote_server_setup_state(sid)
                    .is_some_and(|state| state.is_in_progress())
            })
    }

    /// Whether `event` can change what the machine observes while it is idle: an ssh hook, a
    /// remote shell announcing itself, a command starting, a program becoming interactive.
    fn ssh_event_can_start_a_window(event: &ModelEvent) -> bool {
        matches!(
            event,
            ModelEvent::SSH(_)
                | ModelEvent::SshInitShell { .. }
                | ModelEvent::CommandBecameInteractive { .. }
                | ModelEvent::Handler(
                    AnsiHandlerEvent::InitShell { .. } | AnsiHandlerEvent::Preexec
                )
        )
    }

    /// The facts at a model event (#1424 step 7): feeds the machine what the published cells and
    /// the view's rich content say now. Idempotent, never blocks (`model_facts_nonblocking`), never
    /// mutates anything but the machine; an idle machine is fed only by the events that can start
    /// a window.
    pub(super) fn ssh_observe_for_event(
        &mut self,
        event: &ModelEvent,
        ctx: &mut ViewContext<Self>,
    ) {
        if !Self::ssh_machine_runs() {
            return;
        }
        if !self.ssh_phase.is_active() && !Self::ssh_event_can_start_a_window(event) {
            return;
        }
        self.ssh_observe(ctx);
        if Self::ssh_events_own_the_gate() {
            self.ssh_arm_gate_at_event(ctx);
        }
    }

    /// As [`Self::ssh_observe_for_event`] for an output wakeup: only while the machine has
    /// something to watch (a window, a hold or a pending session).
    pub(super) fn ssh_observe_for_wakeup(&mut self, ctx: &mut ViewContext<Self>) {
        if Self::ssh_machine_runs() && self.ssh_phase.is_active() {
            self.ssh_observe(ctx);
        }
    }

    /// Feeds the machine the facts of this moment.
    pub(super) fn ssh_observe(&mut self, ctx: &mut ViewContext<Self>) {
        if !Self::ssh_machine_runs() {
            return;
        }
        let read = self.model_facts_nonblocking();
        let pending = read.snapshot.pending_session_id;
        let observation = SshObservation {
            pending_session: pending.is_some(),
            hook_seen: read.snapshot.has_pending_ssh_session,
            dialog_up: self.ssh_dialog_up_in(pending, ctx),
            setup_in_flight: self.ssh_setup_in_flight_in(pending, ctx),
            secret_pass_through: self.ssh_wrapper_secret_pass_through(),
        };
        self.ssh_phase.apply(SshEdge::Observed(observation));
        if Self::ssh_events_own_the_gate() {
            self.ssh_note_swallow_clock(pending, ctx);
        }
    }

    /// The remote-setup `SessionsEvent` (and the choice dialog being added or removed): the
    /// setup state of the pending session changed.
    pub(super) fn ssh_on_setup_changed(&mut self, ctx: &mut ViewContext<Self>) {
        self.ssh_observe(ctx);
    }

    /// An ssh recognised at preexec only (no submit saw it) arms the window HERE, at the event
    /// that shows it running, instead of lazily on the first key (table mode).
    pub(super) fn ssh_arm_gate_at_event(&mut self, ctx: &mut ViewContext<Self>) {
        if self.ssh_connect_gate.is_none() {
            let snapshot = self.model_facts_nonblocking().snapshot;
            let ssh_running =
                self.warpify_state.get_pending_ssh_host().is_some() && snapshot.is_long_running();
            if ssh_running {
                // `refresh_ssh_connect_gate` arms it (and sends `ConnectStart`).
                self.refresh_ssh_connect_gate();
            }
        }
        // The window's own give-up deadline, armed with the window: a window nothing holds keys
        // behind must still end.
        if self.ssh_connect_gate.is_some() {
            self.schedule_ssh_typeahead_give_up(ctx);
        }
    }

    /// The bootstrap swallow clock starts when the remote session is announced (an event), not at
    /// the first key, and its ceiling is a one-shot give-up (table mode).
    fn ssh_note_swallow_clock(&mut self, pending: Option<SessionId>, ctx: &mut ViewContext<Self>) {
        let tracked_before = self.ssh_bootstrap_swallow_started.map(|(sid, _)| sid);
        let for_swallow = if self.ssh_phase.armed() {
            pending
        } else {
            None
        };
        let _ = ssh_bootstrap_swallow_active(
            for_swallow,
            self.ssh_bootstrap_failed_session,
            &mut self.ssh_bootstrap_swallow_started,
            Instant::now(),
        );
        let tracked_now = self.ssh_bootstrap_swallow_started.map(|(sid, _)| sid);
        if let Some(sid) = tracked_now
            && tracked_before != Some(sid)
        {
            ctx.spawn(
                Timer::after(SSH_BOOTSTRAP_SWALLOW_CEILING),
                move |me, _, _| {
                    me.ssh_swallow_give_up(sid);
                },
            );
        }
    }

    /// The swallow ceiling of `sid` passed with the session still pending: keys are no longer
    /// dropped on its behalf. A logged give-up.
    fn ssh_swallow_give_up(&mut self, sid: SessionId) {
        let still_swallowing = self
            .ssh_bootstrap_swallow_started
            .map(|(tracked, _)| tracked)
            == Some(sid)
            && self.ssh_bootstrap_failed_session != Some(sid)
            && self.model_facts_nonblocking().snapshot.pending_session_id == Some(sid);
        if !still_swallowing {
            return;
        }
        self.note_ssh_give_up("swallow_ceiling");
        self.ssh_edge(SshEdge::SwallowGiveUp);
    }

    /// A safety net fired where an event should have: counted and logged with its reason (the
    /// tests assert none fires on the event path; the logs show which net caught a bad case).
    pub(super) fn note_ssh_give_up(&mut self, why: &'static str) {
        self.ssh_give_ups += 1;
        log::warn!(
            "ssh typeahead: gave up waiting for an event ({why}); the release came from a safety \
             net, not from the event that normally ends it"
        );
        warpui::input_order::log_key_hop("kh_ssh_give_up", || {
            format!("why={why} held_bytes={}", self.ssh_held_bytes())
        });
    }

    /// The connect gate was released: tells the machine, and logs a release that was a give-up.
    pub(super) fn note_ssh_gate_released(&mut self, how: SshRelease) {
        warpui::input_order::log_hop("ssh_gate_released", || format!("how={}", how.name()));
        if how.is_give_up() {
            self.note_ssh_give_up(how.name());
        }
        self.ssh_edge(SshEdge::Released(how));
        // A release with no handler of its own to hand over (an agent write's lazy refresh, an
        // interrupt check) leaves the keys held behind a closed window: the next key or turn
        // hands them over (never a timer).
        if Self::ssh_events_own_the_gate() && self.ssh_typeahead_holding {
            self.ssh_handover_after_release = true;
        }
    }

    /// A non-ssh command started while an unmanaged window was open: the window is stale. The
    /// hold, if any, is handed over on the next turn (table mode): the event that releases the
    /// window is the one that hands over, not the next key.
    pub(super) fn ssh_on_other_command_started(&mut self, ctx: &mut ViewContext<Self>) {
        self.note_ssh_gate_released(SshRelease::OtherCommand);
        if Self::ssh_events_own_the_gate() && self.ssh_typeahead_holding {
            ctx.spawn(async {}, |me, _, ctx| {
                me.deliver_ssh_typeahead_if_ready("command_started", ctx);
            });
        }
    }

    /// The give-up timer of the connect window fired (`schedule_ssh_typeahead_give_up`).
    pub(super) fn ssh_give_up_fired(&mut self, ctx: &mut ViewContext<Self>) {
        if self.ssh_typeahead_holding {
            // Held keys: the hand-over runs the same refresh (and logs the release). #1577: this
            // is a timer, so what it hands over comes back as text; it never reaches a pty.
            self.ssh_give_up_in_progress = true;
            self.deliver_ssh_typeahead_if_ready("give_up", ctx);
            self.ssh_give_up_in_progress = false;
            return;
        }
        if Self::ssh_events_own_the_gate() && self.ssh_connect_gate.is_some() {
            // Nothing is held, but the window must still end: no event came (and the key path no
            // longer releases it). The release is logged through `note_ssh_gate_released`.
            self.refresh_ssh_connect_gate();
            if self.ssh_connect_gate.is_some() {
                self.schedule_ssh_typeahead_give_up(ctx);
            }
        }
    }

    /// A key is about to be held for the connect (`table` mode) but its event has not armed the
    /// window yet: the published facts already show an ssh running (the pty reader got there before
    /// the view handled `CommandBecameInteractive`). The window is armed now, by the same arming the
    /// event does, so the hold has its gate and its give-up deadline. Idempotent; a key that
    /// overtook its event is the only caller.
    pub(super) fn ssh_ensure_window_for_hold(&mut self, ctx: &mut ViewContext<Self>) {
        if Self::ssh_events_own_the_gate() && self.ssh_connect_gate.is_none() {
            self.ssh_arm_gate_at_event(ctx);
        }
    }

    /// Whether an ssh command the window was never armed for (recognised at preexec only) is
    /// running according to the published facts: the lazy-arming condition of
    /// `refresh_ssh_connect_gate`, read-only. The facts of a key that overtook its event carry the
    /// phase the event will give them (`table` mode).
    pub(super) fn ssh_window_would_arm(&self, snapshot: &KeyModelSnapshot) -> bool {
        if self.ssh_connect_gate.is_some()
            || self.warpify_state.get_pending_ssh_host().is_none()
            || !snapshot.is_long_running()
        {
            return false;
        }
        let age = snapshot
            .active_block_start
            .and_then(|start| Local::now().signed_duration_since(start).to_std().ok());
        let started_after_release = match (
            snapshot.active_block_start,
            self.ssh_connect_gate_released_at,
        ) {
            (Some(start), Some(released)) => start > released,
            _ => true,
        };
        started_after_release && age.is_some_and(|age| age < SSH_CONNECT_SILENCE_CEILING)
    }

    /// The phase the facts carry. The machine owns the lifecycle (the window, the hold, the
    /// release); `observed` is what the published cells say at THIS decision (a key can overtake
    /// the output wakeup that would have told the machine, so the decision folds the fresh
    /// observation into a COPY of the machine: idempotent, and the machine itself only changes at
    /// events). In `table` mode that phase is the one the facts carry, with the window the event
    /// is about to open already open (`would_arm`); in `shadow` mode the legacy derivation
    /// (`derived`) is carried and a difference is reported (`kh_ssh_phase_disagree`).
    pub(super) fn ssh_phase_for_facts(
        &self,
        derived: SshPhase,
        alt_screen: bool,
        would_arm: bool,
        observed: SshObservation,
    ) -> SshPhase {
        let mode = destination_mode();
        if mode == DestinationMode::Legacy {
            return derived;
        }
        let mut machine = self.ssh_phase.clone();
        if mode == DestinationMode::Table && would_arm && !machine.armed() {
            machine.apply(SshEdge::ConnectStart);
        }
        machine.apply(SshEdge::Observed(observed));
        let machine = machine.phase(alt_screen);
        if mode == DestinationMode::Table {
            return machine;
        }
        if machine != derived {
            SSH_PHASE_DISAGREEMENTS.fetch_add(1, Ordering::Relaxed);
            warpui::input_order::log_key_hop("kh_ssh_phase_disagree", || {
                format!(
                    "legacy={} machine={}",
                    phase_name(derived),
                    phase_name(machine)
                )
            });
        }
        derived
    }
}

/// A phase for the log.
fn phase_name(phase: SshPhase) -> &'static str {
    match phase {
        SshPhase::Idle => "idle",
        SshPhase::Holding => "holding",
    }
}
