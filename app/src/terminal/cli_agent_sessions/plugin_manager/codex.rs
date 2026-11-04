use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::{env, fs, io};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    CliAgentPluginManager, ConfigFileCheckCache, PluginCommandRunner, PluginInstallError,
    PluginInstructionStep, PluginInstructions, ShellCommandRunner, compare_versions,
    is_warp_plugin_source, plugin_source_reachable, source_unreachable_error,
};
use crate::features::FeatureFlag;
use crate::terminal::shell::ShellType;

const PLUGIN_NAME: &str = "warp";
const PLUGIN_KEY: &str = "warp@codex-warp";
/// Phosphor's GitLab copy of the marketplace (full-history mirror of
/// `warpdotdev/codex-warp`). `codex plugin marketplace add` takes any
/// `https://` git URL; `owner/repo` shorthand is expanded to github.com only.
/// Codex records it verbatim as `source` under `[marketplaces.codex-warp]`
/// with `source_type = "git"`. Keep the instruction commands below in step with
/// this (a test pins them).
const MARKETPLACE_URL: &str = "https://gitlab.com/winters-phosphor/codex-warp.git";
/// The marketplace's `name` in its `marketplace.json`, unchanged by the move.
const MARKETPLACE_NAME: &str = "codex-warp";

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

/// Set once this process has *completed* a move attempt (the pre-flight
/// passed and the remove/add ran, whatever its outcome). After it,
/// `needs_source_migration` stays `false`, so the move chip is not offered
/// again in this process (a Codex too old to have `marketplace remove` fails
/// once, not on every click). A failed pre-flight does not set it.
static SOURCE_MOVE_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Serialises this process's install, update and move; each re-checks its
/// precondition once it holds the lock.
static OPERATION_LOCK: LazyLock<futures::lock::Mutex<()>> =
    LazyLock::new(|| futures::lock::Mutex::new(()));

const CODEX_CONFIG_DIR: &str = ".codex";
const CODEX_HOME_ENV: &str = "CODEX_HOME";

// Keep in sync with the plugin version in winters-phosphor/codex-warp.
const MINIMUM_PLUGIN_VERSION: &str = "0.4.0";

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

    /// Ensures the codex-warp marketplace is registered and current, while
    /// preserving a non-Git/local marketplace override. Never moves a source:
    ///
    /// - no marketplace configured: add it from Phosphor's repository (also
    ///   the recovery when an earlier move removed it and the add failed);
    /// - a Git marketplace (Phosphor's, or still Warp's -- the move chip moves
    ///   that one): `marketplace upgrade`;
    /// - a non-Git source: leave it alone.
    async fn ensure_marketplace(&self, log: &mut String) -> Result<(), PluginInstallError> {
        match codex_home_dir()
            .ok()
            .and_then(|dir| codex_warp_marketplace_config(&dir))
        {
            None => {
                self.run_logged(&["plugin", "marketplace", "add", MARKETPLACE_URL], log)
                    .await
            }
            Some(config) if config.is_git() => {
                self.run_logged(&["plugin", "marketplace", "upgrade", MARKETPLACE_NAME], log)
                    .await
            }
            Some(_) => Ok(()),
        }
    }

    /// Re-registers a marketplace recorded from Warp's GitHub from Phosphor's
    /// copy. `marketplace upgrade` re-fetches from the recorded `source`;
    /// Codex has no command that re-points a marketplace, and `marketplace
    /// add` refuses a name that is "already added from a different source;
    /// remove it before adding this source", so the move is `remove` then
    /// `add`. `remove` drops only the marketplace entry and its checkout; the
    /// `[plugins."warp@codex-warp"]` entry and the plugin cache stay, and the
    /// caller's `plugin add` refreshes the plugin.
    ///
    /// The caller has passed the pre-flight. If the add fails the marketplace
    /// is re-added from `original`'s recorded source (with its ref and sparse
    /// paths).
    async fn move_marketplace(
        &self,
        original: &CodexWarpMarketplaceConfig,
        log: &mut String,
    ) -> Result<(), PluginInstallError> {
        self.run_logged(&["plugin", "marketplace", "remove", MARKETPLACE_NAME], log)
            .await?;
        let Err(err) = self
            .run_logged(&["plugin", "marketplace", "add", MARKETPLACE_URL], log)
            .await
        else {
            return Ok(());
        };
        match original.add_arguments() {
            Some(args) => {
                log.push_str("Restore: re-registering the marketplace from its previous source\n");
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let restored = self.run_logged(&args, log).await.is_ok();
                log.push_str(if restored {
                    "Restore: the previous marketplace is back\n"
                } else {
                    "Restore failed: no codex-warp marketplace is registered; run the update \
                     again to add Phosphor's\n"
                });
            }
            None => log.push_str("Restore: no recorded marketplace source to restore\n"),
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
        if codex_warp_marketplace_config(&codex_dir).is_some_and(|config| !config.is_git()) {
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
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || plugin_needs_source_migration(&codex_dir))
    }

    fn has_local_marketplace_override(&self) -> bool {
        let Ok(codex_dir) = codex_home_dir() else {
            return false;
        };
        codex_warp_marketplace_config(&codex_dir).is_some_and(|config| !config.is_git())
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

    /// Moves a Git marketplace registered from Warp's GitHub to Phosphor's
    /// copy ([`Self::move_marketplace`]) and refreshes the plugin from it.
    /// Run only from the move chip.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        if !FeatureFlag::CodexPlugin.is_enabled() {
            return Ok(());
        }
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let Some(config) = codex_home_dir()
            .ok()
            .and_then(|dir| codex_warp_marketplace_config(&dir))
            .filter(CodexWarpMarketplaceConfig::is_registered_from_warp)
        else {
            // Moved meanwhile (another move held the lock), or nothing to move.
            return Ok(());
        };
        if !plugin_source_reachable(&*self.runner, MARKETPLACE_URL, &mut log).await {
            return Err(source_unreachable_error(log));
        }
        let moved = self.move_marketplace(&config, &mut log).await;
        SOURCE_MOVE_ATTEMPTED.store(true, Ordering::Relaxed);
        moved?;
        self.run_logged(&["plugin", "add", PLUGIN_KEY], &mut log)
            .await
    }

    fn install_success_message(&self) -> &'static str {
        "Notification plugin installed. Please restart Codex to activate."
    }

    fn move_success_message(&self) -> &'static str {
        "Notification plugin moved to Phosphor's source. Please restart Codex to activate."
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

    /// Remove-and-re-add steps, shown only by the move chip (which appears
    /// only for a marketplace registered from Warp's GitHub, never for a local
    /// override, which `marketplace remove` would drop).
    fn move_instructions(&self) -> &'static PluginInstructions {
        if FeatureFlag::CodexPlugin.is_enabled() {
            &PLUGIN_MOVE_INSTRUCTIONS
        } else {
            &EMPTY_INSTRUCTIONS
        }
    }

    fn supports_update(&self) -> bool {
        FeatureFlag::CodexPlugin.is_enabled()
    }
}

