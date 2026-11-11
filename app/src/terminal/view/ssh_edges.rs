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
//! | `HoldStarted` / `HoldEnded` | `set_ssh_typeahead_holding`, the one place the hold flag changes |
//! | `Released` | `refresh_ssh_connect_gate` releasing the gate (an event, or a logged give-up), and a non-ssh command starting |
//!
//! The machine's phase is the one the facts carry, and the key path does not refresh the connect
//! gate: the work the refreshers once did on a key (arming the gate lazily, releasing it past its
//! ceiling, handing over a hold whose gate is gone) is done by the events above and by the give-up
//! timers, each of which is LOGGED ([`TerminalView::note_ssh_give_up`]). (The legacy derivation of
//! the phase and its `shadow` comparison, `kh_ssh_phase_disagree`, went with #1353 step 8.)
//!
//! Timers that remain, and why (every one is a logged give-up, none is the release):
//!  * the connect gate's ceiling and its post-bootstrap / post-block graces
//!    (`schedule_ssh_typeahead_give_up`, armed from events): a remote that never prints, never
//!    bootstraps or never shows a prompt must not own the keyboard;
//!  * the bootstrap-failed timer (`on_bootstrap_failed_timer_complete`, #1494);
//!  * the first-Precmd wait cap (`SSH_TYPEAHEAD_PRECMD_WAIT_CAP`).
//! Never logged: the text of a key. Counts and reasons only.
//!
//! Nothing here reads the screen: no edge is "visible text was printed" or "the line looks like a
//! prompt" any more (the latch those fed failed review four times, #1611, #1612, #1621; see
//! `DECLINED.md`).

use chrono::Local;
use instant::Instant;
use warpui::ViewContext;

use super::{TerminalView, choice_block_owns_input};
use crate::features::FeatureFlag;
use crate::terminal::input_queue::{SshEdge, SshObservation, SshPhase, SshRelease};
use crate::terminal::model::key_facts::KeyModelSnapshot;
use crate::terminal::model::session::SessionId;
use crate::terminal::model_events::{AnsiHandlerEvent, ModelEvent};
use crate::terminal::view::rich_content::RichContentMetadata;

impl TerminalView {
    /// Feeds one edge.
    pub(super) fn ssh_edge(&mut self, edge: SshEdge) {
        self.ssh_phase.apply(edge);
        // The ssh phase moved: the line pinned to a destination is over.
        if !matches!(edge, SshEdge::Observed(_) | SshEdge::BootstrapFailed) {
            self.request_queue_pin_clear();
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
        if !self.ssh_phase.is_active() && !Self::ssh_event_can_start_a_window(event) {
            return;
        }
        self.ssh_observe(ctx);
        self.ssh_arm_gate_at_event(ctx);
    }

    /// As [`Self::ssh_observe_for_event`] for an output wakeup: only while the machine has
    /// something to watch (a window, a hold or a pending session).
    pub(super) fn ssh_observe_for_wakeup(&mut self, ctx: &mut ViewContext<Self>) {
        if self.ssh_phase.is_active() {
            self.ssh_observe(ctx);
        }
    }

    /// Feeds the machine the facts of this moment.
    pub(super) fn ssh_observe(&mut self, ctx: &mut ViewContext<Self>) {
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
    }

    /// The remote-setup `SessionsEvent` (and the choice dialog being added or removed): the
    /// setup state of the pending session changed.
    pub(super) fn ssh_on_setup_changed(&mut self, ctx: &mut ViewContext<Self>) {
        self.ssh_observe(ctx);
    }

    /// An ssh recognised at preexec only (no submit saw it) arms the window HERE, at the event
    /// that shows it running, instead of lazily on the first key.
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
        if self.ssh_typeahead_holding {
            self.ssh_handover_after_release = true;
        }
    }

