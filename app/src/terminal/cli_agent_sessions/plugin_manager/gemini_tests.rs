use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{
    BUNDLE_DIR_ENV, CliAgentPluginManager, GeminiPluginManager, InstallSource,
    MINIMUM_PLUGIN_VERSION, check_installed, compare_versions, install_source, installed_version,
    plugin_is_outdated, plugin_needs_source_migration, reset_source_move_attempt,
};
use crate::terminal::cli_agent_sessions::plugin_manager::tests::FakeRunner;

#[test]
fn can_auto_install_is_true() {
    assert!(GeminiPluginManager::new(None, None, None).can_auto_install());
}

#[test]
fn minimum_version() {
    assert_eq!(
        GeminiPluginManager::new(None, None, None).minimum_plugin_version(),
        MINIMUM_PLUGIN_VERSION
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

/// A folder shaped like the bundled extension, at `<root>/bundled/gemini_plugin`.
fn make_bundle(root: &Path) -> PathBuf {
    let bundle = root.join("bundled").join("gemini_plugin");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("gemini-extension.json"), "{}").unwrap();
    bundle
}

fn shipped_bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/bundled/gemini_plugin")
        .canonicalize()
        .expect("resources/bundled/gemini_plugin exists")
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn shipped_extension_version_matches_the_minimum_version() {
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(shipped_bundle().join("gemini-extension.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"], "gemini-warp");
    assert_eq!(manifest["version"], MINIMUM_PLUGIN_VERSION);
    let hooks: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(shipped_bundle().join("hooks/hooks.json")).unwrap(),
    )
    .unwrap();
    assert!(hooks["hooks"]["AfterAgent"].is_array());
}

#[test]
fn shipped_extension_has_no_warp_metadata_and_no_network_calls() {
    let mut files = Vec::new();
    files_under(&shipped_bundle(), &mut files);
    assert!(!files.is_empty());
    for file in files {
        if file.file_name().unwrap() == "LICENSE" {
            continue; // the upstream MIT notice stays
        }
        let text = fs::read_to_string(&file).unwrap_or_default().to_lowercase();
        for forbidden in [
            "oz-",
            "warpdotdev",
            "warp.dev",
            "http://",
            "https://",
            "curl ",
            "wget ",
            "urllib",
            "/dev/tcp",
            "git clone",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} contains {forbidden:?}",
                file.display()
            );
        }
    }
}

#[test]
fn instructions_use_the_bundled_folder_and_no_remote() {
    let manager = GeminiPluginManager::new(None, None, None);
    let install = manager.install_instructions();
    assert_eq!(install.steps.len(), 1);
    // The update steps are also the move chip's: `extensions update` would re-fetch from
    // the recorded source.
    let update = manager.update_instructions();
    assert_eq!(update.steps.len(), 2);
    assert_eq!(
        update.steps[0].command,
        "gemini extensions uninstall gemini-warp"
    );
    assert_eq!(update.steps[1].command, install.steps[0].command);
    assert_eq!(
        manager.move_instructions().steps[1].command,
        install.steps[0].command
    );
    let add = install.steps[0].command;
    assert!(add.starts_with("gemini extensions install \""), "{add}");
    assert!(add.ends_with("\" --consent"), "{add}");
    assert!(
        add.contains("bundled") || add.contains("resources"),
        "{add}"
    );
    for step in install.steps.iter().chain(update.steps.iter()) {
        let lower = step.command.to_lowercase();
        for remote in ["github", "gitlab", "http", "git@", " update "] {
            assert!(!lower.contains(remote), "{}", step.command);
        }
    }
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
const OLD_PHOSPHOR_REPO: &str = "https://gitlab.com/winters-phosphor/gemini-cli-warp";
const OLD_BUNDLE: &str = "/opt/old-phosphor/resources/bundled/gemini_plugin";

/// Runs `body` with `GEMINI_CLI_HOME` at `home`, the bundled folder at `bundle` (a missing
/// folder when `None`) and the move not yet attempted.
fn with_gemini_home<T>(home: &Path, bundle: Option<&Path>, body: impl FnOnce() -> T) -> T {
    reset_source_move_attempt();
    let missing = home.join("no-bundle-here");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var("GEMINI_CLI_HOME", home);
        std::env::set_var(BUNDLE_DIR_ENV, bundle.unwrap_or(&missing));
    }
    let result = body();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::remove_var("GEMINI_CLI_HOME");
        std::env::remove_var(BUNDLE_DIR_ENV);
    }
    result
}

