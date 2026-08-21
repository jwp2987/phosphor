#![allow(deprecated)]

use command::{blocking, r#async::Command};
use futures::StreamExt;
use futures_lite::future;
use instant::Instant;
use std::{
    env,
    ffi::CString,
    fs,
    os::unix::{ffi::OsStrExt as _, fs::MetadataExt, io::AsRawFd as _},
    path::{Path, PathBuf},
    str,
    sync::Mutex,
    time::Duration,
};
use warp_core::safe_error;

use anyhow::{anyhow, bail, ensure, Context, Result};
use channel_versions::VersionInfo;
use nix::unistd::{fchown, getgid};
use nix::{errno::Errno, unistd::getuid};
use warp_core::macos::get_bundle_path;
use warpui::{AppContext, ModelContext, SingletonEntity};

use crate::{
    appearance::AppearanceManager,
    autoupdate::{AutoupdateStage, AutoupdateState},
    channel::{Channel, ChannelState},
    safe_info,
};

use super::{
    github, release_assets_directory_url, DownloadProgress, DownloadReady, ProgressCallback,
};

// Relative path to the directory containing old executables from before an autoupdate.
//
// TODO(vorporeal): This and relevant code should be deleted after auto-updates have been
//      storing the old executable in the user application data directory for a couple
//      releases.
const OLD_EXECUTABLE_PATH: &str = "Contents/MacOS/old";

// Name of the old executable file that was kept around during an autoupdate.
const OLD_EXECUTABLE_FILE_NAME: &str = "old";

// Tmp file name used to check if the user has the correct permissions for autoupdate.
const PERMISSIONS_TMP_FILE_NAME: &str = "permission_test";

fn old_executable_file_path() -> PathBuf {
    warp_core::paths::state_dir().join(OLD_EXECUTABLE_FILE_NAME)
}

/// Removes the old executable dir from the app bundle. This is necessary because after an
/// autoupdate deleting the running executable causes the pty to not start for a reason we don't
/// fully understand. This allows to clean up old executables when the app is first launched.
pub(super) fn remove_old_executable() -> Result<()> {
    // TODO(vorporeal): This code should be deleted after auto-updates have been
    //      storing the old executable in the user application data directory for
    //      a couple releases.
    log::info!("Removing old executable dir...");
    let old_executable_path = PathBuf::from(get_bundle_path()?).join(OLD_EXECUTABLE_PATH);
    if let Ok(metadata) = fs::metadata(&old_executable_path) {
        if metadata.is_dir() {
            fs::remove_dir_all(old_executable_path)?;
        }
    }

    log::info!("Removing old executable file...");
    let old_executable_file_path = old_executable_file_path();
    if let Ok(metadata) = fs::metadata(&old_executable_file_path) {
        if metadata.is_file() {
            fs::remove_file(old_executable_file_path)?;
        }
    }

    Ok(())
}

pub(super) fn manually_download_version(
    channel: &Channel,
    version_info: &VersionInfo,
    ctx: &mut AppContext,
) {
    let url = update_url(*channel, version_info.version.as_str());
    ctx.open_url(&url);
}

/// If the autoupdate state is ready, asynchronously apply the update and cleanup the autoupdate artifacts.
///
/// The completion callback is invoked with `Ok(Some(version))` if an update was applied, and `Ok(None)` if there was no update.
/// If there was an update, but applying it failed, it's invoked with `Err(err)`.
pub(super) fn apply_update_async<F>(app: &mut AppContext, callback: F)
where
    F: FnOnce(
            &mut AutoupdateState,
            Result<Option<VersionInfo>>,
            &mut ModelContext<AutoupdateState>,
        ) + Send
        + 'static,
{
    AutoupdateState::handle(app).update(app, |autoupdate_state, ctx| {
        match autoupdate_state.stage.clone() {
            AutoupdateStage::UpdateReady {
                new_version,
                update_id,
            }
            | AutoupdateStage::Updating {
                new_version,
                update_id,
            } => {
                let update_id_clone = update_id.clone();
                // Apply the update in a background thread.
                ctx.spawn(
                    async move {
                        let result =
                            apply_update(ChannelState::channel(), &new_version, &update_id)
                                .await
                                .map(|_| Some(new_version));
                        cleanup(&update_id).await;
                        result
                    },
                    move |autoupdate_state, result, ctx| {
                        if result.is_ok() {
                            // Reset app icon to previously selected app icon
                            AppearanceManager::as_ref(ctx).set_app_icon(ctx);
                        }
                        autoupdate_state.clear_downloaded_update(&update_id_clone, ctx);
                        callback(autoupdate_state, result, ctx);
                    },
                );
            }
            _ => {
                callback(autoupdate_state, Ok(None), ctx);
            }
        }
    })
}

/// The dmg that passed SHA-256 verification in [`oss_download_dmg`], together
/// with the digest it was checked against.
///
/// This exists because the download path and the install path used to name the
/// file two different ways: verification hashed `dmg_path()`, while
/// [`oss_open_installer`] handed `/usr/bin/open` whatever `find_latest_dmg`
/// turned up -- the newest `*.dmg` by mtime anywhere under
/// `cache_dir/autoupdate/`. In practice both resolve to the same file, and
/// steering them apart needs write access to the user's cache directory, so
/// this is defence in depth rather than a hole. Still: verify one file and
/// execute another is not a property worth keeping.
///
/// Keeping the digest as well as the path means the re-check below does not
/// depend on `github::cached_release()` still being populated at install time.
static VERIFIED_OSS_DMG: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

pub(super) fn relaunch() -> Result<()> {
    let channel = ChannelState::channel();

    // openWarp (Channel::Oss): there's no code signature, so RENAME_SWAP can't be
    // used to replace the bundle in place. Instead, call `/usr/bin/open <dmg>`
    // to have Finder pop up the standard mount window; the user drags it into
    // Applications to finish installing. `open -n bundle` isn't called here to
    // restart itself, because the current process has already requested
    // terminate during apply_update, and the UI already knows to wait for the
    // user to manually close and reopen. The dmg is likewise opened via Finder
    // only after the current process exits.
    if matches!(channel, Channel::Oss) {
        return oss_open_installer();
    }

    let bundle_path = PathBuf::from(get_bundle_path()?);

    // Wait for the current process to exit before launching the new version of
    // Zap, to avoid briefly showing multiple icons in the Dock. An intermediate
    // shell process polls the current PID here, and launches the new app once
    // the process has exited.
    //
    // Checks every 200ms whether the current process is still running; launches
    // the new version once it has exited.
    //
    // The shell command must be assembled carefully: `pid` comes from the
    // current process and is numeric, but the bundle path and environment
    // variable value must be shell-escaped to avoid injection via metacharacters
    // in the path.
    let pid = std::process::id();
    let quoted_bundle = shell_escape::escape(bundle_path.to_string_lossy());

    let mut open_args = format!(
        "/usr/bin/open -n {} --args {}",
        quoted_bundle,
        warp_cli::finish_update_flag(),
    );
    // When testing the local channel version JSON, let the newly launched
    // binary keep referencing the same file, so we can verify the changelog
    // display after auto-update.
    if let Ok(path) = env::var("WARP_CHANNEL_VERSIONS_PATH") {
        let quoted_path = shell_escape::escape(path.into());
        open_args.push_str(&format!(" --env WARP_CHANNEL_VERSIONS_PATH={quoted_path}"));
    }

    let relaunch_script =
        format!("while ps -p {pid} >/dev/null 2>&1; do sleep 0.2; done; {open_args}");

    log::info!("Executing relaunch command {relaunch_script:?}");
    blocking::Command::new("sh")
        .arg("-c")
        .arg(relaunch_script)
        .spawn()?;
    Ok(())
}

/// File the detached install script writes its refusal into, so a failure that
/// happens after this process is gone still has somewhere to land.
///
/// The parent's stderr is not that place. For a bundled `.app` it is not the
/// Phosphor log and usually not anything at all, and by the time the script
/// runs the process that owned it has exited. A file under the cache directory
/// outlives us, and [`check_and_report_update_errors`] picks it up on the next
/// launch.
const OSS_INSTALL_FAILURE_LOG: &str = "install-failure.log";

fn oss_autoupdate_dir() -> PathBuf {
    let mut dir = warp_core::paths::cache_dir();
    dir.push("autoupdate");
    dir
}

fn oss_install_failure_log() -> PathBuf {
    oss_autoupdate_dir().join(OSS_INSTALL_FAILURE_LOG)
}

/// Resolves the dmg [`oss_open_installer`] will hand to Finder, together with
/// the digest it must hash to.
///
/// Prefer the exact path this process verified. Only if that record is missing
/// -- it should not be, the download and the install happen in one process --
/// do we fall back to scanning cache_dir/autoupdate/ for the newest dmg, and
/// then the file has to earn its way through verification before it is opened.
/// The fallback writes its result back into [`VERIFIED_OSS_DMG`] so that the
/// scan and the `verify_oss_asset_sha256` call it just paid for happen once per
/// install rather than once per caller -- both
/// [`oss_verify_installer_before_relaunch`] and [`oss_open_installer`] resolve
/// through here, and they must in any case agree on which file they are talking
/// about.
///
/// A poisoned mutex is recovered rather than discarded: `Option<(PathBuf,
/// String)>` has no invariant a panicking writer could have broken, and `.ok()`
/// here would silently downgrade "we have a verified dmg" into "go scan the
/// cache directory".
///
/// Note there is deliberately no `path.exists()` filter on the record. That was
/// a second check-then-use, and it silently converted "the file we verified is
/// gone" -- which should never happen inside one process -- into the directory
/// scan below. If we hold a record we use it, and the digest checks report any
/// problem with it.
fn resolve_oss_dmg() -> Result<(PathBuf, String)> {
    let verified = VERIFIED_OSS_DMG
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    let (dmg, expected) = match verified {
        Some((path, expected)) => (path, expected),
        None => {
            let autoupdate_dir = oss_autoupdate_dir();
            let found = find_latest_dmg(&autoupdate_dir).ok_or_else(|| {
                anyhow!("Phosphor: could not find a downloaded dmg (directory: {autoupdate_dir:?})")
            })?;
            log::warn!(
                "Phosphor: no verified dmg on record, re-verifying the newest one found on disk ({found:?})"
            );
            let expected =
                super::verify_oss_asset_sha256(&found, &dmg_name(ChannelState::channel()))?;
            *VERIFIED_OSS_DMG
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                Some((found.clone(), expected.clone()));
            (found, expected)
        }
    };

    ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "Phosphor: refusing to open {dmg:?}: recorded digest {expected:?} is not a SHA-256 hex string"
    );

    Ok((dmg, expected))
}

