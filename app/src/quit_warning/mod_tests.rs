use super::{
    should_display_warning_for, unsaved_edits_always_ask_for, warning_can_be_switched_off,
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
