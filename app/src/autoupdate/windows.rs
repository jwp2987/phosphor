use crate::server::telemetry::TelemetryEvent;
use anyhow::anyhow;
use anyhow::{bail, Result};
use channel_versions::VersionInfo;
use command::blocking::Command;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};
use std::{io::Write as _, time::Duration};
use tempfile::TempPath;
use warp_core::channel::{Channel, ChannelState};
use warpui::AppContext;

use super::{
    github, release_assets_directory_url, DownloadProgress, DownloadReady, ProgressCallback,
};
use crate::util::windows::install_dir;

lazy_static! {
    /// The temporary file that stores the installer for the new update, plus
    /// the digest it was verified against at download time.
    static ref INSTALLER_PATH: Arc<Mutex<Option<DownloadedInstaller>>> = Default::default();
}

/// A downloaded installer, kept together with whatever we know about its
/// contents so `relaunch` can re-check them immediately before executing it.
struct DownloadedInstaller {
    path: TempPath,
    /// Lowercase-hex SHA-256 recorded by `verify_oss_asset_sha256` at download
    /// time, on the channels that have one.
    ///
    /// `None` on the official channels, and it means exactly what it looks
    /// like: nothing re-checks those bytes before `relaunch` executes them.
    /// There is no digest in hand to compare against -- the official download
    /// path publishes none -- and nothing else in this process fills the gap.
    /// The tree's only signature verification is `mac::verify_code_signature`,
    /// which is macOS-only; there is no Windows-side equivalent.
    ///
    /// An earlier version of this comment claimed Authenticode covered it. It
    /// does not, and the claim was worth removing rather than softening,
    /// because each half of it is false in a different way:
    /// * `CreateProcess` does not validate Authenticode. Signature checking on
    ///   Windows is opt-in (`WinVerifyTrust`) or policy-driven (WDAC / AppLocker
    ///   in a mode nobody enables by default); a plain `Command::new(path)`
    ///   performs none of it.
    /// * SmartScreen and the "publisher unknown" UAC prompt key off the
    ///   mark-of-the-web, which is attached by the *downloader*. This installer
    ///   is written to `%TEMP%` by this process, so it carries no MOTW, and UAC
    ///   only displays a publisher string it reads from the file -- it does not
    ///   gate on it.
    ///
    /// What actually protects the official channels is the TLS fetch from the
    /// releases host plus Inno Setup's own integrity check of its payload, and
    /// neither of those covers the file sitting in `%TEMP%` between download
    /// and launch. Closing that properly means a `WinVerifyTrust` call with a
    /// pinned publisher, which this fork has not written. The OSS path does
    /// record `Some` -- a failed verification returns before `INSTALLER_PATH`
    /// is populated at all -- so the re-check in `relaunch` covers that channel
    /// and only that channel.
    verified_sha256: Option<String>,
}