/// Re-hashes the installer dmg in *this* process, while the app is still
/// running and still has a UI.
///
/// # Why this exists as well as the in-script check
///
/// The digest comparison in [`oss_open_installer`] is emitted into the detached
/// script because that is the only place it can sit adjacent to the use (see
/// that function). Adjacency is worth having, but it cost every reporting
/// channel: the script runs after `terminate_app`, so its verdict has no
/// process to return an exit status to, no `AutoupdateState` to move, and no
/// stderr anybody reads. A mismatch would have quit the app for an update and
/// then done nothing, visibly or in the log.
///
/// So the verdict is computed twice, for two different jobs:
///
/// * here, *before* `terminate_app` is called at all, where a failure returns
///   `Err` through `finalize_update` into `relaunch_failed` -- the app does not
///   quit, and the workspace shows the error banner with the "Update Phosphor
///   manually" button. This catches every mismatch that already exists when the
///   user clicks install, which is all of them except a live swap;
/// * in the script, microseconds before `exec open`, which is the only check
///   that covers the teardown window. Its refusal is recorded in
///   [`oss_install_failure_log`] and reported on the next launch.
///
/// Neither replaces the other: this one can report but is not adjacent, that
/// one is adjacent but can only report to a file.
fn oss_verify_installer_before_relaunch() -> Result<()> {
    let (dmg, expected) = resolve_oss_dmg()?;
    let actual = super::sha256_file(&dmg)
        .with_context(|| format!("Phosphor: could not hash the installer dmg {dmg:?}"))?;
    ensure!(
        actual == expected,
        "Phosphor: refusing to open {dmg:?}: sha256 is {actual}, expected {expected}"
    );
    log::info!("Phosphor: re-verified installer dmg {dmg:?} before requesting termination");
    Ok(())
}

