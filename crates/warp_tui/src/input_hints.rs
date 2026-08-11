//! Mode-dependent ghosted placeholder hints for the TUI prompt input.
//!
//! Content policy for the input's empty-buffer ghost text: keybinding
//! guidance whose entries adapt to the transcript and orchestration state. The
//! keys referenced here are typed characters (`!`, `/`) or fixed navigation
//! keys (`←`, `Shift+↑`), not remappable bindings, so the strings are static;
//! binding-backed hints must resolve their keystroke display through the live
//! keymap instead (see `crate::keybindings::plan_toggle_hint`).

const ASK_AGENT_HINT: &str = "Ask the agent anything";
const ORCHESTRATION_HINT: &str = "Shift + ↑ for other agents";
const SHELL_MODE_HINT: &str = "! for shell mode";
const COMMANDS_HINT: &str = "/ for commands";
const CONVERSATIONS_HINT: &str = "← for conversations";
const HINT_SEPARATOR: &str = " • ";

/// Ghost text for an empty `!` shell-mode input: how to run and how to get
/// back to agent mode (esc is the input's contextual escape; backspace on the
/// empty input exits too).
pub(crate) const SHELL_HINT: &str = "Run a shell command • esc for agent mode";

/// Ghosted hint row shown in the input's slot while a user-controlled
/// long-running command owns input (the input box itself stays hidden),
/// advertising the live keybinding that manually attaches the agent to it.
/// `None` when no key is bound, so callers hide the row instead of showing an
/// unusable hint. Ported from the pin's `long_running_command_hint`
/// (`02b53fcd8`), which built the same string from the live
/// `AttachAgentToRunningCommand` binding rather than a fixed ctrl-c-to-interrupt
/// string -- ctrl-c does still interrupt the command (see
/// `TuiTerminalSessionView::handle_terminal_use_interrupt`), this is just the
/// discoverability hint for the *other* affordance available here.
pub(crate) fn long_running_command_hint(attach_key: Option<&str>) -> Option<String> {
    attach_key.map(|key| format!("{key}  to use agent"))
}

/// The agent-mode placeholder hint for the current transcript and orchestration
/// state.
pub(crate) fn agent_input_hint(
    transcript_is_empty: bool,
    orchestration_tabs_available: bool,
) -> String {
    let mut hints = Vec::with_capacity(4);
    if transcript_is_empty {
        if orchestration_tabs_available {
            hints.push(ORCHESTRATION_HINT);
        }
        hints.extend([COMMANDS_HINT, CONVERSATIONS_HINT]);
    } else {
        hints.push(ASK_AGENT_HINT);
        if orchestration_tabs_available {
            hints.push(ORCHESTRATION_HINT);
        }
        hints.extend([SHELL_MODE_HINT, COMMANDS_HINT]);
    }
    hints.join(HINT_SEPARATOR)
}

#[cfg(test)]
#[path = "input_hints_tests.rs"]
mod tests;
