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

/// Opens, closes and metadata changes never start a window and are never reported, so reading a
/// watched file cannot cause a spurious flush.
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

fn rename(mode: RenameMode, paths: &[&str]) -> DebouncedEvent {
    let mut e = notify::Event::new(EventKind::Modify(ModifyKind::Name(mode)));
    for p in paths {
        e = e.add_path(PathBuf::from(p));
    }
    DebouncedEvent::new(e, Instant::now())
}

/// What a consumer sees for a raw event sequence on the event-driven path.
fn consumer_view(events: Vec<DebouncedEvent>) -> BulkFilesystemWatcherEvent {
    let events: Vec<_> = events
        .into_iter()
        .filter(|e| is_relevant_event(&e.event))
        .collect();
    deduplicate_and_merge_raw_notifier_events(&normalize_renames(events))
        .expect("a rename produces an update")
}

fn moved(pairs: &[(&str, &str)]) -> HashMap<PathBuf, PathBuf> {
    pairs
        .iter()
        .map(|(to, from)| (PathBuf::from(to), PathBuf::from(from)))
        .collect()
}

/// `A -> B` as inotify reports it (From, To, Both) is one move, exactly what the stitched event
/// of the old debouncer gave the consumer.
#[test]
fn a_paired_rename_is_one_move() {
    let view = consumer_view(vec![
        rename(RenameMode::From, &["/A"]),
        rename(RenameMode::To, &["/B"]),
        rename(RenameMode::Both, &["/A", "/B"]),
    ]);
    assert_eq!(view.moved, moved(&[("/B", "/A")]));
    assert!(view.added.is_empty() && view.deleted.is_empty());
}

/// `A -> B -> C` collapses to `C <- A`, not `C <- B`.
#[test]
fn a_chained_rename_keeps_the_original_source() {
    let view = consumer_view(vec![
        rename(RenameMode::From, &["/A"]),
        rename(RenameMode::To, &["/B"]),
        rename(RenameMode::Both, &["/A", "/B"]),
        rename(RenameMode::From, &["/B"]),
        rename(RenameMode::To, &["/C"]),
        rename(RenameMode::Both, &["/B", "/C"]),
    ]);
    assert_eq!(view.moved, moved(&[("/C", "/A")]));
}

/// A backend that reports only `Both` loses nothing, single or chained.
#[test]
fn both_only_backends_still_move() {
    let single = consumer_view(vec![rename(RenameMode::Both, &["/A", "/B"])]);
    assert_eq!(single.moved, moved(&[("/B", "/A")]));
    let chained = consumer_view(vec![
        rename(RenameMode::Both, &["/A", "/B"]),
        rename(RenameMode::Both, &["/B", "/C"]),
    ]);
    assert_eq!(chained.moved, moved(&[("/C", "/A")]));
}

/// Without `normalize_renames` the same sequence is wrong: this pins why it exists.
#[test]
fn replaying_all_three_halves_would_misreport_a_chain() {
    let raw = vec![
        rename(RenameMode::From, &["/A"]),
        rename(RenameMode::To, &["/B"]),
        rename(RenameMode::Both, &["/A", "/B"]),
        rename(RenameMode::From, &["/B"]),
        rename(RenameMode::To, &["/C"]),
        rename(RenameMode::Both, &["/B", "/C"]),
    ];
    let naive = deduplicate_and_merge_raw_notifier_events(&raw).unwrap();
    assert_eq!(naive.moved, moved(&[("/C", "/B")]));
}

/// An unpaired `From` (a move out of the tree) is left alone for the merge.
#[test]
fn unpaired_halves_are_kept() {
    let kept = normalize_renames(vec![rename(RenameMode::From, &["/A"])]);
    assert_eq!(kept.len(), 1);
}

#[test]
fn legacy_tick_is_coarse_but_never_longer_than_the_timeout() {
    assert_eq!(
        debouncer_tick(Duration::from_millis(500)),
        Duration::from_millis(250)
    );
    assert_eq!(
        debouncer_tick(Duration::from_millis(10)),
        Duration::from_millis(10)
    );
}