/// Runs [`oss_verify_installer_before_relaunch`] off the main thread and hands
/// the verdict back to `callback` on it.
///
/// Hashing a few hundred megabytes is not something to do on the UI thread, and
/// `finalize_update`'s contract already allows the deferred steps to be async.
pub(super) fn oss_verify_installer_async<F>(app: &mut AppContext, callback: F)
where
    F: FnOnce(&mut AutoupdateState, Result<()>, &mut ModelContext<AutoupdateState>)
        + Send
        + 'static,
{
    AutoupdateState::handle(app).update(app, |_autoupdate_state, ctx| {
        ctx.spawn(
            async move { oss_verify_installer_before_relaunch() },
            move |autoupdate_state, result, ctx| {
                callback(autoupdate_state, result, ctx);
            },
        );
    });
}

/// Reports a refusal recorded by the detached install script on a previous run.
///
/// The script cannot reach `AutoupdateState` -- it outlives the process that
/// owned it -- so it appends its reason to [`oss_install_failure_log`] instead.
/// `oss_open_installer` removes that file immediately before spawning, so
/// anything found here belongs to the most recent install attempt and cannot be
/// a stale report from an earlier one. The file is consumed once read, for the
/// same reason.
///
/// This is a weaker channel than the banner the pre-flight check raises: it
/// arrives one launch late and lands in the log and the error reporter rather
/// than on screen. It is what is reachable from a process that no longer
/// exists, and it is strictly better than the previous behaviour, which was to
/// write to a closed stderr.
pub(super) fn check_and_report_update_errors(_ctx: &mut AppContext) {
    let path = oss_install_failure_log();
    let reason = match fs::read_to_string(&path) {
        Ok(reason) => reason,
        // The overwhelmingly common case: the last install did not refuse.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            log::warn!("Phosphor: could not read {path:?}: {e:#}");
            return;
        }
    };
    if let Err(e) = fs::remove_file(&path) {
        log::warn!("Phosphor: could not remove {path:?} after reporting it: {e:#}");
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return;
    }
    crate::report_error!(&anyhow!(
        "Phosphor: the previous update was refused after shutdown and no installer was opened: {reason}"
    ));
}

/// OSS macOS install entry point: triggers Finder's standard mount of the dmg
/// via `/usr/bin/open <dmg>` once the current process exits.
///
/// # Why the digest check lives in the spawned script
///
/// `open` cannot run now. Finder must not mount the image until this process is
/// gone, so the work is handed to a detached `sh` that polls for our pid to
/// disappear. Anything this function checks is therefore checked an app
/// shutdown -- plus up to one 200 ms poll interval -- before the file is used,
/// on a path under `cache_dir()` that any process running as this user can
/// replace in the meantime. A previous version of this code hashed the file
/// here and called that "no longer verifies one file and executes another";
/// that claim was simply false, because the two events were not adjacent.
///
/// So the comparison is emitted into the script instead, between the wait loop
/// and the `exec`. The residual window is now the microseconds between
/// `shasum` returning and `exec open`, rather than a whole app teardown. It is
/// not zero -- macOS has no "open this file descriptor" primitive for `open(1)`
/// to close it completely -- but it is no longer a window an attacker can
/// arrive during.
///
/// Three supporting properties:
/// * A missing `shasum`, an unreadable file or any mismatch makes the script
///   exit non-zero *before* `open`, so every failure mode is fail-closed.
/// * The script's verdict is not thrown away. Nothing waits on this child --
///   the app is already terminating, which is the whole reason the script
///   exists -- so its exit status and its stderr both die with the parent. It
///   therefore writes its reason to [`oss_install_failure_log`], which
///   [`check_and_report_update_errors`] reports on the next launch, and the
///   same comparison is run in-process by
///   [`oss_verify_installer_before_relaunch`] *before* `terminate_app`, where a
///   failure can still raise a banner. See that function for the split.
/// * The alternatives considered were: holding an fd across the gap (useless,
///   `open(1)` takes a path); staging into a directory only this process can
///   write (does not help against a same-uid attacker, which is the realistic
///   one on macOS); and copying to a fresh temp path after hashing (a
///   several-hundred-MB copy that still leaves the copy sitting user-writable
///   across the same teardown). Re-checking adjacent to the use is the only one
///   that removes the window rather than moving it.
fn oss_open_installer() -> Result<()> {
    let (dmg, expected) = resolve_oss_dmg()?;

    log::info!("Phosphor: preparing to open installer dmg {dmg:?}");

    // Clear any refusal left by a previous attempt, so that whatever is in the
    // file at the next launch describes *this* one. The directory is created if
    // missing: the script's `>>"$log"` is deliberately silent about its own
    // failures (it has nowhere to complain to), so an absent directory would
    // turn the whole reporting channel into a no-op without saying so.
    let failure_log = oss_install_failure_log();
    if let Some(parent) = failure_log.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            log::warn!("Phosphor: could not create {parent:?} for the install failure log: {e:#}");
        }
    }
    match fs::remove_file(&failure_log) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("Phosphor: could not clear {failure_log:?}: {e:#}"),
    }

    let pid = std::process::id();
    let script = format!(
        "while ps -p {pid} >/dev/null 2>&1; do sleep 0.2; done; {}",
        oss_install_script_body(&dmg, &expected, &failure_log)
    );
    log::info!("Executing OSS install command {script:?}");
    blocking::Command::new("sh").arg("-c").arg(script).spawn()?;
    Ok(())
}

