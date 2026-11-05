use std::ops::Not;
use std::time::{Duration, Instant};

use arboard::{
    self, Clipboard as LinuxClipboardInner, GetExtLinux, LinuxClipboardKind, SetExtLinux,
};
use zbus::zvariant::NoneValue;

use crate::{
    Clipboard,
    clipboard::{
        Availability, AvailabilityProbe, BackgroundClipboardWriter, ClipboardContent,
        ClipboardReader, ClipboardTarget, ReadKind, ReadOutcome,
    },
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
///
/// Both are give-up nets, not paces (#1692, #1693): the pending-write wait is woken by the serial
/// writer finishing the copy (a Condvar notify) and the handoff budget by the helper thread's
/// done message, so a normal quit -- no copy in flight, or one that lands at once -- is no
/// slower for them. They only need to outlast a loaded machine, where a copy that takes a few
/// hundred ms to land used to be dropped at quit (300 ms / 600 ms). The handoff budget covers the
/// pending wait plus arboard's own clipboard-manager handover wait (1 s, also event-driven), and
/// stays under the signal shutdown deadline (`termination_signals::SHUTDOWN_DEADLINE`, 5 s) so a
/// SIGTERM quit still gets to the end of its steps.
const EXIT_PENDING_WRITE_WAIT: Duration = Duration::from_millis(2_000);
const EXIT_HANDOFF_BUDGET: Duration = Duration::from_millis(3_000);

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

/// Runs `work` on a helper thread and waits for it until it finishes or `budget` passes,
/// whichever is first. Returns whether it finished in time. The wait is the thread's done
/// message (`recv_timeout`), so it ends the moment the work does; `budget` is only the give-up
/// for a wedged X connection. A failed spawn returns `false` without running the work (quit
/// must not hang on the clipboard, and running it inline would have no bound).
fn run_with_budget(work: impl FnOnce() + Send + 'static, budget: Duration) -> bool {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("clipboard-exit".into())
        .spawn(move || {
            work();
            let _ = done_tx.send(());
        })
        .is_ok();
    spawned && done_rx.recv_timeout(budget).is_ok()
}

/// Kill switch for the targets-based availability query (#1306): `PHOSPHOR_CLIPBOARD_TARGETS_QUERY=0`
/// makes the availability unknown (no query is sent).
fn targets_query_enabled() -> bool {
    std::env::var("PHOSPHOR_CLIPBOARD_TARGETS_QUERY").map_or(true, |v| v != "0")
}

/// Availability from a targets query: `Some` when the query answered (the clipboard is non-empty
/// iff the owner offers text, an image, HTML or a file list), `None` when it failed and the caller
/// should treat the availability as unknown.
fn has_content_from_formats(
    formats: &Result<arboard::ClipboardFormats, arboard::Error>,
) -> Option<bool> {
    formats.as_ref().ok().map(arboard::ClipboardFormats::any)
}

/// Tighter cap for the post-text probes (#1192): replaces whatever deadline the read started with
/// and clears it on drop.
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

thread_local! {
    /// Whether a [`ReadBudget`] is already capping this thread's reads, so a nested read (the
    /// fallback probe calling `read()`) keeps the outer, tighter-started deadline.
    static READ_BUDGET_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Caps the owner's first-answer wait on this thread while alive; clears the cap on drop. An
/// outer budget wins: starting one inside another is a no-op. Every blocking read primitive below
/// takes one (they run only inside the reader on a worker thread, #1405), so a read waits at most
/// the budget instead of arboard's 10 s per attempt (#1380).
struct ReadBudget {
    owns: bool,
}

impl ReadBudget {
    fn start(budget: Duration) -> Self {
        Self::start_at(Instant::now() + budget)
    }

    fn start_at(deadline: Instant) -> Self {
        let owns = !READ_BUDGET_ACTIVE.with(|active| active.replace(true));
        if owns {
            arboard::set_first_answer_deadline(Some(deadline));
        }
        Self { owns }
    }
}

impl Drop for ReadBudget {
    fn drop(&mut self) {
        if self.owns {
            arboard::set_first_answer_deadline(None);
            READ_BUDGET_ACTIVE.with(|active| active.set(false));
        }
    }
}

/// The availability query on a fresh handle, for the worker and listener threads (#1406).
fn probe_availability(target: ClipboardTarget) -> Availability {
    match LinuxClipboard::new() {
        Ok(mut clipboard) => clipboard.availability(target),
        Err(_) => Availability::Unknown,
    }
}

/// Whether the X11 owner listener is running, so the cached availability is event-driven and a
/// menu open needs no refresh.
static OWNER_EVENTS_LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts (once) the thread that turns X11 clipboard owner changes into cache refreshes: an XFixes
/// `SelectionNotify` for CLIPBOARD and PRIMARY arrives whenever any client takes a selection, and
/// the thread blocks on the X connection for it (no polling). Not started on Wayland, whose offer
/// events only reach a client that has keyboard focus; there the cache is refreshed when a menu
/// opens and after our own writes instead. Any failure leaves the events off and logs why.
fn start_owner_listener() {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::Event;
    use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
    use x11rb::protocol::xproto::ConnectionExt as _;

    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("clipboard-owner-events".into())
            .spawn(|| {
                let setup = || -> Result<_, Box<dyn std::error::Error>> {
                    let (conn, screen) = x11rb::rust_connection::RustConnection::connect(None)?;
                    let root = conn.setup().roots[screen].root;
                    conn.xfixes_query_version(5, 0)?.reply()?;
                    let clipboard = conn.intern_atom(false, b"CLIPBOARD")?.reply()?.atom;
                    let primary = u32::from(x11rb::protocol::xproto::AtomEnum::PRIMARY);
                    let mask = SelectionEventMask::SET_SELECTION_OWNER
                        | SelectionEventMask::SELECTION_WINDOW_DESTROY
                        | SelectionEventMask::SELECTION_CLIENT_CLOSE;
                    conn.xfixes_select_selection_input(root, clipboard, mask)?;
                    conn.xfixes_select_selection_input(root, primary, mask)?;
                    conn.flush()?;
                    Ok((conn, clipboard, primary))
                };
                let (conn, clipboard, primary) = match setup() {
                    Ok(ready) => ready,
                    Err(err) => {
                        log::info!(
                            "Clipboard owner events unavailable ({err}); refreshing on menu open"
                        );
                        return;
                    }
                };
                OWNER_EVENTS_LIVE.store(true, std::sync::atomic::Ordering::SeqCst);
                // The first answer, then one per owner change.
                for target in [ClipboardTarget::Clipboard, ClipboardTarget::Primary] {
                    crate::clipboard::availability_changed(target, &probe_availability);
                }
                loop {
                    match conn.wait_for_event() {
                        Ok(Event::XfixesSelectionNotify(event)) => {
                            let target = if event.selection == clipboard {
                                ClipboardTarget::Clipboard
                            } else if event.selection == primary {
                                ClipboardTarget::Primary
                            } else {
                                continue;
                            };
                            crate::clipboard::availability_changed(target, &probe_availability);
                        }
                        Ok(_) => {}
                        Err(err) => {
                            // The X connection is gone; menu opens refresh again from now on.
                            log::warn!("Clipboard owner events ended: {err}");
                            OWNER_EVENTS_LIVE.store(false, std::sync::atomic::Ordering::SeqCst);
                            return;
                        }
                    }
                }
            })
            .is_ok();
        if !spawned {
            log::warn!("Unable to start the clipboard owner-event thread");
        }
    });
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

    fn reader(&mut self, kind: ReadKind) -> ClipboardReader {
        // arboard's X11 and Wayland backends are process-global, so a fresh handle on the worker
        // reads the same selections (and the same data when we own them). Every kind has the same
        // bounded wait for the owner's FIRST answer (#1331): the terminal paste holds typed keys
        // for as long as this takes, and arboard's own retries made that 30+ s. The give-up is the
        // `TimedOut` answer, logged by the caller with the request id.
        Box::new(move || {
            let Ok(mut clipboard) = LinuxClipboard::new() else {
                log::warn!("clipboard read ({kind:?}): no clipboard connection could be opened");
                return ReadOutcome::Unavailable;
            };
            let started = Instant::now();
            let budget = ReadBudget::start_at(started + FULL_READ_BUDGET);
            let content = match kind {
                ReadKind::Full => clipboard.read(),
                ReadKind::TextAndPaths => clipboard.read_text_and_paths(),
                ReadKind::Primary => clipboard.read_from_primary_clipboard(),
            };
            drop(budget);
            let flagged = clipboard.take_read_failure();
            if read_timed_out(
                flagged,
                started.elapsed(),
                FULL_READ_BUDGET,
                !content.plain_text.is_empty(),
            ) {
                ReadOutcome::TimedOut
            } else {
                ReadOutcome::Content(content)
            }
        })
    }

    fn has_primary_selection(&self) -> bool {
        true
    }

    fn availability_probe(&mut self) -> Option<AvailabilityProbe> {
        // The TARGETS query waits for the owner's first answer; the probe runs on a worker with a
        // fresh handle (arboard's X11 and Wayland state is process-global), never the UI thread.
        start_owner_listener();
        Some(std::sync::Arc::new(probe_availability))
    }

    fn has_availability_events(&self) -> bool {
        OWNER_EVENTS_LIVE.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn availability(&mut self, target: ClipboardTarget) -> Availability {
        if crate::clipboard::pending_write(target).is_some() {
            return Availability::HasContent;
        }
        // One TARGETS request (X11) or the offer's mime types (Wayland) answers "is anything
        // there" without reading or decoding any of it, so an image-only clipboard no longer pays
        // for an image decode (#1306). Bounded by the probe budget; a failed query is `Unknown`
        // (the menu stays enabled) rather than a fallback read on the calling thread (#1405).
        if !targets_query_enabled() {
            return Availability::Unknown;
        }
        let kind = match target {
            ClipboardTarget::Clipboard => LinuxClipboardKind::Clipboard,
            ClipboardTarget::Primary => LinuxClipboardKind::Primary,
        };
        let formats = {
            let _budget = ProbeBudget::start();
            self.inner.get().clipboard(kind).formats()
        };
        match has_content_from_formats(&formats) {
            Some(true) => Availability::HasContent,
            Some(false) => Availability::Empty,
            None => {
                log::info!("Clipboard targets query failed ({formats:?}); availability unknown");
                Availability::Unknown
            }
        }
    }

    fn flush_on_exit(&mut self) {
        // The X11 selection dies with the process unless the clipboard manager takes it over
        // (ICCCM SAVE_TARGETS). arboard only does that when its last handle drops, which never
        // happens for a long-lived app (#1330). Run it on a helper thread so a wedged X
        // connection cannot hold the quit past the budget. Wayland has no such protocol.
        let finished = run_with_budget(
            || {
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
            },
            EXIT_HANDOFF_BUDGET,
        );
        if !finished {
            log::warn!("Clipboard handoff on exit did not finish in time; quitting anyway");
        }
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
}

/// The blocking read primitives. Private to this module: they run only inside the reader
/// [`Clipboard::reader`] hands out, on a worker thread (#1405).
impl LinuxClipboard {
    /// True once if the most recent read failed because the clipboard owner did not answer in time
    /// (as opposed to the clipboard having nothing to read), clearing the flag (#1173).
    fn take_read_failure(&mut self) -> bool {
        std::mem::take(&mut self.read_failed)
    }

    fn read(&mut self) -> ClipboardContent {
        let _budget = ReadBudget::start(FULL_READ_BUDGET);
        match self.read_from_specific_clipboard(LinuxClipboardKind::Clipboard) {
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
        let _budget = ReadBudget::start(FULL_READ_BUDGET);
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

    fn read_from_primary_clipboard(&mut self) -> ClipboardContent {
        let _budget = ReadBudget::start(FULL_READ_BUDGET);
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
