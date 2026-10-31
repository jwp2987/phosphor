use super::reuse_unless_disabled;

#[test]
fn instance_buffers_are_reused_unless_the_per_frame_flag_is_set() {
    assert!(reuse_unless_disabled(None));
    assert!(reuse_unless_disabled(Some("0")));
    assert!(!reuse_unless_disabled(Some("1")));
    assert!(!reuse_unless_disabled(Some("true")));
}
