use super::*;

/// An empty path must never reach the platform watcher, because macOS turns it into a
/// null-`CFError` release that traps and kills the process.
#[test]
fn empty_paths_never_reach_the_platform_watcher() {
    assert!(ensure_watchable_path(Path::new("")).is_err());

    let directory = std::env::temp_dir();
    assert_eq!(ensure_watchable_path(&directory).ok(), Some(directory));
}

fn event(kind: EventKind, path: &str) -> notify::Result<notify::Event> {
    Ok(notify::Event::new(kind).add_path(PathBuf::from(path)))
}

fn data_modified(path: &str) -> notify::Result<notify::Event> {
    event(
        EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
        path,
    )
}

/// Collects every flush `run_debounce_loop` makes for the events sent by `feed`, which runs on
/// its own thread so the loop sees real arrival timing.
fn collect_flushes(
    timeout: Duration,
    feed: impl FnOnce(mpsc::Sender<notify::Result<notify::Event>>) + Send + 'static,
) -> Vec<Vec<DebouncedEvent>> {
    let (tx, rx) = mpsc::channel();
    let feeder = thread::spawn(move || feed(tx));
    let mut flushes = Vec::new();
    run_debounce_loop(&rx, timeout, |events, _errors| {
        if !events.is_empty() {
            flushes.push(events);
        }
    });
    feeder.join().unwrap();
    flushes
}

/// A burst inside one window is flushed once, in arrival order.
#[test]
fn a_burst_is_flushed_as_one_batch() {
    let flushes = collect_flushes(Duration::from_millis(200), |tx| {
        for path in ["/a", "/b", "/c"] {
            tx.send(data_modified(path)).unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        // Keep the sender alive past the window so the batch is closed by the timeout.
        thread::sleep(Duration::from_millis(400));
    });
    assert_eq!(flushes.len(), 1);
    let paths: Vec<_> = flushes[0]
        .iter()
        .map(|e| e.event.paths[0].clone())
        .collect();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/c")
        ]
    );
}

/// Events separated by more than the window are separate batches.
#[test]
fn events_after_the_window_start_a_new_batch() {
    let flushes = collect_flushes(Duration::from_millis(80), |tx| {
        tx.send(data_modified("/a")).unwrap();
        thread::sleep(Duration::from_millis(300));
        tx.send(data_modified("/b")).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    assert_eq!(flushes.len(), 2);
}

/// Opens, closes, metadata changes and the `Both` half of a rename never start a window and are
/// never reported, so reading a watched file or a paired rename cannot cause a spurious flush.
#[test]
fn irrelevant_events_are_dropped() {
    let flushes = collect_flushes(Duration::from_millis(50), |tx| {
        tx.send(event(
            EventKind::Access(notify::event::AccessKind::Any),
            "/a",
        ))
        .unwrap();
        tx.send(event(
            EventKind::Modify(ModifyKind::Metadata(notify::event::MetadataKind::Any)),
            "/a",
        ))
        .unwrap();
        tx.send(event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            "/a",
        ))
        .unwrap();
        thread::sleep(Duration::from_millis(200));
    });
    assert!(flushes.is_empty());
}

/// Dropping the platform watcher (the sender) ends the loop and flushes what is pending, so no
/// event is lost on shutdown and the thread does not outlive the watcher.
#[test]
fn pending_events_are_flushed_when_the_watcher_is_dropped() {
    let flushes = collect_flushes(Duration::from_secs(30), |tx| {
        tx.send(event(
            EventKind::Create(notify::event::CreateKind::File),
            "/new",
        ))
        .unwrap();
    });
    assert_eq!(flushes.len(), 1);
    assert_eq!(flushes[0][0].event.paths, vec![PathBuf::from("/new")]);
}
