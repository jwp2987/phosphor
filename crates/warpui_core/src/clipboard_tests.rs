use super::*;

#[test]
fn default_probes_follow_read() {
    let mut clipboard = InMemoryClipboard::default();
    assert!(!clipboard.has_content());
    assert_eq!(clipboard.read_plain_text(), "");

    clipboard.write(ClipboardContent {
        plain_text: "hello".to_owned(),
        images: Some(vec![ImageData {
            data: vec![1, 2, 3],
            mime_type: "image/png".to_owned(),
            filename: None,
        }]),
        ..Default::default()
    });
    assert!(clipboard.has_content());
    assert_eq!(clipboard.read_plain_text(), "hello");
    let without_images = clipboard.read_without_images();
    assert_eq!(without_images.plain_text, "hello");
    assert!(without_images.images.is_none());
    // The stored clipboard still has its image.
    assert!(clipboard.read().has_image_data());
}

#[test]
fn image_only_clipboard_has_content() {
    let mut clipboard = InMemoryClipboard::default();
    clipboard.write(ClipboardContent {
        images: Some(vec![ImageData {
            data: vec![1],
            mime_type: "image/png".to_owned(),
            filename: None,
        }]),
        ..Default::default()
    });
    assert!(clipboard.has_content());
    assert!(clipboard.read_without_images().images.is_none());
}

/// A clipboard whose `read` must never run: `has_content` answers from a cheap probe (#1306).
struct ProbeOnlyClipboard;

impl Clipboard for ProbeOnlyClipboard {
    fn write(&mut self, _contents: ClipboardContent) {}
    fn read(&mut self) -> ClipboardContent {
        panic!("has_content must not read or decode clipboard content");
    }
    fn has_content(&mut self) -> bool {
        true
    }
    #[cfg(target_family = "wasm")]
    fn save(&mut self, _content: ClipboardContent) {}
}

#[test]
fn has_content_override_does_not_read() {
    assert!(ProbeOnlyClipboard.has_content());
}

#[test]
fn background_readers_default_to_none() {
    // Platforms without a thread-safe read keep the synchronous path (#1305).
    struct Plain;
    impl Clipboard for Plain {
        fn write(&mut self, _contents: ClipboardContent) {}
        fn read(&mut self) -> ClipboardContent {
            ClipboardContent::default()
        }
        #[cfg(target_family = "wasm")]
        fn save(&mut self, _content: ClipboardContent) {}
    }
    assert!(Plain.background_text_reader().is_none());
    assert!(Plain.background_full_reader().is_none());
    assert!(!Plain.has_content());
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
fn in_memory_clipboard_has_no_background_writer() {
    let mut clipboard = InMemoryClipboard::default();
    let back = write_in_background(
        &mut clipboard,
        ClipboardContent::plain_text("x".into()),
        ClipboardTarget::Clipboard,
        |_| {},
    );
    assert_eq!(back.unwrap_err().plain_text, "x");
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

#[cfg(not(target_family = "wasm"))]
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

#[cfg(not(target_family = "wasm"))]
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
            Box::new(move || {
                RAN.fetch_add(1, Ordering::SeqCst);
                let _ = rx.lock().recv();
                BackgroundTextRead::Content(ClipboardContent::default())
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
        Box::new(|| {
            RAN.fetch_add(1, Ordering::SeqCst);
            BackgroundTextRead::Content(ClipboardContent::default())
        }),
    );
    assert!(matches!(third(), BackgroundTextRead::TimedOut));
    assert_eq!(RAN.load(Ordering::SeqCst), 2);
    release_tx.send(()).unwrap();
    release_tx.send(()).unwrap();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(TRACKER.in_flight(), 0);
}

#[test]
fn over_cap_paste_is_refused_not_truncated() {
    let big = ClipboardContent::plain_text("x".repeat(11));
    match cap_paste_size(BackgroundTextRead::Content(big), 10) {
        BackgroundTextRead::TooLarge(n) => assert_eq!(n, 11),
        _ => panic!("expected TooLarge"),
    }
    // Exactly at the cap is admitted whole, as are paths and html counted together.
    match cap_paste_size(
        BackgroundTextRead::Content(ClipboardContent::plain_text("x".repeat(10))),
        10,
    ) {
        BackgroundTextRead::Content(c) => assert_eq!(c.plain_text.len(), 10),
        _ => panic!("expected Content"),
    }
    let mixed = ClipboardContent {
        plain_text: "aaaaaa".into(),
        html: Some("bbbbbb".into()),
        ..Default::default()
    };
    assert!(matches!(
        cap_paste_size(BackgroundTextRead::Content(mixed), 10),
        BackgroundTextRead::TooLarge(12)
    ));
    assert!(matches!(
        cap_paste_size(BackgroundTextRead::TimedOut, 10),
        BackgroundTextRead::TimedOut
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

    fn read(&mut self) -> ClipboardContent {
        // A synchronous read on the UI thread is exactly what #1380 removes.
        assert_ne!(
            std::thread::current().id(),
            self.main_thread,
            "the read must not run on the calling thread"
        );
        ClipboardContent::plain_text(self.text.clone())
    }

    fn background_full_reader(&mut self) -> Option<BackgroundTextReader> {
        let text = self.text.clone();
        let main_thread = self.main_thread;
        Some(Box::new(move || {
            assert_ne!(std::thread::current().id(), main_thread);
            BackgroundTextRead::Content(ClipboardContent::plain_text(text))
        }))
    }

    fn background_text_reader(&mut self) -> Option<BackgroundTextReader> {
        self.background_full_reader()
    }

    #[cfg(target_family = "wasm")]
    fn save(&mut self, _content: ClipboardContent) {}
}

#[test]
fn read_in_background_runs_the_reader_off_the_calling_thread() {
    let mut clipboard = WorkerClipboard {
        text: "from the worker".to_owned(),
        main_thread: std::thread::current().id(),
    };
    for kind in [ReadKind::Full, ReadKind::TextAndPaths] {
        match read_in_background(&mut clipboard, kind) {
            StartedRead::Pending(pending) => match futures::executor::block_on(pending) {
                BackgroundTextRead::Content(content) => {
                    assert_eq!(content.plain_text, "from the worker")
                }
                _ => panic!("expected content"),
            },
            StartedRead::Ready(_) => panic!("a platform with a worker reader must not read here"),
        }
    }
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
