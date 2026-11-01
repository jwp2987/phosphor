use std::ops::Not;

use arboard::{self, Clipboard as WindowsClipboardInner};

use crate::{
    clipboard::{BackgroundClipboardWriter, ClipboardContent},
    Clipboard,
};

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

    fn background_writer(&mut self) -> Option<BackgroundClipboardWriter> {
        // A Win32 clipboard owner/viewer can stall `OpenClipboard`; arboard opens it per call, so a
        // fresh handle on the writer thread is equivalent (#1307).
        Some(std::sync::Arc::new(|contents, _target| {
            let Ok(mut clipboard) = WindowsClipboard::new() else {
                log::warn!("Unable to open the clipboard for a background write");
                return false;
            };
            let result = if let Some(html) = &contents.html {
                clipboard.inner.set().html(html, Some(&contents.plain_text))
            } else {
                clipboard.inner.set().text(&contents.plain_text)
            };
            result
                .map_err(|err| log::warn!("Unable to set clipboard text: {err:?}"))
                .is_ok()
        }))
    }

    fn read(&mut self) -> ClipboardContent {
        let mut content = ClipboardContent {
            plain_text: self.inner.get().text().unwrap_or_default(),
            ..Default::default()
        };

        // Try to get HTML content
        if let Ok(html) = self.inner.get().html() {
            content.html = html.is_empty().not().then_some(html);
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

        // Try to get image content from clipboard
        content.images = crate::clipboard_utils::read_images_from_clipboard(
            &mut self.inner,
            &content.html,
            &content.plain_text,
        );

        content
    }
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
