use crate::persistence::ModelEvent;
use crate::terminal::line_editor_status::LineEditorStatus;
use crate::features::FeatureFlag;
#[cfg(not(target_family = "wasm"))]
use crate::terminal::model::escape_sequences;
use crate::terminal::model::lifecycle::StartCommandOutcome;
use crate::terminal::model::session::{ExecutorCommandEvent, Sessions};

use crate::terminal::model::terminal_model::ExitReason;
use crate::terminal::view;
use crate::terminal::writeable_pty::command_history::update_command_history;
use crate::terminal::writeable_pty::pty_controller::{EventLoopSender, PASTE_WRITE_MIN_BYTES};
use crate::terminal::writeable_pty::terminal_surface::{PtyIntent, PtyIntentEvent, TerminalSurface};
use crate::terminal::writeable_pty::{PtyController, PtyControllerEvent};
use crate::terminal::ModelEventDispatcher;
use crate::terminal::{TerminalModel, TerminalView};

use async_channel::Receiver;
use parking_lot::FairMutex;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use warpui::{AppContext, Entity, ModelHandle, ViewHandle};

/// #1125: the input already cleared the line when it submitted it, so a command the controller
/// did not start (another one waiting, one running, a terminated session) must never vanish:
/// say so, and give a user's own line back to the editor.
pub(crate) fn handle_command_not_run<S: TerminalSurface>(
    surface: &warpui::WeakViewHandle<S>,
    event: &view::ExecuteCommandEvent,
    outcome: StartCommandOutcome,
    ctx: &mut AppContext,
) where
    <S as Entity>::Event: PtyIntentEvent,
{
    if outcome.is_accepted() {
        return;
    }
    if let Some(surface) = surface.upgrade(ctx) {
        surface.update(ctx, |surface, ctx| {
            surface.on_command_coalesced(&event.command, ctx);
            if matches!(
                event.source,
                crate::terminal::input::CommandExecutionSource::User
            ) {
                surface.restore_user_command(&event.command, ctx);
            }
        });
    }
}

