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

#[test]
fn typing_that_keeps_the_bar_the_same_does_not_rerender() {
    use super::{buffer_event_needs_render, BufferSignature};
    let ls = BufferSignature::of("ls");
    let lsa = BufferSignature::of("ls -a");
    // The first event always renders; then more of the same kind of text does not.
    assert!(buffer_event_needs_render(None, ls, true));
    assert!(!buffer_event_needs_render(Some(ls), lsa, true));
    // Emptiness and the command prefixes the bar reads still render.
    assert!(buffer_event_needs_render(
        Some(BufferSignature::of("")),
        ls,
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of(""),
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of("/agent"),
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of(" /plan x"),
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of("/new"),
        true
    ));
    // Kill switch off: every event renders.
    assert!(buffer_event_needs_render(Some(ls), lsa, false));
}
