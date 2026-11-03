use super::*;

fn image(bytes: Vec<u8>) -> ImageData {
    ImageData {
        data: bytes,
        mime_type: "image/png".to_owned(),
        filename: None,
    }
}

#[test]
fn in_memory_availability_needs_no_read() {
    let mut clipboard = InMemoryClipboard::default();
    assert_eq!(
        clipboard.availability(ClipboardTarget::Clipboard),
        Availability::Empty
    );
    assert!(!clipboard.has_content());

    clipboard.write(ClipboardContent {
        plain_text: "hello".to_owned(),
        images: Some(vec![image(vec![1, 2, 3])]),
        ..Default::default()
    });
    assert!(clipboard.has_content());
}

#[test]
fn image_only_clipboard_has_content() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write(ClipboardContent {
        images: Some(vec![image(vec![1])]),
        ..Default::default()
    });
    assert!(clipboard.has_content());
}

/// A clipboard whose reader must never be created: `has_content` answers from a cheap query
/// (#1306, #1405).
struct QueryOnlyClipboard(Availability);

impl Clipboard for QueryOnlyClipboard {
    fn write(&mut self, _contents: ClipboardContent) {}
    fn reader(&mut self, _kind: ReadKind) -> ClipboardReader {
        panic!("an availability query must not read or decode clipboard content");
    }
    fn availability(&mut self, _target: ClipboardTarget) -> Availability {
        self.0
    }
}

#[test]
fn has_content_follows_availability_and_never_reads() {
    assert!(QueryOnlyClipboard(Availability::HasContent).has_content());
    assert!(!QueryOnlyClipboard(Availability::Empty).has_content());
    // A platform that cannot tell without reading must not disable "Paste".
    assert!(QueryOnlyClipboard(Availability::Unknown).has_content());
}

#[test]
fn text_and_paths_read_drops_html_and_images_but_full_keeps_them() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write(ClipboardContent {
        plain_text: "hello".to_owned(),
        html: Some("<b>hello</b>".to_owned()),
        images: Some(vec![image(vec![1])]),
        ..Default::default()
    });
    let text = read_content_blocking(&mut clipboard, ReadKind::TextAndPaths);
    assert_eq!(text.plain_text, "hello");
    assert!(text.html.is_none() && text.images.is_none());
    let full = read_content_blocking(&mut clipboard, ReadKind::Full);
    assert!(full.html.is_some() && full.has_image_data());
}

#[test]
fn legacy_readers_are_absent_for_inline_clipboards() {
    // An inline snapshot has nothing to move off the calling thread (#1405).
    let mut clipboard = InMemoryClipboard::default();
    assert!(clipboard.background_text_reader().is_none());
    assert!(clipboard.background_full_reader().is_none());
    assert!(clipboard.background_primary_reader().is_none());
}

#[test]
fn request_ids_are_unique_and_increasing() {
    let mut clipboard = InMemoryClipboard::default();
    let a = request_read(&mut clipboard, ReadKind::Full);
    let b = request_read(&mut clipboard, ReadKind::Full);
    assert!(b.id > a.id);
    assert_eq!(a.kind, ReadKind::Full);
}

#[test]
fn unavailable_is_empty_content_in_the_older_vocabulary() {
    assert!(matches!(
        BackgroundTextRead::from(ReadOutcome::Unavailable),
        BackgroundTextRead::Content(content) if content.is_empty()
    ));
    assert!(matches!(
        BackgroundTextRead::from(ReadOutcome::TimedOut),
        BackgroundTextRead::TimedOut
    ));
    assert!(ReadOutcome::TimedOut.into_content().is_empty());
}

#[test]
fn ready_request_resolves_through_into_future() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write(ClipboardContent::plain_text("later".to_owned()));
    let request = request_read(&mut clipboard, ReadKind::TextAndPaths);
    assert!(matches!(request.start, ReadStart::Ready(_)));
    match futures::executor::block_on(request.into_future()) {
        ReadOutcome::Content(content) => assert_eq!(content.plain_text, "later"),
        other => panic!("expected content, got {other:?}"),
    }
}

#[test]
fn later_write_wins_when_it_runs_first() {
    let order = WriteOrder::new();
    let first = order.next_generation();
    let second = order.next_generation();
    let written = std::cell::RefCell::new(vec![]);
    assert_eq!(
        order.run(second, ClipboardTarget::Clipboard, || {
            written.borrow_mut().push(2);
            true
        }),
        WriteOutcome::Written
    );
    assert_eq!(
        order.run(first, ClipboardTarget::Clipboard, || {
            written.borrow_mut().push(1);
            true
        }),
        WriteOutcome::Superseded
    );
    assert_eq!(*written.borrow(), vec![2]);
}

