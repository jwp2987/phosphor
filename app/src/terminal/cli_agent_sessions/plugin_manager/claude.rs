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
    PluginInstructionStep, PluginInstructions, ShellCommandRunner, compare_versions,
    is_warp_plugin_source,
};
use crate::terminal::shell::ShellType;

const PLUGIN_KEY: &str = "warp@claude-code-warp";
/// The env var the Claude CLI honors when locating its config/state directory.
const CLAUDE_CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
/// Overrides where the bundled marketplace folder is looked up (unit tests, app tests and
/// dev builds that run outside an installed bundle). Unset in a normal install.
pub(crate) const BUNDLE_DIR_ENV: &str = "PHOSPHOR_CLAUDE_PLUGIN_DIR";
/// The marketplace's `name` in its `marketplace.json`. Kept from the original plugin so an
/// existing install's `warp@claude-code-warp` key stays valid.
const MARKETPLACE_NAME: &str = "claude-code-warp";
/// Where the plugin lives inside the app's resources (`resources/bundled/claude_plugin` in
/// the repo): a folder that is a complete Claude Code marketplace.
const BUNDLE_RELATIVE: &str = "bundled/claude_plugin";

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

/// Set once this process has *completed* a move attempt (the bundle was found and the
/// remove/add ran, whatever its outcome). After it, `needs_source_migration` stays `false`,
/// so the move chip is not offered again in this process: a move that left a stale record,
/// or failed, is retried after a restart, not on every render. A move with no bundle to move
/// to does not set it -- nothing was attempted.
static SOURCE_MOVE_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Logged at most once per process (see [`note_settings_still_declare_warp`]).
static SETTINGS_NOTE_LOGGED: AtomicBool = AtomicBool::new(false);

/// Serialises this process's install, update and move: the footer chip and
/// the background launch paths can otherwise run `marketplace remove` / `add`
/// over each other. Each operation re-checks its precondition once it holds
/// the lock.
static OPERATION_LOCK: LazyLock<futures::lock::Mutex<()>> =
    LazyLock::new(|| futures::lock::Mutex::new(()));

/// The plugin version shipped in `resources/bundled/claude_plugin` (a unit test pins this to
/// that folder's `plugin.json` and `marketplace.json`). Plugin updates arrive with Phosphor
/// releases, so this moves in step with the event parser in `cli_agent_sessions/event`.
const MINIMUM_PLUGIN_VERSION: &str = "2.4.1";

/// The folder holding the plugin that ships with this build, if it can be found:
/// `$PHOSPHOR_CLAUDE_PLUGIN_DIR`, else `<bundled resources>/bundled/claude_plugin`. `None`
/// when that folder has no `.claude-plugin/marketplace.json`. Phosphor never fetches the
/// plugin from anywhere, so without it install/update fail with a clear message.
pub(crate) fn bundled_marketplace_dir() -> Option<PathBuf> {
    let candidate = match env::var_os(BUNDLE_DIR_ENV).filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => warp_core::paths::bundled_resources_dir()?.join(BUNDLE_RELATIVE),
    };
    candidate
        .join(".claude-plugin")
        .join("marketplace.json")
        .is_file()
        .then_some(candidate)
}

fn bundle_missing_error(mut log: String) -> PluginInstallError {
    log.push_str(
        "The Claude Code plugin that ships with Phosphor was not found in the app's resources. \
         Phosphor does not download it; reinstall Phosphor.\n",
    );
    PluginInstallError {
        message: "The bundled Claude Code plugin was not found".to_owned(),
        log,
    }
}

pub(super) struct ClaudeCodePluginManager {
    runner: Arc<dyn PluginCommandRunner>,
}

