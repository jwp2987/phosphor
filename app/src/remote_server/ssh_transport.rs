//! SSH-specific implementation of [`RemoteTransport`].
//!
//! [`SshTransport`] uses an existing SSH ControlMaster socket to check/install
//! the remote server binary and to launch the `remote-server-proxy` process
//! whose stdin/stdout become the protocol channel.
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use warpui::r#async::{executor, FutureExt as _};

use remote_server::auth::RemoteServerAuthContext;
use remote_server::client::RemoteServerClient;
use remote_server::setup::{
    PreinstallCheckResult, RemotePlatform, daemon_pid_name, daemon_socket_name, parse_uname_output,
    remote_server_daemon_dir,
};
use remote_server::ssh::ssh_args;
use remote_server::transport::{Connection, RemoteTransport};

/// SSH transport: connects via a ControlMaster socket.
///
/// `socket_path` is the local Unix socket created by the ControlMaster
/// process (`ssh -N -o ControlMaster=yes -o ControlPath=<path>`). All SSH
/// commands (binary check, install, proxy launch) are multiplexed through
/// this socket without re-authenticating.
#[derive(Clone)]
pub struct SshTransport {
    socket_path: PathBuf,
    auth_context: Arc<RemoteServerAuthContext>,
    /// Whether we own the ControlMaster behind `socket_path`. `false` when
    /// the SSH wrapper attached to a master the user already had running,
    /// in which case teardown must not run `ssh -O exit` against it (doing
    /// so would kill the user's other multiplexed connections). See
    /// GitHub issue #37.
    owns_control_master: bool,
}

impl fmt::Debug for SshTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshTransport")
            .field("socket_path", &self.socket_path)
            .field("owns_control_master", &self.owns_control_master)
            .finish_non_exhaustive()
    }
}

impl SshTransport {
    pub fn new(
        socket_path: PathBuf,
        auth_context: Arc<RemoteServerAuthContext>,
        owns_control_master: bool,
    ) -> Self {
        Self {
            socket_path,
            auth_context,
            owns_control_master,
        }
    }

    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Whether we created the ControlMaster (and must tear it down) versus
    /// reused an existing user-owned one (which must be left running).
    pub fn owns_control_master(&self) -> bool {
        self.owns_control_master
    }

    pub fn remote_daemon_socket_path(&self) -> String {
        format!(
            "{}/{}",
            remote_server_daemon_dir(&self.auth_context.remote_server_identity_key()),
            daemon_socket_name()
        )
    }

    pub fn remote_daemon_pid_path(&self) -> String {
        format!(
            "{}/{}",
            remote_server_daemon_dir(&self.auth_context.remote_server_identity_key()),
            daemon_pid_name()
        )
    }

    /// Maps ControlMaster ownership onto the `control_path` teardown token
    /// returned in [`Connection`]. Only a master we created yields a path,
    /// so teardown ([`RemoteServerManager`] -> [`stop_control_master`])
    /// runs `ssh -O exit` against it. A reused, user-owned master maps to
    /// `None`, so teardown leaves it running and does not kill the user's
    /// other multiplexed connections (GitHub issue #37).
    ///
    /// [`RemoteServerManager`]: remote_server::manager::RemoteServerManager
    /// [`stop_control_master`]: remote_server::ssh::stop_control_master
    fn control_path_for_teardown(
        owns_control_master: bool,
        socket_path: PathBuf,
    ) -> Option<PathBuf> {
        if owns_control_master {
            Some(socket_path)
        } else {
            None
        }
    }

    fn remote_proxy_command(&self) -> String {
        let binary = remote_server::setup::remote_server_binary();
        let identity_key = self.auth_context.remote_server_identity_key();
        let quoted_identity_key = shell_words::quote(&identity_key);
        format!("{binary} remote-server-proxy --identity-key {quoted_identity_key}")
    }
}

#[derive(Debug)]
enum InstallError {
    ScriptFailed { exit_code: i32, stderr: String },
    Other(anyhow::Error),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScriptFailed { exit_code, stderr } => {
                write!(f, "install script failed (exit {exit_code}): {stderr}")
            }
            Self::Other(error) => write!(f, "{error:#}"),
        }
    }
}

impl From<anyhow::Error> for InstallError {
    fn from(error: anyhow::Error) -> Self {
        Self::Other(error)
    }
}

async fn detect_remote_platform(socket_path: &Path) -> Result<RemotePlatform> {
    let output = remote_server::ssh::run_ssh_command(
        socket_path,
        "uname -sm",
        remote_server::setup::CHECK_TIMEOUT,
    )
    .await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return parse_uname_output(&stdout);
    }

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!("uname -sm exited with code {code}: {stderr}"))
}

async fn verify_installed_binary(socket_path: &Path) -> Result<()> {
    let output = remote_server::ssh::run_ssh_command(
        socket_path,
        &remote_server::setup::binary_check_command(),
        remote_server::setup::CHECK_TIMEOUT,
    )
    .await?;

    if output.status.success() {
        return Ok(());
    }

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!(
        "installed binary check failed with code {code}: {stderr}"
    ))
}

async fn run_install_script(
    socket_path: &Path,
    staging_tarball_path: Option<&str>,
    timeout: std::time::Duration,
) -> core::result::Result<(), InstallError> {
    let script = remote_server::setup::install_script(staging_tarball_path);
    match remote_server::ssh::run_ssh_script(socket_path, &script, timeout).await {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(InstallError::ScriptFailed { exit_code, stderr })
        }
        Err(error) => Err(InstallError::Other(error)),
    }
}

