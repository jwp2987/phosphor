//! #1960 item 2 (owner 2026-11-12): a dim one-line notice in the pane when a managed ssh session
//! runs WITHOUT the remote helper. It is a view beside the blocks, never terminal text and never
//! user input: nothing is written to the pty and it never takes focus or blocks keys. Shown once
//! per session; the string comes from the localisation files.

use std::collections::HashSet;

use warp_core::ui::theme::color::internal_colors;
use warpui::{
    AppContext, Element, Entity, SingletonEntity, View, ViewContext,
    elements::{Container, Text},
};

use super::TerminalView;
use super::rich_content::RichContentInsertionPosition;
use crate::{Appearance, terminal::model::session::SessionId};

/// The localised notice line.
pub(crate) fn notice_text() -> String {
    crate::t!("terminal-ssh-no-helper-notice")
}

/// Whether the notice is due now: the first request for `session_id` takes the slot, any later
/// one does not. The set is the view's record of sessions that already showed it.
pub(crate) fn take_notice_slot(shown: &mut HashSet<SessionId>, session_id: SessionId) -> bool {
    shown.insert(session_id)
}

pub struct SshNoHelperNotice;

impl Entity for SshNoHelperNotice {
    type Event = ();
}

impl View for SshNoHelperNotice {
    fn ui_name() -> &'static str {
        "SshNoHelperNotice"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        Container::new(
            Text::new(
                notice_text(),
                appearance.ui_font_family(),
                appearance.monospace_font_size() - 2.,
            )
            .with_color(internal_colors::neutral_5(theme))
            .finish(),
        )
        .with_horizontal_padding(12.)
        .with_vertical_padding(2.)
        .finish()
    }
}

impl TerminalView {
    /// Shows the notice for `session_id` once per session.
    pub(super) fn show_ssh_no_helper_notice(
        &mut self,
        session_id: SessionId,
        ctx: &mut ViewContext<Self>,
    ) {
        if !take_notice_slot(&mut self.ssh_no_helper_notice_sessions, session_id) {
            return;
        }
        let notice = ctx.add_view(|_| SshNoHelperNotice);
        self.insert_rich_content(
            None,
            notice,
            None,
            RichContentInsertionPosition::Append {
                insert_below_long_running_block: true,
            },
            ctx,
        );
    }
}

#[cfg(test)]
#[path = "ssh_no_helper_notice_tests.rs"]
mod tests;