/// Everything the install script does once this process is gone: check the
/// digest, and `exec open` only if it matches.
///
/// # Quoting
///
/// Every value that comes from outside the program is bound to a shell variable
/// once, at the top, and referenced as `"$dmg"` / `"$expected"` / `"$log"`
/// thereafter. That is not stylistic. `shell_escape::escape` produces a
/// *single*-quoted word, which is a correct assignment right-hand side and a
/// correct standalone argument, but is not safe to paste inside a
/// double-quoted string: there the single quotes are ordinary characters and
/// `$(...)`, backticks and `\` all still expand. The refusal message used to
/// interpolate the escaped path exactly that way, so a dmg named
/// `$(...)something.dmg` executed its own name -- in the branch whose entire
/// job is to report that the file is not trusted. The name is
/// attacker-influenceable: `find_latest_dmg` takes the newest `*.dmg` under
/// `cache_dir()/autoupdate/` verbatim.
///
/// Expanding a variable's *value* does not re-expand it, in any POSIX shell, so
/// the variable form has no such reading. `sh`, `dash`, `bash` and `zsh` were
/// all checked.
///
/// `shasum` is part of the macOS base install (/usr/bin/shasum); if it is
/// absent the `command -v` test fails and we never reach `open`. `open` is
/// non-blocking by default; once Finder has the dmg it mounts it and shows the
/// mount window, and the user drags it into Applications to finish the upgrade.
fn oss_install_script_body(dmg: &Path, expected: &str, failure_log: &Path) -> String {
    let quoted_dmg = shell_escape::escape(dmg.to_string_lossy());
    let quoted_expected = shell_escape::escape(expected.to_owned().into());
    let quoted_log = shell_escape::escape(failure_log.to_string_lossy());
    format!(
        "log={quoted_log}; dmg={quoted_dmg}; expected={quoted_expected}; \
         command -v /usr/bin/shasum >/dev/null 2>&1 || {{ echo 'Phosphor: /usr/bin/shasum missing, refusing to open the installer' >>\"$log\" 2>/dev/null; exit 1; }}; \
         actual=$(/usr/bin/shasum -a 256 \"$dmg\" 2>/dev/null | /usr/bin/cut -d' ' -f1); \
         if [ \"$actual\" != \"$expected\" ]; then \
           echo \"refusing to open $dmg: sha256 is $actual, expected $expected\" >>\"$log\" 2>/dev/null; \
           exit 1; \
         fi; \
         exec /usr/bin/open \"$dmg\""
    )
}

/// Finds the most recently downloaded dmg under the `autoupdate/` directory.
/// OSS only downloads dmg files, no other files, so taking the latest by file
/// mtime is sufficient. Returns None if there's currently no dmg available
/// (an abnormal situation).
fn find_latest_dmg(autoupdate_dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    let read_dir = fs::read_dir(autoupdate_dir).ok()?;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(inner) = fs::read_dir(&path) else {
            continue;
        };
        for inner_entry in inner.flatten() {
            let inner_path = inner_entry.path();
            if inner_path
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| !e.eq_ignore_ascii_case("dmg"))
            {
                continue;
            }
            let Ok(meta) = fs::metadata(&inner_path) else {
                continue;
            };
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            if newest.as_ref().is_none_or(|(_, t)| mtime > *t) {
                newest = Some((inner_path, mtime));
            }
        }
    }
    newest.map(|(p, _)| p)
}

pub async fn cleanup(update_id: &str) {
    let download_dir = get_download_dir(update_id);
    if download_dir.exists() {
        log::info!("Cleaning up download dir {:?}", &download_dir);
        if let Err(e) = async_fs::remove_dir_all(&download_dir).await {
            safe_error!(
                safe: ("Error cleaning up download dir: {e:?}"),
                full: ("Error cleaning up download dir {:?}: {:?}", &download_dir, e)
            );
        }
    }
}

/// Clean up all autoupdate directories except the specified one.
/// This helps prevent accumulation of old update directories from failed downloads,
/// race conditions, or incomplete cleanups.
pub async fn cleanup_all_except(preserve_update_id: Option<&str>) {
    let mut autoupdate_dir = warp_core::paths::cache_dir();
    autoupdate_dir.push("autoupdate");

    if !autoupdate_dir.exists() {
        return;
    }

    log::debug!("Cleaning up all autoupdate directories except {preserve_update_id:?}");

    let mut entries = match async_fs::read_dir(&autoupdate_dir).await {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Could not read autoupdate directory {autoupdate_dir:?}: {e:?}");
            return;
        }
    };

    while let Some(entry) = entries.next().await {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                log::warn!("Error reading autoupdate directory entry: {e:?}");
                continue;
            }
        };

        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Skip the directory we want to preserve
        if let Some(preserve_id) = preserve_update_id {
            if file_name == preserve_id {
                log::debug!("Preserving autoupdate directory: {path:?}");
                continue;
            }
        }

        let metadata = match async_fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                log::warn!("Could not get metadata for {path:?}: {e:?}");
                continue;
            }
        };

        if metadata.is_dir() {
            log::debug!("Removing old autoupdate directory: {path:?}");
            if let Err(e) = async_fs::remove_dir_all(&path).await {
                log::warn!("Failed to remove autoupdate directory {path:?}: {e:?}");
            }
        }
    }
}

/// Determines if the user needs authorization in order to update Zap.
async fn needs_authorization(bundle_path: &Path) -> Result<bool> {
    // For the bundle path itself, check permissions without creating a test file so as to not
    // interfere with code signing.
    let bundle_dir_writable = permissions::is_writable(bundle_path)?;
    if !bundle_dir_writable {
        log::info!("App location is not writable, needs authorization");
        return Ok(true);
    } else {
        log::info!("App location is writable");
    }

    if let Some(bundle_parent_path) = bundle_path.parent() {
        if !is_directory_writable(bundle_parent_path).await? {
            log::info!("App parent location is not writable, needs authorization");
            return Ok(true);
        } else {
            log::info!("App parent location is writable");
        }
    }

    Ok(false)
}

