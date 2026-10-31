use super::*;

#[test]
fn fast_frames_are_not_slow() {
    assert!(!is_slow_frame(None, 0));
    assert!(!is_slow_frame(
        Some(SLOW_KEY_TO_PRESENTED_MS),
        SLOW_DRAWABLE_WAIT_MS
    ));
}

#[test]
fn slow_key_or_wait_is_slow() {
    assert!(is_slow_frame(Some(SLOW_KEY_TO_PRESENTED_MS + 1), 0));
    assert!(is_slow_frame(None, SLOW_DRAWABLE_WAIT_MS + 1));
}

#[test]
fn first_slow_frame_warns_immediately() {
    let mut limiter = SlowFrameLimiter::new();
    let summary = limiter
        .record(5, Some(80), 2)
        .expect("first slow frame warns");
    assert_eq!(
        summary,
        SlowFrameSummary {
            count: 1,
            max_key_to_presented_ms: 80,
            max_drawable_wait_ms: 2
        }
    );
}

#[test]
fn slow_frames_inside_window_are_folded_into_next_summary() {
    let mut limiter = SlowFrameLimiter::new();
    assert!(limiter.record(0, Some(80), 0).is_some());
    assert!(limiter.record(1_000, Some(120), 0).is_none());
    assert!(limiter.record(2_000, None, 20).is_none());
    // Fast frames never emit, even after the window.
    assert!(limiter.record(WARN_INTERVAL_MS + 5, Some(3), 0).is_none());
    let summary = limiter
        .record(WARN_INTERVAL_MS + 10, Some(60), 0)
        .expect("window elapsed");
    assert_eq!(summary.count, 3);
    assert_eq!(summary.max_key_to_presented_ms, 120);
    assert_eq!(summary.max_drawable_wait_ms, 20);
    // The counters reset after a report.
    assert!(limiter.record(WARN_INTERVAL_MS + 11, Some(70), 0).is_none());
}

#[test]
fn deferred_draw_rearms_until_cap() {
    assert!(should_rearm_deferred_draw(0));
    assert!(should_rearm_deferred_draw(MAX_CONSECUTIVE_REARMS - 1));
    assert!(!should_rearm_deferred_draw(MAX_CONSECUTIVE_REARMS));
}

#[test]
fn deferral_streak_rearms_then_stops_and_resets_after_a_draw() {
    note_draw_ran();
    for _ in 0..MAX_CONSECUTIVE_REARMS {
        assert!(note_deferred_draw());
    }
    assert!(!note_deferred_draw());
    note_draw_ran();
    assert!(note_deferred_draw());
    note_draw_ran();
}
