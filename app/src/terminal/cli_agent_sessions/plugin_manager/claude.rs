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
    is_warp_plugin_source, plugin_source_reachable, source_unreachable_error,
};
use crate::terminal::shell::ShellType;

const PLUGIN_KEY: &str = "warp@claude-code-warp";
/// The env var the Claude CLI honors when locating its config/state directory.
const CLAUDE_CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
/// Phosphor's GitLab copy of the marketplace (full-history mirror of
/// `warpdotdev/claude-code-warp`). Claude Code takes a non-GitHub host only as a
/// full git URL ending in `.git`: `owner/repo` shorthand always means
/// github.com, and an `https://` URL without `.git` is fetched as a hosted
/// `marketplace.json` instead of cloned. Keep the two instruction commands
/// below in step with this (a test pins them).
const MARKETPLACE_URL: &str = "https://gitlab.com/winters-phosphor/claude-code-warp.git";
/// The marketplace's `name` in its `marketplace.json`, unchanged by the move.
const MARKETPLACE_NAME: &str = "claude-code-warp";

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

/// Set once this process has *completed* a move attempt (the pre-flight passed
/// and the remove/add ran, whatever its outcome). After it,
/// `needs_source_migration` stays `false`, so the move chip is not offered
/// again in this process: a move that left a stale record, or failed after
/// its rollback, is retried after a restart, not on every render. A failed
/// pre-flight does not set it -- nothing was attempted, so the chip stays.
static SOURCE_MOVE_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Logged at most once per process (see [`note_settings_still_declare_warp`]).
static SETTINGS_NOTE_LOGGED: AtomicBool = AtomicBool::new(false);

/// Serialises this process's install, update and move: the footer chip and
/// the background launch paths can otherwise run `marketplace remove` / `add`
/// over each other. Each operation re-checks its precondition once it holds
/// the lock.
static OPERATION_LOCK: LazyLock<futures::lock::Mutex<()>> =
    LazyLock::new(|| futures::lock::Mutex::new(()));

// Keep in sync with the plugin version in winters-phosphor/claude-code-warp.
// (See the Versioning section of that repo's README.)
const MINIMUM_PLUGIN_VERSION: &str = "2.0.0";

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

    /// Puts the marketplace back on `original` (its recorded source before
    /// this operation removed it) and reinstalls the plugin from it, after the
    /// add from Phosphor's copy failed. Best effort: the outcome is logged.
    async fn restore_marketplace(&self, original: Option<&Value>, log: &mut String) {
        let Some(source) = original.and_then(marketplace_add_argument) else {
            log.push_str(
                "Restore: no recorded marketplace source to restore; run the update again once \
                 gitlab.com is reachable\n",
            );
            return;
        };
        log.push_str(&format!(
            "Restore: re-registering the marketplace from its previous source {source}\n"
        ));
        let _ = self
            .run_logged(&["plugin", "marketplace", "remove", MARKETPLACE_NAME], log)
            .await;
        let restored = self
            .run_logged(&["plugin", "marketplace", "add", source.as_str()], log)
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

    /// `marketplace remove` + `add` from Phosphor's copy + `plugin install`,
    /// restoring the previous source when the add or install fails.
    async fn readd_marketplace(
        &self,
        original: Option<&Value>,
        log: &mut String,
    ) -> Result<(), PluginInstallError> {
        let _ = self
            .run_logged(&["plugin", "marketplace", "remove", MARKETPLACE_NAME], log)
            .await;
        let added = match self
            .run_logged(&["plugin", "marketplace", "add", MARKETPLACE_URL], log)
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

    /// Runs `claude plugin` CLI commands via the session shell.
    ///
    /// Never moves anything: a `claude-code-warp` marketplace already
    /// registered (from Warp's GitHub, or a local override) is installed from
    /// as it is, and the move chip then offers the move.
    async fn install(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let claude_dir = claude_home_dir().ok();
        if claude_dir.as_deref().is_some_and(check_installed) {
            // Installed meanwhile (another install held the lock).
            return Ok(());
        }
        if claude_dir
            .as_deref()
            .and_then(registered_marketplace_source)
            .is_none()
        {
            self.run_logged(&["plugin", "marketplace", "add", MARKETPLACE_URL], &mut log)
                .await?;
        }
        self.run_logged(&["plugin", "install", PLUGIN_KEY], &mut log)
            .await?;
        Ok(())
    }

    /// Version update only -- as before Phosphor moved the source, with
    /// Phosphor's URL for the re-add. Run by the update chip and by the
    /// background launch paths; it never moves a Warp-registered marketplace
    /// on its own: such an install is left to the move chip.
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
            // Pre-existing bug, fixed here: an override registered only in
            // known_marketplaces.json used to be removed by the re-add below.
            log.push_str(
                "The claude-code-warp marketplace is a local override; update it from that \
                 checkout. Left as it is.\n",
            );
            return Err(PluginInstallError {
                message: "Local marketplace override left as it is".to_owned(),
                log,
            });
        }
        let original = claude_dir
            .as_deref()
            .and_then(registered_marketplace_source);
        if original
            .as_ref()
            .is_some_and(marketplace_source_is_phosphor)
        {
            // Remove/re-add the marketplace to ensure the local clone is fresh,
            // then reinstall the plugin. We use `plugin install` (not `plugin
            // update`) because `marketplace remove` unlinks the plugin, so
            // `plugin update` would fail with "Plugin is not installed".
            // The remove is destructive, so it waits for the pre-flight, and
            // the previous source is restored if the add or install fails.
            // Only for Phosphor's own URL: re-adding it changes no source.
            if !plugin_source_reachable(&*self.runner, MARKETPLACE_URL, &mut log).await {
                return Err(source_unreachable_error(log));
            }
            self.readd_marketplace(original.as_ref(), &mut log).await?;
        } else {
            // Any other registration -- Warp's GitHub, a mirror or fork, or a
            // missing/unreadable record -- is updated in place from the source
            // it already has. An update never changes where the plugin comes
            // from (the background launch paths run it unattended); moving
            // off Warp's source is the move chip's job.
            log.push_str(
                "The marketplace is not registered from Phosphor's source; updating it in place\n",
            );
            self.run_logged(
                &["plugin", "marketplace", "update", MARKETPLACE_NAME],
                &mut log,
            )
            .await?;
            // `plugin update` (Claude Code: "Update a plugin to the latest
            // version"): `plugin install` on an installed plugin is a no-op.
            self.run_logged(&["plugin", "update", PLUGIN_KEY], &mut log)
                .await?;
        }

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

    /// Moves a marketplace registered from Warp's GitHub to Phosphor's copy:
    /// pre-flight, `marketplace remove` + `add` + `plugin install`, restoring
    /// the recorded source if the add or install fails.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let claude_dir = claude_home_dir().ok();
        if !claude_dir
            .as_deref()
            .is_some_and(marketplace_registered_from_warp)
        {
            // Moved meanwhile (another move held the lock), or nothing to move.
            return Ok(());
        }
        if !plugin_source_reachable(&*self.runner, MARKETPLACE_URL, &mut log).await {
            return Err(source_unreachable_error(log));
        }
        let original = claude_dir
            .as_deref()
            .and_then(registered_marketplace_source);
        let result = self.readd_marketplace(original.as_ref(), &mut log).await;
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
        "Notification plugin moved to Phosphor's source. Please run /reload-plugins to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &UPDATE_INSTRUCTIONS
    }

    /// Version only, as before the source moved: the move has its own chip.
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
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || {
            let needed = plugin_needs_source_migration(&claude_dir);
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

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-claude-install-title"),
    subtitle: crate::t_static!("cli-agent-plugin-claude-install-subtitle"),
    steps: vec![
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-claude-add-marketplace-step"),
            command: "claude plugin marketplace add https://gitlab.com/winters-phosphor/claude-code-warp.git",
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
            command: "claude plugin marketplace add https://gitlab.com/winters-phosphor/claude-code-warp.git",
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

fn plugin_needs_source_migration(claude_dir: &Path) -> bool {
    check_installed(claude_dir) && marketplace_registered_from_warp(claude_dir)
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
             from warpdotdev/claude-code-warp; Phosphor does not edit it. Point it at \
             {MARKETPLACE_URL} to stop Claude Code re-registering the warpdotdev copy.",
            claude_dir.join("settings.json").display()
        );
    }
}

