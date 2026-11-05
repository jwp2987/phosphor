/// Linux-specific clipboard tests.
///
/// Note: Most image processing functionality is tested in ui/src/clipboard_utils_tests.rs
/// to avoid duplication. These tests focus on Linux-specific clipboard behavior.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod clipboard_tests {
    use std::sync::Mutex;

    use crate::clipboard::{Clipboard, ClipboardContent};
    use crate::windowing::winit::linux::LinuxClipboard;

    /// The tests below share the one process-global system clipboard. Serialize
    /// them so parallel execution can't clobber one test's clipboard contents
    /// between another test's write and read.
    static CLIPBOARD_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn create_test_clipboard() -> Option<LinuxClipboard> {
        LinuxClipboard::new().ok()
    }

    /// Helper function to avoid repetitive clipboard creation and early return logic.
    fn with_test_clipboard<F>(test_fn: F)
    where
        F: FnOnce(&mut LinuxClipboard),
    {
        let _guard = CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut clipboard = match create_test_clipboard() {
            Some(clipboard) => clipboard,
            None => {
                eprintln!("Skipping test - no clipboard available (headless environment)");
                return;
            }
        };
        test_fn(&mut clipboard);
    }

    /// Helper to assert that paths are correctly extracted from clipboard text.
    ///
    /// Exercises `parse_valid_filepaths_from_text` directly rather than
    /// round-tripping through the real system clipboard: the round trip is
    /// nondeterministic (other processes and parallel tests share the one OS
    /// clipboard) and would mutate the user's live clipboard. The parser is the
    /// actual unit under test, and it requires the paths to exist on disk, so
    /// callers pass real (temp-file) paths.
    fn assert_paths_extracted(
        clipboard: &mut LinuxClipboard,
        input: &str,
        expected_paths: &[&str],
    ) {
        match clipboard.parse_valid_filepaths_from_text(input) {
            Some(paths) => {
                let expected: Vec<String> =
                    expected_paths.iter().map(|p| (*p).to_string()).collect();
                assert_eq!(paths, expected, "parsed paths from '{input}'");
            }
            None => panic!("Expected to extract paths from: '{input}'"),
        }
    }

    /// Helper to assert that no paths are extracted from clipboard text.
    fn assert_no_paths_extracted(clipboard: &mut LinuxClipboard, input: &str) {
        assert!(
            clipboard.parse_valid_filepaths_from_text(input).is_none(),
            "Expected no paths to be extracted from: '{input}'",
        );
    }

    #[test]
    fn test_clipboard_round_trip() {
        with_test_clipboard(|clipboard| {
            let test_content = ClipboardContent::plain_text("Linux clipboard test".to_string());

            // Write content
            clipboard.write(test_content.clone());

            // Read it back
            let read_content = clipboard.read();

            // Should get the same text back (in environments where clipboard works)
            if !read_content.plain_text.is_empty() {
                assert_eq!(read_content.plain_text, test_content.plain_text);
            }
        });
    }

    #[test]
    fn test_html_content_handling() {
        with_test_clipboard(|clipboard| {
            let test_content = ClipboardContent {
                plain_text: "Test text".to_string(),
                html: Some("<div>Test HTML</div>".to_string()),
                images: None,
                paths: None,
            };

            // Write HTML content
            clipboard.write(test_content.clone());

            // Read it back
            let read_content = clipboard.read();

            // In environments where clipboard works, we should get content back
            // (the exact HTML may not be preserved depending on the system)
            if !read_content.is_empty() {
                assert!(!read_content.plain_text.is_empty());
            }
        });
    }

    #[test]
    fn test_primary_clipboard_operations() {
        with_test_clipboard(|clipboard| {
            let test_content = ClipboardContent::plain_text("Primary clipboard test".to_string());

            // Test primary clipboard write (should not panic)
            clipboard.write_to_primary_clipboard(test_content.clone());

            // Test primary clipboard read (should return valid ClipboardContent)
            let read_content = clipboard.read_from_primary_clipboard();

            // Should always return a ClipboardContent struct, even if empty
            // (this tests the fallback behavior when primary clipboard isn't supported)
            assert!(matches!(read_content.images, None | Some(_)));
            assert!(matches!(read_content.html, None | Some(_)));
        });
    }

    #[test]
    fn test_empty_content_handling() {
        with_test_clipboard(|clipboard| {
            let empty_content = ClipboardContent::plain_text("".to_string());

            // Writing empty content should not panic
            clipboard.write(empty_content);

            // Reading should return valid ClipboardContent (may be empty or have previous content)
            let read_content = clipboard.read();

            // Should always return a valid ClipboardContent struct
            assert!(matches!(read_content.images, None | Some(_)));
        });
    }

    #[test]
    fn test_absolute_paths_extracted() {
        with_test_clipboard(|clipboard| {
            // The parser only accepts absolute paths that exist on disk, so use
            // real temp files rather than fabricated /home/user/... paths.
            let dir = tempfile::tempdir().unwrap();

            // Single path
            let doc = dir.path().join("document.txt");
            std::fs::write(&doc, "").unwrap();
            let doc = doc.to_str().unwrap();
            assert_paths_extracted(clipboard, doc, &[doc]);

            // Multiple paths
            let f1 = dir.path().join("file1.txt");
            let f2 = dir.path().join("file2.pdf");
            std::fs::write(&f1, "").unwrap();
            std::fs::write(&f2, "").unwrap();
            let (f1, f2) = (f1.to_str().unwrap(), f2.to_str().unwrap());
            assert_paths_extracted(clipboard, &format!("{f1}\n{f2}"), &[f1, f2]);
        });
    }

    #[test]
    fn test_file_uri_decoded() {
        with_test_clipboard(|clipboard| {
            let dir = tempfile::tempdir().unwrap();

            // Basic file:// URI (real file, so the exists() check passes)
            let doc = dir.path().join("document.txt");
            std::fs::write(&doc, "").unwrap();
            let doc = doc.to_str().unwrap();
            assert_paths_extracted(clipboard, &format!("file://{doc}"), &[doc]);

            // URL-encoded URI with a space in the path
            let sub = dir.path().join("My Documents");
            std::fs::create_dir_all(&sub).unwrap();
            let spaced = sub.join("file.txt");
            std::fs::write(&spaced, "").unwrap();
            let spaced = spaced.to_str().unwrap();
            let encoded = spaced.replace(' ', "%20");
            assert_paths_extracted(clipboard, &format!("file://{encoded}"), &[spaced]);
        });
    }

    #[test]
    fn test_non_absolute_paths_rejected() {
        with_test_clipboard(|clipboard| {
            // Relative paths should be rejected
            assert_no_paths_extracted(clipboard, "./relative.txt\n../another.txt");

            // Regular text should be rejected
            assert_no_paths_extracted(clipboard, "Hello world\nThis is text");

            // Mixed content should be rejected (strict policy)
            assert_no_paths_extracted(
                clipboard,
                "/home/user/file.txt\nSome text\n/another/file.txt",
            );
        });
    }
}

