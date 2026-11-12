use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{any::Any, path::PathBuf};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use command::r#async::Command;
use futures::future::BoxFuture;
use itertools::Itertools as _;
use remote_server::backend::{ChannelKind, ExecError, ExecOutput, ExecRequest, OutOfBandExecutor};
use remote_server::shellcmd::ShellKind;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use warpui::r#async::FutureExt as _;

use crate::env_vars::{EnvVarValue, serialize_variables_for_shell};
use crate::terminal::shell::{Shell, ShellType};

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

/// The shell-mode channel's give-up net. The remote helper allows a request 120 s
/// (`client::REQUEST_TIMEOUT`); a no-helper `git push`, `git pull` or codebase scan must get the
/// same budget, or it fails only without the helper (#1875).
const SHELL_MODE_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

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

/// One in-flight limit per ControlMaster socket, shared by every user of that master in this
/// process: the generator executor ([`RemoteCommandExecutor`], which is rebuilt when the helper
/// connects or is lost) and the shell-mode channel ([`ControlMasterChannel`]). Two limits of four
/// would put eight mux clients on a master whose sshd allows ten sessions in total.
fn inflight_limit_for_socket(socket: &std::path::Path) -> InflightLimit {
    static LIMITS: std::sync::LazyLock<std::sync::Mutex<HashMap<PathBuf, InflightLimit>>> =
        std::sync::LazyLock::new(Default::default);
    let mut limits = LIMITS.lock().unwrap_or_else(|e| e.into_inner());
    limits
        .entry(socket.to_path_buf())
        .or_insert_with(|| {
            InflightLimit::new(max_inflight_from_env(
                std::env::var("PHOSPHOR_SSH_LEGACY_MAX_INFLIGHT")
                    .ok()
                    .as_deref(),
            ))
        })
        .clone()
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
            inflight: inflight_limit_for_socket(&control_socket_path),
            control_socket_path,
            wsl_distro,
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
        let shell_type = shell.shell_type();
        let command_str = build_exec_script(
            command,
            shell_type,
            current_directory_path,
            environment_variables,
        );
        // #2180: a nushell login shell cannot parse the `cd '..' && ..` text (and sshd hands the
        // channel string to the login shell), so for nu the script is POSIX text run by a
        // login-shell-neutral `sh -s` with the script on stdin. Other shells keep the text they
        // always got: it is written in their own syntax (fish, PowerShell).
        let (channel_command, channel_stdin) = if shell_type == ShellType::Nu {
            (
                remote_server::ssh::NEUTRAL_EXEC_COMMAND.to_string(),
                remote_server::ssh::neutral_exec_script(&command_str).into_bytes(),
            )
        } else {
            (command_str, Vec::new())
        };

        let result = run_over_master(
            &self.control_socket_path,
            self.wsl_distro.as_deref(),
            &self.inflight,
            self.command_timeout,
            channel_command,
            &channel_stdin,
        )
        .await
        .map(|output| output.into());
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

/// The text for one command: env, `cd`, then the command. For nushell the executor's commands
/// are POSIX (`ShellType::executor_syntax`), so env and `cd` are POSIX too (`export`, `&&`);
/// the caller runs that through `sh` ([`remote_server::ssh::NEUTRAL_EXEC_COMMAND`]).
fn build_exec_script(
    command: &str,
    shell_type: ShellType,
    current_directory_path: Option<&str>,
    environment_variables: Option<HashMap<String, String>>,
) -> String {
    let syntax = shell_type.executor_syntax();
    let mut command_str = String::new();
    if let Some(environment_variables) = environment_variables {
        let env_vars = environment_variables
            .into_iter()
            .map(|(key, value)| (key, EnvVarValue::Constant(value)))
            .collect_vec();
        let env_vars_str = serialize_variables_for_shell(
            env_vars.iter().map(|(key, value)| (key.as_str(), value)),
            syntax,
        );
        if shell_type == ShellType::Nu && !env_vars.is_empty() {
            // Under sh a bare `A=1 B=2;` would set shell variables the command never sees.
            command_str.push_str("export ");
        }
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
        let escaped_path = shell_escape_single_quotes(current_directory_path, syntax);
        command_str.push_str(&format!("cd '{escaped_path}' && "));
    }
    command_str.push_str(command);
    command_str
}

/// Runs `command_str` as a fresh mux client on the ControlMaster at `socket` and returns what it
/// printed. Waits (queues) for one of the session's in-flight slots; gives up after `timeout`.
///
/// Guarded mux client (#1100): rides the existing master or fails fast, never opens a new
/// connection or prompts. Argument list only; see `remote_server::ssh::ssh_args`. A caller that
/// drops the future cancels the command: `kill_on_drop` kills the local ssh client, closing its
/// channel on the shared connection (#1128).
async fn run_over_master(
    socket: &std::path::Path,
    wsl_distro: Option<&str>,
    inflight: &InflightLimit,
    timeout: Option<Duration>,
    command_str: String,
    stdin: &[u8],
) -> Result<std::process::Output> {
    let mut ssh_args = remote_server::ssh::ssh_args(std::path::Path::new(
        socket
            .to_str()
            .ok_or_else(|| anyhow!("socket path must exist"))?,
    ));
    ssh_args.push(command_str);

    // If the SSH session originated from WSL, the ControlPath also exists inside WSL.
    // Therefore, SSH commands directly from the Windows host will not work. They must be run
    // inside that same WSL instance
    let mut command = match wsl_distro {
        None => Command::new("ssh"),
        Some(distro_name) => {
            let mut command = Command::new("wsl");
            command.args(["-d", distro_name, "-e", "ssh"]);
            command
        }
    };
    command.args(ssh_args);
    let _slot = inflight.acquire().await;
    let run = async {
        command.env("SHELL", "/bin/sh").kill_on_drop(true);
        if stdin.is_empty() {
            return command
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .map_err(|e| anyhow!(e));
        }
        use futures::AsyncWriteExt as _;
        let mut child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!(e))?;
        // Feed stdin while collecting output: a command that prints before it has read all of
        // its input (a big file chunk, a patch) must not deadlock on a full pipe. Dropping the
        // pipe at the end of the feed closes the command's stdin. A feed error (the command
        // exited early: broken pipe) is only reported when the command itself produced nothing
        // to report.
        let pipe = child.stdin.take();
        let feed = async move {
            match pipe {
                Some(mut pipe) => pipe.write_all(stdin).await,
                None => Ok(()),
            }
        };
        let (fed, output) = futures::future::join(feed, child.output()).await;
        let output = output.map_err(|e| anyhow!(e))?;
        if let Err(e) = fed {
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                return Err(anyhow!(e));
            }
        }
        Ok(output)
    };
    match timeout {
        Some(timeout) => match run.with_timeout(timeout).await {
            Ok(result) => result,
            // The timed-out future is dropped with its ssh child (`kill_on_drop`).
            Err(_) => Err(anyhow!(
                "remote command over the ControlMaster timed out after {timeout:?} and was cancelled; \
                 a git push, pull or write it was running may have been interrupted, so check \
                 the repository on the host"
            )),
        },
        None => run.await,
    }
}

