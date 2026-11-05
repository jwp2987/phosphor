use std::ffi::CStr;

use anyhow::Result;
use cocoa::base::id;
use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeString};
use objc2_foundation::{NSArray, NSData, NSString, ns_string};
use warpui_core::clipboard::{
    Availability, ClipboardContent, ClipboardReader, ClipboardTarget, ImageData, ReadKind,
    ReadOutcome,
};

unsafe extern "C" {
    fn getFilePathsFromPasteboard() -> id;
}

pub struct Clipboard(Retained<NSPasteboard>);

unsafe impl Send for Clipboard {}

impl Clipboard {
    pub fn new() -> Result<Self> {
        // `generalPasteboard` is documented to always return the shared
        // pasteboard, so objc2 models it as a non-null `Retained`.
        Ok(Clipboard(NSPasteboard::generalPasteboard()))
    }
}

fn pasteboard_type_for_image_mime_type(mime_type: &str) -> Option<&'static NSString> {
    match mime_type {
        "image/png" => Some(ns_string!("public.png")),
        "image/jpeg" => Some(ns_string!("public.jpeg")),
        "image/gif" => Some(ns_string!("public.gif")),
        "image/webp" => Some(ns_string!("public.webp")),
        "image/svg+xml" => Some(ns_string!("public.svg-image")),
        _ => None,
    }
}

impl crate::Clipboard for Clipboard {
    /// On the calling (main) thread: macOS has no `background_writer`, so `write_in_background`
    /// writes inline (#1782: every NSPasteboard access is on the main thread).
    fn write(&mut self, contents: ClipboardContent) {
        unsafe {
            let nsstr = NSString::from_str(&contents.plain_text);
            self.0
                .declareTypes_owner(&NSArray::from_slice(&[NSPasteboardTypeString]), None);
            self.0.setString_forType(&nsstr, NSPasteboardTypeString);

            if let Some(html) = contents.html {
                let nsstr = NSString::from_str(&html);
                self.0
                    .addTypes_owner(&NSArray::from_slice(&[NSPasteboardTypeHTML]), None);
                self.0.setString_forType(&nsstr, NSPasteboardTypeHTML);
            }

            if let Some(images) = contents.images {
                for image in images {
                    let Some(pasteboard_type) =
                        pasteboard_type_for_image_mime_type(&image.mime_type)
                    else {
                        continue;
                    };
                    // `NSData::with_bytes` copies the image bytes into a +1-retained
                    // NSData. The pasteboard retains it in `setData:forType:`, and the
                    // `Retained` releases our reference when it drops at the loop end.
                    let data = NSData::with_bytes(&image.data);
                    self.0
                        .addTypes_owner(&NSArray::from_slice(&[pasteboard_type]), None);
                    self.0.setData_forType(Some(&data), pasteboard_type);
                }
            }
        }
    }

    fn availability(&mut self, _target: ClipboardTarget) -> Availability {
        // `availableTypeFromArray:` consults the declared type list only; it never asks the owner
        // to materialise (or us to decode) any data, so a lazy promise from another app cannot
        // block it (#1306). Covers the types `read` consumes. Runs on the calling (main) thread,
        // like every NSPasteboard access (#1782): no `availability_probe` on macOS.
        unsafe {
            let types = NSArray::from_slice(&[
                NSPasteboardTypeString,
                NSPasteboardTypeHTML,
                ns_string!("public.file-url"),
                ns_string!("NSFilenamesPboardType"),
                ns_string!("public.png"),
                ns_string!("public.jpeg"),
                ns_string!("public.gif"),
                ns_string!("public.webp"),
                ns_string!("public.svg-image"),
                ns_string!("com.compuserve.gif"),
            ]);
            if self.0.availableTypeFromArray(&types).is_some() {
                Availability::HasContent
            } else {
                Availability::Empty
            }
        }
    }

    fn reader(&mut self, kind: ReadKind) -> ClipboardReader {
        // #1782 (owner decision 2026-11-05): the pasteboard is read HERE, on the calling thread,
        // which must be the main thread (the GUI's clipboard lives on the main-thread-only
        // AppContext; the TUI asks from its main thread), the way Terminal.app and iTerm read it.
        // An off-main request is logged as an error (see `read_on_main_thread_with`). #1305 had moved this read onto a
        // worker so a slow lazy provider could not hold the UI; r18u26 then died of SIGBUS 1 ms
        // after such a worker read overlapped main-thread AppKit work. Accepted cost (DECLINED.md):
        // a slow lazy provider can briefly freeze the window. The returned closure touches no
        // AppKit object: it runs on the request's worker with the copied text and bytes, does
        // the off-thread part (the image filenames parsed out of the HTML), and the paste gate,
        // ordering, size cap and request log of `request_read` work as before.
        let read = read_on_main_thread(kind);
        Box::new(move || match read {
            Ok(content) => ReadOutcome::Content(finish_read(content)),
            Err(outcome) => outcome,
        })
    }
}

