//! Where an entry goes, decided at drain time (design 2.3 rule 5).
//!
//! This is the pure form of the predicates `TerminalView::typed_characters_on_terminal` evaluates
//! today (approval card, agent-owned command pty, `should_write_typed_chars_to_pty`, hidden input
//! box, focus). Phase 0 does not call it from the view; later phases replace the inline checks
//! with one call to this function.

use super::{EntryKind, is_editor_paste_id};

/// Why an entry was dropped instead of delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropReason {
    /// An ssh dialog is up or a password prompt appeared (#1113): typing is not for the shell.
    SshDialogOrPassword,
    /// Pty-only bytes (mouse reports, wheel arrows) while the agent controls the command (#963).
    AgentControl,
    /// The pty refuses the bytes and the input box is hidden (the old behaviour: drop them).
    RefusedHiddenInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    /// The input editor.
    Editor,
    /// The pty.
    Pty,
    /// The agent's input (steer text, approval-card follow-up).
    AgentInput,
    Drop(DropReason),
}

/// Plain-data facts about the world at drain time. The host fills it; nothing here locks the
/// terminal model (design 3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DestFacts {
    pub drop_reason: Option<DropReason>,
    /// The pending approval card (or an AI block awaiting confirmation) holds focus.
    pub approval_card: bool,
    /// The agent owns the running command's pty.
    pub agent_owns_pty: bool,
    /// `should_write_typed_chars_to_pty`: the pty takes user bytes right now.
    pub pty_takes_input: bool,
    pub input_box_visible: bool,
    /// The editor holds focus (warpui routes keys straight to it).
    pub editor_focused: bool,
    /// A pty exists at all (a closed pty makes a pinned `Pty` destination unavailable).
    pub pty_open: bool,
}

pub fn destination_for(kind: &EntryKind, facts: &DestFacts) -> Destination {
    // Signals go to the command; the gate rules decide what happens around them.
    if matches!(kind, EntryKind::Signal(_)) {
        return Destination::Pty;
    }
    // Phase 1: the terminal's keys already carry their form, and editor keys only ever go to the
    // editor. The later stages (agent, hidden input box) still decide inside the legacy chain.
    match kind {
        EntryKind::PtyBytes { .. } => return Destination::Pty,
        EntryKind::EditorKey(_) => return Destination::Editor,
        // The host routes held input itself at delivery (phase 4).
        EntryKind::Held { .. } => return Destination::Pty,
        // A paste read for the input editor (its id says so) goes to the editor whichever host
        // drains the shared pane queue.
        EntryKind::Paste { paste_id, .. } if is_editor_paste_id(*paste_id) => {
            return Destination::Editor;
        }
        _ => {}
    }
    if let Some(reason) = facts.drop_reason {
        return Destination::Drop(reason);
    }
    if matches!(kind, EntryKind::Raw(_)) {
        return if facts.agent_owns_pty {
            Destination::Drop(DropReason::AgentControl)
        } else if facts.pty_takes_input {
            Destination::Pty
        } else {
            Destination::Drop(DropReason::RefusedHiddenInput)
        };
    }
    if facts.approval_card || facts.agent_owns_pty {
        return Destination::AgentInput;
    }
    if facts.editor_focused {
        return Destination::Editor;
    }
    if facts.pty_takes_input {
        return Destination::Pty;
    }
    if !facts.input_box_visible {
        return Destination::Drop(DropReason::RefusedHiddenInput);
    }
    Destination::Editor
}

/// Whether a line-run pin to `dest` still holds (design 2.3 rule 6): the pinned destination must
/// still exist, and an approval card always wins (its Enter safety must not be bypassed).
pub fn pin_still_valid(dest: Destination, facts: &DestFacts) -> bool {
    if facts.approval_card {
        return false;
    }
    match dest {
        Destination::Editor => facts.input_box_visible || facts.editor_focused,
        Destination::Pty => facts.pty_open && !facts.agent_owns_pty,
        Destination::AgentInput => true,
        Destination::Drop(_) => false,
    }
}

/// Entries only the terminal view can deliver (they carry pty bytes, or a paste read for the pty).
/// On a queue shared by the editor and the view, a drain driven from the editor side stops at the
/// first of these ([`super::Host::handles`]) and the view continues it. Everything else (editor
/// keys, editor pastes, text typed behind an Enter, signals) can be delivered from either side.
pub fn is_pty_entry(kind: &EntryKind) -> bool {
    match kind {
        EntryKind::PtyBytes { .. } | EntryKind::Held { .. } | EntryKind::Raw(_) => true,
        EntryKind::Paste { paste_id, .. } => !is_editor_paste_id(*paste_id),
        _ => false,
    }
}

/// The destination an entry has by its nature, whatever the line-run pin says: pty bytes go to
/// the pty and editor keys to the editor even when the line they follow went the other way (a
/// shared queue carries both).
pub fn intrinsic_destination(kind: &EntryKind) -> Option<Destination> {
    match kind {
        EntryKind::PtyBytes { .. } | EntryKind::Held { .. } => Some(Destination::Pty),
        EntryKind::EditorKey(_) => Some(Destination::Editor),
        EntryKind::Paste { paste_id, .. } if is_editor_paste_id(*paste_id) => {
            Some(Destination::Editor)
        }
        _ => None,
    }
}
