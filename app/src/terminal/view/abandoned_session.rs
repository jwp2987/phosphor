//! The "Session abandoned before bootstrap" log line (#1983, defect 2 of #1798).
//!
//! The line is written when a [`super::TerminalView`] is DROPPED while its model never finished
//! bootstrapping a shell. It describes the dropped view only, never the session the user is
//! typing in. It carried no view, so two such lines during a running ssh command were read as
//! that session's own.
//!
//! Some views never bootstrap by design: a conversation transcript viewer, a read-only viewer of
//! a live conversation and a "loading conversation" pane sit on a `MockTerminalManager`, whose
//! block list no shell ever moves on. Dropping one of them is not an abandoned session and logs
//! nothing.

/// What the dropped view's model says, read once in `Drop`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DroppedViewFacts {
    /// The block list reached `Bootstrapped`.
    pub is_bootstrapped: bool,
    /// The ssh wrapper announced a session whose `InitShell` never came.
    pub has_pending_ssh_session: bool,
    /// The view shows a conversation transcript (or its loading state): it never had a shell.
    pub is_transcript_viewer: bool,
    /// The view was on screen at some point.
    pub was_ever_visible: bool,
}

/// The level to log the line at, or `None` when the dropped view was not an abandoned session.
///
/// A session that was visible is an error; one that never was (a bootstrap that stalled in a tab
/// nobody looked at, such as an rc file asking a question) is a warning.
pub(super) fn abandoned_before_bootstrap_level(facts: DroppedViewFacts) -> Option<log::Level> {
    if facts.is_transcript_viewer {
        return None;
    }
    if !facts.has_pending_ssh_session && facts.is_bootstrapped {
        return None;
    }
    Some(if facts.was_ever_visible {
        log::Level::Error
    } else {
        log::Level::Warn
    })
}

#[cfg(test)]
#[path = "abandoned_session_tests.rs"]
mod tests;
