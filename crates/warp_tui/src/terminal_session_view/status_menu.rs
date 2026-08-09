//! Stateless status projection for the shared read-only menu component.
//!
//! Ported from the pinned Warp oracle (`02b53fcd8`) for issue #389, and now
//! wired up: dispatched by the `/status` slash command and rendered from
//! `TuiTerminalSessionView::compute_status_info` alongside `shortcuts.rs`'s
//! `?`-opened menu, both routed through
//! `TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind)`.
//!
//! Drops the pin's `org`/`email` fields: those are Warp cloud sign-in
//! artifacts (organization and account email), and this fork is BYOP with no
//! account, organization, or sign-in of any kind, so there is nothing
//! truthful to put there.

use crate::read_only_menu::{
    TuiReadOnlyMenu, TuiReadOnlyMenuRow, TuiReadOnlyMenuSection, TuiReadOnlyMenuText,
};
use crate::tui_builder::TuiUiBuilder;

/// Session information displayed by the `/status` menu.
///
/// The pin also carries `org` and `email` fields, sourced from Warp's cloud
/// account/sign-in state. This fork is BYOP with no account, organization,
/// or sign-in email of any kind, so there is nothing truthful to put there;
/// those two cloud-account fields are dropped rather than backed by an
/// invented substitute or left as empty rows.
pub(crate) struct TuiStatusInfo {
    pub(crate) version: String,
    pub(crate) session: String,
    pub(crate) conversation_id: String,
    pub(crate) working_directory: String,
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
    ]
    .into_iter()
    .map(|(label, value)| field_row(label, value, builder))
    .collect();
    TuiReadOnlyMenu::new(vec![TuiReadOnlyMenuSection::new("Status", rows)])
}
