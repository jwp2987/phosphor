use std::ffi::OsString;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

use async_trait::async_trait;
use tempfile::TempDir;
use warp_cli::agent::Harness;

use super::{
    build_local_claude_child_command, build_local_codex_child_command,
    build_local_opencode_child_command, codex_launch_precondition, compose_child_agent_prompt,
    ensure_local_claude_child_plugins, local_claude_child_prompt, normalize_local_child_harness,
    prepare_local_harness_child_launch, split_orchestrate_tasks, validate_local_harness_shell,
};
use crate::ai::local_harness_setup::{
    LOCAL_CODEX_HARNESS_DISABLED_MESSAGE, LocalHarnessSetupState,
};
use crate::terminal::cli_agent_sessions::plugin_manager::{
    CliAgentPluginManager, PluginInstallError, PluginInstructions,
};
use crate::terminal::shell::ShellType;

/// Test-only guard that sets an environment variable for the duration of the
/// guard and restores the previous value (or absence) on drop. Mirrors the
/// pin's `EnvVarGuard` (`local_harness_launch_tests.rs:21-61`, `02b53fcd8`).
struct EnvVarGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl Into<OsString>) -> Self {
        let original = std::env::var_os(key);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var(key, value.into()) };
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(original) = &self.original {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::set_var(self.key, original) };
        } else {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::remove_var(self.key) };
        }
    }
}

/// Writes a no-op executable named `name` into `bin_dir` so `PATH`-based CLI
/// presence checks (`validate_cli_installed`) succeed without a real install.
/// Mirrors the pin's `write_fake_cli` (`local_harness_launch_tests.rs:63-86`,
/// `02b53fcd8`). Being a no-op script also matters for the plugin setup the
/// Claude launch path runs (`ensure_local_claude_child_plugins`): with `HOME`
/// pointed at a temp dir there is no `~/.claude`, so the plugin reads as
/// absent and `install()` is reached, and the fake `claude` binary makes that
/// call a harmless, instant no-op instead of a real plugin-marketplace network
/// round trip.
fn write_fake_cli(bin_dir: &std::path::Path, name: &str) {
    let executable_name = if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        name.to_string()
    };
    let executable_path = bin_dir.join(executable_name);
    let script = if cfg!(windows) {
        "@echo off\r\n"
    } else {
        "#!/bin/sh\n"
    };

    fs::write(&executable_path, script).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&executable_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable_path, permissions).unwrap();
    }
}

/// Adapted from the pin (`app/src/pane_group/pane/local_harness_launch_tests.rs:26-38`,
/// `02b53fcd8`) for #323. The pin asserts `oz run message *`, a client for
/// Warp's cloud-hosted-CLI-task mailbox that this fork physically removed
/// (`crates/warp_cli/src/lib_tests.rs`'s `run_command_is_removed`); this
/// fork's children instead use `oz agent message send`/`list`, the local
/// on-disk mailbox in `crates/warp_cli/src/agent_mailbox.rs`. This test
/// exists so the prompt never again teaches a child a command that errors.
#[test]
fn local_claude_child_prompt_includes_oz_cli_messaging_instructions() {
    let prompt = local_claude_child_prompt("List files");

    assert!(prompt.contains("OZ_CLI"));
    assert!(prompt.contains("OZ_RUN_ID"));
    assert!(prompt.contains("OZ_PARENT_RUN_ID"));
    assert!(prompt.contains("agent message send --sender-run-id"));
    assert!(!prompt.contains("run message send"));
    assert!(prompt.contains("All four send arguments are required"));
    assert!(prompt.contains("Do not pass \"$OZ_PARENT_RUN_ID\" as a positional argument to send"));
    assert!(prompt.contains("agent message list \"$OZ_RUN_ID\" --limit 25"));
    assert!(!prompt.contains("run message list"));
    assert!(prompt.contains("Do not use Claude Code Agent or SendMessage tools"));
    assert!(prompt.ends_with("Task:\nList files"));
}

#[test]
fn normalize_local_child_harness_accepts_supported_aliases() {
    assert_eq!(
        normalize_local_child_harness("claude"),
        Some(Harness::Claude)
    );
    assert_eq!(
        normalize_local_child_harness("claude-code"),
        Some(Harness::Claude)
    );
    assert_eq!(
        normalize_local_child_harness("claude_code"),
        Some(Harness::Claude)
    );
    assert_eq!(
        normalize_local_child_harness("opencode"),
        Some(Harness::OpenCode)
    );
    assert_eq!(
        normalize_local_child_harness("open-code"),
        Some(Harness::OpenCode)
    );
    assert_eq!(
        normalize_local_child_harness("open_code"),
        Some(Harness::OpenCode)
    );
}

