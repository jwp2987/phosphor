//! Memory-behavior repro for APP-4154 batch 1.C (warpui-platform-nsstring).
//!
//! Exercises the `NSString::alloc(nil).init_str(...)` → `make_nsstring(...)` conversions
//! applied to `pasteboard_type_for_image_mime_type` and related clipboard
//! helpers. The helper is shared by every retained-NSString site in this file
//! (6 in `read_image_data_from_pasteboard`, 2 in `Clipboard::write`, and this
//! one in `pasteboard_type_for_image_mime_type`), so it is representative for
//! the whole file.
//!
//! On master the raw `NSString::alloc(nil).init_str(...)` returns a string with
//! a +1 retain count that no pool drain can clean up, so even inside an outer
//! `NSAutoreleasePool` the string survives past `pool.drain()` and memory grows
//! linearly with iteration count.
//!
//! On the PR branch `make_nsstring` autoreleases, so every string returned here
//! is released when the surrounding pool drains and peak RSS stays flat across
//! outer iterations.
//!
//! Run as:
//!   cargo test --release -p warpui \
//!       pasteboard_type_for_image_mime_type_memory_behavior -- --nocapture --ignored
//! and measure peak RSS with `/usr/bin/time -l`.
use cocoa::base::nil;
use cocoa::foundation::NSAutoreleasePool;

use super::pasteboard_type_for_image_mime_type;

/// Number of outer pool cycles. Each cycle creates an `NSAutoreleasePool`,
/// runs the inner loop, then drains. On master the retained NSStrings survive
/// the drain, so memory usage grows proportionally to OUTER * INNER.
const OUTER: usize = 60;

/// Number of inner iterations per pool cycle. Must be large enough to produce
/// a measurable RSS delta but small enough to fit easily in memory on the
/// branch side (where strings are reclaimed per cycle).
const INNER: usize = 20_000;

const MIME_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/svg+xml",
];

#[test]
#[ignore = "memory repro; run with --ignored --nocapture in release mode"]
fn pasteboard_type_for_image_mime_type_memory_behavior() {
    unsafe {
        for _ in 0..OUTER {
            let pool = NSAutoreleasePool::new(nil);
            for _ in 0..INNER {
                for mime in MIME_TYPES {
                    let ns = pasteboard_type_for_image_mime_type(mime);
                    assert!(ns.is_some(), "mime {mime} should be mapped");
                }
            }
            pool.drain();
        }
    }
}

// #1782 (owner decision 2026-11-05): every NSPasteboard read runs on the main thread.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use warpui_core::clipboard::{ClipboardContent, ImageData, ReadKind, ReadOutcome};

use super::{finish_read, on_main_thread, read_on_main_thread_with};

static READS_OFF_MAIN_FROM_WORKER: AtomicUsize = AtomicUsize::new(0);

fn read_recording_off_main(_kind: ReadKind) -> Result<ClipboardContent, ReadOutcome> {
    if !on_main_thread() {
        READS_OFF_MAIN_FROM_WORKER.fetch_add(1, Ordering::SeqCst);
    }
    Ok(ClipboardContent::plain_text(
        "from the main thread".to_string(),
    ))
}

#[test]
fn a_read_requested_off_the_main_thread_never_touches_the_pasteboard_there() {
    // A worker asks: the read is queued for the main thread, not run on the worker. The test
    // harness's main thread does not run the main dispatch queue, so it never answers and the
    // bounded wait gives up instead of deadlocking.
    let result = std::thread::spawn(|| {
        assert!(!on_main_thread());
        read_on_main_thread_with(
            ReadKind::TextAndPaths,
            on_main_thread,
            read_recording_off_main,
            Duration::from_millis(100),
        )
    })
    .join()
    .unwrap();
    assert!(matches!(result, Err(ReadOutcome::TimedOut)), "{result:?}");
    assert_eq!(READS_OFF_MAIN_FROM_WORKER.load(Ordering::SeqCst), 0);
}

fn read_inline(kind: ReadKind) -> Result<ClipboardContent, ReadOutcome> {
    Ok(ClipboardContent::plain_text(format!(
        "read inline {kind:?}"
    )))
}

#[test]
fn a_read_requested_on_the_main_thread_runs_at_once() {
    let result = read_on_main_thread_with(
        ReadKind::Full,
        || true,
        read_inline,
        Duration::from_millis(1),
    );
    match result {
        Ok(content) => assert_eq!(content.plain_text, "read inline Full"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn image_filenames_are_parsed_from_the_copied_html_off_the_main_thread() {
    let html = r#"<img src="/Users/me/Pictures/photo.png">"#;
    let mut content = ClipboardContent::plain_text(String::new());
    content.html = Some(html.to_string());
    content.images = Some(vec![ImageData {
        data: vec![1, 2, 3],
        mime_type: "image/png".to_string(),
        filename: None,
    }]);
    let content = finish_read(content);
    let expected = crate::clipboard_utils::extract_filename_from_html(html);
    assert!(expected.is_some());
    assert_eq!(content.images.unwrap()[0].filename, expected);
}
