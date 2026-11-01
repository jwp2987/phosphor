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
