mod glibc;

pub use glibc::{GlibcVersion, RemoteLibc};

use std::time::Duration;

use anyhow::{anyhow, Result};
use warp_core::channel::{Channel, ChannelState};

/// State machine for the remote server install → launch → initialize flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteServerSetupState {
    /// Checking if the binary exists on remote.
    Checking,
    /// Downloading and installing the binary for the first time on this host.
    Installing { progress_percent: Option<u8> },
    /// Replacing an existing install with a differently-versioned binary.
    /// Rendered as "Updating..." in the UI so the user understands this
    /// isn't a fresh install.
    Updating,
    /// Binary is launched, waiting for InitializeResponse.
    Initializing,
    /// Handshake complete. Ready.
    Ready,
    /// Something failed. Fall back to ControlMaster.
    Failed { error: String },
    /// Preinstall check classified the host as incompatible with the
    /// prebuilt remote-server binary. The controller treats this as a
    /// clean fall-back to the legacy ControlMaster-backed SSH flow,
    /// distinct from `Failed` (which is rendered as a real error).
    Unsupported { reason: UnsupportedReason },
}

impl RemoteServerSetupState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }

    pub fn is_terminal(&self) -> bool {
        self.is_ready() || self.is_failed() || self.is_unsupported()
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Installing { .. } | Self::Updating | Self::Initializing
        )
    }

    pub fn is_connecting(&self) -> bool {
        matches!(
            self,
            Self::Installing { .. } | Self::Updating | Self::Initializing
        )
    }
}

/// Outcome of [`crate::transport::RemoteTransport::run_preinstall_check`].
///
/// The script runs over the existing SSH socket before any install UI
/// surfaces and reports whether the host can run the prebuilt
/// remote-server binary. The Rust side is intentionally a thin parser
/// over the script's structured stdout (see `preinstall_check.sh`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreinstallCheckResult {
    pub status: PreinstallStatus,
    pub libc: RemoteLibc,
    /// Verbatim, trimmed script stdout. Forwarded to telemetry for
    /// diagnosing `Unknown` outcomes on exotic distros.
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreinstallStatus {
    Supported,
    Unsupported {
        reason: UnsupportedReason,
    },
    /// Probe ran but couldn't classify the host. Treated as supported
    /// (fail open) by [`PreinstallCheckResult::is_supported`] so we keep
    /// today's install-and-try behavior on hosts where the probe is
    /// unreliable.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    GlibcTooOld {
        detected: GlibcVersion,
        required: GlibcVersion,
    },
    NonGlibc {
        name: String,
    },
}

impl PreinstallCheckResult {
    /// Whether the host is supported. Both `Supported` and `Unknown`
    /// return true — only positive detection of an incompatible libc
    /// triggers the silent fall-back.
    pub fn is_supported(&self) -> bool {
        match self.status {
            PreinstallStatus::Supported | PreinstallStatus::Unknown => true,
            PreinstallStatus::Unsupported { .. } => false,
        }
    }

    /// Parses the structured `key=value` stdout emitted by
    /// `preinstall_check.sh`. Tolerates unknown keys and lines without
    /// `=` (forward-compatibility): future versions of the script can
    /// add new keys without coordinating a client release.
    pub fn parse(stdout: &str) -> Self {
        let mut status_str: Option<&str> = None;
        let mut reason_str: Option<&str> = None;
        let mut libc_family: Option<&str> = None;
        let mut libc_version: Option<&str> = None;
        let mut required_glibc: Option<&str> = None;

        for line in stdout.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "status" => status_str = Some(value.trim()),
                "reason" => reason_str = Some(value.trim()),
                "libc_family" => libc_family = Some(value.trim()),
                "libc_version" => libc_version = Some(value.trim()),
                "required_glibc" => required_glibc = Some(value.trim()),
                _ => {} // ignore unknown keys
            }
        }

        let libc = glibc::parse_libc(libc_family, libc_version);
        let status = parse_status(status_str, reason_str, &libc, required_glibc);

        Self {
            status,
            libc,
            raw: stdout.trim().to_string(),
        }
    }
}

