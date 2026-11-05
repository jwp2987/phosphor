use crate::sync_shim::atomic::{AtomicU64, AtomicUsize};
use crate::sync_shim::{Condvar, Mutex};

/// A system clipboard. Each platform implements this trait once, in one module, and keeps its
/// quirks (X11 INCR and first-answer deadlines, Wayland offers, NSPasteboard promises, Win32
/// clipboard ownership) behind it. There is deliberately NO synchronous read on this trait (#1405):
/// a read is requested with [`request_read`] and answered later with a [`ReadOutcome`], so no
/// clipboard owner can hold the calling thread. Writes go through [`write_in_background`] and the
/// quit-time flush through [`Clipboard::flush_on_exit`].
pub trait Clipboard: 'static {
    /// The platform's write primitive. Callers go through [`write_in_background`], which orders
    /// writes and runs this off the UI thread where the platform needs it.
    fn write(&mut self, contents: ClipboardContent);

    /// Writes to the primary clipboard, used to support the Primary Selection Protocol (middle-click paste).
    ///
    /// NOTE: For platforms that don't support the primary clipboard, it writes to the default clipboard instead.
    fn write_to_primary_clipboard(&mut self, contents: ClipboardContent) {
        self.write(contents)
    }

    /// A self-contained reader for `kind` that may BLOCK in the OS and therefore only ever runs on
    /// a worker thread (see [`request_read`]). It must own everything it needs (a fresh handle on
    /// the process-global clipboard), apply the platform's own first-answer deadline, and answer
    /// [`ReadOutcome::TimedOut`] when the owner never answers or [`ReadOutcome::Unavailable`] when
    /// no clipboard handle could be opened. The caller wraps it with the blocked-reader cap, the
    /// abandon epoch, the paste size cap and the request log, and answers a copy that is still
    /// being written from memory, so platforms do none of that.
    ///
    /// macOS is the exception (#1782, owner decision 2026-11-05): it reads the pasteboard while
    /// building the reader, on the calling thread, which must be the main thread, and the
    /// returned closure only hands the copied content on, because an NSPasteboard read on a
    /// worker crashed the app (SIGBUS). A request from another thread while an NSApplication
    /// runs is logged as an error and queued for the main thread with a bounded wait; without
    /// one it reads in place (the headless TUI does have one, so its reads start on its main thread). The blocked-reader cap does not bound a macOS
    /// read: it has already happened when the reader is built.
    fn reader(&mut self, kind: ReadKind) -> ClipboardReader;

    /// True when [`Clipboard::reader`] is a cheap in-memory snapshot (the in-process test
    /// clipboard) that is run on the calling thread instead of a worker.
    /// No OS clipboard may ever answer true.
    fn reads_inline(&self) -> bool {
        false
    }

    /// What the clipboard holds, WITHOUT reading it: X11 `TARGETS`, the Wayland offer's mime list,
    /// the NSPasteboard type list, `IsClipboardFormatAvailable`. [`Availability::Unknown`] when the
    /// platform cannot say without reading (the default).
    fn availability(&mut self, _target: ClipboardTarget) -> Availability {
        Availability::Unknown
    }

    /// A self-contained availability query that may BLOCK in the OS (X11 `TARGETS` is bounded by a
    /// 750 ms first-answer deadline, #1406) and therefore only ever runs on a worker thread, or
    /// `None` where [`Clipboard::availability`] is a cheap local call (NSPasteboard's type list,
    /// `IsClipboardFormatAvailable`, the in-memory clipboard) that is run inline.
    fn availability_probe(&mut self) -> Option<AvailabilityProbe> {
        None
    }

    /// True once the platform pushes owner-change events into [`availability_changed`] (X11
    /// XFixes `SelectionNotify`), so the cached answer needs no refresh when a menu opens.
    fn has_availability_events(&self) -> bool {
        false
    }

    /// Whether the clipboard may hold anything. Never reads or decodes; an unknown answer counts as
    /// "may hold something" so a menu entry is not wrongly disabled.
    ///
    /// This asks the platform synchronously, which can block on a dead selection owner (#1406):
    /// UI code uses [`cached_has_content`] instead.
    fn has_content(&mut self) -> bool {
        !matches!(
            self.availability(ClipboardTarget::Clipboard),
            Availability::Empty
        )
    }

    /// Compatibility adapter over [`request_read`] for [`ReadKind::TextAndPaths`]: a reader to run
    /// on another thread, or `None` where the read is an inline snapshot. Prefer [`request_read`].
    fn background_text_reader(&mut self) -> Option<BackgroundTextReader> {
        legacy_reader(self, ReadKind::TextAndPaths)
    }

    /// Compatibility adapter for [`ReadKind::Full`] (see [`Clipboard::background_text_reader`]).
    fn background_full_reader(&mut self) -> Option<BackgroundTextReader> {
        legacy_reader(self, ReadKind::Full)
    }

    /// Compatibility adapter for [`ReadKind::Primary`] (see [`Clipboard::background_text_reader`]).
    /// `None` where there is no separate primary selection.
    fn background_primary_reader(&mut self) -> Option<BackgroundTextReader> {
        if !self.has_primary_selection() {
            return None;
        }
        legacy_reader(self, ReadKind::Primary)
    }

    /// Whether the platform has a primary selection that is separate from the clipboard (Linux).
    fn has_primary_selection(&self) -> bool {
        false
    }

    /// A self-contained writer that can run on another thread, or `None` when the platform's
    /// clipboard write is cheap and must stay on the calling thread. An X11/Wayland or Win32
    /// clipboard owner can stall a write, which froze the UI (#1307). Returns `true` on success.
    fn background_writer(&mut self) -> Option<BackgroundClipboardWriter> {
        None
    }

    /// Called once as the app quits, so what the user copied outlives the process: on X11 it asks
    /// the clipboard manager to take the data over (#1330). Must be bounded and must not panic.
    fn flush_on_exit(&mut self) {}
}

