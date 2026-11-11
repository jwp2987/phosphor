use std::fs;

use std::path::Path;
use std::sync::Arc;

use std::path::PathBuf;

use super::{
    BUNDLE_DIR_ENV, ClaudeCodePluginManager, CliAgentPluginManager, MINIMUM_PLUGIN_VERSION,
    check_installed, claude_code_marketplace_has_local_override, installed_version,
    marketplace_registered_from_warp, plugin_is_outdated, plugin_needs_source_migration,
    reset_source_move_attempt,
};
use crate::terminal::cli_agent_sessions::plugin_manager::tests::{FakeRunner, without_local_path};

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
        "2.4.1"
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

/// Where Phosphor's older builds fetched the plugin from. Only used as a recorded source in
/// these tests; the manager never contacts it any more.
const OLD_GITLAB_URL: &str = "https://gitlab.com/winters-phosphor/claude-code-warp.git";

/// A folder shaped like the bundled marketplace, at `<root>/bundled/claude_plugin`.
fn make_bundle(root: &Path) -> PathBuf {
    let bundle = root.join("bundled").join("claude_plugin");
    fs::create_dir_all(bundle.join(".claude-plugin")).unwrap();
    fs::write(bundle.join(".claude-plugin").join("marketplace.json"), "{}").unwrap();
    bundle
}

fn directory_source(path: &Path) -> serde_json::Value {
    serde_json::json!({"source": "directory", "path": path.to_string_lossy()})
}

fn shipped_bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/bundled/claude_plugin")
        .canonicalize()
        .expect("resources/bundled/claude_plugin exists")
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
    let market = read(".claude-plugin/marketplace.json");
    assert_eq!(market["name"], "claude-code-warp");
    let plugins = market["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1, "only the warp plugin ships");
    assert_eq!(plugins[0]["name"], "warp");
    assert_eq!(plugins[0]["version"], MINIMUM_PLUGIN_VERSION);
    let plugin = read("plugins/warp/.claude-plugin/plugin.json");
    assert_eq!(plugin["name"], "warp");
    assert_eq!(plugin["version"], MINIMUM_PLUGIN_VERSION);
}

