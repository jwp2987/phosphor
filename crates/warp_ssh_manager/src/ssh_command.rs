//! Assembles an `SshServerInfo` into an `ssh ...` command, and spawns a
//! subprocess for testing the connection.
//!
//! When writing to the PTY, calls `build_ssh_command_line`, which
//! shell-escape-quotes every arg to prevent spaces or single quotes in the
//! username / host / key_path from breaking the command line.
//!
//! ## Password auth security & cross-platform compatibility
//!
//! **Non-Windows**: `ssh` in pipe-stdin mode can read the password from stdin
//! normally, so a one-shot stdin injection is used
//! (`build_password_auth_stdin`). The password is held in memory only as a
//! `Zeroizing<String>` throughout, never entering argv, and never appearing
//! in same-machine-readable process info like `/proc/<pid>/cmdline` or `ps`
//! (a fix for the sshpass `-p` mode issue).
//!
//! **Windows**: Win32-OpenSSH refuses to read the password from stdin even
//! when stdin is a pipe, because of `CREATE_NO_WINDOW` (no console), printing
//! `GetConsoleMode on STD_INPUT_HANDLE failed` and hanging — see
//! PowerShell/Win32-OpenSSH issue #1470. The workaround is `SSH_ASKPASS`: a
//! temporary .cmd script is written, ssh spawns it and reads its stdout as the
//! password, completely bypassing stdin and the console.
//! `SSH_ASKPASS_REQUIRE=force` forces the askpass path. The password itself is
//! passed to the askpass script via a temp file (not an env var, to reduce the
//! leak surface); the entire lifecycle is guaranteed by the `AskpassSession`
//! RAII guard, which cleans up immediately after ssh exits.

use crate::types::{AuthType, ConnectionStatus, SshServerInfo};
#[cfg(not(windows))]
use futures_lite::io::AsyncWriteExt as _;
use std::borrow::Cow;
use std::process::Stdio;
use std::time::Duration;
use zeroize::Zeroizing;

/// Connection options that MUST precede the destination on the ssh command line
/// (`-p <port>`, `-i <key>`). Returned without the leading `"ssh"` and without
/// the destination, so callers can splice their own `-o` options in between the
/// options and the destination.
fn ssh_connection_opts(server: &SshServerInfo) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if server.port != 22 {
        args.push("-p".into());
        args.push(server.port.to_string());
    }
    if matches!(server.auth_type, AuthType::Key | AuthType::OneKey) {
        if let Some(path) = server.key_path.as_deref() {
            if !path.is_empty() {
                args.push("-i".into());
                args.push(path.to_string());
            }
        }
    }
    args
}

/// The ssh destination (`user@host`, or bare `host` when no username is set).
fn ssh_destination(server: &SshServerInfo) -> String {
    if server.username.is_empty() {
        server.host.clone()
    } else {
        format!("{}@{}", server.username, server.host)
    }
}

/// Appends the `--` option terminator followed by the destination to `args`.
///
/// Security: without `--`, a `host` (or `username`) beginning with `-` — e.g.
/// `-oProxyCommand=touch /tmp/pwned` — is parsed by `ssh` as an option, which
/// for `ProxyCommand`/`LocalCommand` means arbitrary **local** command execution
/// before any network connection. Shell-escaping does not help: it quotes
/// metacharacters but a leading-dash token is still a valid argv element that
/// `ssh` reads as a flag. `--` stops option parsing; everything after it is
/// positional (destination, then optional remote command).
fn push_destination(args: &mut Vec<String>, server: &SshServerInfo) {
    args.push("--".into());
    args.push(ssh_destination(server));
}

pub fn build_ssh_args(server: &SshServerInfo) -> Vec<String> {
    let mut args: Vec<String> = vec!["ssh".into()];
    args.extend(ssh_connection_opts(server));
    push_destination(&mut args, server);
    args
}