impl ClaudeCodePluginManager {
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
        self.runner.run("claude", args, &[], log).await
    }

    /// Puts the marketplace back on `original` (its recorded source before this operation
    /// removed it) after the add of the bundled folder failed. Only a local-folder source is
    /// restored: a remote one would mean fetching it, which Phosphor never does.
    async fn restore_marketplace(&self, original: Option<&Value>, log: &mut String) {
        let Some(path) = original.and_then(local_source_path) else {
            log.push_str(
                "Restore: the previous marketplace source is not a local folder and is not \
                 re-fetched; run the update again\n",
            );
            return;
        };
        log.push_str(&format!(
            "Restore: re-registering the marketplace from its previous folder {path}\n"
        ));
        let _ = self
            .run_logged(&["plugin", "marketplace", "remove", MARKETPLACE_NAME], log)
            .await;
        let restored = self
            .run_logged(&["plugin", "marketplace", "add", path.as_str()], log)
            .await
            .is_ok()
            && self
                .run_logged(&["plugin", "install", PLUGIN_KEY], log)
                .await
                .is_ok();
        log.push_str(if restored {
            "Restore: the previous marketplace and plugin are back\n"
        } else {
            "Restore failed: the plugin is not installed; run the update again\n"
        });
    }

    /// `marketplace remove` + `add` of the bundled folder + `plugin install`, restoring the
    /// previous source when the add or install fails. Every step is local.
    async fn register_bundle_and_install(
        &self,
        bundle: &Path,
        original: Option<&Value>,
        log: &mut String,
    ) -> Result<(), PluginInstallError> {
        let bundle_arg = bundle.to_string_lossy().into_owned();
        let _ = self
            .run_logged(&["plugin", "marketplace", "remove", MARKETPLACE_NAME], log)
            .await;
        let added = match self
            .run_logged(&["plugin", "marketplace", "add", bundle_arg.as_str()], log)
            .await
        {
            Ok(()) => {
                self.run_logged(&["plugin", "install", PLUGIN_KEY], log)
                    .await
            }
            Err(err) => Err(err),
        };
        if let Err(err) = added {
            self.restore_marketplace(original, log).await;
            return Err(PluginInstallError {
                message: err.message,
                log: log.clone(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl CliAgentPluginManager for ClaudeCodePluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        MINIMUM_PLUGIN_VERSION
    }

    fn can_auto_install(&self) -> bool {
        true
    }

    fn is_installed(&self) -> bool {
        let Ok(claude_dir) = claude_home_dir() else {
            return false;
        };
        check_installed(&claude_dir)
    }

    fn has_local_marketplace_override(&self) -> bool {
        let Ok(claude_dir) = claude_home_dir() else {
            return false;
        };
        claude_code_marketplace_has_local_override(&claude_dir)
    }

    /// Registers the bundled folder as a local marketplace and installs the plugin from it.
    ///
    /// A developer's local override is installed from as it is. A marketplace registered from
    /// anywhere remote (Warp's GitHub, an older GitLab copy) or from a stale bundle folder is
    /// replaced by the bundled one first, so nothing is ever fetched over the network.
    async fn install(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let claude_dir = claude_home_dir().ok();
        if claude_dir.as_deref().is_some_and(check_installed) {
            // Installed meanwhile (another install held the lock).
            return Ok(());
        }
        let Some(bundle) = bundled_marketplace_dir() else {
            return Err(bundle_missing_error(log));
        };
        let registered = claude_dir
            .as_deref()
            .and_then(registered_marketplace_source);
        match registered.as_ref() {
            None => {
                let bundle_arg = bundle.to_string_lossy().into_owned();
                self.run_logged(
                    &["plugin", "marketplace", "add", bundle_arg.as_str()],
                    &mut log,
                )
                .await?;
            }
            Some(source) if source_is_current_bundle(source, &bundle) => {}
            Some(source) if source_is_developer_local(source) => {}
            Some(_) => {
                return self
                    .register_bundle_and_install(&bundle, registered.as_ref(), &mut log)
                    .await;
            }
        }
        self.run_logged(&["plugin", "install", PLUGIN_KEY], &mut log)
            .await?;
        Ok(())
    }

    /// Version update: re-registers the bundled folder (which carries this build's plugin
    /// version) and reinstalls from it. A developer's local override is left alone.
    async fn update(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let claude_dir = claude_home_dir().ok();
        if claude_dir
            .as_deref()
            .is_some_and(|dir| check_installed(dir) && !plugin_is_outdated(dir))
        {
            // Updated meanwhile (another update held the lock).
            return Ok(());
        }
        if claude_dir
            .as_deref()
            .is_some_and(claude_code_marketplace_has_local_override)
        {
            log.push_str(
                "The claude-code-warp marketplace is a local override; update it from that \
                 checkout. Left as it is.\n",
            );
            return Err(PluginInstallError {
                message: "Local marketplace override left as it is".to_owned(),
                log,
            });
        }
        let Some(bundle) = bundled_marketplace_dir() else {
            return Err(bundle_missing_error(log));
        };
        let original = claude_dir
            .as_deref()
            .and_then(registered_marketplace_source);
        // `marketplace remove` unlinks the plugin, so this uses `plugin install` (not
        // `plugin update`, which would fail with "Plugin is not installed").
        self.register_bundle_and_install(&bundle, original.as_ref(), &mut log)
            .await?;

        // Sanity check: verify the on-disk version actually changed.
        let still_outdated = claude_home_dir()
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
    /// old install location) to the bundled folder, restoring a local previous source if the
    /// add or install fails.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let claude_dir = claude_home_dir().ok();
        let bundle = bundled_marketplace_dir();
        if !claude_dir
            .as_deref()
            .is_some_and(|dir| plugin_needs_source_migration(dir, bundle.as_deref()))
        {
            // Moved meanwhile (another move held the lock), or nothing to move.
            return Ok(());
        }
        let Some(bundle) = bundle else {
            return Err(bundle_missing_error(log));
        };
        let original = claude_dir
            .as_deref()
            .and_then(registered_marketplace_source);
        let result = self
            .register_bundle_and_install(&bundle, original.as_ref(), &mut log)
            .await;
        SOURCE_MOVE_ATTEMPTED.store(true, Ordering::Relaxed);
        if let Some(dir) = claude_dir.as_deref() {
            note_settings_still_declare_warp(dir);
        }
        result
    }

    fn install_success_message(&self) -> &'static str {
        "Notification plugin installed. Please run /reload-plugins to activate."
    }

    fn update_success_message(&self) -> &'static str {
        "Notification plugin updated. Please run /reload-plugins to activate."
    }

    fn move_success_message(&self) -> &'static str {
        "Notification plugin moved to the copy bundled with Phosphor. Please run /reload-plugins to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &UPDATE_INSTRUCTIONS
    }

    /// Version only: the move has its own chip.
    fn needs_update(&self) -> bool {
        let Ok(claude_dir) = claude_home_dir() else {
            return false;
        };
        plugin_is_outdated(&claude_dir)
    }

    fn needs_source_migration(&self) -> bool {
        if SOURCE_MOVE_ATTEMPTED.load(Ordering::Relaxed) {
            return false;
        }
        let Ok(claude_dir) = claude_home_dir() else {
            return false;
        };
        let files = [
            claude_dir.join("plugins").join("known_marketplaces.json"),
            claude_dir.join("plugins").join("installed_plugins.json"),
            // Read for the local-override exclusion.
            claude_dir.join("settings.json"),
        ];
        let bundle = bundled_marketplace_dir();
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || {
            let needed = plugin_needs_source_migration(&claude_dir, bundle.as_deref());
            if !needed {
                note_settings_still_declare_warp(&claude_dir);
            }
            needed
        })
    }
}

