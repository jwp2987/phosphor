use super::*;
use crate::BulkFilesystemWatcherEvent;
use futures::channel::oneshot;
use notify_debouncer_full::notify::WatchFilter;
use std::collections::HashSet;

const DEBOUNCE: Duration = Duration::from_millis(60);

struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "phosphor-shared-watcher-{}-{}-{name}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(std::fs::canonicalize(&dir).unwrap())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Handle {
    subscription: Subscription,
    events: async_channel::Receiver<BulkFilesystemWatcherEvent>,
}

fn handle(shared: &Arc<SharedFsWatcher>) -> Handle {
    let (tx, events) = async_channel::unbounded();
    Handle {
        subscription: shared.subscribe(DEBOUNCE, WatcherEventHandler { tx }),
        events,
    }
}

fn add(handle: &Handle, path: &Path, mode: RecursiveMode) -> Result<()> {
    let (response, rx) = oneshot::channel();
    handle
        .subscription
        .send(BackgroundFileWatcherCommand::AddPath {
            path: path.to_path_buf(),
            filter: WatchFilter::accept_all(),
            response,
            recursive_mode: mode,
        })?;
    futures::executor::block_on(rx)?
}

fn remove(handle: &Handle, path: &Path) -> Result<()> {
    let (response, rx) = oneshot::channel();
    handle
        .subscription
        .send(BackgroundFileWatcherCommand::RemovePath {
            path: path.to_path_buf(),
            response,
        })?;
    futures::executor::block_on(rx)?
}

/// Every path reported to `handle` within `wait`.
fn seen_paths(handle: &Handle, wait: Duration) -> HashSet<PathBuf> {
    let deadline = Instant::now() + wait;
    let mut paths = HashSet::new();
    while Instant::now() < deadline {
        while let Ok(event) = handle.events.try_recv() {
            paths.extend(event.added_or_updated_iter().cloned());
            paths.extend(event.deleted.iter().cloned());
        }
        thread::sleep(Duration::from_millis(20));
    }
    paths
}

const WAIT: Duration = Duration::from_millis(900);

fn write(path: &Path) {
    std::fs::write(path, b"x").unwrap();
}

#[test]
fn kill_switch_parsing() {
    assert!(!shared_watcher_enabled(Some("0")));
    assert!(!shared_watcher_enabled(Some(" 0 ")));
    if cfg!(target_os = "linux") {
        assert!(shared_watcher_enabled(None));
        assert!(shared_watcher_enabled(Some("1")));
        assert!(shared_watcher_enabled(Some("")));
    }
}

#[test]
fn global_instance_is_shared_between_subscribers() {
    let a = SharedFsWatcher::acquire().unwrap();
    let b = SharedFsWatcher::acquire().unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn separate_roots_are_isolated() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let (dir_a, dir_b) = (TestDir::new("a"), TestDir::new("b"));
    let (a, b) = (handle(&shared), handle(&shared));
    add(&a, dir_a.path(), RecursiveMode::Recursive).unwrap();
    add(&b, dir_b.path(), RecursiveMode::Recursive).unwrap();

    let in_a = dir_a.path().join("one.txt");
    write(&in_a);
    assert!(seen_paths(&a, WAIT).contains(&in_a));
    assert!(seen_paths(&b, Duration::from_millis(100)).is_empty());
}

#[test]
fn nested_non_recursive_root_does_not_see_the_inner_tree() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let outer = TestDir::new("outer");
    let inner = outer.path().join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    let (home, repo) = (handle(&shared), handle(&shared));
    // A non-recursive root with a recursive one below it share the platform watcher.
    add(&home, outer.path(), RecursiveMode::NonRecursive).unwrap();
    add(&repo, &inner, RecursiveMode::Recursive).unwrap();

    let deep = inner.join("deep.txt");
    write(&deep);
    assert!(seen_paths(&repo, WAIT).contains(&deep));
    assert!(!seen_paths(&home, Duration::from_millis(100)).contains(&deep));
}

