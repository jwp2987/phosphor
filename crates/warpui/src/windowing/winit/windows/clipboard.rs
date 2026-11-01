use std::ops::Not;

use arboard::{self, Clipboard as WindowsClipboardInner};

use crate::clipboard::{BackgroundTextRead, BackgroundTextReader};
use crate::{clipboard::ClipboardContent, Clipboard};

// Standard clipboard format ids (winuser.h), spelled out so no extra windows-crate feature is needed.
const CF_TEXT: u32 = 1;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

pub struct WindowsClipboard {
    inner: WindowsClipboardInner,
}

impl WindowsClipboard {
    pub fn new() -> Result<Self, arboard::Error> {
        Ok(Self {
            inner: WindowsClipboardInner::new()?,
        })
    }
}

impl Clipboard for WindowsClipboard {
    fn write(&mut self, contents: ClipboardContent) {
        let set_result = if let Some(html) = &contents.html {
            self.inner.set().html(html, Some(&contents.plain_text))
        } else {
            self.inner.set().text(&contents.plain_text)
        };

        if let Err(err) = set_result {
            if contents.html.is_some() {
                log::warn!("Unable to set clipboard HTML: {err:?}");
            } else {
                log::warn!("Unable to set clipboard text: {err:?}");
            }
        }
    }

    fn has_content(&mut self) -> bool {
        // `IsClipboardFormatAvailable` never opens the clipboard, so it cannot block on another
        // process holding it and decodes nothing (#1306). Covers the formats `read` consumes.
        use windows::core::w;
        use windows::Win32::System::DataExchange::{
            IsClipboardFormatAvailable, RegisterClipboardFormatW,
        };
        let available =
            |format: u32| format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok();
        [CF_UNICODETEXT, CF_TEXT, CF_HDROP, CF_DIB, CF_DIBV5]
            .into_iter()
            .any(available)
            || available(unsafe { RegisterClipboardFormatW(w!("HTML Format")) })
            || available(unsafe { RegisterClipboardFormatW(w!("PNG")) })
    }

    fn read_text_and_paths(&mut self) -> ClipboardContent {
        // Text (with the HTML-derived fallback `read` applies) and copied paths only: no image
        // read or decode, and the HTML is dropped as `read_text_and_paths` promises.
        let mut content = self.read_text_html_and_paths(false);
        content.html = None;
        content
    }

    fn background_text_reader(&mut self) -> Option<BackgroundTextReader> {
        // A fresh handle on the worker: the Windows clipboard is process-global and arboard opens
        // it per call, so a read blocked on another process holding it stalls only the worker
        // (#1305).
        Some(Box::new(|| match WindowsClipboard::new() {
            Ok(mut clipboard) => BackgroundTextRead::Content(clipboard.read_text_and_paths()),
            Err(_) => BackgroundTextRead::Content(ClipboardContent::default()),
        }))
    }

    fn background_full_reader(&mut self) -> Option<BackgroundTextReader> {
        Some(Box::new(|| match WindowsClipboard::new() {
            Ok(mut clipboard) => BackgroundTextRead::Content(clipboard.read()),
            Err(_) => BackgroundTextRead::Content(ClipboardContent::default()),
        }))
    }

    fn read(&mut self) -> ClipboardContent {
        let mut content = self.read_text_html_and_paths(true);

        // Try to get image content from clipboard
        content.images = crate::clipboard_utils::read_images_from_clipboard(
            &mut self.inner,
            &content.html,
            &content.plain_text,
        );

        content
    }
}

impl WindowsClipboard {
    /// `always_html` false skips the HTML read unless the text is blank (the only case that uses it).
    fn read_text_html_and_paths(&mut self, always_html: bool) -> ClipboardContent {
        let mut content = ClipboardContent {
            plain_text: self.inner.get().text().unwrap_or_default(),
            ..Default::default()
        };

        // Try to get HTML content
        if always_html || content.plain_text.trim().is_empty() {
            if let Ok(html) = self.inner.get().html() {
                content.html = html.is_empty().not().then_some(html);
            }
        }

        // Some environments provide HTML but do not provide a plaintext representation.
        // If that happens, derive a best-effort plaintext fallback from the HTML.
        if content.plain_text.trim().is_empty() {
            if let Some(html) = content.html.as_ref() {
                let derived = crate::clipboard_utils::strip_html_to_plain_text(html);
                if !derived.trim().is_empty() {
                    content.plain_text = derived;
                }
            }
        }

        // Get file paths.
        content.paths = self.inner.get().file_list().ok().map(|list| {
            list.into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        });

        content
    }
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
