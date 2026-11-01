use std::time::Duration;

use crate::terminal::model::session::Sessions;
use crate::terminal::model_events::AnsiHandlerEvent;
use warpui::{r#async::SpawnedFutureHandle, Entity, ModelContext, ModelHandle};

use super::{shell::ShellType, ModelEvent, ModelEventDispatcher};

/// The duration after a precmd/end prompt hook (depending on the shell type) to wait before
/// assuming the shell's line editor is active again. If we receive a preexec hook in that time,
/// we assume there are multiple queued typeahead commands and wait to request input.
/// This prevents Zap sending an escape sequence to an arbitrary running program.
///
/// The fallback: a shell that enables bracketed paste when its line editor starts reading ends
/// the wait at that signal instead ([`LineEditorStatus::note_shell_reading_line`], #1208).
///
/// To avoid flickering, this must be less than `BACKGROUND_OUTPUT_RENDER_DELAY_MS`,
/// which is the delay before a background block is rendered.
const LINE_EDITOR_ACTIVATION_DELAY: Duration = Duration::from_millis(50);

/// Model representing the status of the shell's line editor.
///
/// This model also emits events when the line editor becomes active/inactive.
pub struct LineEditorStatus {
    model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
    is_line_editor_active: bool,
    mark_line_editor_active_abort_handle: Option<SpawnedFutureHandle>,
    sessions: ModelHandle<Sessions>,
    /// ConPTY prompt hold (#1349): while set, a precmd's activation waits this long for the
    /// shell's bracketed-paste signal before falling back, instead of
    /// [`LINE_EDITOR_ACTIVATION_DELAY`]. The held keys are released by the signal itself, so the
    /// fallback must not fire (and start the hand-off) ahead of it. It only bounds a shell that
    /// never signals; when it fires, the shell is known not to signal and this is cleared.
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
            mark_line_editor_active_abort_handle: None,
            did_receive_zsh_precmd: false,
            sessions,
            signal_wait: None,
        }
    }

    pub fn is_line_editor_active(&self) -> bool {
        self.is_line_editor_active
    }

    /// Sets the flag `LineEditorStatusEvent::Active` would set, without the precmd timer.
    #[cfg(test)]
    pub(crate) fn set_line_editor_active_for_test(&mut self, is_active: bool) {
        self.is_line_editor_active = is_active;
    }

    /// Marks the line editor as inactive.
    ///
    /// This is meant to be called when a command is written to the PTY.
    pub(super) fn did_execute_command(&mut self, ctx: &mut ModelContext<Self>) {
        self.set_line_editor_inactive(ctx);
    }

    /// The shell's line editor announced that it is reading (readline/zle enable bracketed paste
    /// as they start a line, #1208): an activation still pending from the prompt's hook is due
    /// now, not at the end of [`LINE_EDITOR_ACTIVATION_DELAY`]. Without a pending activation the
    /// signal proves nothing (a program enabling bracketed paste itself) and the fallback stands.
    pub(super) fn note_shell_reading_line(&mut self, ctx: &mut ModelContext<Self>) {
        if self.is_line_editor_active || self.mark_line_editor_active_abort_handle.is_none() {
            return;
        }
        abort_if_running(self.mark_line_editor_active_abort_handle.take());
        self.is_line_editor_active = true;
        ctx.emit(LineEditorStatusEvent::Active);
    }

    /// Makes a precmd's fallback activation wait `wait` for the shell's ready signal (see
    /// `signal_wait`), or restores the default fallback delay.
    pub(super) fn set_signal_wait(&mut self, wait: Option<Duration>) {
        self.signal_wait = wait;
    }

    /// The signal arrived but could not end the wait itself (a typeahead line is queued ahead of
    /// the hand-off): the pending activation is brought down to the default fallback delay, as
    /// it was before the prompt hold waited for the signal.
    pub(super) fn expedite_pending_activation(&mut self, ctx: &mut ModelContext<Self>) {
        if self.is_line_editor_active
            || self.signal_wait.is_none()
            || self.mark_line_editor_active_abort_handle.is_none()
        {
            return;
        }
        self.mark_line_editor_active_after_delay(LINE_EDITOR_ACTIVATION_DELAY, ctx);
    }

    /// Arms the activation delay as a precmd does, without the model event.
    #[cfg(test)]
    pub(crate) fn arm_activation_for_test(&mut self, ctx: &mut ModelContext<Self>) {
        let delay = self.signal_wait.unwrap_or(LINE_EDITOR_ACTIVATION_DELAY);
        self.mark_line_editor_active_after_delay(delay, ctx);
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
                    let delay = self.signal_wait.unwrap_or(LINE_EDITOR_ACTIVATION_DELAY);
                    self.mark_line_editor_active_after_delay(delay, ctx);
                    self.did_receive_zsh_precmd = false;
                }
            }
            ModelEvent::Handler(AnsiHandlerEvent::EndPrompt)
                if is_active_session_zsh && self.did_receive_zsh_precmd =>
            {
                if !self.is_line_editor_active {
                    self.mark_line_editor_active_after_delay(LINE_EDITOR_ACTIVATION_DELAY, ctx);
                }
                self.did_receive_zsh_precmd = false;
            }
            ModelEvent::Handler(AnsiHandlerEvent::Preexec) => {
                self.set_line_editor_inactive(ctx);
            }
            _ => (),
        }
    }

    fn mark_line_editor_active_after_delay(
        &mut self,
        delay: Duration,
        ctx: &mut ModelContext<Self>,
    ) {
        abort_if_running(self.mark_line_editor_active_abort_handle.take());

        // For zsh, we use this heuristic -- 10ms after EndPrompt -- to approximate when the line
        // editor is active.
        let abort_handle = ctx.spawn_abortable(
            async move { warpui::r#async::Timer::after(delay).await },
            move |me, _, ctx| {
                if me.signal_wait.is_some_and(|wait| wait <= delay) {
                    log::warn!(
                        "line editor: no ready signal within {} ms of the prompt; the shell \
                         does not signal, falling back to the default activation delay",
                        delay.as_millis()
                    );
                    me.signal_wait = None;
                }
                me.is_line_editor_active = true;
                ctx.emit(LineEditorStatusEvent::Active);
            },
            |_, _| (),
        );
        self.mark_line_editor_active_abort_handle = Some(abort_handle);
    }

    fn set_line_editor_inactive(&mut self, ctx: &mut ModelContext<Self>) {
        abort_if_running(self.mark_line_editor_active_abort_handle.take());

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
}

impl Entity for LineEditorStatus {
    type Event = LineEditorStatusEvent;
}

#[cfg(test)]
#[path = "line_editor_status_tests.rs"]
mod tests;
