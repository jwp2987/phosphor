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

// ---- #1778: tests for the clipboard mutants cargo-mutants found surviving ----
//
// None of these touches the real system clipboard: they use `InMemoryClipboard`, a stub, a local
// `PendingWrites` / `WriteOrder`, or pure functions. The few that use the process-global
// `PENDING_WRITES` / `AVAILABILITY` statics use the same target in ONE test each (and retry where
// another test's write could land between two calls), so they need no serialization.

/// Runs `f` on a fresh thread and returns its result, or `None` after `limit`: a test that waits
/// on a gate fails instead of hanging.
fn within<T: Send + 'static>(
    limit: std::time::Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(limit).ok()
}

const LIMIT: std::time::Duration = std::time::Duration::from_secs(2);
const SHORT: std::time::Duration = std::time::Duration::from_millis(200);

/// A clipboard whose reads are off-thread (`reads_inline` is false), with a settable primary
/// selection.
struct WorkerPrimaryClipboard {
    primary: bool,
}

impl Clipboard for WorkerPrimaryClipboard {
    fn write(&mut self, _contents: ClipboardContent) {}
    fn reader(&mut self, kind: ReadKind) -> ClipboardReader {
        Box::new(move || {
            ReadOutcome::Content(ClipboardContent::plain_text(format!("worker {kind:?}")))
        })
    }
    fn has_primary_selection(&self) -> bool {
        self.primary
    }
}

fn legacy_text(reader: Option<BackgroundTextReader>) -> String {
    match reader.expect("a worker clipboard hands out a reader")() {
        BackgroundTextRead::Content(content) => content.plain_text,
        _ => panic!("expected content"),
    }
}

// -- trait defaults and the legacy readers --

#[test]
fn default_primary_write_goes_to_the_clipboard() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write_to_primary_clipboard(ClipboardContent::plain_text("selected".into()));
    assert_eq!(
        read_content_blocking(&mut clipboard, ReadKind::Full).plain_text,
        "selected"
    );
}

#[test]
fn legacy_full_reader_exists_for_worker_clipboards_and_reads_full() {
    let mut clipboard = WorkerPrimaryClipboard { primary: false };
    assert_eq!(
        legacy_text(clipboard.background_full_reader()),
        "worker Full"
    );
    assert_eq!(
        legacy_text(clipboard.background_text_reader()),
        "worker TextAndPaths"
    );
}

#[test]
fn legacy_primary_reader_needs_a_separate_primary_selection() {
    let mut without = WorkerPrimaryClipboard { primary: false };
    assert!(
        without.background_primary_reader().is_none(),
        "no primary selection: no primary reader, even on a worker clipboard"
    );
    let mut with = WorkerPrimaryClipboard { primary: true };
    assert_eq!(
        legacy_text(with.background_primary_reader()),
        "worker Primary"
    );
}

#[test]
fn in_memory_clipboard_has_no_separate_primary_selection() {
    assert!(!InMemoryClipboard::default().has_primary_selection());
    assert!(!WorkerPrimaryClipboard { primary: false }.has_primary_selection());
    assert!(WorkerPrimaryClipboard { primary: true }.has_primary_selection());
}

#[test]
fn paste_cap_is_64_mib() {
    assert_eq!(MAX_PASTE_BYTES, 64 * 1024 * 1024);
    assert_eq!(MAX_PASTE_BYTES, 67_108_864);
}

#[test]
fn read_full_blocking_returns_everything_on_the_clipboard() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write(ClipboardContent {
        plain_text: "t".into(),
        html: Some("<i>t</i>".into()),
        paths: Some(vec!["/a/b".into()]),
        images: Some(vec![image(vec![9])]),
    });
    let full = read_full_blocking(&mut clipboard);
    assert_eq!(full.plain_text, "t");
    assert_eq!(full.html.as_deref(), Some("<i>t</i>"));
    assert_eq!(full.paths, Some(vec!["/a/b".to_owned()]));
    assert!(full.has_image_data());
}

// -- the abandon epoch --