fn parse_status(
    status: Option<&str>,
    reason: Option<&str>,
    _libc: &RemoteLibc,
    _required_glibc: Option<&str>,
) -> PreinstallStatus {
    // remote-server is now a static musl binary (see the comment at the top of
    // `preinstall_check.sh`) that doesn't link the host's dynamic libc.
    // Therefore `glibc_too_old` / `non_glibc` are no longer reasons for
    // "unsupported" — any glibc version and musl/uclibc hosts can run this
    // binary. The new script no longer emits these two reasons; but an old
    // remote side may still be caching the old script, so these libc-gate
    // reasons are here treated as `Supported` rather than `Unsupported`,
    // keeping old and new script judgments consistent.
    match status {
        Some("supported") => PreinstallStatus::Supported,
        Some("unsupported") => match reason {
            // Libc-gate reason left over from the old script: no longer valid under a static binary, treated as supported.
            Some("glibc_too_old") | Some("non_glibc") => PreinstallStatus::Supported,
            // Other unrecognized unsupported reasons: fail open conservatively.
            _ => PreinstallStatus::Unknown,
        },
        // status=unknown, missing, or anything else → fail open.
        _ => PreinstallStatus::Unknown,
    }
}

/// The bundled preinstall check script. Loaded as a string so the SSH
/// transport can pipe it through the existing ControlMaster socket via
/// [`crate::ssh::run_ssh_script`].
///
/// The script is intentionally self-contained — the supported-glibc
/// floor is hardcoded inside the script (see `preinstall_check.sh`)
/// rather than templated from Rust.
pub const PREINSTALL_CHECK_SCRIPT: &str = include_str!("preinstall_check.sh");

/// Detected remote platform from `uname -sm` output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePlatform {
    pub os: RemoteOs,
    pub arch: RemoteArch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteOs {
    Linux,
    MacOs,
}

impl RemoteOs {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::MacOs => "macos",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteArch {
    X86_64,
    Aarch64,
}

impl RemoteArch {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }
}

/// Parse `uname -sm` output into a `RemotePlatform`.
///
/// The expected format is `<os> <arch>`, e.g. `Linux x86_64` or `Darwin arm64`.
/// Takes the last line to skip any shell initialization output.
pub fn parse_uname_output(output: &str) -> Result<RemotePlatform> {
    let line = output
        .lines()
        .last()
        .ok_or_else(|| anyhow!("empty uname output"))?
        .trim();

    let mut parts = line.split_whitespace();
    let os_str = parts
        .next()
        .ok_or_else(|| anyhow!("missing OS in uname output: {line}"))?;
    let arch_str = parts
        .next()
        .ok_or_else(|| anyhow!("missing arch in uname output: {line}"))?;

    let os = match os_str {
        "Linux" => RemoteOs::Linux,
        "Darwin" => RemoteOs::MacOs,
        other => return Err(anyhow!("unsupported OS: {other}")),
    };

    let arch = match arch_str {
        // "amd64" is upstream Warp's alias for x86_64 (some `uname -m` builds and
        // non-GNU userlands report it); restored per oracle parity, see
        // crates/remote_server/src/setup_tests.rs::parse_uname_linux_amd64.
        "x86_64" | "amd64" => RemoteArch::X86_64,
        "aarch64" | "arm64" | "armv8l" => RemoteArch::Aarch64,
        other => return Err(anyhow!("unsupported arch: {other}")),
    };

    Ok(RemotePlatform { os, arch })
}

/// Returns the remote binary install directory, isolated per channel.
///
/// - stable:      `~/.warp/remote-server`
/// - preview:     `~/.warp-preview/remote-server`
/// - dev:         `~/.warp-dev/remote-server`
/// - local:       `~/.warp-local/remote-server`
/// - integration: `~/.warp-dev/remote-server`
/// - warp-oss:    `~/.zap/remote-server`
pub fn remote_server_dir() -> String {
    let warp_dir = match ChannelState::channel() {
        Channel::Stable => ".warp",
        Channel::Preview => ".warp-preview",
        Channel::Dev | Channel::Integration => ".warp-dev",
        Channel::Local => ".warp-local",
        Channel::Oss => ".zap",
    };
    format!("~/{warp_dir}/remote-server")
}

