pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod deepseek;
pub(crate) mod gemini;
pub(crate) mod opencode;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use parking_lot::Mutex;
use warp_util::path::ShellDialect;
use warpui::r#async::Timer;

use crate::features::FeatureFlag;
use crate::terminal::CLIAgent;
use crate::terminal::model::session::LocalCommandExecutor;
use crate::terminal::shell::ShellType;
use claude::ClaudeCodePluginManager;
use codex::CodexPluginManager;
use deepseek::DeepSeekPluginManager;
use gemini::GeminiPluginManager;
use opencode::OpenCodePluginManager;

/// Distinguishes whether the plugin instructions modal should show install or update steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginModalKind {
    Install,
    Update,
    /// Moving an install off Warp's source onto Phosphor's GitLab copy.
    Move,
}

/// A single step in the plugin install/update instructions pane.
pub(crate) struct PluginInstructionStep {
    pub description: &'static str,
    pub command: &'static str,
    /// When true, the code block shows a "Run" button that inserts the command into the terminal.
    /// Defaults-by-convention to `true`; set to `false` for steps that are not runnable
    /// (e.g. config file snippets).
    pub executable: bool,
    /// Optional URL rendered as a clickable "Learn more" link after the description.
    /// When set with an empty `command`, the code block is omitted entirely.
    pub link: Option<&'static str>,
}

/// All content needed to render the plugin instructions pane for a given agent.
pub(crate) struct PluginInstructions {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub steps: Vec<PluginInstructionStep>,
    /// Displayed after the steps in the same style as the subtitle, one per paragraph.
    pub post_install_notes: Vec<&'static str>,
}

/// Error returned when plugin installation fails.
/// Carries both a short user-facing message (for the toast) and a detailed
/// command log (for the log file the user can inspect).
pub(crate) struct PluginInstallError {
    /// Short description shown in the toast notification.
    pub message: String,
    /// Detailed log of every command/step that was attempted.
    pub log: String,
}

impl fmt::Display for PluginInstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl PluginInstallError {
    /// Whether the operation stopped before changing anything because
    /// Phosphor's source did not answer the pre-flight. Such a failure is not
    /// a reason to switch the chip to manual instructions: those run the same
    /// commands without a pre-flight, and the chip works again once the
    /// source answers.
    pub(crate) fn left_install_unchanged(&self) -> bool {
        self.message == SOURCE_UNREACHABLE_MESSAGE
    }
}

impl From<io::Error> for PluginInstallError {
    fn from(err: io::Error) -> Self {
        let msg = err.to_string();
        Self {
            message: msg.clone(),
            log: msg,
        }
    }
}

/// Compares two `X.Y.Z` version strings.
/// Returns `Ordering::Less` if `a < b`, etc.
/// Unparseable components are treated as 0.
pub(crate) fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |s: &str| -> [u64; 3] {
        let mut parts = s.splitn(3, '.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        [major, minor, patch]
    };
    parse(a).cmp(&parse(b))
}

// The CLI-agent notification plugins are fetched from Phosphor's GitLab group
// (`gitlab.com/winters-phosphor`), whose repositories are full-history mirrors
// of the Warp repositories of the same names (MIT notices kept). Marketplace
// names, plugin keys and the OSC 777 wire protocol are unchanged; only where the
// agent CLIs fetch from moved. Each manager holds its own URL. See
// `DECLINED.md`, "CLI agent notification plugins".

/// The GitHub owner the plugins used to be fetched from.
const WARP_PLUGIN_OWNER: &str = "warpdotdev";

