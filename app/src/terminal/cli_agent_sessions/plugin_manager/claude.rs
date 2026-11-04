use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use async_trait::async_trait;
use serde_json::Value;

use super::{
    CliAgentPluginManager, ConfigFileCheckCache, PluginInstallError, PluginInstructionStep,
    PluginInstructions, compare_versions, is_warp_plugin_source, run_cli_command_logged,
};
use crate::terminal::model::session::LocalCommandExecutor;
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

// Keep in sync with the plugin version in winters-phosphor/claude-code-warp.
// (See the Versioning section of that repo's README.)
const MINIMUM_PLUGIN_VERSION: &str = "2.0.0";

pub(super) struct ClaudeCodePluginManager {
    executor: LocalCommandExecutor,
    path_env_var: Option<String>,
}

impl ClaudeCodePluginManager {
    pub(super) fn new(
        shell_path: Option<PathBuf>,
        shell_type: Option<ShellType>,
        path_env_var: Option<String>,
    ) -> Self {
        let shell_type = shell_type.unwrap_or(ShellType::Bash);
        Self {
            executor: LocalCommandExecutor::new(shell_path, shell_type),
            path_env_var,
        }
    }

    async fn run_logged(&self, args: &[&str], log: &mut String) -> Result<(), PluginInstallError> {
        let env_vars = self
            .path_env_var
            .as_deref()
            .map(|path| HashMap::from([("PATH".to_owned(), path.to_owned())]));
        run_cli_command_logged("claude", args, &self.executor, env_vars, log).await
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
    async fn install(&self) -> Result<(), PluginInstallError> {
        let mut log = String::new();
        // A `claude-code-warp` marketplace still registered from Warp's GitHub
        // (plugin uninstalled, marketplace left behind) would keep its source:
        // drop it so the add below registers Phosphor's copy instead.
        if claude_home_dir()
            .ok()
            .is_some_and(|dir| marketplace_registered_from_warp(&dir))
        {
            let _ = self
                .run_logged(
                    &["plugin", "marketplace", "remove", MARKETPLACE_NAME],
                    &mut log,
                )
                .await;
        }
        self.run_logged(&["plugin", "marketplace", "add", MARKETPLACE_URL], &mut log)
            .await?;
        self.run_logged(&["plugin", "install", PLUGIN_KEY], &mut log)
            .await?;
        Ok(())
    }

    async fn update(&self) -> Result<(), PluginInstallError> {
        let mut log = String::new();
        // Remove/re-add the marketplace to ensure the local clone is fresh, then
        // reinstall the plugin. The re-add is also what moves an install
        // registered from Warp's GitHub onto Phosphor's GitLab copy.
        // We use `plugin install` (not `plugin update`) because `marketplace
        // remove` unlinks the plugin, so `plugin update` would fail with
        // "Plugin is not installed".
        let _ = self
            .run_logged(
                &["plugin", "marketplace", "remove", MARKETPLACE_NAME],
                &mut log,
            )
            .await;
        self.run_logged(&["plugin", "marketplace", "add", MARKETPLACE_URL], &mut log)
            .await?;
        self.run_logged(&["plugin", "install", PLUGIN_KEY], &mut log)
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

    fn install_success_message(&self) -> &'static str {
        "Notification plugin installed. Please run /reload-plugins to activate."
    }

    fn update_success_message(&self) -> &'static str {
        "Notification plugin updated. Please run /reload-plugins to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &UPDATE_INSTRUCTIONS
    }

    fn needs_update(&self) -> bool {
        let Ok(claude_dir) = claude_home_dir() else {
            return false;
        };
        plugin_needs_update(&claude_dir)
    }

    fn needs_source_migration(&self) -> bool {
        let Ok(claude_dir) = claude_home_dir() else {
            return false;
        };
        let files = [
            claude_dir.join("settings.json"),
            claude_dir.join("plugins").join("known_marketplaces.json"),
            claude_dir.join("plugins").join("installed_plugins.json"),
        ];
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || plugin_needs_source_migration(&claude_dir))
    }
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

/// Whether `update()` has work to do: the plugin is below the minimum version,
/// or it is installed from a marketplace still registered from Warp's GitHub.
fn plugin_needs_update(claude_dir: &Path) -> bool {
    let outdated = match installed_version(claude_dir) {
        Some(v) => compare_versions(&v, MINIMUM_PLUGIN_VERSION).is_lt(),
        // No version field means very old plugin.
        None => check_installed(claude_dir),
    };
    outdated || plugin_needs_source_migration(claude_dir)
}

fn plugin_needs_source_migration(claude_dir: &Path) -> bool {
    check_installed(claude_dir) && marketplace_registered_from_warp(claude_dir)
}

/// Whether the `claude-code-warp` marketplace is registered from Warp's GitHub
/// repository (`warpdotdev/claude-code-warp`) rather than Phosphor's copy.
///
/// Claude Code records a marketplace's source in two places, and either can be
/// the stale one: the user settings' `extraKnownMarketplaces` (which
/// `plugin marketplace add` declares) and `plugins/known_marketplaces.json`
/// (the installed checkout). GitHub shorthand is recorded as
/// `{"source": "github", "repo": "warpdotdev/claude-code-warp"}`, a git URL as
/// `{"source": "git", "url": "..."}`; older settings may hold a bare string.
///
/// A local-directory override is never reported, so a developer's local
/// marketplace is left alone exactly as `has_local_marketplace_override`
/// promises.
fn marketplace_registered_from_warp(claude_dir: &Path) -> bool {
    if claude_code_marketplace_has_local_override(claude_dir) {
        return false;
    }
    let settings_source = read_json(&claude_dir.join("settings.json")).and_then(|settings| {
        settings
            .get("extraKnownMarketplaces")?
            .get(MARKETPLACE_NAME)?
            .get("source")
            .cloned()
    });
    let known_source = read_json(&claude_dir.join("plugins").join("known_marketplaces.json"))
        .and_then(|known| known.get(MARKETPLACE_NAME)?.get("source").cloned());
    [settings_source, known_source]
        .iter()
        .flatten()
        .any(marketplace_source_is_warp)
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

/// Whether the Claude Code `settings.json` overrides the `claude-code-warp`
/// marketplace to a local filesystem path (`extraKnownMarketplaces`), rather
/// than the canonical Git repository. Used to avoid clobbering a developer's
/// local marketplace override during install/update flows.
fn claude_code_marketplace_has_local_override(claude_dir: &Path) -> bool {
    let settings_path = claude_dir.join("settings.json");
    let Ok(contents) = fs::read_to_string(settings_path) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };

    settings
        .get("extraKnownMarketplaces")
        .and_then(|marketplaces| marketplaces.get(MARKETPLACE_NAME))
        .map(marketplace_entry_has_local_path)
        .unwrap_or(false)
}

fn marketplace_entry_has_local_path(entry: &Value) -> bool {
    let Some(source) = entry.get("source") else {
        return false;
    };
    match source {
        Value::Object(source) => {
            let source_kind = source.get("source").and_then(Value::as_str);
            let path = source.get("path").and_then(Value::as_str);
            source_kind == Some("directory") && path.map(is_local_marketplace_path).unwrap_or(false)
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
