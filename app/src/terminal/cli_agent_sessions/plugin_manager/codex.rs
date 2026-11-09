use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::{env, fs, io};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    CliAgentPluginManager, ConfigFileCheckCache, PluginCommandRunner, PluginInstallError,
    PluginInstructionStep, PluginInstructions, ShellCommandRunner, bundle_missing_error,
    bundled_plugin_dir, compare_versions, looks_like_bundle_path, same_folder,
};
use crate::features::FeatureFlag;
use crate::terminal::shell::ShellType;

const PLUGIN_NAME: &str = "warp";
const PLUGIN_KEY: &str = "warp@codex-warp";
/// The marketplace's `name` in its `.agents/plugins/marketplace.json`. Kept from the original
/// plugin so an existing install's `warp@codex-warp` key stays valid.
const MARKETPLACE_NAME: &str = "codex-warp";
/// Overrides where the bundled marketplace folder is looked up (unit tests, app tests and dev
/// builds that run outside an installed bundle). Unset in a normal install.
pub(crate) const BUNDLE_DIR_ENV: &str = "PHOSPHOR_CODEX_PLUGIN_DIR";
/// Where the plugin lives inside the app's resources (`resources/bundled/codex_plugin` in the
/// repo): a folder that is a complete Codex marketplace. `codex plugin marketplace add` takes
/// a local directory and records it as `source_type = "local"`; nothing is fetched.
const BUNDLE_RELATIVE: &str = "bundled/codex_plugin";
const BUNDLE_MARKER: &str = ".agents/plugins/marketplace.json";

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

/// Set once this process has *completed* a move attempt (the bundle was found and the
/// remove/add ran, whatever its outcome). After it, `needs_source_migration` stays `false`,
/// so the move chip is not offered again in this process (a Codex too old to have
/// `marketplace remove` fails once, not on every click). A move with no bundle to move to
/// does not set it -- nothing was attempted.
static SOURCE_MOVE_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Serialises this process's install, update and move; each re-checks its
/// precondition once it holds the lock.
static OPERATION_LOCK: LazyLock<futures::lock::Mutex<()>> =
    LazyLock::new(|| futures::lock::Mutex::new(()));

const CODEX_CONFIG_DIR: &str = ".codex";
const CODEX_HOME_ENV: &str = "CODEX_HOME";

/// The plugin version shipped in `resources/bundled/codex_plugin` (a unit test pins this to
/// that folder's `plugin.json` and `marketplace.json`). Plugin updates arrive with Phosphor
/// releases.
const MINIMUM_PLUGIN_VERSION: &str = "0.4.3";

/// The folder holding the plugin that ships with this build, if it can be found
/// (`$PHOSPHOR_CODEX_PLUGIN_DIR`, else the app's bundled resources). Phosphor never fetches
/// the plugin from anywhere.
pub(crate) fn bundled_marketplace_dir() -> Option<PathBuf> {
    bundled_plugin_dir(BUNDLE_DIR_ENV, BUNDLE_RELATIVE, BUNDLE_MARKER)
}

pub(super) struct CodexPluginManager {
    runner: Arc<dyn PluginCommandRunner>,
}

