use std::fs;

use std::path::Path;

use super::{
    ClaudeCodePluginManager, CliAgentPluginManager, MARKETPLACE_URL, check_installed,
    claude_code_marketplace_has_local_override, installed_version,
    marketplace_registered_from_warp, plugin_needs_source_migration, plugin_needs_update,
};

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
fn registered_from_warp_when_settings_declare_github_source() {
    let dir = tempfile::tempdir().unwrap();
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    assert!(marketplace_registered_from_warp(dir.path()));
}

#[test]
fn registered_from_warp_when_settings_hold_bare_shorthand() {
    let dir = tempfile::tempdir().unwrap();
    write_settings_source(dir.path(), serde_json::json!("warpdotdev/claude-code-warp"));

    assert!(marketplace_registered_from_warp(dir.path()));
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
    write_settings_source(
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
fn current_plugin_from_warp_source_needs_update() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.4.0");
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    assert!(plugin_needs_source_migration(dir.path()));
    assert!(plugin_needs_update(dir.path()));
}

#[test]
fn current_plugin_from_phosphor_source_does_not_need_update() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "2.4.0");
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );

    assert!(!plugin_needs_source_migration(dir.path()));
    assert!(!plugin_needs_update(dir.path()));
}

#[test]
fn outdated_plugin_from_phosphor_source_still_needs_update() {
    let dir = tempfile::tempdir().unwrap();
    write_installed_plugin_version(dir.path(), "1.5.0");
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "git", "url": MARKETPLACE_URL}),
    );

    assert!(plugin_needs_update(dir.path()));
}

#[test]
fn warp_marketplace_without_installed_plugin_needs_no_migration() {
    // Not installed: the install path, not the update path, handles it.
    let dir = tempfile::tempdir().unwrap();
    write_settings_source(
        dir.path(),
        serde_json::json!({"source": "github", "repo": "warpdotdev/claude-code-warp"}),
    );

    assert!(marketplace_registered_from_warp(dir.path()));
    assert!(!plugin_needs_source_migration(dir.path()));
}

#[test]
#[serial_test::serial]
fn needs_source_migration_via_trait_uses_claude_config_dir() {
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

    assert_eq!(before, (true, true));
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
