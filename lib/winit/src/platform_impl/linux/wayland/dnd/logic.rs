//! The parts of the Wayland drop target (Phosphor #2085) that need no compositor: the kill
//! switch, the choice of a mime type, the `text/uri-list` parser and the order of the events one
//! drag produces.
//!
//! Only `std` is used here, on purpose: the file is also compiled into Phosphor's own unit tests
//! (`crates/warpui/src/windowing/winit/linux/wayland_dnd_tests.rs`, by `#[path]`), because the
//! vendored winit is a separate workspace whose tests no CI job runs. Keep it formatted with
//! Phosphor's `.rustfmt.toml` for the same reason.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// `PHOSPHOR_WAYLAND_DND=0` turns the drop target off (no data device is created).
pub const KILL_SWITCH_ENV: &str = "PHOSPHOR_WAYLAND_DND";

/// The one mime type a drop is accepted for (RFC 2483).
pub const URI_LIST_MIME: &str = "text/uri-list";

/// A uri-list larger than this is not a list of dragged files; it is read to its end and ignored.
pub const MAX_URI_LIST_BYTES: usize = 1024 * 1024;

/// Only the exact value `0` is the kill switch; unset, empty or anything else leaves it on.
pub fn dnd_enabled(value: Option<&OsStr>) -> bool {
    value != Some(OsStr::new("0"))
}

/// The offered mime type to accept and to ask the data in, or `None` to refuse the drag.
///
/// The returned string is the source's own spelling: it is what `wl_data_offer.accept` and
/// `wl_data_offer.receive` must be given. A plain `text/uri-list` wins over a spelling with
/// parameters or other letter case. Text types are not accepted: a dragged piece of text is not a
/// file, and accepting it would show a drop as possible that then inserts nothing.
pub fn choose_mime(offered: &[String]) -> Option<String> {
    offered
        .iter()
        .find(|mime| mime.as_str() == URI_LIST_MIME)
        .or_else(|| offered.iter().find(|mime| is_uri_list_mime(mime)))
        .cloned()
}

fn is_uri_list_mime(mime: &str) -> bool {
    mime.split(';')
        .next()
        .is_some_and(|essence| essence.trim().eq_ignore_ascii_case(URI_LIST_MIME))
}

/// The local paths in a `text/uri-list` payload, in order.
///
/// Lines end in CRLF (a bare LF, a bare CR and a NUL some sources append are taken too). A line
/// that starts with `#` is a comment. Only `file:` URIs with no host or the host `localhost`
/// count; everything else (other schemes, other hosts, relative paths) is skipped, so one odd
/// entry does not lose the rest. The path is percent-decoded to bytes and is NOT resolved against
/// the file system: no symlink is followed and nothing blocks on a slow mount.
pub fn parse_uri_list(data: &[u8]) -> Vec<PathBuf> {
    data.split(|byte| matches!(byte, b'\n' | b'\r' | 0))
        .filter_map(parse_uri_line)
        .collect()
}

fn parse_uri_line(line: &[u8]) -> Option<PathBuf> {
    let line = trim_ascii_whitespace(line);
    if line.is_empty() || line[0] == b'#' {
        return None;
    }
    const SCHEME: &[u8] = b"file:";
    if line.len() < SCHEME.len() || !line[..SCHEME.len()].eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let rest = &line[SCHEME.len()..];
    let encoded_path = match rest.strip_prefix(b"//") {
        // file://host/path: the host ends at the next slash
        Some(after) => {
            let slash = after.iter().position(|byte| *byte == b'/')?;
            let host = &after[..slash];
            if !(host.is_empty() || host.eq_ignore_ascii_case(b"localhost")) {
                return None;
            }
            &after[slash..]
        }
        // file:/path (RFC 8089 allows the form with no authority)
        None => rest,
    };
    let path = percent_decode(encoded_path);
    if path.first() != Some(&b'/') || path.contains(&0) {
        return None;
    }
    Some(PathBuf::from(OsString::from_vec(path)))
}

