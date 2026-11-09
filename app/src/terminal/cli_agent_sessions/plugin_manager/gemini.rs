use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    CliAgentPluginManager, ConfigFileCheckCache, PluginCommandRunner, PluginInstallError,
    PluginInstructionStep, PluginInstructions, ShellCommandRunner, bundle_missing_error,
    bundled_plugin_dir, compare_versions, looks_like_bundle_path, same_folder,
};
use crate::terminal::shell::ShellType;

/// Overrides where the bundled extension folder is looked up (unit tests, app tests and dev
/// builds that run outside an installed bundle). Unset in a normal install.
pub(crate) const BUNDLE_DIR_ENV: &str = "PHOSPHOR_GEMINI_PLUGIN_DIR";
/// Where the extension lives inside the app's resources (`resources/bundled/gemini_plugin` in
/// the repo). `gemini extensions install <folder>` copies a local folder into
/// `~/.gemini/extensions/<name>` and records `{"source": <folder>, "type": "local"}`; nothing
/// is fetched.
const BUNDLE_RELATIVE: &str = "bundled/gemini_plugin";
const BUNDLE_MARKER: &str = "gemini-extension.json";
/// The extension's `name` in its `gemini-extension.json`, which is also its
/// directory under `~/.gemini/extensions`. Unchanged by the move.
const EXTENSION_NAME: &str = "gemini-warp";
/// Where Gemini CLI records an installed extension's source.
const INSTALL_METADATA_FILE: &str = ".gemini-extension-install.json";

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

/// Set once this process has *completed* a move attempt (the pre-flight
/// passed and the uninstall/install ran, whatever its outcome). After it,
/// `needs_source_migration` stays `false`, so the move chip is not offered
/// again in this process. A move with no bundle to move to does not set it.
static SOURCE_MOVE_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Serialises this process's install, update and move; each re-checks its
/// precondition once it holds the lock.
static OPERATION_LOCK: LazyLock<futures::lock::Mutex<()>> =
    LazyLock::new(|| futures::lock::Mutex::new(()));

/// The env var Gemini CLI reads as its home directory
/// (`packages/core/src/utils/paths.ts`), falling back to the user's home.
const GEMINI_CLI_HOME_ENV: &str = "GEMINI_CLI_HOME";

/// The extension version shipped in `resources/bundled/gemini_plugin` (a unit test pins this
/// to that folder's `gemini-extension.json`). Updates arrive with Phosphor releases.
const MINIMUM_PLUGIN_VERSION: &str = "1.0.0";

/// The folder holding the extension that ships with this build, if it can be found
/// (`$PHOSPHOR_GEMINI_PLUGIN_DIR`, else the app's bundled resources). Phosphor never fetches
/// the extension from anywhere.
pub(crate) fn bundled_extension_dir() -> Option<PathBuf> {
    bundled_plugin_dir(BUNDLE_DIR_ENV, BUNDLE_RELATIVE, BUNDLE_MARKER)
}

pub(super) struct GeminiPluginManager {
    runner: Arc<dyn PluginCommandRunner>,
}

impl GeminiPluginManager {
    pub(super) fn new(
        shell_path: Option<PathBuf>,
        shell_type: Option<ShellType>,
        path_env_var: Option<String>,
    ) -> Self {
        Self::with_runner(Arc::new(ShellCommandRunner::new(
            shell_path,
            shell_type,
            path_env_var,
        )))
    }

    pub(super) fn with_runner(runner: Arc<dyn PluginCommandRunner>) -> Self {
        Self { runner }
    }

    async fn run_logged(&self, args: &[&str], log: &mut String) -> Result<(), PluginInstallError> {
        self.runner.run("gemini", args, &[], log).await
    }