static PLUGIN_INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| {
    PluginInstructions {
        title: crate::t_static!("cli-agent-plugin-codex-plugin-install-title"),
        subtitle: crate::t_static!("cli-agent-plugin-codex-plugin-run-commands-subtitle"),
        steps: vec![
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-codex-add-marketplace-step"),
                command: "codex plugin marketplace add https://gitlab.com/winters-phosphor/codex-warp.git",
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
    }
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

static PLUGIN_UPDATE_INSTRUCTIONS: LazyLock<PluginInstructions> =
    LazyLock::new(|| PluginInstructions {
        title: crate::t_static!("cli-agent-plugin-codex-plugin-update-title"),
        subtitle: crate::t_static!("cli-agent-plugin-codex-plugin-run-commands-subtitle"),
        steps: vec![
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-codex-upgrade-marketplace-step"),
                command: "codex plugin marketplace upgrade codex-warp",
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
        post_install_notes: vec![
            crate::t_static!("cli-agent-plugin-codex-plugin-restart-update-note"),
            crate::t_static!("cli-agent-plugin-codex-marketplace-not-git-note"),
        ],
    });

static PLUGIN_MOVE_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| {
    PluginInstructions {
        title: crate::t_static!("cli-agent-plugin-codex-plugin-update-title"),
        subtitle: crate::t_static!("cli-agent-plugin-codex-plugin-run-commands-subtitle"),
        // Remove and re-add rather than `marketplace upgrade`: an upgrade
        // re-fetches from the recorded source, Warp's GitHub here. Shown only
        // when the recorded source is Warp's (see `update_instructions`).
        steps: vec![
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-remove-existing-marketplace-step"),
                command: "codex plugin marketplace remove codex-warp",
                executable: true,
                link: None,
            },
            PluginInstructionStep {
                description: crate::t_static!("cli-agent-plugin-readd-marketplace-step"),
                command: "codex plugin marketplace add https://gitlab.com/winters-phosphor/codex-warp.git",
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
    }
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

/// Whether `update()` must move the marketplace: the plugin is enabled and its
/// Git marketplace is still registered from Warp's GitHub.
fn plugin_needs_source_migration(codex_dir: &Path) -> bool {
    check_plugin_enabled(codex_dir, PLUGIN_KEY)
        && codex_warp_marketplace_config(codex_dir)
            .is_some_and(|config| config.is_registered_from_warp())
}

struct CodexWarpMarketplaceConfig {
    source_type: Option<String>,
    source: Option<String>,
    ref_name: Option<String>,
    sparse_paths: Vec<String>,
}

impl CodexWarpMarketplaceConfig {
    fn is_git(&self) -> bool {
        self.source_type.as_deref() == Some("git")
    }

    /// The `codex plugin marketplace add` arguments that register this
    /// recorded source again, as Codex wrote it (`source`, `ref`,
    /// `sparse_paths`).
    fn add_arguments(&self) -> Option<Vec<String>> {
        let source = self.source.as_deref().filter(|source| !source.is_empty())?;
        let mut args: Vec<String> = ["plugin", "marketplace", "add", source]
            .iter()
            .map(|arg| (*arg).to_owned())
            .collect();
        if let Some(ref_name) = &self.ref_name {
            args.extend(["--ref".to_owned(), ref_name.clone()]);
        }
        for path in &self.sparse_paths {
            args.extend(["--sparse".to_owned(), path.clone()]);
        }
        Some(args)
    }

    /// A Git marketplace whose recorded `source` is Warp's GitHub repository.
    /// A local (non-Git) override is never reported, whatever its path.
    fn is_registered_from_warp(&self) -> bool {
        self.is_git()
            && self
                .source
                .as_deref()
                .is_some_and(|source| is_warp_plugin_source(source, MARKETPLACE_NAME))
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
        ref_name: marketplace
            .get("ref")
            .and_then(|ref_name| ref_name.as_str())
            .map(str::to_owned),
        sparse_paths: marketplace
            .get("sparse_paths")
            .and_then(|paths| paths.as_array())
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
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
