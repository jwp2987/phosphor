use super::*;

#[test]
fn deferred_classifier_loads_once_and_only_when_first_needed() {
    let deferred = DeferredClassifier {
        loaded: OnceLock::new(),
        nld_classifier_enabled: false,
    };
    // Building it must not load the model: that is the point (the load used to run on the UI
    // thread during startup).
    assert!(deferred.loaded.get().is_none());

    let first = deferred.get().clone();
    assert!(deferred.loaded.get().is_some());
    // Later uses, and the warm-up thread racing the first classification, share one load.
    assert!(Arc::ptr_eq(&first, deferred.get()));
}

#[test]
fn deferred_classifier_is_shared_across_threads_with_a_single_load() {
    let deferred = Arc::new(DeferredClassifier {
        loaded: OnceLock::new(),
        nld_classifier_enabled: false,
    });
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let deferred = deferred.clone();
            std::thread::spawn(move || deferred.get().clone())
        })
        .collect();
    let loaded: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(
        loaded
            .windows(2)
            .all(|pair| Arc::ptr_eq(&pair[0], &pair[1]))
    );
}
