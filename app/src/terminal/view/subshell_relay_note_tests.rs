//! #2142: the rules and texts of the two relay notes.

use super::*;

#[test]
fn the_switch_is_on_unless_it_says_off() {
    for on in [None, Some(""), Some("1"), Some("on"), Some("yes")] {
        assert!(enabled_from_env(on), "{on:?}");
    }
    for off in ["0", "off", "OFF", " false ", "No"] {
        assert!(!enabled_from_env(Some(off)), "{off:?}");
    }
}

#[test]
fn the_wait_note_is_for_a_click_that_waits_for_a_new_prompt_behind_a_relay() {
    assert!(armed_wait_note_wanted(
        true,
        true,
        Some(WaitWhy::RelayNoPrompt)
    ));
    // Not auto-setup or the rc file: nobody clicked.
    assert!(!armed_wait_note_wanted(
        true,
        false,
        Some(WaitWhy::RelayNoPrompt)
    ));
    // Not with the switch off.
    assert!(!armed_wait_note_wanted(
        false,
        true,
        Some(WaitWhy::RelayNoPrompt)
    ));
    // Not for a wait an Enter on an empty line does not end.
    for other in [
        WaitWhy::Secret,
        WaitWhy::AltScreen,
        WaitWhy::LineDirty,
        WaitWhy::InputPrompt,
        WaitWhy::Hop,
        WaitWhy::Unknown,
        WaitWhy::NotConsumed,
        WaitWhy::NoPrompt,
        WaitWhy::TypedLineRuns,
        WaitWhy::UserLineAtCursor,
        WaitWhy::CookedReadBuiltin,
        WaitWhy::CookedNoPrompt,
        WaitWhy::RelayCooked,
    ] {
        assert!(
            !armed_wait_note_wanted(true, true, Some(other)),
            "{other:?}"
        );
    }
    assert!(!armed_wait_note_wanted(true, true, None));
}

#[test]
fn the_cancel_note_is_for_a_clicked_attempt_behind_a_relay() {
    assert!(cancelled_note_wanted(true, true, true, false));
    assert!(cancelled_note_wanted(true, true, false, true));
    assert!(cancelled_note_wanted(true, true, true, true));
    // A click that never waited behind a relay: the old silence.
    assert!(!cancelled_note_wanted(true, true, false, false));
    // Auto-setup and the rc file: nobody clicked.
    assert!(!cancelled_note_wanted(true, false, true, true));
    // The switch off.
    assert!(!cancelled_note_wanted(false, true, true, true));
}

#[test]
fn the_wait_note_says_what_brings_the_setup_and_what_cancels_it() {
    assert!(ARMED_WAIT_TEXT.contains("next empty prompt"));
    assert!(ARMED_WAIT_TEXT.contains("press Enter on an empty line"));
    assert!(ARMED_WAIT_TEXT.contains("cancels the setup"));
    assert!(
        !ARMED_WAIT_TEXT.contains("  "),
        "one space between the words"
    );
}

#[test]
fn the_cancel_note_names_only_a_way_back_that_exists() {
    let with_key = cancelled_text(true, Some("Ctrl-I"));
    assert!(with_key.starts_with("Phosphor setup cancelled: you ran a command before it started."));
    assert!(with_key.ends_with("click Phosphorize subshell or press Ctrl-I."));
    let no_key = cancelled_text(true, None);
    assert!(no_key.ends_with("click Phosphorize subshell."));
    assert!(!no_key.contains("press"));
    // No button to click: the note does not name one, whatever key is bound.
    for key in [None, Some("Ctrl-I")] {
        let no_button = cancelled_text(false, key);
        assert!(no_button.ends_with("start the subshell again."), "{key:?}");
        assert!(!no_button.contains("click"), "{key:?}");
    }
}

#[test]
fn the_two_notes_have_different_names_in_the_hop_line() {
    assert_ne!(ARMED_WAIT_NOTE, CANCELLED_NOTE);
    for name in [ARMED_WAIT_NOTE, CANCELLED_NOTE] {
        assert!(!name.contains(' '), "a hop field holds no space: {name:?}");
    }
}
