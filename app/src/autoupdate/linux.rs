use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use channel_versions::VersionInfo;
use instant::Duration;
use warp_core::channel::{Channel, ChannelState};

use super::release_assets_directory_url;
use super::{DownloadProgress, DownloadReady, ProgressCallback, ReadyForRelaunch};

lazy_static::lazy_static! {
    /// Stores the path to the current executable.
    ///
    /// We cache this before running auto-update because the returned path for
    /// a deleted file includes " (deleted)" _in the file name_, which breaks
    /// the relaunch logic.
    static ref CURRENT_EXE: std::io::Result<PathBuf> = std::env::current_exe();
}

pub(super) async fn download_update_and_cleanup(
    version_info: &VersionInfo,
    _update_id: &str,
    client: &http_client::Client,
    on_progress: ProgressCallback,
) -> Result<DownloadReady> {
    match UpdateMethod::detect() {
        UpdateMethod::Unknown => Ok(DownloadReady::NeedsAuthorization),
        UpdateMethod::AppImage(appimage_path) => {
            appimage::download_update_and_cleanup(version_info, &appimage_path, client, on_progress)
                .await
        }
        UpdateMethod::PackageManager(package_manager) => {
            log::info!("Detected that Phosphor was installed using {package_manager:?}");
            Ok(DownloadReady::NeedsAuthorization)
        }
    }
}

pub(super) fn apply_update() -> Result<ReadyForRelaunch> {
    // Make sure CURRENT_EXE is initialized before we actually apply the update.
    let _ = CURRENT_EXE.as_ref();

    match UpdateMethod::detect() {
        UpdateMethod::Unknown => bail!("Cannot apply update for unknown update method!"),
        UpdateMethod::AppImage(_) => Ok(ReadyForRelaunch::Yes),
        UpdateMethod::PackageManager(package_manager) => bail!(
            "Phosphor does not support package-manager autoupdate for {package_manager}; install the new release manually"
        ),
    }
}

pub(super) fn relaunch() -> Result<()> {
    match UpdateMethod::detect() {
        UpdateMethod::Unknown => bail!("Don't know how to relaunch for an unknown update method!"),
        UpdateMethod::AppImage(appimage_path) => appimage::relaunch(&appimage_path),
        UpdateMethod::PackageManager(_) => package_manager::relaunch(),
    }
}

mod appimage {
    use std::path::Path;

    use super::*;