/// What a clipboard holds, from a query that reads nothing. See [`Clipboard::availability`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Nothing the app can paste is on offer.
    Empty,
    /// At least one pasteable flavor (text, html, a file list or an image) is on offer.
    HasContent,
    /// The platform could not tell without reading.
    Unknown,
}

/// Identifies one clipboard read request, so its start, its give-up and its late arrival can be
/// told apart in the log and by a consumer that matches an answer to the request it made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReadRequestId(pub u64);

static NEXT_READ_REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl ReadRequestId {
    pub fn next() -> Self {
        Self(NEXT_READ_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
}

/// How a clipboard read ended. The one answer type of the async read API.
#[derive(Debug)]
pub enum ReadOutcome {
    /// The text, copied paths and (for [`ReadKind::Full`]) html and images. Empty content when the
    /// clipboard held nothing.
    Content(ClipboardContent),
    /// The text and paths together are larger than [`MAX_PASTE_BYTES`] (this many bytes). The
    /// content was dropped on the reader thread; the paste is refused with a message, never
    /// truncated (#1382).
    TooLarge(usize),
    /// A deadline passed: the clipboard owner never answered, or too many earlier readers are
    /// still blocked in the OS. Always logged where it is decided.
    TimedOut,
    /// No clipboard handle could be opened (no display server, the Win32 clipboard unreachable).
    Unavailable,
}

impl ReadOutcome {
    /// The content, or empty content when the read gave none.
    pub fn into_content(self) -> ClipboardContent {
        match self {
            ReadOutcome::Content(content) => content,
            ReadOutcome::TooLarge(_) | ReadOutcome::TimedOut | ReadOutcome::Unavailable => {
                ClipboardContent::default()
            }
        }
    }
}

/// See [`Clipboard::reader`].
pub type ClipboardReader = Box<dyn FnOnce() -> ReadOutcome + Send + 'static>;

/// The result of a [`Clipboard::background_text_reader`] read: the compatibility form of
/// [`ReadOutcome`] (`Unavailable` is reported as empty content).
pub enum BackgroundTextRead {
    /// The text and copied paths (empty content when the clipboard held no text).
    Content(ClipboardContent),
    /// The clipboard owner never answered.
    TimedOut,
    /// The text and paths together are larger than [`MAX_PASTE_BYTES`] (this many bytes). The
    /// content was dropped on the reader thread; the paste is refused with a message, never
    /// truncated (#1382).
    TooLarge(usize),
}

impl From<ReadOutcome> for BackgroundTextRead {
    fn from(outcome: ReadOutcome) -> Self {
        match outcome {
            ReadOutcome::Content(content) => BackgroundTextRead::Content(content),
            ReadOutcome::TooLarge(bytes) => BackgroundTextRead::TooLarge(bytes),
            ReadOutcome::TimedOut => BackgroundTextRead::TimedOut,
            ReadOutcome::Unavailable => BackgroundTextRead::Content(ClipboardContent::default()),
        }
    }
}

/// The most text (plain text, HTML and copied paths together) one paste may carry. A paste is
/// held in memory while it waits behind other input and again as pty bytes, so a multi-GB
/// clipboard must not be admitted. Over it, the paste is refused with a message (#1382).
pub const MAX_PASTE_BYTES: usize = 64 * 1024 * 1024;

/// See [`Clipboard::background_text_reader`].
pub type BackgroundTextReader = Box<dyn FnOnce() -> BackgroundTextRead + Send + 'static>;

/// Counts background reader threads that have started and not yet returned.
pub struct ReaderTracker {
    in_flight: AtomicUsize,
    /// Bumped by [`ReaderTracker::abandon_pending`]: a reader created before the bump belongs to a
    /// paste that gave up on it (#1353 phase 6, INP-A4).
    abandon_epoch: AtomicU64,
}

impl ReaderTracker {
    crate::model_const_fn! {
    pub fn new() -> Self {
        Self {
            in_flight: AtomicUsize::new(0),
            abandon_epoch: AtomicU64::new(0),
        }
    }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The paste that was waiting on the readers made so far gave up. A reader already blocked in
    /// the OS cannot be interrupted, but it is logged as abandoned when it returns, so the
    /// blocked-reader count and the log say what is going on. Returns how many were in flight.
    pub fn abandon_pending(&self) -> usize {
        self.abandon_epoch
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.in_flight()
    }
}

/// Signals the cancel flag of every clipboard read started so far (#1353 phase 6, INP-A4): the
/// paste backstop gave up, so those reads' results will not be used. See
/// [`ReaderTracker::abandon_pending`].
pub fn abandon_pending_reads() {
    let blocked = PASTE_READERS.abandon_pending();
    if blocked > 0 {
        log::warn!(
            "paste gave up on its clipboard read; {blocked} reader thread(s) are still blocked in the OS and their results will not be used"
        );
    }
}

impl Default for ReaderTracker {
    fn default() -> Self {
        Self::new()
    }
}

struct InFlightGuard(&'static ReaderTracker);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0
            .in_flight
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Most background readers allowed to be blocked at once. A reader stuck in an OS clipboard call
/// cannot be interrupted, and each paste attempt used to start another thread behind it (#1375).
pub const MAX_IN_FLIGHT_READERS: usize = 4;
/// A read that returns later than this is logged as slow.
const SLOW_READ: std::time::Duration = std::time::Duration::from_secs(5);

crate::model_static! {
    static PASTE_READERS: ReaderTracker = ReaderTracker::new();
}

/// Wraps a platform reader so a wedged one is visible and bounded: the read is logged when it
/// returns long after it started, and once [`MAX_IN_FLIGHT_READERS`] readers are still blocked a
/// new one answers `TimedOut` at once instead of starting yet another blocked OS call. (A blocked
/// OS clipboard call has no cancel; this is the stop for further leaks.)
fn tracked_reader(id: ReadRequestId, kind: ReadKind, inner: ClipboardReader) -> ClipboardReader {
    tracked_reader_with(
        &PASTE_READERS,
        MAX_IN_FLIGHT_READERS,
        SLOW_READ,
        id,
        kind,
        inner,
    )
}

fn tracked_reader_with(
    tracker: &'static ReaderTracker,
    max: usize,
    slow: std::time::Duration,
    id: ReadRequestId,
    kind: ReadKind,
    inner: ClipboardReader,
) -> ClipboardReader {
    use std::sync::atomic::Ordering;
    // The cancel flag of this read: the epoch the paste that asked for it saw.
    let epoch = tracker.abandon_epoch.load(Ordering::SeqCst);
    Box::new(move || {
        let before = tracker.in_flight.fetch_add(1, Ordering::SeqCst);
        let _guard = InFlightGuard(tracker);
        if before >= max {
            log::warn!(
                "clipboard read #{} ({kind:?}) gave up: {before} reader thread(s) are still blocked in the OS; not starting another",
                id.0
            );
            return ReadOutcome::TimedOut;
        }
        let started = std::time::Instant::now();
        let read = cap_paste_size(inner(), MAX_PASTE_BYTES);
        if matches!(read, ReadOutcome::TimedOut) {
            // The give-up net of a stalled owner: always logged (#1405).
            log::warn!(
                "clipboard read #{} ({kind:?}) gave up after {:?}: the owner did not answer",
                id.0,
                started.elapsed()
            );
        }
        if tracker.abandon_epoch.load(Ordering::SeqCst) != epoch {
            // Only logged: another paste may be waiting on a read that was started before this
            // abandon, so the result is still handed back and the consumer decides.
            log::warn!(
                "clipboard read #{} returned after {:?}; a paste gave up on it while it was blocked",
                id.0,
                started.elapsed()
            );
        }
        if started.elapsed() >= slow {
            log::warn!(
                "clipboard read #{} returned after {:?} (budget {:?}); the paste had long given up on it",
                id.0,
                started.elapsed(),
                slow
            );
        }
        read
    })
}

/// Which clipboard flavors an off-thread read needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadKind {
    /// Text, paths, HTML and images.
    Full,
    /// Text and copied paths only (`html` and `images` are `None`): what a paste uses, so the
    /// platform skips the HTML and image probes.
    TextAndPaths,
    /// The Linux primary selection (text and paths); the clipboard where there is none.
    Primary,
}

impl ReadKind {
    /// The clipboard a read of this kind targets, for the copy-still-being-written check.
    pub fn target(self) -> ClipboardTarget {
        match self {
            ReadKind::Primary => ClipboardTarget::Primary,
            ReadKind::Full | ReadKind::TextAndPaths => ClipboardTarget::Clipboard,
        }
    }
}

/// The boxed future of a [`read_in_background`].
pub type PendingRead =
    std::pin::Pin<Box<dyn std::future::Future<Output = BackgroundTextRead> + Send + 'static>>;

/// The boxed future of a [`request_read`].
pub type PendingOutcome =
    std::pin::Pin<Box<dyn std::future::Future<Output = ReadOutcome> + Send + 'static>>;

/// What [`request_read`] started.
pub enum ReadStart {
    /// The answer is already known (an inline snapshot, a copy still being written, or a reader
    /// thread that could not be started).
    Ready(ReadOutcome),
    /// A worker thread is reading; await the future (`ctx.spawn`) and act on the answer there.
    Pending(PendingOutcome),
}

/// One clipboard read in flight: THE async read API (#1405).
pub struct ReadRequest {
    pub id: ReadRequestId,
    pub kind: ReadKind,
    pub start: ReadStart,
}

impl ReadRequest {
    /// The answer as a future either way, for `ctx.spawn`.
    pub fn into_future(self) -> PendingOutcome {
        match self.start {
            ReadStart::Ready(outcome) => Box::pin(futures::future::ready(outcome)),
            ReadStart::Pending(pending) => pending,
        }
    }
}

/// What [`read_in_background`] started.
pub enum StartedRead {
    /// The platform reads inline (see [`Clipboard::reads_inline`]), so the answer is known
    /// already; the caller handles the result at once (no event-loop hop).
    Ready(BackgroundTextRead),
    /// A worker thread is reading; await the future (`ctx.spawn`) and handle the result there.
    Pending(PendingRead),
}

/// Requests a clipboard read that never blocks the calling (UI) thread: the one read API. The
/// read runs on a worker thread with the platform reader's own first-answer deadline, the
/// blocked-reader cap, the abandon epoch and the paste size cap, and carries a request id that
/// appears in every log line about it. A copy that is still being written answers from memory. A
/// reader thread that cannot be started answers `TimedOut`.
pub fn request_read(clipboard: &mut dyn Clipboard, kind: ReadKind) -> ReadRequest {
    let id = ReadRequestId::next();
    let reader = prepared_reader(clipboard, kind, id);
    let start = start_reader(clipboard.reads_inline(), id, reader);
    ReadRequest { id, kind, start }
}

/// [`request_read`] in the older `BackgroundTextRead` vocabulary, for the paste paths written
/// against it.
pub fn read_in_background(clipboard: &mut dyn Clipboard, kind: ReadKind) -> StartedRead {
    match request_read(clipboard, kind).start {
        ReadStart::Ready(outcome) => StartedRead::Ready(outcome.into()),
        ReadStart::Pending(pending) => {
            StartedRead::Pending(Box::pin(
                async move { BackgroundTextRead::from(pending.await) },
            ))
        }
    }
}

/// The reader [`request_read`] runs: the platform's, behind the pending-write short-circuit, the
/// blocked-reader cap, the abandon epoch and the size cap.
fn prepared_reader<C: Clipboard + ?Sized>(
    clipboard: &mut C,
    kind: ReadKind,
    id: ReadRequestId,
) -> ClipboardReader {
    // A copy still being written is what the user just copied (#1307); captured now, on the
    // calling thread, so the answer follows the user's order.
    if let Some(pending) = pending_write(kind.target()) {
        return Box::new(move || {
            let mut content = (*pending).clone();
            if kind == ReadKind::TextAndPaths {
                content.html = None;
                content.images = None;
            }
            ReadOutcome::Content(content)
        });
    }
    let reader = clipboard.reader(kind);
    tracked_reader(id, kind, reader)
}

fn start_reader(inline: bool, id: ReadRequestId, reader: ClipboardReader) -> ReadStart {
    if !inline {
        let (tx, rx) = futures::channel::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("clipboard-read".into())
            .spawn(move || {
                let _ = tx.send(reader());
            });
        return match spawned {
            Ok(_) => ReadStart::Pending(Box::pin(async move {
                rx.await.unwrap_or(ReadOutcome::TimedOut)
            })),
            Err(err) => {
                log::warn!(
                    "Unable to start the clipboard read thread for request #{}: {err}",
                    id.0
                );
                ReadStart::Ready(ReadOutcome::TimedOut)
            }
        };
    }
    let _ = (inline, id);
    ReadStart::Ready(reader())
}

/// The older `Option<BackgroundTextReader>` form of a read, for the paste paths that take a
/// reader to run themselves; `None` where the read is an inline snapshot.
fn legacy_reader<C: Clipboard + ?Sized>(
    clipboard: &mut C,
    kind: ReadKind,
) -> Option<BackgroundTextReader> {
    {
        if clipboard.reads_inline() {
            return None;
        }
        let reader = prepared_reader(clipboard, kind, ReadRequestId::next());
        Some(Box::new(move || BackgroundTextRead::from(reader())))
    }
}

/// A BOUNDED BLOCKING read on the calling thread, for tests, the integration harness and the
/// headless TUI only (`script/check_queue_access` fails the build when production code calls it,
/// #1405). It is not a second read implementation: it runs the same reader [`request_read`]
/// would, so the deadline, the blocked-reader cap and the size cap all apply. Production code
/// asks [`request_read`] and acts when the answer lands, holding keys typed meanwhile behind a
/// gate.
pub fn read_outcome_blocking(clipboard: &mut dyn Clipboard, kind: ReadKind) -> ReadOutcome {
    let reader = prepared_reader(clipboard, kind, ReadRequestId::next());
    reader()
}

/// [`read_outcome_blocking`] reduced to the content (empty when the read gave none).
pub fn read_content_blocking(clipboard: &mut dyn Clipboard, kind: ReadKind) -> ClipboardContent {
    read_outcome_blocking(clipboard, kind).into_content()
}

/// [`read_content_blocking`] for [`ReadKind::Full`]: the whole clipboard, for tests and the few
/// callers listed there.
pub fn read_full_blocking(clipboard: &mut dyn Clipboard) -> ClipboardContent {
    read_content_blocking(clipboard, ReadKind::Full)
}

/// Refuses content whose text exceeds `max` bytes, dropping it here on the reader thread so the
/// oversize buffer is freed before it is queued anywhere.
pub fn cap_paste_size(read: ReadOutcome, max: usize) -> ReadOutcome {
    match read {
        ReadOutcome::Content(content) if content.text_bytes() > max => {
            let bytes = content.text_bytes();
            log::warn!("clipboard paste of {bytes} bytes exceeds the {max} byte cap; refused");
            ReadOutcome::TooLarge(bytes)
        }
        other => other,
    }
}

/// Which system clipboard a background write targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardTarget {
    Clipboard,
    /// The Linux primary selection (middle-click paste); other platforms write the clipboard.
    Primary,
}

/// See [`Clipboard::background_writer`].
pub type BackgroundClipboardWriter =
    std::sync::Arc<dyn Fn(ClipboardContent, ClipboardTarget) -> bool + Send + Sync + 'static>;

/// How a background clipboard write ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    Written,
    /// A later copy to the same target had already been written, so this one was dropped.
    Superseded,
    Failed,
}

/// Orders background writes: each write takes a generation when it is started (on the calling
/// thread, so generations follow the user's order) and a write whose generation is older than
/// one already applied to its target is dropped. Writes to one target are serialized.
pub struct WriteOrder {
    next: AtomicU64,
    applied: [Mutex<u64>; 2],
}

impl WriteOrder {
    crate::model_const_fn! {
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            applied: [Mutex::new(0), Mutex::new(0)],
        }
    }
    }

    pub fn next_generation(&self) -> u64 {
        self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    }

    /// Runs `write` unless a later generation has already been applied to `target`.
    pub fn run(
        &self,
        generation: u64,
        target: ClipboardTarget,
        write: impl FnOnce() -> bool,
    ) -> WriteOutcome {
        let mut applied = self.applied[target as usize].lock();
        if *applied > generation {
            return WriteOutcome::Superseded;
        }
        *applied = generation;
        if write() {
            WriteOutcome::Written
        } else {
            WriteOutcome::Failed
        }
    }
}