/// Whether `source` names Warp's GitHub copy of the plugin repository `repo`
/// (`warpdotdev/<repo>`), in any spelling an agent CLI records for it, and
/// only those, anchored at the start:
///
/// - GitHub `owner/repo` shorthand, optionally `github:`-prefixed;
/// - `(git+)http(s)://(www.)github.com/owner/repo`;
/// - `(git+)ssh://git@github.com/owner/repo`;
/// - `git@github.com:owner/repo`;
///
/// each optionally with `.git` and a `#ref`, `@ref`, `/` or `?` suffix.
///
/// An install registered from such a source keeps receiving Warp's updates
/// until it is re-registered from Phosphor's GitLab copy, which is what the
/// managers' `needs_source_migration` reports. A similarly named repository
/// (`claude-code-warp-internal`), a mirror whose path merely contains
/// `github.com/warpdotdev/...` (`https://git.corp/mirrors/github.com/...`), and
/// a local or `file://` path never match: the move removes what it matches.
pub(crate) fn is_warp_plugin_source(source: &str, repo: &str) -> bool {
    let source = source.trim().to_ascii_lowercase();
    let shorthand = format!("{WARP_PLUGIN_OWNER}/{}", repo.to_ascii_lowercase());
    let source = source.strip_prefix("git+").unwrap_or(&source);
    let path = if let Some(rest) = source.strip_prefix("github:") {
        Some(rest)
    } else if let Some(rest) = source
        .strip_prefix("https://")
        .or_else(|| source.strip_prefix("http://"))
    {
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        rest.strip_prefix("github.com/")
    } else if let Some(rest) = source.strip_prefix("ssh://git@github.com/") {
        Some(rest)
    } else if let Some(rest) = source.strip_prefix("git@github.com:") {
        Some(rest)
    } else if source.contains(':') || source.starts_with(['/', '.', '~', '\\']) {
        // Some other scheme (`file:`, `gitlab:`, another host) or a path.
        None
    } else {
        Some(source)
    };
    let Some(rest) = path.and_then(|path| path.strip_prefix(shorthand.as_str())) else {
        return false;
    };
    let rest = rest.strip_prefix(".git").unwrap_or(rest);
    rest.is_empty() || rest.starts_with(['/', '#', '@', '?'])
}

/// Memoises a check derived from a few small config files, keyed on each
/// file's modification time and length.
///
/// `needs_source_migration` is asked from the agent footer's render path, where
/// a connected, up-to-date plugin is the common case, so re-reading and
/// re-parsing the agent's settings on every frame would be pure overhead. A
/// `stat` per file is all a repeat call costs; any write to one of the files
/// (the agent CLI re-registering its marketplace, say) changes the key and the
/// check runs again.
pub(crate) struct ConfigFileCheckCache {
    last: Mutex<Option<(Vec<FileStamp>, bool)>>,
}

type FileStamp = (PathBuf, Option<(SystemTime, u64)>);

impl ConfigFileCheckCache {
    pub(crate) const fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    pub(crate) fn get_or_compute(&self, files: &[PathBuf], compute: impl FnOnce() -> bool) -> bool {
        let stamps: Vec<FileStamp> = files
            .iter()
            .map(|path| {
                let stamp = fs::metadata(path)
                    .ok()
                    .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
                (path.clone(), stamp)
            })
            .collect();
        let mut last = self.last.lock();
        if let Some((cached_stamps, value)) = last.as_ref()
            && *cached_stamps == stamps
        {
            return *value;
        }
        let value = compute();
        *last = Some((stamps, value));
        value
    }
}

/// Runs the agent CLIs (and `git`) for the plugin managers. The shell-backed
/// [`ShellCommandRunner`] is what the app uses; tests substitute a scripted one
/// so `install()` / `update()` can be driven without running any CLI.
#[async_trait]
pub(crate) trait PluginCommandRunner: Send + Sync {
    /// Runs `cli_name args...` with `env` added to the environment, appending
    /// the command and its output to `log`. Every argument reaches the CLI as
    /// one word, whatever it contains.
    async fn run(
        &self,
        cli_name: &str,
        args: &[&str],
        env: &[(&str, &str)],
        log: &mut String,
    ) -> Result<(), PluginInstallError>;
}

/// Runs plugin commands through the session's login shell, with the session's
/// `PATH` when one was captured (nvm-installed CLIs are only on it there).
pub(crate) struct ShellCommandRunner {
    executor: LocalCommandExecutor,
    dialect: ShellDialect,
    path_env_var: Option<String>,
}

impl ShellCommandRunner {
    pub(crate) fn new(
        shell_path: Option<PathBuf>,
        shell_type: Option<ShellType>,
        path_env_var: Option<String>,
    ) -> Self {
        let shell_type = shell_type.unwrap_or(ShellType::Bash);
        Self {
            executor: LocalCommandExecutor::new(shell_path, shell_type),
            dialect: ShellDialect::from(shell_type),
            path_env_var,
        }
    }
}

