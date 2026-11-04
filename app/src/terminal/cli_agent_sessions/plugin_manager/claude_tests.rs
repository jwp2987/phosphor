use std::fs;

use std::path::Path;
use std::sync::Arc;

use super::{
    ClaudeCodePluginManager, CliAgentPluginManager, MARKETPLACE_URL, check_installed,
    claude_code_marketplace_has_local_override, installed_version,
    marketplace_registered_from_warp, plugin_is_outdated, plugin_needs_source_migration,
    reset_source_move_attempt,
};
use crate::terminal::cli_agent_sessions::plugin_manager::tests::{FakeRunner, preflight_command};

#[test]
fn installed_when_plugin_present() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    let json = serde_json::json!({
        "plugins": {
            "warp@claude-code-warp": [{"version": "1.0.0"}]
        }
    });
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert!(check_installed(dir.path()));
}

#[test]
fn can_auto_install_is_true() {
    assert!(ClaudeCodePluginManager::new(None, None, None).can_auto_install());
}

#[test]
fn minimum_version() {
    assert_eq!(
        ClaudeCodePluginManager::new(None, None, None).minimum_plugin_version(),
        "2.0.0"
    );
}

#[test]
fn local_marketplace_override_detects_directory_source() {
    let dir = tempfile::tempdir().unwrap();
    let settings = serde_json::json!({
        "extraKnownMarketplaces": {
            "claude-code-warp": {
                "source": {
                    "path": "/Users/example/Developer/claude-code-warp-internal",
                    "source": "directory"
                }
            }
        }
    });
    fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();

    assert!(claude_code_marketplace_has_local_override(dir.path()));
}

#[test]
fn local_marketplace_override_ignores_repo_source() {
    let dir = tempfile::tempdir().unwrap();
    let settings = serde_json::json!({
        "extraKnownMarketplaces": {
            "claude-code-warp": {
                "source": "warpdotdev/claude-code-warp"
            }
        }
    });
    fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();

    assert!(!claude_code_marketplace_has_local_override(dir.path()));
}

#[test]
#[serial_test::serial]
fn local_marketplace_override_via_trait_uses_claude_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let settings = serde_json::json!({
        "extraKnownMarketplaces": {
            "claude-code-warp": {
                "source": {
                    "path": "../claude-code-warp-internal",
                    "source": "directory"
                }
            }
        }
    });
    fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir.path()) };
    let result = ClaudeCodePluginManager::new(None, None, None).has_local_marketplace_override();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

    assert!(result);
}

#[test]
fn not_installed_when_plugin_key_absent() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    let json = serde_json::json!({
        "plugins": {
            "some-other-plugin": [{"version": "1.0.0"}]
        }
    });
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert!(!check_installed(dir.path()));
}

#[test]
fn not_installed_when_plugin_array_empty() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    let json = serde_json::json!({
        "plugins": {
            "warp@claude-code-warp": []
        }
    });
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert!(!check_installed(dir.path()));
}

#[test]
fn not_installed_when_file_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!check_installed(dir.path()));
}

#[test]
fn not_installed_when_json_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();
    fs::write(plugins_dir.join("installed_plugins.json"), "not json").unwrap();

    assert!(!check_installed(dir.path()));
}

#[test]
fn not_installed_when_plugins_key_missing() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    let json = serde_json::json!({"other_key": "value"});
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert!(!check_installed(dir.path()));
}

/// Tests `ClaudeCodePluginManager::is_installed` end-to-end by pointing
/// `CLAUDE_CONFIG_DIR` at a temp directory with a valid installed_plugins.json.
#[test]
#[serial_test::serial]
fn is_installed_via_trait_with_claude_config_dir_env() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    let json = serde_json::json!({
        "plugins": {
            "warp@claude-code-warp": [{"version": "1.0.0"}]
        }
    });
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir.path()) };
    let result = ClaudeCodePluginManager::new(None, None, None).is_installed();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

    assert!(result);
}

#[test]
#[serial_test::serial]
fn not_installed_via_trait_when_claude_config_dir_empty() {
    let dir = tempfile::tempdir().unwrap();

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir.path()) };
    let result = ClaudeCodePluginManager::new(None, None, None).is_installed();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

    assert!(!result);
}

#[test]
fn installed_version_returns_version_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    let json = serde_json::json!({
        "plugins": {
            "warp@claude-code-warp": [{"version": "1.5.0"}]
        }
    });
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert_eq!(installed_version(dir.path()).as_deref(), Some("1.5.0"));
}