/// Forgets a move attempt, for tests that drive several in one process.
#[cfg(test)]
pub(super) fn reset_source_move_attempt() {
    SOURCE_MOVE_ATTEMPTED.store(false, Ordering::Relaxed);
}

/// `claude plugin marketplace add <bundled folder>`, as a command a user can paste. The
/// folder is resolved once; a build run outside an install shows a placeholder path.
fn add_bundle_command() -> &'static str {
    static COMMAND: LazyLock<&'static str> = LazyLock::new(|| {
        let folder = bundled_marketplace_dir()
            .map(|dir| dir.display().to_string())
            .unwrap_or_else(|| format!("<Phosphor install folder>/resources/{BUNDLE_RELATIVE}"));
        Box::leak(format!("claude plugin marketplace add \"{folder}\"").into_boxed_str())
    });
    *COMMAND
}

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-claude-install-title"),
    subtitle: crate::t_static!("cli-agent-plugin-claude-install-subtitle"),
    steps: vec![
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-claude-add-marketplace-step"),
            command: add_bundle_command(),
            executable: true,
            link: None,
        },
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-install-warp-plugin-step"),
            command: "claude plugin install warp@claude-code-warp",
            executable: true,
            link: None,
        },
    ],
    post_install_notes: vec![
        crate::t_static!("cli-agent-plugin-claude-restart-note"),
        crate::t_static!("cli-agent-plugin-claude-known-issues-note"),
    ],
});

static UPDATE_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-claude-update-title"),
    subtitle: crate::t_static!("cli-agent-plugin-run-following-commands"),
    steps: vec![
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-remove-existing-marketplace-step"),
            command: "claude plugin marketplace remove claude-code-warp",
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
            description: crate::t_static!("cli-agent-plugin-install-latest-version-step"),
            command: "claude plugin install warp@claude-code-warp",
            executable: true,
            link: None,
        },
    ],
    post_install_notes: vec![crate::t_static!(
        "cli-agent-plugin-claude-restart-update-note"
    )],
});

fn check_installed(claude_dir: &Path) -> bool {
    check_plugin_installed(claude_dir, PLUGIN_KEY)
}

/// Whether the installed plugin is below the minimum version. `needs_update`
/// adds the source half (`needs_source_migration`).
fn plugin_is_outdated(claude_dir: &Path) -> bool {
    match installed_version(claude_dir) {
        Some(v) => compare_versions(&v, MINIMUM_PLUGIN_VERSION).is_lt(),
        // No version field means very old plugin.
        None => check_installed(claude_dir),
    }
}