/// Determines if a directory is writable as part of an update. This means:
/// * Zap can create files in the directory
/// * Zap can modify the permissions of created files
async fn is_directory_writable(directory: &Path) -> Result<bool> {
    // Just because we have writability access does not mean we can set the correct owner/group.
    // Test if we can set the owner/group on a temporarily created file. If we can, then we can
    // probably perform an update without authorization.
    let tmp_file_name = directory.join(PERMISSIONS_TMP_FILE_NAME);

    safe_info!(
        safe: ("Writing to a tmp file to determine if permissions are correct"),
        full: ("Writing to a tmp file to determine if permissions are correct in {}", directory.display())
    );

    let needs_authorization = match async_fs::File::create(&tmp_file_name).await {
        Ok(file) => {
            let fchown_result = fchown(file.as_raw_fd(), Some(getuid()), Some(getgid()));
            if let Err(err) = &fchown_result {
                log::warn!("Could not set permissions on tmp file: {err:#}");
            }

            // Only remove the tmp file if it was created - otherwise, we'll mask permission
            // errors.
            async_fs::remove_file(&tmp_file_name).await?;
            fchown_result.is_ok()
        }
        Err(e) => {
            // Obvious indicator we may need authorization.
            log::warn!("Could not create tmp file: {e:#}");
            false
        }
    };

    Ok(needs_authorization)
}

/// Verifies that the staged bundle path has a valid macOS code signature, and that its
/// team identifier matches Zap's team identifier.
async fn verify_code_signature(component: &str, path: &Path) -> Result<()> {
    // Verify the signature of the staged update bundle with team identifier
    let codesign_verify_output = Command::new("/usr/bin/codesign")
        .arg("-v")
        .arg(format!(
            "-R=certificate leaf[subject.OU] = \"{}\"",
            warp_core::macos::APPLE_TEAM_ID
        ))
        .arg(path)
        .output()
        .await?;
    ensure!(
        codesign_verify_output.status.success(),
        "Failed to verify code signature for {component} with team identifier: {codesign_verify_output:?}"
    );

    safe_info!(
        safe: ("Code signature is valid for {component}"),
        full: ("Code signature is valid for {}", path.display())
    );

    Ok(())
}

pub(super) async fn download_update_and_cleanup(
    version_info: &VersionInfo,
    update_id: &str,
    last_successful_update_id: Option<&str>,
    client: &http_client::Client,
    on_progress: ProgressCallback,
) -> Result<DownloadReady> {
    let channel = ChannelState::channel();

    // openWarp (Channel::Oss): there's no Apple Developer ID signature, so the
    // official download_and_extract_binary (mount + cp + codesign verify +
    // RENAME_SWAP) can't be used. The OSS path only stream-downloads the dmg
    // into cache_dir/autoupdate/<id>/; on apply, `relaunch()` uses `open <dmg>`
    // to have Finder pop up the standard mount window, and the user drags it
    // into Applications.
    let result = if matches!(channel, Channel::Oss) {
        oss_download_dmg(channel, version_info, update_id, client, on_progress).await
    } else {
        download_and_extract_binary(channel, version_info, update_id, client, on_progress).await
    };
    if result.is_err() {
        cleanup_all_except(last_successful_update_id).await;
    }
    result
}

/// OSS-only download: stream the dmg to disk at
/// `cache_dir/autoupdate/<update_id>/<dmg>`, without mounting or verifying a
/// code signature. Returns `DownloadReady::Yes` to indicate the installer is
/// ready; the layer above switches to `UpdateReady` and waits for the user to
/// click "Install now" to trigger `relaunch()`.
async fn oss_download_dmg(
    channel: Channel,
    version_info: &VersionInfo,
    update_id: &str,
    client: &http_client::Client,
    on_progress: ProgressCallback,
) -> Result<DownloadReady> {
    log::info!(
        "Phosphor: downloading update dmg, version {} on channel {channel}",
        &version_info.version
    );

    let download_dir = get_download_dir(update_id);
    async_fs::create_dir_all(&download_dir).await?;

    let dmg_path_buf = download_dmg(&channel, version_info, update_id, client, on_progress).await?;

    // Deliberately skip hdiutil mount / verify_code_signature: OSS has no Apple
    // codesign, and there's no need to copy the .app into the current bundle
    // anyway. The dmg itself is the thing the user is meant to "open".
    //
    // What replaces it is weaker, and worth naming precisely: the SHA-256 comes
    // from the same api.github.com response that gave us the download URL, so it
    // proves the bytes on disk are the bytes GitHub's API described. That covers
    // a truncated download or a CDN edge serving something else. It is not
    // supply-chain integrity -- nothing here is signed by a key this binary
    // holds, so a forged API response carries a matching forged digest.
    let asset_name = dmg_name(channel);
    let digest = match super::verify_oss_asset_sha256(&dmg_path_buf, &asset_name) {
        Ok(digest) => digest,
        Err(e) => {
            // Immediately delete the downloaded file on verification failure, so
            // the user doesn't click "Install" and open a corrupted dmg.
            let _ = async_fs::remove_file(&dmg_path_buf).await;
            return Err(e);
        }
    };

    // Hand the install path the file we actually checked, instead of letting it
    // re-discover a dmg by mtime.
    //
    // A poisoned mutex is recovered rather than skipped, for the same reason
    // `resolve_oss_dmg` recovers on the read side: `if let Ok(..)` here would
    // drop the record on the floor and send the install path back to
    // `find_latest_dmg`, permanently, because poisoning never clears.
    *VERIFIED_OSS_DMG
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((dmg_path_buf, digest));
    Ok(DownloadReady::Yes)
}

