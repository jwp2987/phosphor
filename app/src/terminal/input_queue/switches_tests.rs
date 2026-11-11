use super::*;

/// A kill switch turns its fix off only for the four "off" words; unset, empty and anything else
/// leave it on, so a typo never disables a fix.
#[test]
fn a_kill_switch_is_off_only_for_the_off_words() {
    for off in ["0", "off", "OFF", "false", "False", "no", " 0 ", "\tno\n"] {
        assert!(!switch_is_on(Some(off)), "{off:?}");
    }
    for on in [
        None,
        Some(""),
        Some("1"),
        Some("on"),
        Some("yes"),
        Some("00"),
    ] {
        assert!(switch_is_on(on), "{on:?}");
    }
}

/// The names are part of the interface (docs/testing.md, the design note): a rename is a change
/// the docs must follow.
#[test]
fn the_switch_names_are_stable() {
    assert_eq!(KEEP_SYNCED_ORIGIN_ENV, "PHOSPHOR_QUEUE_KEEP_SYNCED_ORIGIN");
    assert_eq!(
        SSH_DROP_ACCOUNTS_ENTRIES_ENV,
        "PHOSPHOR_SSH_DROP_ACCOUNTS_ENTRIES"
    );
    assert_eq!(
        YOUNG_HOLD_READER_LOOK_ENV,
        "PHOSPHOR_YOUNG_HOLD_READER_LOOK"
    );
    assert_eq!(
        BARE_ENTER_HOLDS_LINES_ENV,
        "PHOSPHOR_BARE_ENTER_HOLDS_LINES"
    );
    assert_eq!(
        KEEP_DEFERRED_ENTER_KEYS_WITH_LINE_ENV,
        "PHOSPHOR_KEEP_DEFERRED_ENTER_KEYS_WITH_LINE"
    );
    assert_eq!(
        SUBSHELL_LINE_EDITOR_PROOF_ENV,
        "PHOSPHOR_SUBSHELL_LINE_EDITOR_PROOF"
    );
}
