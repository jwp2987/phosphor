use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{
    BUNDLE_DIR_ENV, MINIMUM_PLUGIN_VERSION, OpenCodePluginManager, PluginEntryKind,
    classify_plugin_entry, file_needs_source_migration, migrate_plugin_entries,
    plugin_array_elements, plugin_spec_for, reset_source_move_attempt, write_config_atomically,
};
use crate::terminal::cli_agent_sessions::plugin_manager::CliAgentPluginManager;
use crate::terminal::cli_agent_sessions::plugin_manager::tests::FakeRunner;

fn manager() -> OpenCodePluginManager {
    OpenCodePluginManager::new(None, None, None)
}

#[test]
fn can_auto_install_is_false() {
    assert!(!manager().can_auto_install());
}

/// B9: a version update stays manual (the instructions modal); only the
/// move chip edits the config, and it needs no auto-install support.
#[test]
fn version_updates_stay_manual() {
    assert!(!manager().can_auto_install());
    assert!(!manager().needs_update());
}

#[test]
fn install_instructions_has_steps() {
    let instructions = manager().install_instructions();
    assert!(!instructions.steps.is_empty());
    assert!(!instructions.title.is_empty());
}

#[test]
fn update_instructions_has_steps() {
    let instructions = manager().update_instructions();
    assert!(!instructions.steps.is_empty());
    assert!(!instructions.title.is_empty());
}

/// A folder shaped like the bundled plugin, at `<root>/bundled/opencode_plugin`. It need not
/// exist: the entry text is derived from the path alone.
fn test_bundle() -> PathBuf {
    std::env::temp_dir()
        .join("phosphor-opencode-test")
        .join("bundled")
        .join("opencode_plugin")
}

fn spec() -> String {
    plugin_spec_for(&test_bundle()).unwrap()
}

fn shipped_bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/bundled/opencode_plugin")
        .canonicalize()
        .expect("resources/bundled/opencode_plugin exists")
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
fn plugin_spec_is_the_file_url_of_the_bundled_folder() {
    let spec = spec();
    assert!(spec.starts_with("file://"), "{spec}");
    assert!(spec.ends_with("bundled/opencode_plugin"), "{spec}");
    assert_eq!(
        classify_plugin_entry(&spec),
        Some(PluginEntryKind::Phosphor)
    );
}

#[test]
fn shipped_plugin_version_matches_the_minimum_version() {
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(shipped_bundle().join("package.json")).unwrap())
            .unwrap();
    assert_eq!(package["version"], MINIMUM_PLUGIN_VERSION);
    assert_eq!(package["exports"]["./tui"]["import"], "./dist/tui.js");
    assert_eq!(package["exports"]["./server"]["import"], "./dist/index.js");
    for built in ["dist/index.js", "dist/tui.js", "dist/notify.js"] {
        assert!(shipped_bundle().join(built).is_file(), "{built}");
    }
}