/// Returns a short, deterministic directory name for a remote-server
/// identity key, used for the daemon socket and PID file paths.
///
/// Hashes the key to 8 hex chars so the socket path stays within the
/// `sun_path` limit across all channels.
///
/// Regression fix (ported alongside `identity_dir_name_is_short_hash` /
/// `socket_path_fits_within_sun_path_worst_case` in setup_tests.rs): this
/// used to percent-encode the raw identity key instead of hashing it. For a
/// UUID-shaped key (the common case — see `app/src/remote_server/auth_context.rs`)
/// percent-encoding is a no-op, so the daemon dir embedded the full ~36-char
/// key with no shortening at all, defeating the `sun_path` safety margin this
/// function exists for. See crates/remote_server/src/setup_tests.rs.
pub fn remote_server_identity_dir_name(identity_key: &str) -> String {
    use std::hash::{Hash, Hasher};

    if identity_key.is_empty() {
        return "empty".to_string();
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    identity_key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())[..8].to_string()
}

/// Percent-encodes an identity key for use in filesystem paths.
///
/// Keeps ASCII alphanumeric characters plus `-` and `_`; percent-encodes all
/// other bytes. Used by [`remote_server_daemon_data_dir`] for persistent
/// data that must not collide across identities (unlike
/// [`remote_server_identity_dir_name`], collision risk here is not
/// acceptable, so the full key is kept rather than hashed).
fn percent_encode_identity_key(identity_key: &str) -> String {
    if identity_key.is_empty() {
        return "empty".to_string();
    }

    let mut encoded = String::with_capacity(identity_key.len());
    for byte in identity_key.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// Returns the identity-isolated remote directory used for the daemon socket
/// and PID file. Uses the hashed identity dir name so the full socket path
/// fits within `sun_path`.
pub fn remote_server_daemon_dir(identity_key: &str) -> String {
    format!(
        "{}/{}",
        remote_server_dir(),
        remote_server_identity_dir_name(identity_key)
    )
}

/// Returns the identity-scoped remote directory used for daemon-owned
/// per-user data files (e.g. SQLite databases).
///
/// Uses the full percent-encoded identity key (not the hash) so persistent
/// data is never shared between distinct identities due to a hash collision.
/// The `sun_path` limit does not apply here: this path is only used for
/// regular file I/O, not Unix sockets.
pub fn remote_server_daemon_data_dir(identity_key: &str) -> String {
    format!(
        "{}/{}/data",
        remote_server_dir(),
        percent_encode_identity_key(identity_key)
    )
}

/// Returns a short, deterministic 8-hex-char hash of the app version string.
///
/// Used to version-discriminate daemon socket and PID files without
/// embedding the full version string in the filename, which would push the
/// Unix domain socket path over the `sun_path` limit (107 bytes on Linux,
/// 103 on macOS) for users with moderately long identity keys or home
/// directory paths.
pub fn version_hash() -> Option<String> {
    use std::hash::{Hash, Hasher};

    let version = ChannelState::app_version()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    version.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish())[..8].to_string())
}

/// Returns the daemon socket filename, versioned with a short hash when a
/// release tag is baked in.
///
/// - With `GIT_RELEASE_TAG`:    `server-{hash8}.sock` (e.g. `server-a1b2c3d4.sock`)
/// - Without (plain cargo run): `server.sock`
pub fn daemon_socket_name() -> String {
    match version_hash() {
        Some(hash) => format!("server-{hash}.sock"),
        None => "server.sock".to_string(),
    }
}

/// Returns the daemon PID filename, versioned with a short hash when a
/// release tag is baked in.
///
/// - With `GIT_RELEASE_TAG`:    `server-{hash8}.pid`
/// - Without (plain cargo run): `server.pid`
pub fn daemon_pid_name() -> String {
    match version_hash() {
        Some(hash) => format!("server-{hash}.pid"),
        None => "server.pid".to_string(),
    }
}

/// Returns the remote remote-server binary's file name.
pub fn binary_name() -> &'static str {
    ChannelState::channel().cli_command_name()
}