/// The shell backend's out-of-band channel for a legacy ssh session: a fresh mux client on the
/// session's ControlMaster per command (parity design R1). It never touches the session's pty.
///
/// The command timeout (120 s, the helper's request budget) is a give-up net like the generator executor's: it fails the
/// call (the caller reports the error); nothing is decided or retried on it. At most
/// [`DEFAULT_MAX_INFLIGHT`] commands run at once per master (shared with
/// [`RemoteCommandExecutor`]); more wait their turn.
#[derive(Debug)]
pub struct ControlMasterChannel {
    control_socket_path: PathBuf,
    wsl_distro: Option<String>,
    shell_kind: ShellKind,
    inflight: InflightLimit,
    command_timeout: Option<Duration>,
}

impl ControlMasterChannel {
    pub fn new(
        control_socket_path: PathBuf,
        wsl_distro: Option<String>,
        shell_kind: ShellKind,
    ) -> Self {
        Self {
            inflight: inflight_limit_for_socket(&control_socket_path),
            control_socket_path,
            wsl_distro,
            shell_kind,
            command_timeout: shell_mode_timeout_from_env(
                std::env::var("PHOSPHOR_SSH_LEGACY_COMMAND_TIMEOUT_SECS")
                    .ok()
                    .as_deref(),
            ),
        }
    }
}

/// [`command_timeout_from_env`] with the shell-mode default ([`SHELL_MODE_COMMAND_TIMEOUT`]).
pub(crate) fn shell_mode_timeout_from_env(value: Option<&str>) -> Option<Duration> {
    match value.map(str::trim).and_then(|v| v.parse::<u64>().ok()) {
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
        None => Some(SHELL_MODE_COMMAND_TIMEOUT),
    }
}

/// Cuts `stdout` at `limit` (0 = no limit) and says whether it did.
fn cap_stdout(mut stdout: Vec<u8>, limit: usize) -> (Vec<u8>, bool) {
    if limit > 0 && stdout.len() > limit {
        stdout.truncate(limit);
        (stdout, true)
    } else {
        (stdout, false)
    }
}

impl OutOfBandExecutor for ControlMasterChannel {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::ControlMaster
    }

    fn shell_kind(&self) -> ShellKind {
        self.shell_kind
    }

    fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
        Box::pin(async move {
            // The master's socket is the channel; once the ssh session ends it is gone.
            if !self.control_socket_path.exists() && self.wsl_distro.is_none() {
                return Err(ExecError::Unavailable(
                    "the ssh session's ControlMaster is gone".to_string(),
                ));
            }
            let started = instant::Instant::now();
            let output = run_over_master(
                &self.control_socket_path,
                self.wsl_distro.as_deref(),
                &self.inflight,
                self.command_timeout,
                request.command,
                &request.stdin,
            )
            .await
            .map_err(|e| ExecError::Failed(format!("{e:#}")))?;
            SLOW_REMOTE_COMMAND.finish(started, || {
                "executor=control_master, shell-mode".to_string()
            });
            let (stdout, truncated) = cap_stdout(output.stdout, request.max_output_bytes);
            Ok(ExecOutput {
                stdout,
                stderr: output.stderr,
                exit_code: output.status.code(),
                truncated,
            })
        })
    }
}

#[cfg(test)]
#[path = "remote_command_executor_tests.rs"]
mod tests;
