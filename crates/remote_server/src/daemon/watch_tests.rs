use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;

#[derive(Default, Clone)]
struct Recorder {
    calls: Arc<Mutex<Vec<String>>>,
    refuse: Arc<Mutex<Option<WatchErrorKind>>>,
}

#[derive(Clone, Copy)]
enum WatchErrorKind {
    Os,
    Capacity,
}

impl Backend for Recorder {
    fn watch(&mut self, path: &Path) -> Result<(), WatchError> {
        match *self.refuse.lock().unwrap() {
            Some(WatchErrorKind::Os) => return Err(WatchError::Os("gone".into())),
            Some(WatchErrorKind::Capacity) => return Err(WatchError::CapReached { cap: 0 }),
            None => {}
        }
        self.calls
            .lock()
            .unwrap()
            .push(format!("watch {}", path.display()));
        Ok(())
    }

    fn unwatch(&mut self, path: &Path) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("unwatch {}", path.display()));
    }
}

#[test]
fn the_os_watch_lives_while_any_consumer_holds_the_path() {
    let rec = Recorder::default();
    let mut set = WatchSet::with_backend(Box::new(rec.clone()), 10);
    let p = Path::new("/a");
    set.acquire(p).unwrap();
    set.acquire(p).unwrap();
    assert_eq!(set.len(), 1);
    set.release(p);
    assert!(set.is_watched(p), "one consumer still holds it");
    set.release(p);
    assert!(!set.is_watched(p));
    set.release(p); // extra release is a no-op
    assert_eq!(*rec.calls.lock().unwrap(), vec!["watch /a", "unwatch /a"]);
}

#[test]
fn the_cap_refuses_new_paths_but_not_new_references_to_watched_ones() {
    let rec = Recorder::default();
    let mut set = WatchSet::with_backend(Box::new(rec), 2);
    set.acquire(Path::new("/a")).unwrap();
    set.acquire(Path::new("/b")).unwrap();
    let err = set.acquire(Path::new("/c")).unwrap_err();
    assert!(matches!(err, WatchError::CapReached { cap: 2 }), "{err}");
    assert!(err.is_capacity());
    set.acquire(Path::new("/a")).unwrap();
    // Releasing frees a slot.
    set.release(Path::new("/b"));
    set.acquire(Path::new("/c")).unwrap();
}

#[test]
fn an_os_refusal_is_not_recorded_and_distinguishes_capacity_from_a_missing_path() {
    let rec = Recorder::default();
    let mut set = WatchSet::with_backend(Box::new(rec.clone()), 10);
    *rec.refuse.lock().unwrap() = Some(WatchErrorKind::Os);
    let err = set.acquire(Path::new("/gone")).unwrap_err();
    assert!(!err.is_capacity());
    *rec.refuse.lock().unwrap() = Some(WatchErrorKind::Capacity);
    let err = set.acquire(Path::new("/full")).unwrap_err();
    assert!(err.is_capacity());
    assert_eq!(set.len(), 0);
}

#[test]
fn a_disabled_set_fails_every_acquire_as_unavailable() {
    let mut set = WatchSet::disabled();
    let err = set.acquire(Path::new("/a")).unwrap_err();
    assert!(matches!(err, WatchError::Unavailable(_)));
    assert!(err.is_capacity());
}

#[test]
fn access_events_are_not_forwarded_and_the_rest_are_classified() {
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind, RenameMode};
    assert_eq!(classify(&EventKind::Access(AccessKind::Any)), None);
    assert_eq!(
        classify(&EventKind::Create(CreateKind::File)),
        Some(FsKind::Create)
    );
    assert_eq!(
        classify(&EventKind::Remove(RemoveKind::Any)),
        Some(FsKind::Remove)
    );
    assert_eq!(
        classify(&EventKind::Modify(ModifyKind::Name(RenameMode::Both))),
        Some(FsKind::Rename)
    );
    assert_eq!(
        classify(&EventKind::Modify(ModifyKind::Any)),
        Some(FsKind::Modify)
    );
}

#[test]
fn a_real_write_reaches_the_event_channel_with_no_debounce() {
    let dir = tempfile::tempdir().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let (tx, rx) = mpsc::channel::<Event>();
    let mut set = WatchSet::new(tx);
    if set.acquire(&dir_path).is_err() {
        eprintln!("no watcher on this machine; skipping");
        return;
    }
    std::fs::write(dir_path.join("new.txt"), "x").unwrap();
    // Wait for the event itself (no sleeping): the first FsChange naming the file.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(Event::FsChange { paths, .. }) if paths.iter().any(|p| p.ends_with("new.txt")) => {
                break;
            }
            Ok(_) => continue,
            Err(_) => panic!("no FsChange for the new file"),
        }
    }
}