#[test]
fn shipped_plugin_has_no_warp_metadata_and_no_network_calls() {
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
            "warp-dot-dev",
            "http://",
            "https://",
            "gitlab",
            "github",
            "fetch(",
            "node:http",
            "node:https",
            "node:net",
            "xmlhttprequest",
            "child_process",
            "sourcemappingurl",
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
fn instructions_name_the_bundled_file_entry_and_no_remote() {
    let manager = manager();
    for instructions in [
        manager.install_instructions(),
        manager.update_instructions(),
    ] {
        let entry = instructions.steps[1].command;
        assert!(entry.starts_with("\"plugin\": [\"file://"), "{entry}");
        assert!(
            entry.contains("bundled/opencode_plugin") || entry.contains("resources"),
            "{entry}"
        );
        let lower = entry.to_lowercase();
        for remote in ["github", "gitlab", "http", "git+", "git@", "npm:"] {
            assert!(!lower.contains(remote), "{entry}");
        }
    }
}

#[test]
fn classifies_every_warp_spelling() {
    for spec in [
        "@warp-dot-dev/opencode-warp",
        "@warp-dot-dev/opencode-warp@0.1.5",
        "@warp-dot-dev/opencode-warp@latest",
        "@warp-dot-dev/opencode-warp@^0.1.0",
        "npm:@warp-dot-dev/opencode-warp@0.1.5",
        "github:warpdotdev/opencode-warp",
        "warpdotdev/opencode-warp",
        "git+https://github.com/warpdotdev/opencode-warp.git",
        "git+https://github.com/warpdotdev/opencode-warp.git#v0.1.5",
    ] {
        assert_eq!(
            classify_plugin_entry(spec),
            Some(PluginEntryKind::Warp),
            "{spec}"
        );
    }
}

#[test]
fn classifies_earlier_phosphor_git_spellings_as_remote() {
    // What older builds wrote; Phosphor never fetches these, so they are moved.
    for spec in [
        "git+https://gitlab.com/winters-phosphor/opencode-warp.git#v0.2.0",
        "git+https://gitlab.com/winters-phosphor/opencode-warp.git",
        "https://gitlab.com/winters-phosphor/opencode-warp",
        "gitlab:winters-phosphor/opencode-warp#v0.2.0",
        "git+ssh://git@gitlab.com/winters-phosphor/opencode-warp.git#v0.2.0",
        "git@gitlab.com:winters-phosphor/opencode-warp.git",
    ] {
        assert_eq!(
            classify_plugin_entry(spec),
            Some(PluginEntryKind::Warp),
            "{spec}"
        );
    }
}

#[test]
fn classifies_bundle_folder_entries_at_any_install_location() {
    for spec in [
        "file:///opt/phosphor/resources/bundled/opencode_plugin",
        "file:///Applications/Phosphor.app/Contents/Resources/bundled/opencode_plugin/",
    ] {
        assert_eq!(
            classify_plugin_entry(spec),
            Some(PluginEntryKind::Phosphor),
            "{spec}"
        );
    }
}

#[test]
fn leaves_other_and_local_entries_unclassified() {
    for spec in [
        "file:///home/dev/opencode-warp/src/index.ts",
        "/home/dev/opencode-warp/dist/index.js",
        "./plugins/opencode-warp",
        "@warp-dot-dev/opencode-warp-internal",
        "github:warpdotdev/opencode-warp-internal",
        "opencode-wakatime",
        "@someone/opencode-warp",
    ] {
        assert_eq!(classify_plugin_entry(spec), None, "{spec}");
    }
}

#[test]
fn migrates_bare_npm_entry() {
    let spec = spec();
    let text = "{\n  \"plugin\": [\"@warp-dot-dev/opencode-warp\"]\n}\n";
    assert_eq!(
        migrate_plugin_entries(text, &test_bundle()).unwrap(),
        format!("{{\n  \"plugin\": [\"{spec}\"]\n}}\n")
    );
}

#[test]
fn migrates_pinned_npm_entry() {
    let spec = spec();
    let text = "{\n  \"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]\n}\n";
    assert_eq!(
        migrate_plugin_entries(text, &test_bundle()).unwrap(),
        format!("{{\n  \"plugin\": [\"{spec}\"]\n}}\n")
    );
}

#[test]
fn migration_keeps_other_plugins_comments_and_formatting() {
    let text = r#"{
  // my plugins
  "$schema": "https://opencode.ai/config.json",
  "plugin": [
    "opencode-wakatime",
    "@warp-dot-dev/opencode-warp@0.1.5", /* notifications */
    "file:///home/dev/my-plugin/index.ts",
  ],
  "model": "anthropic/claude-sonnet-4-5"
}
"#;
    let expected = text.replace("@warp-dot-dev/opencode-warp@0.1.5", &spec);
    assert_eq!(
        migrate_plugin_entries(text, &test_bundle()).unwrap(),
        expected
    );
}

