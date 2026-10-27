use std::path::Path;
use std::process::Output;
use std::time::Duration;

use anyhow::{anyhow, Result};
use command::r#async::Command;
use warpui::r#async::FutureExt as _;

/// Timeout for `ssh -O exit`. The command only talks to the local
/// ControlMaster over a Unix socket, so it should return almost
/// immediately; if it doesn't, we'd rather give up than block
/// teardown.
const STOP_CONTROL_MASTER_TIMEOUT: Duration = Duration::from_secs(5);

/// Builds the common SSH argument list for multiplexed connections through
/// an existing ControlMaster socket.
pub fn ssh_args(socket_path: &Path) -> Vec<String> {
    vec![
        "-q".to_string(),
        "-o".to_string(),
        "PasswordAuthentication=no".to_string(),
        "-o".to_string(),
        "ForwardX11=no".to_string(),
        "-o".to_string(),
        format!("ControlPath={}", socket_path.display()),
        "placeholder@placeholder".to_string(),
    ]
}

/// Runs `ssh -O exit -o ControlPath=<socket_path>` to force the local
/// SSH `ControlMaster` managing `socket_path` to exit immediately,
/// without waiting for multiplexed channels to finish draining.
///
/// `warp_ssh_helper` starts the master as its own background process
/// (`ssh -o ControlMaster=yes -N -f`, #1035) and runs the interactive session as a multiplex
/// client of it. The master can outlive the session: it waits for the server to close
/// half-closed channels (e.g. from `ssh ... remote-server-proxy`), and a Windows OpenSSH server
/// never closes an abandoned session channel at all, so `ControlPersist` alone does not expire
/// it. Sending `-O exit` ends it right away.
///
/// **Only safe to call once the user's shell has already exited** --
/// this tears down the interactive ssh outright. In practice it is
/// invoked from the `ExitShell` teardown path on the client.
///
/// Fire-and-forget. Errors are logged but not propagated: at teardown
/// time there is nothing useful to do with them.
pub async fn stop_control_master(socket_path: &Path) {
    stop_control_master_inner(socket_path).await;
    // A master that was SIGKILLed (the pty teardown kills the whole process tree, #975/#980)
    // never unlinks its socket, and `ssh -O exit` then fails, leaving a stale
    // `~/.ssh/<session id>` behind. We only get here for a master Phosphor owns, so removing
    // the leftover is safe.
    remove_stale_control_socket(socket_path);
}

/// Removes `socket_path` if it is still there after the master was stopped. Missing is the
/// normal case (a graceful master unlinks it) and is ignored.
///
/// The path originates from the shell's DCS hook and so is forgeable by anything that can print
/// to the pty. Only a real Unix socket is removed: the path is inspected with
/// `symlink_metadata` (never following symlinks), and regular files, directories and symlinks
/// are left alone.
pub(crate) fn remove_stale_control_socket(socket_path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt as _;
        match std::fs::symlink_metadata(socket_path) {
            Ok(meta) if meta.file_type().is_socket() => match std::fs::remove_file(socket_path) {
                Ok(()) => {
                    log::info!("removed stale ControlPath socket {}", socket_path.display())
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => log::warn!(
                    "failed to remove ControlPath socket {}: {e}",
                    socket_path.display()
                ),
            },
            Ok(_) => log::warn!(
                "refusing to remove {}: not a Unix socket",
                socket_path.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!(
                "failed to inspect ControlPath socket {}: {e}",
                socket_path.display()
            ),
        }
    }
    #[cfg(not(unix))]
    let _ = socket_path;
}

async fn stop_control_master_inner(socket_path: &Path) {
    let args = ssh_args(socket_path);
    let result = async {
        Command::new("ssh")
            .arg("-O")
            .arg("exit")
            .args(&args)
            .kill_on_drop(true)
            .output()
            .await
    }
    .with_timeout(STOP_CONTROL_MASTER_TIMEOUT)
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            log::info!(
                "stop_control_master: `ssh -O exit` succeeded for {}",
                socket_path.display()
            );
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::info!(
                "stop_control_master: `ssh -O exit` for {} exited with {:?}: {stderr}",
                socket_path.display(),
                output.status.code(),
            );
        }
        Ok(Err(e)) => {
            log::info!(
                "stop_control_master: failed to spawn `ssh -O exit` for {}: {e}",
                socket_path.display()
            );
        }
        Err(_) => {
            log::warn!(
                "stop_control_master: `ssh -O exit` for {} timed out after {:?}",
                socket_path.display(),
                STOP_CONTROL_MASTER_TIMEOUT,
            );
        }
    }
}