impl CodexPluginManager {
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
        self.runner.run("codex", args, &[], log).await
    }

    /// Ensures the codex-warp marketplace is registered from the bundled folder, while
    /// preserving a developer's own local marketplace:
    ///
    /// - no marketplace configured: add the bundled folder;
    /// - registered from the current bundled folder: nothing to do;
    /// - registered from a remote repository (Warp's GitHub, an older GitLab copy) or from a
    ///   bundle folder at an old install location: replaced by the bundled folder
    ///   ([`Self::move_marketplace`]), since `marketplace upgrade` would fetch from the
    ///   recorded remote;
    /// - a developer's own local folder: left alone.
    async fn ensure_marketplace(&self, log: &mut String) -> Result<(), PluginInstallError> {
        let config = codex_home_dir()
            .ok()
            .and_then(|dir| codex_warp_marketplace_config(&dir));
        let bundle = bundled_marketplace_dir();
        match config {
            Some(config) if config.is_developer_local() => Ok(()),
            Some(config)
                if bundle
                    .as_deref()
                    .is_some_and(|b| config.is_current_bundle(b)) =>
            {
                Ok(())
            }
            Some(config) => {
                let bundle = bundle.ok_or_else(|| bundle_missing_error("Codex", log.clone()))?;
                self.move_marketplace(&bundle, &config, log).await
            }
            None => {
                let bundle = bundle.ok_or_else(|| bundle_missing_error("Codex", log.clone()))?;
                let bundle_arg = bundle.to_string_lossy().into_owned();
                self.run_logged(&["plugin", "marketplace", "add", bundle_arg.as_str()], log)
                    .await
            }
        }
    }

    /// Re-registers the marketplace from the bundled folder. Codex has no command that
    /// re-points a marketplace, and `marketplace add` refuses a name that is "already added
    /// from a different source; remove it before adding this source", so this is `remove`
    /// then `add`. `remove` drops only the marketplace entry and its checkout; the
    /// `[plugins."warp@codex-warp"]` entry and the plugin cache stay, and the caller's
    /// `plugin add` refreshes the plugin.
    ///
    /// If the add fails, a previous *local* source is registered again; a remote one is not
    /// re-fetched (Phosphor never does that), the log says to run the update again.
    async fn move_marketplace(
        &self,
        bundle: &Path,
        original: &CodexWarpMarketplaceConfig,
        log: &mut String,
    ) -> Result<(), PluginInstallError> {
        let bundle_arg = bundle.to_string_lossy().into_owned();
        self.run_logged(&["plugin", "marketplace", "remove", MARKETPLACE_NAME], log)
            .await?;
        let Err(err) = self
            .run_logged(&["plugin", "marketplace", "add", bundle_arg.as_str()], log)
            .await
        else {
            return Ok(());
        };
        match original.local_path() {
            Some(path) => {
                log.push_str(&format!(
                    "Restore: re-registering the marketplace from its previous folder {path}\n"
                ));
                let restored = self
                    .run_logged(&["plugin", "marketplace", "add", path.as_str()], log)
                    .await
                    .is_ok();
                log.push_str(if restored {
                    "Restore: the previous marketplace is back\n"
                } else {
                    "Restore failed: no codex-warp marketplace is registered; run the update \
                     again\n"
                });
            }
            None => log.push_str(
                "Restore: the previous marketplace source is not a local folder and is not \
                 re-fetched; run the update again\n",
            ),
        }
        Err(PluginInstallError {
            message: err.message,
            log: log.clone(),
        })
    }
}

