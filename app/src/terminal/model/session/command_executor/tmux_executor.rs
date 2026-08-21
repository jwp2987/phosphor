use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use anyhow::Result;
use async_channel::{self, Receiver, Sender};
use async_trait::async_trait;
use chrono::DateTime;
use parking_lot::Mutex;

use super::{ExecuteCommandOptions, ExecutorCommandEvent};
use crate::server::datetime_ext::DateTimeExt;
use crate::terminal::event::ExecutedExecutorCommandEvent;
use crate::terminal::model::tmux::commands::TmuxCommand;
use crate::terminal::shell::Shell;

use super::CommandExecutor;
use warp_completer::completer::{CommandExitStatus, CommandOutput};
use warp_core::command::ExitCode;
use warp_util::on_cancel::OnCancelFutureExt;

/// A `Session`-scoped executor for commands via tmux.
pub struct TmuxCommandExecutor {
    executor_command_tx: Sender<ExecutorCommandEvent>,
    /// Command id -> the sender half of the channel the awaiting `execute_command`
    /// future is parked on.
    ///
    /// Every entry must be removed on exactly one of the three ways a command can end:
    /// output arrives ([`Self::handle_executed_command_event`]), dispatch to tmux fails
    /// ([`Self::execute_command_internal`]), or the awaiting future is dropped (the
    /// `on_cancel` hook in [`CommandExecutor::execute_command`]). Holding the sender is
    /// not free bookkeeping: it is the *only* sender, so while it is in this map the
    /// channel cannot close, and the future parked on `recv()` cannot be woken by
    /// anything except a real result. A forgotten entry is therefore both a leak and a
    /// future that can never resolve.
    in_flight_commands: Arc<Mutex<HashMap<String, Sender<CommandOutput>>>>,
}

impl fmt::Debug for TmuxCommandExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TmuxCommandExecutor {{}}")
    }
}

#[cfg_attr(target_family = "wasm", allow(dead_code))]
impl TmuxCommandExecutor {
    pub fn new(executor_command_tx: Sender<ExecutorCommandEvent>) -> Self {
        Self {
            executor_command_tx,
            in_flight_commands: Default::default(),
        }
    }

    fn execute_command_internal(
        &self,
        command_id: &str,
        current_directory_path: Option<&str>,
        command: &str,
        _shell: &Shell,
        environment_variables: Option<HashMap<String, String>>,
    ) -> Result<Receiver<CommandOutput>> {
        let (output_channel_tx, output_channel_rx) = async_channel::unbounded::<CommandOutput>();

        self.in_flight_commands
            .lock()
            .insert(command_id.to_string(), output_channel_tx);

        let tmux_command = TmuxCommand::RunInBackgroundWindow {
            current_directory_path: current_directory_path.map(|s| s.to_string()),
            command_id: command_id.to_string(),
            command: command.to_string(),
            environment_variables,
        };

        if let Err(e) = self
            .executor_command_tx
            .try_send(ExecutorCommandEvent::ExecuteTmuxCommand(tmux_command))
        {
            // The command never reached tmux, so no `ExecutedExecutorCommandEvent` will
            // ever carry this id. Returning the receiver anyway parked the caller on a
            // channel whose only sender sits in `in_flight_commands` forever: a future
            // that hangs for the life of the session, behind a log line. Drop the
            // registration and report the failure.
            self.forget_command(command_id);
            log::warn!("Failed to send TmuxCommand to pty_controller: {e}");
            return Err(anyhow::anyhow!(
                "Failed to send TmuxCommand to pty_controller: {e}"
            ));
        }

        Ok(output_channel_rx)
    }

    /// Removes `command_id` from [`Self::in_flight_commands`], dropping the sender so
    /// the channel closes and any future still parked on it is released.
    fn forget_command(&self, command_id: &str) {
        self.in_flight_commands.lock().remove(command_id);
    }

    pub fn handle_executed_command_event(&self, event: ExecutedExecutorCommandEvent) {
        // `remove`, not `get`: a command reports exactly once, and this is the terminal
        // event for it. Mirrors `InBandCommandExecutor`'s `take()` on the same
        // transition.
        if let Some(output_tx) = self.in_flight_commands.lock().remove(&event.command_id) {
            if !output_tx.is_closed() {
                // We shouldn't be receiving exit codes that aren't 32 bit signed integers.
                let exit_code = Some(ExitCode::from(event.exit_code as i32));
                let command_output = if event.exit_code == 0 {
                    CommandOutput {
                        stdout: event.output,
                        stderr: vec![],
                        status: CommandExitStatus::Success,
                        exit_code,
                    }
                } else {
                    CommandOutput {
                        stdout: vec![],
                        stderr: event.output,
                        status: CommandExitStatus::Failure,
                        exit_code,
                    }
                };
                if let Err(error) = output_tx.try_send(command_output) {
                    log::error!("Error occurred when sending generator command output: {error}");
                }
            }
        }
    }
}

#[async_trait]
impl CommandExecutor for TmuxCommandExecutor {
    /// Executes `command` while attached to an active tmux control mode session.
    /// Runs the command in a background tmux window.
    async fn execute_command(
        &self,
        command: &str,
        shell: &Shell,
        current_directory_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
        _execute_command_options: ExecuteCommandOptions,
    ) -> Result<CommandOutput> {
        let command_id = DateTime::now().timestamp_micros().to_string();

        // Generator commands (completions, autosuggestions, decorations) are aborted on
        // the next keystroke, so the awaiting future being dropped is the *common* way a
        // command ends here, not an edge case. Without this hook the registration
        // outlives every one of them and `in_flight_commands` grows for the life of the
        // session. Same shape as `InBandCommandExecutor::execute_command`'s `on_cancel`.
        let future = async {
            let output_channel_rx = self.execute_command_internal(
                command_id.as_str(),
                current_directory_path,
                command,
                shell,
                environment_variables,
            )?;
            output_channel_rx.recv().await.map_err(anyhow::Error::from)
        }
        .on_cancel(|| self.forget_command(command_id.as_str()));

        future.await
    }

    // `cancel_active_commands` is deliberately NOT overridden; the trait's no-op default
    // is the honest answer here, and an override would be worse than nothing.
    //
    // Nothing this executor can do cancels the work. A command runs in a detached tmux
    // background window and `TmuxCommand` has no variant that kills one, so an override
    // could only clear the local map. That buys no correctness: each command owns a
    // private channel, so a late result cannot be delivered to the wrong consumer the way
    // it can for `InBandCommandExecutor` (whose override exists precisely to make a stale
    // `handle_executed_command_event` a no-op against a shared queue).
    //
    // It would also cost something real. `cancel_active_commands` is session-global and
    // fires when the user presses Enter (`terminal/view.rs`, `InputEvent::ExecuteCommand`),
    // so clearing the map there would close the channel under every in-flight generator
    // command and turn each one into a failed probe — the failure mode already recorded
    // against the local executor in `session.rs`'s issue #616 comment. The three abort
    // paths that actually want a command forgotten (autosuggestions, completions,
    // decorations) drop their futures, which the `on_cancel` hook above already handles.

    fn supports_parallel_command_execution(&self) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
#[path = "tmux_executor_tests.rs"]
mod tests;