/// What an `install_remote_server.sh` failure means for the SCP fallback.
///
/// The fallback re-downloads the *same* GitHub release tarball on the client
/// and installs it through the script's staging branch, which skips digest
/// verification on the documented assumption that a staged tarball is a
/// locally cross-compiled dev binary that never crossed an untrusted network.
/// For a fallback-fetched *release* tarball that assumption is false, so the
/// fallback is only ever a legitimate recovery when nothing was verified in
/// the first place — never as a second chance after verification failed.
///
/// The three outcomes therefore have to be told apart, which is what the
/// script's exit-code contract exists for. Fusing any two of them is the whole
/// bug: "the host could not fetch" and "the bytes are not the release we
/// pinned" are opposite verdicts, and reading one as the other is wrong in
/// both directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallFailureKind {
    /// The remote host never obtained the bytes: no fetcher installed
    /// (exit 3), or the fetch failed at the transport layer — DNS, connection
    /// refused, TLS, timeout, HTTP 4xx/5xx (exit 7). Also covers an SSH-level
    /// failure where the script did not run to completion at all, which
    /// reaches us in *two* shapes and not one: as [`InstallError::Other`]
    /// (ssh could not be spawned, the write to its stdin failed, the run timed
    /// out), and as a reported script exit of **255**, which is OpenSSH's own
    /// error status rather than anything this script can emit.
    ///
    /// No integrity claim was made and none was violated, and nothing was
    /// installed. The client downloading and pushing over the already
    /// authenticated SSH channel is precisely the recovery the SCP fallback
    /// was built for.
    TransportFailed,
    /// Integrity could not be established, or was established and FAILED:
    /// exit 4 (this client was built with no pinned digest), 5 (no SHA-256
    /// tool on the host, so the digest cannot be checked) and 6 (digest
    /// MISMATCH — the download succeeded and the content is not the pinned
    /// release, i.e. detected tampering).
    ///
    /// Fails closed. This is the security property; the fallback must not run.
    IntegrityFailed,
    /// Fatal for a reason the fallback cannot fix: exit 10 (unsupported
    /// arch/OS — no asset exists for this host), 8 (the pinned tarball will
    /// not open or holds no recognised binary, so re-fetching the identical
    /// bytes is pointless), 9 (the script failed somewhere unclassified).
    ///
    /// Also the default for any code this client does not recognise. That
    /// default is deliberately fail-closed: an unknown code means we cannot
    /// tell whether verification happened, and "could not check" must never
    /// collapse into "check passed". It is also what stops a future script
    /// revision from silently re-opening this hole.
    Fatal,
}

/// Classifies an install failure against the exit-code contract documented at
/// the top of `crates/remote_server/src/install_remote_server.sh`.
///
/// Both files carry the same table and `exit_codes_in_install_script_are_all_classified`
/// pins them together by parsing the script's own `EXIT_*=` assignments.
///
/// A note on what this replaced, because the shape recurs: the previous
/// version was a `should_skip_scp_fallback` boolean over the codes 2/4/5/6.
/// It was correct about those codes and still ineffective, because
/// `install_remote_server.sh` did not actually emit them under failure — it
/// ran `curl` under `set -e`, and `set -e` aborts with the *failing command's*
/// status. curl's status space overlaps this one, so a DNS failure (curl 6)
/// arrived here as `6` and was hard-failed as a digest mismatch, while a
/// connection refused (curl 7) or an HTTP 500 (curl 22) arrived as codes this
/// list did not know and fell through to the unverified fallback. Classifying
/// the codes correctly is only half the fix; the script has to emit them.
fn classify_install_failure(error: &InstallError) -> InstallFailureKind {
    match error {
        // The SSH run itself failed — the socket died, the script timed out,
        // the process was killed. The script never reported a verdict, so
        // there is no verdict to override; nothing unverified was installed.
        InstallError::Other(_) => InstallFailureKind::TransportFailed,
        InstallError::ScriptFailed { exit_code, .. } => match *exit_code {
            3 | 7 => InstallFailureKind::TransportFailed,
            4 | 5 | 6 => InstallFailureKind::IntegrityFailed,
            // 255 is SSH's status, not the script's, and it arrives here
            // wearing the script's clothes: `run_ssh_script` returns
            // `Ok(output)` whenever the `ssh` *process* ran, and OpenSSH
            // reports its own failures — connection closed, ControlMaster
            // gone, host key changed, remote command killed by a signal — as
            // exit 255. So `Other` above is reached only when ssh could not be
            // spawned at all, its stdin write failed, or the run timed out;
            // every other socket-level death lands in this arm.
            //
            // The script cannot collide with it: every explicit `exit` uses an
            // EXIT_* constant (all ≤ 10), its ERR trap remaps any stray status
            // onto 9, and `exit_codes_in_install_script_are_all_classified`
            // parses the constants out of the rendered script to keep that
            // true. 255 therefore means the script did not run to completion —
            // the same state as `Other`, with nothing verified and nothing
            // installed — so the SCP fallback is the designed recovery.
            //
            // Letting this fall into `_ => Fatal` reported "install script
            // failed (exit 255)" for a script that never ran, and withdrew the
            // fallback from the most common genuine transport failure there
            // is. The doc on `TransportFailed` claimed it was covered while
            // the code did the opposite.
            255 => InstallFailureKind::TransportFailed,
            _ => InstallFailureKind::Fatal,
        },
    }
}

/// The decision [`SshTransport::install_binary`] makes about a failed install.
///
/// This is the call site's routing, not a predicate the call site is free to
/// consult and then ignore: `Fail` carries the exact message `install_binary`
/// returns, so the two cannot diverge without the tests noticing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallFallbackRoute {
    /// Retry through [`scp_install_fallback`].
    ScpFallback,
    /// Give up with this message. No fallback.
    Fail(String),
}

/// Routes a failed install script run. See [`InstallFailureKind`].
fn route_install_failure(error: &InstallError) -> InstallFallbackRoute {
    match classify_install_failure(error) {
        InstallFailureKind::TransportFailed => InstallFallbackRoute::ScpFallback,
        // Deliberately louder than a plain error: exit 6 means bytes served
        // for this release did not match the digest compiled into this client,
        // which is what tampering looks like from here. Operators need to see
        // that it was refused rather than quietly retried.
        InstallFailureKind::IntegrityFailed => InstallFallbackRoute::Fail(format!(
            "remote-server install refused: integrity check failed, \
             so the SCP fallback (which installs unverified) was NOT attempted: {error}"
        )),
        InstallFailureKind::Fatal => InstallFallbackRoute::Fail(error.to_string()),
    }
}

