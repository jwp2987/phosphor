use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::{
    OpenCodePluginManager, PLUGIN_SPEC, PluginEntryKind, classify_plugin_entry,
    migrate_plugin_entries, plugin_array_elements, reset_source_move_attempt,
    write_config_atomically,
};
use crate::terminal::cli_agent_sessions::plugin_manager::CliAgentPluginManager;
use crate::terminal::cli_agent_sessions::plugin_manager::tests::{FakeRunner, preflight_command};

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

#[test]
fn plugin_spec_is_a_git_spec_for_the_gitlab_tag() {
    // `git+https:` is what npm-package-arg's `fromURL` classifies as git.
    assert_eq!(
        PLUGIN_SPEC,
        "git+https://gitlab.com/winters-phosphor/opencode-warp.git#v0.2.0"
    );
    assert_eq!(
        classify_plugin_entry(PLUGIN_SPEC),
        Some(PluginEntryKind::Phosphor)
    );
}

#[test]
fn instructions_name_the_gitlab_spec() {
    let expected = format!("\"plugin\": [\"{PLUGIN_SPEC}\"]");
    assert_eq!(manager().install_instructions().steps[1].command, expected);
    assert_eq!(manager().update_instructions().steps[1].command, expected);
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
fn classifies_phosphor_spellings() {
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
    let text = "{\n  \"plugin\": [\"@warp-dot-dev/opencode-warp\"]\n}\n";
    assert_eq!(
        migrate_plugin_entries(text).unwrap(),
        format!("{{\n  \"plugin\": [\"{PLUGIN_SPEC}\"]\n}}\n")
    );
}

#[test]
fn migrates_pinned_npm_entry() {
    let text = "{\n  \"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]\n}\n";
    assert_eq!(
        migrate_plugin_entries(text).unwrap(),
        format!("{{\n  \"plugin\": [\"{PLUGIN_SPEC}\"]\n}}\n")
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
    let expected = text.replace("@warp-dot-dev/opencode-warp@0.1.5", PLUGIN_SPEC);
    assert_eq!(migrate_plugin_entries(text).unwrap(), expected);
}

#[test]
fn migration_keeps_tuple_options() {
    let text = r#"{"plugin": [["@warp-dot-dev/opencode-warp", {"sound": false}]]}"#;
    assert_eq!(
        migrate_plugin_entries(text).unwrap(),
        format!(r#"{{"plugin": [["{PLUGIN_SPEC}", {{"sound": false}}]]}}"#)
    );
}

#[test]
fn migration_drops_warp_entries_when_phosphor_is_already_listed() {
    let text =
        format!("{{\"plugin\": [\"{PLUGIN_SPEC}\", \"@warp-dot-dev/opencode-warp\", \"other\"]}}");
    assert_eq!(
        migrate_plugin_entries(&text).unwrap(),
        format!("{{\"plugin\": [\"{PLUGIN_SPEC}\", \"other\"]}}")
    );

    let trailing = format!(
        "{{\"plugin\": [\"other\", \"{PLUGIN_SPEC}\", \"@warp-dot-dev/opencode-warp@0.1.5\"]}}"
    );
    assert_eq!(
        migrate_plugin_entries(&trailing).unwrap(),
        format!("{{\"plugin\": [\"other\", \"{PLUGIN_SPEC}\"]}}")
    );
}

#[test]
fn migration_collapses_duplicate_warp_entries_into_one() {
    let text =
        r#"{"plugin": ["@warp-dot-dev/opencode-warp", "a", "@warp-dot-dev/opencode-warp@0.1.5"]}"#;
    assert_eq!(
        migrate_plugin_entries(text).unwrap(),
        format!(r#"{{"plugin": ["{PLUGIN_SPEC}", "a"]}}"#)
    );
}

#[test]
fn migration_never_touches_a_local_entry() {
    let text = r#"{"plugin": ["file:///home/dev/opencode-warp/src/index.ts", "other"]}"#;
    assert_eq!(migrate_plugin_entries(text), None);
}

#[test]
fn migration_ignores_entries_outside_the_top_level_plugin_array() {
    let text = r#"{"agent": {"plugin": ["@warp-dot-dev/opencode-warp"]}, "note": "@warp-dot-dev/opencode-warp"}"#;
    assert_eq!(migrate_plugin_entries(text), None);
}

#[test]
fn unreadable_config_is_left_alone() {
    assert_eq!(
        migrate_plugin_entries(r#"{"plugin": ["@warp-dot-dev/opencode-warp""#),
        None
    );
    assert!(plugin_array_elements("[]").is_none());
}

fn manager_with(runner: &Arc<FakeRunner>) -> OpenCodePluginManager {
    OpenCodePluginManager::with_runner(runner.clone())
}

/// Runs the move with `OPENCODE_CONFIG_DIR` at `dir` and the move-attempt
/// flag reset; returns its result and `needs_source_migration` afterwards.
fn drive_move(dir: &Path, runner: &Arc<FakeRunner>) -> (Result<(), String>, bool) {
    reset_source_move_attempt();
    let manager = manager_with(runner);
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("OPENCODE_CONFIG_DIR", dir) };
    let result = futures::executor::block_on(manager.move_to_phosphor_source());
    let needs_move = manager.needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("OPENCODE_CONFIG_DIR") };
    (result.map_err(|err| err.message), needs_move)
}

#[test]
#[serial_test::serial]
fn move_rewrites_the_global_config() {
    let dir = tempfile::tempdir().unwrap();
    let text = "{\n  \"plugin\": [\"other\", \"@warp-dot-dev/opencode-warp@0.1.5\"]\n}\n";
    write_config(dir.path(), "opencode.jsonc", text);
    let runner = Arc::new(FakeRunner::new());
    let manager = manager_with(&runner);
    reset_source_move_attempt();

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("OPENCODE_CONFIG_DIR", dir.path()) };
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
    unsafe { std::env::remove_var("OPENCODE_CONFIG_DIR") };

    assert!(result.is_ok());
    // `needs_update` stays version-only (the reported version drives it).
    assert_eq!(before, (true, true, false));
    assert_eq!(after, (true, false, false));
    assert_eq!(
        runner.calls(),
        vec![preflight_command(
            "https://gitlab.com/winters-phosphor/opencode-warp.git"
        )]
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.jsonc")).unwrap(),
        text.replace("@warp-dot-dev/opencode-warp@0.1.5", PLUGIN_SPEC)
    );
}

#[test]
#[serial_test::serial]
fn move_changes_nothing_when_the_preflight_fails() {
    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]}";
    write_config(dir.path(), "opencode.json", text);
    let runner = Arc::new(FakeRunner::new().failing("git "));

    let (result, needs_move) = drive_move(dir.path(), &runner);

    assert!(result.is_err());
    // Nothing was attempted, so the chip is still offered (B3).
    assert!(needs_move);
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.json")).unwrap(),
        text
    );
}

