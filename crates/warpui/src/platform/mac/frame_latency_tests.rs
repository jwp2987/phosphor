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
fn dropped_draw_is_owed_and_queues_exactly_one_flush() {
    let owed = DrawOwed::new();
    assert!(owed.note_deferred(), "first drop queues the flush");
    assert!(owed.is_owed());
    // Further drops before the borrow is released do not queue more flushes.
    for _ in 0..5000 {
        assert!(!owed.note_deferred());
    }
    assert!(owed.take_for_flush(), "flush requests the owed redraw once");
    assert!(!owed.is_owed());
    assert!(!owed.take_for_flush(), "nothing owed after the flush");
}

#[test]
fn flush_does_nothing_if_a_draw_ran_in_between() {
    let owed = DrawOwed::new();
    assert!(owed.note_deferred());
    owed.note_draw_ran();
    assert!(!owed.take_for_flush());
}

#[test]
fn a_new_drop_after_a_flush_queues_a_new_flush() {
    let owed = DrawOwed::new();
    assert!(owed.note_deferred());
    assert!(owed.take_for_flush());
    assert!(owed.note_deferred());
    assert!(owed.take_for_flush());
}

fn gate(unpresented: i32, skipping_for_ms: Option<u64>) -> GateInputs {
    GateInputs {
        enabled: true,
        must_draw: false,
        presented_once: unpresented >= 0,
        unpresented,
        skipping_for_ms,
    }
}

#[test]
fn the_gate_skips_only_when_a_drawable_is_already_queued() {
    // Nothing has reached the screen yet (-1): the first drawables are free.
    assert_eq!(gate_decision(gate(-1, None)), GateDecision::Draw);
    // One on screen, none queued: the next nextDrawable returns at once.
    assert_eq!(gate_decision(gate(0, None)), GateDecision::Draw);
    // One on screen, one queued: nextDrawable would block the main thread.
    assert_eq!(gate_decision(gate(1, None)), GateDecision::Skip);
    assert_eq!(gate_decision(gate(2, Some(10))), GateDecision::Skip);
}

#[test]
fn the_gate_never_skips_when_disabled_or_when_a_draw_is_required() {
    let mut inputs = gate(1, None);
    inputs.enabled = false;
    assert_eq!(gate_decision(inputs), GateDecision::Draw);
    let mut inputs = gate(1, None);
    inputs.must_draw = true;
    assert_eq!(gate_decision(inputs), GateDecision::Draw);
}

#[test]
fn the_gate_watchdog_forces_a_draw_after_a_long_skip_run() {
    assert_eq!(
        gate_decision(gate(1, Some(GATE_WATCHDOG_MS - 1))),
        GateDecision::Skip
    );
    assert_eq!(
        gate_decision(gate(1, Some(GATE_WATCHDOG_MS))),
        GateDecision::Draw
    );
}

#[test]
fn a_skip_run_counts_frames_and_ends_when_a_frame_is_drawn() {
    let run = SkipRun::default();
    assert_eq!(run.running_for_ms(5), None);
    assert_eq!(run.finish(5), None);
    run.note_skip(10);
    run.note_skip(14);
    run.note_skip(30);
    assert_eq!(run.running_for_ms(40), Some(30));
    assert_eq!(run.finish(40), Some((3, 30)));
    // the run is over: the next one starts fresh
    assert_eq!(run.running_for_ms(50), None);
    run.note_skip(60);
    assert_eq!(run.finish(61), Some((1, 1)));
}

#[test]
fn a_skip_run_counts_the_key_driven_frames_it_skipped() {
    let run = SkipRun::default();
    run.note_skip(1);
    run.note_skip(2);
    run.note_key_skip();
    run.note_skip(3);
    assert_eq!(run.take_key_skipped(), 1);
    assert_eq!(run.finish(10), Some((3, 9)));
    // the next run starts at zero
    run.note_skip(20);
    assert_eq!(run.take_key_skipped(), 0);
}

#[test]
fn key_presented_detail_has_the_fields_the_harness_parses() {
    assert_eq!(
        key_presented_detail(8_300_000, 34_500_000, false, 2_250_000),
        "submit_to_presented_ms=8.30 key_to_presented_ms=34.50 sync=0 wait_ms=2.25"
    );
    assert!(key_presented_detail(0, 1, true, 0).contains("sync=1"));
}

#[test]
fn key_frame_sched_detail_is_milliseconds() {
    assert_eq!(key_frame_sched_detail(1_234_000), "ms=1.23");
    assert_eq!(key_frame_sched_detail(0), "ms=0.00");
}

