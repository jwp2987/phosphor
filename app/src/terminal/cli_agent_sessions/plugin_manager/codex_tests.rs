use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::{CodexPluginManager, MARKETPLACE_URL, reset_source_move_attempt};
use crate::features::FeatureFlag;
use crate::terminal::cli_agent_sessions::plugin_manager::CliAgentPluginManager;
use crate::terminal::cli_agent_sessions::plugin_manager::tests::{FakeRunner, preflight_command};

#[test]
fn can_auto_install_is_true() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    assert!(CodexPluginManager::new(None, None, None).can_auto_install());
}

#[test]
fn can_auto_install_is_false_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    assert!(!CodexPluginManager::new(None, None, None).can_auto_install());
}

#[test]
fn install_instructions_are_native_without_codex_plugin() {
    // `title` is filled via `t_static!`, which resolves through the i18n loader once
    // (LazyLock) and permanently caches the raw fluent key if evaluated before
    // `init()`. Under nextest, each test runs in its own process with no loader
    // initialized by default, so the loader must be initialized here to get the
    // real English copy instead of the untranslated key.
    crate::i18n::init(Some("en"));

    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    let instructions = CodexPluginManager::new(None, None, None).install_instructions();
    assert_eq!(
        instructions.title,
        "Enable Phosphor Notifications for Codex"
    );
    assert_eq!(
        instructions.steps[1].command,
        "[tui]\nnotification_condition = \"always\""
    );
}

#[test]
fn supports_update() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    assert!(CodexPluginManager::new(None, None, None).supports_update());
}

#[test]
fn does_not_support_update_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    assert!(!CodexPluginManager::new(None, None, None).supports_update());
}

#[test]
fn minimum_version() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    assert_eq!(
        CodexPluginManager::new(None, None, None).minimum_plugin_version(),
        "0.4.0"
    );
}

#[test]
fn minimum_version_is_zero_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    assert_eq!(
        CodexPluginManager::new(None, None, None).minimum_plugin_version(),
        "0.0.0"
    );
}

#[test]
fn install_instructions_has_marketplace_and_plugin_add_steps() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let instructions = CodexPluginManager::new(None, None, None).install_instructions();
    assert_eq!(
        instructions.steps[0].command,
        "codex plugin marketplace add https://gitlab.com/winters-phosphor/codex-warp.git"
    );
    assert_eq!(
        instructions.steps[0].command,
        format!("codex plugin marketplace add {}", super::MARKETPLACE_URL)
    );
    assert_eq!(
        instructions.steps[1].command,
        "codex plugin add warp@codex-warp"
    );
    assert_eq!(instructions.steps.len(), 2);
    assert!(!instructions.title.is_empty());
}

#[test]
#[serial_test::serial]
fn update_instructions_has_marketplace_and_plugin_add_steps() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let instructions = CodexPluginManager::new(None, None, None).update_instructions();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };
    assert_eq!(
        instructions.steps[0].command,
        "codex plugin marketplace upgrade codex-warp"
    );
    assert_eq!(
        instructions.steps[1].command,
        "codex plugin add warp@codex-warp"
    );
    assert_eq!(instructions.steps.len(), 2);
    assert!(!instructions.title.is_empty());
}

#[test]
#[serial_test::serial]
fn update_instructions_readd_marketplace_from_phosphor_source_when_registered_from_warp() {
    // `marketplace upgrade` would re-fetch from the recorded source (Warp's
    // GitHub), so only then does the manual update re-register it; a local
    // override keeps the upgrade steps above, which do not remove it.
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_git_marketplace(
        dir.path(),
        "https://github.com/warpdotdev/codex-warp.git",
    );
    reset_source_move_attempt();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let instructions = CodexPluginManager::new(None, None, None).update_instructions();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };
    assert_eq!(
        instructions.steps[0].command,
        "codex plugin marketplace remove codex-warp"
    );
    assert_eq!(
        instructions.steps[1].command,
        format!("codex plugin marketplace add {}", super::MARKETPLACE_URL)
    );
    assert_eq!(
        instructions.steps[2].command,
        "codex plugin add warp@codex-warp"
    );
    assert_eq!(instructions.steps.len(), 3);
    assert!(!instructions.title.is_empty());
}

#[test]
#[serial_test::serial]
fn update_instructions_keep_upgrade_steps_for_a_local_override() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    write_marketplace_config(dir.path(), "local");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let instructions = CodexPluginManager::new(None, None, None).update_instructions();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };
    assert_eq!(
        instructions.steps[0].command,
        "codex plugin marketplace upgrade codex-warp"
    );
}

#[test]
fn update_instructions_are_empty_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    let instructions = CodexPluginManager::new(None, None, None).update_instructions();
    assert!(instructions.steps.is_empty());
    assert!(instructions.title.is_empty());
}