/// The retry/timeout classification behind #1173: a slow `ContentNotAvailable` is retried and,
/// if it persists, reported; a fast one (empty clipboard) is neither.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod read_retry_tests {
    use std::time::Duration;

    use super::super::{TEXT_READ_ATTEMPTS, TEXT_READ_TIMEOUT_FLOOR, read_text_with_retry};

    const SLOW: Duration = Duration::from_millis(10);

    #[test]
    fn slow_failure_is_retried_then_reported() {
        let mut reads = 0;
        let mut pauses = 0;
        let (result, timed_out) = read_text_with_retry(
            || {
                reads += 1;
                (Err(arboard::Error::ContentNotAvailable), SLOW)
            },
            || pauses += 1,
        );
        assert!(result.is_err());
        assert!(timed_out, "persistent slow failure must be reported");
        assert_eq!(reads, TEXT_READ_ATTEMPTS);
        assert_eq!(pauses, TEXT_READ_ATTEMPTS - 1);
    }

    #[test]
    fn slow_failure_then_success_returns_text_without_failure() {
        let mut reads = 0;
        let (result, timed_out) = read_text_with_retry(
            || {
                reads += 1;
                if reads < 3 {
                    (Err(arboard::Error::ContentNotAvailable), SLOW)
                } else {
                    (Ok("pasted".to_string()), SLOW)
                }
            },
            || {},
        );
        assert_eq!(result.unwrap(), "pasted");
        assert!(!timed_out);
        assert_eq!(reads, 3);
    }

    #[test]
    fn fast_failure_is_an_empty_clipboard_not_retried() {
        let mut reads = 0;
        let (result, timed_out) = read_text_with_retry(
            || {
                reads += 1;
                (
                    Err(arboard::Error::ContentNotAvailable),
                    TEXT_READ_TIMEOUT_FLOOR / 2,
                )
            },
            || panic!("must not pause for an empty clipboard"),
        );
        assert!(result.is_err());
        assert!(!timed_out);
        assert_eq!(reads, 1);
    }

    #[test]
    fn other_errors_are_not_retried() {
        let mut reads = 0;
        let (result, timed_out) = read_text_with_retry(
            || {
                reads += 1;
                (Err(arboard::Error::ClipboardNotSupported), SLOW)
            },
            || panic!("must not pause"),
        );
        assert!(result.is_err());
        assert!(!timed_out);
        assert_eq!(reads, 1);
    }
}