/// Returns the full remote binary path corresponding to the current channel and client version.
///
/// Local builds keep the unversioned path so `script/deploy_remote_server` can
/// overwrite the same dev slot. Zap release builds with a `GIT_RELEASE_TAG`
/// use a version suffix, so new versions naturally trigger reinstalling;
/// local source builds have no release tag and still use the unsuffixed path.
pub fn remote_server_binary() -> String {
    let dir = remote_server_dir();
    let name = binary_name();
    match ChannelState::channel() {
        Channel::Local => format!("{dir}/{name}"),
        Channel::Oss if ChannelState::app_version().is_none() => format!("{dir}/{name}"),
        Channel::Oss => format!("{dir}/{name}-{}", pinned_version()),
        Channel::Stable | Channel::Preview | Channel::Dev | Channel::Integration => {
            format!("{dir}/{name}-{}", pinned_version())
        }
    }
}

/// Returns the shell command to check the remote remote-server binary exists and is executable.
///
/// Consistent with upstream, this actually runs `--version` instead of just
/// `test -x`; this way, a corrupted or argument-parsing-broken binary can be identified early.
pub fn binary_check_command() -> String {
    format!("{} --version", remote_server_binary())
}

/// Returns the shell command to remove the current remote-server binary.
///
/// The global bundled resources directory (see
/// [`remote_server_bundled_resources_dir`]) is deliberately left in place:
/// the next install overwrites it, and an older daemon that is still
/// running parsed its skills from it at startup.
pub fn remote_server_removal_command() -> String {
    format!("rm -f {}", remote_server_binary())
}

/// Returns the version number used for the versioned install path. Prefers
/// the compile-time-injected `GIT_RELEASE_TAG`; falls back to
/// `CARGO_PKG_VERSION` when there's no release tag, keeping channels that
/// need a versioned path deterministic, and failing clearly when the
/// corresponding release asset is missing rather than mistakenly using the unversioned path.
fn pinned_version() -> &'static str {
    ChannelState::app_version().unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// Name of the global, version-independent resources directory inside
/// [`remote_server_dir`], meant to be populated by the install script from
/// the release artifact's `resources/` tree (bundled skills, settings
/// schema).
///
/// `install_remote_server.sh` populates this from the artifact's `resources/`
/// tree, substituting the name via the `{bundled_resources_dir_name}`
/// placeholder in [`install_script`]; the release pipeline already tars that
/// tree into the CLI asset (`phosphor_release.yml`). The daemon-side consumer
/// is `daemon_bundled_resources_dir` in `app/src/remote_server/server_model.rs`.
///
/// A missing `resources/` tree is not an install failure: dev-mode installs
/// cross-compile a bare binary (see [`DEV_MUSL_TARGET`]) and older release
/// artifacts predate the tree, so the daemon still has to handle the "no
/// bundled resources" branch.
pub const BUNDLED_RESOURCES_DIR_NAME: &str = "bundled_resources";

/// Returns the global, version-independent directory where the install
/// script would place the artifact's `resources/` tree. Shell-form path
/// (`~/...`); the daemon expands it against its own home directory.
///
/// Deliberately not version-scoped: the last install wins, and slight
/// version skew between the resources and a running daemon is accepted
/// (the daemon parses its skills once at startup).
pub fn remote_server_bundled_resources_dir() -> String {
    format!("{}/{}", remote_server_dir(), BUNDLED_RESOURCES_DIR_NAME)
}

/// The install script template lives in a separate `.sh` file for easier maintenance.
/// Placeholders like `{download_base_url}` are replaced by [`install_script`].
const INSTALL_SCRIPT_TEMPLATE: &str = include_str!("install_remote_server.sh");

/// Returns the install script. When `staging_tarball_path` is non-empty, the
/// script skips the remote download and instead unpacks the tarball the client pre-uploaded via SCP.
pub fn install_script(staging_tarball_path: Option<&str>) -> String {
    let version_suffix = version_suffix();
    INSTALL_SCRIPT_TEMPLATE
        .replace("{download_base_url}", &download_url())
        .replace("{install_dir}", &remote_server_dir())
        .replace("{binary_name}", binary_name())
        .replace("{release_asset_prefix}", RELEASE_ASSET_PREFIX)
        .replace("{version_suffix}", &version_suffix)
        .replace("{staging_tarball_path}", staging_tarball_path.unwrap_or(""))
        .replace("{bundled_resources_dir_name}", BUNDLED_RESOURCES_DIR_NAME)
        // All published platforms are substituted, not just the one we think the host is:
        // the script re-derives `uname` itself and picks the matching digest, so a host that
        // turns out to be a different platform than the client guessed still verifies rather
        // than silently falling through to an empty expectation.
        .replace("{sha256_linux_x86_64}", expected_sha256("linux", "x86_64"))
        .replace("{sha256_linux_aarch64}", expected_sha256("linux", "aarch64"))
        .replace("{sha256_macos_x86_64}", expected_sha256("macos", "x86_64"))
        .replace("{sha256_macos_aarch64}", expected_sha256("macos", "aarch64"))
}