crate::model_static! {
    static WRITE_ORDER: WriteOrder = WriteOrder::new();
}

/// Writes that were started but have not finished. A read that follows a copy by less than the
/// write takes must see the new text, so reads consult this before the system clipboard.
pub struct PendingWrites {
    writes: Mutex<Vec<(u64, ClipboardTarget, std::sync::Arc<ClipboardContent>)>>,
    /// Notified whenever a write is forgotten, so a waiter (quit) wakes when the last pending
    /// copy lands instead of polling for it.
    changed: Condvar,
}

impl PendingWrites {
    crate::model_const_fn! {
    pub fn new() -> Self {
        Self {
            writes: Mutex::new(Vec::new()),
            changed: Condvar::new(),
        }
    }
    }

    pub fn add(
        &self,
        generation: u64,
        target: ClipboardTarget,
        content: std::sync::Arc<ClipboardContent>,
    ) {
        self.writes.lock().push((generation, target, content));
    }

    /// Forgets the write `generation` (finished, superseded or failed).
    pub fn finish(&self, generation: u64) {
        self.writes.lock().retain(|(g, _, _)| *g != generation);
        self.changed.notify_all();
    }

    /// Blocks until no write to `target` is unfinished, or `timeout` passes (a give-up net for a
    /// wedged selection owner). Returns whether none remain. Woken by [`Self::finish`].
    pub fn wait_until_idle(&self, target: ClipboardTarget, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut writes = self.writes.lock();
        while writes.iter().any(|(_, t, _)| *t == target) {
            if self.changed.wait_until(&mut writes, deadline).timed_out() {
                return !writes.iter().any(|(_, t, _)| *t == target);
            }
        }
        true
    }

