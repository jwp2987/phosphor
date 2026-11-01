use super::*;

#[test]
fn default_limit_refuses_a_40_mb_file_and_admits_a_small_one() {
    let limit = limit_from_env_value(None);
    assert_eq!(limit, Some(CODE_EDITOR_MAX_FILE_BYTES));
    assert!(exceeds_limit(40 * 1024 * 1024, limit));
    assert!(!exceeds_limit(CODE_EDITOR_MAX_FILE_BYTES, limit));
    assert!(!exceeds_limit(10 * 1024, limit));
}

#[test]
fn env_value_overrides_or_disables_the_limit() {
    assert_eq!(limit_from_env_value(Some("0")), None);
    assert_eq!(limit_from_env_value(Some("off")), None);
    assert_eq!(limit_from_env_value(Some("64")), Some(64 * 1024 * 1024));
    assert_eq!(
        limit_from_env_value(Some("garbage")),
        Some(CODE_EDITOR_MAX_FILE_BYTES)
    );
    assert!(!exceeds_limit(u64::MAX, None));
}

#[test]
fn a_missing_file_is_not_reported_as_too_large() {
    assert_eq!(
        too_large_for_code_editor(Path::new("/nonexistent/phosphor/file")),
        None
    );
}