#[test]
fn targets_are_ordered_independently_and_failures_report() {
    let order = WriteOrder::new();
    let a = order.next_generation();
    let b = order.next_generation();
    assert_eq!(
        order.run(b, ClipboardTarget::Primary, || true),
        WriteOutcome::Written
    );
    assert_eq!(
        order.run(a, ClipboardTarget::Clipboard, || false),
        WriteOutcome::Failed
    );
}

#[test]
fn slow_earlier_write_never_overwrites_a_later_one() {
    let order = std::sync::Arc::new(WriteOrder::new());
    let value = std::sync::Arc::new(Mutex::new(0u64));
    let first = order.next_generation();
    let second = order.next_generation();
    let (o1, v1) = (order.clone(), value.clone());
    let slow = std::thread::spawn(move || {
        o1.run(first, ClipboardTarget::Clipboard, || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            *v1.lock() = 1;
            true
        })
    });
    std::thread::sleep(std::time::Duration::from_millis(20));
    let (o2, v2) = (order.clone(), value.clone());
    let fast = std::thread::spawn(move || {
        o2.run(second, ClipboardTarget::Clipboard, || {
            *v2.lock() = 2;
            true
        })
    });
    slow.join().unwrap();
    fast.join().unwrap();
    assert_eq!(*value.lock(), 2);
}

#[test]
fn write_without_a_writer_thread_still_lands_and_reports() {
    // One write API (#1405): a platform with a cheap write is written inline, `on_done` fires, and
    // the caller never needs its own synchronous fallback.
    let mut clipboard = InMemoryClipboard::default();
    let done = std::sync::Arc::new(Mutex::new(None));
    let done_in = done.clone();
    let result = write_in_background(
        &mut clipboard,
        ClipboardContent::plain_text("x".into()),
        ClipboardTarget::Clipboard,
        move |outcome| *done_in.lock() = Some(outcome),
    );
    assert!(result.is_ok());
    assert_eq!(*done.lock(), Some(WriteOutcome::Written));
    assert_eq!(
        read_content_blocking(&mut clipboard, ReadKind::Full).plain_text,
        "x"
    );
}

#[test]
fn read_right_after_copy_sees_the_pending_write() {
    let pending = PendingWrites::new();
    let old = std::sync::Arc::new(ClipboardContent::plain_text("old".into()));
    let new = std::sync::Arc::new(ClipboardContent::plain_text("new".into()));
    assert!(pending.latest(ClipboardTarget::Clipboard).is_none());
    pending.add(1, ClipboardTarget::Clipboard, old);
    pending.add(2, ClipboardTarget::Clipboard, new);
    pending.add(
        3,
        ClipboardTarget::Primary,
        std::sync::Arc::new(ClipboardContent::plain_text("sel".into())),
    );
    // A paste 0 ms after the second copy gets the second copy's text.
    assert_eq!(
        pending
            .latest(ClipboardTarget::Clipboard)
            .unwrap()
            .plain_text,
        "new"
    );
    assert_eq!(
        pending.latest(ClipboardTarget::Primary).unwrap().plain_text,
        "sel"
    );
    // The newer copy finishing first leaves the older one from winning.
    pending.finish(2);
    assert_eq!(
        pending
            .latest(ClipboardTarget::Clipboard)
            .unwrap()
            .plain_text,
        "old"
    );
    pending.finish(1);
    assert!(pending.latest(ClipboardTarget::Clipboard).is_none());
}

#[test]
fn serial_writer_keeps_one_handle_and_write_order() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static OPENED: AtomicUsize = AtomicUsize::new(0);
    static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let writer = serial_writer::<u8>(
        || {
            OPENED.fetch_add(1, Ordering::SeqCst);
            Some(0)
        },
        |_, content, _| {
            LOG.lock().push(content.plain_text);
            true
        },
    );
    for text in ["a", "b", "c"] {
        assert!(writer(
            ClipboardContent::plain_text(text.into()),
            ClipboardTarget::Clipboard
        ));
    }
    assert_eq!(*LOG.lock(), vec!["a", "b", "c"]);
    assert_eq!(OPENED.load(Ordering::SeqCst), 1);
}

