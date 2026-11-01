//! Cached, off-thread filesystem probes for code that runs on the UI thread.
//!
//! Render paths and menu builders must never `stat` (a network volume or a macOS privacy
//! prompt can stall for seconds). They ask this cache instead: a lookup is a hash-map read;
//! a missing or stale entry returns the last known value (or the caller's default) and
//! schedules ONE background re-probe. A path that becomes valid is therefore recognised on
//! the first lookup after the entry's TTL; `invalidate_all` forces that immediately.
//!
//! Completion is an event: `subscribe()` yields a `PathProbed` whenever a result differs from
//! what the last lookup returned. A stat that never returns (dead mount) is abandoned after
//! `GIVE_UP` (logged) so it cannot hold one of the pool's workers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a probe result is trusted before a lookup schedules a refresh.
pub const PROBE_TTL: Duration = Duration::from_secs(5);
const MAX_ENTRIES: usize = 4096;

type Task = Box<dyn FnOnce() + Send + 'static>;

/// Worker threads (a stalled network stat only blocks one of them).
const WORKERS: usize = 3;
/// Pending probes beyond this are dropped (and retried by a later lookup).
const QUEUE_CAPACITY: usize = 256;

static QUEUE: std::sync::OnceLock<Option<std::sync::mpsc::SyncSender<Task>>> =
    std::sync::OnceLock::new();

fn submit(task: Task) -> bool {
    let sender = QUEUE.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Task>(QUEUE_CAPACITY);
        let rx = std::sync::Arc::new(Mutex::new(rx));
        for i in 0..WORKERS {
            let rx = rx.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("path-probe-{i}"))
                .spawn(move || {
                    loop {
                        // Hold the receiver lock only while waiting for the next task.
                        let task = {
                            let guard = rx.lock().unwrap_or_else(|e| e.into_inner());
                            guard.recv()
                        };
                        match task {
                            Ok(task) => task(),
                            Err(_) => return,
                        }
                    }
                });
            if spawned.is_err() && i == 0 {
                return None;
            }
        }
        Some(tx)
    });
    match sender {
        Some(tx) => tx.try_send(task).is_ok(),
        None => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind {
    Exists,
    IsDir,
}

/// Sent to subscribers when a probe lands a value different from the one the last lookup
/// returned, so the requester can re-render instead of waiting for an unrelated repaint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathProbed {
    pub path: PathBuf,
    pub value: bool,
}

/// Safety net, not a poll: a `stat` that has not returned after this long is on a dead
/// mount (no OS API offers a cancellable stat). The worker is released and the path stays
/// "unknown" (default / last value) until the stat eventually returns.
const GIVE_UP: Duration = Duration::from_secs(3);
/// Cap on stat threads that are alive at once (stuck ones included).
const MAX_STAT_THREADS: usize = 32;
static STAT_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Entry {
    value: Option<bool>,
    /// What the most recent lookup returned (to notify only on a real change).
    served: Option<bool>,
    probed_at: Option<Instant>,
    in_flight: bool,
}

pub struct PathProbeCache {
    entries: Mutex<HashMap<(Kind, PathBuf), Entry>>,
    subscribers: Mutex<Vec<async_channel::Sender<PathProbed>>>,
    give_up: Duration,
}

impl Default for PathProbeCache {
    fn default() -> Self {
        Self::with_give_up(GIVE_UP)
    }
}

impl PathProbeCache {
    fn with_give_up(give_up: Duration) -> Self {
        Self {
            entries: Mutex::default(),
            subscribers: Mutex::default(),
            give_up,
        }
    }

    /// Receive an event whenever a probe result differs from what lookups last returned.
    /// Dropping the receiver unsubscribes (pruned on the next send).
    pub fn subscribe(&self) -> async_channel::Receiver<PathProbed> {
        let (tx, rx) = async_channel::unbounded();
        self.subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(tx);
        rx
    }

    fn notify(&self, event: PathProbed) {
        let mut subs = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        subs.retain(|tx| tx.try_send(event.clone()).is_ok());
    }

