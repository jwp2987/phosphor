//! Keys that scroll the scrollback while a foreground command runs, and at a prompt (#2080).
//!
//! While a command runs in the foreground, Page Up / Page Down / Home / End belong to the program
//! (`TerminalView::page_up` and its neighbours write them to the pty), so no key read back through
//! a build log or a `tail -f`: only the wheel and the scrollbar did. Other terminals keep the plain
//! keys for the program and scroll on the shifted ones; these four bindings do the same:
//!
//! | key              | scrolls              |
//! |------------------|----------------------|
//! | Shift+Page Up    | one page up          |
//! | Shift+Page Down  | one page down        |
//! | Shift+Home       | to the top           |
//! | Shift+End        | to the newest output |
//!
//! They are live where the block list is what the pane shows:
//!
//! * while a command runs and the terminal has the keys (round 15);
//! * at a prompt too (round 16, owner decision 2026-11-11), so the same four keys read back in
//!   both states. There the input editor has the focus and is asked first: Shift+Page Up /
//!   Shift+Page Down have no meaning in it and always arrive here; Shift+Home / Shift+End select
//!   to the line start / end in an input that holds text and arrive here only while the input is
//!   EMPTY (nothing to select; `EditorView` steps aside, see [`at_prompt_enabled`]);
//! * not in the alternate screen: a full-screen program (vim, less, tmux) has no scrollback of
//!   ours behind it and keeps the shifted keys (`CSI 5;2~` and the like), as before;
//! * not while a command runs AND the input editor has focus (the rich input of a CLI agent):
//!   unchanged from round 15.
//!
//! Since round 16 they are editable bindings (Settings > Keyboard shortcuts, `keybindings.yaml`):
//! `terminal:scrollback_page_up`, `terminal:scrollback_page_down`, `terminal:scrollback_top`,
//! `terminal:scrollback_bottom`.
//!
//! Nothing is written to the pty for these keys, so the scroll position is not snapped back to the
//! bottom by the key itself; the next key that is written to the program does that, as after a
//! wheel scroll.
//!
//! This is deliberately more than the upstream pin does (it has no such binding); recorded in
//! `DECLINED.md` (IMPROVED).

use warpui::keymap::{ContextPredicate, EditableBinding};

use crate::terminal::block_list_viewport::ScrollPositionUpdate;

/// Environment variable that, set to `0`, leaves the four bindings unregistered: the shifted keys
/// then go to the running program, as before #2080. Read once, when the bindings are registered.
pub(super) const SCROLLBACK_KEYS_ENV: &str = "PHOSPHOR_SCROLLBACK_KEYS";

/// Whether the bindings are registered: they are, unless the kill switch says `0`.
pub(super) fn scrollback_keys_env_enabled() -> bool {
    scrollback_keys_enabled_from(std::env::var(SCROLLBACK_KEYS_ENV).ok().as_deref())
}

/// [`scrollback_keys_env_enabled`] with the variable's value passed in (tests).
pub(super) fn scrollback_keys_enabled_from(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim() != "0")
}

/// Environment variable that, set to `0`, keeps the four keys to a running command only, as in
/// round 15: at a prompt they do what they did before (Shift+Home / Shift+End select in the
/// input, Shift+Page Up / Shift+Page Down do nothing). Read once, when the bindings are registered.
pub(crate) const SCROLLBACK_KEYS_AT_PROMPT_ENV: &str = "PHOSPHOR_SCROLLBACK_KEYS_AT_PROMPT";

/// Whether the keys also scroll at a prompt: they do, unless one of the two kill switches says
/// `0`. The input editor reads this too (`editor/view/mod.rs`): with it on, an EMPTY terminal
/// input leaves Shift+Home / Shift+End to the bindings here.
pub(crate) fn at_prompt_enabled() -> bool {
    at_prompt_enabled_from(
        std::env::var(SCROLLBACK_KEYS_ENV).ok().as_deref(),
        std::env::var(SCROLLBACK_KEYS_AT_PROMPT_ENV).ok().as_deref(),
    )
}

/// [`at_prompt_enabled`] with the two variables' values passed in (tests).
pub(crate) fn at_prompt_enabled_from(keys: Option<&str>, at_prompt: Option<&str>) -> bool {
    scrollback_keys_enabled_from(keys) && scrollback_keys_enabled_from(at_prompt)
}

