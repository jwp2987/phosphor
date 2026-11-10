use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{
    BUNDLE_DIR_ENV, CodexPluginManager, MINIMUM_PLUGIN_VERSION, reset_source_move_attempt,
};
use crate::features::FeatureFlag;
use crate::terminal::cli_agent_sessions::plugin_manager::CliAgentPluginManager;
use crate::terminal::cli_agent_sessions::plugin_manager::tests::{FakeRunner, without_local_path};

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
        MINIMUM_PLUGIN_VERSION
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
fn instructions_use_the_bundled_folder_and_no_remote() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let manager = CodexPluginManager::new(None, None, None);
    let install = manager.install_instructions();
    assert_eq!(install.steps.len(), 2);
    assert_eq!(install.steps[1].command, "codex plugin add warp@codex-warp");
    assert!(!install.title.is_empty());

    // The update steps are also the move chip's: remove and re-add from the bundled folder
    // (`marketplace upgrade` would fetch from a recorded remote).
    let update = manager.update_instructions();
    assert_eq!(update.steps.len(), 3);
    assert_eq!(
        update.steps[0].command,
        "codex plugin marketplace remove codex-warp"
    );
    assert_eq!(update.steps[2].command, install.steps[1].command);
    assert!(!update.title.is_empty());
    assert_eq!(
        manager.move_instructions().steps[1].command,
        update.steps[1].command
    );

    for add in [install.steps[0].command, update.steps[1].command] {
        assert!(add.starts_with("codex plugin marketplace add \""), "{add}");
        assert!(
            add.contains("bundled") || add.contains("resources"),
            "{add}"
        );
    }
    for step in install.steps.iter().chain(update.steps.iter()) {
        let lower = step.command.to_lowercase();
        for remote in ["github", "gitlab", "http", "git@", "upgrade"] {
            assert!(!lower.contains(remote), "{}", step.command);
        }
    }
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
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, MINIMUM_PLUGIN_VERSION);

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

/// A folder shaped like the bundled marketplace, at `<root>/bundled/codex_plugin`.
fn make_bundle(root: &Path) -> PathBuf {
    let bundle = root.join("bundled").join("codex_plugin");
    fs::create_dir_all(bundle.join(".agents").join("plugins")).unwrap();
    fs::write(
        bundle
            .join(".agents")
            .join("plugins")
            .join("marketplace.json"),
        "{}",
    )
    .unwrap();
    bundle
}

fn shipped_bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/bundled/codex_plugin")
        .canonicalize()
        .expect("resources/bundled/codex_plugin exists")
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
fn shipped_plugin_version_matches_the_minimum_version() {
    let bundle = shipped_bundle();
    let read = |rel: &str| -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(bundle.join(rel)).unwrap()).unwrap()
    };
    let market = read(".agents/plugins/marketplace.json");
    assert_eq!(market["name"], "codex-warp");
    let plugins = market["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1, "only the warp plugin ships");
    assert_eq!(plugins[0]["name"], "warp");
    assert_eq!(plugins[0]["version"], MINIMUM_PLUGIN_VERSION);
    let plugin = read("plugins/warp/.codex-plugin/plugin.json");
    assert_eq!(plugin["name"], "warp");
    assert_eq!(plugin["version"], MINIMUM_PLUGIN_VERSION);
    // The version the session-start hook reports is the same one.
    let session_start =
        fs::read_to_string(bundle.join("plugins/warp/scripts/on-session-start.sh")).unwrap();
    assert!(
        session_start.contains(&format!("PLUGIN_VERSION=\"{MINIMUM_PLUGIN_VERSION}\"")),
        "on-session-start.sh reports another version"
    );
}

