use super::{count, record, reset_for_test, seen, subscribe, test_guard};

/// #2095: the first unreadable store of a run is news (one notice per launch); later ones are
/// counted and tell nobody.
#[test]
fn the_first_unreadable_store_of_a_run_is_reported_once() {
    let _guard = test_guard();
    reset_for_test();
    assert!(!seen());
    let changes = subscribe();

    assert!(record(), "the first one is reported");
    assert!(seen());
    assert!(!record(), "a second one is not a second notice");
    assert!(!record());
    assert_eq!(count(), 3);

    assert_eq!(changes.try_recv(), Ok(()));
    assert!(
        changes.try_recv().is_err(),
        "subscribers hear of it exactly once"
    );
    reset_for_test();
    assert!(!seen());
}