/// How long a read requested off the main thread waits for the main thread to perform it, while
/// an NSApplication runs. The GUI's own requests come from the main thread and never wait; an
/// off-main request is a bug (logged as an error), and this bounds it so it cannot deadlock.
const OFF_MAIN_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

unsafe extern "C" {
    /// AppKit's global application instance; nil until `NSApplication.sharedApplication` runs.
    /// Not nil in the TUI: `mark_process_as_background_only` creates it, although the TUI's main
    /// thread never drains the main queue. The TUI is safe because all its reads start on its main
    /// thread (read inline); this nil check only covers processes with no NSApplication at all.
    #[allow(non_upper_case_globals)]
    static NSApp: id;
}

/// Whether an NSApplication exists, i.e. a main thread that drains the main dispatch queue.
fn app_running() -> bool {
    // SAFETY: a plain read of AppKit's global pointer.
    unsafe { !NSApp.is_null() }
}

/// Whether the calling thread is the process's main thread.
fn on_main_thread() -> bool {
    // SAFETY: `pthread_main_np` only reads the calling thread's identity.
    unsafe { libc::pthread_main_np() == 1 }
}

/// The one NSPasteboard read (#1782): text and paths, plus HTML and image bytes for a full read.
/// Main thread only.
fn read_pasteboard(kind: ReadKind) -> Result<ClipboardContent, ReadOutcome> {
    objc2::rc::autoreleasepool(|_| match Clipboard::new() {
        Ok(mut clipboard) => Ok(match kind {
            ReadKind::TextAndPaths => clipboard.read_text_and_paths(),
            // No separate primary selection on macOS: it is the pasteboard.
            ReadKind::Full | ReadKind::Primary => clipboard.read(),
        }),
        Err(_) => Err(ReadOutcome::Unavailable),
    })
}

fn read_on_main_thread(kind: ReadKind) -> Result<ClipboardContent, ReadOutcome> {
    let started = std::time::Instant::now();
    debug_assert!(
        on_main_thread() || !app_running(),
        "a pasteboard read was requested off the main thread while the app runs (#1782)"
    );
    let read = read_on_main_thread_with(
        kind,
        on_main_thread,
        app_running,
        read_pasteboard,
        OFF_MAIN_WAIT,
    );
    // The accepted cost of #1782, measured: how long the main thread spent in the read. appci
    // big_paste judges it against its macOS paste-read budget.
    warpui_core::input_order::log_hop("pasteboard_read", || {
        format!(
            "kind={kind:?} ms={:.2} bytes={} ok={}",
            started.elapsed().as_secs_f64() * 1000.0,
            read.as_ref().map_or(0, ClipboardContent::text_bytes),
            read.is_ok()
        )
    });
    read
}

/// Runs `read` on the main thread: at once when the caller is the main thread. Off the main
/// thread: with no NSApplication at all (no AppKit main-thread work to race) it reads in place;
/// otherwise (including the headless TUI, which has an NSApplication but never drains the main
/// queue, so its reads must start on its main thread) it is an
/// error, queued on the main dispatch queue with a bounded wait (`TimedOut` after `wait`). The
/// main thread never waits for a worker here, so this cannot deadlock.
fn read_on_main_thread_with(
    kind: ReadKind,
    on_main: impl Fn() -> bool,
    app_running: impl Fn() -> bool,
    read: fn(ReadKind) -> Result<ClipboardContent, ReadOutcome>,
    wait: std::time::Duration,
) -> Result<ClipboardContent, ReadOutcome> {
    if on_main() || !app_running() {
        return read(kind);
    }
    let caller = std::thread::current();
    log::error!(
        "clipboard read ({kind:?}) requested off the main thread by thread '{}' while the app \
         runs: queued for the main thread (#1782; requests must come from the main thread)",
        caller.name().unwrap_or("<unnamed>")
    );
    let (tx, rx) = std::sync::mpsc::channel();
    dispatch::Queue::main().exec_async(move || {
        let _ = tx.send(read(kind));
    });
    rx.recv_timeout(wait).unwrap_or_else(|_| {
        log::error!(
            "clipboard read ({kind:?}) requested off the main thread: the main thread did not \
             perform it within {wait:?}"
        );
        Err(ReadOutcome::TimedOut)
    })
}