#[test]
fn install_sources_are_classified_from_the_recorded_metadata() {
    let source_of = |source: &str, install_type: &str| {
        let dir = tempfile::tempdir().unwrap();
        write_extension(dir.path(), "1.0.0");
        write_install_metadata(dir.path(), source, install_type);
        install_source(dir.path())
    };
    assert_eq!(source_of(WARP_REPO, "git"), InstallSource::Remote);
    assert_eq!(
        source_of(WARP_REPO, "github-release"),
        InstallSource::Remote
    );
    assert_eq!(source_of(OLD_PHOSPHOR_REPO, "git"), InstallSource::Remote);
    assert_eq!(
        source_of(OLD_BUNDLE, "local"),
        InstallSource::Bundle(OLD_BUNDLE.to_owned())
    );
    // A developer's checkout: left alone, whatever its path says.
    assert_eq!(
        source_of("/home/dev/github.com/warpdotdev/gemini-cli-warp", "local"),
        InstallSource::DeveloperLocal
    );
    assert_eq!(
        source_of("/home/dev/gemini-cli-warp", "link"),
        InstallSource::DeveloperLocal
    );
    // A link to a bundle path is still a link: uninstalling it would drop the link.
    assert_eq!(source_of(OLD_BUNDLE, "link"), InstallSource::DeveloperLocal);
    let no_metadata = tempfile::tempdir().unwrap();
    write_extension(no_metadata.path(), "1.0.0");
    assert_eq!(install_source(no_metadata.path()), InstallSource::Unknown);
}

#[test]
fn only_an_install_off_the_current_bundle_needs_source_migration() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let needs = |source: &str, install_type: &str, bundle: Option<&Path>| {
        let dir = tempfile::tempdir().unwrap();
        write_extension(dir.path(), MINIMUM_PLUGIN_VERSION);
        write_install_metadata(dir.path(), source, install_type);
        plugin_needs_source_migration(dir.path(), bundle)
    };
    assert!(needs(WARP_REPO, "git", Some(&bundle)));
    assert!(needs(OLD_PHOSPHOR_REPO, "git", Some(&bundle)));
    assert!(needs(OLD_BUNDLE, "local", Some(&bundle)));
    assert!(!needs(&bundle.to_string_lossy(), "local", Some(&bundle)));
    assert!(!needs("/home/dev/gemini-cli-warp", "local", Some(&bundle)));
    assert!(!needs("/home/dev/gemini-cli-warp", "link", Some(&bundle)));
    // With no bundled folder to move to there is nothing to offer.
    assert!(!needs(WARP_REPO, "git", None));
    // Not installed.
    assert!(!plugin_needs_source_migration(
        tempfile::tempdir().unwrap().path(),
        Some(&bundle)
    ));
}

#[test]
fn a_current_extension_needs_no_update_whatever_its_source() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), MINIMUM_PLUGIN_VERSION);
    write_install_metadata(dir.path(), WARP_REPO, "git");
    // Version only: the move chip alone offers the source change.
    assert!(!plugin_is_outdated(dir.path()));

    write_extension(dir.path(), "0.9.0");
    assert!(plugin_is_outdated(dir.path()));
}

#[derive(Clone, Copy)]
enum Operation {
    Install,
    Update,
    Move,
}