    /// The newest unfinished write to `target`, if any.
    pub fn latest(&self, target: ClipboardTarget) -> Option<std::sync::Arc<ClipboardContent>> {
        self.writes
            .lock()
            .iter()
            .filter(|(_, t, _)| *t == target)
            .max_by_key(|(g, _, _)| *g)
            .map(|(_, _, c)| c.clone())
    }
}

crate::model_static! {
    static PENDING_WRITES: PendingWrites = PendingWrites::new();
}

/// The content of the newest background write to `target` that has not finished yet. Platform
/// reads return this instead of the (stale) system clipboard, so a paste right after a copy
/// pastes the new text.
pub fn pending_write(target: ClipboardTarget) -> Option<std::sync::Arc<ClipboardContent>> {
    PENDING_WRITES.latest(target)
}

/// Waits (up to `timeout`) for the in-flight background copies to `target` to finish; woken by the
/// writer finishing, not by polling. Returns whether none remain.
pub fn wait_for_pending_writes(target: ClipboardTarget, timeout: std::time::Duration) -> bool {
    PENDING_WRITES.wait_until_idle(target, timeout)
}

/// Builds a [`BackgroundClipboardWriter`] backed by ONE long-lived thread that owns ONE platform
/// clipboard handle for the process lifetime and performs every write in submission order. The
/// handle is never dropped after a write: on X11 the selection data is served by the process-global
/// context, and dropping the last handle hands the data to the clipboard manager and tears the
/// context down (`lib/arboard/src/platform/linux/x11.rs`, `impl Drop for Clipboard`).
pub fn serial_writer<H: 'static>(
    open: fn() -> Option<H>,
    write: fn(&mut H, ClipboardContent, ClipboardTarget) -> bool,
) -> BackgroundClipboardWriter {
    type Job = (
        ClipboardContent,
        ClipboardTarget,
        std::sync::mpsc::Sender<bool>,
    );
    let (tx, rx) = std::sync::mpsc::channel::<Job>();
    let spawned = std::thread::Builder::new()
        .name("clipboard-owner".into())
        .spawn(move || {
            let mut handle: Option<H> = None;
            while let Ok((content, target, reply)) = rx.recv() {
                if handle.is_none() {
                    handle = open();
                }
                let ok = handle.as_mut().is_some_and(|h| write(h, content, target));
                let _ = reply.send(ok);
            }
        })
        .is_ok();
    let tx = Mutex::new(tx);
    std::sync::Arc::new(move |content, target| {
        if !spawned {
            return false;
        }
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        if tx.lock().send((content, target, reply_tx)).is_err() {
            return false;
        }
        reply_rx.recv().unwrap_or(false)
    })
}

