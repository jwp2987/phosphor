//! Where a key typed in a terminal pane goes (#1404).
//!
//! The terminal view is the pane's key sink (`warpui::pane_key`): it receives every key event
//! once, with the live focus chain, and decides the destination here, from facts. Elements and
//! the input's editor no longer "claim" keys; focus decides only which of the pane's two
//! destinations a key is shown at (the input editor or the terminal) and which card it belongs
//! to, never whether the key is delivered.
//!
//! One pure table, testable alone:
//!
//! * [`key_target`]: given what has focus, is the key the pane's (and for the editor or the
//!   terminal), or a text field inside the pane's (find bar, block filter, any other editor)?
//!
//! Where a key on the terminal destination goes from there (the pty, the editor, a hold) is the
//! destination table's (`input_queue/table.rs`, #1424). The ordered rows that used to be written
//! here (`route_typed`, `after_pty_attempt`) were the legacy typed path and went with it (#1353
//! step 8).

use std::sync::OnceLock;

/// `PHOSPHOR_PANE_KEY`: `0`/`off` takes the sink out (the elements take the key if the terminal
/// itself has focus, and each editor its own, as before the sink), `shadow` makes the sink decide
/// and log (`kh_pane_key_shadow`) but leave delivery to the elements, anything else is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneKeyMode {
    Off,
    Shadow,
    On,
}

impl PaneKeyMode {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("0") | Some("off") | Some("false") => Self::Off,
            Some("shadow") => Self::Shadow,
            _ => Self::On,
        }
    }
}

pub fn mode() -> PaneKeyMode {
    static MODE: OnceLock<PaneKeyMode> = OnceLock::new();
    *MODE.get_or_init(|| PaneKeyMode::parse(std::env::var("PHOSPHOR_PANE_KEY").ok().as_deref()))
}

/// Whether the pane sink delivers the keys (so the elements and the input's editor must not).
pub fn sink_owns_keys() -> bool {
    mode() == PaneKeyMode::On
}

/// The focus facts a key is routed on, read from the live focus chain when the key arrives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaneFocusFacts {
    /// Focus is in a text field inside the pane that is not the input's editor (the find bar, the
    /// block filter, any other editor): the key is that field's.
    pub in_text_field: bool,
    /// Focus is in the input's editor.
    pub editor_focused: bool,
    /// Focus is on the terminal view itself.
    pub terminal_focused: bool,
    /// The pane holds the keys typed during an ssh connect as one ordered hold. Every key of the
    /// pane joins it through the terminal, whichever of its views has focus (#1400).
    pub ssh_hold_active: bool,
    /// Focus is on a settled AI block or action card (#1095): a pane child that binds none of the
    /// control keys, so a control key typed there is the terminal's (#1552, Fable B1/F9). Any
    /// other pane child with focus (a pending approval card, an AI block awaiting confirmation,
    /// the ssh choice card, a question card) binds its own Enter/Tab/Escape and keeps them.
    pub settled_ai_focus: bool,
    /// Focus is on the input view itself, or a view inside it, but not its editor, and no card or
    /// other pane child holds it (#1552, owner decision (a)). The input's editor binds nothing
    /// there, so a control key typed now follows the typed keys of the gap through the terminal's
    /// routing, to the same destination as the line it ends.
    pub input_gap_focus: bool,
}

/// What kind of key event is being routed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyClass {
    /// Printable text (typed characters, an IME commit).
    Typed,
    /// A control character: Enter, Backspace, Ctrl-C, ...
    Control,
    /// IME composition text (set / clear marked text).
    Composition,
}

/// Where the sink sends a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTarget {
    /// Not the pane's: the event continues to the elements (the text field has it).
    NotMine,
    /// The input's editor, through the editor's own edit pipeline.
    Editor,
    /// The terminal view's routing (`typed_characters_on_terminal` and its rows).
    Terminal,
}

pub fn key_target(class: KeyClass, facts: &PaneFocusFacts) -> KeyTarget {
    if facts.in_text_field {
        return KeyTarget::NotMine;
    }
    match class {
        // The hold is one ordered queue offered through the terminal, so the editor's focus does
        // not keep a key from it (#1400); otherwise the focused editor has the key, and every
        // other focus in the pane (the terminal, a card, a settled AI block, a gap between two
        // views) is delivered through the terminal's routing instead of being dropped (#1397).
        KeyClass::Typed => {
            if facts.ssh_hold_active {
                KeyTarget::Terminal
            } else if facts.editor_focused {
                KeyTarget::Editor
            } else {
                KeyTarget::Terminal
            }
        }
        // The editor's control keys are its keybindings: they stay with the editor. A control key
        // typed while the terminal itself has focus is the terminal's (#1411): the sink translates
        // it into bytes from the published terminal modes and routes it like the typed keys, so
        // no element needs the model's lock to do it. In a focus gap only the ssh hold (#1332,
        // #1394), a settled AI target (#1552) or focus on the input view outside its editor (#1552
        // (a)) takes it, to follow the typed keys of the gap
        // through the terminal's routing (#1397) instead of leaving through the element tree to
        // the input's keybinding on an empty editor; any other pane child with focus binds its
        // own control keys. The terminal's table then decides: the pty, the editor's submit, or
        // a drop that puts focus back on the input.
        KeyClass::Control => {
            if facts.editor_focused {
                KeyTarget::NotMine
            } else if facts.terminal_focused
                || facts.ssh_hold_active
                || facts.settled_ai_focus
                || facts.input_gap_focus
            {
                KeyTarget::Terminal
            } else {
                KeyTarget::NotMine
            }
        }
        // A composition is shown where it will be committed.
        KeyClass::Composition => {
            if facts.editor_focused {
                KeyTarget::Editor
            } else if facts.terminal_focused {
                KeyTarget::Terminal
            } else {
                KeyTarget::NotMine
            }
        }
    }
}

/// #1552 (b): whether the prompt after a long-running command hands focus back. Only when focus
/// is inside the pane (`focus_inside_pane`) and in a gap there: not the editor, not the terminal
/// view, not a card or AI block that keeps its own keys (those are named by `focus`). The
/// redetermination that follows still never takes focus from the find bar, the block filter, an
/// AI block the user navigates or a queued-prompt editor.
pub fn hands_back_focus_at_prompt(
    focus: crate::terminal::input_queue::FocusSnap,
    focus_inside_pane: bool,
) -> bool {
    focus_inside_pane && focus == crate::terminal::input_queue::FocusSnap::Other
}

#[cfg(test)]
#[path = "pane_key_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pane_key_snapshot_tests.rs"]
mod snapshot_tests;
