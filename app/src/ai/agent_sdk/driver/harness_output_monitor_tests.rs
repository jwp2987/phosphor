use std::time::Duration;

use super::{STALL_CONFIRMATION_BUDGET, STALL_POLL_INTERVAL, outputs_stalled, pattern_for_match};

#[test]
fn pattern_for_match_returns_originating_needle() {
    let patterns: &[&'static str] = &["credit balance is too low", "invalid_api_key"];
    // The DFA gives us the matched substring (e.g. lifted from the grid as
    // mixed case); we must map back to the original `'static` needle.
    let resolved = pattern_for_match("Credit Balance Is Too Low", patterns);
    assert_eq!(resolved, Some("credit balance is too low"));
}

#[test]
fn pattern_for_match_is_case_insensitive() {
    let patterns: &[&'static str] = &["INVALID_API_KEY"];
    let resolved = pattern_for_match("invalid_api_key", patterns);
    assert_eq!(resolved, Some("INVALID_API_KEY"));
}

#[test]
fn pattern_for_match_returns_none_when_no_match() {
    let patterns: &[&'static str] = &["needle a", "needle b"];
    assert!(pattern_for_match("entirely different text", patterns).is_none());
}

#[test]
fn pattern_for_match_picks_first_matching_needle() {
    // When two needles lowercase-equal the same matched text (degenerate
    // case), we deterministically return the first one in the slice.
    let patterns: &[&'static str] = &["Foo", "foo"];
    let resolved = pattern_for_match("FOO", patterns);
    assert_eq!(resolved, Some("Foo"));
}

// --- outputs_stalled / stall confirmation ---

#[test]
fn outputs_stalled_returns_true_when_inputs_equal() {
    let snapshot = "Error: credit balance is too low\n";
    assert!(outputs_stalled(Some(snapshot), Some(snapshot)));
}

#[test]
fn outputs_stalled_returns_false_when_inputs_differ_by_any_byte() {
    // Spinner case: a single character differs between frames. The
    // confirmation loop must treat this as "still moving" and not
    // mistakenly declare the harness stalled.
    let before = "Retrying \u{2807}";
    let after = "Retrying \u{2826}";
    assert!(!outputs_stalled(Some(before), Some(after)));
}

#[test]
fn outputs_stalled_returns_false_when_either_input_is_none() {
    // A failed snapshot fetch must default to "not confirmed" rather than
    // killing the harness on a transient lookup error.
    assert!(!outputs_stalled(None, Some("data")));
    assert!(!outputs_stalled(Some("data"), None));
    assert!(!outputs_stalled(None, None));
}

#[test]
fn stall_confirmation_budget_matches_six_poll_intervals() {
    // The loop guarantees up to BUDGET/INTERVAL iterations. Pin the ratio
    // so a careless tweak to either constant can't accidentally turn the
    // confirmation into a single comparison or extend it indefinitely.
    assert_eq!(
        STALL_CONFIRMATION_BUDGET.as_secs() / STALL_POLL_INTERVAL.as_secs(),
        6
    );
    // And that they're cleanly divisible (no leftover sub-interval window).
    assert_eq!(
        STALL_CONFIRMATION_BUDGET.as_secs() % STALL_POLL_INTERVAL.as_secs(),
        0
    );
    // Sanity: STALL_POLL_INTERVAL must be non-zero or the loop would spin.
    assert!(STALL_POLL_INTERVAL > Duration::ZERO);
}

// --- event-driven watch (#1381, APP-13) ---

mod watch {
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    use futures::executor::block_on;

    use super::super::{BlockOutputMatch, watch_with};

    fn hit() -> BlockOutputMatch {
        BlockOutputMatch {
            matched_text: "invalid_api_key".into(),
            excerpt: "Error: invalid_api_key".into(),
        }
    }

    const QUIET: Duration = Duration::from_millis(40);
    const BUDGET: Duration = Duration::from_millis(400);

    /// The error is already printed when the watch starts: it is reported after one quiet
    /// window, not after a 5 s sample. (The old code could not report before its first 5 s
    /// tick at all.)
    #[test]
    fn reports_error_already_on_screen_after_one_quiet_window() {
        let (_tx, rx) = async_channel::bounded::<()>(1);
        let started = std::time::Instant::now();
        let got = block_on(watch_with(
            rx,
            || async { Some(hit()) },
            || async { Some("Error: invalid_api_key".to_string()) },
            QUIET,
            BUDGET,
            Duration::ZERO,
        ));
        assert_eq!(got.unwrap().matched_text, "invalid_api_key");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// An error printed after an arbitrarily long quiet stretch is found on the output event
    /// that carries it. The old schedule stopped looking after ~90 s.
    #[test]
    fn error_printed_later_is_found_on_its_output_event() {
        let (tx, rx) = async_channel::bounded::<()>(1);
        let printed = Cell::new(false);
        let tx2 = tx.clone();
        let got = block_on(
            watch_with(
                rx,
                || {
                    let seen = printed.get();
                    async move { seen.then(hit) }
                },
                || async { Some("Error: invalid_api_key".to_string()) },
                QUIET,
                BUDGET,
                Duration::ZERO,
            )
            .fuse_with(async {
                // The output event arrives after the (empty) first scan.
                warpui::r#async::Timer::after(Duration::from_millis(60)).await;
                printed.set(true);
                tx2.send(()).await.unwrap();
            }),
        );
        assert!(got.is_some());
    }

    /// A hit while the block keeps changing (a retry spinner) is not reported.
    #[test]
    fn hit_while_block_keeps_changing_is_not_reported() {
        let (tx, rx) = async_channel::bounded::<()>(1);
        let frame = Cell::new(0u32);
        let calls = RefCell::new(0u32);
        let outcome = block_on(async {
            let watcher = watch_with(
                rx,
                || async { Some(hit()) },
                || {
                    *calls.borrow_mut() += 1;
                    frame.set(frame.get() + 1);
                    let f = frame.get();
                    async move { Some(format!("retrying {f}")) }
                },
                QUIET,
                BUDGET,
                Duration::from_millis(5),
            );
            futures::pin_mut!(watcher);
            let driver = async {
                // Spin the block for longer than the budget.
                for _ in 0..40 {
                    warpui::r#async::Timer::after(Duration::from_millis(15)).await;
                    let _ = tx.try_send(());
                }
            };
            futures::pin_mut!(driver);
            match futures::future::select(watcher, driver).await {
                futures::future::Either::Left(_) => "reported",
                futures::future::Either::Right(_) => "not reported",
            }
        });
        assert_eq!(outcome, "not reported");
    }

    /// Terminal gone: the watch ends instead of spinning.
    #[test]
    fn closed_signal_ends_the_watch() {
        let (tx, rx) = async_channel::bounded::<()>(1);
        drop(tx);
        let got = block_on(watch_with(
            rx,
            || async { None },
            || async { None },
            QUIET,
            BUDGET,
            Duration::ZERO,
        ));
        assert!(got.is_none());
    }

    trait FuseWith: std::future::Future + Sized {
        /// Run `self` while `side` runs alongside; resolve with `self`'s output.
        async fn fuse_with<S: std::future::Future<Output = ()>>(self, side: S) -> Self::Output {
            futures::pin_mut!(self);
            futures::pin_mut!(side);
            match futures::future::select(self, side).await {
                futures::future::Either::Left((out, _)) => out,
                futures::future::Either::Right(((), rest)) => rest.await,
            }
        }
    }
    impl<F: std::future::Future> FuseWith for F {}
}