/// `<[u8]>::trim_ascii`, which is newer than the vendored winit's minimum Rust version.
fn trim_ascii_whitespace(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if !first.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    while let [rest @ .., last] = bytes {
        if !last.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    bytes
}

/// `%XX` becomes the byte; a `%` that is not followed by two hex digits stays as it is.
fn percent_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let decoded = match (input[i], input.get(i + 1), input.get(i + 2)) {
            (b'%', Some(high), Some(low)) => hex_value(*high).zip(hex_value(*low)),
            _ => None,
        };
        match decoded {
            Some((high, low)) => {
                out.push(high * 16 + low);
                i += 3;
            }
            None => {
                out.push(input[i]);
                i += 1;
            }
        }
    }
    out
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// What the Wayland glue must do next for one drag, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `WindowEvent::HoveredFile`
    HoveredFile(PathBuf),
    /// `WindowEvent::HoveredFileCancelled`
    HoveredFileCancelled,
    /// Report the pointer position of the drop (`WindowEvent::CursorMoved`): no pointer motion
    /// reaches a window during a drag, and the app drops at its last pointer position.
    DropPosition,
    /// `WindowEvent::DroppedFile`
    DroppedFile(PathBuf),
    /// Ask the source for the data once more (`wl_data_offer.receive`).
    RequestData,
    /// The drop is over. `accepted`: files were taken (`wl_data_offer.finish`); otherwise the
    /// offer is only destroyed, which tells the source the drop was not used.
    Finish { accepted: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Hovering,
    Dropped,
    Left,
}

/// One accepted drag over a window, from `wl_data_device.enter` to its end.
///
/// The data is asked for once, when the drag enters (the paths are what `HoveredFile` reports),
/// and that answer is reused for the drop. A drop that comes before the answer waits for it; a
/// drop whose answer held no file asks one more time, for a source that only gives its data after
/// the drop. A read is never cut short: every pipe is read to its end, so a source is never left
/// writing into a closed pipe.
#[derive(Debug)]
pub struct Drag {
    phase: Phase,
    reading: bool,
    paths: Vec<PathBuf>,
    asked_again: bool,
    finished: bool,
}

impl Drag {
    /// `reading`: the data was asked for and its pipe is being read.
    pub fn new(reading: bool) -> Self {
        Self {
            phase: Phase::Hovering,
            reading,
            paths: Vec::new(),
            asked_again: false,
            finished: false,
        }
    }

    /// The pipe reached its end with `data` (empty when the read failed or was too large).
    pub fn on_data(&mut self, data: &[u8]) -> Vec<Effect> {
        self.reading = false;
        match self.phase {
            Phase::Left => Vec::new(),
            Phase::Hovering => {
                self.paths = parse_uri_list(data);
                self.paths
                    .iter()
                    .cloned()
                    .map(Effect::HoveredFile)
                    .collect()
            }
            Phase::Dropped => {
                self.paths = parse_uri_list(data);
                self.complete_drop()
            }
        }
    }

    /// `wl_data_device.drop`
    pub fn on_drop(&mut self) -> Vec<Effect> {
        if self.phase != Phase::Hovering {
            return Vec::new();
        }
        self.phase = Phase::Dropped;
        if self.reading {
            Vec::new()
        } else {
            self.complete_drop()
        }
    }

    /// `wl_data_device.leave`. After a drop it means nothing: the drop is completed by its data.
    pub fn on_leave(&mut self) -> Vec<Effect> {
        if self.phase != Phase::Hovering {
            return Vec::new();
        }
        self.phase = Phase::Left;
        vec![Effect::HoveredFileCancelled]
    }

    /// The drag is still over the window and was not dropped.
    pub fn is_hovering(&self) -> bool {
        self.phase == Phase::Hovering
    }

    /// Nothing more will come of this drag: it can be forgotten.
    pub fn is_done(&self) -> bool {
        self.finished || (self.phase == Phase::Left && !self.reading)
    }