/// Whether the installed plugin's marketplace is registered from somewhere other than the
/// bundled folder: a remote repository (Warp's GitHub, an older GitLab copy) or a bundle
/// folder at an old install location. A developer's own local folder is never moved, and
/// with no bundled folder to move to there is nothing to offer.
fn plugin_needs_source_migration(claude_dir: &Path, bundle: Option<&Path>) -> bool {
    let Some(bundle) = bundle else {
        return false;
    };
    if !check_installed(claude_dir) || claude_code_marketplace_has_local_override(claude_dir) {
        return false;
    }
    registered_marketplace_source(claude_dir)
        .is_some_and(|source| !source_is_current_bundle(&source, bundle))
}

/// Whether the `claude-code-warp` marketplace is registered from Warp's GitHub
/// repository (`warpdotdev/claude-code-warp`) rather than Phosphor's copy.
///
/// Decided from `plugins/known_marketplaces.json`, the record of what the CLI
/// actually registered and cloned. GitHub shorthand is recorded as
/// `{"source": "github", "repo": "warpdotdev/claude-code-warp"}`, a git URL as
/// `{"source": "git", "url": "..."}`. A Warp entry still *declared* in
/// `settings.json` `extraKnownMarketplaces` (say, one the user wrote, or a
/// project or managed scope) does not count: Phosphor never rewrites that
/// file, so counting it would re-run the move on every launch; it is logged
/// once instead ([`note_settings_still_declare_warp`]).
///
/// A local-directory override is never reported, so a developer's local
/// marketplace is left alone exactly as `has_local_marketplace_override`
/// promises.
fn marketplace_registered_from_warp(claude_dir: &Path) -> bool {
    if claude_code_marketplace_has_local_override(claude_dir) {
        return false;
    }
    registered_marketplace_source(claude_dir)
        .is_some_and(|source| marketplace_source_is_warp(&source))
}

/// The `source` recorded for the marketplace in `known_marketplaces.json`.
fn registered_marketplace_source(claude_dir: &Path) -> Option<Value> {
    read_json(&claude_dir.join("plugins").join("known_marketplaces.json"))?
        .get(MARKETPLACE_NAME)?
        .get("source")
        .cloned()
}

/// Whether `settings.json` still declares the marketplace from Warp's GitHub.
fn settings_declare_warp(claude_dir: &Path) -> bool {
    read_json(&claude_dir.join("settings.json"))
        .and_then(|settings| {
            settings
                .get("extraKnownMarketplaces")?
                .get(MARKETPLACE_NAME)?
                .get("source")
                .cloned()
        })
        .is_some_and(|source| marketplace_source_is_warp(&source))
}

/// Logs, once per process, that `settings.json` still declares the
/// marketplace from Warp's GitHub although the registered one is not (or no
/// longer) Warp's. Phosphor leaves the user's settings alone; this says why a
/// Claude Code reconcile could bring Warp's source back.
fn note_settings_still_declare_warp(claude_dir: &Path) {
    if marketplace_registered_from_warp(claude_dir) || !settings_declare_warp(claude_dir) {
        return;
    }
    if !SETTINGS_NOTE_LOGGED.swap(true, Ordering::Relaxed) {
        log::info!(
            "[PLUGIN_INSTALL] {} still declares extraKnownMarketplaces[\"{MARKETPLACE_NAME}\"] \
             from warpdotdev/claude-code-warp; Phosphor does not edit it. Remove that entry to \
             stop Claude Code re-registering the warpdotdev copy over the network.",
            claude_dir.join("settings.json").display()
        );
    }
}

/// The folder a recorded marketplace source points at, when the source is a local folder
/// (`{"source": "directory", "path": ...}`, or a bare local path string).
fn local_source_path(source: &Value) -> Option<String> {
    match source {
        Value::Object(source) => {
            let kind = source.get("source").and_then(Value::as_str);
            let path = source.get("path").and_then(Value::as_str);
            if matches!(kind, Some("directory" | "file")) {
                path.filter(|path| !path.is_empty()).map(str::to_owned)
            } else {
                None
            }
        }
        Value::String(source) if is_local_marketplace_path(source) => Some(source.clone()),
        _ => None,
    }
}

/// Whether `path` is where a Phosphor install keeps the plugin
/// (`.../bundled/claude_plugin`), at this install location or an older one.
fn looks_like_bundle_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.trim_end_matches('/').ends_with(BUNDLE_RELATIVE)
}

