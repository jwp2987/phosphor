use super::*;

// -- AnimationClock -----------------------------------------------------

#[test]
fn clock_elapsed_starts_at_the_given_offset() {
    let clock = AnimationClock::starting_at(Duration::from_secs(5));
    // No monotonic time has passed yet (or vanishingly little), so elapsed()
    // should read back essentially the starting offset.
    let elapsed = clock.elapsed();
    assert!(
        elapsed >= Duration::from_secs(5),
        "elapsed ({elapsed:?}) should be at least the initial offset"
    );
    assert!(
        elapsed < Duration::from_secs(6),
        "elapsed ({elapsed:?}) should not have jumped a full second"
    );
}

#[test]
fn clock_elapsed_advances_with_wall_time() {
    let clock = AnimationClock::starting_at(Duration::ZERO);
    std::thread::sleep(Duration::from_millis(10));
    assert!(
        clock.elapsed() >= Duration::from_millis(10),
        "elapsed() should include monotonic time since the clock was built"
    );
}

// -- KeyframeTimeline -----------------------------------------------------

fn timeline() -> KeyframeTimeline<&'static str> {
    KeyframeTimeline::new([
        Keyframe::from_millis("a", 100),
        Keyframe::from_millis("b", 200),
        Keyframe::from_millis("c", 300),
    ])
}

#[test]
fn value_at_returns_the_keyframe_holding_at_that_offset() {
    let timeline = timeline();
    // "a" holds [0, 100), "b" holds [100, 300), "c" holds [300, 600).
    assert_eq!(*timeline.value_at(Duration::ZERO), "a");
    assert_eq!(*timeline.value_at(Duration::from_millis(99)), "a");
    assert_eq!(*timeline.value_at(Duration::from_millis(100)), "b");
    assert_eq!(*timeline.value_at(Duration::from_millis(299)), "b");
    assert_eq!(*timeline.value_at(Duration::from_millis(300)), "c");
    assert_eq!(*timeline.value_at(Duration::from_millis(599)), "c");
}

#[test]
fn value_at_loops_back_to_the_first_keyframe_after_the_period() {
    let timeline = timeline();
    // Total period is 100 + 200 + 300 = 600ms.
    assert_eq!(*timeline.value_at(Duration::from_millis(600)), "a");
    assert_eq!(*timeline.value_at(Duration::from_millis(700)), "b");
    // Several loops in, still wraps correctly.
    assert_eq!(*timeline.value_at(Duration::from_millis(600 * 3)), "a");
}

#[test]
fn values_returns_every_keyframe_in_order() {
    let timeline = timeline();
    assert_eq!(
        timeline.values().copied().collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[test]
#[should_panic]
fn new_panics_if_every_keyframe_has_a_zero_hold() {
    let _: KeyframeTimeline<&'static str> =
        KeyframeTimeline::new([Keyframe::from_millis("a", 0), Keyframe::from_millis("b", 0)]);
}

#[test]
fn value_at_works_with_a_single_keyframe() {
    let timeline = KeyframeTimeline::new([Keyframe::from_millis("only", 50)]);
    assert_eq!(*timeline.value_at(Duration::ZERO), "only");
    assert_eq!(*timeline.value_at(Duration::from_millis(49)), "only");
    // Loops back to itself past the period.
    assert_eq!(*timeline.value_at(Duration::from_millis(50)), "only");
}

/// Millisecond [`Duration`] shorthand.
fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn holds_each_keyframe_for_its_duration_and_loops() {
    let timeline = KeyframeTimeline::new([
        Keyframe::from_millis("a", 100),
        Keyframe::from_millis("b", 50),
    ]);
    assert_eq!(*timeline.value_at(ms(0)), "a");
    assert_eq!(*timeline.value_at(ms(99)), "a");
    assert_eq!(*timeline.value_at(ms(100)), "b");
    assert_eq!(*timeline.value_at(ms(149)), "b");
    // The timeline loops from its 150ms period.
    assert_eq!(*timeline.value_at(ms(150)), "a");
    assert_eq!(*timeline.value_at(ms(400)), "b");
}

#[test]
fn skips_zero_hold_keyframes() {
    let timeline = KeyframeTimeline::new([
        Keyframe::new("a", ms(100)),
        Keyframe::new("b", Duration::ZERO),
        Keyframe::new("c", ms(100)),
    ]);
    assert_eq!(*timeline.value_at(ms(99)), "a");
    assert_eq!(*timeline.value_at(ms(100)), "c");
}

#[test]
fn values_are_in_timeline_order() {
    let timeline =
        KeyframeTimeline::new([Keyframe::from_millis("a", 1), Keyframe::from_millis("b", 1)]);
    assert_eq!(timeline.values().copied().collect::<Vec<_>>(), ["a", "b"]);
}

#[test]
#[should_panic(expected = "non-zero hold")]
fn rejects_a_timeline_with_no_duration() {
    KeyframeTimeline::<&str>::new([]);
}

#[test]
fn clock_starts_at_its_initial_elapsed_and_advances() {
    // An initial offset far beyond any plausible process uptime must not
    // panic (the underflow `Instant::now() - elapsed` would) and must be
    // preserved in the reported elapsed time.
    let initial = Duration::from_secs(60 * 60 * 24 * 365 * 100);
    let clock = AnimationClock::starting_at(initial);
    assert!(clock.elapsed() >= initial);
}