#[test]
fn abandon_pending_reads_bumps_the_global_epoch() {
    let before = PASTE_READERS
        .abandon_epoch
        .load(std::sync::atomic::Ordering::SeqCst);
    abandon_pending_reads();
    let after = PASTE_READERS
        .abandon_epoch
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        after > before,
        "a paste that gave up must signal the readers it abandoned"
    );
}

// -- write ordering --

#[test]
fn generations_start_at_one_and_count_up() {
    let order = WriteOrder::new();
    assert_eq!(order.next_generation(), 1);
    assert_eq!(order.next_generation(), 2);
    assert_eq!(order.next_generation(), 3);
}

#[test]
fn a_write_of_the_same_generation_as_the_applied_one_still_runs() {
    let order = WriteOrder::new();
    let g = order.next_generation();
    assert_eq!(
        order.run(g, ClipboardTarget::Clipboard, || true),
        WriteOutcome::Written
    );
    // Equal is not "later has been applied": only a strictly newer generation supersedes.
    assert_eq!(
        order.run(g, ClipboardTarget::Clipboard, || true),
        WriteOutcome::Written
    );
    assert_eq!(
        order.run(g - 1, ClipboardTarget::Clipboard, || true),
        WriteOutcome::Superseded
    );
}

// -- PendingWrites::wait_until_idle --

fn pending_with(targets: &[ClipboardTarget]) -> PendingWrites {
    let pending = PendingWrites::new();
    for (i, target) in targets.iter().enumerate() {
        pending.add(
            i as u64 + 1,
            *target,
            std::sync::Arc::new(ClipboardContent::plain_text("w".into())),
        );
    }
    pending
}

#[test]
fn idle_wait_returns_true_at_once_when_nothing_is_pending() {
    let pending = PendingWrites::new();
    let start = std::time::Instant::now();
    assert!(pending.wait_until_idle(ClipboardTarget::Clipboard, LIMIT));
    assert!(start.elapsed() < SHORT, "nothing pending: no waiting");
}

#[test]
fn idle_wait_ignores_writes_to_the_other_target() {
    let pending = pending_with(&[ClipboardTarget::Primary]);
    let start = std::time::Instant::now();
    assert!(pending.wait_until_idle(ClipboardTarget::Clipboard, LIMIT));
    assert!(
        start.elapsed() < SHORT,
        "a pending primary write is not a reason to hold up the clipboard"
    );
}

#[test]
fn idle_wait_gives_up_false_when_the_write_never_lands() {
    let pending = pending_with(&[ClipboardTarget::Clipboard, ClipboardTarget::Primary]);
    let start = std::time::Instant::now();
    assert!(!pending.wait_until_idle(ClipboardTarget::Clipboard, SHORT));
    assert!(
        start.elapsed() >= SHORT / 2,
        "it must wait for the budget, not return early"
    );
    assert!(!pending.wait_until_idle(ClipboardTarget::Primary, SHORT));
}

#[test]
fn idle_wait_gives_up_true_when_only_the_other_target_stays_pending_after_the_timeout() {
    // The write to the awaited target lands, the other one never does: the wait ends true.
    let pending = std::sync::Arc::new(pending_with(&[
        ClipboardTarget::Clipboard,
        ClipboardTarget::Primary,
    ]));
    let p = pending.clone();
    let landed = within(LIMIT, move || {
        let finisher = {
            let p = p.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                p.finish(1);
            })
        };
        let idle = p.wait_until_idle(
            ClipboardTarget::Clipboard,
            std::time::Duration::from_secs(1),
        );
        finisher.join().unwrap();
        idle
    });
    assert_eq!(landed, Some(true));
    assert!(pending.latest(ClipboardTarget::Primary).is_some());
}