#[test]
fn tracked_reader_stops_starting_blocked_readers_past_the_cap() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static TRACKER: ReaderTracker = ReaderTracker::new();
    static RAN: AtomicUsize = AtomicUsize::new(0);
    let slow = std::time::Duration::from_secs(60);
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let release_rx = std::sync::Arc::new(Mutex::new(release_rx));
    // Two readers wedge (cap 2) until released.
    let mut handles = Vec::new();
    for _ in 0..2 {
        let rx = release_rx.clone();
        let reader = tracked_reader_with(
            &TRACKER,
            2,
            slow,
            ReadRequestId::next(),
            ReadKind::Full,
            Box::new(move || {
                RAN.fetch_add(1, Ordering::SeqCst);
                let _ = rx.lock().recv();
                ReadOutcome::Content(ClipboardContent::default())
            }),
        );
        handles.push(std::thread::spawn(reader));
    }
    while TRACKER.in_flight() < 2 {
        std::thread::yield_now();
    }
    // A third attempt does not make another blocking call.
    let third = tracked_reader_with(
        &TRACKER,
        2,
        slow,
        ReadRequestId::next(),
        ReadKind::Full,
        Box::new(|| {
            RAN.fetch_add(1, Ordering::SeqCst);
            ReadOutcome::Content(ClipboardContent::default())
        }),
    );
    assert!(matches!(third(), ReadOutcome::TimedOut));
    assert_eq!(RAN.load(Ordering::SeqCst), 2);
    release_tx.send(()).unwrap();
    release_tx.send(()).unwrap();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(TRACKER.in_flight(), 0);
}

/// #1353 phase 6 (INP-A4): a paste that gives up on its read signals the readers' cancel flag, and
/// the count of readers still blocked in the OS is what it reports.
#[test]
fn abandoning_pending_reads_reports_the_blocked_readers() {
    static TRACKER: ReaderTracker = ReaderTracker::new();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let reader = tracked_reader_with(
        &TRACKER,
        2,
        std::time::Duration::from_secs(60),
        ReadRequestId::next(),
        ReadKind::Full,
        Box::new(move || {
            let _ = release_rx.recv();
            ReadOutcome::Content(ClipboardContent::default())
        }),
    );
    assert_eq!(TRACKER.abandon_pending(), 0, "nothing is blocked yet");
    let handle = std::thread::spawn(reader);
    while TRACKER.in_flight() < 1 {
        std::thread::yield_now();
    }
    assert_eq!(
        TRACKER.abandon_pending(),
        1,
        "the wedged reader is the abandoned one"
    );
    release_tx.send(()).unwrap();
    // The abandoned read still returns what it read (another paste may be waiting on one made
    // before the abandon); it is only logged.
    assert!(matches!(handle.join().unwrap(), ReadOutcome::Content(_)));
    assert_eq!(TRACKER.in_flight(), 0);
}

#[test]
fn over_cap_paste_is_refused_not_truncated() {
    let big = ClipboardContent::plain_text("x".repeat(11));
    match cap_paste_size(ReadOutcome::Content(big), 10) {
        ReadOutcome::TooLarge(n) => assert_eq!(n, 11),
        _ => panic!("expected TooLarge"),
    }
    // Exactly at the cap is admitted whole, as are paths and html counted together.
    match cap_paste_size(
        ReadOutcome::Content(ClipboardContent::plain_text("x".repeat(10))),
        10,
    ) {
        ReadOutcome::Content(c) => assert_eq!(c.plain_text.len(), 10),
        _ => panic!("expected Content"),
    }
    let mixed = ClipboardContent {
        plain_text: "aaaaaa".into(),
        html: Some("bbbbbb".into()),
        ..Default::default()
    };
    assert!(matches!(
        cap_paste_size(ReadOutcome::Content(mixed), 10),
        ReadOutcome::TooLarge(12)
    ));
    assert!(matches!(
        cap_paste_size(ReadOutcome::TimedOut, 10),
        ReadOutcome::TimedOut
    ));
    assert!(matches!(
        cap_paste_size(ReadOutcome::Unavailable, 10),
        ReadOutcome::Unavailable
    ));
}

/// A clipboard whose reads are served by a worker-thread reader, like the Linux/Windows/macOS ones.
struct WorkerClipboard {
    text: String,
    main_thread: std::thread::ThreadId,
}

impl Clipboard for WorkerClipboard {
    fn write(&mut self, content: ClipboardContent) {
        self.text = content.plain_text;
    }

    fn reader(&mut self, _kind: ReadKind) -> ClipboardReader {
        let text = self.text.clone();
        let main_thread = self.main_thread;
        Box::new(move || {
            // A read on the UI thread is exactly what #1380 and #1405 remove.
            assert_ne!(std::thread::current().id(), main_thread);
            ReadOutcome::Content(ClipboardContent::plain_text(text))
        })
    }
}

