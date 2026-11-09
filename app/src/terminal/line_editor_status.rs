use std::collections::HashSet;
use std::time::Duration;

use crate::terminal::model::session::{SessionId, Sessions};
use crate::terminal::model_events::AnsiHandlerEvent;
use warpui::{Entity, ModelContext, ModelHandle};

use super::{ModelEvent, ModelEventDispatcher, shell::ShellType};

// The prompt event (Precmd, or zsh's EndPrompt) ALWAYS opens the write gate (#1825): commands and
// keys the editor submitted go to the pty at the shell's prompt, exactly as for a shell that never
// signals; the tty buffers them for a line editor that is not reading yet. Nothing waits for the
// shell's own ready signal (bracketed-paste enable, #1208, or the `LineEditorReading` marker,
// #1390): a user can turn it off (`unset zle_bracketed_paste`), and a command parked behind a
// signal that never comes wedges the pane (no Preexec, so no next prompt). The signal only
// UPGRADES the prompt: `proven_reading` (the ESC i line-report request goes only to a proven
// reader, #1798) and, for a session known to signal, the hand-off's request decision, which waits
// for the signal (`awaiting_signal`) because that decision writes the request. A session whose
// prompt cycle ended with no signal is demoted (`signal_missed`): its next prompt decides at the
// event. No timer is involved anywhere on this path.

/// Model representing the status of the shell's line editor.
///
/// This model also emits events when the line editor becomes active/inactive.
pub struct LineEditorStatus {
    model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
    is_line_editor_active: bool,
    /// The shell's own ready signal ended this prompt cycle's wait (or arrived since the editor
    /// was declared active): the line editor is proven to be reading. Cleared whenever the line
    /// editor goes inactive or a prompt hook arms a new wait. The line-report request
    /// (ESC i) is written only while this is set (#1798): to a shell not reading, the tty echoes
    /// it as `^[i`.
    proven_reading: bool,
    /// Sessions whose shell has signalled (learned per session, #1823): their request decision
    /// waits for the signal of each prompt (`awaiting_signal`). The gate itself never does.
    ready_signal_seen: HashSet<Option<SessionId>>,
    /// The gate is open for this prompt but the hand-off's request decision still waits for the
    /// shell's ready signal (the session is known to signal). Cleared by the signal, by a command
    /// start (`signal_missed` then records the miss).
    awaiting_signal: bool,
    /// A prompt cycle ended (a command started) while its decision still awaited the signal: the
    /// session did not signal for it, and is demoted at its next prompt event.
    signal_missed: bool,
    /// The signal arrived but could not start the decision itself (a typeahead line is queued
    /// ahead of the hand-off, or the keys held for it are not counted yet): the next hand-off
    /// retry (an event) starts it. See [`LineEditorStatus::activate_pending_signal`].
    signal_activation_pending: bool,
    sessions: ModelHandle<Sessions>,
    /// ConPTY prompt hold (#1349): while set, the shell is expected to signal, so the hand-off's
    /// decision waits for the signal like a learned session's. Cleared when a prompt cycle ended
    /// without one (the hold's own cap hands the held keys back).
    signal_wait: Option<Duration>,

    /// `true` if the active session is zsh and precmd was received before the most recent end
    /// prompt marker.
    ///
    /// When receiving an end prompt marker in zsh, this is used as a proxy to determine if the
    /// session is bootstrapped -- the prompt markers are emitted by zsh regardless of whether or
    /// not its a Warpified session, so to in order properly signal downstream that the line editor
    /// (for Warpified sessions) is active, we must check if there was a corresponding precmd
    /// emitted prior to the end prompt marker.
    ///
    /// Precmd is always emitted before prompt markers.
    did_receive_zsh_precmd: bool,
}

