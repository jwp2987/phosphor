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