    pub(super) async fn download_update_and_cleanup(
        version_info: &VersionInfo,
        appimage_path: &Path,
        client: &http_client::Client,
        on_progress: ProgressCallback,
    ) -> Result<DownloadReady> {
        use futures::StreamExt as _;
        use instant::Instant;
        const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

        let channel = ChannelState::channel();
        // openWarp: fetch the real download URL from the cached GitHub Release,
        // bypassing the empty releases_base_url. Official channels still go
        // through release_assets_directory_url.
        let url = if matches!(channel, warp_core::channel::Channel::Oss) {
            // OSS Linux AppImage's default asset name is "Phosphor-x86_64.AppImage"
            // (derived from the .desktop file's Name= field by linuxdeploy).
            // The known release asset name is fixed in GitHub Actions.
            let asset = "Phosphor-x86_64.AppImage";
            if let Some(release) = crate::autoupdate::github::cached_release() {
                if let Some(found) = release.find_asset(asset) {
                    found.browser_download_url.clone()
                } else {
                    log::warn!(
                        "openWarp: cached release tag {} has no asset named {asset}, falling back to the tag URL",
                        release.tag_name
                    );
                    format!(
                        "https://github.com/{}/releases/download/v{}/{asset}",
                        repo_name(channel),
                        version_info.version
                    )
                }
            } else {
                format!(
                    "https://github.com/{}/releases/download/v{}/{asset}",
                    repo_name(channel),
                    version_info.version
                )
            }
        } else {
            let Some(appimage_name) = option_env!("APPIMAGE_NAME") else {
                bail!("APPIMAGE_NAME environment variable was not set at compile time!");
            };
            format!(
                "{}/{}",
                release_assets_directory_url(channel, &version_info.version),
                appimage_name
            )
        };

        // Create a temporary file that we'll write the download into.
        let mut new_appimage = tempfile::NamedTempFile::new()?;

        log::info!("Downloading {url} to {}...", new_appimage.path().display());

        let response = client
            .get(&url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await?
            .error_for_status()?;

        // Stream-read chunks + write, throttling progress reporting along the
        // way. The AppImage is large (tens of MB); a single `.bytes()` call
        // would block the whole UI until the download finished, so this uses a
        // stream instead so the UI can show progress.
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
            new_appimage.as_file_mut().write_all(&chunk)?;
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

        // openWarp: verify the temp file's SHA-256 before overwriting the
        // original AppImage, to guard against a CDN man-in-the-middle / network
        // corruption. Other channels skip this (they have their own process).
        if matches!(channel, warp_core::channel::Channel::Oss) {
            let temp_path = new_appimage.path().to_path_buf();
            if let Err(e) =
                crate::autoupdate::verify_oss_asset_sha256(&temp_path, "Phosphor-x86_64.AppImage")
            {
                // The temp file is cleaned up automatically when NamedTempFile
                // drops; only the error needs to be returned here.
                return Err(e);
            }
        }

        log::info!(
            "Copying downloaded AppImage from {} to {}",
            new_appimage.path().display(),
            appimage_path.display()
        );

        // Copy permissions to new app before moving it to ensure we don't leave it
        // in a bad state if the move succeeds but we are unable to update the
        // permissions afterwards.
        new_appimage
            .as_file_mut()
            .set_permissions(appimage_path.metadata()?.permissions())?;

        // Move new AppImage over the one that launched the current Zap instance.
        let new_appimage_path = new_appimage.into_temp_path();
        let mv_status = command::r#async::Command::new("mv")
            .arg(new_appimage_path.as_os_str())
            .arg(appimage_path)
            .output()
            .await?
            .status;
        if !mv_status.success() {
            bail!("Failed to move new AppImage over the old one: {mv_status}");
        }

        // Ensure we don't accidentally drop `new_appimage_path` before we finish
        // moving it to its final location.
        let _ = new_appimage_path;

        Ok(DownloadReady::Yes)
    }

    pub(super) fn relaunch(appimage_path: &Path) -> Result<()> {
        let mut command = command::blocking::Command::new(appimage_path);
        // Pass a flag to the app to let it know it was restarted as part of the
        // autoupdate process.
        command.arg(warp_cli::finish_update_flag());
        // When testing the local channel version JSON, let the newly launched
        // binary keep referencing the same file, so we can verify the changelog
        // display after auto-update.
        if let Ok(path) = std::env::var("WARP_CHANNEL_VERSIONS_PATH") {
            command.env("WARP_CHANNEL_VERSIONS_PATH", path);
        }

        log::info!("Relaunching warp for update...");
        command.spawn()?;
        Ok(())
    }
}

mod package_manager {
    use super::*;

    pub(super) fn relaunch() -> Result<()> {
        let Ok(program) = CURRENT_EXE.as_ref() else {
            bail!(
                "Failed to get path to current executable to relaunch after completing auto-update"
            );
        };
        log::info!("Relaunching using path: {program:?}");
        let mut command = command::blocking::Command::new(program);
        // Add any arguments that were passed to warp, skipping the first
        // argument (the name of the executable) and dropping the flag for
        // finishing an update.
        let finish_update_flag = warp_cli::finish_update_flag();
        command.args(
            std::env::args()
                .skip(1)
                .filter(|arg| arg != &finish_update_flag),
        );
        // Pass a flag to the app to let it know it was restarted as part of the
        // autoupdate process.
        command.arg(finish_update_flag);
        // When testing the local channel version JSON, let the newly launched
        // binary keep referencing the same file, so we can verify the changelog
        // display after auto-update.
        if let Ok(path) = std::env::var("WARP_CHANNEL_VERSIONS_PATH") {
            command.env("WARP_CHANNEL_VERSIONS_PATH", path);
        }

        log::info!("Relaunching warp for update...");
        command.spawn()?;
        Ok(())
    }
}

/// Returns which method should be used to update Zap.
#[derive(Debug)]
pub(crate) enum UpdateMethod {
    /// We don't know how to update Zap.
    Unknown,
    /// Zap is running as an AppImage and should be updated in-place.
    AppImage(PathBuf),
    /// Zap can be updated using the given package manager.
    PackageManager(PackageManager),
}

impl UpdateMethod {
    pub(crate) fn detect() -> Self {
        if let Some(appimage_path) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
            return Self::AppImage(appimage_path);
        }
        if let Ok(package_manager) = PackageManager::detect() {
            // Log the upgrade command the user should run, to make it easy to
            // find from the logs when troubleshooting. The UI still falls back
            // to the GitHub release page (the user can download the .deb/.rpm and
            // apt install / dnf install it themselves).
            package_manager.log_upgrade_hint();
            return Self::PackageManager(package_manager);
        }
        Self::Unknown
    }
}

