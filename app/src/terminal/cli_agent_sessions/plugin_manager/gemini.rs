use std::collections::HashMap;
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

/// Phosphor's GitLab copy of the extension (full-history mirror of
/// `warpdotdev/gemini-cli-warp`). `gemini extensions install` treats any
/// `https://` source as git and clones a non-GitHub one with `git clone`
/// (GitHub URLs alone try a release download first). Keep the instruction
/// commands below in step with this (a test pins them).
const EXTENSION_REPO: &str = "https://gitlab.com/winters-phosphor/gemini-cli-warp";
/// The repository name, shared by Warp's and Phosphor's copies.
const EXTENSION_REPO_NAME: &str = "gemini-cli-warp";
/// The extension's `name` in its `gemini-extension.json`, which is also its
/// directory under `~/.gemini/extensions`. Unchanged by the move.
const EXTENSION_NAME: &str = "gemini-warp";
/// Where Gemini CLI records an installed extension's source.
const INSTALL_METADATA_FILE: &str = ".gemini-extension-install.json";

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

// Keep in sync with the plugin version in winters-phosphor/gemini-cli-warp.
const MINIMUM_PLUGIN_VERSION: &str = "1.0.0";

pub(super) struct GeminiPluginManager {
    executor: LocalCommandExecutor,
    path_env_var: Option<String>,
}

impl GeminiPluginManager {
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
        run_cli_command_logged("gemini", args, &self.executor, env_vars, log).await
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
        // The source half goes through the cached trait method: this is asked
        // from the footer's render path too.
        plugin_is_outdated(&extensions_dir) || self.needs_source_migration()
    }

    fn needs_source_migration(&self) -> bool {
        let Ok(extensions_dir) = gemini_extensions_dir() else {
            return false;
        };
        let extension_dir = extensions_dir.join(EXTENSION_NAME);
        let files = [
            extension_dir.join("gemini-extension.json"),
            extension_dir.join(INSTALL_METADATA_FILE),
        ];
        SOURCE_MIGRATION_CHECK
            .get_or_compute(&files, || plugin_needs_source_migration(&extensions_dir))
    }

    async fn install(&self) -> Result<(), PluginInstallError> {
        let mut log = String::new();
        self.run_logged(
            &["extensions", "install", EXTENSION_REPO, "--consent"],
            &mut log,
        )
        .await?;
        Ok(())
    }

    async fn update(&self) -> Result<(), PluginInstallError> {
        let mut log = String::new();
        let move_source = gemini_extensions_dir()
            .ok()
            .is_some_and(|dir| installed_from_warp(&dir));
        if move_source {
            // `extensions update` re-fetches from the recorded source, so an
            // extension installed from Warp's GitHub would stay on it. Gemini
            // CLI has no command that re-points an extension (its own
            // `migratedTo` hand-off must be published by the OLD repository),
            // and `extensions install` refuses an extension that "is already
            // installed. Please uninstall it first." So: uninstall, then
            // install from Phosphor's copy.
            self.run_logged(&["extensions", "uninstall", EXTENSION_NAME], &mut log)
                .await?;
            self.run_logged(
                &["extensions", "install", EXTENSION_REPO, "--consent"],
                &mut log,
            )
            .await?;
        } else {
            self.run_logged(&["extensions", "update", EXTENSION_NAME], &mut log)
                .await?;
        }

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

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-gemini-install-title"),
    subtitle: crate::t_static!("cli-agent-plugin-gemini-run-command-restart"),
    steps: vec![PluginInstructionStep {
        description: crate::t_static!("cli-agent-plugin-install-warp-extension-step"),
        command: "gemini extensions install https://gitlab.com/winters-phosphor/gemini-cli-warp --consent",
        executable: true,
        link: None,
    }],
    post_install_notes: vec![crate::t_static!("cli-agent-plugin-gemini-restart-note")],
});

static UPDATE_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-gemini-update-title"),
    subtitle: crate::t_static!("cli-agent-plugin-gemini-run-command-restart"),
    // Uninstall and reinstall rather than `extensions update`: an update
    // re-fetches from the source the extension was installed from, which for
    // older installs is Warp's GitHub.
    steps: vec![
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-uninstall-warp-extension-step"),
            command: "gemini extensions uninstall gemini-warp",
            executable: true,
            link: None,
        },
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-install-warp-extension-step"),
            command: "gemini extensions install https://gitlab.com/winters-phosphor/gemini-cli-warp --consent",
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

fn plugin_needs_source_migration(extensions_dir: &Path) -> bool {
    check_installed(extensions_dir) && installed_from_warp(extensions_dir)
}

/// Whether the installed extension's recorded source is Warp's GitHub
/// repository. Gemini CLI writes `{"source": ..., "type": ...}` to
/// `.gemini-extension-install.json` in the extension's directory; only a
/// `git` or `github-release` install is reported, so a `local` or `link`
/// install (a developer's checkout) is left alone.
fn installed_from_warp(extensions_dir: &Path) -> bool {
    let metadata_path = extensions_dir
        .join(EXTENSION_NAME)
        .join(INSTALL_METADATA_FILE);
    let Ok(contents) = fs::read_to_string(metadata_path) else {
        return false;
    };
    let Ok(metadata) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };
    let remote = matches!(
        metadata.get("type").and_then(Value::as_str),
        Some("git" | "github-release")
    );
    remote
        && metadata
            .get("source")
            .and_then(Value::as_str)
            .is_some_and(|source| {
                // Warp's repository was planned as `gemini-warp` (the
                // extension's own name) before it shipped as `gemini-cli-warp`.
                is_warp_plugin_source(source, EXTENSION_REPO_NAME)
                    || is_warp_plugin_source(source, EXTENSION_NAME)
            })
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

/// Returns the path to `~/.gemini/extensions`.
fn gemini_extensions_dir() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".gemini").join("extensions"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine home directory",
            )
        })
}

#[cfg(test)]
#[path = "gemini_tests.rs"]
mod tests;