    /// Replaces the installed extension with the bundled folder.
    ///
    /// `extensions update` re-fetches from the recorded source (a remote one would mean the
    /// network; a recorded bundle folder may have moved with the app), and `extensions
    /// install` refuses an extension that "is already installed. Please uninstall it
    /// first." So: uninstall, then install from the bundled folder.
    ///
    /// The extension directory is copied to a fresh, uniquely named backup first and put
    /// back if the uninstall or the install fails, so a failed replace leaves the working
    /// install in place. With nothing installed it is just the install.
    async fn replace_extension(
        &self,
        extensions_dir: &Path,
        bundle: &Path,
        log: &mut String,
    ) -> Result<(), PluginInstallError> {
        let bundle_arg = bundle.to_string_lossy().into_owned();
        let install_args = ["extensions", "install", bundle_arg.as_str(), "--consent"];
        let extension_dir = extensions_dir.join(EXTENSION_NAME);
        if !extension_dir.exists() {
            return self.run_logged(&install_args, log).await;
        }
        let backup = match back_up_dir(&extension_dir) {
            Ok(backup) => backup,
            Err(err) => {
                log.push_str(&format!(
                    "Could not copy {} aside ({err}); left the extension as it is\n",
                    extension_dir.display()
                ));
                return Err(PluginInstallError {
                    message: "Could not back up the installed extension".to_owned(),
                    log: log.clone(),
                });
            }
        };
        let moved = match self
            .run_logged(&["extensions", "uninstall", EXTENSION_NAME], log)
            .await
        {
            Ok(()) => self.run_logged(&install_args, log).await,
            Err(err) => Err(err),
        };
        match moved {
            Ok(()) => {
                let _ = fs::remove_dir_all(&backup);
                Ok(())
            }
            Err(err) => {
                match restore_dir(&backup, &extension_dir) {
                    Ok(()) => log.push_str(&format!(
                        "Restore: put the previous extension back at {}\n",
                        extension_dir.display()
                    )),
                    Err(restore_err) => log.push_str(&format!(
                        "Restore failed ({restore_err}); the previous extension is kept at {}\n",
                        backup.display()
                    )),
                }
                Err(PluginInstallError {
                    message: err.message,
                    log: log.clone(),
                })
            }
        }
    }
}

#[async_trait]
impl CliAgentPluginManager for GeminiPluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        MINIMUM_PLUGIN_VERSION
    }

    fn can_auto_install(&self) -> bool {
        true
    }

    fn is_installed(&self) -> bool {
        let Ok(extensions_dir) = gemini_extensions_dir() else {
            return false;
        };
        check_installed(&extensions_dir)
    }

    fn needs_update(&self) -> bool {
        let Ok(extensions_dir) = gemini_extensions_dir() else {
            return false;
        };
        // Version only: the move has its own chip.
        plugin_is_outdated(&extensions_dir)
    }

    fn needs_source_migration(&self) -> bool {
        if SOURCE_MOVE_ATTEMPTED.load(Ordering::Relaxed) {
            return false;
        }
        let Ok(extensions_dir) = gemini_extensions_dir() else {
            return false;
        };
        let extension_dir = extensions_dir.join(EXTENSION_NAME);
        let files = [
            extension_dir.join("gemini-extension.json"),
            extension_dir.join(INSTALL_METADATA_FILE),
        ];
        let bundle = bundled_extension_dir();
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || {
            plugin_needs_source_migration(&extensions_dir, bundle.as_deref())
        })
    }

    /// Installs the bundled folder: `gemini extensions install <folder> --consent`.
    async fn install(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        if gemini_extensions_dir().is_ok_and(|dir| check_installed(&dir)) {
            // Installed meanwhile (another install held the lock).
            return Ok(());
        }
        let Some(bundle) = bundled_extension_dir() else {
            return Err(bundle_missing_error("Gemini CLI", log));
        };
        let bundle_arg = bundle.to_string_lossy().into_owned();
        self.run_logged(
            &["extensions", "install", bundle_arg.as_str(), "--consent"],
            &mut log,
        )
        .await?;
        Ok(())
    }

    /// Version update: replaces the installed extension with the bundled one (which carries
    /// this build's version). A developer's `local` / `link` install is left alone.
    async fn update(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let extensions_dir = gemini_extensions_dir().ok();
        if extensions_dir
            .as_deref()
            .is_some_and(|dir| check_installed(dir) && !plugin_is_outdated(dir))
        {
            // Updated meanwhile (another update held the lock).
            return Ok(());
        }
        if extensions_dir
            .as_deref()
            .is_some_and(|dir| install_source(dir) == InstallSource::DeveloperLocal)
        {
            log.push_str(
                "The gemini-warp extension is a local install of your own checkout; update it \
                 from there. Left as it is.\n",
            );
            return Err(PluginInstallError {
                message: "Local extension left as it is".to_owned(),
                log,
            });
        }
        let Some(bundle) = bundled_extension_dir() else {
            return Err(bundle_missing_error("Gemini CLI", log));
        };
        let Some(extensions_dir) = extensions_dir else {
            return Err(PluginInstallError {
                message: "Could not determine the Gemini CLI extensions directory".to_owned(),
                log,
            });
        };
        self.replace_extension(&extensions_dir, &bundle, &mut log)
            .await?;

        // Sanity check: verify the on-disk version actually changed.
        let still_outdated = gemini_extensions_dir()
            .ok()
            .and_then(|dir| installed_version(&dir))
            .map(|v| compare_versions(&v, MINIMUM_PLUGIN_VERSION).is_lt())
            .unwrap_or(true);
        if still_outdated {
            log.push_str("Post-update version check: plugin is still outdated\n");
            return Err(PluginInstallError {
                message: "Plugin update did not take effect".to_owned(),
                log,
            });
        }
        Ok(())
    }

    /// Moves an extension installed from a remote repository (or from a bundle folder at an
    /// old install location) to the bundled folder ([`Self::replace_extension`]). Run only
    /// from the move chip. Every step is local.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let bundle = bundled_extension_dir();
        let Some(extensions_dir) = gemini_extensions_dir()
            .ok()
            .filter(|dir| plugin_needs_source_migration(dir, bundle.as_deref()))
        else {
            // Moved meanwhile (another move held the lock), or nothing to move.
            return Ok(());
        };
        let Some(bundle) = bundle else {
            return Err(bundle_missing_error("Gemini CLI", log));
        };
        let moved = self
            .replace_extension(&extensions_dir, &bundle, &mut log)
            .await;
        SOURCE_MOVE_ATTEMPTED.store(true, Ordering::Relaxed);
        moved
    }

    fn move_success_message(&self) -> &'static str {
        "Notification plugin moved to the copy bundled with Phosphor. Please restart Gemini CLI to activate."
    }

    fn install_success_message(&self) -> &'static str {
        "Notification plugin installed. Please restart Gemini CLI to activate."
    }

    fn update_success_message(&self) -> &'static str {
        "Notification plugin updated. Please restart Gemini CLI to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &UPDATE_INSTRUCTIONS
    }
}

