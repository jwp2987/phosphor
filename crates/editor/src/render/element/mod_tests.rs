use super::*;

/// Disabled blinking must return no delay and must never toggle visibility —
/// the short-circuit `RichTextElement::paint` relies on to skip scheduling a
/// repaint at all when `cursor_blink` is off. See issue #787 (this behavior
/// predates it and must be unchanged by it).
#[test]
fn test_update_blink_state_disabled_returns_none_and_does_not_toggle() {
    let state = DisplayState::default();
    let initial_visible = state.blink_cursor_visible.load(Ordering::Relaxed);

    assert_eq!(state.update_blink_state(false), None);
    assert_eq!(
        state.blink_cursor_visible.load(Ordering::Relaxed),
        initial_visible,
        "disabled blink must not toggle visibility"
    );
}

/// With no deadline set yet (a freshly constructed `DisplayState`, as for a
/// newly focused editor), the first call is treated as already due: it
/// toggles immediately and arms the next deadline a full
/// `CURSOR_BLINK_INTERVAL` out. This is the delay `RichTextElement::paint`
/// hands to `PaintContext::repaint_after_paint_only_in_region` — issue #787
/// changed *what* schedules the repaint, not this timing.
#[test]
fn test_update_blink_state_first_call_toggles_immediately() {
    let state = DisplayState::default();
    let initial_visible = state.blink_cursor_visible.load(Ordering::Relaxed);

    let delay = state.update_blink_state(true);

    assert_ne!(
        state.blink_cursor_visible.load(Ordering::Relaxed),
        initial_visible,
        "the first call (no deadline set yet) must toggle immediately"
    );
    assert_eq!(delay, Some(CURSOR_BLINK_INTERVAL));
}

/// A call made before the deadline it just armed must not toggle visibility
/// again — otherwise a window re-painting faster than the blink interval
/// (e.g. from an unrelated redraw) would make the cursor flicker instead of
/// blinking at a steady cadence.
#[test]
fn test_update_blink_state_does_not_toggle_again_before_the_deadline() {
    let state = DisplayState::default();
    state.update_blink_state(true);
    let visible_after_first_call = state.blink_cursor_visible.load(Ordering::Relaxed);

    let delay = state.update_blink_state(true);

    assert_eq!(
        state.blink_cursor_visible.load(Ordering::Relaxed),
        visible_after_first_call,
        "a call before the deadline must not toggle visibility again"
    );
    assert!(
        delay.is_some_and(|delay| delay <= CURSOR_BLINK_INTERVAL),
        "the returned delay must still count down to the same deadline, not reset it"
    );
}

/// `reset_cursor_blink_timer` (called when e.g. the cursor moves) must make
/// the cursor visible right away and push the next toggle a full interval
/// out, regardless of whatever deadline was already pending.
#[test]
fn test_reset_cursor_blink_timer_makes_cursor_visible_and_rearms_the_deadline() {
    let state = DisplayState::default();
    // Toggle once so visibility is in a known (non-initial) state, then reset.
    state.update_blink_state(true);
    state.reset_cursor_blink_timer();

    assert!(
        state.blink_cursor_visible.load(Ordering::Relaxed),
        "resetting the blink timer must make the cursor visible"
    );
    // Not due yet (reset just pushed the deadline a full interval out), so
    // this call must report a delay close to, but not past, the full
    // interval, rather than treating it as due and toggling again.
    let delay = state.update_blink_state(true);
    assert!(
        delay.is_some_and(|delay| delay <= CURSOR_BLINK_INTERVAL),
        "a call right after resetting must not be due yet: got {delay:?}"
    );
    assert!(
        state.blink_cursor_visible.load(Ordering::Relaxed),
        "a call that isn't due yet must not toggle visibility"
    );
}
