use super::{display_changed, enabled_from_env};

#[test]
fn only_zero_is_the_kill_switch() {
    assert!(enabled_from_env(None));
    assert!(enabled_from_env(Some("")));
    assert!(enabled_from_env(Some("1")));
    assert!(enabled_from_env(Some("off")));
    assert!(!enabled_from_env(Some("0")));
    assert!(!enabled_from_env(Some(" 0 ")));
}

#[test]
fn an_unchanged_display_is_not_notified_and_a_changed_one_is() {
    assert!(!display_changed(&1, &1, true));
    assert!(display_changed(&1, &2, true));
}

#[test]
fn with_the_kill_switch_every_change_notifies() {
    assert!(display_changed(&1, &1, false));
    assert!(display_changed(&1, &2, false));
}