/// `gemini extensions install "<bundled folder>" --consent`, as a command a user can paste.
/// The folder is resolved once; a build run outside an install shows a placeholder path.
fn install_bundle_command() -> &'static str {
    static COMMAND: LazyLock<&'static str> = LazyLock::new(|| {
        let folder = bundled_extension_dir()
            .map(|dir| dir.display().to_string())
            .unwrap_or_else(|| format!("<Phosphor install folder>/resources/{BUNDLE_RELATIVE}"));
        Box::leak(format!("gemini extensions install \"{folder}\" --consent").into_boxed_str())
    });
    *COMMAND
}

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-gemini-install-title"),
    subtitle: crate::t_static!("cli-agent-plugin-gemini-run-command-restart"),
    steps: vec![PluginInstructionStep {
        description: crate::t_static!("cli-agent-plugin-install-warp-extension-step"),
        command: install_bundle_command(),
        executable: true,
        link: None,
    }],
    post_install_notes: vec![crate::t_static!("cli-agent-plugin-gemini-restart-note")],
});

/// Also the move chip's manual steps (the trait default): uninstall and reinstall rather than
/// `extensions update`, which re-fetches from the recorded source.
static UPDATE_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-gemini-update-title"),
    subtitle: crate::t_static!("cli-agent-plugin-gemini-run-command-restart"),
    steps: vec![
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-uninstall-warp-extension-step"),
            command: "gemini extensions uninstall gemini-warp",
            executable: true,
            link: None,
        },
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-install-warp-extension-step"),
            command: install_bundle_command(),
            executable: true,
            link: None,
        },
    ],
    post_install_notes: vec![crate::t_static!(
        "cli-agent-plugin-gemini-restart-update-note"
    )],
});

fn check_installed(extensions_dir: &Path) -> bool {
    let manifest_path = extensions_dir
        .join(EXTENSION_NAME)
        .join("gemini-extension.json");
    let Ok(contents) = fs::read_to_string(manifest_path) else {
        return false;
    };
    serde_json::from_str::<Value>(&contents).is_ok()
}

/// Whether the installed extension is below the minimum version.
/// `needs_update` adds the source half (`needs_source_migration`).
fn plugin_is_outdated(extensions_dir: &Path) -> bool {
    match installed_version(extensions_dir) {
        Some(v) => compare_versions(&v, MINIMUM_PLUGIN_VERSION).is_lt(),
        // No version field means very old or malformed extension.
        None => check_installed(extensions_dir),
    }
}

