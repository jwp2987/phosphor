//! `SessionBootstrapped` and `RunCommand` for the helper: runs a command the way the app
//! daemon's `LocalCommandExecutor::execute_local_command` does (a non-interactive shell of the
//! session's type that skips the user's rc files), as a plain `std::process` child in its own
//! process group so `Abort` can kill the whole tree.

use std::collections::HashMap;
use std::io::Read as _;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::proto::{
    RunCommandError, RunCommandErrorCode, RunCommandRequest, RunCommandResponse, RunCommandSuccess,
    run_command_response, server_message,
};
use crate::protocol::MAX_MESSAGE_SIZE;

use super::jobs::Cancel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShellKind {
    Bash,
    Zsh,
    Fish,
    Nu,
    PowerShell,
}

impl ShellKind {
    /// Same name rules as the app's `ShellType::from_name`.
    pub(super) fn from_name(name: &str) -> Option<Self> {
        if name == "bash" || name == "-bash" || name.ends_with("/bash") {
            Some(Self::Bash)
        } else if name == "zsh" || name == "-zsh" || name.ends_with("/zsh") {
            Some(Self::Zsh)
        } else if name == "fish" || name == "-fish" || name.ends_with("/fish") {
            Some(Self::Fish)
        } else if name == "nu"
            || name == "-nu"
            || name == "nushell"
            || name.ends_with("/nu")
            || name.ends_with("/nushell")
        {
            Some(Self::Nu)
        } else if name == "pwsh"
            || name.ends_with("/pwsh")
            || name == "powershell"
            || name.ends_with("/powershell")
        {
            Some(Self::PowerShell)
        } else {
            None
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Nu => "nu",
            Self::PowerShell => "pwsh",
        }
    }

    /// The flag that makes the shell skip the user's configuration.
    fn no_config_flag(self) -> &'static str {
        match self {
            Self::Zsh => "-f",
            Self::Bash => "--norc",
            Self::Fish => "--no-config",
            Self::PowerShell => "-NoProfile",
            Self::Nu => "--no-config-file",
        }
    }
}

/// The shell a session was bootstrapped with.
#[derive(Clone, Debug)]
pub(super) struct SessionShell {
    pub(super) kind: ShellKind,
    pub(super) path: Option<PathBuf>,
}

/// Finds `name` on `PATH`.
fn resolve_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Builds the command line for `command` in `shell`. Commands the client runs for itself are
/// POSIX syntax, which nushell cannot parse, so they go through bash (or sh) there.
pub(super) fn build_command(shell: &SessionShell, command: &str) -> Command {
    match shell.kind {
        ShellKind::Nu => {
            let bash = resolve_in_path("bash");
            let mut cmd = Command::new(bash.as_deref().unwrap_or(std::path::Path::new("sh")));
            if bash.is_some() {
                cmd.arg("--norc");
            }
            cmd.arg("-c").arg(command);
            cmd
        }
        ShellKind::PowerShell => {
            let program = shell
                .path
                .clone()
                .unwrap_or_else(|| PathBuf::from(shell.kind.name()));
            let mut cmd = Command::new(program);
            cmd.arg(shell.kind.no_config_flag())
                .arg("-Command")
                .arg(command);
            cmd
        }
        kind => {
            let program = shell
                .path
                .clone()
                .unwrap_or_else(|| PathBuf::from(kind.name()));
            let mut cmd = Command::new(program);
            cmd.arg(kind.no_config_flag()).arg("-c").arg(command);
            cmd
        }
    }
}

struct Output {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<i32>,
}

fn run(
    shell: &SessionShell,
    command: &str,
    cwd: Option<&str>,
    env: &HashMap<String, String>,
    cancel: &Cancel,
) -> std::io::Result<Output> {
    let mut cmd = build_command(shell, command);
    cmd.envs(env);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Own process group: `Abort` kills the shell and everything it started.
        .process_group(0);
    let mut child = cmd.spawn()?;
    let pgid = child.id() as i32;
    cancel.set_child_group(pgid);

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    // Both pipes are drained concurrently, or a child filling one would block forever.
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });
    let mut out_buf = Vec::new();
    let _ = stdout.read_to_end(&mut out_buf);
    let err_buf = err_reader.join().unwrap_or_default();
    let status = child.wait();
    cancel.clear_child_group();
    let status = status?;
    Ok(Output {
        stdout: out_buf,
        stderr: err_buf,
        exit_code: status.code(),
    })
}

/// Truncates the two streams proportionally so the response stays under the wire message cap,
/// leaving headroom for the framing (same rule as the app daemon, #10681).
pub(super) fn truncate_to_wire_limit(stdout: &mut Vec<u8>, stderr: &mut Vec<u8>) {
    const MAX_OUTPUT_BYTES: usize = MAX_MESSAGE_SIZE - 1024;
    let total = stdout.len() + stderr.len();
    if total > MAX_OUTPUT_BYTES {
        let ratio = MAX_OUTPUT_BYTES as f64 / total as f64;
        stdout.truncate((stdout.len() as f64 * ratio) as usize);
        stderr.truncate((stderr.len() as f64 * ratio) as usize);
    }
}

pub(super) fn session_not_found(session_id: u64) -> server_message::Message {
    server_message::Message::RunCommandResponse(RunCommandResponse {
        result: Some(run_command_response::Result::Error(RunCommandError {
            code: RunCommandErrorCode::SessionNotFound.into(),
            message: format!("No executor for session {session_id}"),
        })),
    })
}

/// Executes `req` (blocking; the caller runs it on a worker thread).
pub(super) fn run_command(
    shell: &SessionShell,
    req: RunCommandRequest,
    cancel: &Cancel,
) -> server_message::Message {
    let result = match run(
        shell,
        &req.command,
        req.working_directory.as_deref(),
        &req.environment_variables,
        cancel,
    ) {
        Ok(Output {
            mut stdout,
            mut stderr,
            exit_code,
        }) => {
            truncate_to_wire_limit(&mut stdout, &mut stderr);
            run_command_response::Result::Success(RunCommandSuccess {
                stdout,
                stderr,
                exit_code,
            })
        }
        Err(e) => run_command_response::Result::Error(RunCommandError {
            code: RunCommandErrorCode::ExecutionFailed.into(),
            message: format!("Failed to execute command: {e}"),
        }),
    };
    server_message::Message::RunCommandResponse(RunCommandResponse {
        result: Some(result),
    })
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
