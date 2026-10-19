use super::is_ai_detected;

#[test]
fn autodetected_requires_live_ai_input_type() {
    // Regression for #894: autodetection enabled + unlocked + non-empty buffer
    // while the live input type is Shell must NOT read as "autodetected AI".
    assert!(!is_ai_detected(true, true, false, false));
    assert!(is_ai_detected(true, true, false, true));
}

#[test]
fn autodetected_requires_other_conditions() {
    assert!(!is_ai_detected(false, true, false, true));
    assert!(!is_ai_detected(true, false, false, true));
    assert!(!is_ai_detected(true, true, true, true));
}
