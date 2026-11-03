//! Unit tests for the inherited ignore-Ctrl-C reset (#1570).

use super::reset_enabled;

#[test]
fn reset_is_on_by_default() {
    assert!(reset_enabled(None));
    assert!(reset_enabled(Some("")));
    assert!(reset_enabled(Some("1")));
    assert!(reset_enabled(Some("on")));
}

#[test]
fn kill_switch_keeps_the_inherited_flag() {
    for value in ["0", "off", "false", "no", " OFF ", "False"] {
        assert!(
            !reset_enabled(Some(value)),
            "{value:?} must turn the reset off"
        );
    }
}