/// Apply the downloaded update.
///
/// This is async and should be run in a background task.
async fn apply_update(channel: Channel, version_info: &VersionInfo, update_id: &str) -> Result<()> {
    let update_start = Instant::now();

    let bundle_path = PathBuf::from(get_bundle_path()?);
    let bundle_parent_path = bundle_path
        .parent()
        .ok_or_else(|| anyhow!("Could not get parent directory of application bundle"))?;

    // Double-check that we have permissions to apply the update.
    if !permissions::is_writable(&bundle_path)? {
        bail!("App location is not writable, cannot apply update");
    }
    if !is_directory_writable(bundle_parent_path).await? {
        bail!("App parent location is not writable, cannot apply update");
    }

    // Read a file out of the old bundle to ensure that we've triggered macOS' directory
    // permissions checks.
    let old_info_plist = bundle_path.join("Contents/Info.plist");
    if async_fs::File::open(&old_info_plist).await.is_err() {
        bail!("App location is not readable, cannot apply update");
    }

    let dmg_path = dmg_path(&channel, version_info, update_id);
    let temp_app_path = temporary_target_path(channel, version_info, &dmg_path)?;

    let staged_bundle =
        StagedBundle::for_bundle_path(channel, version_info, temp_app_path, &bundle_path).await?;

    // Copy permissions to new app
    let bundle_metadata = async_fs::metadata(&bundle_path).await?;
    async_fs::set_permissions(&staged_bundle.path, bundle_metadata.permissions()).await?;

    // Verify that the new version actually exists before proceeding
    let executable_path_buf = staged_bundle.path.join(executable_path(channel));
    if !executable_path_buf.exists() {
        bail!(
            "New executable does not exist at path: {:?}",
            executable_path_buf
        );
    }

    // Atomically rename the new app to have the same name as the old one.
    log::info!("Renaming new app to original app name");
    let from = CString::new(staged_bundle.path.as_os_str().as_bytes())?;
    let to = CString::new(bundle_path.as_os_str().as_bytes())?;

    Errno::result(unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_SWAP) })
        .context("Error swapping old and new app bundles")?;

    // Move the current running executable into a temporary directory so we can delete the
    // rest of the old bundle without removing the running executable (since removing it
    // causes the `fork` syscall to fail).
    let executable_temp_file = old_executable_file_path();
    if async_fs::metadata(executable_temp_file.as_path())
        .await
        .is_ok()
    {
        // If we performed this process already but didn't relaunch Zap, the old executable will
        // still be located in the user application data directory.  In that case, leave it there.
        log::info!("Already autoupdated without relaunching; ignoring executable from old bundle");
    } else {
        // Compute the location of the old executable (which, after the swap of the app contents,
        // is located in the "new app" directory).
        let new_app_executable_path = staged_bundle.path.join(executable_path(channel));

        log::info!(
            "Moving old executable at path {new_app_executable_path:?} into user application data dir at path {executable_temp_file:?}"
        );
        let mv_output = Command::new("mv")
            .arg(new_app_executable_path)
            .arg(executable_temp_file)
            .output()
            .await?;

        ensure!(
            mv_output.status.success(),
            "Failed to move old executable: {mv_output:?}"
        );
    }

    log::info!("Setting installed version to {:?}", &version_info);
    log::info!("Applied update in {:?}", update_start.elapsed());

    Ok(())
}

/// The staged app bundle that we're about to install. It's copied out of the `.dmg` file into a
/// temporary location.
struct StagedBundle {
    /// Path to the on-disk temporary bundle.
    path: PathBuf,
    /// Whether or not the temporary bundle was copied into the same directory as the existing app.
    /// This is only necessary if `$TMPDIR` and the app are on different filesystems.
    in_app_directory: bool,
}

impl StagedBundle {
    async fn for_bundle_path(
        channel: Channel,
        version_info: &VersionInfo,
        temp_app_path: PathBuf,
        bundle_path: &Path,
    ) -> Result<Self> {
        let temp_device_id = async_fs::metadata(&temp_app_path)
            .await
            .context("Could not get metadata for temporary app bundle")?
            .dev();
        let bundle_device_id = async_fs::metadata(bundle_path)
            .await
            .context("Could not get metadata for app bundle")?
            .dev();

        if temp_device_id == bundle_device_id {
            // The old and new app bundles are on the same filesystem (this is the expected case).
            Ok(Self {
                path: temp_app_path,
                in_app_directory: false,
            })
        } else {
            let bundle_parent_path = bundle_path
                .parent()
                .ok_or_else(|| anyhow!("Could not get parent directory of application bundle"))?;
            log::info!("Copying app contents from {temp_app_path:?} to {bundle_parent_path:?}");

            let cp_output = Command::new("cp")
                // Recursively copy the directory, preserving symlinks.
                .arg("-R")
                // Overwrite files at the destination.
                .arg("-f")
                .arg(&temp_app_path)
                .arg(bundle_parent_path)
                .output()
                .await?;

            ensure!(
                cp_output.status.success(),
                "Failed to copy app contents from temporary directory into bundle directory: {cp_output:?}"
            );

            Ok(Self {
                path: bundle_parent_path.join(versioned_app_name(channel, &version_info.version)),
                in_app_directory: true,
            })
        }
    }
}

impl Drop for StagedBundle {
    fn drop(&mut self) {
        // Clean up in the destructor so that it happens even if the installation errors.
        // If we used the original temporary app bundle, it'll get removed by the final cleanup
        // step, along with the dmg.
        if self.in_app_directory {
            log::info!("Removing temporary app bundle");
            if let Err(err) = fs::remove_dir_all(&self.path) {
                log::error!("Failed to remove temporary bundle: {err:#}");
            }
        }
    }
}

async fn download_and_extract_binary(
    channel: Channel,
    version_info: &VersionInfo,
    update_id: &str,
    client: &http_client::Client,
    on_progress: ProgressCallback,
) -> Result<DownloadReady> {
    let bundle_path = PathBuf::from(get_bundle_path()?);
    let needs_authorization = needs_authorization(bundle_path.as_path())
        .await
        .unwrap_or(true);
    if needs_authorization {
        return Ok(DownloadReady::NeedsAuthorization);
    }

    log::info!(
        "Downloading update, version {} on channel {channel}",
        &version_info.version,
    );

    let download_dir = get_download_dir(update_id);
    log::info!("Creating download dir {:?}", &download_dir);
    async_fs::create_dir_all(&download_dir).await?;

    let dmg_path = download_dmg(&channel, version_info, update_id, client, on_progress).await?;

    // Mount the downloaded dmg so we can copy out the binary.
    let mountpoint = mount_dmg(&dmg_path, update_id).await?;

    let target = temporary_target_path(channel, version_info, &dmg_path)?;
    // Copy the binary into the temporary directory where we downloaded the dmg.
    copy_app_from_dmg(&channel, &mountpoint, &target).await?;

    // Unmount the dmg once we no longer need it. This prevents lingering images from unapplied
    // updates.
    if let Err(err) = unmount_dmg(mountpoint).await {
        let err = err.context("Error unmounting dmg for update");
        crate::report_error!(&err);
    }

    // Ensure that the new app we just downloaded has both integrity (e.g. no corrupted files)
    // and validity (it was signed by us).
    // Store the executable path in a variable to prevent temporary value issues.
    let executable_path_buf = target.join(executable_path(channel));
    let verification_start = Instant::now();
    future::try_zip(
        verify_code_signature("bundle", &target),
        verify_code_signature("executable", executable_path_buf.as_path()),
    )
    .await?;

    log::info!(
        "Verified new app code signature in {:?}",
        verification_start.elapsed()
    );

    Ok(DownloadReady::Yes)
}