pub fn build_ssh_command_line(server: &SshServerInfo) -> String {
    let args = build_ssh_args(server);
    args.iter()
        .map(|a| shell_escape::unix::escape(Cow::Borrowed(a.as_str())).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ConnectionTestResult {
    pub status: ConnectionStatus,
    pub latency_ms: Option<u64>,
    pub error_message: Option<String>,
}

pub async fn test_connection(
    server: &SshServerInfo,
    password: Option<Zeroizing<String>>,
) -> ConnectionTestResult {
    let start = instant::Instant::now();

    let result = match server.auth_type {
        AuthType::Key => test_key_auth(server).await,
        AuthType::Password | AuthType::OneKey => test_password_auth(server, password).await,
    };

    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(()) => ConnectionTestResult {
            status: ConnectionStatus::Online,
            latency_ms: Some(latency),
            error_message: None,
        },
        Err(e) => ConnectionTestResult {
            status: ConnectionStatus::Offline,
            latency_ms: Some(latency),
            error_message: Some(e),
        },
    }
}

async fn test_key_auth(server: &SshServerInfo) -> Result<(), String> {
    // The -o options must be inserted before the destination, otherwise SSH
    // treats -o as part of the remote command instead of its own option; the
    // destination itself is appended (guarded by `--`) via push_destination.
    let mut args: Vec<String> = vec!["ssh".into()];
    args.extend(ssh_connection_opts(server));
    args.extend([
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=5".into(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "LogLevel=ERROR".into(),
    ]);
    push_destination(&mut args, server);
    args.push("echo ok".into());
    let cmd_args = args;

    match tokio::time::timeout(TEST_TIMEOUT, run_ssh_test(&cmd_args)).await {
        Ok(Ok(output)) => {
            // Strictly matches `echo ok`, not letting through a false positive
            // where banner/motd happens to end with "ok".
            if output.trim() == "ok" {
                Ok(())
            } else {
                Err(format!("Unexpected output: {}", output.trim()))
            }
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("Connection timeout".into()),
    }
}

async fn test_password_auth(
    server: &SshServerInfo,
    password: Option<Zeroizing<String>>,
) -> Result<(), String> {
    let password = password.ok_or("Password not provided")?;

    // Build the ssh command args (note: -o options must be inserted before
    // the destination, see that function's comment)
    let cmd_args = build_password_auth_cmd_args(server);

    // Platform branch: Windows uses SSH_ASKPASS, other platforms use stdin injection
    #[cfg(windows)]
    return test_password_auth_windows(cmd_args, &password).await;
    #[cfg(not(windows))]
    test_password_auth_unix(cmd_args, &password).await
}

/// Non-Windows platforms: `ssh` can read the password from pipe stdin normally.
#[cfg(not(windows))]
async fn test_password_auth_unix(
    cmd_args: Vec<String>,
    password: &Zeroizing<String>,
) -> Result<(), String> {
    let stdin_bytes = build_password_auth_stdin(password);

    let mut child = command::r#async::Command::new("ssh")
        .args(&cmd_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to start ssh: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&stdin_bytes)
            .await
            .map_err(|e| format!("Failed to write password: {e}"))?;
    }

    let output = match tokio::time::timeout(TEST_TIMEOUT, child.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("Failed to read ssh output: {e}")),
        Err(_) => return Err("Connection timeout".into()),
    };

    finalize_password_test_result(&output)
}

/// Windows platform: uses the SSH_ASKPASS mechanism to hand the password to ssh, completely bypassing stdin/console.
#[cfg(windows)]
async fn test_password_auth_windows(
    cmd_args: Vec<String>,
    password: &Zeroizing<String>,
) -> Result<(), String> {
    let askpass = AskpassSession::new(password).map_err(|e| format!("Failed to prepare askpass: {e}"))?;

    let mut cmd = command::r#async::Command::new("ssh");
    cmd.args(&cmd_args)
        // ssh no longer needs to read the password from stdin; set it to null to avoid ssh mistakenly thinking there's a tty
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    askpass.apply_env(&mut cmd);

    let child = cmd.spawn().map_err(|e| format!("Failed to start ssh: {e}"))?;

    // When the timeout hits, child is dropped → kill_on_drop automatically kills ssh.
    // The askpass guard is dropped at the end of the function, cleaning up temp files.
    let output = match tokio::time::timeout(TEST_TIMEOUT, child.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("Failed to read ssh output: {e}")),
        Err(_) => return Err("Connection timeout".into()),
    };
    drop(askpass);

    finalize_password_test_result(&output)
}

/// Parses the ssh subprocess's output, unifying success/failure decision logic (shared by both platforms).
fn finalize_password_test_result(output: &std::process::Output) -> Result<(), String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr_trimmed = String::from_utf8_lossy(&output.stderr).trim().to_string();

    // Always log the real ssh stderr, leaving a trace even on success, to help
    // investigate afterward discrepancies like "why did the server accept the
    // password but the UI still reported success".
    if !stderr_trimmed.is_empty() {
        log::warn!("ssh test stderr: {stderr_trimmed}");
    }

    // Success decision: strictly matches the `echo ok` output. The previous
    // `ends_with("ok")` fallback would misjudge success when banner / motd
    // happened to end with "ok"; that fallback has been removed here.
    if output.status.success() && stdout.trim() == "ok" {
        Ok(())
    } else if stderr_trimmed.contains("Permission denied")
        || stderr_trimmed.contains("Authentication failed")
    {
        // The error message includes a trimmed stderr (<= 200 chars), helping
        // the user judge whether the server side didn't enable password auth,
        // or configured kbd-only AuthenticationMethods, etc.
        let detail = if stderr_trimmed.is_empty() {
            String::new()
        } else {
            let snippet: String = stderr_trimmed.chars().take(200).collect();
            if stderr_trimmed.chars().count() > 200 {
                format!(" ({snippet}...)")
            } else {
                format!(" ({snippet})")
            }
        };
        Err(format!("Authentication failed: wrong password{detail}"))
    } else {
        Err(format!(
            "Unexpected output: stdout={} stderr={}",
            stdout.trim(),
            stderr_trimmed
        ))
    }
}