#[test]
fn request_read_runs_the_reader_off_the_calling_thread() {
    let mut clipboard = WorkerClipboard {
        text: "from the worker".to_owned(),
        main_thread: std::thread::current().id(),
    };
    for kind in [ReadKind::Full, ReadKind::TextAndPaths] {
        match request_read(&mut clipboard, kind).start {
            ReadStart::Pending(pending) => match futures::executor::block_on(pending) {
                ReadOutcome::Content(content) => {
                    assert_eq!(content.plain_text, "from the worker")
                }
                other => panic!("expected content, got {other:?}"),
            },
            ReadStart::Ready(_) => panic!("a platform with a worker reader must not read here"),
        }
    }
}

#[test]
fn read_in_background_adapter_stays_pending_for_worker_clipboards() {
    let mut clipboard = WorkerClipboard {
        text: "adapter".to_owned(),
        main_thread: std::thread::current().id(),
    };
    match read_in_background(&mut clipboard, ReadKind::TextAndPaths) {
        StartedRead::Pending(pending) => match futures::executor::block_on(pending) {
            BackgroundTextRead::Content(content) => assert_eq!(content.plain_text, "adapter"),
            _ => panic!("expected content"),
        },
        StartedRead::Ready(_) => panic!("must read on a worker"),
    }
    // The reader form handed to the paste paths is a worker reader too.
    let reader = clipboard
        .background_text_reader()
        .expect("worker clipboards hand out a reader");
    assert!(matches!(
        std::thread::spawn(reader).join().unwrap(),
        BackgroundTextRead::Content(_)
    ));
}

#[test]
fn read_in_background_without_a_worker_reader_answers_at_once() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write(ClipboardContent::plain_text("local".to_owned()));
    match read_in_background(&mut clipboard, ReadKind::TextAndPaths) {
        StartedRead::Ready(BackgroundTextRead::Content(content)) => {
            assert_eq!(content.plain_text, "local")
        }
        _ => panic!("expected an immediate read"),
    }
}

/// A reader that ignores the owner's silence reports `TimedOut`; the blocking helper passes that
/// through instead of turning it into an empty paste.
#[test]
fn a_timed_out_reader_is_reported_not_hidden() {
    struct Silent;
    impl Clipboard for Silent {
        fn write(&mut self, _contents: ClipboardContent) {}
        fn reader(&mut self, _kind: ReadKind) -> ClipboardReader {
            Box::new(|| ReadOutcome::TimedOut)
        }
        fn reads_inline(&self) -> bool {
            true
        }
    }
    assert!(matches!(
        read_outcome_blocking(&mut Silent, ReadKind::Full),
        ReadOutcome::TimedOut
    ));
    assert!(read_content_blocking(&mut Silent, ReadKind::Full).is_empty());
}

#[test]
fn oversize_content_is_refused_by_every_read_path() {
    struct Huge;
    impl Clipboard for Huge {
        fn write(&mut self, _contents: ClipboardContent) {}
        fn reader(&mut self, _kind: ReadKind) -> ClipboardReader {
            Box::new(|| {
                ReadOutcome::Content(ClipboardContent::plain_text(
                    "x".repeat(MAX_PASTE_BYTES + 1),
                ))
            })
        }
        fn reads_inline(&self) -> bool {
            true
        }
    }
    assert!(matches!(
        read_outcome_blocking(&mut Huge, ReadKind::Full),
        ReadOutcome::TooLarge(n) if n == MAX_PASTE_BYTES + 1
    ));
}

fn leaked_cache() -> &'static AvailabilityCache {
    Box::leak(Box::new(AvailabilityCache::new()))
}

#[test]
fn cache_is_unknown_before_the_first_answer() {
    let cache = AvailabilityCache::new();
    assert_eq!(cache.get(ClipboardTarget::Clipboard), Availability::Unknown);
    assert_eq!(cache.get(ClipboardTarget::Primary), Availability::Unknown);
}

#[test]
fn cache_keeps_the_two_clipboards_apart() {
    let cache = AvailabilityCache::new();
    cache.note(ClipboardTarget::Primary, Availability::HasContent);
    assert_eq!(cache.get(ClipboardTarget::Clipboard), Availability::Unknown);
    assert_eq!(
        cache.get(ClipboardTarget::Primary),
        Availability::HasContent
    );
}

#[test]
fn probe_answer_is_stored_unless_the_owner_changed_meanwhile() {
    let cache = AvailabilityCache::new();
    let target = ClipboardTarget::Clipboard;
    let epoch = cache.begin(target).unwrap();
    // Our own write lands while the probe is out: the probe's answer is older and is dropped.
    cache.note(target, Availability::HasContent);
    assert!(!cache.finish(target, epoch, Availability::Empty));
    assert_eq!(cache.get(target), Availability::HasContent);
    // A probe that saw no change is stored.
    let epoch = cache.begin(target).unwrap();
    assert!(!cache.finish(target, epoch, Availability::Empty));
    assert_eq!(cache.get(target), Availability::Empty);
}