mod bounded_wait_tests {
    use super::super::{
        EXIT_HANDOFF_BUDGET, EXIT_PENDING_WRITE_WAIT, flush_for_exit, read_timed_out,
        run_with_budget,
    };
    use std::time::Duration;

    #[test]
    fn spent_budget_with_no_text_is_a_timeout() {
        let budget = Duration::from_secs(2);
        // The retries after the first slow failure return instantly and look like "empty".
        assert!(read_timed_out(false, budget, budget, false));
        assert!(read_timed_out(true, Duration::ZERO, budget, true));
        assert!(!read_timed_out(
            false,
            Duration::from_millis(5),
            budget,
            false
        ));
        assert!(!read_timed_out(false, budget, budget, true));
    }

    #[test]
    fn exit_handoff_runs_even_when_a_write_never_finishes() {
        let mut ran = false;
        // The bounded wait gave up (returned false): the handoff still runs.
        let did = flush_for_exit(|| false, || ran = true);
        assert!(did && ran);
    }

    #[test]
    fn exit_handoff_runs_after_the_wait_reports_the_copy_landed() {
        let mut ran = false;
        flush_for_exit(|| true, || ran = true);
        assert!(ran);
    }

    /// #1692 / #1693: the quit-time waits outlast a copy that takes a few hundred ms to land on a
    /// loaded machine (they were 300 ms / 600 ms), the handoff budget covers the pending wait plus
    /// arboard's 1 s manager handover, and the whole handoff fits inside the 5 s signal shutdown
    /// deadline.
    #[test]
    fn exit_waits_survive_a_loaded_machine_within_the_shutdown_deadline() {
        assert!(EXIT_PENDING_WRITE_WAIT >= Duration::from_secs(2));
        assert!(EXIT_HANDOFF_BUDGET >= EXIT_PENDING_WRITE_WAIT + Duration::from_secs(1));
        assert!(EXIT_HANDOFF_BUDGET < crate::platform::termination_signals::SHUTDOWN_DEADLINE);
    }

