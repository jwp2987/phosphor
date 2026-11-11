use super::{
    OverwriteAnswer, overwrite_enter_cancels_enabled_for, save_ask_trace,
    save_conflict_ask_enabled_for, save_must_ask,
};

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

/// The button order is part of the behaviour: the in-app dialog gives Escape to the last button.
/// Escape must be the answer that loses nothing. (Until the owner's decision of 2026-11-11 Enter
/// was the first button, Overwrite; Enter is now pinned by the tests below.)
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

/// Owner decision 2026-11-11: Enter on the overwrite question is Cancel, never Overwrite. The
/// question comes up with Cancel selected, and Cancel writes nothing and drops nothing.
#[test]
fn enter_is_cancel_never_overwrite() {
    let enter = OverwriteAnswer::enter_button(true).expect("the question has a selection");
    assert_eq!(OverwriteAnswer::BUTTONS[enter], OverwriteAnswer::Cancel);
    assert_ne!(OverwriteAnswer::BUTTONS[enter], OverwriteAnswer::Overwrite);
    assert!(OverwriteAnswer::BUTTONS[enter].ends_the_save_without_writing());
    // Enter and Escape are the same answer: Escape is the dialog's last button.
    assert_eq!(enter, OverwriteAnswer::BUTTONS.len() - 1);
}

/// Overwrite stays reachable by keyboard: the dialog's Tab moves the selection one button on and
/// wraps (`workspace/native_modal_tests.rs` `tab_and_shift_tab_move_the_selection_and_wrap`), so
/// from Cancel one Tab is Overwrite and one Shift-Tab is Reload. The app case `editor_daily`
/// presses Tab then Enter to overwrite.
#[test]
fn one_tab_from_the_enter_button_is_overwrite() {
    let count = OverwriteAnswer::BUTTONS.len();
    let enter = OverwriteAnswer::enter_button(true).expect("the question has a selection");
    assert_eq!(
        OverwriteAnswer::BUTTONS[(enter + 1) % count],
        OverwriteAnswer::Overwrite
    );
    assert_eq!(
        OverwriteAnswer::BUTTONS[(enter + count - 1) % count],
        OverwriteAnswer::Reload
    );
}

/// `PHOSPHOR_EDITOR_OVERWRITE_ENTER_CANCELS=0` is the behaviour before the decision: no keyboard
/// selection, so the dialog gives Enter to its first button, Overwrite.
#[test]
fn the_kill_switch_puts_enter_back_on_overwrite() {
    assert_eq!(OverwriteAnswer::enter_button(false), None);
    assert_eq!(OverwriteAnswer::BUTTONS[0], OverwriteAnswer::Overwrite);

    assert!(overwrite_enter_cancels_enabled_for(None));
    assert!(overwrite_enter_cancels_enabled_for(Some("1")));
    assert!(overwrite_enter_cancels_enabled_for(Some("")));
    assert!(!overwrite_enter_cancels_enabled_for(Some("0")));
}