#[async_trait]
impl PluginCommandRunner for ShellCommandRunner {
    async fn run(
        &self,
        cli_name: &str,
        args: &[&str],
        env: &[(&str, &str)],
        log: &mut String,
    ) -> Result<(), PluginInstallError> {
        let mut env_vars: HashMap<String, String> = env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        if let Some(path) = self.path_env_var.as_deref() {
            env_vars.insert("PATH".to_owned(), path.to_owned());
        }
        let env_vars = (!env_vars.is_empty()).then_some(env_vars);
        run_cli_command_logged(cli_name, args, self.dialect, &self.executor, env_vars, log).await
    }
}

/// How long the pre-flight reachability check may take before the move is
/// abandoned. `git`'s own low-speed limit (below) ends a stalled transfer;
/// this also bounds a connect that never answers.
const SOURCE_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(20);

/// Pre-flight for a source move: whether Phosphor's repository at `url`
/// answers `git ls-remote` from this machine, through the same runner (and so
/// the same shell and `PATH`) the move itself would use.
///
/// Every move (and Claude's remove-and-re-add update) removes or rewrites a
/// working install before fetching the new one, so a blocked or offline
/// gitlab.com must stop it before anything changes: on `false` the caller
/// leaves the current install exactly as it is. The agents' own fetch also
/// needs `git`, so a missing `git` correctly fails here too.
/// `GIT_TERMINAL_PROMPT=0` keeps git from ever waiting on a credential prompt.
pub(crate) async fn plugin_source_reachable(
    runner: &dyn PluginCommandRunner,
    url: &str,
    log: &mut String,
) -> bool {
    log.push_str(&format!(
        "Pre-flight: checking that {url} is reachable before changing anything\n"
    ));
    let outcome = {
        let check = async {
            Some(
                runner
                    .run(
                        "git",
                        &[
                            "-c",
                            "credential.interactive=never",
                            "-c",
                            "http.lowSpeedLimit=1",
                            "-c",
                            "http.lowSpeedTime=15",
                            "ls-remote",
                            "--exit-code",
                            url,
                            "HEAD",
                        ],
                        &[("GIT_TERMINAL_PROMPT", "0")],
                        log,
                    )
                    .await,
            )
        };
        let timeout = async {
            Timer::after(SOURCE_PREFLIGHT_TIMEOUT).await;
            None
        };
        futures_lite::future::or(check, timeout).await
    };
    match outcome {
        Some(Ok(())) => true,
        Some(Err(_)) => {
            log.push_str(&format!(
                "Pre-flight failed: {url} is not reachable; the current install was left as it is\n"
            ));
            false
        }
        None => {
            log.push_str(&format!(
                "Pre-flight timed out after {}s reaching {url}; the current install was left as it is\n",
                SOURCE_PREFLIGHT_TIMEOUT.as_secs()
            ));
            false
        }
    }
}

/// The message of [`source_unreachable_error`].
const SOURCE_UNREACHABLE_MESSAGE: &str =
    "Phosphor's plugin source is unreachable; the installed plugin was left as it is";

/// The error a manager returns when the pre-flight fails. Nothing was
/// changed, so the caller may offer the same operation again
/// ([`PluginInstallError::left_install_unchanged`]).
pub(crate) fn source_unreachable_error(log: String) -> PluginInstallError {
    PluginInstallError {
        message: SOURCE_UNREACHABLE_MESSAGE.to_owned(),
        log,
    }
}

