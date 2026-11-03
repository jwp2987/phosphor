use super::buffer_event_needs_render;

#[test]
fn a_shell_mode_keystroke_that_keeps_the_buffer_non_empty_does_not_rerender() {
    // Emptiness changing always renders, in either mode.
    assert!(buffer_event_needs_render(true, true, true));
    assert!(buffer_event_needs_render(true, false, true));
    // More text in a non-empty buffer: nothing the bar reads changed.
    assert!(!buffer_event_needs_render(false, true, true));
    assert!(!buffer_event_needs_render(false, false, true));
}

#[test]
fn the_kill_switch_restores_the_per_key_render_in_shell_mode() {
    assert!(buffer_event_needs_render(false, true, false));
    // Agent (AI) mode never re-rendered on a plain edit.
    assert!(!buffer_event_needs_render(false, false, false));
}