/// Download the Inno Setup install wizard, the same one users run on the first Zap install, and
/// place it into the "data dir".
pub(super) async fn download_update_and_cleanup(
    version_info: &VersionInfo,
    _update_id: &str,
    client: &http_client::Client,
    on_progress: ProgressCallback,
) -> Result<DownloadReady> {
    use futures::StreamExt as _;
    use instant::Instant;
    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

    let channel = ChannelState::channel();
    let installer_file_name = installer_file_name()?;
    // openWarp: fetch the real download URL from the cached GitHub Release
    // (asset names are PhosphorSetup.exe / PhosphorSetup-arm64.exe, see
    // installer_file_name()). Other channels use the official base url.
    let url = if matches!(channel, Channel::Oss) {
        if let Some(release) = github::cached_release() {
            if let Some(found) = release.find_asset(&installer_file_name) {
                found.browser_download_url.clone()
            } else {
                log::warn!(
                    "Phosphor: cached release tag {} has no asset named {installer_file_name}, falling back to the tag URL",
                    release.tag_name
                );
                format!(
                    "https://github.com/jwp2987/phosphor/releases/download/v{}/{installer_file_name}",
                    version_info.version
                )
            }
        } else {
            format!(
                "https://github.com/jwp2987/phosphor/releases/download/v{}/{installer_file_name}",
                version_info.version
            )
        }
    } else {
        format!(
            "{}/{}",
            release_assets_directory_url(channel, &version_info.version),
            installer_file_name
        )
    };

    // Reuse a previously staged installer only on a channel that will check the
    // bytes it reuses.
    //
    // `rand_bytes(0)` makes this path fully predictable -- `%TEMP%` plus the
    // version and the asset name -- so "a file is already here" is not evidence
    // that *we* put it here, and the file is later spawned elevated by
    // `relaunch`. On the OSS channel the reused bytes still have to pass
    // `verify_oss_asset_sha256` below before `INSTALLER_PATH` is populated at
    // all, so a leftover, a truncated download or a planted file is rejected
    // there and the `NamedTempFile` drop takes it away. On the official
    // channels there is no digest to check against (see
    // `DownloadedInstaller::verified_sha256`), so a reused file would be run
    // having been examined by nothing whatsoever; the TLS fetch is the weaker-
    // but-real check those channels do have, so pay for it rather than adopt an
    // unexamined file.
    let may_reuse_staged_installer = matches!(channel, Channel::Oss);

    // Create a temporary file that we'll write the download into.
    let mut already_exists = false;
    let mut new_installer = tempfile::Builder::new()
        .rand_bytes(0)
        .suffix(&format!("{}-{}", version_info.version, installer_file_name))
        .make(|path| {
            // Treat a 0-byte file as missing, as the pin does. `path.is_file()`
            // -- what this used to be -- adopts an empty or partial leftover as
            // the installer: the download is then skipped entirely, so nothing
            // ever replaces those bytes. A metadata error counts as missing for
            // the same reason: "could not read" must not become "safe to
            // reuse", and the recovery here (download it again) is cheap and
            // loses nothing.
            already_exists = may_reuse_staged_installer
                && path.metadata().map(|m| m.len() > 0).unwrap_or(false);
            if already_exists {
                File::open(path)
            } else {
                File::create(path)
            }
        })?;

    if !already_exists {
        log::info!("Downloading {url} to {}...", new_installer.path().display());

        let response = client
            .get(&url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await?
            .error_for_status()?;

        let total = response
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        on_progress(DownloadProgress {
            downloaded: 0,
            total,
        });

        let mut downloaded: u64 = 0;
        let mut last_reported = 0u64;
        let mut last_reported_at = Instant::now();
        const REPORT_BYTES_THRESHOLD: u64 = 64 * 1024;
        const REPORT_TIME_THRESHOLD: Duration = Duration::from_millis(250);

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            new_installer.as_file_mut().write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            if downloaded - last_reported >= REPORT_BYTES_THRESHOLD
                || last_reported_at.elapsed() >= REPORT_TIME_THRESHOLD
            {
                on_progress(DownloadProgress {
                    downloaded,
                    total,
                });
                last_reported = downloaded;
                last_reported_at = Instant::now();
            }
        }
        on_progress(DownloadProgress {
            downloaded,
            total,
        });
    } else {
        // Reuse a previously downloaded installer with the same name: no new
        // request is made, just push one extra progress report so the UI
        // directly shows 100%.
        let downloaded = new_installer
            .as_file_mut()
            .metadata()
            .ok()
            .map(|m| m.len())
            .unwrap_or(0);
        on_progress(DownloadProgress {
            downloaded,
            total: Some(downloaded),
        });
    }

    // openWarp: verify the SHA-256 from the GitHub Release metadata, guarding
    // against a CDN man-in-the-middle / corruption. On verification failure,
    // return Err directly; the installer temp file gets cleaned up afterward
    // when TempPath drops. It's deliberately not placed into INSTALLER_PATH
    // here (otherwise a subsequent relaunch() could mistakenly use it).
    let verified_sha256 = if matches!(channel, Channel::Oss) {
        let temp_path = new_installer.path().to_path_buf();
        // Keep the digest: unlike Linux, which moves the verified bytes into
        // place within microseconds of checking them, this file sits on disk
        // from the end of the download until the user clicks "Install and
        // relaunch" -- minutes or hours later, in the user's temp directory.
        // `relaunch` re-checks it against this value before executing it.
        Some(super::verify_oss_asset_sha256(
            &temp_path,
            &installer_file_name,
        )?)
    } else {
        None
    };

    *INSTALLER_PATH.lock() = Some(DownloadedInstaller {
        path: new_installer.into_temp_path(),
        verified_sha256,
    });

    Ok(DownloadReady::Yes)
}