#[test]
fn installed_when_plugin_enabled_in_config() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);

    assert!(super::check_installed(dir.path()));
}

#[test]
fn not_installed_when_plugin_disabled_in_config() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, false);

    assert!(!super::check_installed(dir.path()));
}

#[test]
fn not_installed_when_only_marketplace_present() {
    // Marketplace cloned but the plugin was never enabled.
    let dir = tempfile::tempdir().unwrap();
    write_marketplace_config(dir.path(), "git");

    assert!(!super::check_installed(dir.path()));
}

#[test]
fn not_installed_when_config_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!super::check_installed(dir.path()));
}

#[test]
fn not_installed_when_config_invalid() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "not toml").unwrap();

    assert!(!super::check_installed(dir.path()));
}

#[test]
fn installed_version_reads_cache_manifest_version() {
    let dir = tempfile::tempdir().unwrap();
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.4.0");

    assert_eq!(
        super::installed_version(dir.path()).as_deref(),
        Some("0.4.0")
    );
}

#[test]
fn installed_version_picks_latest_cached() {
    let dir = tempfile::tempdir().unwrap();
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.3.0");
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.5.0");

    assert_eq!(
        super::installed_version(dir.path()).as_deref(),
        Some("0.5.0")
    );
}

#[test]
fn installed_version_returns_none_when_cache_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(super::installed_version(dir.path()), None);
}

#[test]
fn installed_version_returns_none_when_cache_manifest_has_no_version() {
    let dir = tempfile::tempdir().unwrap();
    write_cache_manifest_without_version(dir.path(), super::PLUGIN_NAME, "0.4.0");

    assert_eq!(super::installed_version(dir.path()), None);
}

#[test]
fn needs_update_true_when_enabled_and_version_outdated() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.2.0");

    assert!(super::plugin_needs_update(
        dir.path(),
        super::PLUGIN_NAME,
        super::PLUGIN_KEY,
        "0.4.0"
    ));
}

#[test]
fn needs_update_false_when_enabled_and_version_current() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.4.0");

    assert!(!super::plugin_needs_update(
        dir.path(),
        super::PLUGIN_NAME,
        super::PLUGIN_KEY,
        "0.4.0"
    ));
}

#[test]
fn needs_update_false_when_not_enabled() {
    let dir = tempfile::tempdir().unwrap();
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.2.0");

    assert!(!super::plugin_needs_update(
        dir.path(),
        super::PLUGIN_NAME,
        super::PLUGIN_KEY,
        "0.4.0"
    ));
}

#[test]
fn needs_update_true_when_enabled_without_cached_version() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);

    assert!(super::plugin_needs_update(
        dir.path(),
        super::PLUGIN_NAME,
        super::PLUGIN_KEY,
        "0.4.0"
    ));
}

#[test]
#[serial_test::serial]
fn is_not_installed_via_trait_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).is_installed();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(!result);
}

#[test]
#[serial_test::serial]
fn is_installed_via_trait_with_codex_home_env() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).is_installed();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(result);
}

#[test]
#[serial_test::serial]
fn needs_update_via_trait_with_codex_home_env() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.2.0");

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).needs_update();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(result);
}

#[test]
#[serial_test::serial]
fn does_not_need_update_via_trait_when_version_current() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.4.0");

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).needs_update();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(!result);
}

#[test]
#[serial_test::serial]
fn does_not_need_update_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.2.0");

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).needs_update();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(!result);
}

#[test]
#[serial_test::serial]
fn does_not_need_update_when_not_enabled() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).needs_update();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(!result);
}

#[test]
#[serial_test::serial]
fn does_not_need_update_for_non_git_marketplace_override() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    write_marketplace_config(dir.path(), "directory");

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).needs_update();
    let has_override = CodexPluginManager::new(None, None, None).has_local_marketplace_override();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(!result);
    assert!(has_override);
}

#[test]
fn marketplace_from_warp_github_url_is_registered_from_warp() {
    let dir = tempfile::tempdir().unwrap();
    write_git_marketplace_config(dir.path(), "https://github.com/warpdotdev/codex-warp.git");

    let config = super::codex_warp_marketplace_config(dir.path()).unwrap();
    assert!(config.is_git());
    assert!(config.is_registered_from_warp());
}

#[test]
fn marketplace_from_warp_ssh_url_is_registered_from_warp() {
    let dir = tempfile::tempdir().unwrap();
    write_git_marketplace_config(dir.path(), "git@github.com:warpdotdev/codex-warp.git");

    let config = super::codex_warp_marketplace_config(dir.path()).unwrap();
    assert!(config.is_registered_from_warp());
}