fn same_folder(recorded: &str, bundle: &Path) -> bool {
    let trim = |p: &str| p.trim_end_matches(['/', '\\']).to_owned();
    let canonical = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    trim(recorded) == trim(&bundle.to_string_lossy())
        || canonical(Path::new(recorded)) == canonical(bundle)
}

/// Whether a recorded source is the bundled folder of this install.
fn source_is_current_bundle(source: &Value, bundle: &Path) -> bool {
    local_source_path(source).is_some_and(|path| same_folder(&path, bundle))
}

/// Whether a recorded source is a local folder that is NOT a Phosphor bundle: a developer's
/// own checkout, which Phosphor leaves alone.
fn source_is_developer_local(source: &Value) -> bool {
    local_source_path(source).is_some_and(|path| !looks_like_bundle_path(&path))
}

fn marketplace_source_is_warp(source: &Value) -> bool {
    match source {
        Value::Object(source) => ["repo", "url"].iter().any(|field| {
            source
                .get(*field)
                .and_then(Value::as_str)
                .is_some_and(|location| is_warp_plugin_source(location, MARKETPLACE_NAME))
        }),
        Value::String(source) => is_warp_plugin_source(source, MARKETPLACE_NAME),
        _ => false,
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn check_plugin_installed(claude_dir: &Path, plugin_key: &str) -> bool {
    let plugins_path = claude_dir.join("plugins").join("installed_plugins.json");
    let Ok(contents) = fs::read_to_string(plugins_path) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };
    parsed
        .get("plugins")
        .and_then(|p| p.get(plugin_key))
        .and_then(|v| v.as_array())
        .map(|arr| !arr.is_empty())
        .unwrap_or(false)
}

/// Reads the installed version string for the Zap plugin, if present.
fn installed_version(claude_dir: &Path) -> Option<String> {
    installed_plugin_version(claude_dir, PLUGIN_KEY)
}

fn installed_plugin_version(claude_dir: &Path, plugin_key: &str) -> Option<String> {
    let plugins_path = claude_dir.join("plugins").join("installed_plugins.json");
    let contents = fs::read_to_string(plugins_path).ok()?;
    let parsed: Value = serde_json::from_str(&contents).ok()?;
    parsed
        .get("plugins")?
        .get(plugin_key)?
        .as_array()?
        .first()?
        .get("version")?
        .as_str()
        .map(|s| s.to_owned())
}

/// Whether the `claude-code-warp` marketplace is overridden to a local
/// filesystem path rather than a Git repository: declared so in
/// `settings.json` `extraKnownMarketplaces`, or registered so in
/// `plugins/known_marketplaces.json` (`claude plugin marketplace add
/// ./checkout` records it only there). Used to avoid clobbering a developer's
/// local marketplace during install/update flows.
///
/// Reading `known_marketplaces.json` too fixes a bug that predates the source
/// move: an override registered only there was not seen, so `update()`'s
/// remove + re-add replaced it with the public repository.
fn claude_code_marketplace_has_local_override(claude_dir: &Path) -> bool {
    let declared = read_json(&claude_dir.join("settings.json")).is_some_and(|settings| {
        settings
            .get("extraKnownMarketplaces")
            .and_then(|marketplaces| marketplaces.get(MARKETPLACE_NAME))
            .is_some_and(marketplace_entry_has_local_path)
    });
    let registered = read_json(&claude_dir.join("plugins").join("known_marketplaces.json"))
        .is_some_and(|known| {
            known
                .get(MARKETPLACE_NAME)
                .is_some_and(marketplace_entry_has_local_path)
        });
    declared || registered
}

/// A local-folder entry that is a developer's own, not one of Phosphor's bundle folders.
fn marketplace_entry_has_local_path(entry: &Value) -> bool {
    entry.get("source").is_some_and(source_is_developer_local)
}

fn is_local_marketplace_path(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("~/")
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with("file://")
}

/// Resolves the dir the Claude CLI reads/writes its state from.
///
/// Honors `CLAUDE_CONFIG_DIR` (the variable the Claude CLI itself respects, and
/// the one this app's Claude harness already writes to), falling back to
/// `~/.claude`. Must match where `claude plugin install` writes, else
/// install/verify checks read the wrong dir.
fn claude_home_dir() -> io::Result<PathBuf> {
    let configured = env::var(CLAUDE_CONFIG_DIR_ENV).unwrap_or_default();
    if !configured.is_empty() {
        return Ok(PathBuf::from(configured));
    }
    dirs::home_dir()
        .map(|home| home.join(".claude"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine home directory",
            )
        })
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