#[test]
fn installed_version_returns_none_when_no_version_field() {
    let dir = tempfile::tempdir().unwrap();
    let plugins_dir = dir.path().join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    let json = serde_json::json!({
        "plugins": {
            "warp@claude-code-warp": [{"scope": "user"}]
        }
    });
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert_eq!(installed_version(dir.path()), None);
}

#[test]
fn installed_version_returns_none_when_file_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(installed_version(dir.path()), None);
}

#[test]
fn marketplace_url_is_phosphors_gitlab_copy() {
    // Claude Code clones a non-GitHub host only from a full git URL ending in
    // `.git`; without it the URL is fetched as a hosted marketplace.json.
    assert_eq!(
        MARKETPLACE_URL,
        "https://gitlab.com/winters-phosphor/claude-code-warp.git"
    );
}

#[test]
fn install_instructions_add_marketplace_from_phosphor_source() {
    let instructions = ClaudeCodePluginManager::new(None, None, None).install_instructions();
    assert_eq!(
        instructions.steps[0].command,
        format!("claude plugin marketplace add {MARKETPLACE_URL}")
    );
    assert_eq!(
        instructions.steps[1].command,
        "claude plugin install warp@claude-code-warp"
    );
    assert_eq!(instructions.steps.len(), 2);
}

#[test]
fn update_instructions_readd_marketplace_from_phosphor_source() {
    let instructions = ClaudeCodePluginManager::new(None, None, None).update_instructions();
    assert_eq!(
        instructions.steps[0].command,
        "claude plugin marketplace remove claude-code-warp"
    );
    assert_eq!(
        instructions.steps[1].command,
        format!("claude plugin marketplace add {MARKETPLACE_URL}")
    );
    assert_eq!(
        instructions.steps[2].command,
        "claude plugin install warp@claude-code-warp"
    );
    assert_eq!(instructions.steps.len(), 3);
}

#[test]
fn settings_declaration_alone_is_not_a_warp_registration() {
    // Phosphor never rewrites settings.json, so a Warp entry declared there
    // must not count, or the move would re-run on every launch.
    for source in [
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
        serde_json::json!("warpdotdev/claude-code-warp"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        write_settings_source(dir.path(), source);
        assert!(!marketplace_registered_from_warp(dir.path()));
    }
}

#[test]
fn settings_declaring_warp_do_not_override_a_moved_registration() {
    let dir = tempfile::tempdir().unwrap();
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );

    assert!(!marketplace_registered_from_warp(dir.path()));
}

#[test]
fn registered_from_warp_when_known_marketplaces_record_github_source() {
    // Settings already moved, but the installed checkout is still Warp's.
    let dir = tempfile::tempdir().unwrap();
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    assert!(marketplace_registered_from_warp(dir.path()));
}

#[test]
fn registered_from_warp_when_source_is_github_git_url() {
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({
            "source": "git",
            "url": "https://github.com/warpdotdev/claude-code-warp.git"
        }),
    );

    assert!(marketplace_registered_from_warp(dir.path()));
}

#[test]
fn not_registered_from_warp_when_source_is_phosphor_copy() {
    let dir = tempfile::tempdir().unwrap();
    let source = serde_json::json!({"source": "git", "url": MARKETPLACE_URL});
    write_settings_source(dir.path(), source.clone());
    write_known_marketplace_source(dir.path(), source);

    assert!(!marketplace_registered_from_warp(dir.path()));
}

#[test]
fn not_registered_from_warp_for_similarly_named_repo() {
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp-internal"}),
    );

    assert!(!marketplace_registered_from_warp(dir.path()));
}

#[test]
fn local_directory_override_is_never_reported_as_warp_source() {
    // The checkout record still names Warp's GitHub, but the settings point the
    // marketplace at a local clone: the developer's override wins.
    let dir = tempfile::tempdir().unwrap();
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "directory", "path": "/home/dev/claude-code-warp"}),
    );
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    assert!(!marketplace_registered_from_warp(dir.path()));
}

#[test]
fn not_registered_from_warp_when_marketplace_unknown() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!marketplace_registered_from_warp(dir.path()));
}

#[test]
fn current_plugin_from_warp_source_needs_a_move_not_an_update() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.4.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    assert!(plugin_needs_source_migration(dir.path()));
    // Current version: nothing for `needs_update` (version-only); the move
    // chip alone offers the source change.
    assert!(!plugin_is_outdated(dir.path()));
}

#[test]
fn current_plugin_from_phosphor_source_does_not_need_update() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.4.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );

    assert!(!plugin_needs_source_migration(dir.path()));
    assert!(!plugin_is_outdated(dir.path()));
}