#[test]
fn marketplace_from_phosphor_copy_is_not_registered_from_warp() {
    let dir = tempfile::tempdir().unwrap();
    write_git_marketplace_config(dir.path(), super::MARKETPLACE_URL);

    let config = super::codex_warp_marketplace_config(dir.path()).unwrap();
    assert!(config.is_git());
    assert!(!config.is_registered_from_warp());
}

#[test]
fn local_marketplace_is_never_registered_from_warp() {
    // A non-Git override is left alone, whatever its source says.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        "[marketplaces.codex-warp]\nsource_type = \"local\"\nsource = \"/src/github.com/warpdotdev/codex-warp\"\n",
    )
    .unwrap();

    let config = super::codex_warp_marketplace_config(dir.path()).unwrap();
    assert!(!config.is_registered_from_warp());
}

#[test]
fn enabled_plugin_from_warp_marketplace_needs_source_migration() {
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_git_marketplace(
        dir.path(),
        "https://github.com/warpdotdev/codex-warp.git",
    );

    assert!(super::plugin_needs_source_migration(dir.path()));
}

#[test]
fn enabled_plugin_from_phosphor_marketplace_needs_no_source_migration() {
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_git_marketplace(dir.path(), super::MARKETPLACE_URL);

    assert!(!super::plugin_needs_source_migration(dir.path()));
}

#[test]
fn warp_marketplace_without_enabled_plugin_needs_no_source_migration() {
    // Not installed: `install()`'s `ensure_marketplace` moves it instead.
    let dir = tempfile::tempdir().unwrap();
    write_git_marketplace_config(dir.path(), "https://github.com/warpdotdev/codex-warp.git");

    assert!(!super::plugin_needs_source_migration(dir.path()));
}

#[test]
#[serial_test::serial]
fn current_plugin_from_warp_marketplace_needs_update_via_trait() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_git_marketplace(
        dir.path(),
        "https://github.com/warpdotdev/codex-warp.git",
    );
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.4.0");

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let manager = CodexPluginManager::new(None, None, None);
    let before = (manager.needs_source_migration(), manager.needs_update());
    write_enabled_plugin_with_git_marketplace(dir.path(), super::MARKETPLACE_URL);
    let after = (manager.needs_source_migration(), manager.needs_update());
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert_eq!(before, (true, true));
    assert_eq!(after, (false, false));
}

#[test]
#[serial_test::serial]
fn no_source_migration_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_git_marketplace(
        dir.path(),
        "https://github.com/warpdotdev/codex-warp.git",
    );

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir.path()) };
    let result = CodexPluginManager::new(None, None, None).needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };

    assert!(!result);
}

const WARP_URL: &str = "https://github.com/warpdotdev/codex-warp.git";
const REMOVE: &str = "codex plugin marketplace remove codex-warp";
const PLUGIN_ADD: &str = "codex plugin add warp@codex-warp";

fn add_command(source: &str) -> String {
    format!("codex plugin marketplace add {source}")
}

/// Runs `update()` on a manager backed by `runner` with `CODEX_HOME` at
/// `dir`, returning its result and `needs_source_migration` afterwards.
fn drive_update(dir: &Path, runner: &Arc<FakeRunner>) -> (Result<(), String>, bool) {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    reset_source_move_attempt();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CODEX_HOME", dir) };
    let manager = CodexPluginManager::with_runner(runner.clone());
    let result = futures::executor::block_on(manager.update());
    let needs_move = manager.needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CODEX_HOME") };
    (result.map_err(|err| err.message), needs_move)
}

fn warp_registered_codex_home() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_git_marketplace(dir.path(), WARP_URL);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.4.0");
    dir
}

#[test]
#[serial_test::serial]
fn move_changes_nothing_when_the_preflight_fails() {
    let dir = warp_registered_codex_home();
    let config = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    let runner = Arc::new(FakeRunner::new().failing("git "));

    let (result, needs_move) = drive_update(dir.path(), &runner);

    assert!(result.is_err());
    assert_eq!(runner.calls(), vec![preflight_command(MARKETPLACE_URL)]);
    assert_eq!(
        fs::read_to_string(dir.path().join("config.toml")).unwrap(),
        config
    );
    assert!(!needs_move, "attempted once, not retried every launch");
}

#[test]
#[serial_test::serial]
fn move_restores_the_recorded_source_when_the_add_fails() {
    let dir = warp_registered_codex_home();
    let runner = Arc::new(FakeRunner::new().failing(&add_command(MARKETPLACE_URL)));

    let (result, _) = drive_update(dir.path(), &runner);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command(MARKETPLACE_URL),
            add_command(WARP_URL),
        ]
    );
}

