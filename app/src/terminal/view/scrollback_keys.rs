//! Keys that scroll the scrollback while a foreground command runs (#2080).
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
//! They are live only where the block list is what the pane shows and a command is running:
//!
//! * not in the alternate screen: a full-screen program (vim, less, tmux) has no scrollback of
//!   ours behind it and keeps the shifted keys (`CSI 5;2~` and the like), as before;
//! * not while the input editor has focus (the prompt, or the rich input of a CLI agent):
//!   Shift+Home / Shift+End select text there, and Page Up / Page Down already scroll at a prompt.
//!
//! Nothing is written to the pty for these keys, so the scroll position is not snapped back to the
//! bottom by the key itself; the next key that is written to the program does that, as after a
//! wheel scroll.
//!
//! This is deliberately more than the upstream pin does (it has no such binding); recorded in
//! `DECLINED.md` (IMPROVED).

use warpui::keymap::ContextPredicate;

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

/// One of the keys that scroll the scrollback under a running command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbackKey {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

impl ScrollbackKey {
    pub(super) const ALL: [Self; 4] = [Self::PageUp, Self::PageDown, Self::Top, Self::Bottom];

    /// The default (and only) keystroke of this key.
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

/// Where the four keys are live: the terminal has the keys (not the input editor), a command is
/// running and the pane shows the block list, not the alternate screen. `LongRunningCommand` is
/// never set together with `AltScreen` (`TerminalView::keymap_context`); the explicit exclusion
/// keeps that true here whatever that function does later.
pub(super) fn scrollback_key_predicate() -> ContextPredicate {
    use warpui::keymap::macros::*;

    id!("Terminal")
        & !id!("IMEOpen")
        & id!("LongRunningCommand")
        & !id!("AltScreen")
        & !id!("EditorFocused")
}

/// The detail of the `hop=scrollback_key` log line (input-order log, debug; the real-app case
/// `scroll_keys_foreground` reads it): `key=<page_up|page_down|top|bottom>`. A fixed word only.
pub(super) fn scrollback_key_hop_detail(key: ScrollbackKey) -> String {
    format!("key={}", key.hop_name())
}

#[cfg(test)]
#[path = "scrollback_keys_tests.rs"]
mod tests;
