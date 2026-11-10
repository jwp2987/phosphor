use super::hidden_panel_event_needs_render;

#[test]
fn a_hidden_panel_is_not_rerendered_for_typing() {
    // No queue: the first character typed into an empty input, or a suggestions-mode change,
    // finds the panel hidden and leaves it hidden.
    assert!(!hidden_panel_event_needs_render(false, false, true));
}

#[test]
fn a_shown_panel_and_a_panel_that_changes_visibility_are_rerendered() {
    // Shown: the "⏎ to send" hint follows the input's emptiness.
    assert!(hidden_panel_event_needs_render(true, true, true));
    // An inline menu opened over a queue (hides) or closed again (shows).
    assert!(hidden_panel_event_needs_render(true, false, true));
    assert!(hidden_panel_event_needs_render(false, true, true));
}

#[test]
fn with_the_kill_switch_every_event_rerenders() {
    assert!(hidden_panel_event_needs_render(false, false, false));
}
