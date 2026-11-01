use parking_lot::Mutex;

/// A Clipboard that can read and write strings. Each platform must implement this trait to support
/// writing to a clipboard.
pub trait Clipboard: 'static {
    fn write(&mut self, contents: ClipboardContent);

    fn read(&mut self) -> ClipboardContent;

    /// True once if the most recent read failed because the clipboard owner did not answer in time
    /// (as opposed to the clipboard having nothing to read), clearing the flag. Lets a paste tell
    /// the user it failed instead of silently pasting nothing (#1173).
    fn take_read_failure(&mut self) -> bool {
        false
    }

    /// Whether the clipboard holds anything at all. Platforms should override this with a probe
    /// that does not decode images; the default reads (and decodes) everything.
    fn has_content(&mut self) -> bool {
        !self.read().is_empty()
    }

    /// Like `read`, but `images` is always `None`. For callers that never use image data, so
    /// platforms can skip the (potentially large) image read and decode.
    fn read_without_images(&mut self) -> ClipboardContent {
        let mut content = self.read();
        content.images = None;
        content
    }

    /// Plain text and copied-file paths only (`html` and `images` are `None`). For paste, which
    /// uses nothing else, so platforms can skip the HTML and image probes when text is present.
    fn read_text_and_paths(&mut self) -> ClipboardContent {
        let mut content = self.read_without_images();
        content.html = None;
        content
    }

    /// A self-contained reader for the clipboard's text and copied paths that can run on another
    /// thread, or `None` when the platform's clipboard read is cheap enough (or not thread-safe
    /// enough) to stay on the main thread. A multi-MB X11 selection is transferred in INCR chunks
    /// by a possibly slow owner, which froze the UI for seconds (#1255).
    fn background_text_reader(&mut self) -> Option<BackgroundTextReader> {
        None
    }

    /// Like [`Self::background_text_reader`] but for a full [`Clipboard::read`]: text, copied paths,
    /// HTML and images. For the input editor's paste, which may attach an image. `None` keeps the
    /// read on the main thread (#1300).
    fn background_full_reader(&mut self) -> Option<BackgroundTextReader> {
        None
    }

    /// A self-contained writer that can run on another thread, or `None` when the platform's
    /// clipboard write is cheap and must stay on the calling thread. An X11/Wayland or Win32
    /// clipboard owner can stall a write, which froze the UI (#1307). Returns `true` on success.
    fn background_writer(&mut self) -> Option<BackgroundClipboardWriter> {
        None
    }

    /// Just the plain text of the clipboard (empty if none).
    fn read_plain_text(&mut self) -> String {
        self.read().plain_text
    }

    /// Writes to the primary clipboard, used to support the Primary Selection Protocol (middle-click paste).
    ///
    /// NOTE: For platforms that don't support the primary clipboard, it writes to the default clipboard instead.
    fn write_to_primary_clipboard(&mut self, contents: ClipboardContent) {
        self.write(contents)
    }

    /// Reads the primary clipboard, used to support the Primary Selection Protocol (middle-click paste).
    /// This reads from the default clipboard on platforms other than Linux.
    fn read_from_primary_clipboard(&mut self) -> ClipboardContent {
        self.read()
    }

    #[cfg(target_family = "wasm")]
    fn save(&mut self, content: ClipboardContent);
}

/// The result of a [`Clipboard::background_text_reader`] read.
pub enum BackgroundTextRead {
    /// The text and copied paths (empty content when the clipboard held no text).
    Content(ClipboardContent),
    /// The clipboard owner never answered (the same condition as `take_read_failure`).
    TimedOut,
}

/// See [`Clipboard::background_text_reader`].
pub type BackgroundTextReader = Box<dyn FnOnce() -> BackgroundTextRead + Send + 'static>;

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
    next: std::sync::atomic::AtomicU64,
    applied: [Mutex<u64>; 2],
}

impl WriteOrder {
    pub const fn new() -> Self {
        Self {
            next: std::sync::atomic::AtomicU64::new(0),
            applied: [Mutex::new(0), Mutex::new(0)],
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

static WRITE_ORDER: WriteOrder = WriteOrder::new();

/// Writes `content` on a background thread when the platform supports it and returns `Ok`;
/// `on_done` runs on that thread with the outcome (it is dropped uncalled if the thread cannot be
/// spawned). Returns the content back as `Err` when the caller should write synchronously. `PHOSPHOR_SYNC_CLIPBOARD_WRITE=1`
/// forces the synchronous path.
pub fn write_in_background(
    clipboard: &mut dyn Clipboard,
    content: ClipboardContent,
    target: ClipboardTarget,
    on_done: impl FnOnce(WriteOutcome) + Send + 'static,
) -> Result<(), ClipboardContent> {
    #[cfg(target_family = "wasm")]
    {
        let _ = (clipboard, target, on_done);
        return Err(content);
    }
    #[cfg(not(target_family = "wasm"))]
    {
        if std::env::var_os("PHOSPHOR_SYNC_CLIPBOARD_WRITE").is_some() {
            return Err(content);
        }
        let Some(writer) = clipboard.background_writer() else {
            return Err(content);
        };
        let generation = WRITE_ORDER.next_generation();
        if let Err(err) = std::thread::Builder::new()
            .name("clipboard-write".into())
            .spawn(move || {
                let outcome = WRITE_ORDER.run(generation, target, || writer(content, target));
                on_done(outcome);
            })
        {
            log::warn!("Unable to start the clipboard write thread: {err}");
        }
        Ok(())
    }
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

    fn read(&mut self) -> ClipboardContent {
        self.clipboard_content.lock().clone()
    }

    #[cfg(target_family = "wasm")]
    fn save(&mut self, _content: ClipboardContent) {}
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
