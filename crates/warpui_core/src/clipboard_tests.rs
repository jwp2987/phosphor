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
