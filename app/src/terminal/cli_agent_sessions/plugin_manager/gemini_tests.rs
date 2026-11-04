use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{
    CliAgentPluginManager, EXTENSION_REPO, GeminiPluginManager, MINIMUM_PLUGIN_VERSION,
    check_installed, compare_versions, installed_from_warp, installed_version, plugin_is_outdated,
    plugin_needs_source_migration, reset_source_move_attempt,
};
use crate::terminal::cli_agent_sessions::plugin_manager::tests::{FakeRunner, preflight_command};

#[test]
fn can_auto_install_is_true() {
    assert!(GeminiPluginManager::new(None, None, None).can_auto_install());
}

#[test]
fn minimum_version() {
    assert_eq!(
        GeminiPluginManager::new(None, None, None).minimum_plugin_version(),
        "1.0.0"
    );
}

#[test]
fn install_instructions_has_steps() {
    let instructions = GeminiPluginManager::new(None, None, None).install_instructions();
    assert!(!instructions.steps.is_empty());
    assert!(!instructions.title.is_empty());
}

#[test]
fn update_instructions_has_steps() {
    let instructions = GeminiPluginManager::new(None, None, None).update_instructions();
    assert!(!instructions.steps.is_empty());
    assert!(!instructions.title.is_empty());
}

#[test]
fn installed_when_extension_present() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();

    let json = serde_json::json!({
        "name": "warp",
        "version": "1.0.0",
        "description": "Zap terminal integration for Gemini CLI"
    });
    fs::write(
        ext_dir.join("gemini-extension.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert!(check_installed(dir.path()));
}

#[test]
fn not_installed_when_extension_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!check_installed(dir.path()));
}

#[test]
fn not_installed_when_json_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("gemini-extension.json"), "not json").unwrap();

    assert!(!check_installed(dir.path()));
}

#[test]
fn installed_version_returns_version_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();

    let json = serde_json::json!({
        "name": "warp",
        "version": "1.5.0"
    });
    fs::write(
        ext_dir.join("gemini-extension.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    assert_eq!(installed_version(dir.path()).as_deref(), Some("1.5.0"));
}

#[test]
fn installed_version_returns_none_when_no_version_field() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();

    let json = serde_json::json!({
        "name": "warp"
    });
    fs::write(
        ext_dir.join("gemini-extension.json"),
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
fn needs_update_logic_true_when_version_outdated() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();

    let json = serde_json::json!({
        "name": "warp",
        "version": "0.9.0"
    });
    fs::write(
        ext_dir.join("gemini-extension.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    let needs_update = match installed_version(dir.path()) {
        Some(v) => compare_versions(&v, MINIMUM_PLUGIN_VERSION).is_lt(),
        None => check_installed(dir.path()),
    };
    assert!(needs_update);
}

#[test]
fn needs_update_logic_false_when_version_current() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();

    let json = serde_json::json!({
        "name": "warp",
        "version": "1.0.0"
    });
    fs::write(
        ext_dir.join("gemini-extension.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();

    let needs_update = match installed_version(dir.path()) {
        Some(v) => compare_versions(&v, MINIMUM_PLUGIN_VERSION).is_lt(),
        None => check_installed(dir.path()),
    };
    assert!(!needs_update);
}

#[test]
fn extension_repo_is_phosphors_gitlab_copy() {
    assert_eq!(
        EXTENSION_REPO,
        "https://gitlab.com/winters-phosphor/gemini-cli-warp"
    );
}

#[test]
fn install_instructions_install_from_phosphor_source() {
    let instructions = GeminiPluginManager::new(None, None, None).install_instructions();
    assert_eq!(
        instructions.steps[0].command,
        format!("gemini extensions install {EXTENSION_REPO} --consent")
    );
    assert_eq!(instructions.steps.len(), 1);
}

/// A Gemini CLI home whose `gemini-warp` extension (version `version`) was
/// installed from `source`, plus its extensions directory.
fn gemini_home(source: &str, install_type: &str, version: &str) -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let extensions_dir = home.path().join(".gemini").join("extensions");
    write_extension(&extensions_dir, version);
    write_install_metadata(&extensions_dir, source, install_type);
    (home, extensions_dir)
}

const WARP_REPO: &str = "https://github.com/warpdotdev/gemini-cli-warp";

/// Runs `body` with `GEMINI_CLI_HOME` at `home` and the move not yet attempted.
fn with_gemini_home<T>(home: &Path, body: impl FnOnce() -> T) -> T {
    reset_source_move_attempt();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("GEMINI_CLI_HOME", home) };
    let result = body();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("GEMINI_CLI_HOME") };
    result
}

#[test]
#[serial_test::serial]
fn update_instructions_keep_extensions_update_unless_installed_from_warp() {
    // A linked developer checkout must not be shown an uninstall step.
    let (home, _) = gemini_home("/home/dev/gemini-cli-warp", "link", "1.0.0");
    let command = with_gemini_home(home.path(), || {
        GeminiPluginManager::new(None, None, None)
            .update_instructions()
            .steps[0]
            .command
    });
    assert_eq!(command, "gemini extensions update gemini-warp");
}

