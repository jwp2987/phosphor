mod glibc;

pub use glibc::{GlibcVersion, RemoteLibc};

use std::time::Duration;

use anyhow::{Result, anyhow};
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
    /// This app has no helper to upload for the host (no bundled asset for its platform, or an
    /// unreadable one). Not an error: the session stays on the no-helper path, quietly (#1804).
    HelperUnavailable {
        detail: String,
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
/// - phosphor-oss: `~/.phosphor/remote-server`
pub fn remote_server_dir() -> String {
    // This path lives on the *remote* host, not the local machine. Renaming it
    // orphans any already-installed daemon and its `bundled_resources` tree on
    // that host, with nothing that ever cleans the old directory up. Renamed
    // anyway because there are currently no remote installs to strand.
    let warp_dir = match ChannelState::channel() {
        Channel::Stable => ".warp",
        Channel::Preview => ".warp-preview",
        Channel::Dev | Channel::Integration => ".warp-dev",
        Channel::Local => ".warp-local",
        Channel::Oss => ".phosphor",
    };
    format!("~/{warp_dir}/remote-server")
}

/// Returns a short, deterministic directory name for a remote-server
/// identity key, used for the daemon socket and PID file paths.
///
/// Hashes the key to 8 hex chars via [`stable_short_hash`] so the socket path
/// stays within the `sun_path` limit across all channels. See that function
/// for why the hash must not be `DefaultHasher`.
///
/// Regression fix (ported alongside `identity_dir_name_is_short_hash` /
/// `socket_path_fits_within_sun_path_worst_case` in setup_tests.rs): this
/// used to percent-encode the raw identity key instead of hashing it. For a
/// UUID-shaped key (the common case — see `app/src/remote_server/auth_context.rs`)
/// percent-encoding is a no-op, so the daemon dir embedded the full ~36-char
/// key with no shortening at all, defeating the `sun_path` safety margin this
/// function exists for. See crates/remote_server/src/setup_tests.rs.
pub fn remote_server_identity_dir_name(identity_key: &str) -> String {
    if identity_key.is_empty() {
        return "empty".to_string();
    }

    stable_short_hash(identity_key)
}

use crate::naming::stable_short_hash;

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
    let version = ChannelState::app_version()?;
    Some(stable_short_hash(version))
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

/// Base name of the bundled helper binary: `phosphor-remote`. Installed on the host as
/// `phosphor-remote-<version>` ([`remote_server_binary`]).
pub const HELPER_BASE_NAME: &str = "phosphor-remote";

/// Returns the helper's base file name. (Was the channel's CLI command name while the full app
/// binary was downloaded from GitLab; Wave H replaced that with the bundled helper, #1804.)
pub fn binary_name() -> &'static str {
    HELPER_BASE_NAME
}

/// The previous remote server's file name, the full app binary installed under the channel's
/// CLI command name. Only the one-version cleanup still names it, to delete leftovers.
pub fn legacy_binary_name() -> &'static str {
    ChannelState::channel().cli_command_name()
}

/// Returns the full remote helper path for this client version:
/// `<remote_server_dir>/phosphor-remote-<version>`.
///
/// The version is [`crate::naming::helper_version`], not the channel's app version: client and
/// helper are built from this crate in one pipeline, so it names exactly the helper this client
/// bundles. Two clients of different versions on one host each ensure their own file; the
/// one-version cleanup keeps whichever installed last.
pub fn remote_server_binary() -> String {
    format!("{}/{}", remote_server_dir(), helper_file_name())
}

/// The helper file name for this client version: `phosphor-remote-<version>`.
pub fn helper_file_name() -> String {
    format!("{HELPER_BASE_NAME}-{}", crate::naming::helper_version())
}

/// Name of the symlink the install points at the newest verified helper (informational; the
/// client always launches the exact versioned path).
pub const HELPER_CURRENT_LINK: &str = "current";

/// File name the SCP fallback stages the compressed helper under, inside the install directory.
pub const STAGED_UPLOAD_NAME: &str = ".upload.gz";

/// Returns the shell command to check the remote remote-server binary exists and is executable.
///
/// Consistent with upstream, this actually runs `--version` instead of just
/// `test -x`; this way, a corrupted or argument-parsing-broken binary can be identified early.
pub fn binary_check_command() -> String {
    format!("{} --version", remote_server_binary())
}