/// Wires up bi-directional communication between the PtyController and the TerminalView.
/// Note that this interaction can't live in the TerminalView because the view must be manager-agnostic.
///
/// NOTE: we cannot simply use the strong references (the handle arguments to this wire_up fn)
/// in the subscription callbacks because that will create a reference cycle. Instead,
/// we should use weak handles and upgrade them lazily.
pub fn wire_up_pty_controller_with_surface<T: EventLoopSender, S: TerminalSurface>(
    pty_controller: &ModelHandle<PtyController<T>>,
    surface: &ViewHandle<S>,
    model: Arc<FairMutex<TerminalModel>>,
    sessions: ModelHandle<Sessions>,
    model_event_sender: Option<SyncSender<ModelEvent>>,
    ctx: &mut AppContext,
) where
    <S as Entity>::Event: PtyIntentEvent,
{
    let controller_weak_handle = pty_controller.downgrade();
    let surface_weak_handle = surface.downgrade();
    let surface_weak_handle_for_intents = surface.downgrade();
    let model_clone = model.clone();

    ctx.subscribe_to_view(surface, move |_surface, event, ctx| {
        // NOTE: we cannot simply use the strong reference (the model handle argument to this wire_up fn)
        // because it'll create a reference cycle with the `subscribe_to_model` usage below. Instead,
        // we should use a weak handle and upgrade it lazily.
        let Some(controller) = controller_weak_handle.upgrade(ctx) else {
            return;
        };

        // Collapse the concrete surface event down to the narrow PTY vocabulary
        // so this wiring stays agnostic to the surface type (GUI view vs TUI).
        let Some(intent) = event.pty_intent() else {
            return;
        };

        match intent {
            PtyIntent::CtrlD => {
                controller.update(ctx, |controller, ctx| {
                    controller.write_end_of_transmission_char(ctx);
                });
            }
            // Interrupt the foreground command with a ^C (ETX) written straight to the pty,
            // as the GUI's `TerminalView::ctrl_c` does. The TUI surface projects this (#875);
            // before, it was a no-op there, so ctrl-c and agent stop never reached the command.
            #[cfg(not(target_family = "wasm"))]
            PtyIntent::Interrupt => {
                controller.update(ctx, |controller, ctx| {
                    controller.write_bytes(&[escape_sequences::C0::ETX][..], ctx);
                });
            }
            PtyIntent::SigintForeground => {
                controller.update(ctx, |controller, ctx| {
                    controller.sigint_foreground_or_write_etx(ctx);
                });
            }
            PtyIntent::ShutdownPty => {
                controller.update(ctx, |controller, ctx| {
                    controller.shutdown_pty(ctx);
                });
            }
            PtyIntent::DiscardPendingCommandWrites => {
                controller.update(ctx, |controller, _| {
                    controller.discard_pending_command_writes();
                });
            }
            PtyIntent::DiscardHeldTypeaheadCommand => {
                controller.update(ctx, |controller, _| {
                    controller.discard_held_handoff_command();
                });
            }
            PtyIntent::WriteBytes(bytes) => {
                controller.update(ctx, |controller, ctx| {
                    // TODO: the underlying bytes should be wrapped in an Arc and copied out only when they need to be written to the PTY.
                    if bytes.len() >= PASTE_WRITE_MIN_BYTES {
                        controller.write_paste_bytes(bytes, ctx);
                    } else {
                        controller.write_bytes(bytes, ctx);
                    }
                });
            }
            PtyIntent::WriteLineClearSetup(payload) => {
                controller.update(ctx, |controller, ctx| {
                    controller.write_line_clear_setup(payload, ctx);
                });
            }
            PtyIntent::CancelPacedSetupWrites => {
                controller.update(ctx, |controller, ctx| {
                    controller.cancel_paced_writes(ctx);
                });
            }
            PtyIntent::WriteHandoffUserBytes(bytes) => {
                controller.update(ctx, |controller, ctx| {
                    controller.write_handoff_user_bytes(bytes, ctx);
                });
            }
            PtyIntent::RetryTypeaheadHandoffRequest => {
                controller.update(ctx, |controller, ctx| {
                    controller.retry_typeahead_handoff_request(ctx);
                });
            }
            PtyIntent::WriteAgentInput { bytes, mode } => {
                controller.update(ctx, |controller, ctx| {
                    controller.write_agent_bytes(bytes, &mode, ctx);
                });
            }
            PtyIntent::Resize(size_update) => {
                controller.update(ctx, |controller, ctx| {
                    controller.resize_pty(size_update, ctx);
                });
            }
            PtyIntent::ExecuteCommand(event) => {
                let Some(shell_type) = sessions
                    .as_ref(ctx)
                    .get(event.session_id)
                    .map(|s| s.shell().shell_type())
                else {
                    log::warn!("Failed to get shell type for the session associated to the command execution event.");
                    return;
                };

                if FeatureFlag::TypeaheadHandoff.is_enabled() {
                    let workflow_id = event.workflow_id;
                    let outcome = controller.update(ctx, |controller, ctx| {
                        controller.write_command_annotating_block(
                            &event.command,
                            shell_type,
                            event.source.clone(),
                            move |model| {
                                model
                                    .block_list_mut()
                                    .active_block_mut()
                                    .set_cloud_workflow_state(workflow_id)
                            },
                            ctx,
                        )
                    });
                    handle_command_not_run(&surface_weak_handle_for_intents, &event, outcome, ctx);
                } else {
                    model_clone.lock().block_list_mut().active_block_mut().set_cloud_workflow_state(event.workflow_id);
                    let outcome = controller.update(ctx, |controller, ctx| {
                        controller.write_command(&event.command, shell_type, event.source.clone(), ctx)
                    });
                    handle_command_not_run(&surface_weak_handle_for_intents, &event, outcome, ctx);
                }

                if event.should_add_command_to_history {
                    update_command_history(
                        &event,
                        &model_clone,
                        model_event_sender.as_ref(),
                        &sessions,
                        ctx,
                    );
                }
            }
            PtyIntent::RunNativeShellCompletions { buffer_text, results_tx  } => {
                controller.update(ctx, |controller, ctx| {
                    controller.run_native_shell_completions(buffer_text, results_tx, ctx);
                });
            }
        }
    });

    ctx.subscribe_to_model(pty_controller, move |_pty_controller, event, ctx| {
        let Some(surface) = surface_weak_handle.upgrade(ctx) else {
            return;
        };

        match event {
            PtyControllerEvent::PtyDisconnected => surface.update(ctx, |_, _| {
                model.lock().exit(ExitReason::PtyDisconnected);
            }),
            PtyControllerEvent::TypeaheadHandoffChanged => {
                surface.update(ctx, |surface, ctx| {
                    surface.on_typeahead_handoff_changed(ctx)
                });
            }
            PtyControllerEvent::PromptHoldReleased { hold_id } => {
                let hold_id = *hold_id;
                surface.update(ctx, |surface, ctx| {
                    surface.on_prompt_hold_released(hold_id, ctx)
                });
            }
        }
    });
}