/// Package managers that we understand and can assist with auto-update
/// for. `Pacman` distinguishes two cases: `PacmanOfficial` means the package
/// came from archlinux.org's official repo (can run `sudo pacman -Syu`
/// directly); `PacmanAur` means the package came from the AUR or a local
/// manual `makepkg -si`, in which case an AUR helper should be used
/// (`paru -Syu` / `yay -Syu`) instead of having the user run `pacman -U` on a
/// release asset that doesn't exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageManager {
    Apt {
        package_name: String,
    },
    Yum {
        package_name: String,
    },
    Dnf {
        package_name: String,
    },
    Zypper {
        package_name: String,
    },
    /// A pacman package from archlinux.org's official repo (`pacman -Si <pkg>` hits).
    PacmanOfficial {
        package_name: String,
    },
    /// AUR / manual local install (`pacman -Qi <pkg>` hits but `pacman -Si <pkg>` doesn't).
    PacmanAur {
        package_name: String,
    },
}

impl PackageManager {
    /// Candidate package names to look up in the system package manager for the
    /// current channel, ordered from most to least likely. OSS's deb/rpm/arch
    /// bundle scripts all use `phosphor` as the package name (see
    /// script/linux/bundle_*), but common naming on the AUR is `phosphor-bin` /
    /// `phosphor-git`, so several are tried.
    ///
    /// These must track the package names the bundle scripts actually produce.
    /// A mismatch does not fail loudly — the lookup simply never matches and
    /// autoupdate detection silently reports no package manager.
    fn candidate_names(channel: Channel) -> &'static [&'static str] {
        match channel {
            Channel::Stable => &["warp-terminal"],
            Channel::Preview => &["warp-terminal-preview"],
            Channel::Dev => &["warp-terminal-dev"],
            Channel::Integration => &["warp-terminal-integration"],
            Channel::Local => &["warp-terminal-local"],
            // OSS: bundle_deb/rpm/arch all use `phosphor` as the package name,
            // but AUR maintainers might pick `phosphor-bin` / `phosphor-git`,
            // so try those too.
            Channel::Oss => &["phosphor", "phosphor-bin", "phosphor-git"],
        }
    }

    fn detect() -> Result<Self> {
        let channel = ChannelState::channel();
        let candidates = Self::candidate_names(channel);

        // Try each candidate package name in turn; return the first one that any
        // PM recognizes as installed. After a pacman hit, use `pacman -Si` to
        // distinguish official repo / AUR.
        for &name in candidates {
            if let Some(pm) = Self::probe_one(name)? {
                return Ok(pm);
            }
        }
        bail!(
            "Could not determine which package manager was used to install \
             this build (tried candidate names: {candidates:?})"
        );
    }

    /// Runs the detection script for a specific package name; returns the
    /// corresponding PackageManager on a hit, or None on a miss. After a pacman
    /// hit, additionally queries `pacman -Si` to distinguish official repo from AUR.
    fn probe_one(package_name: &str) -> Result<Option<Self>> {
        // `$PACKAGE_NAME` is passed via env in the shell script, so its content
        // won't be shell-escaped/injected (it's passed to `command`, not
        // interpolated into an `sh -c` string).
        let detect_script = r#"
            command -p pacman -Qi "$PACKAGE_NAME" >/dev/null 2>/dev/null
            if [ $? -eq 0 ]; then
              # Distinguish official repo vs AUR/manual. -Si queries the sync
              # database; AUR/manually installed packages won't show up there.
              if command -p pacman -Si "$PACKAGE_NAME" >/dev/null 2>/dev/null; then
                echo "pacman-official"
              else
                echo "pacman-aur"
              fi
              exit
            fi

            command -p zypper search --match-exact --installed-only "$PACKAGE_NAME" >/dev/null 2>/dev/null
            if [ $? -eq 0 ]; then
              echo "zypper"
              exit
            fi

            command -p dnf list --installed "$PACKAGE_NAME" >/dev/null 2>/dev/null
            if [ $? -eq 0 ]; then
              echo "dnf"
              exit
            fi

            command -p yum list installed "$PACKAGE_NAME" >/dev/null 2>/dev/null
            if [ $? -eq 0 ]; then
              echo "yum"
              exit
            fi

            if [ "$(command -p dpkg-query --show --showformat='${db:Status-Status}' "$PACKAGE_NAME" 2>/dev/null)" = "installed" ]; then
              echo "apt"
              exit
            fi

            exit 1
        "#;

        let output = command::blocking::Command::new("sh")
            .args(["-c", detect_script])
            .env("PACKAGE_NAME", package_name)
            .output();
        let output = match output {
            Ok(o) => o,
            Err(err) => {
                return Err(err).context("Failed to run package manager detection script")
            }
        };

        // exit 1 = this candidate name wasn't recognized by any PM; not an error, try the next candidate.
        if !output.status.success() {
            return Ok(None);
        }
        let stdout = std::str::from_utf8(&output.stdout)
            .map_err(|_| anyhow::anyhow!("non-UTF-8 detect script output"))?;
        let name = package_name.to_string();
        let pm = match stdout.trim() {
            "pacman-official" => Self::PacmanOfficial { package_name: name },
            "pacman-aur" => Self::PacmanAur { package_name: name },
            "zypper" => Self::Zypper { package_name: name },
            "dnf" => Self::Dnf { package_name: name },
            "yum" => Self::Yum { package_name: name },
            "apt" => Self::Apt { package_name: name },
            other => bail!("Unexpected detection output: {other}"),
        };
        Ok(Some(pm))
    }

    /// Writes "the upgrade command the user should run" to the log. OSS users
    /// can find the precise instructions in the logs under
    /// ~/.local/share/phosphor/; the UI still falls back to "go download from
    /// GitHub" without distinguishing by package manager.
    fn log_upgrade_hint(&self) {
        let hint = match self {
            Self::Apt { package_name } => {
                format!(
                    "Please run: download the .deb from the GitHub Release, then `sudo apt install ./{package_name}_*.deb`,\
                     or add the release as an apt source and run `sudo apt update && sudo apt install {package_name}`"
                )
            }
            Self::Yum { package_name } => {
                format!("Please run: download the .rpm, then `sudo yum install ./{package_name}-*.rpm`")
            }
            Self::Dnf { package_name } => {
                format!("Please run: download the .rpm, then `sudo dnf install ./{package_name}-*.rpm`")
            }
            Self::Zypper { package_name } => {
                format!("Please run: download the .rpm, then `sudo zypper install ./{package_name}-*.rpm`")
            }
            Self::PacmanOfficial { package_name } => {
                format!("Please run: `sudo pacman -Syu {package_name}`")
            }
            Self::PacmanAur { package_name } => {
                format!(
                    "It looks like you installed {package_name} from the AUR. Please upgrade using an AUR helper,\
                     e.g.: `paru -Syu {package_name}` or `yay -Syu {package_name}`.\
                     Do not run pacman -U manually — the GitHub Release doesn't ship a .pkg.tar.zst asset."
                )
            }
        };
        log::info!("openWarp upgrade hint: {hint}");
    }
}