/// Runs `operation` on a manager backed by `runner`, returning its result and
/// `needs_source_migration` afterwards.
fn drive(
    home: &Path,
    bundle: Option<&Path>,
    runner: &Arc<FakeRunner>,
    operation: Operation,
) -> (Result<(), String>, bool) {
    with_gemini_home(home, bundle, || {
        let manager = GeminiPluginManager::with_runner(runner.clone());
        let result = futures::executor::block_on(async {
            match operation {
                Operation::Install => manager.install().await,
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

fn install_command(bundle: &Path) -> String {
    format!("gemini extensions install {} --consent", bundle.display())
}

/// What `gemini extensions uninstall gemini-warp` does on disk.
fn uninstall_hook(extensions_dir: PathBuf) -> impl Fn(&str) + Send + Sync + 'static {
    move |command| {
        if command == UNINSTALL {
            fs::remove_dir_all(extensions_dir.join("gemini-warp")).unwrap();
        }
    }
}

/// What the real CLI does for the whole replace: `uninstall` removes the extension,
/// `install <bundle>` copies it and records `{"source": <bundle>, "type": "local"}`.
fn cli_effects(extensions_dir: PathBuf, bundle: PathBuf) -> impl Fn(&str) + Send + Sync + 'static {
    let uninstall = uninstall_hook(extensions_dir.clone());
    move |command| {
        uninstall(command);
        if command == install_command(&bundle) {
            write_extension(&extensions_dir, MINIMUM_PLUGIN_VERSION);
            write_install_metadata(&extensions_dir, &bundle.to_string_lossy(), "local");
        }
    }
}

/// No command Phosphor runs may touch the network: no remote URL, no `extensions update`.
fn assert_all_local(runner: &FakeRunner) {
    for call in runner.calls() {
        let lower = call.to_lowercase();
        assert!(call.starts_with("gemini extensions "), "{call}");
        for remote in ["http", "github", "gitlab", "git@", " update "] {
            assert!(!lower.contains(remote), "{call}");
        }
    }
}

#[test]
#[serial_test::serial]
fn fresh_install_installs_the_bundled_folder() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let home = tempfile::tempdir().unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(home.path(), Some(&bundle), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), vec![install_command(&bundle)]);
    assert_all_local(&runner);
}

#[test]
#[serial_test::serial]
fn install_without_the_bundle_fails_and_runs_nothing() {
    let home = tempfile::tempdir().unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(home.path(), None, &runner, Operation::Install);

    assert!(result.unwrap_err().contains("bundled"));
    assert!(runner.calls().is_empty());
}

#[test]
#[serial_test::serial]
fn update_replaces_a_remote_install_with_the_bundle_and_clears_the_offer() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let (home, extensions_dir) = gemini_home(WARP_REPO, "git", "0.9.0");
    let runner =
        Arc::new(FakeRunner::new().with_hook(cli_effects(extensions_dir.clone(), bundle.clone())));

    let (result, needs_move) = drive(home.path(), Some(&bundle), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![UNINSTALL.to_owned(), install_command(&bundle)]
    );
    assert_all_local(&runner);
    assert_eq!(
        install_source(&extensions_dir),
        InstallSource::Bundle(bundle.to_string_lossy().into_owned())
    );
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn update_of_a_current_extension_does_nothing() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let (home, _) = gemini_home(WARP_REPO, "git", MINIMUM_PLUGIN_VERSION);
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(home.path(), Some(&bundle), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
}

#[test]
#[serial_test::serial]
fn update_never_removes_a_developer_install() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    for install_type in ["local", "link"] {
        let (home, extensions_dir) =
            gemini_home("/home/dev/gemini-cli-warp", install_type, "0.9.0");
        let runner = Arc::new(FakeRunner::new());

        let (result, needs_move) = drive(home.path(), Some(&bundle), &runner, Operation::Update);

        assert!(result.is_err(), "{install_type}");
        assert!(runner.calls().is_empty(), "{install_type}");
        assert!(!needs_move, "{install_type}");
        assert!(check_installed(&extensions_dir));
    }
}

#[test]
#[serial_test::serial]
fn update_without_the_bundle_fails_and_changes_nothing() {
    let (home, extensions_dir) = gemini_home(WARP_REPO, "git", "0.9.0");
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(home.path(), None, &runner, Operation::Update);

    assert!(result.unwrap_err().contains("bundled"));
    assert!(runner.calls().is_empty());
    assert!(check_installed(&extensions_dir));
}

#[test]
#[serial_test::serial]
fn move_puts_the_previous_extension_back_when_the_install_fails() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let (home, extensions_dir) = gemini_home(WARP_REPO, "git", "1.0.0");
    let manifest =
        fs::read_to_string(extensions_dir.join("gemini-warp/gemini-extension.json")).unwrap();
    let runner = Arc::new(
        FakeRunner::new()
            .failing(&install_command(&bundle))
            .with_hook(uninstall_hook(extensions_dir.clone())),
    );

    let (result, needs_move) = drive(home.path(), Some(&bundle), &runner, Operation::Move);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![UNINSTALL.to_owned(), install_command(&bundle)]
    );
    // The working install is back, byte for byte, and no remote was fetched to get there.
    assert_eq!(install_source(&extensions_dir), InstallSource::Remote);
    assert_eq!(
        fs::read_to_string(extensions_dir.join("gemini-warp/gemini-extension.json")).unwrap(),
        manifest
    );
    assert!(!needs_move, "a completed attempt is not re-offered");
}

#[test]
#[serial_test::serial]
fn move_reinstalls_from_the_bundle() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let (home, extensions_dir) = gemini_home(OLD_BUNDLE, "local", "1.0.0");
    let runner =
        Arc::new(FakeRunner::new().with_hook(cli_effects(extensions_dir.clone(), bundle.clone())));

    let (result, needs_move) = drive(home.path(), Some(&bundle), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![UNINSTALL.to_owned(), install_command(&bundle)]
    );
    assert_all_local(&runner);
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn move_with_nothing_to_move_or_no_bundle_runs_nothing() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let (current_home, _) = gemini_home(&bundle.to_string_lossy(), "local", MINIMUM_PLUGIN_VERSION);
    let runner = Arc::new(FakeRunner::new());
    let (result, needs_move) = drive(current_home.path(), Some(&bundle), &runner, Operation::Move);
    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
    assert!(!needs_move);

    let (remote_home, _) = gemini_home(WARP_REPO, "git", MINIMUM_PLUGIN_VERSION);
    let (result, needs_move) = drive(remote_home.path(), None, &runner, Operation::Move);
    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
    assert!(
        !needs_move,
        "no bundle: nothing to move to, nothing offered"
    );
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