    /// A non-ssh command started while an unmanaged window was open: the window is stale. The
    /// hold, if any, is handed over on the next turn: the event that releases the
    /// window is the one that hands over, not the next key.
    pub(super) fn ssh_on_other_command_started(&mut self, ctx: &mut ViewContext<Self>) {
        self.note_ssh_gate_released(SshRelease::OtherCommand);
        if self.ssh_typeahead_holding {
            ctx.spawn(async {}, |me, _, ctx| {
                me.deliver_ssh_typeahead_if_ready("command_started", ctx);
            });
        }
    }

    /// The give-up timer of the connect window fired (`schedule_ssh_typeahead_give_up`).
    pub(super) fn ssh_give_up_fired(&mut self, ctx: &mut ViewContext<Self>) {
        if self.ssh_typeahead_holding {
            // Held keys: the hand-over runs the same refresh (and logs the release). This is a
            // timer's give-up: what was held is written to the pty in typed order (owner decision
            // 2026-11-09, reversing #1661), never routed through the editor's submit.
            self.ssh_give_up_in_progress = true;
            self.deliver_ssh_typeahead_if_ready("give_up", ctx);
            self.ssh_give_up_in_progress = false;
            return;
        }
        if self.ssh_connect_gate.is_some() {
            // Nothing is held, but the window must still end: no event came (and the key path no
            // longer releases it). The release is logged through `note_ssh_gate_released`.
            self.refresh_ssh_connect_gate();
            if self.ssh_connect_gate.is_some() {
                self.schedule_ssh_typeahead_give_up(ctx);
            }
        }
    }

    /// A key is about to be held for the connect but its event has not armed the
    /// window yet: the published facts already show an ssh running (the pty reader got there before
    /// the view handled `CommandBecameInteractive`). The window is armed now, by the same arming the
    /// event does, so the hold has its gate and its give-up deadline. Idempotent; a key that
    /// overtook its event is the only caller.
    pub(super) fn ssh_ensure_window_for_hold(&mut self, ctx: &mut ViewContext<Self>) {
        if self.ssh_connect_gate.is_none() {
            self.ssh_arm_gate_at_event(ctx);
        }
    }

    /// Whether an ssh command the window was never armed for (recognised at preexec only) is
    /// running according to the published facts: the lazy-arming condition of
    /// `refresh_ssh_connect_gate`, read-only. The facts of a key that overtook its event carry the
    /// phase the event will give them.
    pub(super) fn ssh_window_would_arm(&self, snapshot: &KeyModelSnapshot) -> bool {
        if self.ssh_connect_gate.is_some()
            || self.warpify_state.get_pending_ssh_host().is_none()
            || !snapshot.is_long_running()
        {
            return false;
        }
        let started_after_release = match (
            snapshot.active_block_start,
            self.ssh_connect_gate_released_at,
        ) {
            (Some(start), Some(released)) => start > released,
            _ => true,
        };
        // #1809: event-based, as in `refresh_ssh_connect_gate`: not for a block whose ssh already
        // owns the tty (its ownership event arrived before any window existed).
        let already_owned = self.ssh_owned_block_without_window == Some(snapshot.active_block_hash);
        started_after_release && !already_owned
    }

    /// The phase the facts carry. The machine owns the lifecycle (the window, the hold, the
    /// release); `observed` is what the published cells say at THIS decision (a key can overtake
    /// the output wakeup that would have told the machine, so the decision folds the fresh
    /// observation into a COPY of the machine: idempotent, and the machine itself only changes at
    /// events), with the window the event is about to open already open (`would_arm`).
    pub(super) fn ssh_phase_for_facts(
        &self,
        alt_screen: bool,
        would_arm: bool,
        observed: SshObservation,
    ) -> SshPhase {
        let mut machine = self.ssh_phase.clone();
        if would_arm && !machine.armed() {
            machine.apply(SshEdge::ConnectStart);
        }
        machine.apply(SshEdge::Observed(observed));
        machine.phase(alt_screen)
    }
}