#[test]
fn normalize_local_child_harness_rejects_unsupported_values() {
    assert_eq!(normalize_local_child_harness("oz"), None);
    assert_eq!(normalize_local_child_harness(""), None);
}

#[test]
fn normalize_local_child_harness_accepts_codex() {
    // Issue #411's pinned-parity requirement made `Harness::parse_local_child_harness`
    // recognize "codex", so it now parses successfully. #323 completed the launch
    // side (`build_local_codex_child_command`, wired into
    // `prepare_local_harness_child_launch`'s `Harness::Codex` arm), so parsing and
    // launching are both supported now.
    assert_eq!(normalize_local_child_harness("codex"), Some(Harness::Codex));
}

/// #323: the `Harness::Codex` arm of `prepare_local_harness_child_launch`
/// gates on `local_harness_setup_state` (`crate::ai::local_harness_setup`)
/// before launching. That module already has its own unit tests for how it
/// *computes* each `LocalHarnessSetupState`; these three cover the piece it
/// couldn't -- that this call site actually maps each state to the launch
/// decision (refuse and surface a message, or proceed) it's supposed to. A
/// tested module with no caller exercising it was exactly the gap #323
/// exists to close, so a test that only re-exercised the module would repeat
/// the same mistake.
#[test]
fn codex_launch_precondition_allows_launch_when_ready() {
    assert!(codex_launch_precondition(LocalHarnessSetupState::Ready).is_ok());
}

#[test]
fn codex_launch_precondition_refuses_launch_when_product_disabled() {
    let error = codex_launch_precondition(LocalHarnessSetupState::ProductDisabled {
        message: "Local Codex child agents are temporarily disabled.",
    })
    .expect_err("a disabled product state must refuse the launch");

    // Surfaced to the user, not swallowed into a generic/silent failure.
    assert!(
        error
            .to_string()
            .contains("Local Codex child agents are temporarily disabled.")
    );
}

#[test]
fn codex_launch_precondition_refuses_launch_when_cli_missing() {
    let error = codex_launch_precondition(LocalHarnessSetupState::MissingHarness {
        tooltip: "Install Codex to use this local harness.",
    })
    .expect_err("a missing CLI must refuse the launch with the install tooltip");

    // Same install-tooltip text the Claude/OpenCode arms surface for a
    // missing CLI via `validate_cli_installed`, not a second, differently
    // worded mechanism.
    assert!(
        error
            .to_string()
            .contains("Install Codex to use this local harness.")
    );
}

#[test]
fn validate_local_harness_shell_accepts_supported_shells() {
    assert_eq!(validate_local_harness_shell(Some(ShellType::Bash)), Ok(()));
    assert_eq!(validate_local_harness_shell(Some(ShellType::Zsh)), Ok(()));
    assert_eq!(validate_local_harness_shell(Some(ShellType::Fish)), Ok(()));
}

#[test]
fn validate_local_harness_shell_rejects_unsupported_shells() {
    assert_eq!(
        validate_local_harness_shell(Some(ShellType::PowerShell)),
        Err(
            "Local child harnesses currently require bash, zsh, or fish; PowerShell is not supported."
                .to_string()
        )
    );
    assert_eq!(
        validate_local_harness_shell(None),
        Err(
            "Local child harnesses currently require a detected bash, zsh, or fish session."
                .to_string()
        )
    );
}

#[test]
fn build_local_claude_child_command_quotes_the_prompt() {
    let command = build_local_claude_child_command("hello world");

    assert!(command.starts_with("claude --session-id "));
    assert!(command.ends_with(" --dangerously-skip-permissions 'hello world'"));
}

#[test]
fn build_local_opencode_child_command_quotes_the_prompt() {
    assert_eq!(
        build_local_opencode_child_command("hello world"),
        "opencode --prompt 'hello world'"
    );
}

/// Ported from the pin (`local_harness_launch_tests.rs:165-171`, `02b53fcd8`) verbatim, for #323.
#[test]
fn build_local_codex_child_command_quotes_the_prompt() {
    assert_eq!(
        build_local_codex_child_command("hello world"),
        "codex --dangerously-bypass-approvals-and-sandbox 'hello world'"
    );
}
#[test]
fn split_orchestrate_tasks_splits_on_semicolon() {
    assert_eq!(
        split_orchestrate_tasks("write tests; update the docs"),
        vec!["write tests".to_string(), "update the docs".to_string()]
    );
}

#[test]
fn split_orchestrate_tasks_trims_and_drops_empty_segments() {
    // Leading, trailing, and doubled `;` should not produce empty tasks.
    assert_eq!(
        split_orchestrate_tasks("; write tests ;; update the docs ; "),
        vec!["write tests".to_string(), "update the docs".to_string()]
    );
}