#[async_trait]
impl CliAgentPluginManager for CodexPluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        if FeatureFlag::CodexPlugin.is_enabled() {
            MINIMUM_PLUGIN_VERSION
        } else {
            "0.0.0"
        }
    }

    fn can_auto_install(&self) -> bool {
        FeatureFlag::CodexPlugin.is_enabled()
    }

    fn is_installed(&self) -> bool {
        if !FeatureFlag::CodexPlugin.is_enabled() {
            return false;
        }
        let Ok(codex_dir) = codex_home_dir() else {
            return false;
        };
        check_installed(&codex_dir)
    }

    fn needs_update(&self) -> bool {
        if !FeatureFlag::CodexPlugin.is_enabled() {
            return false;
        }
        let Ok(codex_dir) = codex_home_dir() else {
            return false;
        };
        if codex_warp_marketplace_config(&codex_dir)
            .is_some_and(|config| config.is_developer_local())
        {
            return false;
        }
        // Version only: the move has its own chip.
        plugin_needs_update(&codex_dir, PLUGIN_NAME, PLUGIN_KEY, MINIMUM_PLUGIN_VERSION)
    }

    fn needs_source_migration(&self) -> bool {
        if !FeatureFlag::CodexPlugin.is_enabled() || SOURCE_MOVE_ATTEMPTED.load(Ordering::Relaxed) {
            return false;
        }
        let Ok(codex_dir) = codex_home_dir() else {
            return false;
        };
        let files = [codex_dir.join("config.toml")];
        let bundle = bundled_marketplace_dir();
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || {
            plugin_needs_source_migration(&codex_dir, bundle.as_deref())
        })
    }

    fn has_local_marketplace_override(&self) -> bool {
        let Ok(codex_dir) = codex_home_dir() else {
            return false;
        };
        codex_warp_marketplace_config(&codex_dir).is_some_and(|config| config.is_developer_local())
    }

    async fn install(&self) -> Result<(), PluginInstallError> {
        if !FeatureFlag::CodexPlugin.is_enabled() {
            return Ok(());
        }
        log::info!("[PLUGIN_INSTALL] updating codex plugin");
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        if codex_home_dir().is_ok_and(|dir| check_installed(&dir)) {
            // Installed meanwhile (another install held the lock).
            return Ok(());
        }
        ensure_codex_home_dir()?;
        self.ensure_marketplace(&mut log).await?;
        self.run_logged(&["plugin", "add", PLUGIN_KEY], &mut log)
            .await?;
        Ok(())
    }

    async fn update(&self) -> Result<(), PluginInstallError> {
        if !FeatureFlag::CodexPlugin.is_enabled() {
            return Ok(());
        }
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        if codex_home_dir().is_ok_and(|dir| {
            check_installed(&dir)
                && !plugin_needs_update(&dir, PLUGIN_NAME, PLUGIN_KEY, MINIMUM_PLUGIN_VERSION)
        }) {
            // Updated meanwhile (another update held the lock).
            return Ok(());
        }
        ensure_codex_home_dir()?;
        self.ensure_marketplace(&mut log).await?;
        self.run_logged(&["plugin", "add", PLUGIN_KEY], &mut log)
            .await?;

        let still_outdated = codex_home_dir()
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

    /// Moves a marketplace registered from a remote repository (or from a bundle folder at an
    /// old install location) to the bundled folder ([`Self::move_marketplace`]) and refreshes
    /// the plugin from it. Run only from the move chip. Every step is local.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        if !FeatureFlag::CodexPlugin.is_enabled() {
            return Ok(());
        }
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let bundle = bundled_marketplace_dir();
        let Some(config) = codex_home_dir().ok().and_then(|dir| {
            plugin_needs_source_migration(&dir, bundle.as_deref())
                .then(|| codex_warp_marketplace_config(&dir))
                .flatten()
        }) else {
            // Moved meanwhile (another move held the lock), or nothing to move.
            return Ok(());
        };
        let Some(bundle) = bundle else {
            return Err(bundle_missing_error("Codex", log));
        };
        let moved = self.move_marketplace(&bundle, &config, &mut log).await;
        SOURCE_MOVE_ATTEMPTED.store(true, Ordering::Relaxed);
        moved?;
        self.run_logged(&["plugin", "add", PLUGIN_KEY], &mut log)
            .await
    }

    fn install_success_message(&self) -> &'static str {
        "Notification plugin installed. Please restart Codex to activate."
    }

    fn move_success_message(&self) -> &'static str {
        "Notification plugin moved to the copy bundled with Phosphor. Please restart Codex to activate."
    }

    fn update_success_message(&self) -> &'static str {
        "Notification plugin updated. Please restart Codex to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        if FeatureFlag::CodexPlugin.is_enabled() {
            &PLUGIN_INSTALL_INSTRUCTIONS
        } else {
            &NATIVE_INSTALL_INSTRUCTIONS
        }
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        if FeatureFlag::CodexPlugin.is_enabled() {
            &PLUGIN_UPDATE_INSTRUCTIONS
        } else {
            &EMPTY_INSTRUCTIONS
        }
    }

    fn supports_update(&self) -> bool {
        FeatureFlag::CodexPlugin.is_enabled()
    }
}

/// `codex plugin marketplace add <bundled folder>`, as a command a user can paste. The folder
/// is resolved once; a build run outside an install shows a placeholder path.
fn add_bundle_command() -> &'static str {
    static COMMAND: LazyLock<&'static str> = LazyLock::new(|| {
        let folder = bundled_marketplace_dir()
            .map(|dir| dir.display().to_string())
            .unwrap_or_else(|| format!("<Phosphor install folder>/resources/{BUNDLE_RELATIVE}"));
        Box::leak(format!("codex plugin marketplace add \"{folder}\"").into_boxed_str())
    });
    *COMMAND
}