    /// #1693: the handoff budget is a give-up, not a pace. Work that finishes at once ends the
    /// wait at once, so a normal quit is no slower for the larger budget.
    #[test]
    fn exit_handoff_wait_ends_when_the_work_does() {
        use std::time::Instant;
        let start = Instant::now();
        assert!(run_with_budget(|| {}, Duration::from_secs(30)));
        assert!(start.elapsed() < Duration::from_secs(5));

        let start = Instant::now();
        assert!(run_with_budget(
            || std::thread::sleep(Duration::from_millis(50)),
            Duration::from_secs(30)
        ));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    /// #1693: a handoff that never finishes (a wedged X connection) is given up at the budget.
    #[test]
    fn exit_handoff_wait_gives_up_at_its_budget() {
        use std::time::Instant;
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let start = Instant::now();
        let finished = run_with_budget(
            move || {
                let _ = release_rx.recv();
            },
            Duration::from_millis(50),
        );
        assert!(!finished);
        assert!(start.elapsed() >= Duration::from_millis(50));
        drop(release_tx);
    }

    #[test]
    fn exit_wait_is_woken_by_the_writer_finishing_not_polled() {
        use crate::clipboard::{ClipboardContent, ClipboardTarget, PendingWrites};
        use std::sync::Arc;
        use std::time::Instant;

        let pending = Arc::new(PendingWrites::new());
        pending.add(
            1,
            ClipboardTarget::Clipboard,
            Arc::new(ClipboardContent::plain_text("x".into())),
        );
        let finisher = {
            let pending = pending.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                pending.finish(1);
            })
        };
        let start = Instant::now();
        // A generous budget: returning well before it proves the notify woke the waiter.
        assert!(pending.wait_until_idle(ClipboardTarget::Clipboard, Duration::from_secs(10)));
        assert!(start.elapsed() < Duration::from_secs(5));
        finisher.join().unwrap();
    }

    #[test]
    fn exit_wait_gives_up_at_its_budget_when_a_copy_never_finishes() {
        use crate::clipboard::{ClipboardContent, ClipboardTarget, PendingWrites};
        use std::sync::Arc;

        let pending = PendingWrites::new();
        pending.add(
            1,
            ClipboardTarget::Clipboard,
            Arc::new(ClipboardContent::plain_text("x".into())),
        );
        assert!(!pending.wait_until_idle(ClipboardTarget::Clipboard, Duration::from_millis(20)));
        // A different target is not blocked by it.
        assert!(pending.wait_until_idle(ClipboardTarget::Primary, Duration::from_millis(20)));
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod targets_query_tests {
    use super::super::has_content_from_formats;

    /// #1306: an image-only clipboard is "non-empty" from its target list alone, so the availability query
    /// answers without reading or decoding the image. Old code had no such path (it decoded).
    #[test]
    fn image_only_targets_have_content_without_a_read() {
        let formats = Ok(arboard::ClipboardFormats::from_target_names([
            "TARGETS",
            "image/png",
        ]));
        assert_eq!(has_content_from_formats(&formats), Some(true));
    }

    #[test]
    fn no_known_targets_means_empty_without_a_read() {
        let formats = Ok(arboard::ClipboardFormats::from_target_names([
            "TARGETS",
            "SAVE_TARGETS",
        ]));
        assert_eq!(has_content_from_formats(&formats), Some(false));
    }

    /// A failed query must not be read as "empty": availability is then unknown (the Paste entry stays
    /// enabled) and nothing is read on the calling thread (#1405).
    #[test]
    fn failed_query_defers_to_the_read_fallback() {
        let formats = Err(arboard::Error::ContentNotAvailable);
        assert_eq!(has_content_from_formats(&formats), None);
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod read_budget_tests {
    use super::super::{READ_BUDGET_ACTIVE, ReadBudget};
    use std::time::Duration;

    fn active() -> bool {
        READ_BUDGET_ACTIVE.with(|active| active.get())
    }

    /// Every synchronous read takes a budget (#1380); a nested one (the fallback probe calling
    /// `read()`) must not replace or clear the outer one, and the cap is released afterwards.
    #[test]
    fn nested_budget_defers_to_the_outer_and_releases_on_drop() {
        assert!(!active());
        let outer = ReadBudget::start(Duration::from_secs(2));
        assert!(outer.owns && active());
        {
            let inner = ReadBudget::start(Duration::from_secs(60));
            assert!(!inner.owns, "an inner budget never loosens the outer one");
        }
        assert!(
            active(),
            "dropping the inner budget leaves the outer cap in place"
        );
        drop(outer);
        assert!(!active());
    }
}