#[test]
fn shipped_plugin_has_no_oz_part_no_warp_metadata_and_no_network_calls() {
    let bundle = shipped_bundle();
    assert!(!bundle.join("plugins/oz-harness-support").exists());
    let mut files = Vec::new();
    files_under(&bundle, &mut files);
    assert!(!files.is_empty());
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        if name == "LICENSE" {
            continue; // the upstream MIT notice stays
        }
        let text = fs::read_to_string(&file).unwrap_or_default().to_lowercase();
        for forbidden in [
            "oz-harness",
            "warpdotdev",
            "warp.dev",
            "http://",
            "https://",
            "curl ",
            "wget ",
            "urllib",
            "/dev/tcp",
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
fn truncate_chars_cuts_by_characters_not_bytes() {
    let have = |tool: &str| {
        std::process::Command::new(tool)
            .arg("--version")
            .output()
            .is_ok()
    };
    if !have("bash") || !have("jq") {
        eprintln!("skipped: bash and jq are needed");
        return;
    }
    let script = shipped_bundle().join("plugins/warp/scripts/build-payload.sh");
    // `canonicalize` gives a verbatim path on Windows (`\\?\C:\...`); bash is handed the plain
    // form of the same path.
    let script = script
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    // The whole outcome of one run, so a failure says what bash and the script reported.
    let truncate = |text: &str, limit: &str, locale: Option<&str>| {
        let mut command = std::process::Command::new("bash");
        command
            .arg("-c")
            .arg("source \"$1\"; truncate_chars \"$2\" \"$3\"")
            .arg("bash")
            .arg(&script)
            .arg(text)
            .arg(limit);
        if let Some(locale) = locale {
            command.env("LC_ALL", locale);
        }
        let out = command.output().unwrap();
        let stdout = String::from_utf8(out.stdout).expect("output is valid UTF-8");
        let report = format!(
            "{script} locale={locale:?} status={:?} stderr={:?}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        (stdout, report)
    };
    // 13 two-byte characters, 12 of them kept short of a cut: limit 10 -> 7 chars + "...".
    let input = "\u{e9}".repeat(13);
    for locale in ["C", "C.UTF-8"] {
        let (text, report) = truncate(&input, "10", Some(locale));
        assert_eq!(
            text.trim_end_matches('\n'),
            format!("{}...", "\u{e9}".repeat(7)),
            "{report}"
        );
    }
    // Short text is unchanged, a ';' survives.
    let (text, report) = truncate("a;b \u{e9}", "200", None);
    assert_eq!(text.trim_end(), "a;b \u{e9}", "{report}");
}

#[test]
fn instructions_use_the_bundled_folder_and_no_remote() {
    let manager = ClaudeCodePluginManager::new(None, None, None);
    let install = manager.install_instructions();
    assert_eq!(install.steps.len(), 2);
    assert_eq!(
        install.steps[1].command,
        "claude plugin install warp@claude-code-warp"
    );
    let update = manager.update_instructions();
    assert_eq!(update.steps.len(), 3);
    assert_eq!(
        update.steps[0].command,
        "claude plugin marketplace remove claude-code-warp"
    );
    for add in [install.steps[0].command, update.steps[1].command] {
        assert!(add.starts_with("claude plugin marketplace add \""), "{add}");
        assert!(
            add.contains("bundled") || add.contains("resources"),
            "{add}"
        );
        let lower = add.to_lowercase();
        for remote in ["github", "gitlab", "http", "git@"] {
            assert!(!lower.contains(remote), "{add}");
        }
    }
    assert_eq!(update.steps[2].command, install.steps[1].command);
}

#[test]
fn bundle_folder_registrations_are_not_a_developer_override() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = make_bundle(dir.path());
    write_known_marketplace_source(dir.path(), directory_source(&bundle));
    assert!(!claude_code_marketplace_has_local_override(dir.path()));
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "directory", "path": "/home/dev/claude-code-warp"}),
    );
    assert!(claude_code_marketplace_has_local_override(dir.path()));
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
fn registered_from_warp_when_known_marketplaces_record_github_source() {
    let dir = tempfile::tempdir().unwrap();
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": OLD_GITLAB_URL}),
    );
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );
    assert!(marketplace_registered_from_warp(dir.path()));
}

#[test]
fn not_registered_from_warp_for_similarly_named_repo_or_local_override() {
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp-internal"}),
    );
    assert!(!marketplace_registered_from_warp(dir.path()));

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
    assert!(!marketplace_registered_from_warp(
        tempfile::tempdir().unwrap().path()
    ));
}

#[test]
fn migration_is_needed_only_off_the_current_bundle() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let old_root = tempfile::tempdir().unwrap();
    let stale_bundle = make_bundle(old_root.path());

    let needs = |source: serde_json::Value, bundle: Option<&Path>| {
        let dir = tempfile::tempdir().unwrap();
        write_installed_plugin_version(dir.path(), "2.4.1");
        write_known_marketplace_source(dir.path(), source);
        plugin_needs_source_migration(dir.path(), bundle)
    };
    let github = serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"});
    let gitlab = serde_json::json!({"source": "git", "url": OLD_GITLAB_URL});

    assert!(needs(github.clone(), Some(&bundle)));
    assert!(needs(gitlab, Some(&bundle)));
    assert!(needs(directory_source(&stale_bundle), Some(&bundle)));
    assert!(!needs(directory_source(&bundle), Some(&bundle)));
    assert!(
        !needs(
            serde_json::json!({"source": "directory", "path": "/home/dev/claude-code-warp"}),
            Some(&bundle)
        ),
        "a developer's own folder is never moved"
    );
    assert!(!needs(github, None), "no bundle, nothing to move to");

    // Not installed: the install path handles it.
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );
    assert!(!plugin_needs_source_migration(dir.path(), Some(&bundle)));
}