/// The `claude plugin marketplace add` argument that registers `source` (a
/// recorded marketplace source) again.
fn marketplace_add_argument(source: &Value) -> Option<String> {
    let with_ref = |location: &str, source: &serde_json::Map<String, Value>| match source
        .get("ref")
        .and_then(Value::as_str)
    {
        Some(git_ref) if !git_ref.is_empty() => format!("{location}#{git_ref}"),
        _ => location.to_owned(),
    };
    match source {
        Value::String(location) => Some(location.clone()),
        Value::Object(source) => {
            let field = match source.get("source").and_then(Value::as_str)? {
                "github" => "repo",
                "git" | "url" => "url",
                "directory" | "file" => "path",
                _ => return None,
            };
            let location = source.get(field).and_then(Value::as_str)?;
            Some(with_ref(location, source))
        }
        _ => None,
    }
}

/// Whether a recorded marketplace source is Phosphor's GitLab URL
/// (`{"source": "git", "url": ...}`, or a bare string), with or without
/// `.git`, a trailing `/` or a `#ref`, in any letter case.
fn marketplace_source_is_phosphor(source: &Value) -> bool {
    let location = match source {
        Value::Object(source) => source.get("url").and_then(Value::as_str),
        Value::String(source) => Some(source.as_str()),
        _ => None,
    };
    let normalize = |location: &str| {
        let location = location.trim().to_ascii_lowercase();
        let location = location.split('#').next().unwrap_or_default().to_owned();
        let location = location.trim_end_matches('/');
        location.strip_suffix(".git").unwrap_or(location).to_owned()
    };
    location.is_some_and(|location| normalize(location) == normalize(MARKETPLACE_URL))
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

fn marketplace_entry_has_local_path(entry: &Value) -> bool {
    let Some(source) = entry.get("source") else {
        return false;
    };
    match source {
        Value::Object(source) => {
            // A `directory` (or `file`) source is a path on this machine,
            // whatever its spelling (`C:\...` on Windows included).
            let source_kind = source.get("source").and_then(Value::as_str);
            let path = source.get("path").and_then(Value::as_str);
            matches!(source_kind, Some("directory" | "file"))
                && path.is_some_and(|path| !path.is_empty())
        }
        Value::String(source) => is_local_marketplace_path(source),
        _ => false,
    }
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