// ===========================================================================
// Zap fork: dev-mode remote-server install path
//
// The upstream / release build has the remote install script download a
// precompiled remote-server binary from GitHub releases. But for a local
// source build (`cargo run`), that would download the "latest released"
// stale binary instead of the developer's just-edited code, making it
// impossible to debug remote-server changes at all.
//
// So on a source build with no release tag — any profile, including
// `--release`; see `remote_server::setup::is_dev_source_build()` —
// `install_binary()` instead:
//   1. Cross-compiles the local `warp` binary to x86_64 musl (profile /
//      features matching `script/deploy_remote_server` exactly);
//   2. Uploads the artifact over the existing SSH ControlMaster socket via
//      `scp_upload`, to the remote path resolved by
//      `remote_server::setup::remote_server_binary()`;
//   3. Skips the GitHub download install script entirely.
//
// If the cross-compile prerequisites are missing (no musl target installed,
// no musl linker), this doesn't hard-fail — it prints a clear warning and
// falls back to the original download install flow, so dev still works.
// ===========================================================================

/// musl linker candidates that dev-mode cross-compilation may use (in
/// priority order). Usually `x86_64-linux-musl-gcc` on macOS
/// (filosottile/musl-cross), and commonly `musl-gcc` on Linux.
const DEV_MUSL_LINKER_CANDIDATES: &[&str] = &["x86_64-linux-musl-gcc", "musl-gcc"];