/// The one write API. Writes `content` on the platform's ordered writer thread (or inline where the
/// platform's write is cheap) and returns `Ok`; `on_done` runs on that thread with the outcome (it
/// is dropped uncalled if the thread cannot be spawned). Every write takes a generation on the
/// calling thread, so the user's copy order wins, and quit waits for them (woken, not polled) via
/// [`wait_for_pending_writes`]. Returns the content back as `Err` only when
/// `PHOSPHOR_SYNC_CLIPBOARD_WRITE=1` forces the synchronous path.
pub fn write_in_background(
    clipboard: &mut dyn Clipboard,
    content: ClipboardContent,
    target: ClipboardTarget,
    on_done: impl FnOnce(WriteOutcome) + Send + 'static,
) -> Result<(), ClipboardContent> {
    {
        if std::env::var_os("PHOSPHOR_SYNC_CLIPBOARD_WRITE").is_some() {
            return Err(content);
        }
        note_own_write(target, &content);
        let Some(writer) = clipboard.background_writer() else {
            // The platform's write is cheap and ordered by construction (it finishes before this
            // returns): do it here so a caller has ONE write API and `on_done` still fires.
            match target {
                ClipboardTarget::Clipboard => clipboard.write(content),
                ClipboardTarget::Primary => clipboard.write_to_primary_clipboard(content),
            }
            on_done(WriteOutcome::Written);
            return Ok(());
        };
        let generation = WRITE_ORDER.next_generation();
        let pending = std::sync::Arc::new(content);
        PENDING_WRITES.add(generation, target, pending.clone());
        if let Err(err) = std::thread::Builder::new()
            .name("clipboard-write".into())
            .spawn(move || {
                let outcome =
                    WRITE_ORDER.run(generation, target, || writer((*pending).clone(), target));
                PENDING_WRITES.finish(generation);
                on_done(outcome);
            })
        {
            PENDING_WRITES.finish(generation);
            log::warn!("Unable to start the clipboard write thread: {err}");
        }
        Ok(())
    }
}