#[test]
fn outdated_plugin_needs_update_and_current_does_not() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.4.0");
    assert!(plugin_is_outdated(dir.path()));
    write_installed_plugin_version(dir.path(), MINIMUM_PLUGIN_VERSION);
    assert!(!plugin_is_outdated(dir.path()));
}

const REMOVE: &str = "claude plugin marketplace remove claude-code-warp";
const INSTALL: &str = "claude plugin install warp@claude-code-warp";

fn add_command(source: &Path) -> String {
    format!("claude plugin marketplace add {}", source.display())
}

#[derive(Clone, Copy)]
enum Operation {
    Install,
    Update,
    Move,
}

/// Runs `operation` against a manager on `runner` with `CLAUDE_CONFIG_DIR` at `dir`, the
/// bundled folder at `bundle` (a missing folder when `None`), and the move-attempt flag
/// reset. Returns its result and the manager's `needs_source_migration` afterwards.
fn drive(
    dir: &Path,
    bundle: Option<&Path>,
    runner: &Arc<FakeRunner>,
    operation: Operation,
) -> (Result<(), String>, bool) {
    reset_source_move_attempt();
    let missing = dir.join("no-bundle-here");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var("CLAUDE_CONFIG_DIR", dir);
        std::env::set_var(BUNDLE_DIR_ENV, bundle.unwrap_or(&missing));
    }
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
    unsafe {
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        std::env::remove_var(BUNDLE_DIR_ENV);
    }
    (result.map_err(|err| err.message), needs_move)
}

/// What the real CLI does to its records: `marketplace add <bundle>` records the folder,
/// `plugin install` records the shipped version.
fn cli_effects(dir: &Path, bundle: &Path) -> impl Fn(&str) + Send + Sync + 'static {
    let dir = dir.to_owned();
    let bundle = bundle.to_owned();
    move |command| {
        if command == add_command(&bundle) {
            write_known_marketplace_source(&dir, directory_source(&bundle));
        } else if command == INSTALL {
            write_installed_plugin_version(&dir, MINIMUM_PLUGIN_VERSION);
        }
    }
}

fn warp_registered_claude_dir(version: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), version);
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );
    dir
}

/// No command Phosphor runs may touch the network: no git, no remote URL.
/// The folder argument is `bundle`, a temp folder this test made: it is cut out before the scan,
/// so the name of the temp directory (`/home/gitlab-runner/...` on the CI box) is not read as a
/// remote.
fn assert_all_local(runner: &FakeRunner, bundle: &Path) {
    for call in runner.calls() {
        let lower = without_local_path(&call, bundle).to_lowercase();
        assert!(call.starts_with("claude plugin "), "{call}");
        for remote in ["http", "github", "gitlab", "git@", "ls-remote"] {
            assert!(!lower.contains(remote), "{call}");
        }
    }
}

#[test]
#[serial_test::serial]
fn fresh_install_adds_the_bundled_folder_then_installs() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![add_command(&bundle), INSTALL.to_owned()]
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
    assert_eq!(runner.calls(), Vec::<String>::new());
}

#[test]
#[serial_test::serial]
fn install_over_a_remote_registration_replaces_it_with_the_bundle() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );
    let runner = Arc::new(FakeRunner::new().with_hook(cli_effects(dir.path(), &bundle)));

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![REMOVE.to_owned(), add_command(&bundle), INSTALL.to_owned()]
    );
    assert_all_local(&runner, &bundle);
}

#[test]
#[serial_test::serial]
fn install_from_a_developer_override_changes_no_registration() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "directory", "path": "/home/dev/claude-code-warp"}),
    );
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Install);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), vec![INSTALL.to_owned()]);
}

#[test]
#[serial_test::serial]
fn update_of_a_current_plugin_does_nothing() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_claude_dir(MINIMUM_PLUGIN_VERSION);
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), Vec::<String>::new());
}

#[test]
#[serial_test::serial]
fn update_re_registers_the_bundle_with_no_network_step() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_claude_dir("2.0.0");
    let runner = Arc::new(FakeRunner::new().with_hook(cli_effects(dir.path(), &bundle)));

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![REMOVE.to_owned(), add_command(&bundle), INSTALL.to_owned()]
    );
    assert_all_local(&runner, &bundle);
    assert!(!needs_move, "now registered from the bundle");
    assert_eq!(
        installed_version(dir.path()).as_deref(),
        Some(MINIMUM_PLUGIN_VERSION)
    );
}