/// Shell command (run on the remote) that exits 0 only when a COMPLETE prior install exists:
/// a regular, executable file named `<binary>` or `<binary>-<version>` inside `dir`.
///
/// The bare existence of `dir` is not evidence of an install: a failed install used to leave
/// an empty directory behind, after which every connect skipped the install/continue choice
/// and silently retried (#931). Wrapped in `sh -c` so it does not depend on the remote login
/// shell (fish/csh reject `for ... do ... done`).
pub fn prior_install_probe_command_for(dir: &str, binary: &str) -> String {
    format!(
        "sh -c 'for f in {dir}/{binary} {dir}/{binary}-*; do if [ -f \"$f\" ] && [ -x \"$f\" ]; then exit 0; fi; done; exit 1'"
    )
}

/// [`prior_install_probe_command_for`] for this channel's install directory and binary name.
pub fn prior_install_probe_command() -> String {
    prior_install_probe_command_for(&remote_server_dir(), binary_name())
}

/// Shell command (run on the remote after a FAILED install) that removes what a failed install
/// can leave behind: the staged SCP upload, the in-band upload's temporary file, a
/// non-executable (so necessarily partial) binary, and the install directory itself if -- and
/// only if -- it is empty. It never touches an executable binary, the resources tree, or any
/// other file, so it is safe after a failed update over a working install too. Always exits 0.
pub fn install_failure_cleanup_command_for(dir: &str, binary_path: &str) -> String {
    format!(
        "sh -c 'rm -f {dir}/{STAGED_UPLOAD_NAME} {dir}/.up.*; if [ -e {binary_path} ] && [ ! -x {binary_path} ]; then rm -f {binary_path}; fi; rmdir {dir} 2>/dev/null; true'"
    )
}

/// [`install_failure_cleanup_command_for`] for this channel.
pub fn install_failure_cleanup_command() -> String {
    install_failure_cleanup_command_for(&remote_server_dir(), &remote_server_binary())
}

/// Shell fragment shared by the install script and [`one_version_cleanup_command_for`]; see the
/// header of `one_version_cleanup.sh` for its inputs and what it removes.
const ONE_VERSION_CLEANUP_FRAGMENT: &str = include_str!("one_version_cleanup.sh");

/// Base names whose files [`one_version_cleanup_command`] manages in the install directory: the
/// helper, and the channel's old full-app binary, so a host that had the previous remote server
/// loses its roughly 270 MB of leftovers once the helper is verified. A base name's unversioned
/// file (`<base>` itself) is removed too unless it is the one kept.
fn one_version_binaries() -> String {
    format!("{} {}", binary_name(), legacy_binary_name())
}

/// Shell command (run on the remote after the new binary has been verified) that leaves exactly
/// ONE installed version: it removes every other `<binary>` / `<binary>-*` file in `dir` (older
/// versions and the legacy unversioned binary) and the socket and pid files of other versions
/// whose daemon is no longer running. `keep` is the file name to keep; `hash` is the kept
/// version's [`version_hash`] (empty for an unversioned dev build, which sweeps no daemon
/// files). A running old daemon is left alone and exits on its idle timeout. Always exits 0.
///
/// Wrapped in `sh -c` so it does not depend on the remote login shell; `dir` is left unquoted
/// so a leading `~` expands (same convention as [`prior_install_probe_command_for`]).
pub fn one_version_cleanup_command_for(
    dir: &str,
    binaries: &str,
    keep: &str,
    hash: &str,
) -> String {
    format!(
        "sh -c 'ovc_dir={dir}; ovc_binaries=\"{binaries}\"; ovc_keep=\"{keep}\"; ovc_hash=\"{hash}\"; {ONE_VERSION_CLEANUP_FRAGMENT}'"
    )
}

