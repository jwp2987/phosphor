use super::*;

/// Regression for jwp2987/phosphor#676 (upstream `b4a2a8faa`): `mcp_execution_path` is only
/// ever written by the GUI terminal bootstrap, so a TUI or SDK process on a fresh profile
/// must be allowed to inherit its launcher's PATH or no stdio MCP server can start.
#[test]
fn tui_and_sdk_can_inherit_process_path_for_mcp() {
    assert!(ExecutionMode::Tui.can_inherit_process_path_for_mcp());
    assert!(ExecutionMode::Sdk.can_inherit_process_path_for_mcp());
}

/// The desktop app keeps requiring the shell-derived PATH, so a missing one surfaces as the
/// actionable "open a new terminal session" toast rather than a spawn with the wrong PATH.
#[test]
fn desktop_app_requires_shell_derived_path_for_mcp() {
    assert!(!ExecutionMode::App.can_inherit_process_path_for_mcp());
}