#[test]
fn a_key_frame_is_due_only_when_the_key_press_requested_a_redraw() {
    let requests = RedrawRequests::default();
    let at_key_start = requests.latest();
    // The key changed nothing in this window: no frame.
    assert!(!requests.begin_key_frame(true, at_key_start));
    let request = requests.note_request();
    assert_eq!(request, 1, "numbers start at 1; 0 means untracked");
    assert!(requests.begin_key_frame(true, at_key_start));
}

#[test]
fn the_kill_switch_leaves_every_request_to_the_display_pass() {
    let requests = RedrawRequests::default();
    let at_key_start = requests.latest();
    let request = requests.note_request();
    assert!(!requests.begin_key_frame(false, at_key_start));
    assert!(!requests.finish_key_frame());
    assert!(!requests.already_drawn(request));
}

#[test]
fn a_drawn_key_frame_covers_the_requests_made_before_it_began() {
    let requests = RedrawRequests::default();
    // An output frame was requested before the key and its posted block has not run yet.
    let output = requests.note_request();
    let at_key_start = requests.latest();
    let key_down = requests.note_request();
    let typed = requests.note_request();
    assert!(requests.begin_key_frame(true, at_key_start));
    requests.note_frame_submitted();
    assert!(requests.finish_key_frame());
    // The frame was built from the state after all three: none needs another display pass.
    assert!(requests.already_drawn(output));
    assert!(requests.already_drawn(key_down));
    assert!(requests.already_drawn(typed));
    // A request after the key frame is a new invalidation and is never dropped.
    let later = requests.note_request();
    assert!(!requests.already_drawn(later));
}

#[test]
fn a_key_frame_that_was_not_drawn_covers_nothing() {
    // The #1838 gate skipped it, the app was borrowed, or no drawable was vended: the posted
    // requests must still reach AppKit, or the key would never be shown.
    let requests = RedrawRequests::default();
    let at_key_start = requests.latest();
    let request = requests.note_request();
    assert!(requests.begin_key_frame(true, at_key_start));
    assert!(!requests.finish_key_frame());
    assert!(!requests.already_drawn(request));
}

#[test]
fn a_request_made_while_the_key_frame_draws_is_not_covered() {
    // A scene build that invalidates the window again asks for a frame the key frame is not.
    let requests = RedrawRequests::default();
    let at_key_start = requests.latest();
    let request = requests.note_request();
    assert!(requests.begin_key_frame(true, at_key_start));
    let during = requests.note_request();
    requests.note_frame_submitted();
    assert!(requests.finish_key_frame());
    assert!(requests.already_drawn(request));
    assert!(!requests.already_drawn(during));
}

#[test]
fn a_key_frame_never_starts_inside_another() {
    let requests = RedrawRequests::default();
    let at_key_start = requests.latest();
    requests.note_request();
    assert!(requests.begin_key_frame(true, at_key_start));
    // A nested key press handled while the first frame draws.
    requests.note_request();
    assert!(!requests.begin_key_frame(true, at_key_start));
    requests.note_frame_submitted();
    assert!(requests.finish_key_frame());
    // The outer finish ended it; a second finish reports nothing drawn.
    assert!(!requests.finish_key_frame());
}

#[test]
fn an_untracked_request_is_never_dropped() {
    let requests = RedrawRequests::default();
    let at_key_start = requests.latest();
    requests.note_request();
    assert!(requests.begin_key_frame(true, at_key_start));
    requests.note_frame_submitted();
    assert!(requests.finish_key_frame());
    assert!(!requests.already_drawn(0));
}

#[test]
fn frames_drawn_outside_a_key_frame_cover_nothing() {
    let requests = RedrawRequests::default();
    let request = requests.note_request();
    requests.note_frame_submitted();
    assert!(!requests.already_drawn(request));
    assert!(!requests.finish_key_frame());
}

#[test]
fn only_zero_turns_the_key_frame_off() {
    assert!(key_frame_now_enabled_from_env(None));
    assert!(key_frame_now_enabled_from_env(Some("1")));
    assert!(!key_frame_now_enabled_from_env(Some("0")));
    assert!(!key_frame_now_enabled_from_env(Some("0\n")));
}

#[test]
fn key_frame_now_detail_has_the_fields_the_harness_parses() {
    // script/realapp/appci/render_hops.py _KEY_FRAME_NOW.
    assert_eq!(key_frame_now_detail(true, 4.256), "drawn=1 ms=4.26");
    assert_eq!(key_frame_now_detail(false, 0.0), "drawn=0 ms=0.00");
}