#[test]
#[serial_test::serial]
fn move_restore_keeps_the_recorded_ref_and_sparse_paths() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        format!(
            "[marketplaces.codex-warp]\nsource_type = \"git\"\nsource = \"{WARP_URL}\"\nref = \"main\"\nsparse_paths = [\"plugins/warp\"]\n\n[plugins.\"{}\"]\nenabled = true\n",
            super::PLUGIN_KEY
        ),
    )
    .unwrap();
    let runner = Arc::new(FakeRunner::new().failing(&add_command(MARKETPLACE_URL)));

    let (result, _) = drive_update(dir.path(), &runner);

    assert!(result.is_err());
    assert_eq!(
        runner.calls().last().unwrap(),
        &format!("{} --ref main --sparse plugins/warp", add_command(WARP_URL))
    );
}

#[test]
#[serial_test::serial]
fn move_reregisters_the_marketplace_from_gitlab() {
    let dir = warp_registered_codex_home();
    let codex_home = dir.path().to_owned();
    let runner = Arc::new(FakeRunner::new().with_hook(move |command| {
        if command == add_command(MARKETPLACE_URL) {
            write_enabled_plugin_with_git_marketplace(&codex_home, MARKETPLACE_URL);
        }
    }));

    let (result, needs_move) = drive_update(dir.path(), &runner);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command(MARKETPLACE_URL),
            PLUGIN_ADD.to_owned(),
        ]
    );
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn codex_without_marketplace_remove_fails_once_and_keeps_the_install() {
    let dir = warp_registered_codex_home();
    let config = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    let runner = Arc::new(FakeRunner::new().failing(REMOVE));

    let (result, needs_move) = drive_update(dir.path(), &runner);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![preflight_command(MARKETPLACE_URL), REMOVE.to_owned()]
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("config.toml")).unwrap(),
        config
    );
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn update_adds_the_marketplace_when_none_is_configured() {
    // E.g. an earlier move removed it and the add failed: the next update
    // adds Phosphor's instead of running `upgrade` on nothing.
    let dir = tempfile::tempdir().unwrap();
    write_plugin_config(dir.path(), super::PLUGIN_KEY, true);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.4.0");
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive_update(dir.path(), &runner);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![add_command(MARKETPLACE_URL), PLUGIN_ADD.to_owned()]
    );
}

#[test]
#[serial_test::serial]
fn update_upgrades_a_marketplace_already_on_gitlab() {
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_git_marketplace(dir.path(), MARKETPLACE_URL);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.4.0");
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive_update(dir.path(), &runner);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            "codex plugin marketplace upgrade codex-warp".to_owned(),
            PLUGIN_ADD.to_owned()
        ]
    );
}

/// The shape `codex plugin marketplace add <url>` writes, plus the enabled
/// plugin entry `codex plugin add warp@codex-warp` writes.
fn write_enabled_plugin_with_git_marketplace(dir: &Path, source: &str) {
    fs::write(
        dir.join("config.toml"),
        format!(
            "[marketplaces.codex-warp]\nsource_type = \"git\"\nsource = \"{source}\"\n\n[plugins.\"{}\"]\nenabled = true\n",
            super::PLUGIN_KEY
        ),
    )
    .unwrap();
}

fn write_git_marketplace_config(dir: &Path, source: &str) {
    fs::write(
        dir.join("config.toml"),
        format!("[marketplaces.codex-warp]\nsource_type = \"git\"\nsource = \"{source}\"\n"),
    )
    .unwrap();
}

fn write_plugin_config(dir: &Path, plugin_key: &str, enabled: bool) {
    fs::write(
        dir.join("config.toml"),
        format!("[plugins.\"{plugin_key}\"]\nenabled = {enabled}\n"),
    )
    .unwrap();
}

fn write_marketplace_config(dir: &Path, source_type: &str) {
    fs::write(
        dir.join("config.toml"),
        format!(
            "[marketplaces.codex-warp]\nsource_type = \"{source_type}\"\nsource = \"/tmp/codex-warp\"\n"
        ),
    )
    .unwrap();
}

fn write_cache_manifest(dir: &Path, plugin_name: &str, version: &str) {
    write_cache_manifest_json(
        dir,
        plugin_name,
        version,
        serde_json::json!({ "name": plugin_name, "version": version }),
    );
}

fn write_cache_manifest_without_version(dir: &Path, plugin_name: &str, version_dir: &str) {
    write_cache_manifest_json(
        dir,
        plugin_name,
        version_dir,
        serde_json::json!({ "name": plugin_name }),
    );
}

fn write_cache_manifest_json(
    dir: &Path,
    plugin_name: &str,
    version_dir: &str,
    manifest: serde_json::Value,
) {
    let manifest_dir = dir
        .join("plugins")
        .join("cache")
        .join("codex-warp")
        .join(plugin_name)
        .join(version_dir)
        .join(".codex-plugin");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(manifest_dir.join("plugin.json"), manifest.to_string()).unwrap();
}