#[test]
fn shipped_plugin_has_no_oz_part_no_warp_metadata_and_no_network_calls() {
    let bundle = shipped_bundle();
    assert!(!bundle.join("plugins/orchestration").exists());
    let mut files = Vec::new();
    files_under(&bundle, &mut files);
    assert!(!files.is_empty());
    for file in files {
        if file.file_name().unwrap() == "LICENSE" {
            continue; // the upstream MIT notice stays
        }
        let text = fs::read_to_string(&file).unwrap_or_default().to_lowercase();
        for forbidden in [
            "oz-",
            "oz_",
            "orchestration",
            "warpdotdev",
            "warp.dev",
            "http://",
            "https://",
            "curl ",
            "wget ",
            "urllib",
            "/dev/tcp",
            "invoke-webrequest",
            "invoke-restmethod",
            "webclient",
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
fn registrations_are_classified_by_their_recorded_source() {
    let dir = tempfile::tempdir().unwrap();
    let classify = |source_type: &str, source: &str| {
        fs::write(
            dir.path().join("config.toml"),
            format!(
                "[marketplaces.codex-warp]\nsource_type = \"{source_type}\"\nsource = \"{source}\"\n"
            ),
        )
        .unwrap();
        let config = super::codex_warp_marketplace_config(dir.path()).unwrap();
        (
            config.is_git(),
            config.is_bundle_folder(),
            config.is_developer_local(),
        )
    };
    // Remote: moved, never fetched.
    assert_eq!(
        classify("git", "https://github.com/warpdotdev/codex-warp.git"),
        (true, false, false)
    );
    assert_eq!(
        classify("git", "https://gitlab.com/winters-phosphor/codex-warp.git"),
        (true, false, false)
    );
    // One of Phosphor's bundle folders, at any install location.
    assert_eq!(
        classify("local", "/opt/phosphor/resources/bundled/codex_plugin"),
        (false, true, false)
    );
    // A developer's own checkout: left alone, whatever its name says.
    assert_eq!(
        classify("local", "/src/github.com/warpdotdev/codex-warp"),
        (false, false, true)
    );
    assert_eq!(
        classify("directory", "/src/codex-warp-checkout"),
        (false, false, true)
    );
}

#[test]
fn only_a_registration_off_the_current_bundle_needs_source_migration() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let bundle_str = bundle.to_string_lossy().into_owned();
    let dir = tempfile::tempdir().unwrap();
    let needs = |source_type: &str, source: &str, bundle: Option<&Path>| {
        write_enabled_plugin_with_marketplace(dir.path(), source_type, source);
        super::plugin_needs_source_migration(dir.path(), bundle)
    };
    assert!(needs(
        "git",
        "https://github.com/warpdotdev/codex-warp.git",
        Some(&bundle)
    ));
    assert!(needs(
        "git",
        "https://gitlab.com/winters-phosphor/codex-warp.git",
        Some(&bundle)
    ));
    assert!(needs(
        "local",
        "/opt/old-phosphor/resources/bundled/codex_plugin",
        Some(&bundle)
    ));
    assert!(!needs("local", &bundle_str, Some(&bundle)));
    assert!(!needs("local", &format!("{bundle_str}/"), Some(&bundle)));
    assert!(!needs(
        "directory",
        "/src/codex-warp-checkout",
        Some(&bundle)
    ));
    // With no bundled folder to move to there is nothing to offer.
    assert!(!needs(
        "git",
        "https://github.com/warpdotdev/codex-warp.git",
        None
    ));
    // Not installed: `install()` replaces the registration instead.
    write_git_marketplace_config(dir.path(), "https://github.com/warpdotdev/codex-warp.git");
    assert!(!super::plugin_needs_source_migration(
        dir.path(),
        Some(&bundle)
    ));
}

#[test]
#[serial_test::serial]
fn current_plugin_from_a_remote_marketplace_needs_a_move_not_an_update_via_trait() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    reset_source_move_attempt();
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_marketplace(
        dir.path(),
        "git",
        "https://github.com/warpdotdev/codex-warp.git",
    );
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, MINIMUM_PLUGIN_VERSION);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var("CODEX_HOME", dir.path());
        std::env::set_var(BUNDLE_DIR_ENV, &bundle);
    }
    let manager = CodexPluginManager::new(None, None, None);
    let before = (manager.needs_source_migration(), manager.needs_update());
    write_enabled_plugin_with_marketplace(dir.path(), "local", &bundle.to_string_lossy());
    let after = (manager.needs_source_migration(), manager.needs_update());
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::remove_var("CODEX_HOME");
        std::env::remove_var(BUNDLE_DIR_ENV);
    }

    // `needs_update` is version-only (the minimum is cached); the move has its own chip.
    assert_eq!(before, (true, false));
    assert_eq!(after, (false, false));
}