/// [`one_version_cleanup_command_for`] for this channel and client version.
pub fn one_version_cleanup_command() -> String {
    one_version_cleanup_command_for(
        &remote_server_dir(),
        &one_version_binaries(),
        &helper_file_name(),
        &crate::naming::helper_version_hash(),
    )
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

/// The version string baked into the helper's install path: [`crate::naming::helper_version`].
fn pinned_version() -> &'static str {
    crate::naming::helper_version()
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
/// Placeholders like `{install_dir}` are replaced by [`install_script`].
const INSTALL_SCRIPT_TEMPLATE: &str = include_str!("install_remote_server.sh");

/// Returns the helper install script, to be piped into `bash -s` with the gzip-compressed helper
/// appended after it on the same stdin (see [`crate::ssh::run_ssh_script_with_payload`]); the
/// script ends in a call to `main`, so bash has read exactly the script when the helper bytes
/// begin. With `staging_gz_path` set (the SCP fallback uploaded the file first) it reads that
/// file instead and stdin is unused.
///
/// `sha256_override` is the digest of the decompressed helper to require on every platform
/// placeholder; `None` uses the digests compiled into this client ([`pinned_helper_sha256`]).
/// The override exists for source builds that bundle a helper they built themselves and so have
/// no pinned digest (see [`is_dev_source_build`]); callers must not pass it on release builds.
pub fn install_script(staging_gz_path: Option<&str>, sha256_override: Option<&str>) -> String {
    let digest = |os: &str, arch: &str| -> String {
        sha256_override
            .map(str::to_string)
            .unwrap_or_else(|| pinned_helper_sha256(os, arch).unwrap_or("").to_string())
    };
    INSTALL_SCRIPT_TEMPLATE
        .replace("{install_dir}", &remote_server_dir())
        .replace("{helper_file_name}", &helper_file_name())
        .replace("{helper_current_link}", HELPER_CURRENT_LINK)
        .replace("{one_version_binaries}", &one_version_binaries())
        .replace(
            "{daemon_version_hash}",
            &crate::naming::helper_version_hash(),
        )
        .replace("{staging_gz_path}", staging_gz_path.unwrap_or(""))
        // All platforms are substituted, not just the one we think the host is: the script
        // re-derives `uname` itself and picks the matching digest, so a host that turns out to
        // be a different platform than the client guessed still verifies rather than silently
        // falling through to an empty expectation.
        .replace("{sha256_linux_x86_64}", &digest("linux", "x86_64"))
        .replace("{sha256_linux_aarch64}", &digest("linux", "aarch64"))
        .replace("{sha256_macos_x86_64}", &digest("macos", "x86_64"))
        .replace("{sha256_macos_aarch64}", &digest("macos", "aarch64"))
        // Last, so nothing inside the fragment is ever mistaken for a placeholder.
        .replace("{one_version_cleanup}", ONE_VERSION_CLEANUP_FRAGMENT)
}

/// Headlines the client prefixes onto an install failure when the install script refused to
/// install for an integrity reason (`install_remote_server.sh` exits 4, 5 and 6). Written by
/// `app/src/remote_server/ssh_transport.rs::route_install_failure` and read back by the SSH
/// failure banner (`app/src/terminal/view/ssh_remote_server_failed_banner.rs`), which picks its
/// title from them -- the install result reaches the banner only as a `String`, so these shared
/// constants are the contract between the two (#1086). Kept here rather than in either user
/// because this crate is the one both can import on every target.
///
/// Exit 4: this client was built with no pinned digest (every local/dev build).
pub const INSTALL_REFUSED_NO_PINNED_DIGEST: &str = "remote-server install refused: this build has no compiled-in checksum to \
     verify the remote binary against (a local/dev build), so it will not \
     install an unverified binary";
/// Exit 5: the remote host has no SHA-256 tool, so the download could not be checked.
pub const INSTALL_REFUSED_NO_DIGEST_TOOL: &str = "remote-server install refused: the remote host has no tool to compute a \
     SHA-256 digest, so the download could not be verified";
/// Exit 6: the downloaded bytes did not match the pinned digest.
pub const INSTALL_REFUSED_DIGEST_MISMATCH: &str = "remote-server install refused: integrity check failed, \
     so the install was not retried";

/// SHA-256 of the decompressed helper binary for one remote platform, embedded into this client
/// at **build time** from the helper files being bundled (`script/bundle` generates the
/// `PHOSPHOR_REMOTE_SHA256_*` variables from `bundled_resources/remote/`).
///
/// This is the integrity root for helper installs. Both the helper bytes and this digest come
/// from the same signed app bundle and reach the remote host down the user's already
/// authenticated **ssh channel**, so no network location is trusted, and nothing is fetched
/// from the internet at all. The client re-checks its bundled file against the digest before
/// uploading, and the remote script re-checks what it unpacked.
///
/// `None` when the variable was not set at compile time (any local build). Release builds then
/// refuse to install (fail closed, [`INSTALL_REFUSED_NO_PINNED_DIGEST`]); a source build uses a
/// digest it computes from the helper it bundles, see [`HelperDigest`].
pub fn pinned_helper_sha256(os: &str, arch: &str) -> Option<&'static str> {
    let digest = match (os, arch) {
        ("linux", "x86_64") => option_env!("PHOSPHOR_REMOTE_SHA256_LINUX_X86_64"),
        ("linux", "aarch64") => option_env!("PHOSPHOR_REMOTE_SHA256_LINUX_AARCH64"),
        ("macos", "x86_64") => option_env!("PHOSPHOR_REMOTE_SHA256_MACOS_X86_64"),
        ("macos", "aarch64") => option_env!("PHOSPHOR_REMOTE_SHA256_MACOS_AARCH64"),
        _ => None,
    };
    digest.map(str::trim).filter(|d| !d.is_empty())
}

