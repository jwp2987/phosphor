use super::*;

/// An empty path must never reach the platform watcher, because macOS turns it into a
/// null-`CFError` release that traps and kills the process.
#[test]
fn empty_paths_never_reach_the_platform_watcher() {
    assert!(ensure_watchable_path(Path::new("")).is_err());

    let directory = std::env::temp_dir();
    assert_eq!(ensure_watchable_path(&directory).ok(), Some(directory));
}

/// The debouncer polls, so the tick is the idle wakeup rate. It must be coarser than the
/// debouncer's default (timeout / 4) yet never exceed the timeout, which `new_debouncer_opt`
/// rejects as an invalid tick rate.
#[test]
fn debouncer_tick_is_coarse_but_never_longer_than_the_timeout() {
    assert_eq!(
        debouncer_tick(Duration::from_millis(500)),
        Duration::from_millis(250)
    );
    assert_eq!(
        debouncer_tick(Duration::from_secs(1)),
        Duration::from_millis(500)
    );
    // Short timeouts are clamped to the timeout itself, not to MIN_DEBOUNCER_TICK.
    assert_eq!(
        debouncer_tick(Duration::from_millis(200)),
        Duration::from_millis(200)
    );
    assert_eq!(
        debouncer_tick(Duration::from_millis(10)),
        Duration::from_millis(10)
    );
}