impl std::fmt::Display for PackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageManager::Apt { .. } => write!(f, "apt"),
            PackageManager::Yum { .. } => write!(f, "yum"),
            PackageManager::Dnf { .. } => write!(f, "dnf"),
            PackageManager::Zypper { .. } => write!(f, "zypper"),
            PackageManager::PacmanOfficial { .. } => write!(f, "pacman (official)"),
            PackageManager::PacmanAur { .. } => write!(f, "pacman (AUR)"),
        }
    }
}

/// The GitHub `owner/repo` slug that publishes this channel's release assets.
///
/// Warp derives a *package-repository* name per channel (`warpdotdev`,
/// `warpdotdev-dev`, ...) identifying its self-hosted apt/dnf/pacman
/// repositories on releases.warp.dev. This fork has no self-hosted package
/// repositories — that cloud release infrastructure was amputated — so rather
/// than a package-repo name this returns the GitHub repository that actually
/// serves the fork's release assets. Deriving the repo from the channel (rather
/// than hardcoding the slug inline) mirrors Warp's per-channel design and keeps
/// the two AppImage download-URL fallbacks above in sync from a single source.
///
/// The fork ships every channel it publishes — in practice the open-source
/// `Oss` build — from one GitHub repository, so all channels resolve to the
/// same slug today; the exhaustive `match` leaves room to point a future
/// channel at its own repository without touching the call sites.
fn repo_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable
        | Channel::Preview
        | Channel::Dev
        | Channel::Local
        | Channel::Integration
        | Channel::Oss => "jwp2987/phosphor",
    }
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