#[test]
fn idle_wait_is_woken_by_the_write_finishing_before_the_budget() {
    let pending = std::sync::Arc::new(pending_with(&[ClipboardTarget::Clipboard]));
    let p = pending.clone();
    let result = within(std::time::Duration::from_secs(4), move || {
        let finisher = {
            let p = p.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(100));
                p.finish(1);
            })
        };
        let start = std::time::Instant::now();
        let idle = p.wait_until_idle(
            ClipboardTarget::Clipboard,
            std::time::Duration::from_secs(3),
        );
        finisher.join().unwrap();
        (idle, start.elapsed())
    });
    let (idle, elapsed) = result.expect("the wait must not hang");
    assert!(idle, "the last pending write landed within the budget");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "woken by finish, not by the timeout: {elapsed:?}"
    );
}

// -- the process-global pending writes (Clipboard target; one test owns them) --

struct FinishOnDrop(u64);

impl Drop for FinishOnDrop {
    fn drop(&mut self) {
        PENDING_WRITES.finish(self.0);
    }
}

#[test]
fn global_pending_write_is_seen_by_reads_and_waited_for() {
    let generation = u64::MAX - 7;
    let mut full = ClipboardContent::plain_text("pending text".into());
    full.html = Some("<b>pending</b>".into());
    full.images = Some(vec![image(vec![1])]);
    PENDING_WRITES.add(
        generation,
        ClipboardTarget::Clipboard,
        std::sync::Arc::new(full),
    );
    let guard = FinishOnDrop(generation);

    // pending_write returns the newest unfinished write of the target.
    let seen = pending_write(ClipboardTarget::Clipboard).expect("a write is pending");
    assert_eq!(seen.plain_text, "pending text");
    assert!(pending_write(ClipboardTarget::Primary).is_none());

    // The quit-time wait: nothing pending for Primary is idle; a stuck Clipboard write is not.
    assert!(wait_for_pending_writes(ClipboardTarget::Primary, SHORT));
    assert!(!wait_for_pending_writes(ClipboardTarget::Clipboard, SHORT));

    // A read right after the copy answers from the pending write; a paste-style read drops the
    // html and images, a full read keeps them.
    let mut clipboard = InMemoryClipboard::default();
    let text = read_content_blocking(&mut clipboard, ReadKind::TextAndPaths);
    assert_eq!(text.plain_text, "pending text");
    assert!(text.html.is_none() && text.images.is_none());
    let full = read_content_blocking(&mut clipboard, ReadKind::Full);
    assert_eq!(full.plain_text, "pending text");
    assert_eq!(full.html.as_deref(), Some("<b>pending</b>"));
    assert!(full.has_image_data());

    drop(guard);
    assert!(pending_write(ClipboardTarget::Clipboard).is_none());
    assert!(wait_for_pending_writes(ClipboardTarget::Clipboard, SHORT));
}

// -- the availability cache (global statics) --

/// Retries `check` a few times: the statics are process-wide, so another test's write in the same
/// process (plain `cargo test`) could land between our two calls. A mutated function fails every
/// attempt.
fn eventually(mut check: impl FnMut() -> bool) -> bool {
    (0..50).any(|_| check())
}

#[test]
fn menu_entry_follows_what_the_clipboard_holds() {
    let mut clipboard = InMemoryClipboard::default();
    assert!(
        eventually(|| !cached_has_content(&mut clipboard)),
        "an empty clipboard disables Paste"
    );
    clipboard.write(ClipboardContent::plain_text("x".into()));
    assert!(
        eventually(|| cached_has_content(&mut clipboard)),
        "a clipboard with text enables Paste"
    );
}

#[test]
fn availability_events_and_own_writes_update_the_cache_at_once() {
    // Primary only, and in one test, so nothing else touches this slot.
    let target = ClipboardTarget::Primary;
    availability_changed(target, &|_| Availability::HasContent);
    assert_eq!(AVAILABILITY.get(target), Availability::HasContent);
    availability_changed(target, &|_| Availability::Empty);
    assert_eq!(AVAILABILITY.get(target), Availability::Empty);

    note_own_write(target, &ClipboardContent::plain_text("x".into()));
    assert_eq!(AVAILABILITY.get(target), Availability::HasContent);
    note_own_write(target, &ClipboardContent::default());
    assert_eq!(AVAILABILITY.get(target), Availability::Empty);
    // Any flavor counts as content, not only text.
    note_own_write(
        target,
        &ClipboardContent {
            images: Some(vec![image(vec![1])]),
            ..Default::default()
        },
    );
    assert_eq!(AVAILABILITY.get(target), Availability::HasContent);
}

