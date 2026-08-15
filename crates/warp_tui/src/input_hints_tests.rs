use super::{
    ASK_AGENT_HINT, COMMANDS_HINT, CONVERSATIONS_HINT, SHELL_HINT, SHELL_MODE_HINT, SHORTCUTS_HINT,
    agent_input_hint, long_running_command_hint,
};

#[test]
fn transcript_state_selects_the_applicable_hint_segments() {
    let zero_state = agent_input_hint(true, false);
    assert!(zero_state.contains(COMMANDS_HINT));
    assert!(zero_state.contains(CONVERSATIONS_HINT));
    assert!(zero_state.contains(SHORTCUTS_HINT));
    assert!(!zero_state.contains(ASK_AGENT_HINT));
    assert!(!zero_state.contains(SHELL_MODE_HINT));

    let started = agent_input_hint(false, false);
    assert!(started.contains(ASK_AGENT_HINT));
    assert!(started.contains(SHORTCUTS_HINT));
    assert!(started.contains(SHELL_MODE_HINT));
    assert!(started.contains(COMMANDS_HINT));
    assert!(!started.contains(CONVERSATIONS_HINT));
}

/// The shell-mode ghost text advertises the same `?` overlay the agent-mode
/// hint does. Both were present in the pin but only in the unwired copy under
/// `terminal_session_view::state`, so the rendered hints silently lost them.
#[test]
fn shell_hint_advertises_the_shortcuts_overlay() {
    assert!(SHELL_HINT.contains(SHORTCUTS_HINT));
}

#[test]
fn long_running_command_hint_needs_a_bound_key() {
    assert_eq!(long_running_command_hint(None), None);
    assert_eq!(
        long_running_command_hint(Some("Ctrl + Shift + \u{23ce}")),
        Some("Ctrl + Shift + \u{23ce}  to use agent".to_owned())
    );
}
