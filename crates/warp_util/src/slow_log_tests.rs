use super::*;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn log() -> SlowLog {
    SlowLog::new("test", ms(50), ms(1000))
}

#[test]
fn under_threshold_is_suppressed_and_leaves_state_alone() {
    let l = log();
    let t0 = Instant::now();
    assert_eq!(l.check(ms(49), t0), Verdict::Suppress);
    // The first real slow op still emits with no suppressed summary.
    assert_eq!(
        l.check(ms(50), t0),
        Verdict::Emit {
            suppressed: 0,
            worst: ms(50)
        }
    );
}

#[test]
fn burst_is_rate_limited_then_summarized() {
    let l = log();
    let t0 = Instant::now();
    assert!(matches!(l.check(ms(60), t0), Verdict::Emit { .. }));
    assert_eq!(l.check(ms(70), t0 + ms(100)), Verdict::Suppress);
    assert_eq!(l.check(ms(200), t0 + ms(200)), Verdict::Suppress);
    assert_eq!(l.check(ms(80), t0 + ms(300)), Verdict::Suppress);
    assert_eq!(
        l.check(ms(55), t0 + ms(1000)),
        Verdict::Emit {
            suppressed: 3,
            worst: ms(200)
        }
    );
    // The window restarts after an emit.
    assert_eq!(l.check(ms(60), t0 + ms(1500)), Verdict::Suppress);
}

#[test]
fn truncate_for_log_limits_and_flattens() {
    assert_eq!(truncate_for_log("ls -la", 10), "ls -la");
    assert_eq!(truncate_for_log("abcdefghij", 4), "abcd…");
    assert_eq!(truncate_for_log("one\ntwo", 10), "one…");
}