/// A self-contained availability query for a worker thread. See [`Clipboard::availability_probe`].
pub type AvailabilityProbe =
    std::sync::Arc<dyn Fn(ClipboardTarget) -> Availability + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug)]
struct CacheEntry {
    /// The newest answer; [`Availability::Unknown`] until the first one (the menu stays enabled).
    known: Availability,
    /// Bumped by every owner-change event and every write of ours, so a probe that started before
    /// either cannot overwrite a newer answer with a stale one.
    epoch: u64,
    in_flight: bool,
    /// An owner change arrived while a probe was in flight: ask again once it lands.
    rerun: bool,
}

/// The cached availability of each clipboard (#1406). The paste menu reads [`Self::get`] and never
/// queries; the answer is refreshed off the UI thread by owner-change events, by our own writes
/// and, where the platform has no event, when a menu opens. At most one probe per clipboard runs
/// at a time; an event during a probe is coalesced into one re-run.
pub struct AvailabilityCache {
    entries: [Mutex<CacheEntry>; 2],
}

impl AvailabilityCache {
    crate::model_const_fn! {
    pub fn new() -> Self {
        crate::model_const_fn! {
            fn entry() -> Mutex<CacheEntry> {
                Mutex::new(CacheEntry {
                    known: Availability::Unknown,
                    epoch: 0,
                    in_flight: false,
                    rerun: false,
                })
            }
        }
        Self {
            entries: [entry(), entry()],
        }
    }
    }

    fn entry(&self, target: ClipboardTarget) -> &Mutex<CacheEntry> {
        &self.entries[target as usize]
    }

    pub fn get(&self, target: ClipboardTarget) -> Availability {
        self.entry(target).lock().known
    }

    /// Records an answer that needs no probe: our own write, or a cheap inline query.
    pub fn note(&self, target: ClipboardTarget, answer: Availability) {
        let mut entry = self.entry(target).lock();
        entry.known = answer;
        entry.epoch += 1;
    }

    /// The owner changed. Returns whether the caller must start a probe (none is running); when
    /// one is running it is told to run again instead.
    pub fn invalidate(&self, target: ClipboardTarget) -> bool {
        let mut entry = self.entry(target).lock();
        entry.epoch += 1;
        if entry.in_flight {
            entry.rerun = true;
            false
        } else {
            true
        }
    }

    /// Claims the probe slot: the epoch to finish with, or `None` when a probe is already
    /// running (it is asked to run again, so this request is not lost).
    pub fn begin(&self, target: ClipboardTarget) -> Option<u64> {
        let mut entry = self.entry(target).lock();
        if entry.in_flight {
            entry.rerun = true;
            return None;
        }
        entry.in_flight = true;
        entry.rerun = false;
        Some(entry.epoch)
    }

