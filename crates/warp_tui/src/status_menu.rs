//! Stateless status projection for the shared read-only menu component.
//!
//! Ported from the pinned Warp oracle (`02b53fcd8`) for issue #389.
//!
//! **Not yet wired up.** In the oracle this lives at
//! `terminal_session_view/status_menu.rs` and is invoked from
//! `TuiTerminalSessionView::compute_status_info` (real session/account data)
//! and dispatched by the `/status` slash command and the `?`-opened menu's
//! session context — both in `terminal_session_view.rs`, which is out of
//! scope for this change (owned by another agent this round). This module
//! ports the presentation logic only; wiring it to real data and to the
//! `/status` command is a follow-up.

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