/// SHA-256 of the published CLI tarball for one remote platform, embedded into this client
/// at **build time** by the release workflow.
///
/// This is the integrity root for remote-server installs, and the reason it is a compile-time
/// constant rather than something fetched at install time is the whole point:
///
/// The install script runs on the remote host and pulls the tarball from GitHub over plain
/// HTTPS. Before this existed, whatever bytes came back were `chmod +x`'d and executed --
/// GitHub's TLS and release storage were the entire trust model, and a tampered or
/// mis-published release would install and run on every host the user SSHes into, silently.
///
/// A checksum published *alongside* the release would not fix that: anyone able to replace the
/// tarball can replace a checksum file next to it. A digest compiled into the client instead
/// reaches the remote host down the user's already-authenticated **SSH channel**, as part of
/// the script text itself. That takes GitHub out of the integrity path entirely -- the release
/// can only install if it matches what this client was built expecting.
///
/// Empty when the corresponding `PHOSPHOR_CLI_SHA256_*` env var was not set at compile time
/// (any local build). The script treats empty as **fail-closed on the download path** and
/// refuses to install; it does not warn and continue. That is deliberate: a silent skip would
/// make the protection vanish exactly when a build is misconfigured. It does not affect local
/// development, because a DEBUG build with no release tag never downloads at all -- it
/// cross-compiles and uploads over SCP (see [`is_dev_mode_install`] and `DEV_MUSL_TARGET`),
/// and that path is trusted because it likewise arrived over SSH.
fn expected_sha256(os: &str, arch: &str) -> &'static str {
    let digest = match (os, arch) {
        ("linux", "x86_64") => option_env!("PHOSPHOR_CLI_SHA256_LINUX_X86_64"),
        ("linux", "aarch64") => option_env!("PHOSPHOR_CLI_SHA256_LINUX_AARCH64"),
        ("macos", "x86_64") => option_env!("PHOSPHOR_CLI_SHA256_MACOS_X86_64"),
        ("macos", "aarch64") => option_env!("PHOSPHOR_CLI_SHA256_MACOS_AARCH64"),
        _ => None,
    };
    digest.unwrap_or("").trim()
}

/// Repository the remote-server CLI tarball is downloaded from.
///
/// This fork's, not upstream Zap's. It pointed at `zerx-lab/warp` until
/// 2026-08-11, inherited unchanged through the rebrand, which meant every SSH
/// remote-server install fetched (or rather, failed to fetch) a *different
/// project's* binary. Combined with the asset prefix below, the URL 404'd for
/// every user of this fork, so remote-server setup over SSH could not succeed
/// at all.
const RELEASE_REPO: &str = "jwp2987/phosphor";

/// Filename prefix of the published CLI tarball, e.g.
/// `phosphor-cli-linux-x86_64.tar.gz`.
///
/// Deliberately NOT `binary_name()`: that returns the channel's *command* name
/// (`zap-oss` on the OSS channel), which is not what the release workflow names
/// its assets. The two drifted apart at the rebrand and nothing caught it
/// because no test asserted the URL against a real release.
const RELEASE_ASSET_PREFIX: &str = "phosphor-cli";

/// Builds the download base URL for the CLI release asset.
///
/// Note `latest/download` only resolves to a **non-prerelease**. Every release
/// published so far is marked prerelease, so the `None` arm below is currently
/// a 404 in practice; a build carrying `GIT_RELEASE_TAG` takes the `Some` arm
/// and works. Recorded rather than papered over — the fix is to publish a
/// non-prerelease, not to change this code.
fn download_url() -> String {
    let release_path = match ChannelState::app_version() {
        Some(tag) => format!("download/{tag}"),
        None => "latest/download".to_string(),
    };
    format!("https://github.com/{RELEASE_REPO}/releases/{release_path}")
}

