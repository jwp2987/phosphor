use std::fs;
use std::path::Path;

use super::{
    CliAgentPluginManager, EXTENSION_REPO, GeminiPluginManager, MINIMUM_PLUGIN_VERSION,
    check_installed, compare_versions, installed_from_warp, installed_version,
    plugin_needs_source_migration, plugin_needs_update,
};

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

#[test]
fn update_instructions_reinstall_from_phosphor_source() {
    // `extensions update` would re-fetch from the recorded source (Warp's
    // GitHub, for older installs), so the manual update reinstalls instead.
    let instructions = GeminiPluginManager::new(None, None, None).update_instructions();
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
    assert!(plugin_needs_update(dir.path()));
}

#[test]
fn current_extension_from_phosphor_copy_needs_no_update() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), MINIMUM_PLUGIN_VERSION);
    write_install_metadata(dir.path(), EXTENSION_REPO, "git");

    assert!(!plugin_needs_source_migration(dir.path()));
    assert!(!plugin_needs_update(dir.path()));
}

#[test]
fn outdated_extension_from_phosphor_copy_still_needs_update() {
    let dir = tempfile::tempdir().unwrap();
    write_extension(dir.path(), "0.9.0");
    write_install_metadata(dir.path(), EXTENSION_REPO, "git");

    assert!(plugin_needs_update(dir.path()));
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