#[test]
fn split_orchestrate_tasks_single_task_has_no_semicolon() {
    assert_eq!(
        split_orchestrate_tasks("write tests"),
        vec!["write tests".to_string()]
    );
}

#[test]
fn split_orchestrate_tasks_blank_argument_spawns_nothing() {
    assert_eq!(split_orchestrate_tasks("   "), Vec::<String>::new());
}

#[test]
fn compose_child_agent_prompt_trims_whitespace() {
    assert_eq!(compose_child_agent_prompt("  write tests  "), "write tests");
}

#[test]
fn compose_child_agent_prompt_is_a_verbatim_passthrough() {
    // No parent-transcript summarization or wrapping -- see the doc comment
    // on `compose_child_agent_prompt` for why.
    let task = "Refactor `foo.rs` to use the new API; keep tests green";
    assert_eq!(compose_child_agent_prompt(task), task);
}

/// Adapted from the pin (`local_harness_launch_tests.rs:420-438`, `02b53fcd8`).
/// Drops the pin's `ai_client` argument for the same reason as the tests
/// below (this fork's `prepare_local_harness_child_launch` no longer creates
/// a cloud agent task). Locks in the ordering fix that put the Codex
/// product-disabled/missing-CLI precondition *before*
/// `validate_local_harness_shell`, matching the pin -- `shell_type: None`
/// here means a fork that checked shell support first would surface
/// "your shell isn't supported" instead of the actually-operative
/// "Codex is disabled" message. `FeatureFlag::LocalClaudeCodexChildHarnesses`
/// is off by default, so no override is needed to hit the disabled path.
#[tokio::test]
async fn prepare_local_harness_child_launch_rejects_disabled_codex_before_shell_validation() {
    let result = prepare_local_harness_child_launch(
        "hello world".to_string(),
        "codex".to_string(),
        None,
        Some("parent-run".to_string()),
        None,
        None,
    )
    .await;

    match result {
        Ok(_) => panic!("disabled local codex should be rejected"),
        // `prepare_local_harness_child_launch` wraps `AgentDriverError::HarnessSetupFailed`'s
        // Display output ("Harness 'codex' setup failed: {reason}") via `.to_string()`, so
        // check for the reason as a substring rather than exact equality -- same idiom as
        // `codex_launch_precondition_refuses_launch_when_product_disabled` above.
        Err(err) => assert!(err.contains(LOCAL_CODEX_HARNESS_DISABLED_MESSAGE)),
    }
}

/// Adapted from the pin (`local_harness_launch_tests.rs:320-375`, `02b53fcd8`),
/// for the #2 sweep. Two drops from the pin version: no `ai_client` argument
/// (this fork's `prepare_local_harness_child_launch` no longer creates a
/// cloud agent task -- see `local_child_task_config`'s doc comment above and
/// `local_harness_launch.rs`'s "no longer used" note -- so there is nothing
/// to mock), and no assertion on the exact `run_id` value (the pin's mock
/// returns a fixed uuid; this fork generates one locally with `Uuid::new_v4`,
/// so only its shape is checked). The `ANTHROPIC_MODEL` merge behavior itself
/// (`harness_model_env_vars`) is unchanged from the pin.
#[tokio::test]
#[serial_test::serial]
async fn prepare_local_claude_child_merges_anthropic_model_env_var() {
    let fake_home = TempDir::new().unwrap();
    let fake_bin_dir = TempDir::new().unwrap();
    let working_dir = fake_home.path().join("workspace");
    fs::create_dir_all(&working_dir).unwrap();
    write_fake_cli(fake_bin_dir.path(), "claude");

    let _home = EnvVarGuard::set("HOME", fake_home.path().as_os_str().to_os_string());
    let _path = EnvVarGuard::set("PATH", fake_bin_dir.path().as_os_str().to_os_string());

    let prepared = prepare_local_harness_child_launch(
        "hello world".to_string(),
        "claude".to_string(),
        Some("opus".to_string()),
        Some("parent-run".to_string()),
        Some(ShellType::Zsh),
        Some(working_dir),
    )
    .await
    .unwrap();

    assert_eq!(
        prepared.env_vars.get(&OsString::from("ANTHROPIC_MODEL")),
        Some(&OsString::from("opus"))
    );
    assert!(
        prepared
            .command
            .contains("agent message send --sender-run-id")
    );
    assert!(prepared.command.contains("OZ_PARENT_RUN_ID"));
    assert!(!prepared.run_id.is_empty());
}