/// The off-main-thread part of a read: the images' filenames, parsed out of the copied HTML.
fn finish_read(mut content: ClipboardContent) -> ClipboardContent {
    let filename = content
        .html
        .as_deref()
        .filter(|html| !html.is_empty())
        .and_then(crate::clipboard_utils::extract_filename_from_html);
    if let Some(images) = content.images.as_mut() {
        for image in images.iter_mut().filter(|image| image.filename.is_none()) {
            image.filename = filename.clone();
        }
    }
    content
}

/// An NSString's UTF-8 text, or `""` when `UTF8String` returns null (it may, for a string that
/// cannot be converted): these reads run on the main thread (#1782), where a null `CStr` would
/// take the whole app down.
fn ns_string_to_string(string: &NSString) -> String {
    // SAFETY: `UTF8String` returns null or a NUL-terminated buffer that lives at least as long
    // as the enclosing autorelease pool; it is copied out at once.
    unsafe {
        let ptr = string.UTF8String();
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

/// The read primitives: main thread only (#1782; see `Clipboard::reader`).
impl Clipboard {
    pub(crate) fn read_text_and_paths(&mut self) -> ClipboardContent {
        // Skips the HTML and image reads (the latter decodes and copies every image flavor).
        unsafe { self.read_text_and_paths_inner() }
    }

    pub(crate) fn read(&mut self) -> ClipboardContent {
        unsafe {
            let mut content = self.read_text_and_paths_inner();

            let html = self.0.stringForType(NSPasteboardTypeHTML);
            if let Some(html) = html.as_deref() {
                content.html = Some(ns_string_to_string(html))
            }

            // Try to read image data from clipboard
            content.images = self.read_image_data_from_pasteboard();

            content
        }
    }
}

impl Clipboard {
    unsafe fn read_text_and_paths_inner(&mut self) -> ClipboardContent {
        unsafe {
            // Try getting file paths from the clipboard. If we end up with an empty
            // array of file paths, fallback to getting the string from the pasteboard.
            let file_paths = &*getFilePathsFromPasteboard().cast::<NSArray<NSString>>();
            let available_paths = file_paths.count();

            let text = self.0.stringForType(NSPasteboardTypeString);
            let mut content = ClipboardContent::plain_text(
                text.as_deref().map(ns_string_to_string).unwrap_or_default(),
            );

            if available_paths > 0 {
                content.paths = Some(
                    (0..available_paths)
                        .map(|i| {
                            // #1784: `len()` counts UTF-16 units, not the UTF-8 bytes
                            // `UTF8String` returns: a non-ASCII path was cut mid-character into
                            // a `String` that is not UTF-8. Read up to the NUL instead.
                            ns_string_to_string(&file_paths.objectAtIndex(i))
                        })
                        .collect::<Vec<String>>(),
                );
            }
            content
        }
    }
}

impl Clipboard {
    /// Reads image data from the macOS pasteboard (main thread only, #1782).
    ///
    /// Checks for supported image formats and returns the first available image
    /// data found, prioritizing common web-compatible formats.
    fn read_image_data_from_pasteboard(&self) -> Option<Vec<ImageData>> {
        unsafe {
            // Check for common image types on macOS pasteboard
            // macOS pasteboard type identifiers for supported image formats
            // Ordered by preference for web compatibility
            let supported_pasteboard_types = [
                ns_string!("public.png"),
                ns_string!("public.jpeg"),
                ns_string!("public.gif"),
                ns_string!("public.webp"),
                ns_string!("public.svg-image"),
                ns_string!("com.compuserve.gif"),
            ];

            let mut images = Vec::new();

            for pasteboard_type in supported_pasteboard_types {
                if let Some(data) = self.0.dataForType(pasteboard_type) {
                    let length = data.len();
                    if length > 0 {
                        let mime_type = match ns_string_to_string(pasteboard_type).as_str() {
                            "public.png" => "image/png",
                            "public.jpeg" => "image/jpeg",
                            "public.gif" | "com.compuserve.gif" => "image/gif",
                            "public.webp" => "image/webp",
                            "public.svg-image" => "image/svg+xml",
                            _ => "image/unknown",
                        };

                        // The filename is parsed out of the HTML off the main thread
                        // (`finish_read`); only the bytes are copied here.
                        let filename = None;

                        images.push(ImageData {
                            data: data.to_vec(),
                            mime_type: mime_type.to_string(),
                            filename,
                        });
                    }
                }
            }

            if images.is_empty() {
                None
            } else {
                Some(images)
            }
        }
    }
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