#[test]
#[serial_test::serial]
fn update_without_the_bundle_fails_and_changes_nothing() {
    let dir = warp_registered_claude_dir("2.0.0");
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), None, &runner, Operation::Update);

    assert!(result.is_err());
    assert_eq!(runner.calls(), Vec::<String>::new());
}

#[test]
#[serial_test::serial]
fn update_never_removes_a_developer_override() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.0.0");
    write_known_marketplace_source(
        dir.path(),
        serde_json::json!({"source": "directory", "path": "/home/dev/claude-code-warp"}),
    );
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert!(result.is_err());
    assert_eq!(runner.calls(), Vec::<String>::new());
}

#[test]
#[serial_test::serial]
fn a_failed_add_restores_a_previous_local_bundle_folder() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let old_root = tempfile::tempdir().unwrap();
    let old_bundle = make_bundle(old_root.path());
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.0.0");
    write_known_marketplace_source(dir.path(), directory_source(&old_bundle));
    let runner = Arc::new(FakeRunner::new().failing(&add_command(&bundle)));

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![
            REMOVE.to_owned(),
            add_command(&bundle),
            REMOVE.to_owned(),
            add_command(&old_bundle),
            INSTALL.to_owned(),
        ]
    );
}

#[test]
#[serial_test::serial]
fn a_failed_add_never_re_fetches_a_remote_previous_source() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_claude_dir("2.0.0");
    let runner = Arc::new(FakeRunner::new().failing(&add_command(&bundle)));

    let (result, _) = drive(dir.path(), Some(&bundle), &runner, Operation::Update);

    assert!(result.is_err());
    assert_eq!(
        runner.calls(),
        vec![REMOVE.to_owned(), add_command(&bundle)]
    );
    assert_all_local(&runner, &bundle);
}

#[test]
#[serial_test::serial]
fn move_re_registers_from_the_bundle_and_clears_the_offer() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_claude_dir(MINIMUM_PLUGIN_VERSION);
    let runner = Arc::new(FakeRunner::new().with_hook(cli_effects(dir.path(), &bundle)));

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert_eq!(
        runner.calls(),
        vec![REMOVE.to_owned(), add_command(&bundle), INSTALL.to_owned()]
    );
    assert_all_local(&runner, &bundle);
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn a_move_whose_record_stays_stale_is_not_reoffered() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_claude_dir(MINIMUM_PLUGIN_VERSION);
    // No hook: the record keeps naming Warp's GitHub after the move ran.
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn move_with_nothing_to_move_runs_nothing() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), MINIMUM_PLUGIN_VERSION);
    write_known_marketplace_source(dir.path(), directory_source(&bundle));
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive(dir.path(), Some(&bundle), &runner, Operation::Move);

    assert_eq!(result, Ok(()));
    assert_eq!(runner.calls(), Vec::<String>::new());
    assert!(!needs_move);
}

#[test]
#[serial_test::serial]
fn needs_source_migration_via_trait_uses_claude_config_dir_and_the_bundle() {
    reset_source_move_attempt();
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = warp_registered_claude_dir(MINIMUM_PLUGIN_VERSION);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var("CLAUDE_CONFIG_DIR", dir.path());
        std::env::set_var(BUNDLE_DIR_ENV, &bundle);
    }
    let manager = ClaudeCodePluginManager::new(None, None, None);
    let before = (manager.needs_source_migration(), manager.needs_update());
    // Re-registering from the bundle clears it, even though the first answer was memoised.
    write_known_marketplace_source(dir.path(), directory_source(&bundle));
    let after = (manager.needs_source_migration(), manager.needs_update());
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        std::env::remove_var(BUNDLE_DIR_ENV);
    }

    assert_eq!(before, (true, false));
    assert_eq!(after, (false, false));
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
