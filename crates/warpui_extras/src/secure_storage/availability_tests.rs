use super::*;

/// The first failure is the only one reported; later failures change nothing and notify nobody.
#[test]
fn unavailable_is_reported_once_per_run() {
    let _guard = test_guard();
    reset_for_test();
    let changes = subscribe();

    assert!(should_contact_keyring());
    assert!(mark_unavailable(), "the first failure changes the state");
    assert!(!mark_unavailable(), "a second failure is not news");
    assert!(!mark_unavailable());
    assert!(
        !should_contact_keyring(),
        "no further keyring calls this run"
    );

    assert_eq!(changes.try_recv(), Ok(Availability::Unavailable));
    assert!(
        changes.try_recv().is_err(),
        "exactly one transition published"
    );
    reset_for_test();
}

/// Only an explicit retry (or a late success) lets the keyring be tried again.
#[test]
fn retry_and_late_success_reopen_the_keyring() {
    let _guard = test_guard();
    reset_for_test();
    let changes = subscribe();

    assert!(!retry(), "nothing to retry while the state is unknown");
    mark_unavailable();
    assert!(retry());
    assert_eq!(current(), Availability::Unknown);
    assert!(should_contact_keyring());

    mark_unavailable();
    assert!(mark_available(), "a call that succeeded late flips it back");
    assert!(should_contact_keyring());

    let seen: Vec<_> = std::iter::from_fn(|| changes.try_recv().ok()).collect();
    assert_eq!(
        seen,
        [
            Availability::Unavailable,
            Availability::Unknown,
            Availability::Unavailable,
            Availability::Available,
        ]
    );
    reset_for_test();
}

/// A dropped subscriber is pruned instead of failing later transitions.
#[test]
fn dropped_subscribers_are_pruned() {
    let _guard = test_guard();
    reset_for_test();
    drop(subscribe());
    let live = subscribe();
    assert!(mark_unavailable());
    assert_eq!(live.try_recv(), Ok(Availability::Unavailable));
    reset_for_test();
}