#[test]
fn outdated_plugin_from_phosphor_source_still_needs_update() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "1.5.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );

    assert!(plugin_is_outdated(dir.path()));
}

#[test]
fn warp_marketplace_without_installed_plugin_needs_no_migration() {
    // Not installed: the install path, not the update path, handles it.
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    assert!(marketplace_registered_from_warp(dir.path()));
    assert!(!plugin_needs_source_migration(dir.path()));
}

#[test]
#[serial_test::serial]
fn needs_source_migration_via_trait_uses_claude_config_dir() {
    reset_source_move_attempt();
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.4.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir.path()) };
    let manager = ClaudeCodePluginManager::new(None, None, None);
    let before = (manager.needs_source_migration(), manager.needs_update());
    // Re-registering the marketplace (what `update()` does) clears it, even
    // though the first answer was memoised.
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL, "moved": true}),
    );
    let after = (manager.needs_source_migration(), manager.needs_update());
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

    // `needs_update` is version-only (2.4.0 is current); the move has its own
    // chip.
    assert_eq!(before, (true, false));
    assert_eq!(after, (false, false));
}

const REMOVE: &str = "claude plugin marketplace remove claude-code-warp";
const INSTALL: &str = "claude plugin install warp@claude-code-warp";

fn add_command(source: &str) -> String {
    format!("claude plugin marketplace add {source}")
}

/// A Claude config dir with the plugin at `version` and its marketplace
/// registered from Warp's GitHub, as `claude plugin marketplace add
/// warpdotdev/claude-code-warp` leaves it.
fn warp_registered_claude_dir(version: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), version);
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );
    dir
}

#[derive(Clone, Copy)]
enum Operation {
    Install,
    Update,
    Move,
}

/// Runs `operation` against a manager on `runner` with `CLAUDE_CONFIG_DIR`
/// pointed at `dir` and the move-attempt flag reset, returning its result and
/// the manager's `needs_source_migration` afterwards.
fn drive(dir: &Path, runner: &Arc<FakeRunner>, operation: Operation) -> (Result<(), String>, bool) {
    reset_source_move_attempt();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir) };
    let manager = ClaudeCodePluginManager::with_runner(runner.clone());
    let result = futures::executor::block_on(async {
        match operation {
            Operation::Install => manager.install().await,
            Operation::Update => manager.update().await,
            Operation::Move => manager.move_to_phosphor_source().await,
        }
    });
    let needs_move = manager.needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    (result.map_err(|err| err.message), needs_move)
}

const PLUGIN_UPDATE: &str = "claude plugin update warp@claude-code-warp";
const MARKETPLACE_UPDATE: &str = "claude plugin marketplace update claude-code-warp";

