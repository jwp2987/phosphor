//! Ported from Warp's `app/src/server/cloud_objects/update_manager_tests.rs`.
//!
//! Only the object-naming behavior is portable: Zap removed the sync queue, the RTC listener and
//! the mockable server API that the rest of that suite is built on, so those cases have no fork
//! counterpart. Duplicate naming is pure local logic and is retained unchanged.

use super::get_duplicate_object_name;

#[test]
fn test_get_duplicate_object_name() {
    assert_eq!(
        get_duplicate_object_name("my object name"),
        "my object name (1)"
    );
    assert_eq!(
        get_duplicate_object_name("my object name (1)"),
        "my object name (2)"
    );
    assert_eq!(
        get_duplicate_object_name("my object name (23)"),
        "my object name (24)"
    );
    assert_eq!(
        get_duplicate_object_name("my object name(1234)"),
        "my object name(1234) (1)"
    );
    assert_eq!(
        get_duplicate_object_name("my object name (0)"),
        "my object name (1)"
    );
    assert_eq!(
        get_duplicate_object_name("my object name (-3)"),
        "my object name (-3) (1)"
    );
    assert_eq!(
        get_duplicate_object_name("my object name (18446744073709551615)"),
        "my object name (18446744073709551615) (1)"
    );
    assert_eq!(
        get_duplicate_object_name("my object name (18446744073709551616)"),
        "my object name (18446744073709551616) (1)"
    );
}
