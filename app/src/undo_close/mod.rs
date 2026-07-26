pub mod settings;
mod stack;

use warpui::{keymap::EditableBinding, AppContext};

use crate::{util::bindings::CustomAction, workspace::WorkspaceAction};

pub use self::{settings::UndoCloseSettings, stack::UndoCloseStack, stack::UndoCloseStackEvent};

/// Register keybindings for undo close functionality.
pub fn init(ctx: &mut AppContext) {
    use warpui::keymap::macros::*;

    ctx.register_editable_bindings([EditableBinding::new(
        "app:reopen_closed_session",
        crate::t!("keybinding-desc-undo-close-reopen-session"),
        // Trigger ReopenClosedSession on the active workspace when
        // the action is taken from the command palette.
        WorkspaceAction::ReopenClosedSession,
    )
    .with_custom_action(CustomAction::ReopenClosedSession)
    // Scope to the GUI `Workspace` context so this binding doesn't leak into the
    // headless TUI's keymap contexts (mirrors the sibling `workspace:*` bindings).
    .with_context_predicate(id!("Workspace"))]);
}