fn version_suffix() -> String {
    match ChannelState::channel() {
        Channel::Local => String::new(),
        Channel::Oss if ChannelState::app_version().is_none() => String::new(),
        Channel::Oss | Channel::Stable | Channel::Preview | Channel::Dev | Channel::Integration => {
            format!("-{}", pinned_version())
        }
    }
}

/// Returns the CLI tarball URL for the given remote platform.
///
/// Must use [`RELEASE_ASSET_PREFIX`], not a literal — this is the URL the SSH
/// transport actually fetches (`ssh_transport::…`), so a prefix here that
/// disagrees with the one the install script asks for is a silent 404 on the
/// one path a user goes through. The prefix was threaded through the install
/// template but missed here, leaving the 2026-08-11 fix half-applied.
pub fn download_tarball_url(platform: &RemotePlatform) -> String {
    format!(
        "{}/{RELEASE_ASSET_PREFIX}-{}-{}.tar.gz",
        download_url(),
        platform.os.as_str(),
        platform.arch.as_str(),
    )
}

/// Zap fork: in dev mode (DEBUG source build, no release tag), the SSH
/// transport no longer downloads a stale release from GitHub — it instead
/// cross-compiles the current `warp` binary locally and uploads it. The
/// constants below centrally describe this cross-compiled artifact, kept
/// consistent with `script/deploy_remote_server` (same profile / same
/// features / same target) to avoid the two diverging.
///
/// Cross-compile target triple.
pub const DEV_MUSL_TARGET: &str = "x86_64-unknown-linux-musl";

/// Cargo profile used for cross-compiling. Corresponds to `[profile.dev-remote]`
/// in `Cargo.toml`, which inherits `dev` and strips symbols to reduce size and speed up upload.
pub const DEV_REMOTE_PROFILE: &str = "dev-remote";

/// Features enabled for cross-compiling, consistent with `script/deploy_remote_server`.
pub const DEV_REMOTE_FEATURES: &str = "release_bundle,crash_reporting,standalone,agent_mode_debug";

/// Determines whether we're currently on the "dev-mode remote-server install" path.
///
/// Default condition: a DEBUG build (`debug_assertions`) with no injected
/// `GIT_RELEASE_TAG` (`app_version().is_none()`, i.e. a local source build,
/// not a release). This matches the same standard used for "no release tag"
/// in `remote_server_binary()` / `download_url()`. Always `false` for release
/// builds, with no behavior change.
///
/// Explicit override: setting `WARP_REMOTE_SERVER_FROM_LOCAL=1` forces the
/// local cross-compile path (`0`/unset is treated as off). Used to temporarily
/// test a local remote-server against a release build.
pub fn is_dev_source_build() -> bool {
    if let Some(raw) = std::env::var_os("WARP_REMOTE_SERVER_FROM_LOCAL") {
        let lossy = raw.to_string_lossy();
        let trimmed = lossy.trim();
        let disabled =
            trimmed.is_empty() || trimmed == "0" || trimmed.eq_ignore_ascii_case("false");
        if !disabled {
            return true;
        }
    }
    cfg!(debug_assertions) && ChannelState::app_version().is_none()
}

/// Timeout for checking whether the binary exists.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for the regular remote install script.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(60);

/// The SCP fallback includes a local download, upload, and remote extraction, so give it a more generous timeout.
pub const SCP_INSTALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Dev-mode cross-compiling may need to build the entire crate graph from scratch, so give it a very generous timeout.
pub const DEV_CROSS_COMPILE_TIMEOUT: Duration = Duration::from_secs(900);

/// Timeout for uploading the locally cross-compiled artifact in dev mode. Dev
/// binaries (unoptimized + debug info) can be hundreds of MB; even with scp's
/// `-C` compression, uploading over the public internet can take minutes, so
/// this is given a generous ceiling well above `SCP_INSTALL_TIMEOUT`.
pub const DEV_UPLOAD_TIMEOUT: Duration = Duration::from_secs(1800);

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