/// Which digest an install requires, and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelperDigest {
    /// Compiled into this client.
    Pinned(String),
    /// Computed from the helper a source build bundled; guards the transfer only.
    Computed(String),
}

impl HelperDigest {
    pub fn hex(&self) -> &str {
        match self {
            Self::Pinned(h) | Self::Computed(h) => h,
        }
    }

    /// The value for [`install_script`]'s `sha256_override`: `None` for a pinned digest (the
    /// script uses the compiled-in table), the computed one otherwise.
    pub fn script_override(&self) -> Option<&str> {
        match self {
            Self::Pinned(_) => None,
            Self::Computed(h) => Some(h),
        }
    }
}

/// Decides the digest to require for a helper whose decompressed bytes hash to `actual_hex`.
///
/// * A pinned digest must equal `actual_hex`, or the bundle is not the one this client was built
///   with: `Err` carries the mismatch reason (fail closed, nothing is uploaded).
/// * With no pinned digest, only a source build (`dev_source_build`) may continue, on the
///   digest it computed; a release build refuses ([`INSTALL_REFUSED_NO_PINNED_DIGEST`]).
pub fn decide_helper_digest(
    pinned: Option<&str>,
    actual_hex: &str,
    dev_source_build: bool,
) -> Result<HelperDigest, String> {
    match pinned {
        Some(pinned) if pinned.eq_ignore_ascii_case(actual_hex) => {
            Ok(HelperDigest::Pinned(pinned.to_ascii_lowercase()))
        }
        Some(pinned) => Err(format!(
            "{INSTALL_REFUSED_DIGEST_MISMATCH_LOCAL}: expected {pinned}, bundled file is {actual_hex}"
        )),
        None if dev_source_build => Ok(HelperDigest::Computed(actual_hex.to_ascii_lowercase())),
        None => Err(INSTALL_REFUSED_NO_PINNED_DIGEST.to_string()),
    }
}

/// Headline for a bundled helper that does not match the digest compiled into this client
/// (a damaged or swapped app bundle). Nothing was uploaded.
pub const INSTALL_REFUSED_DIGEST_MISMATCH_LOCAL: &str = "remote-server install refused: the helper bundled with this app does not match the checksum \
     compiled into it, so nothing was uploaded";

/// Asset name of the bundled helper for a platform, without extension:
/// `phosphor-remote-<os>-<arch>` (the file on disk is `<name>.gz`).
pub fn helper_asset_name(platform: &RemotePlatform) -> String {
    format!(
        "{HELPER_BASE_NAME}-{}-{}",
        platform.os.as_str(),
        platform.arch.as_str()
    )
}

/// Start of every "no helper to upload" message (`ssh_transport::helper_unavailable`). The install
/// result reaches the manager only as a `String`, so this prefix is the contract that lets it tell
/// "nothing to install here" (a quiet fall-back to the no-helper path) from a real failure.
pub const HELPER_UNAVAILABLE_PREFIX: &str = "the phosphor-remote helper is not available for ";

/// Whether an install error means there was no helper to upload, rather than an install failing.
pub fn is_helper_unavailable(error: &str) -> bool {
    error.starts_with(HELPER_UNAVAILABLE_PREFIX)
}