static PLUGIN_INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> =
    LazyLock::new(|| PluginInstructions {
        title: crate::t_static!("cli-agent-plugin-codex-plugin-install-title"),
        subtitle: crate::t_static!("cli-agent-plugin-codex-plugin-run-commands-subtitle"),
        steps: vec![
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-codex-add-marketplace-step"),
                command: add_bundle_command(),
                executable: true,
                link: None,
            },
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-install-warp-plugin-step"),
                command: "codex plugin add warp@codex-warp",
                executable: true,
                link: None,
            },
        ],
        post_install_notes: vec![crate::t_static!(
            "cli-agent-plugin-codex-plugin-restart-note"
        )],
    });

static NATIVE_INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> =
    LazyLock::new(|| PluginInstructions {
        title: crate::t_static!("cli-agent-plugin-codex-install-title"),
        subtitle: crate::t_static!("cli-agent-plugin-codex-install-subtitle"),
        steps: vec![
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-codex-update-step"),
                command: "",
                executable: false,
                link: Some("https://developers.openai.com/codex/cli#upgrade"),
            },
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-codex-notification-step"),
                command: "[tui]\nnotification_condition = \"always\"",
                executable: false,
                link: None,
            },
        ],
        post_install_notes: vec![crate::t_static!("cli-agent-plugin-codex-restart-note")],
    });

static EMPTY_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: "",
    subtitle: "",
    steps: vec![],
    post_install_notes: vec![],
});

/// Also the move chip's manual steps (the trait default): remove and re-add rather than
/// `marketplace upgrade`, which would re-fetch from a recorded remote source.
static PLUGIN_UPDATE_INSTRUCTIONS: LazyLock<PluginInstructions> =
    LazyLock::new(|| PluginInstructions {
        title: crate::t_static!("cli-agent-plugin-codex-plugin-update-title"),
        subtitle: crate::t_static!("cli-agent-plugin-codex-plugin-run-commands-subtitle"),
        steps: vec![
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-remove-existing-marketplace-step"),
                command: "codex plugin marketplace remove codex-warp",
                executable: true,
                link: None,
            },
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-readd-marketplace-step"),
                command: add_bundle_command(),
                executable: true,
                link: None,
            },
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-codex-reinstall-plugin-step"),
                command: "codex plugin add warp@codex-warp",
                executable: true,
                link: None,
            },
        ],
        post_install_notes: vec![crate::t_static!(
            "cli-agent-plugin-codex-plugin-restart-update-note"
        )],
    });

fn check_installed(codex_dir: &Path) -> bool {
    check_plugin_enabled(codex_dir, PLUGIN_KEY)
}

/// Whether `config.toml` marks the given plugin key as enabled.
fn check_plugin_enabled(codex_dir: &Path, plugin_key: &str) -> bool {
    let config_path = codex_dir.join("config.toml");
    let Ok(contents) = fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(parsed) = contents.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    parsed
        .get("plugins")
        .and_then(|plugins| plugins.get(plugin_key))
        .and_then(|plugin| plugin.get("enabled"))
        .and_then(|enabled| enabled.as_bool())
        .unwrap_or(false)
}

/// Reads the latest cached Phosphor plugin version, if present.
fn installed_version(codex_dir: &Path) -> Option<String> {
    installed_plugin_version(codex_dir, PLUGIN_NAME)
}

/// Reads the latest cached version for `plugin_name` from
/// `plugins/cache/codex-warp/<plugin_name>/<version>/.codex-plugin/plugin.json`.
fn installed_plugin_version(codex_dir: &Path, plugin_name: &str) -> Option<String> {
    let cache_dir = codex_dir
        .join("plugins")
        .join("cache")
        .join(MARKETPLACE_NAME)
        .join(plugin_name);
    let entries = fs::read_dir(cache_dir).ok()?;
    let mut latest: Option<String> = None;
    for entry in entries.flatten() {
        let manifest_path = entry.path().join(".codex-plugin").join("plugin.json");
        let Some(version) = plugin_manifest_version(manifest_path) else {
            continue;
        };
        if latest
            .as_deref()
            .map(|current| compare_versions(&version, current).is_gt())
            .unwrap_or(true)
        {
            latest = Some(version);
        }
    }
    latest
}