#[test]
#[serial_test::serial]
fn move_instructions_reinstall_from_phosphor_source() {
    // `extensions update` would re-fetch from the recorded source (Warp's
    // GitHub), so the move chip's manual steps reinstall instead.
    let (home, _) = gemini_home(WARP_REPO, "git", "1.0.0");
    let (instructions, update_step) = with_gemini_home(home.path(), || {
        let manager = GeminiPluginManager::new(None, None, None);
        (
            manager.move_instructions(),
            manager.update_instructions().steps[0].command,
        )
    });
    assert_eq!(update_step, "gemini extensions update gemini-warp");
    assert_eq!(
        instructions.steps[0].command,
        "gemini extensions uninstall gemini-warp"
    );
    assert_eq!(
        instructions.steps[1].command,
        format!("gemini extensions install {EXTENSION_REPO} --consent")
    );
    assert_eq!(instructions.steps.len(), 2);
}

#[test]
fn installed_from_warp_for_github_git_install() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), "1.0.0");
    write_install_metadata(
        dir.path(),
        "https://github.com/warpdotdev/gemini-cli-warp",
        "git",
    );

    assert!(installed_from_warp(dir.path()));
}

#[test]
fn installed_from_warp_for_github_release_install() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), "1.0.0");
    write_install_metadata(
        dir.path(),
        "https://github.com/warpdotdev/gemini-cli-warp",
        "github-release",
    );

    assert!(installed_from_warp(dir.path()));
}

#[test]
fn not_installed_from_warp_for_phosphor_copy() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), "1.0.0");
    write_install_metadata(dir.path(), EXTENSION_REPO, "git");

    assert!(!installed_from_warp(dir.path()));
}

#[test]
fn local_or_linked_install_is_never_reported_as_warp() {
    for install_type in ["local", "link"] {
        let dir = tempfile::tempdir().unwrap();
        write_extension(dir.path(), "1.0.0");
        write_install_metadata(
            dir.path(),
            "/home/dev/github.com/warpdotdev/gemini-cli-warp",
            install_type,
        );

        assert!(!installed_from_warp(dir.path()), "{install_type}");
    }
}

#[test]
fn not_installed_from_warp_without_install_metadata() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), "1.0.0");

    assert!(!installed_from_warp(dir.path()));
}

#[test]
fn current_extension_from_warp_needs_update() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), MINIMUM_PLUGIN_VERSION);
    write_install_metadata(
        dir.path(),
        "https://github.com/warpdotdev/gemini-cli-warp",
        "git",
    );

    assert!(plugin_needs_source_migration(dir.path()));
    // Current version: it is the source alone that makes `needs_update` true.
    assert!(!plugin_is_outdated(dir.path()));
}

#[test]
fn current_extension_from_phosphor_copy_needs_no_update() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), MINIMUM_PLUGIN_VERSION);
    write_install_metadata(dir.path(), EXTENSION_REPO, "git");

    assert!(!plugin_needs_source_migration(dir.path()));
    assert!(!plugin_is_outdated(dir.path()));
}

#[test]
fn outdated_extension_from_phosphor_copy_still_needs_update() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), "0.9.0");
    write_install_metadata(dir.path(), EXTENSION_REPO, "git");

    assert!(plugin_is_outdated(dir.path()));
}

#[derive(Clone, Copy)]
enum Operation {
    Update,
    Move,
}

/// Runs `operation` on a manager backed by `runner`, returning its result and
/// `needs_source_migration` afterwards.
fn drive(
    home: &Path,
    runner: &Arc<FakeRunner>,
    operation: Operation,
) -> (Result<(), String>, bool) {
    with_gemini_home(home, || {
        let manager = GeminiPluginManager::with_runner(runner.clone());
        let result = futures::executor::block_on(async {
            match operation {
                Operation::Update => manager.update().await,
                Operation::Move => manager.move_to_phosphor_source().await,
            }
        });
        (
            result.map_err(|err| err.message),
            manager.needs_source_migration(),
        )
    })
}

const UNINSTALL: &str = "gemini extensions uninstall gemini-warp";

fn install_command() -> String {
    format!("gemini extensions install {EXTENSION_REPO} --consent")
}

/// What `gemini extensions uninstall gemini-warp` does on disk.
fn uninstall_hook(extensions_dir: PathBuf) -> impl Fn(&str) + Send + Sync + 'static {
    move |command| {
        if command == UNINSTALL {
            fs::remove_dir_all(extensions_dir.join("gemini-warp")).unwrap();
        }
    }
}