/// Whether the installed extension came from somewhere other than the bundled folder: a
/// remote repository (Warp's GitHub, an older GitLab copy) or a bundle folder at an old
/// install location. A developer's `local` / `link` install is never moved, and with no
/// bundled folder to move to there is nothing to offer.
fn plugin_needs_source_migration(extensions_dir: &Path, bundle: Option<&Path>) -> bool {
    let Some(bundle) = bundle else {
        return false;
    };
    if !check_installed(extensions_dir) {
        return false;
    }
    match install_source(extensions_dir) {
        InstallSource::Remote => true,
        InstallSource::Bundle(path) => !same_folder(&path, bundle),
        InstallSource::DeveloperLocal | InstallSource::Unknown => false,
    }
}

/// Where the installed extension was installed from, as Gemini CLI recorded it.
#[derive(Debug, PartialEq, Eq)]
enum InstallSource {
    /// A `git` or `github-release` install.
    Remote,
    /// A `local` install of one of Phosphor's bundle folders (any install location).
    Bundle(String),
    /// A `local` or `link` install of any other folder: a developer's checkout.
    DeveloperLocal,
    /// No readable record.
    Unknown,
}

/// Reads `.gemini-extension-install.json` (`{"source": ..., "type": ...}`) in the extension's
/// directory.
fn install_source(extensions_dir: &Path) -> InstallSource {
    let metadata_path = extensions_dir
        .join(EXTENSION_NAME)
        .join(INSTALL_METADATA_FILE);
    let Ok(contents) = fs::read_to_string(metadata_path) else {
        return InstallSource::Unknown;
    };
    let Ok(metadata) = serde_json::from_str::<Value>(&contents) else {
        return InstallSource::Unknown;
    };
    let source = metadata.get("source").and_then(Value::as_str);
    match (metadata.get("type").and_then(Value::as_str), source) {
        (Some("git" | "github-release"), _) => InstallSource::Remote,
        (Some("local"), Some(path)) if looks_like_bundle_path(path, BUNDLE_RELATIVE) => {
            InstallSource::Bundle(path.to_owned())
        }
        (Some("local" | "link"), _) => InstallSource::DeveloperLocal,
        _ => InstallSource::Unknown,
    }
}

/// Reads the installed version string for the Zap extension, if present.
fn installed_version(extensions_dir: &Path) -> Option<String> {
    let manifest_path = extensions_dir
        .join(EXTENSION_NAME)
        .join("gemini-extension.json");
    let contents = fs::read_to_string(manifest_path).ok()?;
    let parsed: Value = serde_json::from_str(&contents).ok()?;
    parsed.get("version")?.as_str().map(|s| s.to_owned())
}

/// Copies `from` to a new, uniquely named directory under the temp dir
/// (outside the extensions directory, which Gemini CLI loads every
/// subdirectory of) and returns it. Never reuses or deletes an existing
/// directory: `create_dir` fails on one that exists, and the next name is
/// tried.
fn back_up_dir(from: &Path) -> io::Result<PathBuf> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    for _ in 0..16 {
        let candidate = env::temp_dir().join(format!(
            "phosphor-{EXTENSION_NAME}-backup-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                copy_dir_contents(from, &candidate)?;
                return Ok(candidate);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free backup directory name",
    ))
}

/// Puts the copy at `backup` back at `to` (replacing whatever a failed
/// uninstall or install left there), then removes the copy.
fn restore_dir(backup: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        fs::remove_dir_all(to)?;
    }
    copy_dir_all(backup, to)?;
    // The backup is this move's own directory (see `back_up_dir`).
    let _ = fs::remove_dir_all(backup);
    Ok(())
}

/// Copies the entries of `from` into the existing directory `to`.
fn copy_dir_contents(from: &Path, to: &Path) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if file_type.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, &target)?;
            #[cfg(not(unix))]
            fs::copy(entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn copy_dir_all(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    copy_dir_contents(from, to)
}

/// Returns the path to `<home>/.gemini/extensions`, where `<home>` is
/// `GEMINI_CLI_HOME` when set (as Gemini CLI itself resolves it), else the
/// user's home directory.
fn gemini_extensions_dir() -> io::Result<PathBuf> {
    env::var(GEMINI_CLI_HOME_ENV)
        .ok()
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .map(|home| home.join(".gemini").join("extensions"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine home directory",
            )
        })
}

/// Forgets a move attempt, for tests that drive several in one process.
#[cfg(test)]
pub(super) fn reset_source_move_attempt() {
    SOURCE_MOVE_ATTEMPTED.store(false, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "gemini_tests.rs"]
mod tests;