/// Encodes the password into the byte stream to write to ssh stdin: password
/// UTF-8 + newline. Split into a pure function to make it easy for unit tests
/// to assert "stdin contains the literal password + newline". Only actually
/// called on the unix branch (Windows uses SSH_ASKPASS), but the function
/// itself compiles cross-platform so `build_password_auth_stdin_*` unit tests
/// can also run on Windows CI.
// Only called by tests on Windows; the production path uses SSH_ASKPASS, so dead_code is suppressed
#[cfg_attr(windows, allow(dead_code))]
fn build_password_auth_stdin(password: &Zeroizing<String>) -> Zeroizing<Vec<u8>> {
    let mut v = Zeroizing::new(Vec::with_capacity(password.len() + 1));
    v.extend_from_slice(password.as_bytes());
    v.push(b'\n');
    v
}

/// Assembles the full argv passed to the ssh subprocess during password-auth testing.
///
/// Unlike `build_ssh_args`: this skips the first item `"ssh"` (we spawn
/// explicitly via `Command::new("ssh")`), and appends test-only `-o` options and the `echo ok` remote command.
///
/// Key option meanings:
/// - `BatchMode=no`: allows ssh to read the password from stdin / askpass (stdin is needed when not using askpass)
/// - `PreferredAuthentications=password`: declares **only** wanting to try
///   password, without `keyboard-interactive`. Otherwise the server-side PAM
///   would trigger a kbd-interactive fallback after password, the kbd-int
///   sub-prompt gets no response, retries one by one, and triggers
///   `pam_faildelay` (~2s per attempt), accumulating ~8-10s and maxing out `TEST_TIMEOUT`.
/// - `KbdInteractiveAuthentication=no`: a client capability switch that
///   directly disables the entire kbd-int protocol. `PreferredAuthentications`
///   alone isn't enough — it only constrains the prompt count of the password
///   sub-method, kbd-int can still run; both switches together are defense in depth.
/// - `NumberOfPasswordPrompts=1`: the password sub-method only allows 1 retry.
/// - `ConnectTimeout=5`: single TCP connection timeout.
/// - `StrictHostKeyChecking=no`: doesn't block on known_hosts (avoiding host key
///   changes from causing false positives; real terminal connections go through a different path).
/// - `LogLevel=ERROR`: suppresses host key prompts / banner noise etc.
///
/// `echo ok` is used as the remote command, with stdout strictly matched to
/// determine success (avoiding false positives where banner / motd happens to end with "ok").
///
/// author: logic
/// date: 2026-06-01
fn build_password_auth_cmd_args(server: &SshServerInfo) -> Vec<String> {
    // No leading "ssh" (Command::new("ssh") supplies it). The -o options must be
    // inserted before the destination, otherwise SSH treats -o as part of the
    // remote command instead of its own option; the destination is appended
    // (guarded by `--`) via push_destination.
    let mut args: Vec<String> = ssh_connection_opts(server);
    args.extend([
        "-o".into(),
        "BatchMode=no".into(),
        "-o".into(),
        "PreferredAuthentications=password".into(),
        "-o".into(),
        "KbdInteractiveAuthentication=no".into(),
        "-o".into(),
        "NumberOfPasswordPrompts=1".into(),
        "-o".into(),
        "ConnectTimeout=5".into(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "LogLevel=ERROR".into(),
    ]);
    push_destination(&mut args, server);
    args.push("echo ok".into());
    args
}

async fn run_ssh_test(args: &[String]) -> Result<String, std::io::Error> {
    // Uniformly spawn subprocesses via command::r#async, which carries
    // CREATE_NO_WINDOW on Windows to avoid flashing a console window (see
    // .clippy.toml's ban on tokio::process::Command).
    let output = command::r#async::Command::new(&args[0])
        .args(&args[1..])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Success decision: the process exit code is 0, or the remote `echo ok`
    // output has been returned (some sshpass warnings make the exit code
    // non-zero while stdout still contains "ok").
    if output.status.success() || stdout.contains("ok") {
        Ok(stdout)
    } else {
        Err(std::io::Error::other(stderr))
    }
}

/// Windows-only askpass session: creates a password file + askpass helper
/// script in a temp directory, exposed to `ssh` via the `SSH_ASKPASS`
/// environment variable, automatically cleaning up both files on drop.
///
/// On Windows, `ssh.exe` refuses to read the password from stdin even when
/// stdin is a pipe, because there's no console (printing
/// `GetConsoleMode on STD_INPUT_HANDLE failed` and hanging); see
/// PowerShell/Win32-OpenSSH issue #1470 for details. The workaround is
/// `SSH_ASKPASS`: once `ssh` sees this environment variable, it spawns the
/// specified program and treats its stdout as the password, completely
/// bypassing stdin and the console. `SSH_ASKPASS_REQUIRE=force` forces ssh to
/// go through the askpass path even when it detects a
/// TTY.
///
/// The password is passed to the askpass script via a temp file (not an env
/// var, to reduce the leak surface): an env var would be visible in the `ssh`
/// subprocess and all its child processes. The askpass process's lifetime is
/// extremely short (ssh execs it immediately after fork, and it exits right
/// after reading), so the on-disk exposure window is bounded to milliseconds.
///
/// **Security trade-off**: the two temp files don't set
/// `FILE_ATTRIBUTE_HIDDEN` and don't touch ACLs, relying on Windows'
/// `%TEMP%` default isolation (`C:\Users\<user>\AppData\Local\Temp`,
/// per-user). An earlier version tried the hidden attribute + tightening ACLs
/// via icacls to `(R)`, but `FILE_ATTRIBUTE_HIDDEN` made `posix_spawnp` return
/// `ERROR_ACCESS_DENIED` (error 5) during the `CreateProcessW` stage — askpass
/// couldn't start at all, and the password ended up mistakenly being sent to
/// the server's password prompt (the user saw "wrong password" when it
/// actually was never transmitted at all). Windows temp dir's per-user
/// isolation is already good enough; simple-and-reliable is prioritized over
/// "defense in depth" here.
///
/// author: logic
/// date: 2026-06-01
#[cfg(windows)]
struct AskpassSession {
    password_path: std::path::PathBuf,
    script_path: std::path::PathBuf,
}

#[cfg(windows)]
impl AskpassSession {
    fn new(password: &Zeroizing<String>) -> std::io::Result<Self> {
        use std::io::Write as _;

        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let suffix = format!("{pid}-{nanos}");

        let password_path = dir.join(format!("warp-ssh-askpass-{suffix}.txt"));
        let script_path = dir.join(format!("warp-ssh-askpass-{suffix}.cmd"));

        // Write the password to a temp file (no hidden attribute, no ACL changes, see the type doc's security trade-off)
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&password_path)?;
            f.write_all(password.as_bytes())?;
            f.sync_all()?;
        }

        // Write the askpass helper script: reads the first line of the file
        // pointed to by %WARP_SSH_ASKPASS_FILE%, echoing it to stdout.
        // `set /p` reads the first line (stripping the newline), `echo !PW!`
        // outputs it. Uses `setlocal enabledelayedexpansion` + `!PW!` delayed
        // expansion to avoid the password being truncated by %PW%'s immediate
        // expansion re-parsing when it contains cmd special characters (&, |, <, >, ^).
        let body = "@echo off\r\nsetlocal enabledelayedexpansion\r\nset /p PW=<\"%WARP_SSH_ASKPASS_FILE%\"\r\necho !PW!\r\nendlocal\r\n";
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&script_path)?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
        }

        Ok(Self {
            password_path,
            script_path,
        })
    }

    /// Attaches the environment variables required by SSH_ASKPASS to the ssh subprocess.
    fn apply_env(&self, cmd: &mut command::r#async::Command) {
        cmd.env("SSH_ASKPASS", &self.script_path)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("WARP_SSH_ASKPASS_FILE", &self.password_path)
            .env_remove("DISPLAY");
    }
}

#[cfg(windows)]
impl Drop for AskpassSession {
    fn drop(&mut self) {
        // Delete both temp files immediately after ssh exits, shortening the
        // window the password lives on disk.
        // Errors are swallowed: cleanup failure shouldn't affect the main flow's return value.
        let _ = std::fs::remove_file(&self.password_path);
        let _ = std::fs::remove_file(&self.script_path);
    }
}

#[cfg(test)]
#[path = "ssh_command_tests.rs"]
mod tests;