#[test]
fn migration_keeps_tuple_options() {
    let spec = spec();
    let text = r#"{"plugin": [["@warp-dot-dev/opencode-warp", {"sound": false}]]}"#;
    assert_eq!(
        migrate_plugin_entries(text, &test_bundle()).unwrap(),
        format!(r#"{{"plugin": [["{spec}", {{"sound": false}}]]}}"#)
    );
}

#[test]
fn migration_drops_warp_entries_when_phosphor_is_already_listed() {
    let spec = spec();
    let text = format!("{{\"plugin\": [\"{spec}\", \"@warp-dot-dev/opencode-warp\", \"other\"]}}");
    assert_eq!(
        migrate_plugin_entries(&text, &test_bundle()).unwrap(),
        format!("{{\"plugin\": [\"{spec}\", \"other\"]}}")
    );

    let trailing =
        format!("{{\"plugin\": [\"other\", \"{spec}\", \"@warp-dot-dev/opencode-warp@0.1.5\"]}}");
    assert_eq!(
        migrate_plugin_entries(&trailing, &test_bundle()).unwrap(),
        format!("{{\"plugin\": [\"other\", \"{spec}\"]}}")
    );
}

#[test]
fn migration_collapses_duplicate_warp_entries_into_one() {
    let spec = spec();
    let text =
        r#"{"plugin": ["@warp-dot-dev/opencode-warp", "a", "@warp-dot-dev/opencode-warp@0.1.5"]}"#;
    assert_eq!(
        migrate_plugin_entries(text, &test_bundle()).unwrap(),
        format!(r#"{{"plugin": ["{spec}", "a"]}}"#)
    );
}

#[test]
fn migration_never_touches_a_developer_local_entry() {
    let text = r#"{"plugin": ["file:///home/dev/opencode-warp/src/index.ts", "other"]}"#;
    assert_eq!(migrate_plugin_entries(text, &test_bundle()), None);
}

#[test]
fn migration_ignores_entries_outside_the_top_level_plugin_array() {
    let text = r#"{"agent": {"plugin": ["@warp-dot-dev/opencode-warp"]}, "note": "@warp-dot-dev/opencode-warp"}"#;
    assert_eq!(migrate_plugin_entries(text, &test_bundle()), None);
}

#[test]
fn unreadable_config_is_left_alone() {
    assert_eq!(
        migrate_plugin_entries(
            r#"{"plugin": ["@warp-dot-dev/opencode-warp""#,
            &test_bundle()
        ),
        None
    );
    assert!(plugin_array_elements("[]").is_none());
}

fn manager_with(runner: &Arc<FakeRunner>) -> OpenCodePluginManager {
    OpenCodePluginManager::with_runner(runner.clone())
}

/// A folder shaped like the bundled plugin, at `<root>/bundled/opencode_plugin`.
fn make_bundle(root: &Path) -> PathBuf {
    let bundle = root.join("bundled").join("opencode_plugin");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("package.json"), "{}").unwrap();
    bundle
}

/// Runs the move with `OPENCODE_CONFIG_DIR` at `dir`, the bundled folder at `bundle` (a
/// missing folder when `None`) and the move-attempt flag reset; returns its result and
/// `needs_source_migration` afterwards.
fn drive_move(
    dir: &Path,
    bundle: Option<&Path>,
    runner: &Arc<FakeRunner>,
) -> (Result<(), String>, bool) {
    reset_source_move_attempt();
    let manager = manager_with(runner);
    let missing = dir.join("no-bundle-here");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var("OPENCODE_CONFIG_DIR", dir);
        std::env::set_var(BUNDLE_DIR_ENV, bundle.unwrap_or(&missing));
    }
    let result = futures::executor::block_on(manager.move_to_phosphor_source());
    let needs_move = manager.needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::remove_var("OPENCODE_CONFIG_DIR");
        std::env::remove_var(BUNDLE_DIR_ENV);
    }
    (result.map_err(|err| err.message), needs_move)
}

#[test]
#[serial_test::serial]
fn move_rewrites_the_global_config_with_no_command() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let spec = plugin_spec_for(&bundle).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let text = "{\n  \"plugin\": [\"other\", \"@warp-dot-dev/opencode-warp@0.1.5\"]\n}\n";
    write_config(dir.path(), "opencode.jsonc", text);
    let runner = Arc::new(FakeRunner::new());
    let manager = manager_with(&runner);
    reset_source_move_attempt();

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var("OPENCODE_CONFIG_DIR", dir.path());
        std::env::set_var(BUNDLE_DIR_ENV, &bundle);
    }
    let before = (
        manager.is_installed(),
        manager.needs_source_migration(),
        manager.needs_update(),
    );
    let result = futures::executor::block_on(manager.move_to_phosphor_source());
    let after = (
        manager.is_installed(),
        manager.needs_source_migration(),
        manager.needs_update(),
    );
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::remove_var("OPENCODE_CONFIG_DIR");
        std::env::remove_var(BUNDLE_DIR_ENV);
    }

    assert!(result.is_ok());
    // `needs_update` stays version-only (the reported version drives it).
    assert_eq!(before, (true, true, false));
    assert_eq!(after, (true, false, false));
    // Editing the config is the whole move: no git, no npm, no network step.
    assert!(runner.calls().is_empty(), "{:?}", runner.calls());
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.jsonc")).unwrap(),
        text.replace("@warp-dot-dev/opencode-warp@0.1.5", &spec)
    );
}

#[test]
#[serial_test::serial]
fn move_without_the_bundle_fails_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]}";
    write_config(dir.path(), "opencode.json", text);
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive_move(dir.path(), None, &runner);

    assert!(result.unwrap_err().contains("bundled"));
    assert!(runner.calls().is_empty());
    // No bundle to move to: nothing is offered either.
    assert!(!needs_move);
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.json")).unwrap(),
        text
    );
}

#[test]
#[serial_test::serial]
fn a_bundle_entry_from_another_install_location_is_moved_to_this_one() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let spec = plugin_spec_for(&bundle).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let old = "file:///opt/old-phosphor/resources/bundled/opencode_plugin";
    write_config(
        dir.path(),
        "opencode.json",
        &format!("{{\"plugin\": [\"{old}\"]}}"),
    );
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive_move(dir.path(), Some(&bundle), &runner);

    assert_eq!(result, Ok(()));
    assert!(!needs_move);
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.json")).unwrap(),
        format!("{{\"plugin\": [\"{spec}\"]}}")
    );
}