#[test]
#[serial_test::serial]
fn move_with_nothing_to_move_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"file:///home/dev/opencode-warp/src/index.ts\"]}";
    write_config(dir.path(), "opencode.json", text);
    let runner = Arc::new(FakeRunner::new());

    let (result, needs_move) = drive_move(dir.path(), &runner);

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

    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]}";
    write_config(dir.path(), "opencode.json", text);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let runner = Arc::new(FakeRunner::new());

    let (result, _) = drive_move(dir.path(), &runner);
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
    let text = format!(
        "{{\n  \"plugin\": [\n    \"{PLUGIN_SPEC}\",\n    // the old notifications plugin\n    \"@warp-dot-dev/opencode-warp@0.1.5\", /* pinned */\n    \"b\"\n  ]\n}}\n"
    );
    assert_eq!(
        migrate_plugin_entries(&text).unwrap(),
        format!(
            "{{\n  \"plugin\": [\n    \"{PLUGIN_SPEC}\",\n    // the old notifications plugin /* pinned */\n    \"b\"\n  ]\n}}\n"
        )
    );
}

#[test]
fn removing_the_last_entry_takes_the_comma_before_it() {
    let text = format!(
        "{{\n  \"plugin\": [\n    \"{PLUGIN_SPEC}\",\n    \"@warp-dot-dev/opencode-warp\"\n  ]\n}}\n"
    );
    assert_eq!(
        migrate_plugin_entries(&text).unwrap(),
        format!("{{\n  \"plugin\": [\n    \"{PLUGIN_SPEC}\"\n  ]\n}}\n")
    );
}

#[test]
fn crlf_files_stay_crlf() {
    let text = format!(
        "{{\r\n  \"plugin\": [\r\n    \"{PLUGIN_SPEC}\",\r\n    \"@warp-dot-dev/opencode-warp\",\r\n    \"b\"\r\n  ]\r\n}}\r\n"
    );
    assert_eq!(
        migrate_plugin_entries(&text).unwrap(),
        format!("{{\r\n  \"plugin\": [\r\n    \"{PLUGIN_SPEC}\",\r\n    \"b\"\r\n  ]\r\n}}\r\n")
    );
}

/// B7: a Warp entry with options is never dropped; with Phosphor's plain spec
/// already listed, the options entry is kept (rewritten) and the plain one
/// goes, so the options survive and the plugin loads once.
#[test]
fn options_survive_when_phosphor_is_already_listed() {
    let text = format!(
        r#"{{"plugin": ["{PLUGIN_SPEC}", ["@warp-dot-dev/opencode-warp", {{"sound": false}}]]}}"#
    );
    assert_eq!(
        migrate_plugin_entries(&text).unwrap(),
        format!(r#"{{"plugin": [["{PLUGIN_SPEC}", {{"sound": false}}]]}}"#)
    );
}

fn write_config(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}