const UPDATE_LOG_FILENAME: &str = "warp_update.log";

fn autoupdate_log_file() -> Result<PathBuf> {
    warp_logging::log_directory().map(|dir| dir.join(UPDATE_LOG_FILENAME))
}

fn parse_exit_code_after_marker(contents_lowercase: &[u8], failed_marker: &[u8]) -> Option<i32> {
    const EXIT_CODE_MARKER: &[u8] = b"exit code: ";

    let failed_pos = memchr::memmem::find(contents_lowercase, failed_marker)?;
    let after_failed = &contents_lowercase[failed_pos..];
    let marker_pos = memchr::memmem::find(after_failed, EXIT_CODE_MARKER)?;
    let after_marker = &after_failed[marker_pos + EXIT_CODE_MARKER.len()..];
    let sign_len = if after_marker.first() == Some(&b'-') {
        1
    } else {
        0
    };
    let digit_len = after_marker[sign_len..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digit_len == 0 {
        return None;
    }
    std::str::from_utf8(&after_marker[..sign_len + digit_len])
        .ok()?
        .parse()
        .ok()
}

/// Parses the taskkill exit code from an Inno Setup log containing a
/// "force-kill failed for" line. Returns `None` if no such line is found or
/// the exit code cannot be parsed.
fn parse_forcekill_exit_code(contents_lowercase: &[u8]) -> Option<i32> {
    const FAILED_MARKER: &[u8] = b"force-kill failed for";
    parse_exit_code_after_marker(contents_lowercase, FAILED_MARKER)
}

/// Parses the PowerShell exit code from an Inno Setup log containing a
/// "minidump-server cleanup failed" line. Returns `None` if no such line is
/// found or the exit code cannot be parsed.
fn parse_minidump_cleanup_exit_code(contents_lowercase: &[u8]) -> Option<i32> {
    const FAILED_MARKER: &[u8] = b"minidump-server cleanup failed";
    parse_exit_code_after_marker(contents_lowercase, FAILED_MARKER)
}

/// Checks the autoupdate log file from a previous update attempt.
/// Records known issues found during the previous update attempt.
/// The log file is renamed after processing to avoid duplicate reports on subsequent launches.
pub(super) fn check_and_report_update_errors(ctx: &mut AppContext) {
    let log_path = match autoupdate_log_file() {
        Ok(path) => path,
        Err(e) => {
            log::warn!("Failed to determine autoupdate log file path: {e:#}");
            return;
        }
    };

    // Inno Setup logs use the system's active codepage (often Windows-1252), not UTF-8.
    // We read as raw bytes to avoid silently skipping non-UTF-8 log files.
    let contents = match fs::read(&log_path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            log::info!("No autoupdate logs found");
            return;
        }
        Err(e) => {
            log::warn!("Failed to read autoupdate log file: {e:#}");
            return;
        }
    };

    let contents_lowercase = contents.to_ascii_lowercase();

    let has_unable_to_close = memchr::memmem::find(
        &contents_lowercase,
        b"setup was unable to automatically close all applications",
    )
    .is_some();
    if has_unable_to_close {
        crate::send_telemetry_sync_from_app_ctx!(
            TelemetryEvent::AutoupdateUnableToCloseApplications,
            ctx
        );
    }

    let has_file_in_use = memchr::memmem::find(
        &contents_lowercase,
        b"the process cannot access the file because it is being used by another process",
    )
    .is_some();
    if has_file_in_use {
        crate::send_telemetry_sync_from_app_ctx!(TelemetryEvent::AutoupdateFileInUse, ctx);
    }

    // Fired when the mutex polling loop timed out and a force-kill was attempted.
    let has_mutex_timeout =
        memchr::memmem::find(&contents_lowercase, b"warp mutex still held after timeout").is_some();
    if has_mutex_timeout {
        crate::send_telemetry_sync_from_app_ctx!(TelemetryEvent::AutoupdateMutexTimeout, ctx);
    }

    // Fired when taskkill returned non-zero after the mutex timeout.
    // Exit code 128 means "no matching process found" — the process was already
    // gone when taskkill ran — so suppress that harmless race condition.
    if let Some(exit_code) = parse_forcekill_exit_code(&contents_lowercase)
        && exit_code != 128
    {
        log::warn!("Phosphor: autoupdate force-kill failed (exit code {exit_code})");
        crate::send_telemetry_sync_from_app_ctx!(TelemetryEvent::AutoupdateForcekillFailed, ctx);
    }

    // The PowerShell cleanup of the orphaned minidump-server process returned a
    // non-zero exit code. The fork has no dedicated telemetry event for this
    // (cloud telemetry is amputated), so it is recorded in the local log
    // instead — matching how the fork records autoupdate errors locally below.
    if let Some(exit_code) = parse_minidump_cleanup_exit_code(&contents_lowercase) {
        log::warn!("Phosphor: autoupdate minidump-server cleanup failed (exit code {exit_code})");
    }

    // openWarp doesn't upload autoupdate failure logs; it only records the
    // error count locally. The complete log file is preserved via the
    // `.log.reported` rename below, so users/debuggers can look at the local
    // file directly when needed.
    #[cfg(feature = "crash_reporting")]
    {
        const IGNOREABLE_ERRORS: &[&[u8]] = &[
            b"there is not enough space on the disk",
            b"setprocessmitigationpolicy failed with error code 87",
            // Bundled skill files whose names contain "error" appear in "Dest filename:" log lines
            // and produce false positives.
            b"error-codes.md",
            b"error-recovery.md",
        ];

        let mut error_count = memchr::memmem::find_iter(&contents_lowercase, b"error").count();
        for pattern in IGNOREABLE_ERRORS {
            let ignoreable_count = memchr::memmem::find_iter(&contents_lowercase, pattern).count();
            error_count = error_count.saturating_sub(ignoreable_count);
        }

        if error_count > 0 {
            log::error!(
                "Phosphor: Windows auto-update log contains {error_count} error(s) (log: {:?})",
                log_path
            );
        }
        let _ = &contents;
    }

    // Rename the log file to avoid duplicate reports on subsequent launches.
    // We keep the file around so the user can still view it or attach it to a GitHub issue.
    let reported_path = log_path.with_extension("log.reported");
    if let Err(e) = fs::rename(&log_path, &reported_path) {
        log::warn!("Failed to rename autoupdate log file after reporting: {e:#}");
    }
}

