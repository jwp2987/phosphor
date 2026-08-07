use super::CodexPluginManager;
use crate::terminal::cli_agent_sessions::plugin_manager::CliAgentPluginManager;

#[test]
fn can_auto_install_is_false() {
    assert!(!CodexPluginManager.can_auto_install());
}

#[test]
fn does_not_support_update() {
    assert!(!CodexPluginManager.supports_update());
}

#[test]
fn install_instructions_has_steps() {
    let instructions = CodexPluginManager.install_instructions();
    assert!(!instructions.steps.is_empty());
    assert!(!instructions.title.is_empty());
}

#[test]
fn minimum_version_is_zero() {
    // The fork ships no Codex plugin, so there is no minimum version to enforce.
    assert_eq!(CodexPluginManager.minimum_plugin_version(), "0.0.0");
}

#[test]
fn install_instructions_are_native() {
    // Without a plugin, the only install path is enabling Codex's own in-focus
    // notifications via `~/.codex/config.toml`.
    let instructions = CodexPluginManager.install_instructions();
    assert_eq!(instructions.steps.len(), 2);
    assert_eq!(
        instructions.steps[1].command,
        "[tui]\nnotification_condition = \"always\""
    );
    assert!(!instructions.steps[1].executable);
}

#[test]
fn update_instructions_are_empty() {
    let instructions = CodexPluginManager.update_instructions();
    assert!(instructions.steps.is_empty());
    assert!(instructions.title.is_empty());
    assert!(instructions.post_install_notes.is_empty());
}