/// A hook that does what `claude plugin update` does to the version record.
/// Only `plugin update` changes the version: `plugin install` on an installed
/// plugin is a no-op, so an in-place update that used it would fail the
/// post-update version check here, as it would for real.
fn updates_version(dir: &Path, version: &'static str) -> impl Fn(&str) + Send + Sync + 'static {
    let dir = dir.to_owned();
    move |command| {
        if command == PLUGIN_UPDATE {
            write_installed_plugin_version(&dir, version);
        }
    }
}

/// A hook that does what `claude plugin install` does to the version record
/// after `marketplace remove` unlinked the plugin.
fn installs_version(dir: &Path, version: &'static str) -> impl Fn(&str) + Send + Sync + 'static {
    let dir = dir.to_owned();
    move |command| {
        if command == INSTALL {
            write_installed_plugin_version(&dir, version);
        }
    }
}

#[test]
#[serial_test::serial]
fn move_changes_nothing_when_the_preflight_fails() {
    let dir = warp_registered_claude_dir("2.4.0");
    let known = fs::read_to_string(dir.path().join("plugins/known_marketplaces.json")).unwrap();
    let runner = Arc::new(FakeRunner::new().failing("git "));

    let (result, needs_move) = drive(dir.path(), &runner, Operation::Move);

    assert!(result.is_err());
    assert_eq!(runner.calls(), vec![preflight_command(MARKETPLACE_URL)]);
    assert_eq!(
        fs::read_to_string(dir.path().join("plugins/known_marketplaces.json")).unwrap(),
        known
    );
    // Nothing was attempted, so the chip is still offered (B3).
    assert!(needs_move);
}

#[test]
#[serial_test::serial]
fn move_restores_the_previous_source_when_the_add_fails() {
    let dir = warp_registered_claude_dir("2.4.0");
    let runner = Arc::new(FakeRunner::new().failing(&add_command(MARKETPLACE_URL)));

    let (result, needs_move) = drive(dir.path(), &runner, Operation::Move);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command("warpdotdev/claude-code-warp"),
            INSTALL.to_owned(),
        ]
    );
    // A completed attempt is not re-offered in this process.
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn move_restores_the_previous_source_when_the_install_fails() {
    let dir = warp_registered_claude_dir("2.4.0");
    let runner = Arc::new(FakeRunner::new().failing(INSTALL));

    let (result, _) = drive(dir.path(), &runner, Operation::Move);

    assert!(result.is_err());
    let calls = runner.calls();
    assert_eq!(
        calls[..4],
        [
            preflight_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command(MARKETPLACE_URL),
            INSTALL.to_owned(),
        ]
    );
    assert_eq!(
        calls[4..6],
        [
            REMOVE.to_owned(),
            add_command("warpdotdev/claude-code-warp")
        ]
    );
}

#[test]
#[serial_test::serial]
fn move_reregisters_the_marketplace_from_gitlab() {
    let dir = warp_registered_claude_dir("2.4.0");
    let claude_dir = dir.path().to_owned();
    let runner = Arc::new(FakeRunner::new().with_hook(move |command| {
        if command == add_command(MARKETPLACE_URL) {
            write_known_marketplace_source(
                &claude_dir,
                serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
            );
        }
    }));

    let (result, needs_move) = drive(dir.path(), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command(MARKETPLACE_URL),
            INSTALL.to_owned(),
        ]
    );
    assert!(!marketplace_registered_from_warp(dir.path()));
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn a_move_whose_record_stays_stale_is_not_reoffered() {
    // The CLI reported success but the record still names Warp: the chip must
    // not come back in this process.
    let dir = warp_registered_claude_dir("2.4.0");
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive(dir.path(), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert!(marketplace_registered_from_warp(dir.path()));
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn concurrent_moves_run_one_at_a_time_and_the_second_finds_nothing_to_move() {
    let dir = warp_registered_claude_dir("2.4.0");
    let claude_dir = dir.path().to_owned();
    let runner = Arc::new(FakeRunner::new().with_hook(move |command| {
        if command == add_command(MARKETPLACE_URL) {
            write_known_marketplace_source(
                &claude_dir,
                serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
            );
        }
    }));
    reset_source_move_attempt();

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", dir.path()) };
    let first = ClaudeCodePluginManager::with_runner(runner.clone());
    let second = ClaudeCodePluginManager::with_runner(runner.clone());
    let (a, b) = futures::executor::block_on(futures::future::join(
        first.move_to_phosphor_source(),
        second.move_to_phosphor_source(),
    ));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

    assert_eq!((a.is_ok(), b.is_ok()), (true, true));
    // One move only: the second re-checked under the lock and stopped.
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command(MARKETPLACE_URL),
            INSTALL.to_owned(),
        ]
    );
}

#[test]
#[serial_test::serial]
fn update_never_moves_a_warp_registered_marketplace() {
    // The background launch paths call update(); it must refresh in place.
    let dir = warp_registered_claude_dir("1.5.0");
    let runner = Arc::new(FakeRunner::new().with_hook(updates_version(dir.path(), "2.4.0")));

    let (result, needs_move) = drive(dir.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![MARKETPLACE_UPDATE.to_owned(), PLUGIN_UPDATE.to_owned()]
    );
    assert!(marketplace_registered_from_warp(dir.path()));
    assert!(needs_move, "still offered on the move chip");
}

/// A corporate mirror or fork is neither Warp's nor Phosphor's: the
/// background update must never swap it for GitLab.
#[test]
#[serial_test::serial]
fn update_keeps_a_mirror_source_in_place() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "1.5.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({
            "source": "git",
            "url": "https://git.corp.example/mirrors/claude-code-warp.git"
        }),
    );
    let known = fs::read_to_string(dir.path().join("plugins/known_marketplaces.json")).unwrap();
    let runner = Arc::new(FakeRunner::new().with_hook(updates_version(dir.path(), "2.4.0")));

    let (result, _) = drive(dir.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![MARKETPLACE_UPDATE.to_owned(), PLUGIN_UPDATE.to_owned()]
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("plugins/known_marketplaces.json")).unwrap(),
        known
    );
}

#[test]
#[serial_test::serial]
fn update_without_a_marketplace_record_updates_in_place() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "1.5.0");
    let runner = Arc::new(FakeRunner::new().with_hook(updates_version(dir.path(), "2.4.0")));

    let (result, _) = drive(dir.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![MARKETPLACE_UPDATE.to_owned(), PLUGIN_UPDATE.to_owned()]
    );
}

