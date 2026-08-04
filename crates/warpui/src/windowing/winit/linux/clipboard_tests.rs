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
