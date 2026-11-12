use super::*;

/// A provider that fails the same request with the same error every time.
struct AlwaysFailingProvider {
    requests_received: u32,
}

impl AlwaysFailingProvider {
    fn send(&mut self, _fingerprint: u64) -> String {
        self.requests_received += 1;
        "tool call read_shell_command_output could not be mapped".to_owned()
    }
}

/// The resend loop of the controller, reduced to its decision: ask, and on failure resend unless
/// the streak refuses. Returns the refusal text.
fn run_resend_loop(provider: &mut AlwaysFailingProvider, fingerprint: u64) -> String {
    let mut streak = FailureStreak::default();
    // A runaway loop must fail the test, not hang it.
    for _ in 0..100 {
        if let Some(error) = streak.refusal(fingerprint) {
            return refusal_reason(error, streak.count());
        }
        let error = provider.send(fingerprint);
        streak.record(fingerprint, &error);
    }
    panic!("the request was resent 100 times");
}

#[test]
fn an_identical_failing_request_is_sent_exactly_n_times_then_refused_with_the_provider_message() {
    let fingerprint = request_fingerprint(["m1", "m2", "m3"]);
    let mut provider = AlwaysFailingProvider {
        requests_received: 0,
    };
    let reason = run_resend_loop(&mut provider, fingerprint);
    assert_eq!(
        provider.requests_received,
        u32::from(MAX_IDENTICAL_FAILURES)
    );
    assert!(
        reason.contains("read_shell_command_output could not be mapped"),
        "the provider's message, with the tool it names, is shown: {reason}"
    );
}

#[test]
fn a_different_request_or_a_different_error_starts_the_count_again() {
    let a = request_fingerprint(["m1"]);
    let b = request_fingerprint(["m1", "m2"]);
    let mut streak = FailureStreak::default();
    streak.record(a, "boom");
    assert_eq!(streak.refusal(a), None, "one failure is not a streak");
    streak.record(b, "boom");
    assert_eq!(
        streak.refusal(b),
        None,
        "another request restarts the count"
    );
    streak.record(b, "other");
    assert_eq!(streak.refusal(b), None, "another error restarts the count");
    streak.record(b, "other");
    assert_eq!(streak.refusal(b), Some("other"));
    assert_eq!(
        streak.refusal(a),
        None,
        "only the failing request is refused"
    );
}

#[test]
fn the_fingerprint_follows_the_messages_and_their_order() {
    assert_eq!(
        request_fingerprint(["a", "b"]),
        request_fingerprint(["a", "b"])
    );
    assert_ne!(
        request_fingerprint(["a", "b"]),
        request_fingerprint(["b", "a"])
    );
    assert_ne!(request_fingerprint(["a"]), request_fingerprint(["a", "b"]));
}
