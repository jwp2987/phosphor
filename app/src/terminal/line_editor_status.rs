use std::time::Duration;

use crate::terminal::model::session::Sessions;
use crate::terminal::model_events::AnsiHandlerEvent;
use warpui::{Entity, ModelContext, ModelHandle, r#async::SpawnedFutureHandle};

use super::{ModelEvent, ModelEventDispatcher, shell::ShellType};

/// The give-up net for a shell whose line editor is expected to signal that it is reading
/// (bracketed-paste enable, #1208, or the `LineEditorReading` marker, #1390; tracked by
/// `ready_signal_seen`, or a ConPTY prompt hold that waits for the signal).
///
/// Owner rule (two weeks of timer bugs, #1672/#1798): the input pipeline is event-driven, and a
/// timer may only hand text back or exit -- it never opens the write gate, never writes to the pty
/// and never decides routing. So:
/// * a shell that signals opens the gate ONLY on the signal;
/// * a shell that has never signalled opens the gate on the Precmd/EndPrompt event itself, with no
///   delay: the prompt is the only evidence it gives, and the tty buffers keys for it;
/// * when the signal of a signalling shell does not come within this net, the held keys go back to
///   the editor (`LineEditorStatusEvent::SignalGaveUp`, handled by the controller and the view
///   through the existing hand-back paths) and the gate stays shut. A late signal, or the next
///   prompt hook, re-arms normally.
const LINE_EDITOR_SIGNAL_GIVE_UP: Duration = Duration::from_millis(1000);

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
    /// This session's shell has signalled at least once, so its readiness comes from the signal:
    /// the prompt hook arms the give-up net instead of opening the gate.
    ready_signal_seen: bool,
    /// A prompt hook armed the wait for the shell's ready signal and nothing has ended it: the
    /// signal (even after the give-up net fired) opens the gate.
    awaiting_signal: bool,
    /// The signal arrived but could not open the gate itself (a typeahead line is queued ahead of
    /// the hand-off, or the keys held for it are not counted yet): the next hand-off retry (an
    /// event) opens it. See [`LineEditorStatus::activate_pending_signal`].
    signal_activation_pending: bool,
    mark_line_editor_active_abort_handle: Option<SpawnedFutureHandle>,
    sessions: ModelHandle<Sessions>,
    /// ConPTY prompt hold (#1349): while set, the shell is expected to signal, so a precmd arms
    /// the give-up net (this long) instead of opening the gate on the event. It only bounds a
    /// shell that never signals; when it fires, the shell is known not to signal and this is
    /// cleared (the next prompt hook then opens the gate on the event).
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
            ready_signal_seen: false,
            awaiting_signal: false,
            signal_activation_pending: false,
            mark_line_editor_active_abort_handle: None,
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
    fn expects_ready_signal(&self) -> bool {
        self.ready_signal_seen || self.signal_wait.is_some()
    }

    /// How long the wait for the shell's signal runs before the net hands the held keys back.
    fn give_up_delay(&self) -> Duration {
        self.signal_wait.unwrap_or(LINE_EDITOR_SIGNAL_GIVE_UP)
    }

    /// Whether the shell's signal arrived but is waiting for a hand-off retry to open the gate.
    pub(super) fn signal_activation_pending(&self) -> bool {
        self.signal_activation_pending && self.awaiting_signal && !self.is_line_editor_active
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
    /// as they start a line, #1208): the wait the prompt's hook armed ends now, and this is the
    /// only thing that opens the gate for a shell that signals. Without a wait armed the signal
    /// proves nothing (a program enabling bracketed paste itself).
    pub(super) fn note_shell_reading_line(&mut self, ctx: &mut ModelContext<Self>) {
        if self.is_line_editor_active {
            // Declared active on the prompt event before the signal came: the signal proves the
            // editor reads now (only a prompt cycle is ever active; a command's start made it
            // inactive), and later prompts wait for the signal.
            self.ready_signal_seen = true;
            self.proven_reading = true;
            return;
        }
        if !self.awaiting_signal {
            return;
        }
        self.open_gate_on_signal(ctx);
    }

    /// The shell's signal opens the gate: the line editor is proven to be reading.
    fn open_gate_on_signal(&mut self, ctx: &mut ModelContext<Self>) {
        abort_if_running(self.mark_line_editor_active_abort_handle.take());
        self.awaiting_signal = false;
        self.signal_activation_pending = false;
        self.ready_signal_seen = true;
        self.proven_reading = true;
        self.is_line_editor_active = true;
        ctx.emit(LineEditorStatusEvent::Active);
    }

    /// Makes a precmd's fallback activation wait `wait` for the shell's ready signal (see
    /// `signal_wait`), or restores the default fallback delay.
    #[allow(dead_code)] // used only by tests
    pub(super) fn set_signal_wait(&mut self, wait: Option<Duration>) {
        self.signal_wait = wait;
    }

    /// The signal arrived but could not open the gate itself (a typeahead line is queued ahead of
    /// the hand-off, or the keys held for the signal are released but not counted yet): it is
    /// remembered, and the next hand-off retry -- an event, sent when the view has counted the
    /// released keys or the hand-off's counts settle -- opens the gate through
    /// [`Self::activate_pending_signal`]. No timer is involved.
    pub(super) fn expedite_pending_activation(&mut self, _ctx: &mut ModelContext<Self>) {
        if self.is_line_editor_active || !self.awaiting_signal {
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

    /// Arms the wait for the shell's signal as a precmd does for a signalling shell, without the
    /// model event.
    #[cfg(test)]
    pub(crate) fn arm_activation_for_test(&mut self, ctx: &mut ModelContext<Self>) {
        let delay = self.give_up_delay();
        self.arm_signal_give_up(delay, ctx);
    }

    /// The prompt hook as the model event runs it (any shell).
    #[cfg(test)]
    pub(crate) fn prompt_hook_for_test(&mut self, ctx: &mut ModelContext<Self>) {
        self.on_prompt_hook(ctx);
    }

    /// Marks the shell as one that has signalled before.
    #[cfg(test)]
    pub(crate) fn set_ready_signal_seen_for_test(&mut self, seen: bool) {
        self.ready_signal_seen = seen;
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

    /// A prompt's hook (Precmd, or zsh's EndPrompt) arrived with the line editor inactive. An
    /// event decides, never a timer: a shell that signals waits for its signal (the net only hands
    /// keys back); a shell that never has opens the gate on this event.
    fn on_prompt_hook(&mut self, ctx: &mut ModelContext<Self>) {
        if self.expects_ready_signal() {
            let delay = self.give_up_delay();
            self.arm_signal_give_up(delay, ctx);
            return;
        }
        // The prompt is the only evidence this shell gives; the tty buffers keys for it. Opened
        // unproven: no line-report request follows (see `is_proven_reading`).
        abort_if_running(self.mark_line_editor_active_abort_handle.take());
        self.awaiting_signal = false;
        self.signal_activation_pending = false;
        self.proven_reading = false;
        self.is_line_editor_active = true;
        ctx.emit(LineEditorStatusEvent::Active);
    }

    /// Arms the wait for the shell's ready signal, with `delay` as the give-up net. When it
    /// fires it only reports that the signal did not come (`SignalGaveUp`): the held keys go back
    /// to the editor. It never opens the gate, writes, or decides routing. The wait stays armed
    /// (`awaiting_signal`), so a late signal still opens the gate.
    fn arm_signal_give_up(&mut self, delay: Duration, ctx: &mut ModelContext<Self>) {
        abort_if_running(self.mark_line_editor_active_abort_handle.take());
        self.proven_reading = false;
        self.awaiting_signal = true;
        self.signal_activation_pending = false;

        let abort_handle = ctx.spawn_abortable(
            async move { warpui::r#async::Timer::after(delay).await },
            move |me, _, ctx| {
                warpui::input_order::log_hop("line_editor_signal_give_up", || {
                    format!("delay_ms={}", delay.as_millis())
                });
                if me.signal_wait.is_some_and(|wait| wait <= delay) {
                    log::warn!(
                        "line editor: no ready signal within {} ms of the prompt; the shell \
                         does not signal, so held keys go back to the editor and the next prompt \
                         opens on its own event",
                        delay.as_millis()
                    );
                    me.signal_wait = None;
                }
                ctx.emit(LineEditorStatusEvent::SignalGaveUp);
            },
            |_, _| (),
        );
        self.mark_line_editor_active_abort_handle = Some(abort_handle);
    }

    fn set_line_editor_inactive(&mut self, ctx: &mut ModelContext<Self>) {
        abort_if_running(self.mark_line_editor_active_abort_handle.take());
        self.proven_reading = false;
        self.awaiting_signal = false;
        self.signal_activation_pending = false;

        if !self.is_line_editor_active {
            return;
        }
        self.is_line_editor_active = false;
        ctx.emit(LineEditorStatusEvent::Inactive);
    }
}

/// Aborts the given `abort_handle` if it is `Some()`.
fn abort_if_running(abort_handle: Option<SpawnedFutureHandle>) {
    if let Some(abort_handle) = abort_handle {
        abort_handle.abort();
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LineEditorStatusEvent {
    Active,
    Inactive,
    /// A signalling shell's ready signal did not come within the give-up net: keys held for it go
    /// back to the editor. The gate stays shut; nothing is written (#1672/#1798).
    SignalGaveUp,
}

impl Entity for LineEditorStatus {
    type Event = LineEditorStatusEvent;
}

#[cfg(test)]
#[path = "line_editor_status_tests.rs"]
mod tests;