async fn unmount_dmg(mountpoint: PathBuf) -> Result<()> {
    let mut hdiutil_cmd = Command::new("/usr/bin/hdiutil");
    hdiutil_cmd.arg("detach");
    hdiutil_cmd.arg(&mountpoint);
    hdiutil_cmd.arg("-force");

    log::info!("Attempting to detach dmg with command \"{hdiutil_cmd:?}\"");

    let output = hdiutil_cmd.output().await?;

    ensure!(output.status.success(), "Failed to detach dmg: {output:?}");
    log::info!("hdiutil detach succeeded: {output:?}");
    Ok(())
}

async fn copy_app_from_dmg(channel: &Channel, mountpoint: &Path, target: &Path) -> Result<()> {
    let mounted_app_path = mountpoint.join(app_name(*channel));

    log::info!("Copying dmg contents from {mounted_app_path:?} to {target:?}");

    let cp_output = Command::new("cp")
        // Recursively copy the directory, preserving symlinks.
        .arg("-R")
        .arg(mounted_app_path)
        .arg(target)
        .output()
        .await?;

    ensure!(
        cp_output.status.success(),
        "Failed to copy app out of mounted dmg: {cp_output:?}"
    );

    Ok(())
}

// 10 minutes
const DMG_TIMEOUT_S: u64 = 600;

/// The temporary path for downloading the new dmg into.
fn dmg_path(channel: &Channel, version_info: &VersionInfo, update_id: &str) -> PathBuf {
    let mut dir = get_download_dir(update_id);
    let file_name = format!(
        "{}.{}.dmg",
        &version_info.version,
        app_name_prefix(*channel)
    );
    dir.push(file_name);
    dir
}

/// The temporary path for placing our downloaded app binary.
fn temporary_target_path(
    channel: Channel,
    version_info: &VersionInfo,
    dmg_path: &Path,
) -> Result<PathBuf> {
    Ok(dmg_path
        .parent()
        .ok_or_else(|| anyhow!("Could not get parent directory of downloaded DMG"))?
        .join(versioned_app_name(channel, &version_info.version)))
}

async fn download_dmg(
    channel: &Channel,
    version_info: &VersionInfo,
    update_id: &str,
    client: &http_client::Client,
    on_progress: ProgressCallback,
) -> Result<PathBuf> {
    let update_url = update_url(*channel, &version_info.version);
    log::info!("Fetching new dmg at {update_url}");
    let res = client
        .get(&update_url)
        .timeout(Duration::from_secs(DMG_TIMEOUT_S))
        .send()
        .await?
        .error_for_status()?;
    // http_client::Response has no content_length(); it can only be read from headers.
    let total = res
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    let dmg_file = dmg_path(channel, version_info, update_id);

    // Report 0/total so the UI can immediately render the progress bar; each subsequent chunk write is then throttled before reporting.
    on_progress(DownloadProgress {
        downloaded: 0,
        total,
    });

    let mut file = async_fs::File::create(&dmg_file).await?;
    let mut downloaded: u64 = 0;
    // Throttle: don't report on every chunk (reqwest chunks can be quite small,
    // which would spam the UI with redraws). Only push once per 64 KiB
    // accumulated or 250ms elapsed; the last one is force-flushed outside the loop.
    let mut last_reported = 0u64;
    let mut last_reported_at = Instant::now();
    const REPORT_BYTES_THRESHOLD: u64 = 64 * 1024;
    const REPORT_TIME_THRESHOLD: Duration = Duration::from_millis(250);

    use futures_lite::io::AsyncWriteExt as _;
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
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
    file.sync_data().await?;

    log::info!("Wrote DMG to tempfile at {:?}", &dmg_file);
    Ok(dmg_file)
}

fn get_download_dir(update_id: &str) -> PathBuf {
    let mut dir = warp_core::paths::cache_dir();
    dir.push("autoupdate");
    dir.push(update_id);
    dir
}

fn get_mountpoint(update_id: &str) -> PathBuf {
    let mut volume = PathBuf::from("/Volumes");
    volume.push(update_id);
    volume
}

async fn mount_dmg(dmg_dir: &Path, update_id: &str) -> Result<PathBuf> {
    let volume = get_mountpoint(update_id);
    let mut hdiutil_cmd = Command::new("/usr/bin/hdiutil");
    hdiutil_cmd.args(["attach", "-mountpoint"]);
    hdiutil_cmd.arg(&volume);
    // Explanation of flags:
    // -nobrowse: Do not show the Zap DMG in Finder or similar apps.
    // -noautoopen: Do not open the Zap DMG in Finder.
    // -readonly: For safety, we mount read-only since there's no need to modify the new app version.
    // -autofsck: Ensure that the DMG contents are verified. This is on by default for quarantined images, but macOS
    //    doesn't necessarily recognize our download as such.
    hdiutil_cmd.args(["-nobrowse", "-noautoopen", "-readonly", "-autofsck"]);
    hdiutil_cmd.arg(dmg_dir);

    log::info!("Attempting to mount dmg with command \"{hdiutil_cmd:?}\"");

    let output = hdiutil_cmd.output().await?;

    ensure!(output.status.success(), "Failed to mount dmg: {output:?}");

    log::info!("hdiutil mount succeeded");
    Ok(volume)
}