/// Run a single SSH command through the ControlMaster socket and return a result where:
/// - `Err` for transport-level failures (e.g. couldn't spawn `ssh`, or timeout).
/// - `Ok(output)` callers should check `output.status` to distinguish a successful remote command from a non-zero remote exit.
pub async fn run_ssh_command(
    socket_path: &Path,
    remote_command: &str,
    timeout: Duration,
) -> Result<Output> {
    async {
        Command::new("ssh")
            .args(ssh_args(socket_path))
            .arg(remote_command)
            .kill_on_drop(true)
            .output()
            .await
    }
    .with_timeout(timeout)
    .await
    .map_err(|_| anyhow!("SSH command timed out after {timeout:?}"))?
    .map_err(|e| anyhow!("SSH command failed to execute: {e}"))
}

/// Pipe a script into `bash -s` on the remote host via the ControlMaster
/// socket. Returns a result where:
/// - `Err` for transport-level failures (e.g. couldn't spawn `ssh`, or timeout).
/// - `Ok(output)` callers should check `output.status` to distinguish a successful remote script from a non-zero remote exit.
///
/// We pipe via stdin rather than passing the script as an SSH command-line
/// argument because the install script is multi-line and contains shell
/// constructs (case statements, variable expansions, single/double quotes)
/// that would require complex, fragile escaping if passed as an argument.
/// The `bash -s` + stdin approach avoids all escaping issues and has no
/// argument length limits.
pub async fn run_ssh_script(socket_path: &Path, script: &str, timeout: Duration) -> Result<Output> {
    use std::process::Stdio;

    let mut child = Command::new("ssh")
        .args(ssh_args(socket_path))
        .arg("bash -s")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn SSH for script: {e}"))?;

    // Write the script to stdin.
    if let Some(mut stdin) = child.stdin.take() {
        use futures_lite::io::AsyncWriteExt;
        stdin
            .write_all(script.as_bytes())
            .await
            .map_err(|e| anyhow!("Failed to write script to stdin: {e}"))?;
        // Close stdin so the remote bash exits after reading the script.
        drop(stdin);
    }

    child
        .output()
        .with_timeout(timeout)
        .await
        .map_err(|_| anyhow!("Script timed out after {timeout:?}"))?
        .map_err(|e| anyhow!("Script failed: {e}"))
}

/// Uploads a single file to the remote host via an existing ControlMaster socket.
pub async fn scp_upload(
    socket_path: &Path,
    local_path: &Path,
    remote_path: &str,
    timeout: Duration,
) -> Result<()> {
    let output = async {
        Command::new("scp")
            // -C: compress the transfer. The remote-server binary uploaded in
            // dev mode is hundreds of MB; compression can significantly cut
            // upload time and reduce timeout risk.
            .arg("-C")
            .arg("-o")
            .arg(format!("ControlPath={}", socket_path.display()))
            .arg("-o")
            .arg("ControlMaster=no")
            .arg("-o")
            .arg("PasswordAuthentication=no")
            .arg("-o")
            .arg("ForwardX11=no")
            .arg("-o")
            .arg("ConnectTimeout=15")
            .arg(local_path.as_os_str())
            .arg(format!("placeholder@placeholder:{remote_path}"))
            .kill_on_drop(true)
            .output()
            .await
    }
    .with_timeout(timeout)
    .await
    .map_err(|_| anyhow!("SCP upload timed out after {timeout:?}"))?
    .map_err(|e| anyhow!("SCP upload failed to execute: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!("SCP upload failed: {stderr}"))
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