/// Returns the current workspace root directory.
///
/// `ssh_transport.rs` belongs to the `app` crate; `CARGO_MANIFEST_DIR` points
/// to `<workspace>/app`, whose parent is the workspace root.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        // In principle `app` always has a parent; fall back to the manifest
        // dir itself just in case it doesn't.
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// Returns PATH with `~/.cargo/bin` (and `$CARGO_HOME/bin`) appended.
///
/// The warp process is often launched by the desktop environment or the
/// system `cargo`, whose PATH may only contain `/usr/bin` and not
/// `~/.cargo/bin`. That would cause:
///   - `cargo zigbuild` failing to find the `cargo-zigbuild` subcommand →
///     falling back to musl-gcc;
///   - cargo-zigbuild itself failing to find `cargo` / `rustc`.
/// All cross-compile-related subprocesses uniformly use the PATH returned
/// here, so both can resolve it. Returns `None` if no adjustment is needed
/// (no HOME / can't join paths), in which case the caller keeps the
/// inherited PATH.
fn dev_build_path_env() -> Option<std::ffi::OsString> {
    let mut extra: Vec<PathBuf> = Vec::new();
    if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
        extra.push(PathBuf::from(cargo_home).join("bin"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        extra.push(PathBuf::from(home).join(".cargo").join("bin"));
    }
    if extra.is_empty() {
        return None;
    }
    let current = std::env::var_os("PATH").unwrap_or_default();
    extra.extend(std::env::split_paths(&current));
    std::env::join_paths(extra).ok()
}

/// Finds the first available musl linker in `PATH`; returns `None` if none found.
fn find_musl_linker() -> Option<&'static str> {
    DEV_MUSL_LINKER_CANDIDATES.iter().copied().find(|linker| {
        command::blocking::Command::new(linker)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

/// Build backend used by dev-mode cross-compilation.
enum DevBuildBackend {
    /// `cargo zigbuild`: zig acts as a complete C/C++ musl cross toolchain,
    /// no need to separately install `*-musl-gcc` / `*-musl-g++`, and can
    /// correctly compile C/C++-source dependencies like `freetype-sys`. This
    /// is the preferred backend.
    Zigbuild,
    /// Native `cargo build` + musl linker. Only reliable when the system has
    /// a complete musl C/C++ cross toolchain installed — if only
    /// `*-musl-gcc` is present without `*-musl-g++`, C++ dependencies like
    /// `freetype-sys` will fail to compile.
    MuslGcc(&'static str),
}

/// Detects whether `cargo-zigbuild` is available.
///
/// Probes `cargo-zigbuild --version` directly (the binary itself), not
/// `cargo zigbuild --version` — the latter would be parsed by the
/// `zigbuild` subcommand as an unknown argument and fail. The probe uses the
/// same PATH as the actual build (with `~/.cargo/bin` injected).
fn cargo_zigbuild_available() -> bool {
    let mut cmd = command::blocking::Command::new("cargo-zigbuild");
    cmd.arg("--version");
    if let Some(path) = dev_build_path_env() {
        cmd.env("PATH", path);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Selects the dev cross-compile backend: prefers `cargo zigbuild`, falling
/// back to native `cargo build` + musl linker. Returns `None` if neither is
/// available, and the caller falls back to the download install.
fn select_dev_build_backend() -> Option<DevBuildBackend> {
    if cargo_zigbuild_available() {
        return Some(DevBuildBackend::Zigbuild);
    }
    find_musl_linker().map(DevBuildBackend::MuslGcc)
}

/// Checks whether the `x86_64-unknown-linux-musl` target has been installed via rustup.
async fn musl_target_installed() -> bool {
    let output = command::r#async::Command::new("rustup")
        .arg("target")
        .arg("list")
        .arg("--installed")
        .kill_on_drop(true)
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim() == remote_server::setup::DEV_MUSL_TARGET),
        // If rustup output can't be obtained, conservatively assume it's not
        // installed, triggering the fallback.
        _ => false,
    }
}

/// Cross-compiles the local `warp` binary to musl, returning the artifact path.
///
/// profile / features are aligned with `script/deploy_remote_server`.
async fn cross_compile_remote_server(backend: &DevBuildBackend) -> Result<PathBuf> {
    let root = workspace_root();
    // `[[bin]]` name for the current channel — the OSS fork is `warp-oss`
    // (see app/Cargo.toml). Can't hardcode `warp`: that bin goes through
    // `load_config!("local")`, which needs the private `warp-channel-config`
    // to generate `local_config.json`; the OSS fork doesn't have it and
    // would fail to compile. `warp-oss` (src/bin/oss.rs) inlines
    // `ChannelConfig` and has no such dependency.
    let bin_name = remote_server::setup::binary_name();
    let backend_desc = match backend {
        DevBuildBackend::Zigbuild => "cargo-zigbuild".to_string(),
        DevBuildBackend::MuslGcc(linker) => format!("cargo-build/{linker}"),
    };
    log::info!(
        "dev remote-server: cross-compiling {bin_name} -> {} (profile={}, backend={backend_desc})",
        remote_server::setup::DEV_MUSL_TARGET,
        remote_server::setup::DEV_REMOTE_PROFILE,
    );
    // The first run compiles the entire warp binary, usually taking several
    // minutes. stdout/stderr are inherited directly into the terminal
    // running Zap, so the developer can see cargo's live build progress
    // (otherwise it's silent the whole time and easy to mistake for a hang).
    log::info!(
        "dev remote-server: cross-compiling, first run usually takes several minutes \
         — cargo progress will print to the terminal running Phosphor"
    );

    let status = async {
        let mut cmd = command::r#async::Command::new("cargo");
        cmd.current_dir(&root);
        // Inject `~/.cargo/bin` to ensure `cargo zigbuild` can resolve the
        // `cargo-zigbuild` subcommand, and that cargo-zigbuild itself can
        // find `cargo` / `rustc`.
        if let Some(path) = dev_build_path_env() {
            cmd.env("PATH", path);
        }
        match backend {
            // zigbuild is a cargo subcommand that bundles its own zig linker
            // and C/C++ cross compiler, so no LINKER env needs to be set.
            DevBuildBackend::Zigbuild => {
                cmd.arg("zigbuild");
            }
            // Native cargo build: specify the musl linker via env and
            // override rustflags, to avoid macOS-only flags in
            // .cargo/config.toml polluting the cross-compile.
            DevBuildBackend::MuslGcc(linker) => {
                cmd.arg("build")
                    .env("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", *linker)
                    .env(
                        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS",
                        "-C symbol-mangling-version=v0",
                    );
            }
        }
        cmd.arg("-p")
            .arg("warp")
            .arg("--bin")
            .arg(bin_name)
            .arg("--target")
            .arg(remote_server::setup::DEV_MUSL_TARGET)
            .arg("--profile")
            .arg(remote_server::setup::DEV_REMOTE_PROFILE)
            .arg("--features")
            .arg(remote_server::setup::DEV_REMOTE_FEATURES)
            // inherit: pass cargo's live progress through to the terminal
            // instead of buffering it silently the whole time.
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .status()
            .await
    }
    .with_timeout(remote_server::setup::DEV_CROSS_COMPILE_TIMEOUT)
    .await
    .map_err(|_| {
        anyhow!(
            "dev remote-server cross-compile timed out (>{:?})",
            remote_server::setup::DEV_CROSS_COMPILE_TIMEOUT
        )
    })?
    .map_err(|e| anyhow!("failed to start cargo build: {e}"))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(anyhow!(
            "cargo cross-compile failed (exit {code}); see cargo output in the terminal running Phosphor"
        ));
    }

    // Artifact location: `<target_dir>/<triple>/<profile>/<bin_name>`.
    // Prefer reading `CARGO_TARGET_DIR`, otherwise fall back to
    // `<workspace>/target`. The repo doesn't set `[build] target-dir` in
    // `.cargo/config.toml`, so only the env var needs to be considered.
    let target_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let binary = target_root
        .join(remote_server::setup::DEV_MUSL_TARGET)
        .join(remote_server::setup::DEV_REMOTE_PROFILE)
        .join(bin_name);
    if !binary.is_file() {
        return Err(anyhow!(
            "cross-compile finished but no artifact found at {} (if CARGO_TARGET_DIR is set, verify the path)",
            binary.display()
        ));
    }
    Ok(binary)
}

/// Dev-mode install: cross-compiles the local `warp` binary and uploads it
/// to the remote remote-server path.
///
/// The upload target matches `remote_server_binary()` exactly, ensuring the
/// subsequent `check_binary()` / proxy startup can find it.
async fn dev_install_local_binary(socket_path: &Path) -> Result<()> {
    // Prerequisite checks: if any is missing, return an error and let the
    // caller fall back to the download install.
    if !musl_target_installed().await {
        return Err(anyhow!(
            "rust target {} is not installed; run `rustup target add {}`",
            remote_server::setup::DEV_MUSL_TARGET,
            remote_server::setup::DEV_MUSL_TARGET,
        ));
    }
    // Select the cross-compile backend: prefer `cargo zigbuild` (zig bundles
    // a complete C/C++ musl toolchain and can compile C++ dependencies like
    // freetype-sys), otherwise fall back to musl-gcc. Error if neither is
    // available.
    let backend = select_dev_build_backend().ok_or_else(|| {
        anyhow!(
            "no usable musl cross-compile backend found. Install cargo-zigbuild + zig\
             (`cargo install cargo-zigbuild`, and install `zig` via your package manager),\
             or install a complete musl C/C++ cross toolchain ({})",
            DEV_MUSL_LINKER_CANDIDATES.join(" / ")
        )
    })?;

    let local_binary = cross_compile_remote_server(&backend).await?;

    // Upload to the exact path resolved by `remote_server_binary()`, first
    // creating the parent directory.
    let remote_binary = remote_server::setup::remote_server_binary();
    let remote_dir = remote_server::setup::remote_server_dir();
    let mkdir_output = remote_server::ssh::run_ssh_command(
        socket_path,
        &format!("mkdir -p {remote_dir}"),
        remote_server::setup::CHECK_TIMEOUT,
    )
    .await?;
    if !mkdir_output.status.success() {
        let code = mkdir_output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&mkdir_output.stderr);
        return Err(anyhow!(
            "remote remote-server directory creation failed (exit {code}): {stderr}"
        ));
    }

    log::info!("dev remote-server: uploading local cross-compiled artifact to {remote_binary} (scp -C compression, may take several minutes for hundreds of MB)");
    // Dev artifacts are hundreds of MB, so use DEV_UPLOAD_TIMEOUT (far
    // longer than SCP_INSTALL_TIMEOUT), to avoid a large file upload being
    // interrupted by the 120s timeout and falling back to downloading a
    // stale release.
    remote_server::ssh::scp_upload(
        socket_path,
        &local_binary,
        &remote_binary,
        remote_server::setup::DEV_UPLOAD_TIMEOUT,
    )
    .await?;

    // Grant executable permission.
    let chmod_output = remote_server::ssh::run_ssh_command(
        socket_path,
        &format!("chmod 755 {remote_binary}"),
        remote_server::setup::CHECK_TIMEOUT,
    )
    .await?;
    if !chmod_output.status.success() {
        let code = chmod_output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&chmod_output.stderr);
        return Err(anyhow!("remote chmod failed (exit {code}): {stderr}"));
    }

    // Reuse the existing verification logic to confirm the uploaded binary runs.
    verify_installed_binary(socket_path).await
}

async fn download_remote_server_tarball(download_url: &str, tarball_path: &Path) -> Result<()> {
    let output = async {
        // `--proto`/`--proto-redir` match what the remote path in
        // `install_remote_server.sh` sets: release downloads legitimately
        // redirect to a CDN so `-L` has to stay, but a redirect must not be
        // allowed to downgrade the transport to plain HTTP.
        command::r#async::Command::new("curl")
            .arg("-fSL")
            .arg("--proto")
            .arg("=https")
            .arg("--proto-redir")
            .arg("=https")
            .arg("--connect-timeout")
            .arg("15")
            .arg(download_url)
            .arg("-o")
            .arg(tarball_path.as_os_str())
            .kill_on_drop(true)
            .output()
            .await
    }
    .with_timeout(remote_server::setup::SCP_INSTALL_TIMEOUT)
    .await
    .map_err(|_| {
        anyhow!(
            "local tarball download timed out after {:?}",
            remote_server::setup::SCP_INSTALL_TIMEOUT
        )
    })?
    .map_err(|e| anyhow!("local curl failed to execute: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!(
        "local tarball download failed with code {code}: {stderr}"
    ))
}

async fn scp_install_fallback(socket_path: &Path) -> Result<()> {
    let platform = detect_remote_platform(socket_path).await?;
    let download_url = remote_server::setup::download_tarball_url(&platform);
    let remote_server_dir = remote_server::setup::remote_server_dir();
    let mkdir_cmd = format!("mkdir -p {remote_server_dir}");
    let mkdir_output = remote_server::ssh::run_ssh_command(
        socket_path,
        &mkdir_cmd,
        remote_server::setup::CHECK_TIMEOUT,
    )
    .await?;

    if !mkdir_output.status.success() {
        let code = mkdir_output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&mkdir_output.stderr);
        return Err(anyhow!(
            "remote-server dir creation failed with code {code}: {stderr}"
        ));
    }

    let tempdir = tempfile::tempdir()?;
    let tarball_path = tempdir.path().join("zap.tar.gz");
    download_remote_server_tarball(&download_url, &tarball_path).await?;

    let remote_tarball_path = format!("{remote_server_dir}/zap-upload.tar.gz");
    remote_server::ssh::scp_upload(
        socket_path,
        &tarball_path,
        &remote_tarball_path,
        remote_server::setup::SCP_INSTALL_TIMEOUT,
    )
    .await?;

    run_install_script(
        socket_path,
        Some(&remote_tarball_path),
        remote_server::setup::SCP_INSTALL_TIMEOUT,
    )
    .await
    .map_err(|error| anyhow!("staged install failed: {error}"))?;

    verify_installed_binary(socket_path).await
}