fn update_url(channel: Channel, version: &str) -> String {
    let asset = dmg_name(channel);
    if matches!(channel, Channel::Oss) {
        // OSS uses GitHub Releases: prefer the real browser_download_url from
        // fetch_latest_release's cache (in case the repo gets redirected / the
        // asset gets renamed). When the cache is empty, assemble a standard
        // `releases/download/<tag>/<asset>` fallback URL, with the tag built
        // directly from VersionInfo.version plus a `v` prefix (VersionInfo has
        // already trimmed the `v`).
        if let Some(release) = github::cached_release() {
            if let Some(found) = release.find_asset(&asset) {
                return found.browser_download_url.clone();
            }
            log::warn!(
                "Phosphor: cached release tag {} has no asset named {asset}, falling back to the tag URL",
                release.tag_name
            );
        }
        return format!(
            "https://github.com/jwp2987/phosphor/releases/download/v{version}/{asset}"
        );
    }
    format!(
        "{}/{}",
        release_assets_directory_url(channel, version),
        asset
    )
}

fn app_name(channel: Channel) -> String {
    format!("{}.app", app_name_prefix(channel))
}

fn versioned_app_name(channel: Channel, version: &str) -> String {
    format!("{}({}).app", app_name_prefix(channel), version)
}

fn dmg_name(channel: Channel) -> String {
    // If the user is on an Apple Silicon Mac, download an arm64-only bundle.
    let is_arm64 = command::blocking::Command::new("uname")
        .arg("-m")
        .output()
        .is_ok_and(|output| output.stdout.starts_with(b"arm64"));

    // openWarp GitHub Release asset names are fixed as `Phosphor-arm64.dmg` /
    // `Phosphor-intel.dmg` (from script/macos/bundle's WARP_APP_NAME +
    // --dmg-name-suffix), which doesn't match `app_name_prefix("phosphor-oss")`.
    // This is hardcoded only for OSS and doesn't affect official channels'
    // universal naming.
    if matches!(channel, Channel::Oss) {
        return if is_arm64 {
            "Phosphor-arm64.dmg".to_string()
        } else {
            "Phosphor-intel.dmg".to_string()
        };
    }

    if is_arm64 {
        return format!("{}-arm64.dmg", app_name_prefix(channel));
    }

    // Otherwise, download a universal bundle.
    format!("{}.dmg", app_name_prefix(channel))
}

fn app_name_prefix(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "Zap",
        Channel::Preview => "WarpPreview",
        Channel::Local => "warp",
        Channel::Integration => "integration",
        Channel::Dev => "WarpDev",
        Channel::Oss => "phosphor-oss",
    }
}

/// The name of the executable inside `Contents/MacOS`.
///
/// For OSS this is the Cargo `[[bin]]` target name (and so the bundle's
/// `CFBundleExecutable`), not `Channel::cli_command_name()`.
fn executable_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "stable",
        Channel::Preview => "preview",
        Channel::Local => "warp",
        Channel::Integration => "integration",
        Channel::Dev => "dev",
        Channel::Oss => "phosphor-oss",
    }
}

fn executable_path(channel: Channel) -> String {
    if ChannelState::is_release_bundle() {
        format!("Contents/MacOS/{}", executable_name(channel))
    } else {
        executable_name(channel).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal branch of the install script must not execute the dmg's own
    /// file name.
    ///
    /// `shell_escape::escape` single-quotes, and the refusal message used to
    /// interpolate that single-quoted word inside a *double*-quoted `echo`,
    /// where the quotes are literal and `$(...)`, backticks and `\` all still
    /// expand. The file name reaches us from `find_latest_dmg`, which takes the
    /// newest `*.dmg` under `cache_dir()/autoupdate/` verbatim, so this ran
    /// attacker-chosen text in the one branch whose job is to say that the file
    /// is not to be trusted.
    #[test]
    fn refusal_branch_does_not_execute_the_dmg_name() {
        let dir = std::env::temp_dir().join(format!(
            "phosphor-oss-install-script-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        // A name a same-uid attacker can create in the cache directory. Both
        // substitution forms, and no `/` so it stays a single file name.
        let dmg = dir.join("$(touch executed)`touch backtick`evil.dmg");
        fs::write(&dmg, b"not the expected bytes").expect("write dmg");
        let log = dir.join(OSS_INSTALL_FAILURE_LOG);

        // A digest the file cannot hash to, so the refusal branch is the one
        // that runs.
        let body = oss_install_script_body(&dmg, &"a".repeat(64), &log);
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&body)
            // The payloads write relative names, so they would land here.
            .current_dir(&dir)
            .status()
            .expect("run install script body");

        assert!(!status.success(), "a digest mismatch must not reach `open`");
        assert!(
            !dir.join("executed").exists() && !dir.join("backtick").exists(),
            "the dmg's file name was executed by the branch reporting it: {body}"
        );

        // ... and the refusal is recorded somewhere that outlives the parent,
        // naming the path literally rather than having expanded it.
        let recorded = fs::read_to_string(&log).expect("the refusal must be written to the log");
        assert!(
            recorded.contains(&*dmg.to_string_lossy()),
            "the refusal should name the file it refused, got {recorded:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// The success path still `exec`s `open` on the same variable the digest
    /// was checked against -- the adjacency the script exists for.
    #[test]
    fn script_execs_open_on_the_verified_variable() {
        let body = oss_install_script_body(
            Path::new("/tmp/Phosphor.dmg"),
            &"b".repeat(64),
            Path::new("/tmp/install-failure.log"),
        );
        assert!(body.trim_end().ends_with("exec /usr/bin/open \"$dmg\""));
        assert!(body.contains("actual=$(/usr/bin/shasum -a 256 \"$dmg\""));
        // No escaped word may appear inside a double-quoted string; that is the
        // shape the injection had.
        assert!(!body.contains("'/tmp/Phosphor.dmg':"));
    }
}