/// The command line for `cli_name args...` in `dialect`, every argument
/// escaped as one word. Restore arguments come from the user's own config
/// (a recorded marketplace source, a path with spaces), so none is trusted
/// to be a plain word.
pub(crate) fn shell_command_line(cli_name: &str, args: &[&str], dialect: ShellDialect) -> String {
    std::iter::once(cli_name)
        .chain(args.iter().copied())
        .map(|word| dialect.escape(word).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs a CLI subcommand through [`LocalCommandExecutor`], appending the
/// command and its output to `log`.
pub(crate) async fn run_cli_command_logged(
    cli_name: &str,
    args: &[&str],
    dialect: ShellDialect,
    executor: &LocalCommandExecutor,
    env_vars: Option<HashMap<String, String>>,
    log: &mut String,
) -> Result<(), PluginInstallError> {
    let display_cmd = shell_command_line(cli_name, args, dialect);
    log.push_str(&format!("$ {display_cmd}\n"));
    let result = executor
        .execute_local_command_in_login_shell(&display_cmd, None, env_vars)
        .await;
    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            for stream in [&stdout, &stderr] {
                if stream.is_empty() {
                    continue;
                }
                log.push_str(stream);
                if !stream.ends_with('\n') {
                    log.push('\n');
                }
            }
            if output.success() {
                log.push('\n');
                return Ok(());
            }
            Err(PluginInstallError {
                message: format!("'{display_cmd}' failed"),
                log: log.to_owned(),
            })
        }
        Err(err) => {
            log.push_str(&format!("error: {err}\n"));
            Err(PluginInstallError {
                message: format!("failed to run '{display_cmd}'"),
                log: log.clone(),
            })
        }
    }
}

/// Manages the Zap notification plugin for a specific CLI agent.
///
/// Each supported CLI agent has its own implementation that knows how to
/// check installation state and perform install/update operations.
///
/// # No Oz-platform-plugin surface here
///
/// The pin's version of this trait carries a second, parallel set of methods for
/// each agent's *Oz platform* plugin — `install_platform_plugin`,
/// `update_platform_plugin`, `is_platform_plugin_installed`,
/// `platform_plugin_needs_update`, installing `oz-harness-support@claude-code-warp`
/// and `orchestration@codex-warp`. **They are deliberately absent here, not
/// unported** — those plugins' skills and hooks shell out to `oz harness-support …`
/// and `oz run message …`, both of which are hosted-backend command surfaces this
/// fork removed. See `DECLINED.md`, "Oz platform plugins", for the evidence and the
/// decision; do not re-add these methods without a consumer that can actually use
/// the skills they install.
#[async_trait]
pub(crate) trait CliAgentPluginManager: Send + Sync {
    /// The minimum plugin version required by this Zap build.
    fn minimum_plugin_version(&self) -> &'static str;

    /// Whether this agent supports one-click auto-install/update.
    /// When `false`, the footer always opens the manual instructions modal.
    fn can_auto_install(&self) -> bool;

    /// Whether the Zap notification plugin is installed.
    /// Default returns `false` (no filesystem check).
    fn is_installed(&self) -> bool {
        false
    }

    /// Whether the on-disk plugin version is below the minimum required.
    /// Default returns `false` (no filesystem check).
    fn needs_update(&self) -> bool {
        false
    }

    /// Whether the installed plugin is still registered from Warp's GitHub
    /// (or npm) source rather than Phosphor's GitLab copy, so that it keeps
    /// taking Warp's updates until it is re-registered. Drives only the
    /// footer's dedicated move chip; it is *not* part of `needs_update()`, so
    /// the background launch paths never act on it. Implementations cache it
    /// (the footer asks on render) and stop reporting it once this process has
    /// completed a move attempt. Never true for a local-path override.
    /// Default returns `false`.
    fn needs_source_migration(&self) -> bool {
        false
    }