#[test]
#[serial_test::serial]
fn no_source_migration_without_codex_plugin() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_marketplace(
        dir.path(),
        "git",
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

fn add_command(source: &Path) -> String {
    format!("codex plugin marketplace add {}", source.display())
}

#[derive(Clone, Copy)]
enum Operation {
    Install,
    Update,
    Move,
}

/// Runs `operation` on a manager backed by `runner` with `CODEX_HOME` at `dir`, the bundled
/// folder at `bundle` (a missing folder when `None`) and the move-attempt flag reset,
/// returning its result and `needs_source_migration` afterwards.
fn drive(
    dir: &Path,
    bundle: Option<&Path>,
    runner: &Arc<FakeRunner>,
    operation: Operation,
) -> (Result<(), String>, bool) {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    reset_source_move_attempt();
    let missing = dir.join("no-bundle-here");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var("CODEX_HOME", dir);
        std::env::set_var(BUNDLE_DIR_ENV, bundle.unwrap_or(&missing));
    }
    let manager = CodexPluginManager::with_runner(runner.clone());
    let result = futures::executor::block_on(async {
        match operation {
            Operation::Install => manager.install().await,
            Operation::Update => manager.update().await,
            Operation::Move => manager.move_to_phosphor_source().await,
        }
    });
    let needs_move = manager.needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::remove_var("CODEX_HOME");
        std::env::remove_var(BUNDLE_DIR_ENV);
    }
    (result.map_err(|err| err.message), needs_move)
}

/// What the real CLI does to its records: `marketplace add <bundle>` records the folder as a
/// `local` source (and keeps the plugin enabled), `plugin add` caches the shipped version.
fn cli_effects(dir: &Path, bundle: &Path) -> impl Fn(&str) + Send + Sync + 'static {
    let dir = dir.to_owned();
    let bundle = bundle.to_owned();
    move |command| {
        if command == add_command(&bundle) {
            write_enabled_plugin_with_marketplace(&dir, "local", &bundle.to_string_lossy());
        } else if command == PLUGIN_ADD {
            write_cache_manifest(&dir, super::PLUGIN_NAME, MINIMUM_PLUGIN_VERSION);
        }
    }
}

fn warp_registered_codex_home(version: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_marketplace(dir.path(), "git", WARP_URL);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, version);
    dir
}

/// No command Phosphor runs may touch the network: no git, no remote URL, no upgrade.
/// The folder argument is `bundle`, a temp folder this test made: it is cut out before the scan,
/// so the name of the temp directory (`/home/gitlab-runner/...` on the CI box) is not read as a
/// remote.
fn assert_all_local(runner: &FakeRunner, bundle: &Path) {
    for call in runner.calls() {
        let lower = without_local_path(&call, bundle).to_lowercase();
        assert!(call.starts_with("codex plugin "), "{call}");
        for remote in ["http", "github", "gitlab", "git@", "ls-remote", "upgrade"] {
            assert!(!lower.contains(remote), "{call}");
        }
    }
}

#[test]
#[serial_test::serial]
fn fresh_install_adds_the_bundled_folder_then_the_plugin() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![add_command(&bundle), PLUGIN_ADD.to_owned()]
    );
    assert_all_local(&runner, &bundle);
}

#[test]
#[serial_test::serial]
fn install_without_the_bundle_fails_and_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), None, &runner, Operation::Install);

    assert!(result.unwrap_err().contains("bundled"));
    assert!(runner.calls().is_empty());
}

#[test]
#[serial_test::serial]
fn install_over_a_remote_registration_replaces_it_with_the_bundle() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_git_marketplace_config(dir.path(), WARP_URL);
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            REMOVE.to_owned(),
            add_command(&bundle),
            PLUGIN_ADD.to_owned()
        ]
    );
    assert_all_local(&runner, &bundle);
}

#[test]
#[serial_test::serial]
fn install_from_a_developer_override_changes_no_registration() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_marketplace_config(dir.path(), "local");
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), vec![PLUGIN_ADD.to_owned()]);
}

#[test]
#[serial_test::serial]
fn update_of_a_current_plugin_does_nothing() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_codex_home(MINIMUM_PLUGIN_VERSION);
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
}

#[test]
#[serial_test::serial]
fn update_re_registers_a_remote_marketplace_from_the_bundle_with_no_network_step() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_codex_home("0.2.0");
    let runner = Arc::new(FakeRunner::new().with_hook(cli_effects(dir.path(), &bundle)));

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            REMOVE.to_owned(),
            add_command(&bundle),
            PLUGIN_ADD.to_owned()
        ]
    );
    assert_all_local(&runner, &bundle);
    assert!(!needs_move, "the move chip is no longer needed");
}