// -- ClipboardContent --

#[test]
fn text_bytes_sums_text_html_and_every_path() {
    let content = ClipboardContent {
        plain_text: "ab".into(),
        html: Some("cde".into()),
        paths: Some(vec!["f".into(), "gh".into()]),
        images: Some(vec![image(vec![0; 100])]),
    };
    // 2 + 3 + (1 + 2); image bytes are not text.
    assert_eq!(content.text_bytes(), 8);
    assert_eq!(ClipboardContent::default().text_bytes(), 0);
    assert_eq!(ClipboardContent::plain_text("abc".into()).text_bytes(), 3);
    assert_eq!(
        ClipboardContent {
            html: Some("xy".into()),
            ..Default::default()
        }
        .text_bytes(),
        2
    );
    assert_eq!(
        ClipboardContent {
            paths: Some(vec!["abcd".into()]),
            ..Default::default()
        }
        .text_bytes(),
        4
    );
}

#[test]
fn has_image_data_needs_at_least_one_image() {
    assert!(!ClipboardContent::default().has_image_data());
    assert!(
        !ClipboardContent {
            images: Some(vec![]),
            ..Default::default()
        }
        .has_image_data()
    );
    assert!(
        ClipboardContent {
            images: Some(vec![image(vec![1])]),
            ..Default::default()
        }
        .has_image_data()
    );
}

fn with_paths(paths: &[&str]) -> ClipboardContent {
    ClipboardContent {
        paths: Some(paths.iter().map(|p| (*p).to_owned()).collect()),
        ..Default::default()
    }
}

#[test]
fn num_paths_counts_them() {
    assert_eq!(ClipboardContent::default().num_paths(), 0);
    assert_eq!(with_paths(&[]).num_paths(), 0);
    assert_eq!(with_paths(&["/a"]).num_paths(), 1);
    assert_eq!(with_paths(&["/a", "/b.png", "/c"]).num_paths(), 3);
}

#[test]
fn has_non_image_filepaths_looks_at_every_path() {
    assert!(!ClipboardContent::default().has_non_image_filepaths());
    assert!(!with_paths(&[]).has_non_image_filepaths());
    assert!(!with_paths(&["/a/pic.png"]).has_non_image_filepaths());
    assert!(!with_paths(&["/a/pic.png", "/b/PHOTO.JPG"]).has_non_image_filepaths());
    assert!(with_paths(&["/a/notes.txt"]).has_non_image_filepaths());
    // Mixed: one non-image among images is enough, wherever it is.
    assert!(with_paths(&["/a/pic.png", "/b/notes.txt"]).has_non_image_filepaths());
    assert!(with_paths(&["/b/notes.txt", "/a/pic.png"]).has_non_image_filepaths());
}

// -- should_insert_text_on_paste: every operand --