#[test]
fn overlapping_roots_both_receive_through_a_private_watcher() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let root = TestDir::new("overlap");
    let sub = root.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let (outer, nested) = (handle(&shared), handle(&shared));
    add(&outer, root.path(), RecursiveMode::Recursive).unwrap();
    // Conflicts with the recursive root above: served by this subscriber's private watcher.
    add(&nested, &sub, RecursiveMode::Recursive).unwrap();

    let file = sub.join("f.txt");
    write(&file);
    assert!(seen_paths(&outer, WAIT).contains(&file));
    assert!(seen_paths(&nested, WAIT).contains(&file));

    // The private path unregisters too.
    remove(&nested, &sub).unwrap();
    let after = sub.join("after.txt");
    write(&after);
    assert!(!seen_paths(&nested, Duration::from_millis(400)).contains(&after));
    assert!(seen_paths(&outer, WAIT).contains(&after));
}

#[test]
fn same_root_twice_is_served_to_both() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let root = TestDir::new("same");
    let (a, b) = (handle(&shared), handle(&shared));
    add(&a, root.path(), RecursiveMode::Recursive).unwrap();
    add(&b, root.path(), RecursiveMode::Recursive).unwrap();
    let file = root.path().join("f.txt");
    write(&file);
    assert!(seen_paths(&a, WAIT).contains(&file));
    assert!(seen_paths(&b, WAIT).contains(&file));
}

#[test]
fn unregistering_one_leaves_the_other_watching() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let (dir_a, dir_b) = (TestDir::new("ua"), TestDir::new("ub"));
    let (a, b) = (handle(&shared), handle(&shared));
    add(&a, dir_a.path(), RecursiveMode::Recursive).unwrap();
    add(&b, dir_b.path(), RecursiveMode::Recursive).unwrap();
    remove(&a, dir_a.path()).unwrap();

    write(&dir_a.path().join("ignored.txt"));
    let in_b = dir_b.path().join("kept.txt");
    write(&in_b);
    assert!(seen_paths(&b, WAIT).contains(&in_b));
    assert!(seen_paths(&a, Duration::from_millis(100)).is_empty());
    // Removing something never registered is an error, as with a private watcher.
    assert!(remove(&a, dir_a.path()).is_err());
}

#[test]
fn dropping_a_subscription_stops_only_its_events() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let (dir_a, dir_b) = (TestDir::new("da"), TestDir::new("db"));
    let (a, b) = (handle(&shared), handle(&shared));
    add(&a, dir_a.path(), RecursiveMode::Recursive).unwrap();
    add(&b, dir_b.path(), RecursiveMode::Recursive).unwrap();
    let a_events = a.events.clone();
    drop(a);
    write(&dir_a.path().join("gone.txt"));
    let in_b = dir_b.path().join("still.txt");
    write(&in_b);
    assert!(seen_paths(&b, WAIT).contains(&in_b));
    assert!(a_events.try_recv().is_err());
}

#[test]
fn dropping_the_last_handle_ends_the_threads() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let dir = TestDir::new("end");
    let h = handle(&shared);
    add(&h, dir.path(), RecursiveMode::Recursive).unwrap();
    assert_eq!(shared.alive_threads(), 2);

    let weak = Arc::downgrade(&shared);
    let alive = shared.alive.clone();
    drop(h);
    drop(shared);
    assert!(weak.upgrade().is_none());
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive.load(Ordering::SeqCst) != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(alive.load(Ordering::SeqCst), 0);
}

/// Each subscriber keeps its own window: a burst is one batch.
#[test]
fn a_burst_is_one_batch_per_subscriber() {
    let shared = Arc::new(SharedFsWatcher::start().unwrap());
    let dir = TestDir::new("burst");
    let h = handle(&shared);
    add(&h, dir.path(), RecursiveMode::Recursive).unwrap();
    for i in 0..5 {
        write(&dir.path().join(format!("f{i}.txt")));
    }
    thread::sleep(Duration::from_millis(600));
    let mut batches = 0;
    let mut paths = HashSet::new();
    while let Ok(event) = h.events.try_recv() {
        batches += 1;
        paths.extend(event.added_or_updated_iter().cloned());
    }
    assert_eq!(paths.len(), 5);
    assert_eq!(batches, 1);
}
