use std::ops::Not;
use std::time::{Duration, Instant};

use arboard::{
    self, Clipboard as LinuxClipboardInner, GetExtLinux, LinuxClipboardKind, SetExtLinux,
};
use zbus::zvariant::NoneValue;

use crate::{
    Clipboard,
    clipboard::{BackgroundTextRead, BackgroundTextReader, ClipboardContent},
};

/// How many times the text read is attempted when the owner is slow (#1173).
const TEXT_READ_ATTEMPTS: usize = 3;
/// Pause between attempts, so a busy owner can finish serving the previous request.
const TEXT_READ_RETRY_PAUSE: Duration = Duration::from_millis(50);
/// A failed read that returned faster than this was an immediate "no text" answer from the
/// owner (an empty clipboard), not a timeout. arboard's X11 chunk timeout is 10 ms, so a timed-out
/// read always takes at least this long; retrying a quick failure would only delay a paste of
/// nothing.
const TEXT_READ_TIMEOUT_FLOOR: Duration = Duration::from_millis(8);

pub struct LinuxClipboard {
    inner: LinuxClipboardInner,
    /// Set when the last text read kept failing after retries (the owner timed out), as opposed
    /// to the clipboard simply having no text. Taken by `take_read_failure`.
    read_failed: bool,
}

/// Reads text, retrying when the failure looks like a timeout rather than an empty clipboard.
///
/// `read` returns the result and how long the attempt took; `pause` sleeps between attempts.
/// Returns the final result and whether it was a timeout-shaped failure (i.e. every attempt
/// failed slowly with `ContentNotAvailable`, which is what arboard reports for an X11
/// INCR/selection-request timeout).
fn read_text_with_retry(
    mut read: impl FnMut() -> (Result<String, arboard::Error>, Duration),
    mut pause: impl FnMut(),
) -> (Result<String, arboard::Error>, bool) {
    let mut attempt = 1;
    loop {
        let (result, elapsed) = read();
        let timeout_shaped = matches!(result, Err(arboard::Error::ContentNotAvailable))
            && elapsed >= TEXT_READ_TIMEOUT_FLOOR;
        if !timeout_shaped {
            return (result, false);
        }
        if attempt >= TEXT_READ_ATTEMPTS {
            return (result, true);
        }
        log::warn!("Clipboard text read timed out (attempt {attempt}); retrying");
        attempt += 1;
        pause();
    }
}

impl LinuxClipboard {
    /// The one text read every path uses: retries a timed-out read and records in `read_failed`
    /// whether it ultimately timed out (#1173).
    fn read_text_retrying(&mut self) -> Result<String, arboard::Error> {
        let (result, timed_out) = read_text_with_retry(
            || {
                let start = Instant::now();
                let result = self.inner.get().text();
                (result, start.elapsed())
            },
            || std::thread::sleep(TEXT_READ_RETRY_PAUSE),
        );
        self.read_failed = timed_out;
        result
    }

    pub fn new() -> Result<Self, arboard::Error> {
        Ok(Self {
            inner: LinuxClipboardInner::new()?,
            read_failed: false,
        })
    }
}

impl Clipboard for LinuxClipboard {
    fn write(&mut self, contents: ClipboardContent) {
        if let Err(err) = self.write_to_specific_clipboard(LinuxClipboardKind::Clipboard, &contents)
        {
            if contents.html.is_some() {
                log::warn!("Unable to set clipboard HTML: {err:?}");
            } else {
                log::warn!("Unable to set clipboard text: {err:?}");
            }
        }
    }

    fn take_read_failure(&mut self) -> bool {
        std::mem::take(&mut self.read_failed)
    }

    fn read(&mut self) -> ClipboardContent {
        match self.read_from_specific_clipboard(LinuxClipboardKind::Clipboard) {
            Ok(content) => content,
            Err(err) => {
                log::warn!("Failed to read from Linux clipboard: {err:?}");
                ClipboardContent::null_value()
            }
        }
    }

