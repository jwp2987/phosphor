use super::{
    closed_dialog_hands_focus_to_tab, enter_button_index, focus_back_enabled_for,
    initial_selection, last_button_index, moved_selection, should_run_disable_callback,
};

/// #2108: Cancel on a close question does nothing but close the dialog, so the focus was left on
/// the dialog's view, which is no longer drawn: no key reached the pane until a click. The
/// workspace takes the focus back for its active tab, but only while the dialog's view still has
/// it: a handler that put the focus somewhere itself keeps its choice.
#[test]
fn a_closed_dialog_hands_the_focus_back_only_while_it_still_has_it() {
    assert!(closed_dialog_hands_focus_to_tab(true, true));
    assert!(!closed_dialog_hands_focus_to_tab(true, false));
}

/// The kill switch `PHOSPHOR_MODAL_FOCUS_BACK=0` puts back the old behaviour; anything else
/// (unset, empty, another value) leaves the fix on.
#[test]
fn the_focus_back_kill_switch_is_off_only_for_zero() {
    assert!(focus_back_enabled_for(None));
    assert!(focus_back_enabled_for(Some("")));
    assert!(focus_back_enabled_for(Some("1")));
    assert!(!focus_back_enabled_for(Some("0")));
    assert!(!closed_dialog_hands_focus_to_tab(false, true));
    assert!(!closed_dialog_hands_focus_to_tab(false, false));
}

/// Escape presses the last button; with no dialog up there is none (this used to be
/// `len() - 1` on an empty list).
#[test]
fn escape_presses_the_last_button_and_nothing_without_a_dialog() {
    assert_eq!(last_button_index(3), Some(2));
    assert_eq!(last_button_index(1), Some(0));
    assert_eq!(last_button_index(0), None);
}

/// #2092: the "don't show again" callback toggles `general.show_warning_before_quitting`. A
/// dialog that has to show whatever that setting says (unsaved editor edits) must never run it:
/// the checkbox state starts ticked when the setting is off, so running it would switch the
/// setting back on behind the user's back.
#[test]
fn a_dialog_without_the_checkbox_never_runs_the_disable_callback() {
    assert!(!should_run_disable_callback(false, true));
    assert!(!should_run_disable_callback(false, false));
}

#[test]
fn a_dialog_with_the_checkbox_runs_it_only_when_ticked() {
    assert!(should_run_disable_callback(true, true));
    assert!(!should_run_disable_callback(true, false));
}

/// Every dialog that does not ask for a keyboard selection is as before: Enter is the first
/// button and Tab moves nothing.
#[test]
fn a_dialog_without_a_selection_keeps_enter_on_the_first_button() {
    let selected = initial_selection(None, 3);
    assert_eq!(selected, None);
    assert_eq!(enter_button_index(selected), 0);
    assert_eq!(moved_selection(selected, 3, true), None);
    assert_eq!(moved_selection(selected, 3, false), None);
    assert_eq!(enter_button_index(moved_selection(selected, 3, true)), 0);
}

/// Owner decision 2026-11-11 (the editor's overwrite question, #2092): a dialog can put Enter
/// on another button than the first. Enter presses that button until Tab moves the selection.
#[test]
fn a_dialog_can_put_enter_on_another_button() {
    let selected = initial_selection(Some(2), 3);
    assert_eq!(selected, Some(2));
    assert_eq!(enter_button_index(selected), 2);
}

/// Tab and Shift-Tab walk the buttons and wrap, so every button can be pressed by keyboard.
#[test]
fn tab_and_shift_tab_move_the_selection_and_wrap() {
    assert_eq!(moved_selection(Some(2), 3, true), Some(0));
    assert_eq!(moved_selection(Some(0), 3, true), Some(1));
    assert_eq!(moved_selection(Some(1), 3, true), Some(2));
    assert_eq!(moved_selection(Some(2), 3, false), Some(1));
    assert_eq!(moved_selection(Some(0), 3, false), Some(2));
    assert_eq!(moved_selection(Some(0), 1, true), Some(0));
    assert_eq!(moved_selection(Some(0), 1, false), Some(0));
}

/// An index past the end is the last button (the one Escape presses), never the first; a dialog
/// with no buttons has no selection.
#[test]
fn a_selection_past_the_end_is_the_last_button() {
    assert_eq!(initial_selection(Some(7), 3), Some(2));
    assert_eq!(initial_selection(Some(0), 0), None);
    assert_eq!(moved_selection(Some(7), 3, true), Some(0));
    assert_eq!(moved_selection(Some(0), 0, true), None);
}
