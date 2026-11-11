use super::{
    close_question_enter_button, close_question_keys_enabled_for, should_display_warning_for,
    unsaved_edits_always_ask_for, warning_can_be_switched_off,
};

/// #2092, the owner's decision of 2026-11-11: losing unsaved editor edits always asks, whatever
/// `general.show_warning_before_quitting` says. Every close path (quit, close window, close tab,
/// close pane, close editor tab) takes its answer from this one rule.
#[test]
fn unsaved_editor_edits_ask_with_the_quit_warning_switched_off() {
    assert!(should_display_warning_for(false, true, 0, 0, true));
    // ... also next to running commands and shared sessions.
    assert!(should_display_warning_for(false, true, 2, 1, true));
    // ... and with the setting on, as before.
    assert!(should_display_warning_for(true, true, 0, 0, true));
}

/// The setting keeps covering what is still running: with it off, running commands and shared
/// sessions alone do not ask.
#[test]
fn the_setting_still_covers_running_commands_and_shared_sessions() {
    assert!(!should_display_warning_for(false, true, 3, 0, false));
    assert!(!should_display_warning_for(false, true, 0, 2, false));
    assert!(!should_display_warning_for(false, true, 3, 2, false));
    assert!(should_display_warning_for(true, true, 1, 0, false));
    assert!(should_display_warning_for(true, true, 0, 1, false));
}

#[test]
fn nothing_to_lose_never_asks() {
    for setting_on in [false, true] {
        for always_ask in [false, true] {
            assert!(!should_display_warning_for(
                setting_on, always_ask, 0, 0, false
            ));
        }
    }
}

/// `PHOSPHOR_UNSAVED_EDITS_ALWAYS_ASK=0` is the behaviour before the decision: the one setting
/// switches off every question, unsaved edits included.
#[test]
fn the_kill_switch_puts_back_one_setting_for_every_question() {
    assert!(!should_display_warning_for(false, false, 0, 0, true));
    assert!(!should_display_warning_for(false, false, 2, 1, true));
    assert!(should_display_warning_for(true, false, 0, 0, true));
    assert!(should_display_warning_for(true, false, 1, 0, false));

    assert!(unsaved_edits_always_ask_for(None));
    assert!(unsaved_edits_always_ask_for(Some("1")));
    assert!(unsaved_edits_always_ask_for(Some("")));
    assert!(!unsaved_edits_always_ask_for(Some("0")));
}

/// "Don't show again" switches off `general.show_warning_before_quitting`. The question about
/// unsaved edits comes back whatever that setting says, so its dialog does not offer the
/// checkbox; the running-commands dialog still does.
#[test]
fn the_unsaved_edits_question_offers_no_dont_show_again() {
    assert!(!warning_can_be_switched_off(true, true));
    assert!(warning_can_be_switched_off(true, false));
    // Kill switch: the checkbox is back, and it does switch the question off.
    assert!(warning_can_be_switched_off(false, true));
    assert!(warning_can_be_switched_off(false, false));
}

/// #2132, the owner's decision of 2026-11-11: the editor tab's close question (Save / Don't Save
/// / Cancel) has a keyboard selection, so Tab reaches "Don't Save". It starts on Save, the first
/// button: Enter alone is what it was. Fails on old code: the function did not exist and the
/// question was built without a selection, so Tab did nothing in it.
#[test]
fn the_editor_tab_close_question_starts_with_save_selected() {
    assert_eq!(close_question_enter_button(true, Some(0), true), Some(0));
}

/// Every other question (pane, tab, window, quit) keeps Enter on its first button and has no
/// keyboard selection; so does an editor tab question that has no Save button.
#[test]
fn other_close_questions_have_no_keyboard_selection() {
    assert_eq!(close_question_enter_button(false, None, true), None);
    assert_eq!(close_question_enter_button(false, Some(1), true), None);
    assert_eq!(close_question_enter_button(true, None, true), None);
}

/// The kill switch `PHOSPHOR_CLOSE_QUESTION_KEYS=0` puts back the question without a keyboard
/// selection; anything else (unset, empty, another value) leaves the decision on.
#[test]
fn the_close_question_keys_kill_switch_is_off_only_for_zero() {
    assert!(close_question_keys_enabled_for(None));
    assert!(close_question_keys_enabled_for(Some("")));
    assert!(close_question_keys_enabled_for(Some("1")));
    assert!(!close_question_keys_enabled_for(Some("0")));
    assert_eq!(close_question_enter_button(true, Some(0), false), None);
}
