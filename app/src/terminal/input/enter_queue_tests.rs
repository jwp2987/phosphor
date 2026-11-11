use super::enter_gate_holds_lines_typed_behind;

/// #1999 (owner decision 2026-11-11): which open Enter gates take the lines typed behind a
/// deferred Enter that wrote no command. Every row of the rule, for both values of the kill
/// switch `PHOSPHOR_BARE_ENTER_HOLDS_LINES` (read once per process, so it is an argument here).
#[test]
fn a_bare_enter_held_for_an_unready_shell_holds_the_lines_typed_behind_it() {
    for switch_on in [true, false] {
        // #1997, unchanged by the switch: a line that waits is never overtaken.
        assert!(
            enter_gate_holds_lines_typed_behind(true, false, switch_on),
            "an unready shell's held line, switch_on={switch_on}"
        );
        assert!(
            enter_gate_holds_lines_typed_behind(false, false, switch_on),
            "a line that waits for its verdict, switch_on={switch_on}"
        );
        // A verdict / completion / route wait with an empty buffer already submitted its line:
        // it is not a bare Enter and never takes the keys.
        assert!(
            !enter_gate_holds_lines_typed_behind(false, true, switch_on),
            "a submitted Enter's gate, switch_on={switch_on}"
        );
    }
    // #1999: the bare Enter held for an unready shell.
    assert!(
        enter_gate_holds_lines_typed_behind(true, true, true),
        "the lines wait behind the bare Enter: typed order kept"
    );
    assert!(
        !enter_gate_holds_lines_typed_behind(true, true, false),
        "switch off: the lines are written at once, as before the decision"
    );
}