#[test]
#[serial_test::serial]
fn an_entry_for_the_current_bundle_needs_no_move() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let spec = plugin_spec_for(&bundle).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let text = format!("{{\"plugin\": [\"{spec}\"]}}");
    write_config(dir.path(), "opencode.json", &text);

    assert!(!file_needs_source_migration(
        &dir.path().join("opencode.json"),
        &bundle
    ));
    let (result, needs_move) = drive_move(dir.path(), Some(&bundle), &Arc::new(FakeRunner::new()));
    assert_eq!(result, Ok(()));
    assert!(!needs_move);
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.json")).unwrap(),
        text
    );
}

#[test]
#[serial_test::serial]
fn move_with_nothing_to_move_runs_nothing() {
    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"file:///home/dev/opencode-warp/src/index.ts\"]}";
    write_config(dir.path(), "opencode.json", text);
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive_move(dir.path(), Some(&bundle), &runner);

    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
    assert!(!needs_move);
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.json")).unwrap(),
        text
    );
}

/// B1: the rewrite goes through a temporary file in the target's directory;
/// when that cannot be created, the original is untouched.
#[cfg(unix)]
#[test]
#[serial_test::serial]
fn a_failed_write_leaves_the_config_untouched() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let bundle = make_bundle(root.path());
    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]}";
    write_config(dir.path(), "opencode.json", text);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive_move(dir.path(), Some(&bundle), &runner);
    let after = fs::read_to_string(dir.path().join("opencode.json")).unwrap();
    let entries = fs::read_dir(dir.path()).unwrap().count();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

    assert!(result.is_err());
    assert_eq!(after, text);
    assert_eq!(entries, 1, "no temporary file left behind");
}

/// B1: a symlinked config keeps its link; the file it points at is replaced.
#[cfg(unix)]
#[test]
fn a_symlinked_config_keeps_its_link() {
    let dotfiles = tempfile::tempdir().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]}";
    write_config(dotfiles.path(), "opencode.json", text);
    let link = config_dir.path().join("opencode.json");
    std::os::unix::fs::symlink(dotfiles.path().join("opencode.json"), &link).unwrap();

    write_config_atomically(&link, "{}").unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(dotfiles.path().join("opencode.json")).unwrap(),
        "{}"
    );
}

/// B7: removing a duplicate entry deletes only the entry, its comma and its
/// line's indentation -- never a comment between entries.
#[test]
fn removal_keeps_comments_between_entries() {
    let spec = spec();
    let text = format!(
        "{{\n  \"plugin\": [\n    \"{spec}\",\n    // the old notifications plugin\n    \"@warp-dot-dev/opencode-warp@0.1.5\", /* pinned */\n    \"b\"\n  ]\n}}\n"
    );
    assert_eq!(
        migrate_plugin_entries(&text, &test_bundle()).unwrap(),
        format!(
            "{{\n  \"plugin\": [\n    \"{spec}\",\n    // the old notifications plugin /* pinned */\n    \"b\"\n  ]\n}}\n"
        )
    );
}

#[test]
fn removing_the_last_entry_takes_the_comma_before_it() {
    let spec = spec();
    let text = format!(
        "{{\n  \"plugin\": [\n    \"{spec}\",\n    \"@warp-dot-dev/opencode-warp\"\n  ]\n}}\n"
    );
    assert_eq!(
        migrate_plugin_entries(&text, &test_bundle()).unwrap(),
        format!("{{\n  \"plugin\": [\n    \"{spec}\"\n  ]\n}}\n")
    );
}

#[test]
fn crlf_files_stay_crlf() {
    let spec = spec();
    let text = format!(
        "{{\r\n  \"plugin\": [\r\n    \"{spec}\",\r\n    \"@warp-dot-dev/opencode-warp\",\r\n    \"b\"\r\n  ]\r\n}}\r\n"
    );
    assert_eq!(
        migrate_plugin_entries(&text, &test_bundle()).unwrap(),
        format!("{{\r\n  \"plugin\": [\r\n    \"{spec}\",\r\n    \"b\"\r\n  ]\r\n}}\r\n")
    );
}

/// B7: a Warp entry with options is never dropped; with Phosphor's plain spec
/// already listed, the options entry is kept (rewritten) and the plain one
/// goes, so the options survive and the plugin loads once.
#[test]
fn options_survive_when_phosphor_is_already_listed() {
    let spec = spec();
    let text =
        format!(r#"{{"plugin": ["{spec}", ["@warp-dot-dev/opencode-warp", {{"sound": false}}]]}}"#);
    assert_eq!(
        migrate_plugin_entries(&text, &test_bundle()).unwrap(),
        format!(r#"{{"plugin": [["{spec}", {{"sound": false}}]]}}"#)
    );
}

fn write_config(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}
