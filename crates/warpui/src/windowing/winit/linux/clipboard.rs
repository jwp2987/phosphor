use std::ops::Not;
use std::time::{Duration, Instant};

use arboard::{
    self, Clipboard as LinuxClipboardInner, GetExtLinux, LinuxClipboardKind, SetExtLinux,
};
use zbus::zvariant::NoneValue;

use crate::{
    clipboard::{
        BackgroundClipboardWriter, BackgroundTextRead, BackgroundTextReader, ClipboardContent,
        ClipboardTarget,
    },
    Clipboard,
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

/// How long the html and image probes that follow a successful text read may wait, in total, for
/// the owner's first answers (#1192). The owner just answered the text request, so a probe it
/// ignores is a stall, not a slow owner; unbounded, each one held the UI thread (and the Enter
/// typed behind a paste) for arboard's whole 10 s first-answer timeout.
const PROBE_BUDGET: Duration = Duration::from_millis(750);

/// How long the input editor's paste may wait, in total, for a clipboard owner that never answers
/// (#1331). arboard waits 10 s per attempt and the text read retries, so a hung owner (a crashed
/// clipboard manager, a remote-desktop bridge) held the typed keys for 30+ s. Only the wait for
/// the owner's FIRST answer is capped; a transfer that has started keeps its stall window.
const FULL_READ_BUDGET: Duration = Duration::from_millis(2_000);

/// How long quitting waits for an in-flight copy to land before handing the clipboard to the
/// clipboard manager, and for the whole handoff (#1330). Quit must never hang on the clipboard.
const EXIT_PENDING_WRITE_WAIT: Duration = Duration::from_millis(300);
const EXIT_HANDOFF_BUDGET: Duration = Duration::from_millis(600);

/// Whether a read that came back with nothing was a timeout. arboard flags the first slow failure,
/// but once `FULL_READ_BUDGET` is spent the retries fail instantly (which looks like an empty
/// clipboard), so a read that ran to the budget with no text also counts.
fn read_timed_out(flagged: bool, elapsed: Duration, budget: Duration, has_text: bool) -> bool {
    flagged || (!has_text && elapsed >= budget)
}

/// Quit-time handoff: waits (bounded) for in-flight copies, then runs `handoff`. Returns whether
/// the handoff ran. `wait_for_pending` blocks until the copies finish (the serial writer wakes it)
/// or its budget passes, and says whether they finished; a copy still unfinished at the budget is
/// logged, not silently dropped.
fn flush_for_exit(wait_for_pending: impl FnOnce() -> bool, handoff: impl FnOnce()) -> bool {
    if !wait_for_pending() {
        log::warn!(
            "A clipboard copy was still in flight {EXIT_PENDING_WRITE_WAIT:?} after quit began; \
             handing the clipboard over without it"
        );
    }
    handoff();
    true
}

/// Kill switch for the targets-based `has_content` (#1306): `PHOSPHOR_CLIPBOARD_TARGETS_QUERY=0`
/// restores the read-based probe.
fn targets_query_enabled() -> bool {
    std::env::var("PHOSPHOR_CLIPBOARD_TARGETS_QUERY").map_or(true, |v| v != "0")
}

/// `has_content` from a targets query: `Some` when the query answered (the clipboard is non-empty
/// iff the owner offers text, an image, HTML or a file list), `None` when it failed and the caller
/// should fall back to reading.
fn has_content_from_formats(
    formats: &Result<arboard::ClipboardFormats, arboard::Error>,
) -> Option<bool> {
    formats.as_ref().ok().map(arboard::ClipboardFormats::any)
}

/// Caps the owner's first-answer wait on this thread while alive; clears the cap on drop.
struct ProbeBudget;

impl ProbeBudget {
    fn start() -> Self {
        arboard::set_first_answer_deadline(Some(Instant::now() + PROBE_BUDGET));
        Self
    }
}

impl Drop for ProbeBudget {
    fn drop(&mut self) {
        arboard::set_first_answer_deadline(None);
    }
}

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
        // A copy still being written is what the user just copied (#1307).
        if let Some(pending) = crate::clipboard::pending_write(ClipboardTarget::Clipboard) {
            self.read_failed = false;
            return Ok(pending.plain_text.clone());
        }
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

    /// The pre-#1306 probe: reads, and for an image-only clipboard decodes. Used when the targets
    /// query is disabled or failed.
    fn has_content_by_reading(&mut self) -> bool {
        // Non-empty text is the overwhelmingly common case and answers the question without
        // the HTML, path and image reads (an image decode can take hundreds of ms).
        if self.inner.get().text().is_ok_and(|text| !text.is_empty()) {
            return true;
        }
        // No plain text: HTML or copied paths still answer without an image decode.
        if !self.read_without_images().is_empty() {
            return true;
        }
        !self.read().is_empty()
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
        if crate::clipboard::pending_write(ClipboardTarget::Clipboard).is_some() {
            return true;
        }
        // One TARGETS request (X11) or the offer's mime types (Wayland) answers "is anything
        // there" without reading or decoding any of it, so an image-only clipboard no longer pays
        // for an image decode (#1306). Bounded like the other probes; a failed query falls back to
        // the reads.
        if targets_query_enabled() {
            let formats = {
                let _budget = ProbeBudget::start();
                self.inner.get().formats()
            };
            if let Some(answer) = has_content_from_formats(&formats) {
                return answer;
            }
            log::info!("Clipboard targets query failed ({formats:?}); falling back to reads");
        }
        // The fallback reads wait at most the full-read budget for the owner's first answer.
        arboard::set_first_answer_deadline(Some(Instant::now() + FULL_READ_BUDGET));
        let answer = self.has_content_by_reading();
        arboard::set_first_answer_deadline(None);
        answer
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
        let pending = crate::clipboard::pending_write(ClipboardTarget::Clipboard);
        Some(Box::new(move || {
            if let Some(pending) = pending {
                let mut content = (*pending).clone();
                content.html = None;
                content.images = None;
                return BackgroundTextRead::Content(content);
            }
            let Ok(mut clipboard) = LinuxClipboard::new() else {
                return BackgroundTextRead::Content(ClipboardContent::null_value());
            };
            // Same bounded first-answer wait as the full reader (#1331): the terminal paste holds
            // typed keys for as long as this takes, and arboard's retries made that 30+ s.
            let started = Instant::now();
            arboard::set_first_answer_deadline(Some(started + FULL_READ_BUDGET));
            let content = clipboard.read_text_and_paths();
            arboard::set_first_answer_deadline(None);
            let flagged = clipboard.take_read_failure();
            if read_timed_out(
                flagged,
                started.elapsed(),
                FULL_READ_BUDGET,
                !content.plain_text.is_empty(),
            ) {
                BackgroundTextRead::TimedOut
            } else {
                BackgroundTextRead::Content(content)
            }
        }))
    }

    fn flush_on_exit(&mut self) {
        // The X11 selection dies with the process unless the clipboard manager takes it over
        // (ICCCM SAVE_TARGETS). arboard only does that when its last handle drops, which never
        // happens for a long-lived app (#1330). Run it on a helper thread so a wedged X
        // connection cannot hold the quit past the budget. Wayland has no such protocol.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("clipboard-exit".into())
            .spawn(move || {
                flush_for_exit(
                    || {
                        // Woken by the serial writer finishing the copy (a Condvar notify), not
                        // polled; `EXIT_PENDING_WRITE_WAIT` is only the logged give-up.
                        crate::clipboard::wait_for_pending_writes(
                            ClipboardTarget::Clipboard,
                            EXIT_PENDING_WRITE_WAIT,
                        )
                    },
                    arboard::hand_over_to_clipboard_manager,
                );
                let _ = done_tx.send(());
            })
            .is_ok();
        if spawned && done_rx.recv_timeout(EXIT_HANDOFF_BUDGET).is_err() {
            log::warn!("Clipboard handoff on exit did not finish in time; quitting anyway");
        }
    }

    fn background_full_reader(&mut self) -> Option<BackgroundTextReader> {
        // Same process-global backends as `background_text_reader`; this one also probes the HTML
        // and image targets, which a silent owner answers only after the arboard timeout (#1300).
        let pending = crate::clipboard::pending_write(ClipboardTarget::Clipboard);
        Some(Box::new(move || {
            if let Some(pending) = pending {
                return BackgroundTextRead::Content((*pending).clone());
            }
            let Ok(mut clipboard) = LinuxClipboard::new() else {
                return BackgroundTextRead::Content(ClipboardContent::null_value());
            };
            let started = Instant::now();
            arboard::set_first_answer_deadline(Some(started + FULL_READ_BUDGET));
            let content = clipboard.read();
            arboard::set_first_answer_deadline(None);
            let flagged = clipboard.take_read_failure();
            if read_timed_out(
                flagged,
                started.elapsed(),
                FULL_READ_BUDGET,
                !content.plain_text.is_empty(),
            ) {
                BackgroundTextRead::TimedOut
            } else {
                BackgroundTextRead::Content(content)
            }
        }))
    }

    fn background_primary_reader(&mut self) -> Option<BackgroundTextReader> {
        // Middle-click paste (#1331): the primary selection's owner can stall a read just as the
        // clipboard's can, and the click used to wait on it on the UI thread with no budget.
        let pending = crate::clipboard::pending_write(ClipboardTarget::Primary);
        Some(Box::new(move || {
            // A copy to PRIMARY still being written is what the user just selected (#1307).
            if let Some(pending) = pending {
                return BackgroundTextRead::Content((*pending).clone());
            }
            let Ok(mut clipboard) = LinuxClipboard::new() else {
                return BackgroundTextRead::Content(ClipboardContent::null_value());
            };
            let started = Instant::now();
            arboard::set_first_answer_deadline(Some(started + FULL_READ_BUDGET));
            let content = clipboard.read_from_primary_clipboard();
            arboard::set_first_answer_deadline(None);
            let flagged = clipboard.take_read_failure();
            if read_timed_out(
                flagged,
                started.elapsed(),
                FULL_READ_BUDGET,
                !content.plain_text.is_empty(),
            ) {
                BackgroundTextRead::TimedOut
            } else {
                BackgroundTextRead::Content(content)
            }
        }))
    }

    fn background_writer(&mut self) -> Option<BackgroundClipboardWriter> {
        // ONE long-lived thread owns ONE clipboard handle for the process lifetime and performs
        // the writes in order, so the owning handle is never dropped after a write: arboard's X11
        // backend serves the selection from a process-global context and tears it down (handing
        // the data to the clipboard manager) when the last handle drops
        // (`lib/arboard/src/platform/linux/x11.rs`, `impl Drop for Clipboard`). The Wayland
        // backend serves each copy from its own thread, independent of the handle. A stalled
        // owner stalls this thread, not the UI (#1307).
        static WRITER: std::sync::OnceLock<BackgroundClipboardWriter> = std::sync::OnceLock::new();
        Some(
            WRITER
                .get_or_init(|| {
                    crate::clipboard::serial_writer(
                        || LinuxClipboard::new().ok(),
                        |clipboard, contents, target| clipboard.write_for_target(&contents, target),
                    )
                })
                .clone(),
        )
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
    /// One write to `target` (primary falls back to the clipboard where unsupported); `true` on
    /// success.
    fn write_for_target(&mut self, contents: &ClipboardContent, target: ClipboardTarget) -> bool {
        match target {
            ClipboardTarget::Clipboard => self
                .write_to_specific_clipboard(LinuxClipboardKind::Clipboard, contents)
                .map_err(|err| log::warn!("Unable to set clipboard text: {err:?}"))
                .is_ok(),
            ClipboardTarget::Primary => {
                match self.write_to_specific_clipboard(LinuxClipboardKind::Primary, contents) {
                    Ok(()) => true,
                    Err(arboard::Error::ClipboardNotSupported) => self
                        .write_to_specific_clipboard(LinuxClipboardKind::Clipboard, contents)
                        .is_ok(),
                    Err(err) => {
                        log::warn!("Unable to set primary clipboard text: {err:?}");
                        false
                    }
                }
            }
        }
    }

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
        let pending_target = match clipboard_kind {
            LinuxClipboardKind::Primary => ClipboardTarget::Primary,
            _ => ClipboardTarget::Clipboard,
        };
        if let Some(pending) = crate::clipboard::pending_write(pending_target) {
            let mut content = (*pending).clone();
            if !include_images {
                content.images = None;
            }
            return Ok(content);
        }
        let text_result = self.read_text_retrying();
        let mut content = ClipboardContent {
            plain_text: text_result.as_ref().map(|s| s.clone()).unwrap_or_default(),
            ..Default::default()
        };

        // Get file paths from clipboard (Linux-specific)
        content.paths = self.parse_valid_filepaths_from_text(&content.plain_text);

        // The probes below ask for targets the owner may not answer; once there is text, bound
        // them (#1192). With no text the probes are the whole read and keep the full timeout.
        let _probe_budget = (!content.plain_text.is_empty()).then(ProbeBudget::start);

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