    /// Stores the probe's `answer` unless the clipboard changed since the probe began, and frees
    /// the slot. Returns whether the probe must run again.
    pub fn finish(&self, target: ClipboardTarget, epoch: u64, answer: Availability) -> bool {
        let mut entry = self.entry(target).lock();
        if entry.epoch == epoch {
            entry.known = answer;
        }
        entry.in_flight = false;
        std::mem::take(&mut entry.rerun)
    }
}

crate::model_static! {
    static AVAILABILITY: AvailabilityCache = AvailabilityCache::new();
}

/// Frees the probe slot if the probe panics, so one bad probe cannot stop every later refresh.
struct ProbeSlot<'a> {
    cache: &'a AvailabilityCache,
    target: ClipboardTarget,
    armed: bool,
}

impl Drop for ProbeSlot<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.cache.entry(self.target).lock().in_flight = false;
        }
    }
}

impl AvailabilityCache {
    /// Runs `probe` and stores its answer, repeating while owner changes arrive meanwhile.
    /// Blocks in `probe`: call it from a worker or an event thread, never the UI thread.
    pub fn refresh_blocking(
        &self,
        target: ClipboardTarget,
        probe: &dyn Fn(ClipboardTarget) -> Availability,
    ) {
        while let Some(epoch) = self.begin(target) {
            let mut slot = ProbeSlot {
                cache: self,
                target,
                armed: true,
            };
            let answer = probe(target);
            slot.armed = false;
            if !self.finish(target, epoch, answer) {
                break;
            }
        }
    }
}

/// Owner-change entry point for platform listeners: marks the cached answer stale and re-probes
/// with `probe` on the CALLING thread, which must therefore be an event-listener thread.
pub fn availability_changed(
    target: ClipboardTarget,
    probe: &dyn Fn(ClipboardTarget) -> Availability,
) {
    if AVAILABILITY.invalidate(target) {
        AVAILABILITY.refresh_blocking(target, probe);
    }
}

fn refresh_in(
    cache: &'static AvailabilityCache,
    clipboard: &mut dyn Clipboard,
    target: ClipboardTarget,
) {
    if let Some(probe) = clipboard.availability_probe() {
        if cache.invalidate(target) {
            let spawned = std::thread::Builder::new()
                .name("clipboard-avail".into())
                .spawn(move || cache.refresh_blocking(target, &*probe));
            if let Err(err) = spawned {
                log::warn!("Unable to start the clipboard availability probe: {err}");
            }
        }
        return;
    }
    cache.note(target, clipboard.availability(target));
}

/// Whether the paste menu entry is enabled: the cached answer, never a synchronous query. Unknown
/// (nothing answered yet, or the query failed) counts as "may hold something". Kicks off an
/// asynchronous refresh unless the platform pushes owner-change events.
pub fn cached_has_content(clipboard: &mut dyn Clipboard) -> bool {
    cached_has_content_in(&AVAILABILITY, clipboard)
}

fn cached_has_content_in(cache: &'static AvailabilityCache, clipboard: &mut dyn Clipboard) -> bool {
    if !clipboard.has_availability_events() {
        refresh_in(cache, clipboard, ClipboardTarget::Clipboard);
    }
    !matches!(cache.get(ClipboardTarget::Clipboard), Availability::Empty)
}

/// Our own write defines the clipboard: record it at once instead of waiting for the owner-change
/// event (or a probe) to say so.
fn note_own_write(target: ClipboardTarget, content: &ClipboardContent) {
    AVAILABILITY.note(
        target,
        if content.is_empty() {
            Availability::Empty
        } else {
            Availability::HasContent
        },
    );
}

// Clipboard could contain content with multiple data types at the same type.
#[derive(Debug, Clone, Default)]
pub struct ClipboardContent {
    // Clipboard contains plain string.
    pub plain_text: String,
    // Clipboard contains a list of file paths.
    // Parsed direct from OS clipboard on Mac/Windows, from plain_text on Linux.
    // On Mac/Linux, plain_text may also be populated.
    pub paths: Option<Vec<String>>,
    // Clipboard contains HTML content.
    pub html: Option<String>,
    // Clipboard contains image data (can be multiple images).
    pub images: Option<Vec<ImageData>>,
}

/// Represents image data from the clipboard.
///
/// Contains the raw image bytes and associated MIME type information.
#[derive(Debug, Clone)]
pub struct ImageData {
    /// Raw image data as bytes.
    pub data: Vec<u8>,
    /// MIME type of the image (e.g., "image/png", "image/jpeg").
    pub mime_type: String,
    /// Original filename if available (e.g., "photo.jpg").
    pub filename: Option<String>,
}

impl ClipboardContent {
    /// Bytes of text this content would paste: plain text, HTML and copied paths (images are
    /// bounded by their own limits).
    pub fn text_bytes(&self) -> usize {
        self.plain_text.len()
            + self.html.as_ref().map_or(0, String::len)
            + self
                .paths
                .as_ref()
                .map_or(0, |paths| paths.iter().map(String::len).sum())
    }