    fn has_content(&mut self) -> bool {
        // Non-empty text is the overwhelmingly common case and answers the question without
        // the HTML, path and image reads (an image decode can take hundreds of ms).
        if self
            .inner
            .get()
            .text()
            .is_ok_and(|text| !text.is_empty())
        {
            return true;
        }
        !self.read().is_empty()
    }

    fn read_without_images(&mut self) -> ClipboardContent {
        match self.read_from_specific_clipboard_inner(LinuxClipboardKind::Clipboard, false) {
            Ok(content) => content,
            Err(err) => {
                log::warn!("Failed to read from Linux clipboard: {err:?}");
                ClipboardContent::null_value()
            }
        }
    }

    fn read_text_and_paths(&mut self) -> ClipboardContent {
        // One text read plus the syntactic path check; the HTML target and image probes are
        // skipped because paste never uses them (a multi-MB paste paid for them on every read).
        let plain_text = match self.read_text_retrying() {
            Ok(text) => text,
            Err(err) => {
                log::warn!("Failed to read from Linux clipboard: {err:?}");
                return ClipboardContent::null_value();
            }
        };
        let paths = self.parse_valid_filepaths_from_text(&plain_text);
        ClipboardContent {
            plain_text,
            paths,
            ..Default::default()
        }
    }

    fn background_text_reader(&mut self) -> Option<BackgroundTextReader> {
        // arboard's X11 and Wayland backends are process-global, so a fresh handle on another
        // thread reads the same selections (and the same data when we own them).
        Some(Box::new(|| {
            let Ok(mut clipboard) = LinuxClipboard::new() else {
                return BackgroundTextRead::Content(ClipboardContent::null_value());
            };
            let content = clipboard.read_text_and_paths();
            if clipboard.take_read_failure() {
                BackgroundTextRead::TimedOut
            } else {
                BackgroundTextRead::Content(content)
            }
        }))
    }

    fn read_plain_text(&mut self) -> String {
        self.read_text_retrying().unwrap_or_default()
    }

    fn write_to_primary_clipboard(&mut self, contents: ClipboardContent) {
        match self.write_to_specific_clipboard(LinuxClipboardKind::Primary, &contents) {
            Ok(_) => (),
            Err(arboard::Error::ClipboardNotSupported) => {
                log::info!(
                    "Primary clipboard is not supported, falling back to default clipboard."
                );
                // Try the default clipboard.
                self.write(contents);
            }
            Err(err) => {
                if contents.html.is_some() {
                    log::warn!("Unable to set primary clipboard HTML: {err:?}");
                } else {
                    log::warn!("Unable to set primary clipboard text: {err:?}");
                }
            }
        }
    }

    fn read_from_primary_clipboard(&mut self) -> ClipboardContent {
        match self.read_from_specific_clipboard(LinuxClipboardKind::Primary) {
            Ok(content) => content,
            Err(arboard::Error::ClipboardNotSupported) => {
                log::info!(
                    "Primary clipboard is not supported, falling back to default clipboard."
                );
                // Try the default clipboard.
                match self.read_from_specific_clipboard(LinuxClipboardKind::Clipboard) {
                    Ok(content) => content,
                    Err(err) => {
                        log::warn!("Unable to read from primary clipboard fallback: {err:?}");
                        ClipboardContent::null_value()
                    }
                }
            }
            Err(err) => {
                log::warn!("Unable to read from primary clipboard: {err:?}");
                ClipboardContent::null_value()
            }
        }
    }
}

