//! A save while the editor's disk-conflict banner shows asks first (#2092, part 2; the banner
//! state is #2087's).
//!
//! The banner means the file on disk is no longer what this buffer started from: another program
//! changed it under unsaved edits, or another view saved it. Before the owner's decision of
//! 2026-11-11 a save in that state (Ctrl-S, "Save" on a close question, a save from code review)
//! wrote the buffer over the other change without a word. Now it asks: Overwrite, Reload or
//! Cancel. The banner's own "Overwrite" button is the explicit answer and does not ask again.
//!
//! This file is the part with no view in it, so it can be unit tested: the rule, the kill switch,
//! the button order and the trace line the app tests read.

/// What the user answered to "the file changed on disk; overwrite it?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OverwriteAnswer {
    /// Write the buffer over what the disk holds now.
    Overwrite,
    /// Drop this buffer's unsaved edits and load what the disk holds now.
    Reload,
    /// Neither: nothing is written, nothing is dropped, the banner stays.
    Cancel,
}

impl OverwriteAnswer {
    /// The dialog's buttons, in order. The in-app dialog (`workspace::native_modal`) gives Enter
    /// to the first button and Escape to the last, so Escape is always the answer that loses
    /// nothing.
    pub(super) const BUTTONS: [OverwriteAnswer; 3] = [
        OverwriteAnswer::Overwrite,
        OverwriteAnswer::Reload,
        OverwriteAnswer::Cancel,
    ];

    /// The word in the trace line.
    pub(super) fn trace_name(self) -> &'static str {
        match self {
            OverwriteAnswer::Overwrite => "overwrite",
            OverwriteAnswer::Reload => "reload",
            OverwriteAnswer::Cancel => "cancel",
        }
    }

    /// Whether the save the user asked for is over without a write (a caller waiting on that
    /// save, like "Save" on a tab's close question, is told it was cancelled and keeps the tab).
    pub(super) fn ends_the_save_without_writing(self) -> bool {
        !matches!(self, OverwriteAnswer::Overwrite)
    }
}

/// Env kill switch: `PHOSPHOR_EDITOR_SAVE_CONFLICT_ASK=0` puts back the behaviour before the
/// decision: a save while the conflict banner shows overwrites the file without asking. Read once
/// per run.
pub(super) fn save_conflict_ask_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        save_conflict_ask_enabled_for(
            std::env::var("PHOSPHOR_EDITOR_SAVE_CONFLICT_ASK")
                .ok()
                .as_deref(),
        )
    })
}

/// The kill switch's rule: on unless the variable is exactly `0`.
pub(super) fn save_conflict_ask_enabled_for(value: Option<&str>) -> bool {
    value != Some("0")
}

/// Whether a save has to ask before it writes.
///
/// - `banner_shows`: the editor's conflict banner is up (`has_version_conflicts`).
/// - `enabled`: the kill switch.
/// - `can_answer`: somebody can press a button. `false` on the integration-test channel, where
///   the other questions (close, quit) are skipped for the same reason.
pub(super) fn save_must_ask(banner_shows: bool, enabled: bool, can_answer: bool) -> bool {
    banner_shows && enabled && can_answer
}

/// The line the app tests read (`editor_daily`), target `editor_disk`, level debug:
/// `editor_save_ask outcome=<asked|overwrite|reload|cancel|not_shown|banner_overwrite>`.
///
/// - `asked`: the question is on screen; nothing was written.
/// - `overwrite` / `reload` / `cancel`: the answer.
/// - `not_shown`: the question could not be put on screen (no window to show it in, or another
///   dialog is up); the save was NOT done.
/// - `banner_overwrite`: the banner's own "Overwrite" button, which is an answer already.
pub(super) fn save_ask_trace(outcome: &str) -> String {
    format!("editor_save_ask outcome={outcome}")
}

#[cfg(test)]
#[path = "save_conflict_tests.rs"]
mod tests;
