use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{any::Any, path::PathBuf};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use command::r#async::Command;
use itertools::Itertools as _;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use warpui::r#async::FutureExt as _;

use crate::env_vars::{EnvVarValue, serialize_variables_for_shell};
use crate::terminal::shell::Shell;

use super::shared::shell_escape_single_quotes;
use super::{CommandExecutor, CommandOutput, ExecuteCommandOptions};

/// A remote generator/background command slower than this is worth a log line (#1146): it
/// shares the ssh connection with the user's keystrokes (#985).
const REMOTE_COMMAND_SLOW: std::time::Duration = std::time::Duration::from_secs(2);
static SLOW_REMOTE_COMMAND: warp_util::slow_log::SlowLog = warp_util::slow_log::SlowLog::new(
    "remote command (ControlMaster executor)",
    REMOTE_COMMAND_SLOW,
    warp_util::slow_log::DEFAULT_WINDOW,
);

/// How many mux clients one session runs on its master at once (#1240). Every request is a
/// channel on the shared connection; sshd refuses channels past its `MaxSessions` (default 10)
/// and OpenSSH prints `channel N: open failed` into the user's live shell. The interactive
/// session and the remote-server proxy hold two, and other panes on the same host may share
/// the master, so this stays well under ten. `PHOSPHOR_SSH_LEGACY_MAX_INFLIGHT` overrides it
/// (0 = unlimited, the behaviour before #1240).
const DEFAULT_MAX_INFLIGHT: usize = 4;

/// The longest a request may hold its mux client (#1240). Generators are bounded by their
/// callers, but the history read and the executables probe are not, and a hung remote command
/// (`kubectl` against a dead cluster) otherwise keeps its channel, and a slot above, until the
/// session ends. Well above any legitimate generator; a slow link reading a large history is
/// the case it must not cut off. `PHOSPHOR_SSH_LEGACY_COMMAND_TIMEOUT_SECS` overrides it
/// (0 = no timeout).
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// Bounds how many requests run at once; a waiter dropped before its turn (a superseded
/// generator) leaves the queue with its future.
#[derive(Clone, Debug)]
pub(crate) struct InflightLimit(Option<Arc<Semaphore>>);

impl InflightLimit {
    pub(crate) fn new(max_inflight: Option<usize>) -> Self {
        Self(max_inflight.map(|n| Arc::new(Semaphore::new(n))))
    }

    /// Waits for a slot; `None` when unlimited. The permit is released when dropped.
    pub(crate) async fn acquire(&self) -> Option<OwnedSemaphorePermit> {
        let semaphore = self.0.clone()?;
        // `acquire_owned` only fails once the semaphore is closed, which never happens here.
        semaphore.acquire_owned().await.ok()
    }

    #[cfg(test)]
    pub(crate) fn available(&self) -> Option<usize> {
        self.0.as_ref().map(|s| s.available_permits())
    }
}

/// `PHOSPHOR_SSH_LEGACY_MAX_INFLIGHT`: unset or unparsable is the default, `0` is unlimited.
pub(crate) fn max_inflight_from_env(value: Option<&str>) -> Option<usize> {
    match value.map(str::trim).and_then(|v| v.parse::<usize>().ok()) {
        Some(0) => None,
        Some(n) => Some(n),
        None => Some(DEFAULT_MAX_INFLIGHT),
    }
}

/// `PHOSPHOR_SSH_LEGACY_COMMAND_TIMEOUT_SECS`: unset or unparsable is the default, `0` is none.
pub(crate) fn command_timeout_from_env(value: Option<&str>) -> Option<Duration> {
    match value.map(str::trim).and_then(|v| v.parse::<u64>().ok()) {
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
        None => Some(DEFAULT_COMMAND_TIMEOUT),
    }
}

/// `CommandExecutor` implementation that executes the given `command` in a forked process
/// that establishes a one-off SSH session with the same remote host as the active SSH session
/// using SSH's ControlMaster/ControlPath feature. This is typically used to run generator
/// commands for remote SSH sessions.
#[derive(Debug)]
pub struct RemoteCommandExecutor {
    control_socket_path: PathBuf,
    wsl_distro: Option<String>,
    inflight: InflightLimit,
    command_timeout: Option<Duration>,
}