#[test]
#[serial_test::serial]
fn move_changes_nothing_when_the_preflight_fails() {
    let (home, extensions_dir) = gemini_home(WARP_REPO, "github-release", "1.0.0");
    let runner = Arc::new(FakeRunner::new().failing("git "));

    let (result, needs_move) = drive(home.path(), &runner, Operation::Move);

    assert!(result.is_err());
    assert_eq!(runner.calls(), vec![preflight_command(EXTENSION_REPO)]);
    assert!(installed_from_warp(&extensions_dir));
    // Nothing was attempted, so the chip is still offered (B3).
    assert!(needs_move);
}

#[test]
#[serial_test::serial]
fn move_puts_the_previous_extension_back_when_the_install_fails() {
    let (home, extensions_dir) = gemini_home(WARP_REPO, "git", "1.0.0");
    let manifest =
        fs::read_to_string(extensions_dir.join("gemini-warp/gemini-extension.json")).unwrap();
    let runner = Arc::new(
        FakeRunner::new()
            .failing(&install_command())
            .with_hook(uninstall_hook(extensions_dir.clone())),
    );

    let (result, _) = drive(home.path(), &runner, Operation::Move);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(EXTENSION_REPO),
            UNINSTALL.to_owned(),
            install_command(),
        ]
    );
    // The working Warp install is back, byte for byte.
    assert!(installed_from_warp(&extensions_dir));
    assert_eq!(
        fs::read_to_string(extensions_dir.join("gemini-warp/gemini-extension.json")).unwrap(),
        manifest
    );
}

#[test]
#[serial_test::serial]
fn move_reinstalls_from_gitlab() {
    let (home, extensions_dir) = gemini_home(WARP_REPO, "github-release", "1.0.0");
    let uninstall = uninstall_hook(extensions_dir.clone());
    let reinstall_dir = extensions_dir.clone();
    let runner = Arc::new(FakeRunner::new().with_hook(move |command| {
        uninstall(command);
        if command == install_command() {
            write_extension(&reinstall_dir, "1.0.0");
            write_install_metadata(&reinstall_dir, EXTENSION_REPO, "git");
        }
    }));

    let (result, needs_move) = drive(home.path(), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            preflight_command(EXTENSION_REPO),
            UNINSTALL.to_owned(),
            install_command(),
        ]
    );
    assert!(!installed_from_warp(&extensions_dir));
    assert!(!needs_move);
}

const UPDATE: &str = "gemini extensions update gemini-warp";

/// What `gemini extensions update gemini-warp` does to the manifest.
fn updates_to(
    extensions_dir: PathBuf,
    version: &'static str,
) -> impl Fn(&str) + Send + Sync + 'static {
    move |command| {
        if command == UPDATE {
            write_extension(&extensions_dir, version);
        }
    }
}

#[test]
#[serial_test::serial]
fn update_from_phosphor_source_runs_extensions_update() {
    let (home, extensions_dir) = gemini_home(EXTENSION_REPO, "git", "0.9.0");
    let runner = Arc::new(FakeRunner::new().with_hook(updates_to(extensions_dir, "1.0.0")));

    let (result, _) = drive(home.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), vec![UPDATE.to_owned()]);
}

#[test]
#[serial_test::serial]
fn update_never_moves_a_warp_install() {
    // The background launch paths call update(); it updates in place.
    let (home, extensions_dir) = gemini_home(WARP_REPO, "git", "0.9.0");
    let runner = Arc::new(FakeRunner::new().with_hook(updates_to(extensions_dir.clone(), "1.0.0")));

    let (result, needs_move) = drive(home.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), vec![UPDATE.to_owned()]);
    assert!(installed_from_warp(&extensions_dir));
    assert!(needs_move, "still offered on the move chip");
}

#[test]
#[serial_test::serial]
fn update_of_a_current_extension_does_nothing() {
    let (home, _) = gemini_home(WARP_REPO, "git", "1.0.0");
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(home.path(), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
}

#[test]
fn each_backup_gets_a_new_directory_and_none_is_reused() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("gemini-extension.json"), "{}").unwrap();

    let first = super::back_up_dir(source.path()).unwrap();
    let second = super::back_up_dir(source.path()).unwrap();

    assert_ne!(first, second);
    assert!(first.join("gemini-extension.json").exists());
    assert!(second.join("gemini-extension.json").exists());
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
}

fn write_extension(extensions_dir: &Path, version: &str) {
    let ext_dir = extensions_dir.join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();
    let json = serde_json::json!({ "name": "gemini-warp", "version": version });
    fs::write(
        ext_dir.join("gemini-extension.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();
}

/// The `.gemini-extension-install.json` Gemini CLI writes next to an installed
/// extension's manifest.
fn write_install_metadata(extensions_dir: &Path, source: &str, install_type: &str) {
    let ext_dir = extensions_dir.join("gemini-warp");
    fs::create_dir_all(&ext_dir).unwrap();
    let json = serde_json::json!({ "source": source, "type": install_type });
    fs::write(
        ext_dir.join(".gemini-extension-install.json"),
        serde_json::to_string(&json).unwrap(),
    )
    .unwrap();
}