    fn complete_drop(&mut self) -> Vec<Effect> {
        if self.finished {
            return Vec::new();
        }
        if self.paths.is_empty() && !self.asked_again {
            self.asked_again = true;
            self.reading = true;
            return vec![Effect::RequestData];
        }
        self.finished = true;
        let mut effects = Vec::with_capacity(self.paths.len() + 2);
        if !self.paths.is_empty() {
            effects.push(Effect::DropPosition);
            effects.extend(self.paths.iter().cloned().map(Effect::DroppedFile));
        }
        effects.push(Effect::Finish {
            accepted: !self.paths.is_empty(),
        });
        effects
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStrExt;

    use super::*;

    fn paths(data: &[u8]) -> Vec<String> {
        parse_uri_list(data)
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    }

    fn strings(mimes: &[&str]) -> Vec<String> {
        mimes.iter().map(|mime| mime.to_string()).collect()
    }

    fn file(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn only_zero_is_the_kill_switch() {
        assert!(dnd_enabled(None));
        assert!(!dnd_enabled(Some(OsStr::new("0"))));
        for on in ["", "1", "00", "false", "off", " 0"] {
            assert!(dnd_enabled(Some(OsStr::new(on))), "{on:?}");
        }
    }

    #[test]
    fn uri_list_is_the_only_accepted_mime_type() {
        let offered = strings(&["text/plain", "text/uri-list", "text/plain;charset=utf-8"]);
        assert_eq!(choose_mime(&offered), Some("text/uri-list".to_string()));
        assert_eq!(
            choose_mime(&strings(&["text/plain", "UTF8_STRING", "STRING"])),
            None
        );
        assert_eq!(
            choose_mime(&strings(&["text/x-moz-url", "text/html"])),
            None
        );
        assert_eq!(
            choose_mime(&strings(&["text/uri-list-extra", "application/x-uri-list"])),
            None
        );
        assert_eq!(choose_mime(&[]), None);
    }

    #[test]
    fn the_chosen_mime_type_keeps_the_spelling_of_the_source() {
        // what is accepted and received must be a string the source offered
        let offered = strings(&["Text/URI-List; charset=utf-8", "text/plain"]);
        assert_eq!(
            choose_mime(&offered),
            Some("Text/URI-List; charset=utf-8".to_string())
        );
        // the plain spelling wins wherever it stands
        let offered = strings(&["text/uri-list;charset=utf-8", "text/uri-list"]);
        assert_eq!(choose_mime(&offered), Some("text/uri-list".to_string()));
    }

    #[test]
    fn one_file_uri_with_crlf() {
        assert_eq!(paths(b"file:///home/u/a.txt\r\n"), ["/home/u/a.txt"]);
        assert_eq!(paths(b"file:///home/u/a.txt"), ["/home/u/a.txt"]);
    }

    #[test]
    fn a_space_is_percent_twenty_and_stays_one_path() {
        assert_eq!(
            paths(b"file:///w/dnd%20file%20a.txt\r\n"),
            ["/w/dnd file a.txt"]
        );
        // a source that did not encode the space: the line is still one path
        assert_eq!(
            paths(b"file:///w/dnd file a.txt\r\n"),
            ["/w/dnd file a.txt"]
        );
    }

    #[test]
    fn unicode_is_decoded_from_percent_bytes_and_taken_raw() {
        let encoded = b"file:///w/%C3%A9t%C3%A9/%E6%96%87%E4%BB%B6.txt\r\n";
        assert_eq!(paths(encoded), ["/w/\u{e9}t\u{e9}/\u{6587}\u{4ef6}.txt"]);
        assert_eq!(
            paths("file:///w/\u{e9}t\u{e9}.txt\r\n".as_bytes()),
            ["/w/\u{e9}t\u{e9}.txt"]
        );
    }

    #[test]
    fn a_name_that_is_not_utf8_keeps_its_bytes() {
        let got = parse_uri_list(b"file:///w/a%FFb\r\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].as_os_str().as_bytes(), b"/w/a\xffb");
    }

    #[test]
    fn several_files_keep_their_order() {
        let data = b"file:///w/a%20b.txt\r\nfile:///w/c.txt\r\nfile:///w/d.txt\r\n";
        assert_eq!(paths(data), ["/w/a b.txt", "/w/c.txt", "/w/d.txt"]);
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let data = b"# dragged from a file manager\r\n\r\nfile:///w/a.txt\r\n  # indented comment\r\nfile:///w/b.txt\r\n";
        assert_eq!(paths(data), ["/w/a.txt", "/w/b.txt"]);
        assert_eq!(paths(b""), Vec::<String>::new());
        assert_eq!(paths(b"\r\n\r\n"), Vec::<String>::new());
        assert_eq!(paths(b"# only a comment\r\n"), Vec::<String>::new());
    }

    #[test]
    fn a_hash_inside_a_name_is_not_a_comment() {
        assert_eq!(paths(b"file:///w/a%23b.txt\r\n"), ["/w/a#b.txt"]);
        assert_eq!(paths(b"file:///w/a#b.txt\r\n"), ["/w/a#b.txt"]);
    }

    #[test]
    fn bare_lf_bare_cr_and_a_trailing_nul_end_a_line() {
        assert_eq!(
            paths(b"file:///w/a.txt\nfile:///w/b.txt\n"),
            ["/w/a.txt", "/w/b.txt"]
        );
        assert_eq!(
            paths(b"file:///w/a.txt\rfile:///w/b.txt"),
            ["/w/a.txt", "/w/b.txt"]
        );
        assert_eq!(paths(b"file:///w/a.txt\r\n\0"), ["/w/a.txt"]);
        assert_eq!(paths(b"file:///w/a.txt\0"), ["/w/a.txt"]);
    }

    #[test]
    fn other_schemes_are_skipped_and_the_files_beside_them_kept() {
        let data = b"https://example.org/a.txt\r\nfile:///w/a.txt\r\nsftp://h/w/b.txt\r\nmailto:x@example.org\r\n/w/bare/path\r\n";
        assert_eq!(paths(data), ["/w/a.txt"]);
        assert_eq!(
            paths(b"filex:///w/a.txt\r\nfile\r\nfil\r\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn host_forms() {
        assert_eq!(paths(b"file:///w/a.txt\r\n"), ["/w/a.txt"], "empty host");
        assert_eq!(
            paths(b"file://localhost/w/a.txt\r\n"),
            ["/w/a.txt"],
            "localhost"
        );
        assert_eq!(
            paths(b"file://LocalHost/w/a.txt\r\n"),
            ["/w/a.txt"],
            "host names ignore case"
        );
        assert_eq!(
            paths(b"file:/w/a.txt\r\n"),
            ["/w/a.txt"],
            "no authority at all"
        );
        assert_eq!(
            paths(b"FILE:///w/a.txt\r\n"),
            ["/w/a.txt"],
            "scheme names ignore case"
        );
        assert_eq!(
            paths(b"file://otherhost/w/a.txt\r\n"),
            Vec::<String>::new(),
            "another machine"
        );
        assert_eq!(
            paths(b"file://localhost\r\n"),
            Vec::<String>::new(),
            "a host and no path"
        );
        assert_eq!(
            paths(b"file:w/a.txt\r\n"),
            Vec::<String>::new(),
            "a relative path"
        );
        assert_eq!(
            paths(b"file:\r\n"),
            Vec::<String>::new(),
            "nothing after the scheme"
        );
    }

    #[test]
    fn percent_edge_cases() {
        assert_eq!(paths(b"file:///w/100%25.txt\r\n"), ["/w/100%.txt"]);
        assert_eq!(
            paths(b"file:///w/a%2Fb\r\n"),
            ["/w/a/b"],
            "an encoded slash is a slash"
        );
        assert_eq!(
            paths(b"file:///w/a%zzb%4\r\n"),
            ["/w/a%zzb%4"],
            "not hex: kept as typed"
        );
        assert_eq!(
            paths(b"file:///w/a%2fb%2F\r\n"),
            ["/w/a/b/"],
            "hex digits in either case"
        );
        assert_eq!(
            paths(b"file:///w/a%00b\r\n"),
            Vec::<String>::new(),
            "a NUL is no path"
        );
        assert_eq!(
            paths(b"file://%2Fw/a\r\n"),
            Vec::<String>::new(),
            "the host is not decoded"
        );
    }

    #[test]
    fn hover_then_drop_reuses_the_data_read_on_enter() {
        let mut drag = Drag::new(true);
        assert!(drag.is_hovering() && !drag.is_done());
        assert_eq!(
            drag.on_data(b"file:///w/a%20b.txt\r\nfile:///w/c.txt\r\n"),
            [
                Effect::HoveredFile(file("/w/a b.txt")),
                Effect::HoveredFile(file("/w/c.txt"))
            ]
        );
        assert!(drag.is_hovering() && !drag.is_done());
        assert_eq!(
            drag.on_drop(),
            [
                Effect::DropPosition,
                Effect::DroppedFile(file("/w/a b.txt")),
                Effect::DroppedFile(file("/w/c.txt")),
                Effect::Finish { accepted: true },
            ]
        );
        assert!(drag.is_done() && !drag.is_hovering());
        // the leave a compositor sends after the drop, and a repeated drop, change nothing
        assert!(drag.on_leave().is_empty());
        assert!(drag.on_drop().is_empty());
        assert!(drag.on_data(b"file:///w/late.txt\r\n").is_empty());
    }

    #[test]
    fn a_drop_before_the_data_waits_for_it() {
        let mut drag = Drag::new(true);
        assert!(
            drag.on_drop().is_empty(),
            "nothing is dropped before the paths are known"
        );
        assert!(!drag.is_done());
        assert!(
            drag.on_leave().is_empty(),
            "the leave after a drop cancels nothing"
        );
        assert_eq!(
            drag.on_data(b"file:///w/a.txt\r\n"),
            [
                Effect::DropPosition,
                Effect::DroppedFile(file("/w/a.txt")),
                Effect::Finish { accepted: true }
            ]
        );
        assert!(drag.is_done());
    }

    #[test]
    fn a_leave_cancels_the_hover_once_and_the_read_still_ends() {
        let mut drag = Drag::new(true);
        assert_eq!(
            drag.on_data(b"file:///w/a.txt\r\n"),
            [Effect::HoveredFile(file("/w/a.txt"))]
        );
        assert_eq!(drag.on_leave(), [Effect::HoveredFileCancelled]);
        assert!(drag.is_done());
        assert!(drag.on_leave().is_empty());
        assert!(
            drag.on_drop().is_empty(),
            "a drag that left cannot be dropped"
        );

        // the drag leaves while its data is still being read: the read goes on to its end and
        // then reports nothing
        let mut drag = Drag::new(true);
        assert_eq!(drag.on_leave(), [Effect::HoveredFileCancelled]);
        assert!(!drag.is_done(), "the pipe is still open");
        assert!(drag.on_data(b"file:///w/a.txt\r\n").is_empty());
        assert!(drag.is_done());
    }

    #[test]
    fn a_drop_with_no_file_asks_once_more_and_then_gives_up() {
        // a source that only hands its data over after the drop
        let mut drag = Drag::new(true);
        assert!(drag.on_data(b"").is_empty());
        assert_eq!(drag.on_drop(), [Effect::RequestData]);
        assert!(!drag.is_done());
        assert_eq!(
            drag.on_data(b"file:///w/a.txt\r\n"),
            [
                Effect::DropPosition,
                Effect::DroppedFile(file("/w/a.txt")),
                Effect::Finish { accepted: true }
            ]
        );
        assert!(drag.is_done());

        // nothing usable either time: one second read, never a third, and the offer is not
        // finished as accepted
        let mut drag = Drag::new(true);
        assert!(drag.on_data(b"https://example.org/\r\n").is_empty());
        assert_eq!(drag.on_drop(), [Effect::RequestData]);
        assert_eq!(
            drag.on_data(b"https://example.org/\r\n"),
            [Effect::Finish { accepted: false }]
        );
        assert!(drag.is_done());

        // the drop came first and the first answer was empty
        let mut drag = Drag::new(true);
        assert!(drag.on_drop().is_empty());
        assert_eq!(drag.on_data(b""), [Effect::RequestData]);
        assert_eq!(drag.on_data(b""), [Effect::Finish { accepted: false }]);
        assert!(drag.is_done());
    }

    #[test]
    fn a_drag_whose_data_could_not_be_asked_for_on_enter_asks_at_the_drop() {
        let mut drag = Drag::new(false);
        assert!(drag.is_hovering() && !drag.is_done());
        assert_eq!(drag.on_drop(), [Effect::RequestData]);
        assert_eq!(
            drag.on_data(b"file:///w/a.txt\r\n"),
            [
                Effect::DropPosition,
                Effect::DroppedFile(file("/w/a.txt")),
                Effect::Finish { accepted: true }
            ]
        );
        // and one that leaves is done at once: no pipe is open
        let mut drag = Drag::new(false);
        assert_eq!(drag.on_leave(), [Effect::HoveredFileCancelled]);
        assert!(drag.is_done());
    }

    #[test]
    fn the_drop_position_comes_before_the_files_and_finish_is_last() {
        let mut drag = Drag::new(true);
        drag.on_data(b"file:///w/a.txt\r\nfile:///w/b.txt\r\n");
        let effects = drag.on_drop();
        assert_eq!(effects.first(), Some(&Effect::DropPosition));
        assert_eq!(effects.last(), Some(&Effect::Finish { accepted: true }));
        let finishes = effects
            .iter()
            .filter(|e| matches!(e, Effect::Finish { .. }))
            .count();
        assert_eq!(finishes, 1);
    }
}
