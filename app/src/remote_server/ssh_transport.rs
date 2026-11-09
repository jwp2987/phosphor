//! SSH-specific implementation of [`RemoteTransport`].
//!
//! [`SshTransport`] uses an existing SSH ControlMaster socket to check/install
//! the bundled `phosphor-remote` helper (uploaded in-band, never downloaded) and to launch its
//! `proxy` process whose stdin/stdout become the protocol channel.
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use warpui::r#async::{FutureExt as _, executor};

use remote_server::auth::RemoteServerAuthContext;
use remote_server::client::RemoteServerClient;
use remote_server::setup::{
    HelperDigest, PreinstallCheckResult, RemotePlatform, decide_helper_digest, parse_uname_output,
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
        format!("{binary} proxy {quoted_identity_key}")
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

/// Runs the install script on the host with `payload` (the gzip-compressed helper, or empty
/// for the SCP-staged variant) appended on the same stdin.
async fn run_install_script(
    socket_path: &Path,
    script: &str,
    payload: &[u8],
    timeout: std::time::Duration,
) -> core::result::Result<(), InstallError> {
    match remote_server::ssh::run_ssh_script_with_payload(socket_path, script, payload, timeout)
        .await
    {
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
/// The in-band upload and the SCP fallback carry the SAME locally bundled bytes and run the SAME
/// verifying script, so unlike the old download-then-scp design the fallback is never an
/// unverified route: it exists only for when the in-band stream itself could not deliver.
/// Integrity verdicts are still never retried, because a retry of the same bytes cannot change
/// them and a verdict must not be shopped around.
///
/// The three outcomes are told apart by the script's exit-code contract. Fusing any two of
/// them is the whole bug: "no bytes arrived" and "the bytes are not the helper this client
/// pinned" are opposite verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallFailureKind {
    /// No usable bytes reached the host: the stream ended before any data decoded (exit 7), or
    /// ssh itself failed so the script did not run to completion at all, which reaches us in
    /// *two* shapes: as [`InstallError::Other`] (ssh could not be spawned, the write to its
    /// stdin failed, the run timed out), and as a reported script exit of **255**, OpenSSH's own
    /// error status rather than anything this script can emit.
    ///
    /// No integrity claim was made and none was violated, and nothing was installed. Uploading
    /// the file with scp and unpacking it on the host is the designed recovery.
    TransportFailed,
    /// Integrity could not be established, or was established and FAILED: exit 4 (this client
    /// was built with no pinned digest), 5 (no SHA-256 tool on the host, so the digest cannot be
    /// checked) and 6 (digest MISMATCH). Fails closed; the fallback must not run.
    IntegrityFailed,
    /// Fatal for a reason the fallback cannot fix: exit 10 (unsupported arch/OS), 3 (the host
    /// has neither gunzip nor gzip, which the scp route needs too), 8 (the uploaded stream
    /// decoded partially and then failed), 9 (the helper does not start on this host, or the
    /// script failed somewhere unclassified).
    ///
    /// Also the default for any code this client does not recognise. That default is
    /// deliberately fail-closed: an unknown code means we cannot tell whether verification
    /// happened, and "could not check" must never collapse into "check passed". It is also what
    /// stops a future script revision from silently re-opening this hole.
    Fatal,
}

/// Classifies an install failure against the exit-code contract documented at the top of
/// `crates/remote_server/src/install_remote_server.sh`.
///
/// Both files carry the same table and `exit_codes_in_install_script_are_all_classified` pins
/// them together by parsing the script's own `EXIT_*=` assignments.
fn classify_install_failure(error: &InstallError) -> InstallFailureKind {
    match error {
        // The SSH run itself failed -- the socket died, the script timed out, the process was
        // killed. The script never reported a verdict, so there is no verdict to override;
        // nothing was installed.
        InstallError::Other(_) => InstallFailureKind::TransportFailed,
        InstallError::ScriptFailed { exit_code, .. } => match *exit_code {
            7 => InstallFailureKind::TransportFailed,
            4..=6 => InstallFailureKind::IntegrityFailed,
            // 255 is SSH's status, not the script's, and it arrives here wearing the script's
            // clothes: `run_ssh_script_with_payload` returns `Ok(output)` whenever the `ssh`
            // *process* ran, and OpenSSH reports its own failures -- connection closed,
            // ControlMaster gone, host key changed, remote command killed by a signal -- as
            // exit 255. The script cannot collide with it: every explicit `exit` uses an EXIT_*
            // constant (all <= 10), its ERR trap remaps any stray status onto 9, and
            // `exit_codes_in_install_script_are_all_classified` parses the constants out of the
            // rendered script to keep that true.
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
///
/// The three exit codes `classify_install_failure` groups into
/// `IntegrityFailed` (4, 5, 6 — see `install_remote_server.sh`'s
/// `EXIT_NO_PINNED_DIGEST` / `EXIT_NO_DIGEST_TOOL` / `EXIT_DIGEST_MISMATCH`)
/// get a distinct headline each here, rather than one shared "integrity check
/// failed" wrapper. Exit 4 (this client was built with no pinned digest —
/// every local/dev build) and 5 (the remote host has no digest tool) are both
/// correct, unremarkable fail-closed refusals with nothing to do with
/// tampering; only 6 (bytes arrived and do not match) is that. The script's
/// own reason was always present in `error`'s `Display`, just appended after
/// a headline that called all three "integrity check failed" — which reads as
/// detected tampering even when it plainly is not (TODO.md "Remote-session
/// setup degrades silently", item 4: "correct fail-closed behaviour, but the
/// only surfacing is one log line" undersold it slightly — it already reached
/// the SSH-remote-server-failed banner (`terminal/view/ssh_remote_server_failed_banner.rs`),
/// but with this same misleading headline).
fn route_install_failure(error: &InstallError) -> InstallFallbackRoute {
    match classify_install_failure(error) {
        InstallFailureKind::TransportFailed => InstallFallbackRoute::ScpFallback,
        InstallFailureKind::IntegrityFailed => {
            let headline = match error {
                InstallError::ScriptFailed { exit_code: 4, .. } => {
                    remote_server::setup::INSTALL_REFUSED_NO_PINNED_DIGEST
                }
                InstallError::ScriptFailed { exit_code: 5, .. } => {
                    remote_server::setup::INSTALL_REFUSED_NO_DIGEST_TOOL
                }
                // Deliberately louder than the other two: this is exit 6, where
                // bytes served for this release did NOT match the digest
                // compiled into this client, which is what tampering looks
                // like from here. Operators need to see that it was refused
                // rather than quietly retried.
                _ => remote_server::setup::INSTALL_REFUSED_DIGEST_MISMATCH,
            };
            InstallFallbackRoute::Fail(format!("{headline}: {error}"))
        }
        InstallFailureKind::Fatal => InstallFallbackRoute::Fail(error.to_string()),
    }
}

// ===========================================================================
// Helper preparation: bundled asset, or (source builds) a local cross-compile
//
// The helper the app uploads is a gzip file shipped in the app bundle
// (`<bundled_resources>/remote/phosphor-remote-<os>-<arch>.gz`, put there by
// `script/bundle`). Nothing is downloaded. A source build has no bundle, so
// when `remote_server::setup::is_dev_source_build()` is true and no asset
// exists, `install_binary()` cross-compiles the helper itself (a small crate:
// `-p remote_server --no-default-features --features daemon`, the same profile
// CI uses) and uploads that through the identical in-band path. Without
// prerequisites (no musl target, no linker) the helper is simply unavailable
// and the session stays on the no-helper path.
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

/// Cross-compiles the `phosphor-remote` helper to musl, returning the artifact path.
///
/// profile / features are aligned with `script/deploy_remote_server`.
async fn cross_compile_remote_server(backend: &DevBuildBackend) -> Result<PathBuf> {
    let root = workspace_root();
    // The helper's `[[bin]]` in `crates/remote_server` (`required-features = ["daemon"]`).
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
    // The first run compiles the helper's small crate graph. stdout/stderr are inherited directly into the terminal
    // running Zap, so the developer can see cargo's live build progress
    // (otherwise it's silent the whole time and easy to mistake for a hang).
    log::info!(
        "dev remote-server: cross-compiling the helper — cargo progress will print to the \
         terminal running Phosphor"
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
            .arg("remote_server")
            .arg("--bin")
            .arg(bin_name)
            .arg("--target")
            .arg(remote_server::setup::DEV_MUSL_TARGET)
            .arg("--profile")
            .arg(remote_server::setup::DEV_REMOTE_PROFILE)
            .arg("--no-default-features")
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

/// The compressed helper for one platform and the digest the install requires of it.
struct PreparedHelper {
    gz: Vec<u8>,
    digest: HelperDigest,
}

/// Lowercase hex SHA-256 of what `gz` decompresses to.
fn sha256_of_gunzipped(gz: &[u8]) -> std::io::Result<String> {
    use sha2::{Digest as _, Sha256};
    use std::io::Read as _;
    let mut decoder = flate2::read::GzDecoder::new(gz);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = decoder.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn gzip_bytes(raw: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::io::Write as _;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(raw)?;
    encoder.finish()
}

/// Why no helper could be prepared. The session stays on the no-helper path; the text reaches
/// the setup-failed banner, so it says what to do about it.
fn helper_unavailable(platform: &RemotePlatform, why: &str) -> String {
    format!(
        "the phosphor-remote helper is not available for {}-{} ({why}); continuing without it",
        platform.os.as_str(),
        platform.arch.as_str()
    )
}

/// Reads the bundled compressed helper for `platform`; `Ok(None)` when this app bundle has none.
async fn load_bundled_helper(platform: &RemotePlatform) -> Result<Option<Vec<u8>>> {
    let Some(path) = remote_server::setup::bundled_helper_asset_path(platform) else {
        return Ok(None);
    };
    match async_fs::read(&path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow!("could not read {}: {e}", path.display())),
    }
}

/// Source builds only: cross-compiles the helper locally and returns it gzip-compressed.
async fn build_dev_helper(platform: &RemotePlatform) -> Result<Vec<u8>> {
    // The dev build targets x86_64 musl only (`DEV_MUSL_TARGET`).
    if platform.os.as_str() != "linux" || platform.arch.as_str() != "x86_64" {
        return Err(anyhow!(
            "source builds cross-compile the helper for {} only",
            remote_server::setup::DEV_MUSL_TARGET
        ));
    }
    if !musl_target_installed().await {
        return Err(anyhow!(
            "rust target {} is not installed; run `rustup target add {}`",
            remote_server::setup::DEV_MUSL_TARGET,
            remote_server::setup::DEV_MUSL_TARGET,
        ));
    }
    // Prefer `cargo zigbuild` (zig bundles a complete C/C++ musl toolchain), otherwise musl-gcc.
    let backend = select_dev_build_backend().ok_or_else(|| {
        anyhow!(
            "no usable musl cross-compile backend found. Install cargo-zigbuild + zig \
             (`cargo install cargo-zigbuild`, and install `zig` via your package manager), \
             or install a complete musl C/C++ cross toolchain ({})",
            DEV_MUSL_LINKER_CANDIDATES.join(" / ")
        )
    })?;
    let binary = cross_compile_remote_server(&backend).await?;
    let raw = async_fs::read(&binary).await?;
    Ok(gzip_bytes(&raw)?)
}

/// Finds the helper for `platform` (the bundle, or a local build in a source build) and decides
/// the digest to require of it. Fails closed on a bundle that does not match the digest compiled
/// into this client, and on a release build with no digest at all; nothing is uploaded then.
async fn prepare_helper(platform: &RemotePlatform) -> Result<PreparedHelper, String> {
    let dev_source_build = remote_server::setup::is_dev_source_build();
    let gz = match load_bundled_helper(platform).await {
        Ok(Some(gz)) => gz,
        Ok(None) if dev_source_build => {
            log::info!("dev remote-server: no bundled helper, cross-compiling one");
            build_dev_helper(platform)
                .await
                .map_err(|e| helper_unavailable(platform, &format!("{e:#}")))?
        }
        Ok(None) => return Err(helper_unavailable(platform, "this app bundle has none")),
        Err(e) => return Err(helper_unavailable(platform, &format!("{e:#}"))),
    };
    let actual = sha256_of_gunzipped(&gz)
        .map_err(|e| helper_unavailable(platform, &format!("the bundled file is damaged: {e}")))?;
    let pinned =
        remote_server::setup::pinned_helper_sha256(platform.os.as_str(), platform.arch.as_str());
    let digest = decide_helper_digest(pinned, &actual, dev_source_build)?;
    Ok(PreparedHelper { gz, digest })
}

/// The install itself: prepare the helper, upload it in-band over the ControlMaster, fall back
/// to scp for a stream that could not deliver. Split out of `install_binary` so the caller can
/// clean up after ANY failure (#931).
async fn install_binary_impl(socket_path: &Path) -> Result<(), String> {
    log::info!(
        "Installing remote helper to {}",
        remote_server::setup::remote_server_binary()
    );
    let platform = detect_remote_platform(socket_path)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let helper = prepare_helper(&platform).await?;
    log::info!(
        "remote helper for {}-{}: {} bytes compressed, digest {}",
        platform.os.as_str(),
        platform.arch.as_str(),
        helper.gz.len(),
        if matches!(helper.digest, HelperDigest::Pinned(_)) {
            "pinned"
        } else {
            "computed (source build)"
        }
    );

    let script = remote_server::setup::install_script(None, helper.digest.script_override());
    match run_install_script(
        socket_path,
        &script,
        &helper.gz,
        remote_server::setup::INSTALL_TIMEOUT,
    )
    .await
    {
        Ok(()) => verify_installed_binary(socket_path)
            .await
            .map_err(|error| format!("{error:#}")),
        // All of the policy lives in `route_install_failure`, which is what the tests
        // exercise. This arm must stay a two-way dispatch with no judgement of its own.
        Err(error) => match route_install_failure(&error) {
            InstallFallbackRoute::Fail(message) => {
                log::error!("remote helper install failed, no fallback: {message}");
                Err(message)
            }
            InstallFallbackRoute::ScpFallback => {
                log::warn!("remote helper in-band upload failed, trying SCP fallback: {error}");
                match scp_install_fallback(socket_path, &helper).await {
                    Ok(()) => Ok(()),
                    Err(fallback_error) => {
                        Err(format!("{error}; SCP fallback failed: {fallback_error:#}"))
                    }
                }
            }
        },
    }
}

/// Best-effort: leave exactly one installed version on the host once the current binary has
/// been verified (Wave H step 1). Removes older versions, the legacy unversioned binary, and the
/// socket/pid files of other versions whose daemon is gone. Never fails the connect: a host
/// where this does not run just keeps a few extra files. See `setup::one_version_cleanup_command`.
async fn remove_other_installed_versions(socket_path: &Path) {
    let cmd = remote_server::setup::one_version_cleanup_command();
    match remote_server::ssh::run_ssh_command(
        socket_path,
        &cmd,
        remote_server::setup::CHECK_TIMEOUT,
    )
    .await
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => log::warn!(
            "remote-server one-version cleanup exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) => log::warn!("remote-server one-version cleanup failed to run: {error:#}"),
    }
}

/// Best-effort remote cleanup after a failed install: removes the staged upload, a partial
/// (non-executable) binary, and the install directory iff it is empty -- never a working
/// install. See `setup::install_failure_cleanup_command`.
async fn cleanup_failed_install(socket_path: &Path) {
    let cmd = remote_server::setup::install_failure_cleanup_command();
    match remote_server::ssh::run_ssh_command(
        socket_path,
        &cmd,
        remote_server::setup::CHECK_TIMEOUT,
    )
    .await
    {
        Ok(output) if output.status.success() => {
            log::info!("remote-server install failed; cleaned up leftovers on the remote");
        }
        Ok(output) => log::warn!(
            "remote-server install cleanup exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) => log::warn!("remote-server install cleanup failed to run: {error:#}"),
    }
}

/// The fallback for an in-band stream that could not deliver: the same compressed bytes go up
/// with scp as `.upload.gz` and the same verifying script unpacks that file.
async fn scp_install_fallback(socket_path: &Path, helper: &PreparedHelper) -> Result<()> {
    let remote_server_dir = remote_server::setup::remote_server_dir();
    let mkdir_cmd = format!("sh -c 'umask 077; mkdir -p {remote_server_dir}'");
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
    let local_path = tempdir.path().join("phosphor-remote.gz");
    async_fs::write(&local_path, &helper.gz).await?;

    let remote_staged_path = format!(
        "{remote_server_dir}/{}",
        remote_server::setup::STAGED_UPLOAD_NAME
    );
    remote_server::ssh::scp_upload(
        socket_path,
        &local_path,
        &remote_staged_path,
        remote_server::setup::SCP_INSTALL_TIMEOUT,
    )
    .await?;

    let script = remote_server::setup::install_script(
        Some(&remote_staged_path),
        helper.digest.script_override(),
    );
    run_install_script(
        socket_path,
        &script,
        &[],
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
                    Some(0) => {
                        // The right version is installed and runs: drop every other version.
                        remove_other_installed_versions(&socket_path).await;
                        Ok(true)
                    }
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
            // #931: "the directory exists" is not "an install exists" -- a failed install
            // used to leave it empty, making every later connect silently retry instead of
            // offering the choice. Require an executable binary inside it.
            let cmd = remote_server::setup::prior_install_probe_command();
            let output = remote_server::ssh::run_ssh_command(
                &socket_path,
                &cmd,
                remote_server::setup::CHECK_TIMEOUT,
            )
            .await?;
            // The probe exits 0 when a complete prior install exists, 1 when not.
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
            let result = install_binary_impl(&socket_path).await;
            if result.is_err() {
                cleanup_failed_install(&socket_path).await;
            } else {
                // `install_binary_impl` ends with `verify_installed_binary` on every path.
                remove_other_installed_versions(&socket_path).await;
            }
            result
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
                .env("SHELL", "/bin/sh")
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

    /// The ControlPath socket, owned or reused: a reused, user-owned master is never torn
    /// down (`control_path_for_teardown`), but its socket still tells a crashed proxy from
    /// the ssh session ending (#1250).
    fn master_socket_path(&self) -> Option<PathBuf> {
        Some(self.socket_path.clone())
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

    /// #985: the interactive ssh owns the connection the remote-server-proxy channel is
    /// multiplexed onto. A second, non-tty channel makes OpenSSH mark the whole connection
    /// "bulk" (IPQoS default `af21 cs1`), which Wi-Fi queues penalise by 40-200 ms per
    /// keystroke. Every bootstrap wrapper pins both classes before ControlMaster, but only
    /// when `ssh -G` reports exactly that default, so a user-configured IPQoS (or an older
    /// OpenSSH with a different default) is never overridden.
    #[test]
    fn interactive_ssh_wrappers_pin_ipqos_only_for_the_default() {
        for (name, script) in [
            (
                "bash",
                include_str!("../../assets/bundled/bootstrap/bash_body.sh"),
            ),
            (
                "zsh",
                include_str!("../../assets/bundled/bootstrap/zsh_body.sh"),
            ),
            (
                "fish",
                include_str!("../../assets/bundled/bootstrap/fish.sh"),
            ),
        ] {
            assert!(
                !script.contains("command ssh -o IPQoS"),
                "{name}: IPQoS must not be passed unconditionally"
            );
            let line = script
                .lines()
                .find(|l| l.contains("command ssh") && l.contains("warp_ipqos_args"))
                .unwrap_or_else(|| panic!("{name}: ssh invocation does not use warp_ipqos_args"));
            let ipqos = line.find("warp_ipqos_args").unwrap();
            let control = line.find("-o ControlMaster=").expect("ControlMaster");
            assert!(ipqos < control, "{name}: IPQoS must precede ControlMaster");
            // The bash wrapper reads the key from its single `ssh -G` capture through
            // `warp_ssh_g_value` (1594b17a4, #1172); zsh and fish still sed it out of that capture.
            assert!(
                script.contains("s/^ipqos //p")
                    || script.contains("warp_ssh_g_value warp_ssh_ipqos ipqos"),
                "{name}: must read the configured IPQoS with ssh -G"
            );
            assert!(
                script.contains("\"af21 cs1\""),
                "{name}: must compare against the OpenSSH default"
            );
            assert!(
                script.contains("IPQoS=af21 af21"),
                "{name}: must pin the interactive class"
            );
        }
    }

    #[test]
    fn remote_proxy_command_quotes_identity_key() {
        let transport = SshTransport::new(
            PathBuf::from("/tmp/control-master.sock"),
            static_auth_context(),
            true,
        );

        let command = transport.remote_proxy_command();

        // `<helper path> proxy <key>`: the exact versioned file, never a symlink another
        // client version may have repointed.
        assert!(
            command.starts_with(&remote_server::setup::remote_server_binary()),
            "{command}"
        );
        assert!(command.contains(" proxy "), "{command}");
        assert!(!command.contains("remote-server-proxy"), "{command}");
        assert!(command.contains("'user id/with spaces'"));
    }

    #[test]
    fn gzip_round_trip_hashes_the_decompressed_bytes() {
        let raw = b"#!/bin/sh\nexit 0\n".repeat(1000);
        let gz = gzip_bytes(&raw).unwrap();
        assert!(gz.len() < raw.len());
        let expected = {
            use sha2::{Digest as _, Sha256};
            hex::encode(Sha256::digest(&raw))
        };
        assert_eq!(sha256_of_gunzipped(&gz).unwrap(), expected);
    }

    #[test]
    fn a_non_gzip_bundle_is_an_error_not_a_hash() {
        assert!(sha256_of_gunzipped(b"definitely not gzip").is_err());
    }

    #[test]
    fn unavailable_helper_message_names_the_platform_and_the_fallback() {
        let platform = RemotePlatform {
            os: remote_server::setup::RemoteOs::Linux,
            arch: remote_server::setup::RemoteArch::Aarch64,
        };
        let message = helper_unavailable(&platform, "this app bundle has none");
        assert!(message.contains("linux-aarch64"), "{message}");
        assert!(message.contains("continuing without it"), "{message}");
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

    // #1250: a reused, user-owned master has no teardown path, but its socket is still the
    // one to stat when the proxy dies, so a crashed proxy is not mistaken for the user leaving.
    #[test]
    fn master_socket_path_is_reported_whether_or_not_the_master_is_owned() {
        let socket_path = PathBuf::from("/tmp/control-master.sock");
        for owned in [true, false] {
            let transport = SshTransport::new(socket_path.clone(), static_auth_context(), owned);
            assert_eq!(
                RemoteTransport::master_socket_path(&transport),
                Some(socket_path.clone()),
            );
        }
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

    // An integrity verdict is never shopped around: the SCP fallback would send the same
    // bytes to the same check, so retrying can only hide a failure. Every integrity failure the
    // script reports must stop there rather than fall through: exit 4 (client built with no
    // pinned digest), 5 (no SHA-256 tool on the host) and 6 (digest mismatch).
    #[test]
    fn integrity_failures_do_not_fall_through_to_the_scp_fallback() {
        for exit_code in [4, 5, 6] {
            assert_eq!(
                classify_install_failure(&script_failure(exit_code)),
                InstallFailureKind::IntegrityFailed,
            );
            assert!(
                !falls_back(&script_failure(exit_code)),
                "exit {exit_code} is an integrity failure and must fail closed, \
                 not fall through to the SCP fallback",
            );
        }
    }

    // The security property, stated on its own so it cannot be lost inside a
    // loop over three codes: exit 6 means the bytes ARRIVED and are not the
    // helper this client pinned. That is what tampering looks
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
        // Owner decision (Wave H, #1804): the fallback is no longer an unverified route, so
        // the headline says the install was not retried rather than that a fallback was skipped.
        assert!(
            message.contains("not retried"),
            "the refusal must say the install was not retried, got: {message}",
        );
    }

    // Exit 4 (`EXIT_NO_PINNED_DIGEST`) is a correct, unremarkable fail-closed
    // refusal on every local/dev build -- not evidence of tampering, and
    // conflating the two (TODO.md "Remote-session setup degrades silently",
    // item 4) reads as an alarming false positive on every build an agent or
    // contributor runs locally.
    #[test]
    fn missing_pinned_digest_is_explained_not_reported_as_tampering() {
        let route = route_install_failure(&script_failure(4));
        let InstallFallbackRoute::Fail(message) = route else {
            panic!("exit 4 (no pinned digest) must not route to the SCP fallback");
        };
        assert!(
            message.contains("no compiled-in checksum"),
            "the refusal must name the real cause (no pinned digest), got: {message}",
        );
        assert!(
            !message.contains("integrity check failed"),
            "a build with no pinned digest is not evidence of a failed integrity check, \
             got: {message}",
        );
    }

    // Exit 5 (`EXIT_NO_DIGEST_TOOL`) is an environment gap on the remote host,
    // not tampering either.
    #[test]
    fn missing_digest_tool_is_explained_not_reported_as_tampering() {
        let route = route_install_failure(&script_failure(5));
        let InstallFallbackRoute::Fail(message) = route else {
            panic!("exit 5 (no digest tool) must not route to the SCP fallback");
        };
        assert!(
            message.contains("no tool to compute a SHA-256 digest"),
            "the refusal must name the real cause (no digest tool on the remote host), \
             got: {message}",
        );
        assert!(
            !message.contains("integrity check failed"),
            "a missing digest tool is not evidence of a failed integrity check, got: {message}",
        );
    }

    // The three IntegrityFailed exit codes must each keep the script's own
    // stderr in the final message -- `route_install_failure`'s headline
    // change must not drop it.
    #[test]
    fn integrity_failure_messages_always_include_the_scripts_own_reason() {
        for exit_code in [4, 5, 6] {
            let error = InstallError::ScriptFailed {
                exit_code,
                stderr: "the script's own explanation".to_string(),
            };
            let InstallFallbackRoute::Fail(message) = route_install_failure(&error) else {
                panic!("exit {exit_code} is an integrity failure and must not fall back");
            };
            assert!(
                message.contains("the script's own explanation"),
                "exit {exit_code}'s refusal must keep the script's own stderr, got: {message}",
            );
        }
    }

    // Unsupported arch/OS: no asset exists, so retrying cannot help either.
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

    // An upload that decoded partially and then failed, or a stream the host's
    // decompressor rejects mid-way. The bytes are exactly what this client sent, so the
    // fallback would send them again and fail identically.
    #[test]
    fn unusable_upload_does_not_fall_through() {
        assert_eq!(
            classify_install_failure(&script_failure(8)),
            InstallFailureKind::Fatal,
        );
        assert!(!falls_back(&script_failure(8)));
    }

    // Exit 3: the host has neither gunzip nor gzip. The scp route unpacks with the same tools, so
    // retrying cannot help.
    #[test]
    fn missing_decompressor_does_not_fall_through() {
        assert_eq!(
            classify_install_failure(&script_failure(3)),
            InstallFailureKind::Fatal,
        );
        assert!(!falls_back(&script_failure(3)));
    }

    // Exit 7 is what the fallback exists for: the in-band stream delivered nothing (an empty or
    // immediately broken stream, e.g. a restricted shell that eats stdin). Nothing was verified
    // and nothing violated; sending the file with scp instead is the designed recovery.
    #[test]
    fn upload_that_delivered_nothing_still_uses_scp_fallback() {
        assert_eq!(
            classify_install_failure(&script_failure(7)),
            InstallFailureKind::TransportFailed,
        );
        assert!(
            falls_back(&script_failure(7)),
            "exit 7 (no data arrived) is a transport failure, not an integrity failure: \
             nothing arrived, so nothing failed verification",
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
        let script = remote_server::setup::install_script(None, None);

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
            "EXIT_UPLOAD_FAILED" => InstallFailureKind::TransportFailed,
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