    /// Moves an install registered from Warp's source onto Phosphor's GitLab
    /// copy. Run only when the user clicks the move chip -- never from a
    /// background path. Implementations take the agent's operation lock,
    /// re-check that a move is still needed, change nothing unless Phosphor's
    /// copy answers a pre-flight, and restore the previous source if the move
    /// fails part-way. Default does nothing.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        Ok(())
    }

    /// Toast message shown after a successful move.
    fn move_success_message(&self) -> &'static str {
        "Notification plugin moved to Phosphor's source. Please restart the session to activate."
    }

    /// Manual instructions for the move, shown when the move chip cannot run
    /// it itself (remote or sandboxed session, custom command, earlier
    /// failure). Defaults to [`Self::update_instructions`].
    fn move_instructions(&self) -> &'static PluginInstructions {
        self.update_instructions()
    }

    /// Whether the agent's plugin marketplace is currently overridden to a
    /// local filesystem path. This is used by local test flows to avoid
    /// clobbering a developer's marketplace override while still preserving
    /// normal install/update behavior in staging and production.
    fn has_local_marketplace_override(&self) -> bool {
        false
    }

    /// Install the Zap notification plugin.
    /// Default returns an error — only agents with `can_auto_install() == true` should override.
    async fn install(&self) -> Result<(), PluginInstallError> {
        Err(PluginInstallError {
            message: "Auto-install not supported for this agent".to_owned(),
            log: String::new(),
        })
    }

    /// Update the Zap notification plugin to the latest version.
    /// Default returns an error — only agents with `can_auto_install() == true` should override.
    async fn update(&self) -> Result<(), PluginInstallError> {
        Err(PluginInstallError {
            message: "Auto-update not supported for this agent".to_owned(),
            log: String::new(),
        })
    }

    /// Toast message shown after a successful auto-install.
    fn install_success_message(&self) -> &'static str {
        "Notification plugin installed. Please restart the session to activate."
    }

    /// Toast message shown after a successful auto-update.
    fn update_success_message(&self) -> &'static str {
        "Notification plugin updated. Please restart the session to activate."
    }

    /// Manual installation instructions for the modal UI.
    fn install_instructions(&self) -> &'static PluginInstructions;

    /// Whether this agent supports version-based update checking.
    /// When `false`, the update chip is never shown; only the install chip appears.
    fn supports_update(&self) -> bool {
        true
    }

    /// Manual update instructions for the modal UI.
    fn update_instructions(&self) -> &'static PluginInstructions;
}

/// Returns a plugin manager for the given CLI agent, or `None` if the agent
/// doesn't have Zap notification plugin support.
pub(crate) fn plugin_manager_for(agent: CLIAgent) -> Option<Box<dyn CliAgentPluginManager>> {
    plugin_manager_for_with_shell(agent, None, None, None)
}
/// Returns a plugin manager for the given CLI agent, or `None` if the agent
/// doesn't have Zap notification plugin support.
///
/// When a shell path and type are provided, plugin commands run through that shell.
/// When `path_env_var` is provided, it is set as the PATH for plugin commands
/// (needed for nvm-installed tools that are only on PATH in interactive shells).
pub(crate) fn plugin_manager_for_with_shell(
    agent: CLIAgent,
    shell_path: Option<PathBuf>,
    shell_type: Option<ShellType>,
    path_env_var: Option<String>,
) -> Option<Box<dyn CliAgentPluginManager>> {
    match agent {
        CLIAgent::Claude => Some(Box::new(ClaudeCodePluginManager::new(
            shell_path,
            shell_type,
            path_env_var,
        ))),
        CLIAgent::OpenCode
            if FeatureFlag::OpenCodeNotifications.is_enabled()
                && FeatureFlag::HOANotifications.is_enabled() =>
        {
            Some(Box::new(OpenCodePluginManager::new(
                shell_path,
                shell_type,
                path_env_var,
            )))
        }
        CLIAgent::Codex
            if FeatureFlag::CodexNotifications.is_enabled()
                && FeatureFlag::HOANotifications.is_enabled() =>
        {
            Some(Box::new(CodexPluginManager::new(
                shell_path,
                shell_type,
                path_env_var,
            )))
        }
        CLIAgent::Gemini
            if FeatureFlag::GeminiNotifications.is_enabled()
                && FeatureFlag::HOANotifications.is_enabled() =>
        {
            Some(Box::new(GeminiPluginManager::new(
                shell_path,
                shell_type,
                path_env_var,
            )))
        }
        CLIAgent::DeepSeek if FeatureFlag::HOANotifications.is_enabled() => {
            Some(Box::new(DeepSeekPluginManager))
        }
        CLIAgent::OpenCode
        | CLIAgent::Codex
        | CLIAgent::Gemini
        | CLIAgent::DeepSeek
        | CLIAgent::Amp
        | CLIAgent::Droid
        | CLIAgent::Copilot
        | CLIAgent::Pi
        | CLIAgent::Auggie
        | CLIAgent::CursorCli
        | CLIAgent::Goose
        | CLIAgent::Antigravity
        | CLIAgent::Omp
        | CLIAgent::Hermes
        | CLIAgent::Vibe
        | CLIAgent::PhosphorTui
        | CLIAgent::Unknown => None,
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