pub(super) fn relaunch() -> Result<()> {
    let channel = ChannelState::channel();

    let install_dir = install_dir()?;
    let Some(DownloadedInstaller {
        path: installer_path,
        verified_sha256,
    }) = INSTALLER_PATH.lock().take()
    else {
        bail!("No installer path");
    };

    // Re-verify adjacent to the use. The download-time check happened when the
    // bytes landed; between then and now the user has been running the app and
    // the installer has been sitting in a world-readable, user-writable temp
    // directory. This is the same reasoning as `mac::oss_open_installer`, and a
    // wider window than that one -- it spans however long the user took to
    // click "Install and relaunch", not an app shutdown.
    //
    // The gap that remains here is only between this hash and `cmd.spawn()`
    // below, with no process teardown in between, which is as close as a
    // path-based `CreateProcess` allows.
    //
    // `None` means no re-check happens -- see `DownloadedInstaller`. That is
    // the official channels, where nothing in this process verifies the file
    // before running it; it is a stated gap, not a check that passed.
    if let Some(expected) = verified_sha256 {
        let actual = super::sha256_file(&installer_path)?;
        if actual != expected {
            bail!(
                "Phosphor: refusing to run {}: its contents changed since verification (expected={expected} actual={actual})",
                installer_path.display()
            );
        }
        log::info!("Phosphor: re-verified installer SHA-256 immediately before launching it");
    } else {
        log::warn!(
            "Phosphor: launching {} without re-verifying it; no digest was recorded for this channel",
            installer_path.display()
        );
    }

    let log_arg = match autoupdate_log_file() {
        Ok(dir) => format!("/LOG={}", dir.display()),
        Err(e) => {
            log::warn!("Failed to determine location for autoupdate logs: {e:#}");
            "/LOG".to_string()
        }
    };

    // openWarp (Channel::Oss): Inno Setup runs "non-silent". Omitting /SILENT
    // lets the user see the standard install UI, so they can personally
    // confirm the version and destination directory being installed, and
    // cancel through the normal UI. /SP- is still kept to skip the "ready to
    // install" confirmation dialog; /NORESTART avoids requiring a Windows
    // restart; /update=1 is used by the Inno script to detect upgrade mode.
    // /NOCLOSEAPPLICATIONS lets Inno wait for the current Zap process to exit
    // naturally (mutex poll), instead of forcing RestartManager to kill the process.
    let mut cmd = Command::new(&installer_path);
    if matches!(channel, Channel::Oss) {
        cmd.args([
            "/SP-",
            "/NORESTART",
            &log_arg,
            "/update=1",
            "/NOCLOSEAPPLICATIONS",
            &format!("/DIR={}", install_dir.display()),
        ]);
    } else {
        // Official channel: keep the original "silent + progress bar" behavior, installing and restarting automatically.
        // The Inno Setup install wizard will run without user input. It will re-launch Zap after
        // installing the update files.
        // https://jrsoftware.org/ishelp/index.php?topic=setupcmdline
        cmd.args([
            // Skip asking the user to confirm.
            "/SP-",
            // Do not prompt the user for anything. Note that we do not use "VERYSILENT" so that a
            // progress bar is still shown. This is useful since the update process may take a few
            // seconds.
            "/SILENT",
            // Do not provide a cancel button on the progress bar page.
            "/NOCANCEL",
            // Indicate that restarting Windows is not necessary.
            "/NORESTART",
            &log_arg,
            "/update=1",
            // Do not forcibly kill Zap via RestartManager. The installer will wait for
            // Zap to exit naturally by polling the single-instance mutex instead.
            "/NOCLOSEAPPLICATIONS",
            &format!("/DIR={}", install_dir.display()),
        ]);
    }
    cmd.spawn()?;

    // DEV ONLY: Sleep after spawning the installer so this process is still alive
    // when Inno Setup tries to overwrite files. This reliably reproduces the
    // auto-update race condition (APP-3702) for testing.
    if matches!(channel, Channel::Dev) {
        log::info!("DEV: Sleeping 10s after spawning installer to reproduce update race");
        std::thread::sleep(Duration::from_secs(10));
    }

    Ok(())
}

fn installer_file_name() -> Result<String> {
    let app_name_prefix = app_name_prefix(ChannelState::channel());

    // For example, on arm64 this is WarpSetup-arm64.exe and on x64 this is
    // WarpSetup.exe.
    if cfg!(target_arch = "aarch64") {
        Ok(format!("{app_name_prefix}Setup-arm64.exe"))
    } else if cfg!(target_arch = "x86_64") {
        Ok(format!("{app_name_prefix}Setup.exe"))
    } else {
        Err(anyhow!(
            "Could not construct setup file name for unsupported architecture"
        ))
    }
}

fn app_name_prefix(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "Zap",
        Channel::Preview => "WarpPreview",
        Channel::Local => "warp",
        Channel::Integration => "integration",
        Channel::Dev => "WarpDev",
        // Aligned with script/windows/bundle.ps1's OSS branch
        // INSTALLER_NAME=Phosphor+Setup, so the GitHub Release asset name
        // PhosphorSetup.exe is generated correctly by installer_file_name().
        Channel::Oss => "Phosphor",
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
