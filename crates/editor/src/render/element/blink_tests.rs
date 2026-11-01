use std::sync::atomic::Ordering;
use std::time::Duration;

use instant::Instant;

use super::{CURSOR_BLINK_INTERVAL, DisplayState, MAX_IDLE_CURSOR_BLINKS};

#[test]
fn idle_cursor_stops_blinking_and_stays_solid() {
    let state = DisplayState::default();
    state.reset_cursor_blink_timer();
    let mut now = Instant::now();

    // Every tick after the interval toggles and asks for another repaint, up to the cap.
    for _ in 0..MAX_IDLE_CURSOR_BLINKS {
        now += CURSOR_BLINK_INTERVAL;
        assert_eq!(state.tick_blink(now), Some(CURSOR_BLINK_INTERVAL));
    }
    // The cap is reached: solid cursor, and no further repaint is scheduled.
    now += CURSOR_BLINK_INTERVAL;
    assert_eq!(state.tick_blink(now), None);
    assert!(state.blink_cursor_visible.load(Ordering::Relaxed));
    now += CURSOR_BLINK_INTERVAL * 100;
    assert_eq!(state.tick_blink(now), None);
    assert!(state.blink_cursor_visible.load(Ordering::Relaxed));
}

#[test]
fn activity_restarts_blinking_after_the_cap() {
    let state = DisplayState::default();
    state.reset_cursor_blink_timer();
    let mut now = Instant::now();
    for _ in 0..=MAX_IDLE_CURSOR_BLINKS {
        now += CURSOR_BLINK_INTERVAL;
        state.tick_blink(now);
    }
    assert_eq!(state.tick_blink(now + CURSOR_BLINK_INTERVAL), None);

    state.reset_cursor_blink_timer();
    let later = Instant::now() + CURSOR_BLINK_INTERVAL + Duration::from_millis(1);
    assert_eq!(state.tick_blink(later), Some(CURSOR_BLINK_INTERVAL));
}