#[test]
fn an_event_during_a_probe_coalesces_into_one_rerun() {
    let cache = AvailabilityCache::new();
    let target = ClipboardTarget::Clipboard;
    assert!(
        cache.invalidate(target),
        "idle: the caller starts the probe"
    );
    let epoch = cache.begin(target).unwrap();
    assert!(
        !cache.invalidate(target),
        "running: told to run again, not to start"
    );
    assert!(!cache.invalidate(target));
    assert_eq!(cache.begin(target), None, "one probe at a time");
    // The stale answer is dropped and exactly one rerun is requested.
    assert!(cache.finish(target, epoch, Availability::Empty));
    assert_eq!(cache.get(target), Availability::Unknown);
    let epoch = cache.begin(target).unwrap();
    assert!(!cache.finish(target, epoch, Availability::HasContent));
    assert_eq!(cache.get(target), Availability::HasContent);
}

#[test]
fn refresh_blocking_reruns_for_an_event_that_arrived_during_the_probe() {
    let cache = AvailabilityCache::new();
    let target = ClipboardTarget::Clipboard;
    let calls = std::cell::Cell::new(0);
    cache.refresh_blocking(target, &|_| {
        calls.set(calls.get() + 1);
        if calls.get() == 1 {
            cache.invalidate(target);
            Availability::Empty
        } else {
            Availability::HasContent
        }
    });
    assert_eq!(calls.get(), 2);
    assert_eq!(cache.get(target), Availability::HasContent);
}

#[test]
fn a_panicking_probe_does_not_wedge_later_refreshes() {
    let cache = AvailabilityCache::new();
    let target = ClipboardTarget::Clipboard;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.refresh_blocking(target, &|_| panic!("probe failed"));
    }));
    assert!(result.is_err());
    cache.refresh_blocking(target, &|_| Availability::Empty);
    assert_eq!(cache.get(target), Availability::Empty);
}

/// A clipboard whose availability query blocks until released and whose synchronous query must
/// never be reached: the menu may only read the cache.
struct BlockingProbeClipboard {
    release: std::sync::Arc<(Mutex<bool>, Condvar)>,
    answer: Availability,
}

impl Clipboard for BlockingProbeClipboard {
    fn write(&mut self, _contents: ClipboardContent) {}

    fn reader(&mut self, _kind: ReadKind) -> ClipboardReader {
        unreachable!("the menu never reads")
    }

    fn availability(&mut self, _target: ClipboardTarget) -> Availability {
        panic!("the menu must not query the clipboard on the calling thread");
    }

    fn availability_probe(&mut self) -> Option<AvailabilityProbe> {
        let release = self.release.clone();
        let answer = self.answer;
        Some(std::sync::Arc::new(move |_| {
            let (open, changed) = &*release;
            let mut open = open.lock();
            while !*open {
                changed.wait(&mut open);
            }
            answer
        }))
    }
}

#[test]
fn menu_enabled_state_never_waits_for_a_blocked_owner() {
    let cache = leaked_cache();
    let release = std::sync::Arc::new((Mutex::new(false), Condvar::new()));
    let mut clipboard = BlockingProbeClipboard {
        release: release.clone(),
        answer: Availability::Empty,
    };
    // The probe is stuck on a dead owner; the menu still answers at once, and Unknown is enabled.
    assert!(cached_has_content_in(cache, &mut clipboard));
    assert_eq!(cache.get(ClipboardTarget::Clipboard), Availability::Unknown);
    // Another menu open while the probe is stuck does not start a second query.
    assert!(cached_has_content_in(cache, &mut clipboard));
    // The owner answers: the cache learns it and the next menu open is disabled.
    *release.0.lock() = true;
    release.1.notify_all();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while cache.get(ClipboardTarget::Clipboard) == Availability::Unknown {
        assert!(std::time::Instant::now() < deadline, "probe never landed");
        std::thread::yield_now();
    }
    assert!(!cached_has_content_in(cache, &mut clipboard));
}

#[test]
fn cheap_platforms_answer_the_menu_inline_and_fresh() {
    let cache = leaked_cache();
    let mut clipboard = InMemoryClipboard::default();
    assert!(!cached_has_content_in(cache, &mut clipboard));
    clipboard.write(ClipboardContent::plain_text("x".to_owned()));
    assert!(cached_has_content_in(cache, &mut clipboard));
}