fn plugin_manifest_version(manifest_path: impl AsRef<Path>) -> Option<String> {
    let contents = fs::read_to_string(manifest_path).ok()?;
    let parsed = serde_json::from_str::<Value>(&contents).ok()?;
    parsed
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

fn plugin_needs_update(
    codex_dir: &Path,
    plugin_name: &str,
    plugin_key: &str,
    minimum_version: &str,
) -> bool {
    if !check_plugin_enabled(codex_dir, plugin_key) {
        return false;
    }
    match installed_plugin_version(codex_dir, plugin_name) {
        Some(v) => compare_versions(&v, minimum_version).is_lt(),
        // No version field means very old plugin.
        None => true,
    }
}

/// Whether the plugin's marketplace is registered from somewhere other than the bundled
/// folder: a remote repository (Warp's GitHub, an older GitLab copy) or a bundle folder at an
/// old install location. A developer's own local folder is never moved, and with no bundled
/// folder to move to there is nothing to offer.
fn plugin_needs_source_migration(codex_dir: &Path, bundle: Option<&Path>) -> bool {
    let Some(bundle) = bundle else {
        return false;
    };
    check_plugin_enabled(codex_dir, PLUGIN_KEY)
        && codex_warp_marketplace_config(codex_dir)
            .is_some_and(|config| !config.is_developer_local() && !config.is_current_bundle(bundle))
}

struct CodexWarpMarketplaceConfig {
    source_type: Option<String>,
    source: Option<String>,
}

impl CodexWarpMarketplaceConfig {
    fn is_git(&self) -> bool {
        self.source_type.as_deref() == Some("git")
    }

    /// The folder of a non-Git (`local`) source.
    fn local_path(&self) -> Option<String> {
        if self.is_git() {
            return None;
        }
        self.source.clone().filter(|source| !source.is_empty())
    }

    /// Registered from one of Phosphor's own bundle folders (at any install location).
    fn is_bundle_folder(&self) -> bool {
        self.local_path()
            .is_some_and(|path| looks_like_bundle_path(&path, BUNDLE_RELATIVE))
    }

    /// Registered from the bundled folder of this install.
    fn is_current_bundle(&self, bundle: &Path) -> bool {
        self.local_path()
            .is_some_and(|path| same_folder(&path, bundle))
    }

    /// A local folder that is NOT a Phosphor bundle folder: a developer's own checkout,
    /// which Phosphor leaves alone.
    fn is_developer_local(&self) -> bool {
        !self.is_git() && !self.is_bundle_folder()
    }
}

fn codex_warp_marketplace_config(codex_dir: &Path) -> Option<CodexWarpMarketplaceConfig> {
    let config_path = codex_dir.join("config.toml");
    let contents = fs::read_to_string(config_path).ok()?;
    let parsed = contents.parse::<toml_edit::DocumentMut>().ok()?;
    let marketplace = parsed.get("marketplaces")?.get(MARKETPLACE_NAME)?;
    Some(CodexWarpMarketplaceConfig {
        source_type: marketplace
            .get("source_type")
            .and_then(|source_type| source_type.as_str())
            .map(str::to_owned),
        source: marketplace
            .get("source")
            .and_then(|source| source.as_str())
            .map(str::to_owned),
    })
}

/// Checks `CODEX_HOME` first, falls back to `~/.codex`.
fn codex_home_dir() -> io::Result<PathBuf> {
    let codex_home = env::var(CODEX_HOME_ENV).unwrap_or_default();
    if !codex_home.is_empty() {
        return Ok(PathBuf::from(codex_home));
    }
    dirs::home_dir()
        .map(|home| home.join(CODEX_CONFIG_DIR))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine home directory",
            )
        })
}

/// Creates the resolved Codex home directory if it does not yet exist.
/// The Codex CLI expects `CODEX_HOME` to exist before running plugin commands.
fn ensure_codex_home_dir() -> io::Result<PathBuf> {
    let dir = codex_home_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Forgets a move attempt, for tests that drive several in one process.
#[cfg(test)]
pub(super) fn reset_source_move_attempt() {
    SOURCE_MOVE_ATTEMPTED.store(false, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