/// Adapted from the pin (`local_harness_launch_tests.rs:377-417`, `02b53fcd8`)
/// for the #2 sweep -- same `ai_client` drop as the test above.
#[tokio::test]
#[serial_test::serial]
async fn prepare_local_claude_child_no_anthropic_model_when_empty() {
    let fake_home = TempDir::new().unwrap();
    let fake_bin_dir = TempDir::new().unwrap();
    let working_dir = fake_home.path().join("workspace");
    fs::create_dir_all(&working_dir).unwrap();
    write_fake_cli(fake_bin_dir.path(), "claude");

    let _home = EnvVarGuard::set("HOME", fake_home.path().as_os_str().to_os_string());
    let _path = EnvVarGuard::set("PATH", fake_bin_dir.path().as_os_str().to_os_string());

    let prepared = prepare_local_harness_child_launch(
        "hello world".to_string(),
        "claude".to_string(),
        None,
        Some("parent-run".to_string()),
        Some(ShellType::Zsh),
        Some(working_dir),
    )
    .await
    .unwrap();

    assert!(
        !prepared
            .env_vars
            .contains_key(&OsString::from("ANTHROPIC_MODEL"))
    );
}

/// A `CliAgentPluginManager` whose on-disk state is fixed by the test, which
/// records whether `ensure_local_claude_child_plugins` reached `install` or
/// `update`.
///
/// There is no pin test to port here: at `02b53fcd8` the pin's
/// `ensure_local_claude_child_plugins` has no test of its own, and neither do
/// its callers exercise it -- `local_harness_launch_tests.rs` there covers the
/// prompt, the harness normalizer and the Codex preconditions only. That is
/// how the fork lost the guard silently in the first place, so #600 adds the
/// coverage rather than just the code.
struct FakePluginManager {
    has_local_override: bool,
    needs_update: bool,
    is_installed: bool,
    installs: AtomicUsize,
    updates: AtomicUsize,
}

impl FakePluginManager {
    fn new(has_local_override: bool, needs_update: bool, is_installed: bool) -> Self {
        Self {
            has_local_override,
            needs_update,
            is_installed,
            installs: AtomicUsize::new(0),
            updates: AtomicUsize::new(0),
        }
    }

    /// `(installs, updates)` observed so far.
    fn calls(&self) -> (usize, usize) {
        (
            self.installs.load(Ordering::SeqCst),
            self.updates.load(Ordering::SeqCst),
        )
    }
}

static FAKE_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: "",
    subtitle: "",
    steps: Vec::new(),
    post_install_notes: Vec::new(),
});

#[async_trait]
impl CliAgentPluginManager for FakePluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        "1.0.0"
    }

    fn can_auto_install(&self) -> bool {
        true
    }

    fn is_installed(&self) -> bool {
        self.is_installed
    }

    fn needs_update(&self) -> bool {
        self.needs_update
    }

    fn has_local_marketplace_override(&self) -> bool {
        self.has_local_override
    }

    async fn install(&self) -> Result<(), PluginInstallError> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn update(&self) -> Result<(), PluginInstallError> {
        self.updates.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &FAKE_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &FAKE_INSTRUCTIONS
    }
}

/// The guard #600 restores. A developer who has pointed the
/// `claude-code-warp` marketplace at a local checkout must not have it
/// silently replaced by the public one just because a local Claude child pane
/// launched. Both CLI paths would clobber it, so neither may run -- note this
/// case is deliberately also `needs_update` *and* not installed, i.e. the
/// state that most strongly invites an install.
#[tokio::test]
async fn local_child_plugin_setup_skips_the_cli_entirely_when_the_marketplace_is_overridden() {
    let manager = FakePluginManager::new(true, true, false);

    ensure_local_claude_child_plugins(&manager).await;

    assert_eq!(manager.calls(), (0, 0));
}

#[tokio::test]
async fn local_child_plugin_setup_installs_when_the_plugin_is_absent() {
    let manager = FakePluginManager::new(false, false, false);

    ensure_local_claude_child_plugins(&manager).await;

    assert_eq!(manager.calls(), (1, 0));
}

/// An outdated plugin takes the update path, not the install path: `update()`
/// refreshes the marketplace clone first, which a plain `install()` does not.
#[tokio::test]
async fn local_child_plugin_setup_updates_when_the_plugin_is_outdated() {
    let manager = FakePluginManager::new(false, true, true);

    ensure_local_claude_child_plugins(&manager).await;

    assert_eq!(manager.calls(), (0, 1));
}

/// The second half of #600. Before it, this path shelled out to
/// `claude plugin marketplace add` + `claude plugin install` on every single
/// child-pane launch, current plugin or not.
#[tokio::test]
async fn local_child_plugin_setup_touches_nothing_when_the_plugin_is_current() {
    let manager = FakePluginManager::new(false, false, true);

    ensure_local_claude_child_plugins(&manager).await;

    assert_eq!(manager.calls(), (0, 0));
}
