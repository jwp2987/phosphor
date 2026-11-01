//! Cached, off-thread filesystem probes for code that runs on the UI thread.
//!
//! Render paths and menu builders must never `stat` (a network volume or a macOS privacy
//! prompt can stall for seconds). They ask this cache instead: a lookup is a hash-map read;
//! a missing or stale entry returns the last known value (or the caller's default) and
//! schedules ONE background re-probe. A path that becomes valid is therefore recognised on
//! the first lookup after the entry's TTL; `invalidate_all` forces that immediately.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a probe result is trusted before a lookup schedules a refresh.
pub const PROBE_TTL: Duration = Duration::from_secs(5);
const MAX_ENTRIES: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind {
    Exists,
    IsDir,
}

struct Entry {
    value: Option<bool>,
    probed_at: Option<Instant>,
    in_flight: bool,
}

#[derive(Default)]
pub struct PathProbeCache {
    entries: Mutex<HashMap<(Kind, PathBuf), Entry>>,
}

impl PathProbeCache {
    fn lookup(
        &'static self,
        kind: Kind,
        path: &Path,
        default: bool,
        probe: fn(&Path) -> bool,
    ) -> bool {
        let (value, spawn) = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if entries.len() >= MAX_ENTRIES && !entries.contains_key(&(kind, path.to_path_buf())) {
                entries.retain(|_, e| e.in_flight);
            }
            let entry = entries.entry((kind, path.to_path_buf())).or_insert(Entry {
                value: None,
                probed_at: None,
                in_flight: false,
            });
            let stale = entry.probed_at.is_none_or(|t| t.elapsed() >= PROBE_TTL);
            let spawn = stale && !entry.in_flight;
            if spawn {
                entry.in_flight = true;
            }
            (entry.value.unwrap_or(default), spawn)
        };
        if spawn {
            let owned = path.to_path_buf();
            let cache: &'static PathProbeCache = self;
            let spawned = std::thread::Builder::new()
                .name("path-probe".into())
                .spawn(move || {
                    let result = probe(&owned);
                    let mut entries = cache.entries.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(entry) = entries.get_mut(&(kind, owned)) {
                        entry.value = Some(result);
                        entry.probed_at = Some(Instant::now());
                        entry.in_flight = false;
                    }
                });
            if spawned.is_err() {
                let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(entry) = entries.get_mut(&(kind, path.to_path_buf())) {
                    entry.in_flight = false;
                }
            }
        }
        value
    }

    /// Marks every entry stale so the next lookup re-probes (value kept meanwhile).
    pub fn invalidate_all(&self) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        for entry in entries.values_mut() {
            entry.probed_at = None;
        }
    }

    #[cfg(test)]
    fn cached_value(&self, kind: Kind, path: &Path) -> Option<bool> {
        let entries = self.entries.lock().unwrap();
        entries
            .get(&(kind, path.to_path_buf()))
            .and_then(|e| e.value)
    }
}

static GLOBAL: std::sync::OnceLock<PathProbeCache> = std::sync::OnceLock::new();

pub fn global() -> &'static PathProbeCache {
    GLOBAL.get_or_init(PathProbeCache::default)
}

/// `path.exists()` without touching the filesystem on the calling thread. Unknown paths
/// report `default` until the first background probe lands.
pub fn exists_cached(path: &Path, default: bool) -> bool {
    global().lookup(Kind::Exists, path, default, |p| p.exists())
}

/// `path.is_dir()` without touching the filesystem on the calling thread. Unknown paths
/// report `false` until the first background probe lands.
pub fn is_dir_cached(path: &Path) -> bool {
    global().lookup(Kind::IsDir, path, false, |p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for(cache: &PathProbeCache, kind: Kind, path: &Path, want: bool) {
        for _ in 0..200 {
            if cache.cached_value(kind, path) == Some(want) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("probe never produced {want}");
    }

    #[test]
    fn lookup_does_not_probe_on_calling_thread_and_refreshes_on_invalidate() {
        let cache: &'static PathProbeCache = Box::leak(Box::default());
        let dir = std::env::temp_dir().join(format!("path-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // First lookup returns the default immediately, whatever the filesystem says.
        assert!(cache.lookup(Kind::Exists, &dir, true, |p| p.exists()));
        wait_for(cache, Kind::Exists, &dir, false);
        // Cached now: reports the probed value.
        assert!(!cache.lookup(Kind::Exists, &dir, true, |p| p.exists()));
        // The path appears; a stale-forced lookup recognises it on the next check.
        std::fs::create_dir_all(&dir).unwrap();
        cache.invalidate_all();
        assert!(!cache.lookup(Kind::Exists, &dir, true, |p| p.exists())); // old value, refresh scheduled
        wait_for(cache, Kind::Exists, &dir, true);
        assert!(cache.lookup(Kind::Exists, &dir, false, |p| p.exists()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_dir_is_false_until_probed() {
        let cache: &'static PathProbeCache = Box::leak(Box::default());
        let dir = std::env::temp_dir();
        assert!(!cache.lookup(Kind::IsDir, &dir, false, |p| p.is_dir()));
        wait_for(cache, Kind::IsDir, &dir, true);
        assert!(cache.lookup(Kind::IsDir, &dir, false, |p| p.is_dir()));
    }
}