    /// Stores a landed result; notifies if it differs from what was last served.
    fn finish(&self, kind: Kind, path: &Path, result: bool) {
        let changed = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            match entries.get_mut(&(kind, path.to_path_buf())) {
                Some(entry) => {
                    entry.value = Some(result);
                    entry.probed_at = Some(Instant::now());
                    entry.in_flight = false;
                    let changed = entry.served != Some(result);
                    entry.served = Some(result);
                    changed
                }
                None => false,
            }
        };
        if changed {
            self.notify(PathProbed {
                path: path.to_path_buf(),
                value: result,
            });
        }
    }

    fn clear_in_flight(&self, kind: Kind, path: &Path) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = entries.get_mut(&(kind, path.to_path_buf())) {
            entry.in_flight = false;
        }
    }

    /// Runs on a pool worker: stats on a dedicated thread and waits at most `give_up`, so a
    /// dead mount can never hold a pool slot.
    fn run_probe(&'static self, kind: Kind, path: PathBuf, probe: fn(&Path) -> bool) {
        use std::sync::atomic::Ordering;
        if STAT_THREADS.fetch_add(1, Ordering::SeqCst) >= MAX_STAT_THREADS {
            STAT_THREADS.fetch_sub(1, Ordering::SeqCst);
            log::warn!("path_probe: too many stuck stats; {path:?} stays unknown");
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = entries.get_mut(&(kind, path)) {
                entry.in_flight = false;
                entry.probed_at = Some(Instant::now());
            }
            return;
        }
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let owned = path.clone();
        let spawned = std::thread::Builder::new()
            .name("stat-probe".into())
            .spawn(move || {
                let result = probe(&owned);
                // A late result (after the worker gave up) still lands and notifies.
                self.finish(kind, &owned, result);
                STAT_THREADS.fetch_sub(1, Ordering::SeqCst);
                let _ = done_tx.send(());
            });
        if spawned.is_err() {
            STAT_THREADS.fetch_sub(1, Ordering::SeqCst);
            self.clear_in_flight(kind, &path);
            return;
        }
        if done_rx.recv_timeout(self.give_up).is_err() {
            // in_flight deliberately stays true: no second stuck thread for this path; the
            // late result (if any) clears it.
            log::warn!(
                "path_probe: stat of {path:?} still blocked after {:?}; releasing the worker, \
                 path stays unknown",
                self.give_up
            );
        }
    }

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
                served: None,
                probed_at: None,
                in_flight: false,
            });
            let stale = entry.probed_at.is_none_or(|t| t.elapsed() >= PROBE_TTL);
            let spawn = stale && !entry.in_flight;
            if spawn {
                entry.in_flight = true;
            }
            let value = entry.value.unwrap_or(default);
            entry.served = Some(value);
            (value, spawn)
        };
        if spawn {
            let owned = path.to_path_buf();
            let cache: &'static PathProbeCache = self;
            let task: Task = Box::new(move || cache.run_probe(kind, owned, probe));
            // Bounded queue + fixed worker pool: hovering many links can never create many
            // threads. If the queue is full the probe is simply dropped and retried by a
            // later lookup.
            if !submit(task) {
                self.clear_in_flight(kind, path);
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

/// Subscribe to probe-completion events from the global cache.
pub fn subscribe() -> async_channel::Receiver<PathProbed> {
    global().subscribe()
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

    #[test]
    fn many_distinct_probes_use_a_fixed_pool_and_all_complete() {
        let cache: &'static PathProbeCache = Box::leak(Box::default());
        let base = std::env::temp_dir().join("path-probe-many");
        let paths: Vec<PathBuf> = (0..100).map(|i| base.join(format!("p{i}"))).collect();
        for p in &paths {
            cache.lookup(Kind::Exists, p, true, |p| p.exists());
        }
        for p in &paths {
            wait_for(cache, Kind::Exists, p, false);
        }
        // Only the bounded pool's threads exist.
        let count = std::fs::read_dir("/proc/self/task")
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter(|e| {
                        std::fs::read_to_string(e.path().join("comm"))
                            .is_ok_and(|c| c.starts_with("path-probe"))
                    })
                    .count()
            })
            .unwrap_or(0);
        assert!(count <= WORKERS, "{count} probe threads");
    }

    fn blocking_probe(p: &Path) -> bool {
        if p.ends_with("blocked") {
            // Simulates a stat on a dead mount: returns only when released.
            while !RELEASE.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        false
    }
    static RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    #[test]
    fn completion_notifies_subscriber_once_with_the_real_value() {
        let cache: &'static PathProbeCache = Box::leak(Box::default());
        let rx = cache.subscribe();
        let p = std::env::temp_dir().join("path-probe-notify-missing-xyz");
        // Default says it exists; the real answer is false.
        assert!(cache.lookup(Kind::Exists, &p, true, |p| p.exists()));
        let ev = rx.recv_blocking().unwrap();
        assert_eq!(
            ev,
            PathProbed {
                path: p.clone(),
                value: false
            }
        );
        // A refresh that finds the same value is not a change: no second event.
        cache.invalidate_all();
        assert!(!cache.lookup(Kind::Exists, &p, true, |p| p.exists()));
        wait_for(cache, Kind::Exists, &p, false);
        std::thread::sleep(Duration::from_millis(50));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn dropped_subscriber_is_pruned_without_panic() {
        let cache: &'static PathProbeCache = Box::leak(Box::default());
        let rx = cache.subscribe();
        drop(rx);
        let p = std::env::temp_dir().join("path-probe-notify-dropped-xyz");
        cache.lookup(Kind::Exists, &p, true, |p| p.exists());
        wait_for(cache, Kind::Exists, &p, false);
        assert!(cache.subscribers.lock().unwrap().is_empty());
    }

    #[test]
    fn a_blocked_stat_does_not_stop_other_paths_resolving() {
        let cache: &'static PathProbeCache = Box::leak(Box::new(PathProbeCache::with_give_up(
            Duration::from_millis(50),
        )));
        let base = std::env::temp_dir().join("path-probe-blocked");
        // More stuck probes than workers: without the give-up the pool would be exhausted.
        for i in 0..WORKERS + 2 {
            cache.lookup(
                Kind::Exists,
                &base.join(format!("d{i}")).join("blocked"),
                true,
                blocking_probe,
            );
        }
        let ok = base.join("fine");
        cache.lookup(Kind::Exists, &ok, true, blocking_probe);
        wait_for(cache, Kind::Exists, &ok, false);
        RELEASE.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