impl LineEditorStatus {
    pub fn new(
        model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
        sessions: ModelHandle<Sessions>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&model_event_dispatcher, |me, event, ctx| {
            me.handle_model_event(event, ctx);
        });
        LineEditorStatus {
            model_event_dispatcher,
            is_line_editor_active: false,
            proven_reading: false,
            ready_signal_seen: HashSet::new(),
            awaiting_signal: false,
            signal_missed: false,
            signal_activation_pending: false,
            did_receive_zsh_precmd: false,
            sessions,
            signal_wait: None,
        }
    }

    pub fn is_line_editor_active(&self) -> bool {
        self.is_line_editor_active
    }

    /// Whether the shell's ready signal proved its line editor is reading at this prompt (#1798).
    /// Active without this means the fallback opened the write gate: keys may go to the pty, but
    /// the line-report request must not.
    pub fn is_proven_reading(&self) -> bool {
        self.is_line_editor_active && self.proven_reading
    }

    /// Whether this shell is expected to signal that its line editor is reading: it has before,
    /// or the ConPTY prompt hold waits for it.
    fn expects_ready_signal(&self, ctx: &ModelContext<Self>) -> bool {
        self.ready_signal_seen.contains(&self.active_session(ctx)) || self.signal_wait.is_some()
    }

    /// The session whose prompt this is. The ready signal is learned PER SESSION (#1823): the
    /// local shell signalling says nothing about the fake / remote / sub shell an `ssh` or `su`
    /// then starts in the same pane, and a session that has not signalled yet opens the gate on
    /// its prompt event like any non-signalling shell.
    fn active_session(&self, ctx: &ModelContext<Self>) -> Option<SessionId> {
        self.model_event_dispatcher.as_ref(ctx).active_session_id()
    }

    /// Whether the request decision of this prompt still waits for the shell's ready signal (the
    /// gate itself is open, #1825).
    pub(super) fn decision_awaits_signal(&self) -> bool {
        self.is_line_editor_active && self.awaiting_signal
    }

    /// Whether the shell's signal arrived but is waiting for a hand-off retry to start the decision.
    pub(super) fn signal_activation_pending(&self) -> bool {
        self.signal_activation_pending && self.awaiting_signal
    }

    /// Sets the flag `LineEditorStatusEvent::Active` would set, without the precmd timer.
    #[cfg(test)]
    pub(crate) fn set_line_editor_active_for_test(&mut self, is_active: bool) {
        self.is_line_editor_active = is_active;
        self.proven_reading = is_active;
    }

    /// Declares the line editor active the way the give-up does: the write gate is open, nothing
    /// is proven.
    #[cfg(test)]
    pub(crate) fn set_line_editor_active_unproven_for_test(&mut self) {
        self.is_line_editor_active = true;
        self.proven_reading = false;
    }

    /// Marks the line editor as inactive.
    ///
    /// This is meant to be called when a command is written to the PTY.
    pub(super) fn did_execute_command(&mut self, ctx: &mut ModelContext<Self>) {
        self.set_line_editor_inactive(ctx);
    }

    /// The shell's line editor announced that it is reading (readline/zle enable bracketed paste
    /// as they start a line, #1208; PowerShell's marker, #1390). The gate is already open (the
    /// prompt event opened it); the signal proves the reader (`proven_reading`), learns the
    /// session as a signalling one, and starts the request decision that waited for it. A signal
    /// outside a prompt cycle proves nothing (a program enabling bracketed paste itself).
    pub(super) fn note_shell_reading_line(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.is_line_editor_active {
            return;
        }
        if self.awaiting_signal {
            self.open_gate_on_signal(ctx);
            return;
        }
        // Gate opened on the event with the decision already made (a session not yet known to
        // signal): a late proof upgrades the reader and teaches the session; no second decision.
        let session = self.active_session(ctx);
        self.ready_signal_seen.insert(session);
        self.proven_reading = true;
    }

    /// The shell's signal arrived for a prompt whose decision waited for it: the line editor is
    /// proven to be reading, and the hand-off decides (the request goes now).
    fn open_gate_on_signal(&mut self, ctx: &mut ModelContext<Self>) {
        self.awaiting_signal = false;
        self.signal_activation_pending = false;
        let session = self.active_session(ctx);
        self.ready_signal_seen.insert(session);
        self.proven_reading = true;
        self.is_line_editor_active = true;
        ctx.emit(LineEditorStatusEvent::Active);
    }

    /// Makes the request decision of a prompt wait for the shell's ready signal (see
    /// `signal_wait`), or restores the default.
    pub(super) fn set_signal_wait(&mut self, wait: Option<Duration>) {
        self.signal_wait = wait;
    }

    /// The signal arrived but could not open the gate itself (a typeahead line is queued ahead of
    /// the hand-off, or the keys held for the signal are released but not counted yet): it is
    /// remembered, and the next hand-off retry -- an event, sent when the view has counted the
    /// released keys or the hand-off's counts settle -- opens the gate through
    /// [`Self::activate_pending_signal`]. No timer is involved.
    pub(super) fn expedite_pending_activation(&mut self, _ctx: &mut ModelContext<Self>) {
        if !self.awaiting_signal {
            return;
        }
        self.signal_activation_pending = true;
    }

    /// Opens the gate for a signal remembered by [`Self::expedite_pending_activation`]. Returns
    /// whether it did.
    pub(super) fn activate_pending_signal(&mut self, ctx: &mut ModelContext<Self>) -> bool {
        if !self.signal_activation_pending() {
            return false;
        }
        self.open_gate_on_signal(ctx);
        true
    }

    /// The prompt event of a shell that signals, without the model event: the gate opens, the
    /// request decision waits for the signal.
    #[cfg(test)]
    pub(crate) fn arm_activation_for_test(&mut self, ctx: &mut ModelContext<Self>) {
        self.open_gate_on_prompt(true, ctx);
    }

    /// The prompt hook as the model event runs it (any shell).
    #[cfg(test)]
    pub(crate) fn prompt_hook_for_test(&mut self, ctx: &mut ModelContext<Self>) {
        self.on_prompt_hook(ctx);
    }

    /// Marks the shell as one that has signalled before.
    #[cfg(test)]
    pub(crate) fn set_ready_signal_seen_for_test(&mut self, seen: bool, ctx: &ModelContext<Self>) {
        let session = self.active_session(ctx);
        if seen {
            self.ready_signal_seen.insert(session);
        } else {
            self.ready_signal_seen.remove(&session);
        }
    }

    /// Test: whether the active session is a learned signalling shell.
    #[cfg(test)]
    pub(crate) fn active_session_signals_for_test(&self, ctx: &ModelContext<Self>) -> bool {
        self.ready_signal_seen.contains(&self.active_session(ctx))
    }

    fn handle_model_event(&mut self, event: &ModelEvent, ctx: &mut ModelContext<Self>) {
        let Some(active_session_id) = self.model_event_dispatcher.as_ref(ctx).active_session_id()
        else {
            return;
        };

        let Some(active_session) = self.sessions.as_ref(ctx).get(active_session_id) else {
            return;
        };

        let is_active_session_zsh = active_session.shell().shell_type() == ShellType::Zsh;
        match event {
            ModelEvent::Handler(AnsiHandlerEvent::Precmd) => {
                if is_active_session_zsh {
                    self.did_receive_zsh_precmd = true;
                } else if !self.is_line_editor_active {
                    self.on_prompt_hook(ctx);
                    self.did_receive_zsh_precmd = false;
                }
            }
            ModelEvent::Handler(AnsiHandlerEvent::EndPrompt)
                if is_active_session_zsh && self.did_receive_zsh_precmd =>
            {
                if !self.is_line_editor_active {
                    self.on_prompt_hook(ctx);
                }
                self.did_receive_zsh_precmd = false;
            }
            ModelEvent::Handler(AnsiHandlerEvent::Preexec) => {
                self.set_line_editor_inactive(ctx);
            }
            _ => (),
        }
    }

    /// A prompt's hook (Precmd, or zsh's EndPrompt) arrived with the line editor inactive. The
    /// EVENT opens the write gate, always (#1825). A session known to signal then waits for its
    /// signal only for the hand-off's request decision (`GateOpened`: queued writes go now);
    /// any other session decides at the event (`Active`, unproven, as a non-signalling shell).
    fn on_prompt_hook(&mut self, ctx: &mut ModelContext<Self>) {
        let session = self.active_session(ctx);
        if self.signal_missed {
            // The previous prompt cycle of this shell ended with no signal: it is not (or no
            // longer) a signalling shell, so its decisions stop waiting for one.
            self.signal_missed = false;
            self.ready_signal_seen.remove(&session);
            self.signal_wait = None;
        }
        let waits_for_signal = self.expects_ready_signal(ctx);
        self.open_gate_on_prompt(waits_for_signal, ctx);
    }

    fn open_gate_on_prompt(
        &mut self,
        decision_waits_for_signal: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        self.signal_activation_pending = false;
        self.proven_reading = false;
        self.is_line_editor_active = true;
        if decision_waits_for_signal {
            self.awaiting_signal = true;
            ctx.emit(LineEditorStatusEvent::GateOpened);
        } else {
            self.awaiting_signal = false;
            ctx.emit(LineEditorStatusEvent::Active);
        }
    }

    fn set_line_editor_inactive(&mut self, ctx: &mut ModelContext<Self>) {
        self.proven_reading = false;
        if self.awaiting_signal {
            // The cycle ended (a command started) with no signal for its decision.
            self.signal_missed = true;
        }
        self.awaiting_signal = false;
        self.signal_activation_pending = false;

        if !self.is_line_editor_active {
            return;
        }
        self.is_line_editor_active = false;
        ctx.emit(LineEditorStatusEvent::Inactive);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LineEditorStatusEvent {
    Active,
    Inactive,
    /// The prompt event opened the write gate (queued writes may go), but the request decision of
    /// this session still waits for its ready signal: `Active` follows with the signal (#1825).
    GateOpened,
}

impl Entity for LineEditorStatus {
    type Event = LineEditorStatusEvent;
}

#[cfg(test)]
#[path = "line_editor_status_tests.rs"]
mod tests;
