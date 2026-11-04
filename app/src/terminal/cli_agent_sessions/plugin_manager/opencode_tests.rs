use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::{
    OpenCodePluginManager, PLUGIN_SPEC, PluginEntryKind, classify_plugin_entry,
    migrate_plugin_entries, plugin_array_elements,
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

#[test]
fn can_auto_update_is_true() {
    assert!(manager().can_auto_update());
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

#[test]
#[serial_test::serial]
fn update_rewrites_the_global_config_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let text = "{\n  \"plugin\": [\"other\", \"@warp-dot-dev/opencode-warp@0.1.5\"]\n}\n";
    write_config(dir.path(), "opencode.jsonc", text);
    let runner = Arc::new(FakeRunner::new());
    let manager = manager_with(&runner);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("OPENCODE_CONFIG_DIR", dir.path()) };
    let before = (
        manager.is_installed(),
        manager.needs_source_migration(),
        manager.needs_update(),
    );
    let result = futures::executor::block_on(manager.update());
    let after = (
        manager.is_installed(),
        manager.needs_source_migration(),
        manager.needs_update(),
    );
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("OPENCODE_CONFIG_DIR") };

    assert!(result.is_ok());
    assert_eq!(before, (true, true, true));
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
fn update_changes_nothing_when_the_preflight_fails() {
    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"@warp-dot-dev/opencode-warp@0.1.5\"]}";
    write_config(dir.path(), "opencode.json", text);
    let runner = Arc::new(FakeRunner::new().failing("git "));
    let manager = manager_with(&runner);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("OPENCODE_CONFIG_DIR", dir.path()) };
    let result = futures::executor::block_on(manager.update());
    let still_needs_move = manager.needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("OPENCODE_CONFIG_DIR") };

    assert!(result.is_err());
    assert!(still_needs_move);
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.json")).unwrap(),
        text
    );
}

#[test]
#[serial_test::serial]
fn update_fails_without_an_entry_to_move() {
    let dir = tempfile::tempdir().unwrap();
    let text = "{\"plugin\": [\"file:///home/dev/opencode-warp/src/index.ts\"]}";
    write_config(dir.path(), "opencode.json", text);
    let runner = Arc::new(FakeRunner::new());
    let manager = manager_with(&runner);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("OPENCODE_CONFIG_DIR", dir.path()) };
    let result = futures::executor::block_on(manager.update());
    let needs_migration = manager.needs_source_migration();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("OPENCODE_CONFIG_DIR") };

    assert!(result.is_err());
    assert!(!needs_migration);
    assert_eq!(
        fs::read_to_string(dir.path().join("opencode.json")).unwrap(),
        text
    );
}

fn write_config(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}
