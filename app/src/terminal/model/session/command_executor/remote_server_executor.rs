use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::remote_server::client::RemoteServerClient;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use warp_completer::completer::{CommandExitStatus, CommandOutput};
use warp_core::command::ExitCode;
use warp_core::SessionId;

use crate::remote_server::proto::run_command_response;
use crate::terminal::model::session::command_executor::{CommandExecutor, ExecuteCommandOptions};
use crate::terminal::shell::Shell;

/// A remote generator/background command slower than this is worth a log line (#1146): it
/// shares the ssh connection with the user's keystrokes (#985).
const REMOTE_COMMAND_SLOW: std::time::Duration = std::time::Duration::from_secs(2);
static SLOW_REMOTE_COMMAND: warp_util::slow_log::SlowLog = warp_util::slow_log::SlowLog::new(
    "remote command (remote-server executor)",
    REMOTE_COMMAND_SLOW,
    warp_util::slow_log::DEFAULT_WINDOW,
);

/// `CommandExecutor` implementation that executes commands via a persistent
/// `warp remote-server` process running on the remote host over SSH.
///
/// The executor is always constructed with a live `RemoteServerClient` that
/// was obtained from [`crate::remote_server::manager::RemoteServerManager`]
/// after the session reached the `Connected` state. The manager owns the
/// authoritative per-session client; this executor holds a cloned `Arc` to
/// the same underlying channels and transitively keeps them alive as long
/// as the `Session` is alive.
///
/// If the underlying SSH connection is torn down mid-session,
/// [`RemoteServerClient::run_command`] will fail naturally and
/// [`execute_command`] surfaces that as an `Err`. We deliberately do *not*
/// silently synthesize an empty `Ok(CommandOutput)` for the disconnected
/// case, because callers (notably the completions/syntax-highlighting
/// pipeline) treat `Ok(empty)` as "there are zero top-level commands" and
/// produce incorrect results.
pub struct RemoteServerCommandExecutor {
    session_id: SessionId,
    client: Arc<RemoteServerClient>,
}

impl std::fmt::Debug for RemoteServerCommandExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteServerCommandExecutor")
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl RemoteServerCommandExecutor {
    /// Creates a new executor backed by an already-connected
    /// [`RemoteServerClient`].
    pub fn new(session_id: SessionId, client: Arc<RemoteServerClient>) -> Self {
        Self { session_id, client }
    }
}

#[async_trait]
impl CommandExecutor for RemoteServerCommandExecutor {
    async fn execute_command(
        &self,
        command: &str,
        _shell: &Shell,
        current_directory_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
        _execute_command_options: ExecuteCommandOptions,
    ) -> Result<CommandOutput> {
        let slow_start = instant::Instant::now();
        let response = self
            .client
            .run_command(
                self.session_id,
                command.to_owned(),
                current_directory_path.map(ToOwned::to_owned),
                environment_variables.unwrap_or_default(),
            )
            .await
            .map_err(|e| anyhow!("Remote command failed (session={:?}): {e}", self.session_id))?;

        SLOW_REMOTE_COMMAND.finish(slow_start, || {
            format!(
                "executor=remote_server, session={:?}, command={:?}",
                self.session_id,
                warp_util::slow_log::truncate_for_log(command, 80)
            )
        });

        match response.result {
            Some(run_command_response::Result::Success(success)) => {
                let status = match success.exit_code {
                    Some(0) => CommandExitStatus::Success,
                    _ => CommandExitStatus::Failure,
                };
                Ok(CommandOutput {
                    stdout: success.stdout,
                    stderr: success.stderr,
                    status,
                    exit_code: success.exit_code.map(ExitCode::from),
                })
            }
            Some(run_command_response::Result::Error(err)) => Err(anyhow!(
                "Remote command error (session={:?}, code={:?}): {}",
                self.session_id,
                err.code(),
                err.message,
            )),
            None => Err(anyhow!(
                "Remote command returned empty response (session={:?})",
                self.session_id,
            )),
        }
    }

    // `cancel_active_commands` is deliberately NOT overridden: it is session-global and would
    // also kill unrelated probes (#616). Cancellation is per request instead: dropping the
    // `execute_command` future drops `RemoteServerClient::run_command`, which sends an `Abort`
    // for that request so the server kills the command (#1128).
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// The remote server multiplexes commands over a single SSH connection,
    /// so parallel execution is safe (unlike `RemoteCommandExecutor` which
    /// opens a new SSH session per command and is limited by `MaxSessions`).
    fn supports_parallel_command_execution(&self) -> bool {
        true
    }

    fn connection_lost(&self) -> bool {
        self.client.is_disconnected()
    }

    fn is_remote_server_executor(&self) -> bool {
        true
    }
}
