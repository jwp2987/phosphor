use super::*;

fn surface_err(error: GetSurfaceTextureError) -> Error {
    Error::SurfaceError(error)
}

#[test]
fn timeout_and_occluded_never_count_toward_exit() {
    let mut state = DrawRetryState::default();
    // Far more than the crash-recovery threshold of 3.
    for i in 0..100 {
        let error = if i % 2 == 0 {
            GetSurfaceTextureError::Timeout
        } else {
            GetSurfaceTextureError::Occluded
        };
        let decision = state.on_frame_error(&surface_err(error));
        assert!(!decision.counts_toward_exit, "iteration {i}");
        assert!(!decision.recreate_renderer);
        assert!(
            decision.retry_after.is_some(),
            "a skipped frame must re-arm"
        );
    }
}

#[test]
fn retry_delay_backs_off_and_is_capped() {
    let mut state = DrawRetryState::default();
    let ms: Vec<_> = (0..8)
        .map(|_| {
            state
                .on_frame_error(&surface_err(GetSurfaceTextureError::Timeout))
                .retry_after
                .unwrap()
                .as_millis()
        })
        .collect();
    assert_eq!(ms, vec![16, 32, 64, 128, 250, 250, 250, 250]);
}

#[test]
fn successful_frame_resets_backoff() {
    let mut state = DrawRetryState::default();
    for _ in 0..4 {
        state.on_frame_error(&surface_err(GetSurfaceTextureError::Timeout));
    }
    state.on_frame_drawn();
    let decision = state.on_frame_error(&surface_err(GetSurfaceTextureError::Timeout));
    assert_eq!(decision.retry_after, Some(Duration::from_millis(16)));
}

#[test]
fn lost_recreates_renderer_and_only_counts_when_persistent() {
    let mut state = DrawRetryState::default();
    for i in 0..MAX_CONSECUTIVE_RECONFIGURE_FAILURES {
        let decision = state.on_frame_error(&surface_err(GetSurfaceTextureError::Lost));
        assert!(decision.recreate_renderer);
        assert!(!decision.counts_toward_exit, "failure {i}");
        assert!(decision.retry_after.is_some());
    }
    let decision = state.on_frame_error(&surface_err(GetSurfaceTextureError::Lost));
    assert!(decision.counts_toward_exit);
}

#[test]
fn outdated_retries_without_recreating() {
    let mut state = DrawRetryState::default();
    let decision = state.on_frame_error(&surface_err(GetSurfaceTextureError::Outdated));
    assert!(!decision.recreate_renderer);
    assert!(!decision.counts_toward_exit);
    assert!(decision.retry_after.is_some());
}

#[test]
fn transient_error_between_lost_errors_resets_the_lost_streak() {
    let mut state = DrawRetryState::default();
    for _ in 0..MAX_CONSECUTIVE_RECONFIGURE_FAILURES {
        state.on_frame_error(&surface_err(GetSurfaceTextureError::Lost));
    }
    state.on_frame_error(&surface_err(GetSurfaceTextureError::Timeout));
    let decision = state.on_frame_error(&surface_err(GetSurfaceTextureError::Lost));
    assert!(!decision.counts_toward_exit);
}

#[test]
fn fatal_errors_still_count_and_recreate() {
    let mut state = DrawRetryState::default();
    let decision = state.on_frame_error(&Error::DeviceLost);
    assert!(decision.counts_toward_exit);
    assert!(decision.recreate_renderer);
    assert_eq!(decision.retry_after, None);
}

#[test]
fn validation_errors_still_count() {
    let mut state = DrawRetryState::default();
    let decision = state.on_frame_error(&surface_err(GetSurfaceTextureError::Validation));
    assert!(decision.counts_toward_exit);
    assert!(!decision.recreate_renderer);
}

#[test]
fn rate_limiter_reports_first_event_then_batches() {
    let t0 = Instant::now();
    let mut limiter = RateLimitedSummary::new(Duration::from_secs(10));

    assert_eq!(
        limiter.record(t0, 5.0),
        Some(Summary { count: 1, max: 5.0 })
    );
    assert_eq!(limiter.record(t0 + Duration::from_secs(1), 9.0), None);
    assert_eq!(limiter.record(t0 + Duration::from_secs(2), 7.0), None);
    assert_eq!(
        limiter.record(t0 + Duration::from_secs(10), 1.0),
        Some(Summary { count: 3, max: 9.0 })
    );
    // Counters restart after a report.
    assert_eq!(limiter.record(t0 + Duration::from_secs(11), 2.0), None);
    assert_eq!(
        limiter.record(t0 + Duration::from_secs(21), 3.0),
        Some(Summary { count: 2, max: 3.0 })
    );
}

#[test]
fn slow_frame_thresholds() {
    let ms = Duration::from_millis;
    assert!(!is_slow_frame(ms(16), None));
    assert!(!is_slow_frame(ms(50), Some(ms(8))));
    assert!(is_slow_frame(ms(51), None));
    assert!(is_slow_frame(ms(10), Some(ms(9))));
}