impl RemoteTransport for SshTransport {
    fn detect_platform(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<RemotePlatform, String>> + Send>> {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            detect_remote_platform(&socket_path)
                .await
                .map_err(|e| format!("{e:#}"))
        })
    }

    fn run_preinstall_check(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PreinstallCheckResult, String>> + Send>> {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            match remote_server::ssh::run_ssh_script(
                &socket_path,
                remote_server::setup::PREINSTALL_CHECK_SCRIPT,
                remote_server::setup::CHECK_TIMEOUT,
            )
            .await
            {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Ok(PreinstallCheckResult::parse(&stdout))
                }
                Ok(output) => {
                    let code = output.status.code().unwrap_or(-1);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(format!(
                        "Preinstall check exited with code {code}: {stderr}"
                    ))
                }
                Err(e) => Err(format!("{e:#}")),
            }
        })
    }

    fn check_binary(&self) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            let bin_path = remote_server::setup::remote_server_binary();
            log::info!("Checking for remote server binary at {bin_path}");
            match remote_server::ssh::run_ssh_command(
                &socket_path,
                &remote_server::setup::binary_check_command(),
                remote_server::setup::CHECK_TIMEOUT,
            )
            .await
            {
                // `{binary} --version` exiting 0 means it exists and runs.
                // 126/127 means missing or not executable; any other
                // nonzero exit is treated as a real check failure.
                Ok(output) => match output.status.code() {
                    Some(0) => Ok(true),
                    Some(126) | Some(127) => Ok(false),
                    Some(code) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(format!("binary check exited with code {code}: {stderr}"))
                    }
                    None => Err("binary check terminated by signal".into()),
                },
                Err(e) => Err(format!("{e:#}")),
            }
        })
    }

    fn check_has_old_binary(&self) -> Pin<Box<dyn Future<Output = anyhow::Result<bool>> + Send>> {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            // Treat the existence of the remote-server install directory
            // itself as evidence of a prior install. If `~/.warp-XX/remote-server`
            // exists, something was installed there before, so any mismatch
            // with the client's expected binary path should be auto-updated
            // rather than surfaced as a first-time install prompt.
            let cmd = format!("test -d {}", remote_server::setup::remote_server_dir());
            let output = remote_server::ssh::run_ssh_command(
                &socket_path,
                &cmd,
                remote_server::setup::CHECK_TIMEOUT,
            )
            .await?;
            // `test -d` exits 0 when present, 1 when missing.
            // Anything else is treated as a check failure.
            match output.status.code() {
                Some(0) => Ok(true),
                Some(1) => Ok(false),
                Some(code) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(anyhow::anyhow!(
                        "remote-server dir check exited with code {code}: {stderr}"
                    ))
                }
                None => Err(anyhow::anyhow!(
                    "remote-server dir check terminated by signal"
                )),
            }
        })
    }

    fn install_binary(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            log::info!(
                "Installing remote server binary to {}",
                remote_server::setup::remote_server_binary()
            );

            // Zap fork: a DEBUG source build (no release tag) takes the
            // dev-mode path — cross-compiling and uploading the local
            // `warp` binary instead of downloading a stale GitHub release.
            // On failure (missing cross-compile prerequisites, etc.), print
            // a warning and fall back to the download install, so the dev
            // experience isn't broken. Release builds skip this whole block
            // and behave unchanged.
            if remote_server::setup::is_dev_source_build() {
                log::info!("dev remote-server: detected DEBUG source build, switching to local cross-compile install");
                match dev_install_local_binary(&socket_path).await {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        log::warn!(
                            "dev remote-server: local cross-compile install unavailable, falling back to download install: {error:#}"
                        );
                        // Fell through — continue with the regular download install flow below.
                    }
                }
            }

            match run_install_script(&socket_path, None, remote_server::setup::INSTALL_TIMEOUT)
                .await
            {
                Ok(()) => verify_installed_binary(&socket_path)
                    .await
                    .map_err(|error| format!("{error:#}")),
                // All of the policy lives in `route_install_failure`, which is
                // what the tests exercise. This arm must stay a two-way
                // dispatch with no judgement of its own: the previous version
                // consulted a boolean predicate here, and the predicate was
                // well tested while this call site was not tested at all.
                Err(error) => match route_install_failure(&error) {
                    InstallFallbackRoute::Fail(message) => {
                        log::error!("remote-server install failed, no fallback: {message}");
                        Err(message)
                    }
                    InstallFallbackRoute::ScpFallback => {
                        log::warn!("remote-server install failed, trying SCP fallback: {error}");
                        match scp_install_fallback(&socket_path).await {
                            Ok(()) => Ok(()),
                            Err(fallback_error) => {
                                Err(format!("{error}; SCP fallback failed: {fallback_error:#}"))
                            }
                        }
                    }
                },
            }
        })
    }

    fn connect(
        &self,
        executor: Arc<executor::Background>,
    ) -> Pin<Box<dyn Future<Output = Result<Connection>> + Send>> {
        let socket_path = self.socket_path.clone();
        let owns_control_master = self.owns_control_master;
        let remote_proxy_command = self.remote_proxy_command();
        Box::pin(async move {
            let mut args = ssh_args(&socket_path);
            args.push(remote_proxy_command);

            // `kill_on_drop(true)` pairs with ownership of the `Child` being
            // returned in the [`Connection`] below: the
            // [`RemoteServerManager`] holds the `Child` on its per-session
            // state, and dropping that state (on explicit teardown or
            // spontaneous disconnect) sends SIGKILL to this ssh process.
            let mut child = command::r#async::Command::new("ssh")
                .args(&args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;

            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| anyhow::anyhow!("Failed to capture child stdin"))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("Failed to capture child stdout"))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| anyhow::anyhow!("Failed to capture child stderr"))?;

            let (client, event_rx, host_response_rx, stderr_tail) =
                RemoteServerClient::from_child_streams(stdin, stdout, stderr, &executor);
            Ok(Connection {
                client,
                event_rx,
                host_response_rx,
                child,
                stderr_tail,
                // Only tag the socket for teardown when we own the master;
                // a reused user-owned master maps to `None` so teardown
                // leaves it running (GitHub issue #37).
                control_path: Self::control_path_for_teardown(owns_control_master, socket_path),
            })
        })
    }

    fn remove_remote_server_binary(
        &self,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            let cmd = remote_server::setup::remote_server_removal_command();
            log::info!("Removing stale remote server binary: {cmd}");
            let output = remote_server::ssh::run_ssh_command(
                &socket_path,
                &cmd,
                remote_server::setup::CHECK_TIMEOUT,
            )
            .await?;
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow::anyhow!("Failed to remove binary: {stderr}"))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use warpui::r#async::BoxFuture;
    fn static_auth_context() -> Arc<RemoteServerAuthContext> {
        Arc::new(RemoteServerAuthContext::new(
            || -> BoxFuture<'static, Option<String>> { Box::pin(async { None }) },
            || "user id/with spaces".to_string(),
        ))
    }

    #[test]
    fn remote_proxy_command_quotes_identity_key() {
        let transport = SshTransport::new(
            PathBuf::from("/tmp/control-master.sock"),
            static_auth_context(),
            true,
        );

        let command = transport.remote_proxy_command();

        assert!(command.contains("remote-server-proxy --identity-key"));
        assert!(command.contains("'user id/with spaces'"));
    }

    // Regression test for GitHub issue #37: teardown unconditionally tore
    // down the ControlMaster even when we reused (rather than created) it,
    // killing the user's other multiplexed SSH connections. The
    // `control_path` returned by `connect()` — which drives whether the
    // manager runs `ssh -O exit` on teardown — must be gated on ownership.
    #[test]
    fn control_path_gated_on_control_master_ownership() {
        let socket_path = PathBuf::from("/tmp/control-master.sock");

        // We created the master: teardown must run, so a path is returned.
        assert_eq!(
            SshTransport::control_path_for_teardown(true, socket_path.clone()),
            Some(socket_path.clone()),
            "owned ControlMaster must yield a control_path so teardown runs `ssh -O exit`",
        );

        // We reused a user-owned master: teardown must NOT run, so `None`.
        assert_eq!(
            SshTransport::control_path_for_teardown(false, socket_path.clone()),
            None,
            "reused user-owned ControlMaster must yield None so teardown leaves it running",
        );
    }

    // The `owns_control_master` flag passed to `new()` is exposed verbatim
    // via the accessor the controller uses to thread ownership into the
    // reconnect path.
    #[test]
    fn owns_control_master_accessor_reflects_constructor() {
        let socket_path = PathBuf::from("/tmp/control-master.sock");
        assert!(
            SshTransport::new(socket_path.clone(), static_auth_context(), true)
                .owns_control_master()
        );
        assert!(
            !SshTransport::new(socket_path, static_auth_context(), false).owns_control_master()
        );
    }

    fn script_failure(exit_code: i32) -> InstallError {
        InstallError::ScriptFailed {
            exit_code,
            stderr: String::new(),
        }
    }

    fn falls_back(error: &InstallError) -> bool {
        route_install_failure(error) == InstallFallbackRoute::ScpFallback
    }

    // ---------------------------------------------------------------------
    // The routing these tests pin is `route_install_failure`, which is the
    // value `install_binary` matches on directly — including the exact string
    // it returns on the fail-closed arms. An earlier round tested a
    // `should_skip_scp_fallback` predicate instead; the predicate was right
    // and the shipped behaviour was still wrong, partly because the call site
    // had no test of its own and partly because the script never emitted the
    // codes the predicate classified (see `classify_install_failure`). Hence
    // both `route_install_failure` coverage and
    // `exit_codes_in_install_script_are_all_classified` below.
    // ---------------------------------------------------------------------

    // The SCP fallback re-fetches the same GitHub release tarball locally and
    // installs it through the script's *unverified* staging branch. So every
    // integrity failure the script reports must stop there rather than fall
    // through: exit 4 (client built with no pinned digest), 5 (no SHA-256 tool
    // on the host) and 6 (digest mismatch — i.e. detected tampering).
    #[test]
    fn integrity_failures_do_not_fall_through_to_unverified_scp_fallback() {
        for exit_code in [4, 5, 6] {
            assert_eq!(
                classify_install_failure(&script_failure(exit_code)),
                InstallFailureKind::IntegrityFailed,
            );
            assert!(
                !falls_back(&script_failure(exit_code)),
                "exit {exit_code} is an integrity failure and must fail closed, \
                 not fall through to the unverified SCP fallback",
            );
        }
    }

    // The security property, stated on its own so it cannot be lost inside a
    // loop over three codes: exit 6 means the download SUCCEEDED and the bytes
    // are not the release this client pinned. That is what tampering looks
    // like from here, and it must be reported as a refusal — not retried, and
    // not reported as a generic install error an operator would skim past.
    #[test]
    fn digest_mismatch_is_refused_loudly_and_never_retried() {
        let route = route_install_failure(&script_failure(6));
        let InstallFallbackRoute::Fail(message) = route else {
            panic!("exit 6 (digest mismatch) must not route to the SCP fallback");
        };
        assert!(
            message.contains("integrity check failed"),
            "the refusal must say an integrity check failed, got: {message}",
        );
        assert!(
            message.contains("NOT attempted"),
            "the refusal must say the unverified fallback was not attempted, got: {message}",
        );
    }

    // Unsupported arch/OS: no asset exists, so retrying the same download
    // locally cannot help either.
    //
    // The code is 10 rather than 2 because bash emits 2 for a *parse* error,
    // which the ERR trap cannot catch (the interpreter never gets far enough
    // to run a trap) — and a mangled placeholder substitution in
    // `setup::install_script` is exactly what would produce one. While this
    // outcome owned 2, a script bash refused to parse arrived here claiming
    // the remote host had an unsupported architecture.
    #[test]
    fn unsupported_platform_does_not_fall_through() {
        assert_eq!(
            classify_install_failure(&script_failure(10)),
            InstallFailureKind::Fatal,
        );
        assert!(!falls_back(&script_failure(10)));
    }

    // The SSH-level failure the client sees most often, and the one this
    // classification exists for. `run_ssh_script` returns `Ok(output)`
    // whenever the `ssh` process itself ran, so a dropped connection, a dead
    // ControlMaster, a changed host key or a signal-killed remote command all
    // arrive as a script exit of 255 — OpenSSH's own error status — rather
    // than as `InstallError::Other`.
    //
    // It must route exactly like `Other` does: the script never ran to
    // completion, so nothing was fetched, nothing was verified and nothing was
    // installed. A round that narrowed the fallback to a code list left 255 in
    // the `_ => Fatal` catch-all, which reported "install script failed (exit
    // 255)" for a script that never ran and removed the fallback from the very
    // class of failure it exists for.
    #[test]
    fn ssh_own_error_status_still_uses_scp_fallback() {
        assert_eq!(
            classify_install_failure(&script_failure(255)),
            InstallFailureKind::TransportFailed,
            "exit 255 is OpenSSH reporting its own failure, not a verdict from \
             the install script",
        );
        assert!(
            falls_back(&script_failure(255)),
            "exit 255 means the script did not run, so nothing was verified and \
             the SCP fallback is the designed recovery — same as an \
             InstallError::Other",
        );
    }

    // A tarball that matched the pinned digest and then would not open, or
    // held no recognised binary. The bytes are exactly what this client asked
    // for, so the fallback would fetch them again and fail identically.
    #[test]
    fn unusable_verified_tarball_does_not_fall_through() {
        assert_eq!(
            classify_install_failure(&script_failure(8)),
            InstallFailureKind::Fatal,
        );
        assert!(!falls_back(&script_failure(8)));
    }

    // Exit 3 is the case the fallback exists for: the remote host has neither
    // curl nor wget, but the client does. Nothing was downloaded remotely, so
    // no integrity claim was violated.
    #[test]
    fn missing_remote_fetcher_still_uses_scp_fallback() {
        assert_eq!(
            classify_install_failure(&script_failure(3)),
            InstallFailureKind::TransportFailed,
        );
        assert!(
            falls_back(&script_failure(3)),
            "exit 3 (no curl/wget on the host) is what the SCP fallback is for",
        );
    }

    // Exit 7 is the other half of the same requirement and the one this round
    // added: the host HAS a fetcher and the fetch failed — DNS, connection
    // refused, TLS, timeout, HTTP 4xx/5xx. Nothing arrived, so nothing was
    // verified and nothing was violated; the client fetching instead is the
    // designed recovery, exactly as for exit 3.
    //
    // This case used to be unreachable. The script ran curl under `set -e`, so
    // a DNS failure exited with curl's own 6 and was hard-failed as a digest
    // mismatch — a regression against the fallback that used to work — while
    // curl 7 and 22 fell through unrecognised into an unverified install.
    #[test]
    fn remote_download_failure_still_uses_scp_fallback() {
        assert_eq!(
            classify_install_failure(&script_failure(7)),
            InstallFailureKind::TransportFailed,
        );
        assert!(
            falls_back(&script_failure(7)),
            "exit 7 (the host could not download) is a transport failure, not an \
             integrity failure: nothing arrived, so nothing failed verification",
        );
    }

    // Split out of the former `non_script_failures_still_use_scp_fallback`,
    // which asserted two unrelated things under one name.
    //
    // The half that encodes a real requirement: an `InstallError::Other` is an
    // SSH-level failure — the socket died, the run timed out, the process was
    // killed. The script never returned a verdict, so there is no verdict to
    // override and nothing unverified was installed. Falling back is correct.
    #[test]
    fn ssh_level_failures_still_use_scp_fallback() {
        let error = InstallError::Other(anyhow!("ssh connection dropped"));
        assert_eq!(
            classify_install_failure(&error),
            InstallFailureKind::TransportFailed,
        );
        assert!(falls_back(&error));
    }

    // The half that encoded the defect. The old test asserted that script exit
    // 1 falls back, on the reasoning "any non-zero we don't recognise is
    // harmless". That is the bug in miniature: an unrecognised code means we
    // cannot tell whether verification ran, and "could not check" must not
    // collapse into "check passed". Unknown codes now fail closed.
    //
    // Exit 1 in particular is what a bare `set -e` abort used to surface as,
    // which is precisely the status that carries no information about whether
    // the digest was checked.
    //
    // The list includes bash's own statuses — 1 (general error), 2 (parse
    // error), 126 (found but not executable), 127 (not found) — because no
    // contract code may claim one of them; see
    // `install_script_exit_codes_avoid_bash_reserved_statuses` in
    // `crates/remote_server/src/setup_tests.rs`. 255 is deliberately absent:
    // it is SSH's, and it is classified above.
    #[test]
    fn unrecognised_script_exit_codes_fail_closed() {
        for exit_code in [1, 2, 22, 42, 126, 127, -1] {
            assert_eq!(
                classify_install_failure(&script_failure(exit_code)),
                InstallFailureKind::Fatal,
                "exit {exit_code} is not in the script's contract",
            );
            assert!(
                !falls_back(&script_failure(exit_code)),
                "exit {exit_code} carries no evidence that verification ran, so it must \
                 not authorise the unverified SCP fallback",
            );
        }
    }

    // Exit 9 is the script's own backstop: `set -e` aborted somewhere the
    // script did not anticipate, and it remaps that onto a reserved code
    // rather than letting the failing command's status leak out. It is
    // declared, but it still says "we do not know what happened" — so it fails
    // closed for the same reason the unrecognised codes above do.
    #[test]
    fn unclassified_script_abort_fails_closed() {
        assert_eq!(
            classify_install_failure(&script_failure(9)),
            InstallFailureKind::Fatal,
        );
        assert!(!falls_back(&script_failure(9)));
    }

    // Ties the Rust contract to the shell one. `install_remote_server.sh`
    // declares its codes as `EXIT_<NAME>=<n>` assignments and uses only those
    // names in its `exit` statements; this parses them back out of the
    // rendered script and asserts each is deliberately classified rather than
    // landing in the `Fatal` catch-all by accident.
    //
    // Without this, the two halves drift: the previous round classified codes
    // 4/5/6 correctly in Rust while the script emitted curl's status instead,
    // and nothing failed.
    #[test]
    fn exit_codes_in_install_script_are_all_classified() {
        let script = remote_server::setup::install_script(None);

        let declared: Vec<(String, i32)> = script
            .lines()
            .filter_map(|line| line.trim().strip_prefix("EXIT_")?.split_once('='))
            .filter_map(|(name, value)| Some((format!("EXIT_{name}"), value.trim().parse().ok()?)))
            .collect();

        assert!(
            declared.len() >= 8,
            "expected the script to declare its exit-code contract as EXIT_*= assignments, \
             found {declared:?}",
        );

        // Every declared code must be classified the way the contract says.
        let expected = |name: &str| match name {
            "EXIT_NO_FETCHER" | "EXIT_DOWNLOAD_FAILED" => InstallFailureKind::TransportFailed,
            "EXIT_NO_PINNED_DIGEST" | "EXIT_NO_DIGEST_TOOL" | "EXIT_DIGEST_MISMATCH" => {
                InstallFailureKind::IntegrityFailed
            }
            _ => InstallFailureKind::Fatal,
        };

        for (name, code) in &declared {
            // No contract code may squat on OpenSSH's own error status: the
            // client cannot tell "the script said 255" from "ssh said 255",
            // and it now reads 255 as the latter.
            assert_ne!(
                *code, 255,
                "{name} claims exit 255, which is OpenSSH's own error status and \
                 is classified as an SSH-level transport failure",
            );
            assert_eq!(
                classify_install_failure(&script_failure(*code)),
                expected(name.as_str()),
                "{name} (exit {code}) is classified against the contract in \
                 install_remote_server.sh; update both files together",
            );
        }

        // And the codes the script actually exits with must all be declared
        // ones, so no literal `exit 6` can sneak past the table above.
        for line in script.lines() {
            for token in line.split("exit ").skip(1) {
                let literal = token.trim_start().trim_start_matches('"');
                if literal.starts_with('$') {
                    continue;
                }
                let digits: String = literal.chars().take_while(char::is_ascii_digit).collect();
                assert!(
                    digits.is_empty(),
                    "install_remote_server.sh exits with the literal {digits} in {line:?}; \
                     use one of the EXIT_* names so the contract stays in one place",
                );
            }
        }
    }
}
