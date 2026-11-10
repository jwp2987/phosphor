//! Escape in the agent view, in a pane whose keys are vi keys (#1984, owner decision 2026-11-10).
//!
//! Escape in the agent view leaves it for the terminal. That is kept. A user of vi keys presses
//! Escape as a habit ("make sure I am in normal mode"), in the input editor's vim normal mode or
//! out of a shell that edits its line in vi mode, and each such press threw them out of the
//! conversation. In a pane like that the Escape belongs to the vi keys and the view stays.
//!
//! What "vi keys" is read from, both facts the app already holds:
//!
//! * the input editor has vim keybindings on (`text_editing.vim_mode_enabled`, and the editor
//!   supports them). In insert, visual or replace mode, or with a pending operator, the editor
//!   takes the Escape itself and it never reaches the exit; this rule covers what is left, an
//!   Escape in normal mode with nothing pending;
//! * the pane's shell reported vi line editing when it bootstrapped (bash `set -o vi`, fish vi
//!   key bindings, PSReadLine `EditMode Vi`, a zsh vi plugin): the same facts the "enable vim
//!   keybindings?" banner is offered on.
//!
//! Not changed: an Escape the view answers for a running command (it stops the agent's turn while
//! the agent's own command runs, #850), a full-screen program or any running command that has
//! the keys (its Escape goes to the pty and never reaches this decision), the menus and attached
//! context an Escape closes first, and every other way out of the view: the back button, the
//! "go back to terminal" row of the zero state, and Ctrl-C on an empty input, pressed twice.

/// Environment variable that, set to `0`, turns this rule off: Escape then leaves the agent view
/// in every pane, as before #1984.
pub(super) const VI_ESCAPE_STAYS_ENV: &str = "PHOSPHOR_VI_ESCAPE_STAYS";

/// Whether the rule is on: it is, unless the kill switch says `0`.
pub(super) fn vi_escape_stays_env_enabled() -> bool {
    vi_escape_stays_enabled_from(std::env::var(VI_ESCAPE_STAYS_ENV).ok().as_deref())
}

/// [`vi_escape_stays_env_enabled`] with the variable's value passed in (tests).
pub(super) fn vi_escape_stays_enabled_from(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim() != "0")
}

/// What the pane says about its keys when an Escape reaches the agent view's exit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ViKeyFacts {
    /// The input editor has vim keybindings on.
    pub editor_vim_keys: bool,
    /// The pane's shell reported vi line editing at bootstrap.
    pub shell_vi_mode: bool,
}

/// What an Escape that reached the agent view's exit does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AgentViewEscape {
    /// Leave the agent view (or walk up to the parent conversation): the behaviour of every pane
    /// before #1984, and of a pane without vi keys now.
    Leave,
    /// The Escape is the vi keys': the view stays.
    StaysWithViKeys,
}

/// Decides an Escape that reached the agent view's exit.
///
/// `view_can_be_left_now` is the existing gate: the view can be exited and no command is running.
/// Where it is false the Escape has another meaning already (it stops the agent's turn, or the
/// view cannot be left at all) and this rule does not apply.
pub(super) fn agent_view_escape(
    facts: ViKeyFacts,
    view_can_be_left_now: bool,
    rule_enabled: bool,
) -> AgentViewEscape {
    if rule_enabled && view_can_be_left_now && (facts.editor_vim_keys || facts.shell_vi_mode) {
        AgentViewEscape::StaysWithViKeys
    } else {
        AgentViewEscape::Leave
    }
}

#[cfg(test)]
#[path = "agent_escape_tests.rs"]
mod tests;
