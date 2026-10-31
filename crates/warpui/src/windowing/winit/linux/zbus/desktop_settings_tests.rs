use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn empty_cache_never_blocks_longer_than_the_wait() {
    let cache = ThemeCache::new();
    let start = Instant::now();
    assert_eq!(cache.get_or_wait(Duration::from_millis(20)), None);
    // The old code blocked up to 200ms per lookup on a missing portal.
    assert!(start.elapsed() < Duration::from_millis(150));
}

#[test]
fn filled_cache_returns_immediately_and_tracks_updates() {
    let cache = ThemeCache::new();
    cache.set(SystemTheme::Dark);
    let start = Instant::now();
    assert_eq!(
        cache.get_or_wait(Duration::from_secs(5)),
        Some(SystemTheme::Dark)
    );
    assert!(start.elapsed() < Duration::from_millis(500));
    cache.set(SystemTheme::Light);
    assert_eq!(cache.get(), Some(SystemTheme::Light));
}

#[test]
fn waiter_is_woken_by_the_prefetch() {
    let cache = Arc::new(ThemeCache::new());
    let writer = Arc::clone(&cache);
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        writer.set(SystemTheme::Dark);
    });
    assert_eq!(
        cache.get_or_wait(Duration::from_secs(5)),
        Some(SystemTheme::Dark)
    );
    handle.join().unwrap();
}
