use std::cmp::Ordering;

use std::cell::Cell;
use std::fs;

use async_trait::async_trait;
use parking_lot::Mutex;

use warp_util::path::ShellDialect;

use super::{
    ConfigFileCheckCache, PluginCommandRunner, PluginInstallError, compare_versions,
    is_warp_plugin_source, plugin_manager_for, plugin_source_reachable, shell_command_line,
};
use crate::terminal::CLIAgent;

#[test]
fn returns_manager_for_claude() {
    assert!(plugin_manager_for(CLIAgent::Claude).is_some());
}

#[test]
fn returns_manager_for_opencode() {
    let _oc_guard = crate::features::FeatureFlag::OpenCodeNotifications.override_enabled(true);
    let _hoa_guard = crate::features::FeatureFlag::HOANotifications.override_enabled(true);
    assert!(plugin_manager_for(CLIAgent::OpenCode).is_some());
}

#[test]
fn returns_manager_for_codex() {
    let _codex_guard = crate::features::FeatureFlag::CodexNotifications.override_enabled(true);
    let _hoa_guard = crate::features::FeatureFlag::HOANotifications.override_enabled(true);
    assert!(plugin_manager_for(CLIAgent::Codex).is_some());
}

#[test]
fn returns_manager_for_gemini() {
    let _gemini_guard = crate::features::FeatureFlag::GeminiNotifications.override_enabled(true);
    let _hoa_guard = crate::features::FeatureFlag::HOANotifications.override_enabled(true);
    assert!(plugin_manager_for(CLIAgent::Gemini).is_some());
}

#[test]
fn returns_none_for_unsupported_agents() {
    assert!(plugin_manager_for(CLIAgent::Amp).is_none());
    assert!(plugin_manager_for(CLIAgent::Droid).is_none());
    assert!(plugin_manager_for(CLIAgent::Copilot).is_none());
    assert!(plugin_manager_for(CLIAgent::Unknown).is_none());
}

#[test]
fn compare_versions_equal() {
    assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
}

#[test]
fn compare_versions_less_than_major() {
    assert_eq!(compare_versions("1.0.0", "2.0.0"), Ordering::Less);
}

#[test]
fn compare_versions_less_than_minor() {
    assert_eq!(compare_versions("1.1.0", "1.2.0"), Ordering::Less);
}

#[test]
fn compare_versions_less_than_patch() {
    assert_eq!(compare_versions("1.1.0", "1.1.1"), Ordering::Less);
}

#[test]
fn compare_versions_greater_than() {
    assert_eq!(compare_versions("3.0.0", "2.0.0"), Ordering::Greater);
}

#[test]
fn compare_versions_unparseable_treated_as_zero() {
    assert_eq!(compare_versions("abc", "0.0.0"), Ordering::Equal);
    assert_eq!(compare_versions("abc", "1.0.0"), Ordering::Less);
}

#[test]
fn compare_versions_partial_version_string() {
    assert_eq!(compare_versions("2", "2.0.0"), Ordering::Equal);
    assert_eq!(compare_versions("2.1", "2.1.0"), Ordering::Equal);
}

#[test]
fn compare_versions_empty_string() {
    assert_eq!(compare_versions("", "2.0.0"), Ordering::Less);
}

#[test]
fn warp_plugin_source_matches_every_recorded_spelling() {
    for source in [
        "warpdotdev/claude-code-warp",
        "WarpDotDev/Claude-Code-Warp",
        "warpdotdev/claude-code-warp#main",
        "warpdotdev/claude-code-warp@v2",
        "github:warpdotdev/claude-code-warp",
        "https://github.com/warpdotdev/claude-code-warp",
        "https://github.com/warpdotdev/claude-code-warp/",
        "https://github.com/warpdotdev/claude-code-warp.git",
        "https://github.com/warpdotdev/claude-code-warp.git#stable",
        "https://www.github.com/warpdotdev/claude-code-warp",
        "git@github.com:warpdotdev/claude-code-warp.git",
        "ssh://git@github.com/warpdotdev/claude-code-warp.git",
        "  https://github.com/warpdotdev/claude-code-warp  ",
        "git+https://github.com/warpdotdev/claude-code-warp.git#v2",
        "git+ssh://git@github.com/warpdotdev/claude-code-warp.git",
        "http://github.com/warpdotdev/claude-code-warp",
    ] {
        assert!(
            is_warp_plugin_source(source, "claude-code-warp"),
            "{source}"
        );
    }
}

#[test]
fn warp_plugin_source_rejects_other_sources() {
    for source in [
        "https://gitlab.com/winters-phosphor/claude-code-warp.git",
        "winters-phosphor/claude-code-warp",
        "warpdotdev/claude-code-warp-internal",
        "https://github.com/warpdotdev/claude-code-warp-internal.git",
        "https://github.com/warpdotdev/codex-warp.git",
        "https://notgithub.com/warpdotdev/claude-code-warp",
        "https://gitlab.com/warpdotdev/claude-code-warp.git",
        "/home/dev/claude-code-warp",
        "",
        // Mirrors and local paths whose path merely contains the GitHub one.
        "https://git.corp/mirrors/github.com/warpdotdev/claude-code-warp.git",
        "file:///home/dev/src/github.com/warpdotdev/claude-code-warp",
        "/home/dev/src/github.com/warpdotdev/claude-code-warp",
        "~/src/github.com/warpdotdev/claude-code-warp",
        "ssh://git@git.corp/github.com/warpdotdev/claude-code-warp.git",
        "gitlab:warpdotdev/claude-code-warp",
    ] {
        assert!(
            !is_warp_plugin_source(source, "claude-code-warp"),
            "{source}"
        );
    }
}