impl RemoteCommandExecutor {
    pub fn new(control_socket_path: PathBuf, wsl_distro: Option<String>) -> Self {
        Self {
            control_socket_path,
            wsl_distro,
            inflight: InflightLimit::new(max_inflight_from_env(
                std::env::var("PHOSPHOR_SSH_LEGACY_MAX_INFLIGHT")
                    .ok()
                    .as_deref(),
            )),
            command_timeout: command_timeout_from_env(
                std::env::var("PHOSPHOR_SSH_LEGACY_COMMAND_TIMEOUT_SECS")
                    .ok()
                    .as_deref(),
            ),
        }
    }
}

#[async_trait]
impl CommandExecutor for RemoteCommandExecutor {
    async fn execute_command(
        &self,
        command: &str,
        shell: &Shell,
        current_directory_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
        _execute_command_options: ExecuteCommandOptions,
    ) -> Result<CommandOutput> {
        let slow_start = instant::Instant::now();
        let logged_command = command;
        // We can't use `.env` and `.current_dir` here to set the PATH and current dir respectively
        // since this is run locally. We just need the subprocess to send the bytes over the
        // ssh connection. That's why we explicitly set the path and cwd as part of command_str.
        let mut command_str = String::new();
        if let Some(environment_variables) = environment_variables {
            let env_vars = environment_variables
                .into_iter()
                .map(|(key, value)| (key, EnvVarValue::Constant(value)))
                .collect_vec();
            let env_vars_str = serialize_variables_for_shell(
                env_vars.iter().map(|(key, value)| (key.as_str(), value)),
                shell.shell_type(),
            );
            command_str.push_str(&env_vars_str);
            command_str.push(';');
        }
        if let Some(current_directory_path) = current_directory_path {
            // Ensure the path escapes embedded single quotes from the remote host's serialized
            // block. Without this a directory whose name contains `'` closes the quoting and the
            // remainder is interpreted as shell syntax -- injected into a command that runs on the
            // REMOTE host. Upstream `88c344e2` ("[Security] Fix command injection in remote ssh
            // sessions") fixed two sites; this fork ported the `history_file` one
            // (`session.rs`) and missed this one.
            let escaped_path =
                shell_escape_single_quotes(current_directory_path, shell.shell_type());
            command_str.push_str(&format!("cd '{escaped_path}' && "));
        }
        command_str.push_str(command);

        // Guarded mux client (#1100): ride the existing master or fail fast, never open a new
        // connection or prompt. Argument list only; see `remote_server::ssh::ssh_args`.
        let mut ssh_args = remote_server::ssh::ssh_args(std::path::Path::new(
            self.control_socket_path
                .to_str()
                .ok_or_else(|| anyhow!("socket path must exist"))?,
        ));
        ssh_args.push(command_str);

        // If the SSH session originated from WSL, the ControlPath also exists inside WSL.
        // Therefore, SSH commands directly from the Windows host will not work. They must be run
        // inside that same WSL instance
        let mut command = match &self.wsl_distro {
            None => Command::new("ssh"),
            Some(distro_name) => {
                let mut command = Command::new("wsl");
                command.args(["-d", distro_name.as_str(), "-e", "ssh"]);
                command
            }
        };

        command.args(ssh_args);
        // A superseded generator is cancelled by dropping this future; `kill_on_drop` then kills
        // the local ssh client, closing its channel on the shared connection (#1128).
        let _slot = self.inflight.acquire().await;
        let run = async {
            command
                .env("SHELL", "/bin/sh")
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true)
                .output()
                .await
                .map(|output| output.into())
                .map_err(|e| anyhow!(e))
        };
        let result = match self.command_timeout {
            Some(timeout) => match run.with_timeout(timeout).await {
                Ok(result) => result,
                // The timed-out future is dropped with its ssh child (`kill_on_drop`).
                Err(_) => Err(anyhow!(
                    "remote command over the ControlMaster timed out after {timeout:?}"
                )),
            },
            None => run.await,
        };
        // Only the generator text (truncated), never `command_str`, which embeds env values.
        SLOW_REMOTE_COMMAND.finish(slow_start, || {
            format!(
                "executor=control_master, command={:?}",
                warp_util::slow_log::truncate_for_log(logged_command, 80)
            )
        });
        result
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    /// The RemoteCommandExecutor can't be run in parallel because a remote box could have a value
    /// of `MaxSessions` (the max number of open sessions for a single connection) that is less than
    /// the number of commands we'd be trying to run in in parallel. Functionally, this would result
    /// in  `channel: open  failed` messages sent back over the PTY in the running sessions.
    fn supports_parallel_command_execution(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "remote_command_executor_tests.rs"]
mod tests;