#[test]
fn should_insert_text_on_paste_truth_table() {
    let img = || Some(vec![image(vec![1])]);
    let paths = |p: &[&str]| Some(p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    let case = |images: Option<Vec<ImageData>>, paths: Option<Vec<String>>| ClipboardContent {
        plain_text: "t".into(),
        images,
        paths,
        html: None,
    };
    // (content, expected, why)
    let table = [
        (case(None, None), true, "text only"),
        (
            case(Some(vec![]), paths(&[])),
            true,
            "empty lists are no images",
        ),
        (case(img(), None), true, "direct image paste keeps its text"),
        (case(img(), paths(&[])), true, "image data, empty path list"),
        (case(None, paths(&["/a/p.png"])), false, "image file only"),
        (
            case(img(), paths(&["/a/p.png"])),
            false,
            "image data plus image file",
        ),
        (case(None, paths(&["/a/n.txt"])), true, "non-image file"),
        (
            case(img(), paths(&["/a/n.txt"])),
            true,
            "image data plus non-image file",
        ),
        (
            case(None, paths(&["/a/p.png", "/a/n.txt"])),
            true,
            "mixed files",
        ),
        (
            case(img(), paths(&["/a/p.png", "/a/n.txt"])),
            true,
            "mixed files and image data",
        ),
    ];
    for (content, expected, why) in table {
        assert_eq!(should_insert_text_on_paste(&content), expected, "{why}");
    }
}

// -- InMemoryClipboard::reads_inline and the deferred-read test support --

#[test]
fn in_memory_reads_inline_unless_deferred() {
    let clipboard = InMemoryClipboard::default();
    assert!(clipboard.reads_inline());
    let deferral = deferred_reads::defer();
    assert!(!clipboard.reads_inline(), "a deferred read is off-thread");
    deferral.release();
    assert!(clipboard.reads_inline(), "released: inline again");
    let deferral = deferred_reads::defer();
    assert!(!clipboard.reads_inline());
    drop(deferral);
    assert!(clipboard.reads_inline(), "dropped: inline again");
}

#[test]
fn deferral_is_active_until_released() {
    assert!(deferred_reads::active().is_none());
    let deferral = deferred_reads::defer();
    assert!(deferred_reads::active().is_some());
    deferral.release();
    assert!(deferred_reads::active().is_none());
    drop(deferral);

    let deferral = deferred_reads::defer();
    assert!(deferred_reads::active().is_some());
    drop(deferral);
    assert!(
        deferred_reads::active().is_none(),
        "dropping the deferral restores inline reads"
    );
}

/// Waits on `gate` from another thread; returns whether it finished within `limit`.
fn gate_opens_within(gate: deferred_reads::ReadGate, limit: std::time::Duration) -> bool {
    within(limit, move || gate.wait()).is_some()
}

#[test]
fn read_gate_blocks_until_the_deferral_is_released() {
    let deferral = deferred_reads::defer();
    let gate = deferred_reads::active().unwrap();
    assert!(
        !gate_opens_within(gate, SHORT),
        "the gate must hold a read while the deferral is live"
    );
    let waiting = deferred_reads::active().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        waiting.wait();
        let _ = tx.send(());
    });
    assert!(rx.recv_timeout(SHORT).is_err(), "still held");
    deferral.release();
    assert!(
        rx.recv_timeout(LIMIT).is_ok(),
        "release opens the gate and wakes the waiter"
    );
}

#[test]
fn read_gate_opens_when_the_deferral_is_dropped() {
    let deferral = deferred_reads::defer();
    let (tx, rx) = std::sync::mpsc::channel();
    let waiting = deferred_reads::active().unwrap();
    std::thread::spawn(move || {
        waiting.wait();
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(SHORT).is_err(),
        "held while the deferral lives"
    );
    drop(deferral);
    assert!(rx.recv_timeout(LIMIT).is_ok(), "drop opens the gate");
}

#[test]
fn read_gate_already_open_does_not_block() {
    let deferral = deferred_reads::defer();
    let gate = deferred_reads::active().unwrap();
    deferral.release();
    assert!(
        gate_opens_within(gate, LIMIT),
        "a released gate lets a late waiter straight through"
    );
}

#[test]
fn deferred_in_memory_read_answers_only_after_release() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write(ClipboardContent::plain_text("typed first".into()));
    let deferral = deferred_reads::defer();
    let request = request_read(&mut clipboard, ReadKind::TextAndPaths);
    let ReadStart::Pending(pending) = request.start else {
        panic!("a deferred read is pending, not an inline snapshot");
    };
    // The snapshot was taken at request time; the answer waits for the release.
    clipboard.write(ClipboardContent::plain_text("changed later".into()));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(futures::executor::block_on(pending));
    });
    assert!(rx.recv_timeout(SHORT).is_err(), "no answer before release");
    drop(deferral);
    match rx.recv_timeout(LIMIT).expect("answer after release") {
        ReadOutcome::Content(content) => assert_eq!(content.plain_text, "typed first"),
        other => panic!("expected content, got {other:?}"),
    }
}