/// One of the keys that scroll the scrollback under a running command and at a prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbackKey {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

impl ScrollbackKey {
    pub(super) const ALL: [Self; 4] = [Self::PageUp, Self::PageDown, Self::Top, Self::Bottom];

    /// The name of this key's editable binding.
    pub(super) fn binding_name(self) -> &'static str {
        match self {
            Self::PageUp => "terminal:scrollback_page_up",
            Self::PageDown => "terminal:scrollback_page_down",
            Self::Top => "terminal:scrollback_top",
            Self::Bottom => "terminal:scrollback_bottom",
        }
    }

    /// The binding's description in the keyboard shortcut settings.
    fn description(self) -> String {
        match self {
            Self::PageUp => crate::t!("keybinding-desc-terminal-scrollback-page-up"),
            Self::PageDown => crate::t!("keybinding-desc-terminal-scrollback-page-down"),
            Self::Top => crate::t!("keybinding-desc-terminal-scrollback-top"),
            Self::Bottom => crate::t!("keybinding-desc-terminal-scrollback-bottom"),
        }
    }

    /// The default keystroke of this key.
    pub(super) fn keystroke(self) -> &'static str {
        match self {
            Self::PageUp => "shift-pageup",
            Self::PageDown => "shift-pagedown",
            Self::Top => "shift-home",
            Self::Bottom => "shift-end",
        }
    }

    /// The scroll this key asks for: the same updates Page Up / Page Down / Home / End make where
    /// those keys scroll (a prompt, a selected block).
    pub(super) fn scroll_update(self) -> ScrollPositionUpdate {
        match self {
            Self::PageUp => ScrollPositionUpdate::AfterPageUp,
            Self::PageDown => ScrollPositionUpdate::AfterPageDown,
            Self::Top => ScrollPositionUpdate::AfterHome,
            Self::Bottom => ScrollPositionUpdate::AfterEnd,
        }
    }

    /// The key's name in the `hop=scrollback_key` log line.
    pub(super) fn hop_name(self) -> &'static str {
        match self {
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// Where the four keys are live. Always: the pane shows the block list, not the alternate screen
/// (`LongRunningCommand` is never set together with `AltScreen` in `TerminalView::keymap_context`;
/// the explicit exclusion keeps that true here whatever that function does later), and no input
/// method is composing.
///
/// * `at_prompt` false (round 15, and the kill switch): only while a command runs and the
///   terminal, not the input editor, has the keys.
/// * `at_prompt` true: also wherever no command runs (a prompt, with the input editor focused or
///   not). Under a running command the input editor's focus still keeps the keys out (the rich
///   input of a CLI agent). At a prompt the editor is asked before the terminal, so a key it
///   binds (Shift+Home / Shift+End in an input that holds text) never gets here.
pub(super) fn scrollback_key_predicate(at_prompt: bool) -> ContextPredicate {
    use warpui::keymap::macros::*;

    let block_list = id!("Terminal") & !id!("IMEOpen") & !id!("AltScreen");
    if at_prompt {
        block_list & (!id!("LongRunningCommand") | !id!("EditorFocused"))
    } else {
        block_list & id!("LongRunningCommand") & !id!("EditorFocused")
    }
}

/// The four editable bindings, in [`ScrollbackKey::ALL`] order.
pub(super) fn scrollback_key_bindings(at_prompt: bool) -> Vec<EditableBinding> {
    ScrollbackKey::ALL
        .iter()
        .map(|key| {
            EditableBinding::new(
                key.binding_name(),
                key.description(),
                super::TerminalAction::ScrollbackKey(*key),
            )
            .with_key_binding(key.keystroke())
            .with_context_predicate(scrollback_key_predicate(at_prompt))
        })
        .collect()
}

/// The detail of the `hop=scrollback_key` log line (input-order log, debug; the real-app case
/// `scroll_keys_foreground` reads it): `key=<page_up|page_down|top|bottom>`. A fixed word only.
pub(super) fn scrollback_key_hop_detail(key: ScrollbackKey) -> String {
    format!("key={}", key.hop_name())
}

#[cfg(test)]
#[path = "scrollback_keys_tests.rs"]
mod tests;
