//! Stateless status projection for the shared read-only menu component.
//!
//! Ported from the pinned Warp oracle (`02b53fcd8`) for issue #389.
//!
//! **Still not wired up.** This module previously landed at the crate-root
//! path `crate::status_menu` instead of nested here under
//! `terminal_session_view/`; it has been moved (not re-ported) to match the
//! pin's layout, since `TuiTerminalSessionAction::ReadOnlyMenuSelectionStarted`/
//! `ReadOnlyMenuSelectionEnded`, `TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind)`,
//! `TuiTerminalSessionState::read_only_menu()`, and
//! `TuiTerminalSessionView::compute_status_info` all live there. Wiring this
//! module (and `shortcuts.rs`) up — the `?` keybinding, the `/status` slash
//! command, the suggestions-mode variant, and real session/account data for
//! `TuiStatusInfo` (`org`/`email` need a BYOP-appropriate source now that
//! there is no cloud sign-in to read them from) — is still a follow-up.

use crate::read_only_menu::{
    TuiReadOnlyMenu, TuiReadOnlyMenuRow, TuiReadOnlyMenuSection, TuiReadOnlyMenuText,
};
use crate::tui_builder::TuiUiBuilder;

/// Session and account information displayed by the `/status` menu.
pub(crate) struct TuiStatusInfo {
    pub(crate) version: String,
    pub(crate) session: String,
    pub(crate) conversation_id: String,
    pub(crate) working_directory: String,
    pub(crate) org: String,
    pub(crate) email: String,
}

fn field_row(label: &str, value: &str, builder: &TuiUiBuilder) -> TuiReadOnlyMenuRow {
    TuiReadOnlyMenuRow::new([TuiReadOnlyMenuText::new([
        (format!("{label:<19}"), builder.read_only_menu_label_style()),
        (value.to_owned(), builder.primary_text_style()),
    ])])
}

/// Builds the dedicated status menu opened by `/status`.
pub(crate) fn menu(status_info: TuiStatusInfo, builder: &TuiUiBuilder) -> TuiReadOnlyMenu {
    let rows = [
        ("Version", status_info.version.as_str()),
        ("Session", status_info.session.as_str()),
        ("Conversation ID", status_info.conversation_id.as_str()),
        ("Working directory", status_info.working_directory.as_str()),
        ("Org", status_info.org.as_str()),
        ("Email", status_info.email.as_str()),
    ]
    .into_iter()
    .map(|(label, value)| field_row(label, value, builder))
    .collect();
    TuiReadOnlyMenu::new(vec![TuiReadOnlyMenuSection::new("Status", rows)])
}