#[test]
#[serial_test::serial]
fn update_of_the_current_bundle_registration_only_adds_the_plugin() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_marketplace(dir.path(), "local", &bundle.to_string_lossy());
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.2.0");
    let runner = Arc::new(FakeRunner::new().with_hook(cli_effects(dir.path(), &bundle)));

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), vec![PLUGIN_ADD.to_owned()]);
}

#[test]
#[serial_test::serial]
fn update_never_removes_a_developer_override() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_marketplace(dir.path(), "local", "/src/codex-warp-checkout");
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.2.0");
    let runner = Arc::new(FakeRunner::new().with_hook(cli_effects(dir.path(), &bundle)));

    let (_, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert!(runner.calls().iter().all(|call| call != REMOVE));
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn update_without_the_bundle_fails_and_changes_nothing() {
    let dir = warp_registered_codex_home("0.2.0");
    let config = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), None, &runner, Operation::Update);

    assert!(result.unwrap_err().contains("bundled"));
    assert!(runner.calls().is_empty());
    assert_eq!(
        fs::read_to_string(dir.path().join("config.toml")).unwrap(),
        config
    );
}

#[test]
#[serial_test::serial]
fn a_failed_add_restores_a_previous_local_bundle_folder() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    let old = "/opt/old-phosphor/resources/bundled/codex_plugin";
    write_enabled_plugin_with_marketplace(dir.path(), "local", old);
    write_cache_manifest(dir.path(), super::PLUGIN_NAME, "0.2.0");
    let runner = Arc::new(FakeRunner::new().failing(&add_command(&bundle)));

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![
            REMOVE.to_owned(),
            add_command(&bundle),
            format!("codex plugin marketplace add {old}"),
        ]
    );
}

#[test]
#[serial_test::serial]
fn a_failed_add_never_re_fetches_a_remote_previous_source() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_codex_home("0.2.0");
    let runner = Arc::new(FakeRunner::new().failing(&add_command(&bundle)));

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Move);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![REMOVE.to_owned(), add_command(&bundle)]
    );
    assert_all_local(&runner, &bundle);
    assert!(!needs_move, "a completed attempt is not re-offered");
}

#[test]
#[serial_test::serial]
fn move_re_registers_from_the_bundle_and_clears_the_offer() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_codex_home(MINIMUM_PLUGIN_VERSION);
    let runner = Arc::new(FakeRunner::new().with_hook(cli_effects(dir.path(), &bundle)));

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![
            REMOVE.to_owned(),
            add_command(&bundle),
            PLUGIN_ADD.to_owned()
        ]
    );
    assert_all_local(&runner, &bundle);
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn codex_without_marketplace_remove_fails_once_and_keeps_the_install() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_codex_home(MINIMUM_PLUGIN_VERSION);
    let config = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    let runner = Arc::new(FakeRunner::new().failing(REMOVE));

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Move);

    assert!(result.is_err());
    assert_eq!(runner.calls(), vec![REMOVE.to_owned()]);
    assert_eq!(
        fs::read_to_string(dir.path().join("config.toml")).unwrap(),
        config
    );
    assert!(
        !needs_move,
        "failed once, not offered again in this process"
    );
}

#[test]
#[serial_test::serial]
fn move_without_the_bundle_fails_and_is_not_offered() {
    let dir = warp_registered_codex_home(MINIMUM_PLUGIN_VERSION);
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive(dir.path(), None, &runner, Operation::Move);

    // With no bundle there is nothing to move to, so the call is a no-op (the chip is not
    // offered either).
    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn move_with_nothing_to_move_runs_nothing() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_enabled_plugin_with_marketplace(dir.path(), "local", &bundle.to_string_lossy());
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
    assert!(!needs_move);
}

/// The shape `codex plugin marketplace add <source>` writes, plus the enabled plugin entry
/// `codex plugin add warp@codex-warp` writes.
fn write_enabled_plugin_with_marketplace(dir: &Path, source_type: &str, source: &str) {
    fs::write(
        dir.join("config.toml"),
        format!(
            "[marketplaces.codex-warp]\nsource_type = \"{source_type}\"\nsource = \"{source}\"\n\n[plugins.\"{}\"]\nenabled = true\n",
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
            "[marketplaces.codex-warp]\nsource_type = \"{source_type}\"\nsource = \"/src/codex-warp-checkout\"\n"
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