/// The no-op the review caught: had the in-place branch used `plugin install`
/// on an installed plugin, the version would not change and the update would
/// report "did not take effect".
#[test]
#[serial_test::serial]
fn an_in_place_update_that_changes_no_version_is_reported() {
    let dir = warp_registered_claude_dir("1.5.0");
    // `plugin update` that installs nothing new.
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), &runner, Operation::Update);

    assert_eq!(result, Err("Plugin update did not take effect".to_owned()));
}

#[test]
fn phosphor_source_spellings_are_recognised() {
    for source in [
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
        serde_json::json!({"source": "git", "url": "https://gitlab.com/winters-phosphor/claude-code-warp"}),
        serde_json::json!({"source": "git", "url": "https://GitLab.com/winters-phosphor/claude-code-warp.git#main"}),
        serde_json::json!(MARKETPLACE_URL),
    ] {
        assert!(super::marketplace_source_is_phosphor(&source), "{source}");
    }
    for source in [
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
        serde_json::json!({"source": "git", "url": "https://git.corp.example/mirrors/claude-code-warp.git"}),
        serde_json::json!({"source": "git", "url": "https://gitlab.com/winters-phosphor/claude-code-warp-internal.git"}),
        serde_json::json!({"source": "directory", "path": "/home/dev/claude-code-warp"}),
    ] {
        assert!(!super::marketplace_source_is_phosphor(&source), "{source}");
    }
}

#[test]
#[serial_test::serial]
fn update_of_a_current_plugin_does_nothing() {
    let dir = warp_registered_claude_dir("2.4.0");
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
}

#[test]
#[serial_test::serial]
fn update_readds_phosphors_marketplace_after_the_preflight() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "1.5.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );
    let runner = Arc::new(FakeRunner::new().with_hook(installs_version(dir.path(), "2.4.0")));

    let (result, _) = drive(dir.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(MARKETPLACE_URL),
            REMOVE.to_owned(),
            add_command(MARKETPLACE_URL),
            INSTALL.to_owned(),
        ]
    );
}

#[test]
#[serial_test::serial]
fn update_of_phosphors_marketplace_waits_for_the_preflight() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "1.5.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );
    let runner = Arc::new(FakeRunner::new().failing("git "));

    let (result, _) = drive(dir.path(), &runner, Operation::Update);

    assert_eq!(runner.calls(), vec![preflight_command(MARKETPLACE_URL)]);
    let message = result.unwrap_err();
    assert!(
        super::super::source_unreachable_error(String::new()).message == message,
        "{message}"
    );
}

/// Pre-existing bug, fixed on this branch: an override registered only in
/// known_marketplaces.json (`claude plugin marketplace add ./checkout`) was
/// invisible to the override check, so update() removed it and re-added the
/// public repository.
#[test]
#[serial_test::serial]
fn update_never_removes_a_local_override_registered_only_in_known_marketplaces() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "1.5.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "directory", "path": "/home/dev/claude-code-warp"}),
    );
    let runner = Arc::new(FakeRunner::new());

    assert!(claude_code_marketplace_has_local_override(dir.path()));
    let (result, needs_move) = drive(dir.path(), &runner, Operation::Update);

    assert!(result.is_err());
    assert!(runner.calls().is_empty());
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn install_over_a_leftover_warp_marketplace_moves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), vec![INSTALL.to_owned()]);
}

#[test]
#[serial_test::serial]
fn plain_install_runs_no_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![add_command(MARKETPLACE_URL), INSTALL.to_owned()]
    );
}

fn write_settings_source(dir: &Path, source: serde_json::Value) {
    let settings = serde_json::json!({
        "extraKnownMarketplaces": {
            "claude-code-warp": { "source": source }
        }
    });
    fs::write(
        dir.join("settings.json"),
        serde_json::to_string(&settings).unwrap(),
    )
    .unwrap();
}

fn write_known_marketplace_source(dir: &Path, source: serde_json::Value) {
    let plugins_dir = dir.join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();
    let known = serde_json::json!({
        "claude-code-warp": {
            "source": source,
            "installLocation": dir.join("plugins/marketplaces/claude-code-warp"),
        }
    });
    fs::write(
        plugins_dir.join("known_marketplaces.json"),
        serde_json::to_string(&known).unwrap(),
    )
    .unwrap();
}

fn write_installed_plugin_version(dir: &Path, version: &str) {
    let plugins_dir = dir.join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();
    let json = serde_json::json!({
        "plugins": {
            "warp@claude-code-warp": [{"version": version}]
        }
    });
    fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();
}
