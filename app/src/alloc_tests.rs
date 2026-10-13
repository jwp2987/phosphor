use super::knob_set_by_user;

const ARENA_MAX: &str = "glibc.malloc.arena_max";

#[test]
fn knob_is_unset_without_any_user_configuration() {
    assert!(!knob_set_by_user(false, None, ARENA_MAX));
    assert!(!knob_set_by_user(false, Some(""), ARENA_MAX));
}

#[test]
fn legacy_environment_variable_counts_as_user_configuration() {
    assert!(knob_set_by_user(true, None, ARENA_MAX));
}

#[test]
fn glibc_tunables_entry_counts_as_user_configuration() {
    assert!(knob_set_by_user(
        false,
        Some("glibc.malloc.arena_max=4"),
        ARENA_MAX
    ));
    assert!(knob_set_by_user(
        false,
        Some("glibc.malloc.check=3:glibc.malloc.arena_max=1:glibc.malloc.tcache_count=0"),
        ARENA_MAX
    ));
}

#[test]
fn other_tunables_do_not_count_as_configuring_this_knob() {
    assert!(!knob_set_by_user(
        false,
        Some("glibc.malloc.trim_threshold=131072:glibc.malloc.arena_test=2"),
        ARENA_MAX
    ));
    assert!(!knob_set_by_user(
        false,
        Some("glibc.malloc.arena_max"),
        ARENA_MAX
    ));
}