#[test]
fn config_file_check_cache_recomputes_only_when_a_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    fs::write(&file, "{}").unwrap();
    let files = [file.clone(), dir.path().join("missing.json")];
    let cache = ConfigFileCheckCache::new();
    let runs = Cell::new(0);
    let check = |answer: bool| {
        cache.get_or_compute(&files, || {
            runs.set(runs.get() + 1);
            answer
        })
    };

    assert!(check(true));
    // Unchanged files: the memoised answer, without running the check.
    assert!(check(false));
    assert_eq!(runs.get(), 1);

    // A different length changes the key even within one mtime tick.
    fs::write(&file, "{\"moved\": true}").unwrap();
    assert!(!check(false));
    assert_eq!(runs.get(), 2);
}

type FakeHook = Box<dyn Fn(&str) + Send + Sync>;

/// A scripted [`PluginCommandRunner`] for driving the managers' `install()` /
/// `update()` without running any CLI. Records every command line
/// (`"<cli> <args...>"`), fails the ones that start with a registered prefix,
/// and runs a hook for the others so a test can apply the command's on-disk
/// effect (what the real CLI would have written).
pub(crate) struct FakeRunner {
    calls: Mutex<Vec<String>>,
    envs: Mutex<Vec<Vec<(String, String)>>>,
    failing: Vec<String>,
    hook: Option<FakeHook>,
}

impl FakeRunner {
    pub(crate) fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            envs: Mutex::new(Vec::new()),
            failing: Vec::new(),
            hook: None,
        }
    }

    /// Commands starting with `prefix` fail (and their hook does not run).
    pub(crate) fn failing(mut self, prefix: &str) -> Self {
        self.failing.push(prefix.to_owned());
        self
    }

    pub(crate) fn with_hook(mut self, hook: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.hook = Some(Box::new(hook));
        self
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }

    /// The extra environment each call was given, in call order.
    pub(crate) fn envs(&self) -> Vec<Vec<(String, String)>> {
        self.envs.lock().clone()
    }
}

#[async_trait]
impl PluginCommandRunner for FakeRunner {
    async fn run(
        &self,
        cli_name: &str,
        args: &[&str],
        env: &[(&str, &str)],
        log: &mut String,
    ) -> Result<(), PluginInstallError> {
        let command = format!("{cli_name} {}", args.join(" "));
        log.push_str(&format!("$ {command}\n"));
        self.calls.lock().push(command.clone());
        self.envs.lock().push(
            env.iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        );
        if self
            .failing
            .iter()
            .any(|prefix| command.starts_with(prefix.as_str()))
        {
            return Err(PluginInstallError {
                message: format!("'{command}' failed"),
                log: log.clone(),
            });
        }
        if let Some(hook) = &self.hook {
            hook(&command);
        }
        Ok(())
    }
}

/// The pre-flight command line the managers run before a move.
pub(crate) fn preflight_command(url: &str) -> String {
    format!(
        "git -c credential.interactive=never -c http.lowSpeedLimit=1 -c http.lowSpeedTime=15 ls-remote --exit-code {url} HEAD"
    )
}

#[test]
fn preflight_runs_git_ls_remote_through_the_runner() {
    let runner = FakeRunner::new();
    let mut log = String::new();
    let url = "https://gitlab.com/winters-phosphor/codex-warp.git";

    assert!(futures::executor::block_on(plugin_source_reachable(
        &runner, url, &mut log
    )));
    assert_eq!(runner.calls(), vec![preflight_command(url)]);
    // git must never wait on a credential prompt.
    assert_eq!(
        runner.envs(),
        vec![vec![("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned())]]
    );
}

/// Restore arguments come from the user's own config, so every argument is
/// escaped as one word for the session's shell (B5).
#[test]
fn shell_command_line_keeps_each_argument_one_word() {
    let posix = shell_command_line(
        "claude",
        &[
            "plugin",
            "marketplace",
            "add",
            "/home/dev/my plugins/claude code",
        ],
        ShellDialect::Posix,
    );
    assert_eq!(
        posix,
        "claude plugin marketplace add /home/dev/my\\ plugins/claude\\ code"
    );
    assert_eq!(
        shell_words::split(&posix).unwrap(),
        vec![
            "claude",
            "plugin",
            "marketplace",
            "add",
            "/home/dev/my plugins/claude code"
        ]
    );

    let hostile = shell_command_line("codex", &["a; rm -rf ~", "$(id)"], ShellDialect::Posix);
    assert_eq!(
        shell_words::split(&hostile).unwrap(),
        vec!["codex", "a; rm -rf ~", "$(id)"]
    );

    let powershell = shell_command_line(
        "gemini",
        &["extensions", "install", "C:\\Users\\dev\\my ext"],
        ShellDialect::PowerShell,
    );
    assert!(!powershell.contains("my ext"), "{powershell}");
    assert!(
        powershell.starts_with("gemini extensions install "),
        "{powershell}"
    );
}

#[test]
fn shell_command_line_leaves_plain_words_alone() {
    assert_eq!(
        shell_command_line(
            "codex",
            &[
                "plugin",
                "marketplace",
                "add",
                "https://gitlab.com/winters-phosphor/codex-warp.git"
            ],
            ShellDialect::Posix,
        ),
        "codex plugin marketplace add https://gitlab.com/winters-phosphor/codex-warp.git"
    );
}

#[test]
fn preflight_reports_an_unreachable_source() {
    let runner = FakeRunner::new().failing("git ");
    let mut log = String::new();

    assert!(!futures::executor::block_on(plugin_source_reachable(
        &runner,
        "https://gitlab.com/winters-phosphor/codex-warp.git",
        &mut log
    )));
    assert!(log.contains("left as it is"), "{log}");
}