impl LinuxClipboard {
    /// Parses Linux clipboard text for absolute file paths.
    ///
    /// When copying files, Linux file managers typically place the paths as text content onto
    /// the clipboard. We parse this text to extract absolute paths, but if ANY line is not an
    /// absolute path, we assume this is regular text content and return None (no paths).
    fn parse_valid_filepaths_from_text(&mut self, text_content: &str) -> Option<Vec<String>> {
        let mut file_paths = Vec::new();

        // Cheap syntactic pass first (no filesystem access): ordinary text is rejected here
        // without ever stat'ing a path taken from the clipboard on the main thread.
        let all_lines_absolute = text_content.trim().lines().map(str::trim).all(|line| {
            line.is_empty()
                || line.starts_with("file://")
                || std::path::Path::new(line).is_absolute()
        });
        if !all_lines_absolute {
            return None;
        }

        // Check for absolute filepaths
        for line in text_content.trim().lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let candidate_path_str = if let Some(uri_path) = line.strip_prefix("file://") {
                match urlencoding::decode(uri_path) {
                    Ok(decoded_path) => decoded_path.into_owned(),
                    Err(_) => uri_path.to_string(),
                }
            } else {
                line.to_string()
            };

            let candidate_path = std::path::Path::new(&candidate_path_str);
            if candidate_path.is_absolute() && candidate_path.exists() {
                file_paths.push(candidate_path_str);
            } else {
                // Not an absolute-path indicates the text was not from copying files, so return
                return None;
            }
        }

        if file_paths.is_empty() {
            None
        } else {
            Some(file_paths)
        }
    }

    /// Reads clipboard content from a specific clipboard buffer.
    fn read_from_specific_clipboard(
        &mut self,
        clipboard_kind: LinuxClipboardKind,
    ) -> Result<ClipboardContent, arboard::Error> {
        self.read_from_specific_clipboard_inner(clipboard_kind, true)
    }

    /// `include_images == false` skips the image read/decode entirely (`images` stays `None`).
    fn read_from_specific_clipboard_inner(
        &mut self,
        clipboard_kind: LinuxClipboardKind,
        include_images: bool,
    ) -> Result<ClipboardContent, arboard::Error> {
        let text_result = self.read_text_retrying();
        let mut content = ClipboardContent {
            plain_text: text_result.as_ref().map(|s| s.clone()).unwrap_or_default(),
            ..Default::default()
        };

        // Get file paths from clipboard (Linux-specific)
        content.paths = self.parse_valid_filepaths_from_text(&content.plain_text);

        // Attempt to use HTML data first.
        match self.inner.get().clipboard(clipboard_kind).html() {
            Ok(html) => {
                content.html = html.is_empty().not().then_some(html);

                // Try to get image content from clipboard
                if include_images {
                    content.images = crate::clipboard_utils::read_images_from_clipboard(
                        &mut self.inner,
                        &content.html,
                        &content.plain_text,
                    );
                }

                return Ok(content);
            }
            Err(err) => {
                log::info!(
                    "Unable to read HTML from clipboard: {err:?}, falling back to plaintext."
                );
            }
        }

        // Fallback to using plaintext
        if include_images {
            content.images = crate::clipboard_utils::read_images_from_clipboard(
                &mut self.inner,
                &None, // No HTML in fallback case
                &content.plain_text,
            );
        }

        // Return success if we have ANY content (text, paths, OR images)
        // Only error if ALL content types failed
        if text_result.is_ok()
            || content
                .paths
                .as_ref()
                .is_some_and(|paths| !paths.is_empty())
            || content.images.as_ref().is_some_and(|imgs| !imgs.is_empty())
        {
            Ok(content)
        } else {
            // All content types failed - return the text error
            text_result.map(|_| content)
        }
    }

    fn write_to_specific_clipboard(
        &mut self,
        clipboard_kind: LinuxClipboardKind,
        contents: &ClipboardContent,
    ) -> Result<(), arboard::Error> {
        if let Some(html) = &contents.html {
            self.inner
                .set()
                .clipboard(clipboard_kind)
                .html(html, Some(&contents.plain_text))
        } else {
            self.inner
                .set()
                .clipboard(clipboard_kind)
                .text(&contents.plain_text)
        }
    }
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