    pub fn plain_text(text: String) -> Self {
        Self {
            plain_text: text,
            paths: Default::default(),
            html: Default::default(),
            images: Default::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        let Self {
            plain_text,
            paths,
            html,
            images,
        } = self;
        plain_text.is_empty() && paths.is_none() && html.is_none() && images.is_none()
    }

    pub fn has_image_data(&self) -> bool {
        self.images
            .as_ref()
            .map(|images| !images.is_empty())
            .unwrap_or(false)
    }

    pub fn num_paths(&self) -> usize {
        self.paths.as_ref().map(|paths| paths.len()).unwrap_or(0)
    }

    /// Check if clipboard contains file paths that are not images
    pub fn has_non_image_filepaths(&self) -> bool {
        self.paths
            .as_ref()
            .map(|paths| {
                paths
                    .iter()
                    .any(|path| !crate::clipboard_utils::has_image_extension(path))
            })
            .unwrap_or(false)
    }
}

pub fn should_insert_text_on_paste(content: &ClipboardContent) -> bool {
    // Insert any text content present when:
    // 1. No images at all (neither data nor paths)
    // 2. Has non-image files (mixed content)
    // 3. Has image data but no file paths (direct image paste)
    if !content.has_image_data() && content.num_paths() == 0 {
        return true; // No images at all
    }
    if content.has_non_image_filepaths() {
        return true; // Mixed content - user likely wants text paths
    }
    // Direct image paste - user would still want any text content present (unless paths)
    content.has_image_data() && content.num_paths() == 0
}

/// Stores clipboard content in the heap of this process. Therefore, it is scoped to this process.
/// This is not a proper implementation for a "real" platform. It's useful in tests or as a
/// temporary substitute.
pub struct InMemoryClipboard {
    clipboard_content: Mutex<ClipboardContent>,
}

impl Default for InMemoryClipboard {
    fn default() -> Self {
        Self {
            clipboard_content: Mutex::new(ClipboardContent::plain_text(String::new())),
        }
    }
}

impl Clipboard for InMemoryClipboard {
    fn write(&mut self, contents: ClipboardContent) {
        *self.clipboard_content.lock() = contents;
    }

    fn reader(&mut self, kind: ReadKind) -> ClipboardReader {
        let mut content = self.clipboard_content.lock().clone();
        if kind == ReadKind::TextAndPaths {
            content.html = None;
            content.images = None;
        }
        #[cfg(any(test, feature = "test-util"))]
        if let Some(gate) = deferred_reads::active() {
            // The snapshot is taken now, in typed order; the answer waits for the test.
            return Box::new(move || {
                gate.wait();
                ReadOutcome::Content(content)
            });
        }
        Box::new(move || ReadOutcome::Content(content))
    }

    fn reads_inline(&self) -> bool {
        #[cfg(any(test, feature = "test-util"))]
        if deferred_reads::active().is_some() {
            return false;
        }
        true
    }

    fn availability(&mut self, _target: ClipboardTarget) -> Availability {
        if self.clipboard_content.lock().is_empty() {
            Availability::Empty
        } else {
            Availability::HasContent
        }
    }
}

/// Test support: makes this thread's in-memory clipboard answer reads like a platform whose owner
/// is slow (a worker thread, an answer that arrives later), so a test can type keys while a read
/// is out and then let it land.
#[cfg(any(test, feature = "test-util"))]
pub mod deferred_reads {
    use super::{Condvar, Mutex};
    use std::sync::Arc;

    type Gate = Arc<(Mutex<bool>, Condvar)>;

    thread_local! {
        static ACTIVE: std::cell::RefCell<Option<Gate>> = const { std::cell::RefCell::new(None) };
    }

    pub(super) struct ReadGate(Gate);

    impl ReadGate {
        pub(super) fn wait(&self) {
            let (open, changed) = &*self.0;
            let mut open = open.lock();
            while !*open {
                changed.wait(&mut open);
            }
        }
    }

    pub(super) fn active() -> Option<ReadGate> {
        ACTIVE.with(|active| active.borrow().clone().map(ReadGate))
    }

    /// Reads requested on this thread while this is alive wait for [`Self::release`]. Dropping it
    /// releases them and restores inline reads.
    pub struct ReadDeferral(Gate);

    /// Starts deferring this thread's in-memory clipboard reads.
    pub fn defer() -> ReadDeferral {
        let gate: Gate = Arc::new((Mutex::new(false), Condvar::new()));
        ACTIVE.with(|active| *active.borrow_mut() = Some(gate.clone()));
        ReadDeferral(gate)
    }

    impl ReadDeferral {
        /// Lets every read requested so far answer, and later reads answer inline again.
        pub fn release(&self) {
            ACTIVE.with(|active| *active.borrow_mut() = None);
            *self.0.0.lock() = true;
            self.0.1.notify_all();
        }
    }

    impl Drop for ReadDeferral {
        fn drop(&mut self) {
            self.release();
        }
    }
}

// A shuttle / loom build compiles only the model tests (the ordinary ones build `static`
// trackers in `const` contexts, which the checkers' primitives cannot).
#[cfg(all(test, not(any(shuttle, phosphor_loom))))]
#[path = "clipboard_tests.rs"]
mod tests;

#[cfg(all(test, any(shuttle, phosphor_loom)))]
#[path = "clipboard_model_tests.rs"]
mod model_tests;
