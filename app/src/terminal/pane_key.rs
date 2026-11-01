//! Where a key typed in a terminal pane goes (#1404).
//!
//! The terminal view is the pane's key sink (`warpui::pane_key`): it receives every key event
//! once, with the live focus chain, and decides the destination here, from facts. Elements and
//! the input's editor no longer "claim" keys; focus decides only which of the pane's two
//! destinations a key is shown at (the input editor or the terminal) and which card it belongs
//! to, never whether the key is delivered.
//!
//! Two pure tables, each testable alone:
//!
//! * [`key_target`]: given what has focus, is the key the pane's (and for the editor or the
//!   terminal), or a text field inside the pane's (find bar, block filter, any other editor)?
//! * [`route_typed`]: given the terminal destination, the ordered rows `typed_characters_on_terminal`
//!   used to encode as ten sequential `if`s (ssh choice card, ssh interrupt, ssh setup hold, enter
//!   queue, PowerShell setup hold, approval card, agent-owned pty, pty, hidden input box). The
//!   order is written once, here. The probes it asks may act when they answer yes (a hold queues
//!   the key), exactly as the inline checks did.

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
        // The editor's control keys are its keybindings, and a terminal that has focus itself
        // translates its keys into escape sequences (that needs the terminal's modes, which the
        // element has): both stay with the elements. What nothing in the elements takes is a
        // control key typed in a focus gap while the ssh hold needs it (#1332, #1394): the sink
        // delivers it to the hold.
        KeyClass::Control => {
            if facts.ssh_hold_active && !facts.editor_focused && !facts.terminal_focused {
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

/// The questions the terminal destination's rows ask, in the order [`route_typed`] asks them.
/// A probe that answers yes may already have acted (queued or held the key, shown a hint): that is
/// what the inline checks did, and the row then ends the route.
pub trait TypedRouteProbes {
    /// The ssh-extension choice card holds focus: the key shows its "choose first" hint.
    fn ssh_choice_card_up(&mut self) -> bool;
    /// An interrupt key (Ctrl-C, Ctrl-D, Ctrl-Z, Ctrl-\) that must not reach the pty now (the
    /// remote bootstrap or the connect window). Notes the hold when it answers yes.
    fn ssh_interrupt_held(&mut self) -> bool;
    /// An interrupt key passes: what was typed before it is stale, as the tty's flush on ^C.
    fn ssh_interrupt_passes(&mut self);
    /// The ssh setup/connect owns the keyboard: the key is held (or dropped, with the dialog up).
    fn ssh_setup_holds_key(&mut self) -> bool;
    /// An Enter's gate holds the pane's keys: the key queues behind it.
    fn enter_queue_takes_key(&mut self) -> bool;
    /// Collapses the ssh warpify UX the key would hide (runs for every key that gets this far).
    fn note_warpification_key(&mut self);
    /// A PowerShell subshell setup holds every key (#1048).
    fn powershell_setup_holds(&mut self) -> bool;
    /// The pending approval card, or an AI block awaiting confirmation, holds focus (#1028).
    fn on_approval_card(&mut self) -> bool;
    /// A settled AI block or action card holds focus (#1095).
    fn on_settled_ai_target(&mut self) -> bool;
    /// The agent drives the running command: typing is for the agent's input (#1179).
    fn agent_owns_typing(&mut self) -> bool;
    /// The pty takes user bytes right now.
    fn pty_takes_input(&mut self) -> bool;
    /// The input box is shown.
    fn input_box_visible(&mut self) -> bool;
}

/// What the rows decided for a key on the terminal destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedRoute {
    /// Shown the ssh choice card's hint; nothing else happens to the key.
    SshChoiceCardHint,
    SshInterruptHeld,
    SshSetupHeld,
    QueuedBehindEnter,
    PowerShellHeld,
    /// Neither held nor queued: the pty if it takes it, else the editor (see [`after_pty_attempt`]).
    Deliver(TypedTail),
}

/// The facts the end of the route needs once the pty has been tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypedTail {
    pub approval_card: bool,
    pub settled_ai_target: bool,
    pub agent_owns_typing: bool,
    /// Write the key to the pty (the pty takes it and neither the card nor the agent owns it).
    pub try_pty: bool,
}

/// The ordered rows for text the terminal destination received. `interrupt_key` is the key being
/// one of the ssh interrupt bytes. The order is the one `typed_characters_on_terminal` always had;
/// each row is tested alone in `pane_key_tests.rs`.
pub fn route_typed(interrupt_key: bool, probes: &mut impl TypedRouteProbes) -> TypedRoute {
    if probes.ssh_choice_card_up() {
        return TypedRoute::SshChoiceCardHint;
    }
    if interrupt_key {
        if probes.ssh_interrupt_held() {
            return TypedRoute::SshInterruptHeld;
        }
        probes.ssh_interrupt_passes();
    } else if probes.ssh_setup_holds_key() {
        return TypedRoute::SshSetupHeld;
    }
    if probes.enter_queue_takes_key() {
        return TypedRoute::QueuedBehindEnter;
    }
    probes.note_warpification_key();
    if probes.powershell_setup_holds() {
        return TypedRoute::PowerShellHeld;
    }
    let approval_card = probes.on_approval_card();
    let settled_ai_target = !approval_card && probes.on_settled_ai_target();
    let agent_owns_typing = probes.agent_owns_typing();
    let try_pty = !approval_card && !agent_owns_typing && probes.pty_takes_input();
    TypedRoute::Deliver(TypedTail {
        approval_card,
        settled_ai_target,
        agent_owns_typing,
        try_pty,
    })
}

/// What happens to the key after the pty was tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TailAction {
    /// The pty took it.
    Done,
    /// The pty refused it (the alt screen, say) and the input box is hidden: the key is dropped.
    DropHiddenInput,
    /// The key goes into the input editor's buffer.
    InsertIntoEditor,
}

pub fn after_pty_attempt(
    tail: &TypedTail,
    written_to_pty: bool,
    probes: &mut impl TypedRouteProbes,
) -> TailAction {
    if written_to_pty {
        return TailAction::Done;
    }
    // Not when the agent owns the command (#1179): those keys are the user's steer text,
    // buffered in the input (shown once it is visible) instead of discarded.
    let refused_with_hidden_input = !tail.approval_card
        && !tail.agent_owns_typing
        && probes.pty_takes_input()
        && !probes.input_box_visible();
    if refused_with_hidden_input {
        TailAction::DropHiddenInput
    } else {
        TailAction::InsertIntoEditor
    }
}

#[cfg(test)]
#[path = "pane_key_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pane_key_snapshot_tests.rs"]
mod snapshot_tests;