/// Wires up bi-directional communication between the RemoteServerController and the TerminalView.
/// This handles the flow to render the remote server block and forward the user's choice to the controller.
///
/// Note that this interaction can't live in the TerminalView because the view must be manager-agnostic.
///
/// NOTE: we cannot simply use the strong references (the handle arguments to this wire_up fn)
/// in the subscription callbacks because that will create a reference cycle. Instead,
/// we should use weak handles and upgrade them lazily.
#[cfg(not(target_family = "wasm"))]
pub fn wire_up_remote_server_controller_with_view<T: EventLoopSender>(
    remote_server_controller: &ModelHandle<
        super::remote_server_controller::RemoteServerController<T>,
    >,
    terminal_view: &ViewHandle<TerminalView>,
    ctx: &mut AppContext,
) {
    let controller_weak = remote_server_controller.downgrade();
    ctx.subscribe_to_view(terminal_view, move |_view, event, ctx| {
        let Some(controller) = controller_weak.upgrade(ctx) else {
            return;
        };
        match event {
            view::Event::RemoteServerInstallRequested { session_id } => {
                controller.update(ctx, |ctrl, ctx| {
                    ctrl.handle_ssh_remote_server_install(*session_id, ctx);
                });
            }
            view::Event::RemoteServerSkipRequested { session_id } => {
                controller.update(ctx, |ctrl, ctx| {
                    ctrl.handle_ssh_remote_server_skip(*session_id, ctx);
                });
            }
            _ => {}
        }
    });
}

/// Creates a PtyController to broker writes to the PTY and registers it as a model.
pub fn init_pty_controller_model<Sender: EventLoopSender>(
    event_loop_tx: Sender,
    executor_command_rx: Receiver<ExecutorCommandEvent>,
    model_events: ModelHandle<ModelEventDispatcher>,
    sessions: ModelHandle<Sessions>,
    model: Arc<FairMutex<TerminalModel>>,
    ctx: &mut AppContext,
) -> ModelHandle<PtyController<Sender>> {
    let line_editor_status =
        ctx.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));

    ctx.add_model(|ctx| {
        PtyController::new(
            event_loop_tx,
            model_events,
            line_editor_status,
            sessions,
            executor_command_rx,
            model,
            ctx,
        )
    })
}

/// Creates a [`RemoteServerController`] that orchestrates the SSH init flow.
#[cfg(not(target_family = "wasm"))]
pub fn init_remote_server_controller<Sender: EventLoopSender>(
    pty_controller: &ModelHandle<PtyController<Sender>>,
    model_events: &ModelHandle<ModelEventDispatcher>,
    ctx: &mut AppContext,
) -> ModelHandle<super::remote_server_controller::RemoteServerController<Sender>> {
    let pty_weak = pty_controller.downgrade();
    ctx.add_model(|ctx| {
        super::remote_server_controller::RemoteServerController::new(
            pty_weak,
            model_events.clone(),
            ctx,
        )
    })
}