/// Directory under the app's bundled resources that holds the compressed helpers.
pub const BUNDLED_HELPER_SUBDIR: &str = "remote";

/// Where the compressed helper for `platform` lives in this app's bundled resources
/// (`<bundled_resources>/remote/phosphor-remote-<os>-<arch>.gz`), if the app has a bundled
/// resources directory at all. The file may still be absent: a missing asset means "helper
/// unavailable" and the session stays on the no-helper path.
pub fn bundled_helper_asset_path(platform: &RemotePlatform) -> Option<std::path::PathBuf> {
    let file = format!("{}.gz", helper_asset_name(platform));
    // Test and manual-run override: a directory that directly holds the `.gz` files. The real
    // app harness (`script/realapp/appci`, the lab `upload` arms) uses it to hand a source
    // build the helper it just built, without a bundle.
    if let Some(dir) = std::env::var_os(HELPER_ASSET_DIR_ENV).filter(|d| !d.is_empty()) {
        return Some(std::path::PathBuf::from(dir).join(file));
    }
    warp_core::paths::bundled_resources_dir().map(|dir| dir.join(BUNDLED_HELPER_SUBDIR).join(file))
}

/// Environment variable naming a directory of `phosphor-remote-<os>-<arch>.gz` files that
/// replaces the bundled resources directory as the helper's source (tests, manual runs).
pub const HELPER_ASSET_DIR_ENV: &str = "PHOSPHOR_REMOTE_HELPER_ASSET_DIR";

/// Source builds have no bundled helper (`script/bundle` produces those), so the SSH transport
/// cross-compiles the helper, `phosphor-remote`, locally and uploads it through the same
/// in-band path a bundled one takes. The constants below describe that build.
///
/// Cross-compile target triple.
pub const DEV_MUSL_TARGET: &str = "x86_64-unknown-linux-musl";

/// Cargo profile used for cross-compiling: `[profile.release-remote-helper]` in the root
/// `Cargo.toml`, the same one CI builds the bundled helpers with.
pub const DEV_REMOTE_PROFILE: &str = "release-remote-helper";

/// Features of the helper build (`-p remote_server --no-default-features --features ...`).
pub const DEV_REMOTE_FEATURES: &str = "daemon";

/// Determines whether we're currently on the "dev-mode remote-server install" path.
///
/// Default condition: a **source build** — not packaged for distribution
/// (`is_release_bundle()`, i.e. the `release_bundle` feature that `script/bundle`
/// sets) and carrying no injected `GIT_RELEASE_TAG` (`app_version().is_none()`).
///
/// This deliberately does NOT key on `debug_assertions`. It used to, and that
/// left `cargo run --release` / `script/run --release` in a gap where neither
/// install path could work:
///
/// * the dev path was off, because `debug_assertions` is off under `--release`;
/// * the download path 404s, because with no release tag `download_url()` falls
///   to `latest/download`, which only resolves to a non-prerelease and every
///   release published so far is a prerelease (see that function's comment).
///
/// So the remote server could never install, `RemoteServerManager` had no
/// connected client, and `command_executor` fell back to the ControlMaster
/// `RemoteCommandExecutor` — which opens one SSH channel per command, exhausts
/// the remote's `MaxSessions`, and prints `channel N: open failed` into the
/// user's session. A release-*profile* source build is a build someone actually
/// runs, so it now takes the same path a debug source build does.
///
/// This is the third instance of the profile/bundle confusion; see the comment
/// in `enabled_features()` (`app/src/lib.rs`) for the one that cost a day.
/// Keying on the bundle feature rather than the profile is what keeps a
/// distributed build from ever attempting a local cross-compile on a user's
/// machine.
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
    !ChannelState::is_release_bundle() && ChannelState::app_version().is_none()
}

/// Timeout for checking whether the binary exists.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for the in-band upload install: the compressed helper (a few MB) streams over the
/// existing ControlMaster, then the host unpacks and verifies it.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);

/// The SCP fallback uploads the file and then runs the install script on it; same budget.
pub const SCP_INSTALL_TIMEOUT: Duration = Duration::from_secs(180);

/// Dev-mode cross-compiling may need to build the entire crate graph from scratch, so give it a very generous timeout.
pub const DEV_CROSS_COMPILE_TIMEOUT: Duration = Duration::from_secs(900);

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
