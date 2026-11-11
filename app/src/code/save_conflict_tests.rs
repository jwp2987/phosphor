use super::{OverwriteAnswer, save_ask_trace, save_conflict_ask_enabled_for, save_must_ask};

/// #2092 part 2, the owner's decision of 2026-11-11: a save while the disk-conflict banner shows
/// asks before it overwrites the file changed on disk, never silently.
#[test]
fn a_save_on_the_conflict_banner_asks() {
    assert!(save_must_ask(true, true, true));
}

/// No banner, no question: an ordinary save is not slowed down.
#[test]
fn a_save_without_the_banner_does_not_ask() {
    assert!(!save_must_ask(false, true, true));
    assert!(!save_must_ask(false, false, true));
    assert!(!save_must_ask(false, true, false));
}

/// `PHOSPHOR_EDITOR_SAVE_CONFLICT_ASK=0` is the behaviour before the decision.
#[test]
fn the_kill_switch_puts_back_the_silent_overwrite() {
    assert!(!save_must_ask(true, false, true));

    assert!(save_conflict_ask_enabled_for(None));
    assert!(save_conflict_ask_enabled_for(Some("1")));
    assert!(save_conflict_ask_enabled_for(Some("")));
    assert!(!save_conflict_ask_enabled_for(Some("0")));
}

/// Where nobody can press a button (the integration-test channel) the question is not put up,
/// like the close and quit questions.
#[test]
fn no_question_where_nobody_can_answer() {
    assert!(!save_must_ask(true, true, false));
}

/// The button order is part of the behaviour: the in-app dialog gives Enter to the first button
/// and Escape to the last. Escape must be the answer that loses nothing.
#[test]
fn the_buttons_are_overwrite_reload_cancel_and_escape_loses_nothing() {
    assert_eq!(
        OverwriteAnswer::BUTTONS,
        [
            OverwriteAnswer::Overwrite,
            OverwriteAnswer::Reload,
            OverwriteAnswer::Cancel,
        ]
    );
    assert_eq!(
        OverwriteAnswer::BUTTONS.last(),
        Some(&OverwriteAnswer::Cancel)
    );
}

/// Only Overwrite writes. Reload and Cancel end the save the user asked for without a write, so
/// a caller waiting on it ("Save" on a tab's close question) keeps the tab.
#[test]
fn only_overwrite_writes() {
    assert!(!OverwriteAnswer::Overwrite.ends_the_save_without_writing());
    assert!(OverwriteAnswer::Reload.ends_the_save_without_writing());
    assert!(OverwriteAnswer::Cancel.ends_the_save_without_writing());
}

/// The app case `editor_daily` reads these lines; the format is pinned here
/// (script/realapp/appci/cases_r15_w5a.py SAVE_ASK_TRACE).
#[test]
fn the_trace_line_format() {
    assert_eq!(save_ask_trace("asked"), "editor_save_ask outcome=asked");
    assert_eq!(
        save_ask_trace(OverwriteAnswer::Overwrite.trace_name()),
        "editor_save_ask outcome=overwrite"
    );
    assert_eq!(
        save_ask_trace(OverwriteAnswer::Reload.trace_name()),
        "editor_save_ask outcome=reload"
    );
    assert_eq!(
        save_ask_trace(OverwriteAnswer::Cancel.trace_name()),
        "editor_save_ask outcome=cancel"
    );
}
