use std::cell::Cell;
use std::rc::Rc;

use anyhow::{anyhow, Result};
use futures::executor::block_on;

use super::*;

fn http_err(status: u16) -> anyhow::Error {
    HttpStatusError {
        status,
        body: format!("status {status} body"),
    }
    .into()
}

#[test]
fn transient_5xx_status_codes_are_retryable() {
    assert!(is_transient_http_error(&http_err(503)));
    assert!(is_transient_http_error(&http_err(500)));
}

#[test]
fn transient_408_and_429_are_retryable() {
    assert!(is_transient_http_error(&http_err(408)));
    assert!(is_transient_http_error(&http_err(429)));
}

#[test]
fn permanent_4xx_status_codes_are_not_retryable() {
    assert!(!is_transient_http_error(&http_err(403)));
    assert!(!is_transient_http_error(&http_err(404)));
    assert!(!is_transient_http_error(&http_err(400)));
}

#[test]
fn errors_without_http_status_are_treated_as_transient() {
    // Network-layer errors (connection reset, timeout, DNS failure) aren't `HttpStatusError`;
    // treat them as transient so the retry loop gives them a chance.
    let err = anyhow!("connection reset by peer");
    assert!(is_transient_http_error(&err));

    let err = anyhow!("Failed to send request: timed out");
    assert!(is_transient_http_error(&err));
}

#[test]
fn retry_loop_succeeds_on_first_attempt() {
    let attempts = Rc::new(Cell::new(0));
    let attempts_clone = attempts.clone();
    let result: Result<()> = block_on(with_bounded_retry("test retry", || {
        attempts_clone.set(attempts_clone.get() + 1);
        async { Ok(()) }
    }));
    result.unwrap();
    assert_eq!(attempts.get(), 1);
}

#[test]
fn retry_loop_retries_transient_and_eventually_succeeds() {
    let attempts = Rc::new(Cell::new(0));
    let attempts_clone = attempts.clone();
    let result: Result<u32> = block_on(with_bounded_retry("test retry", || {
        let n = attempts_clone.get() + 1;
        attempts_clone.set(n);
        async move {
            if n < 2 {
                Err(http_err(503))
            } else {
                Ok(n)
            }
        }
    }));
    assert_eq!(result.unwrap(), 2);
    assert_eq!(attempts.get(), 2);
}

#[test]
fn retry_loop_stops_at_max_attempts_on_persistent_transient() {
    let attempts = Rc::new(Cell::new(0));
    let attempts_clone = attempts.clone();
    let result: Result<()> = block_on(with_bounded_retry("test retry", || {
        attempts_clone.set(attempts_clone.get() + 1);
        async { Err(http_err(503)) }
    }));
    assert!(result.is_err());
    assert_eq!(attempts.get(), MAX_ATTEMPTS);
}

#[test]
fn retry_loop_fails_fast_on_permanent_error() {
    let attempts = Rc::new(Cell::new(0));
    let attempts_clone = attempts.clone();
    let result: Result<()> = block_on(with_bounded_retry("test retry", || {
        attempts_clone.set(attempts_clone.get() + 1);
        async { Err(http_err(403)) }
    }));
    assert!(result.is_err());
    assert_eq!(attempts.get(), 1, "permanent errors should not retry");
}

/// Ported from the pin's `with_bounded_retry_using_applies_custom_classifier`
/// (upstream 4111d08f9, `app/src/ai/agent_sdk/retry_tests.rs`).
///
/// Adapted: the pin passes its second shipped classifier,
/// `is_transient_graphql_or_http_error`, whose distinguishing property is that an error
/// with no typed transport cause is *permanent*. That classifier downcasts to
/// `GraphQLError` — dropped-cloud transport this fork does not have — so the same
/// property is expressed by a local classifier here. What is under test is the
/// combinator, not the classifier: `with_bounded_retry_using` must apply the predicate
/// it is handed rather than falling back to the default `is_transient_http_error`, under
/// which an untyped error *is* transient and would be retried to `MAX_ATTEMPTS`.
#[test]
fn with_bounded_retry_using_applies_custom_classifier() {
    // Permanent unless the chain carries a typed `HttpStatusError` — the inverse of
    // `is_transient_http_error`'s fallback, which is what makes this observable.
    fn untyped_is_permanent(e: &anyhow::Error) -> bool {
        e.chain()
            .any(|cause| cause.downcast_ref::<HttpStatusError>().is_some())
    }

    let attempts = Rc::new(Cell::new(0));
    let attempts_clone = attempts.clone();
    let result: Result<()> = block_on(with_bounded_retry_using(
        "test retry",
        MAX_ATTEMPTS,
        untyped_is_permanent,
        || {
            attempts_clone.set(attempts_clone.get() + 1);
            async { Err(anyhow!("untyped operation-layer error")) }
        },
    ));
    assert!(result.is_err());
    assert_eq!(
        attempts.get(),
        1,
        "an untyped error must fail fast under the supplied classifier, not be retried \
         under the default one"
    );
    // The contrast that makes the assertion above about *which* classifier ran, rather
    // than about the error being unretryable in general, is asserted by
    // `errors_without_http_status_are_treated_as_transient` (the default classifier says
    // transient) and `retry_loop_stops_at_max_attempts_on_persistent_transient` (a
    // transient error is retried to `MAX_ATTEMPTS`). Re-deriving it here would pay two
    // more real backoffs for coverage that already exists.
}

/// Ported from the pin's `with_bounded_retry_using_honors_custom_attempt_budget`
/// (upstream 4111d08f9, `app/src/ai/agent_sdk/retry_tests.rs`).
///
/// Adapted: the pin's persistent transient failure is a `GraphQLError::HttpError`
/// classified by `is_transient_graphql_or_http_error`; neither exists in this fork, so a
/// typed `HttpStatusError` 503 under the default `is_transient_http_error` supplies the
/// same "persistently transient" input. Only the budget is under test, so holding the
/// classifier at the default keeps that the single variable.
#[test]
fn with_bounded_retry_using_honors_custom_attempt_budget() {
    // Deliberately different from the shared `MAX_ATTEMPTS` (3), and kept small so the
    // test only pays for one backoff.
    const CUSTOM_MAX_ATTEMPTS: usize = 2;
    let attempts = Rc::new(Cell::new(0));
    let attempts_clone = attempts.clone();
    let result: Result<()> = block_on(with_bounded_retry_using(
        "test retry",
        CUSTOM_MAX_ATTEMPTS,
        is_transient_http_error,
        || {
            attempts_clone.set(attempts_clone.get() + 1);
            async { Err(http_err(503)) }
        },
    ));
    assert!(result.is_err());
    assert_eq!(
        attempts.get(),
        CUSTOM_MAX_ATTEMPTS,
        "should retry up to the caller-supplied budget, not the shared MAX_ATTEMPTS"
    );
    assert_ne!(
        CUSTOM_MAX_ATTEMPTS, MAX_ATTEMPTS,
        "the budget under test must differ from the default, or the assertion above \
         cannot distinguish them"
    );
}
