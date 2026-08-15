//! Best-effort startup probe for the host terminal's default colors.
//!
//! Writes OSC 10 (default foreground) and OSC 11 (default background) queries
//! followed by a DA1 (`CSI c`) sentinel and reads the replies from stdin. DA1
//! is answered by virtually every terminal and terminals answer queries in
//! order, so its reply marks "all color replies (if any) have arrived" without
//! waiting out the full deadline on terminals that ignore OSC color queries.
//! The deadline bounds startup latency when even DA1 goes unanswered.
//!
//! The probe runs before the TUI driver's input reader exists, so it must not
//! leave stdin blocked: reads are non-blocking behind a `poll` loop, and any
//! reply bytes that arrive after the deadline are left for the driver's
//! crossterm parser to consume (which discards unrecognized sequences).

use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::time::Duration;

use ratatui::crossterm::terminal;

/// How long the probe waits for the terminal's replies before giving up.
/// Local terminals answer in single-digit milliseconds; keeping this short
/// bounds startup latency on terminals (or transports) that never answer.
const PROBE_DEADLINE: Duration = Duration::from_millis(100);

/// How long to keep reading for the DA1 sentinel *after* the OSC 11 background
/// reply has already been parsed.
///
/// Returning the moment the colour arrives leaves the DA1 reply sitting unread
/// in the tty, where it is delivered to whatever reads stdin next -- the user's
/// shell, which prints it as `^[[?62;22;52c` after the TUI exits (#580).
/// Terminals emit the two replies back to back, so this only has to cover the
/// gap between them, not a fresh round trip. Bounded separately from
/// [`PROBE_DEADLINE`] so a terminal that answers OSC 11 and never answers DA1
/// costs 20ms rather than the full probe deadline.
const DA1_DRAIN_GRACE: Duration = Duration::from_millis(20);

/// An 8-bit RGB color reported by the terminal.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ProbedRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ProbedRgb {
    /// Whether the color reads as light, using the Rec. 601 luma weights (the
    /// same classification Codex and amp apply to terminal backgrounds).
    fn is_light(self) -> bool {
        let luma =
            0.299 * f32::from(self.r) + 0.587 * f32::from(self.g) + 0.114 * f32::from(self.b);
        luma > 128.0
    }
}

/// The terminal's default colors as reported by [`probe_terminal_colors`].
/// Either field is `None` when the terminal did not answer that query.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProbedTerminalColors {
    pub fg: Option<ProbedRgb>,
    pub bg: Option<ProbedRgb>,
}

impl ProbedTerminalColors {
    /// Classifies the probed background, falling back to the `COLORFGBG`
    /// environment variable when the terminal did not answer the OSC query.
    /// Callers should treat [`BackgroundLuminance::Unknown`] as dark: it is
    /// the safer default, and matches the TUI's historical dark-only styling.
    pub fn background_luminance(&self) -> BackgroundLuminance {
        match self.bg {
            Some(bg) if bg.is_light() => BackgroundLuminance::Light,
            Some(_) => BackgroundLuminance::Dark,
            None => match std::env::var("COLORFGBG") {
                Ok(value) => colorfgbg_luminance(&value),
                Err(_) => BackgroundLuminance::Unknown,
            },
        }
    }
}

/// Light/dark classification of the terminal's default background.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BackgroundLuminance {
    Light,
    Dark,
    /// The terminal answered neither the OSC 11 query nor set `COLORFGBG`.
    Unknown,
}

/// Queries the host terminal for its default foreground/background colors.
///
/// Returns empty colors (rather than an error) whenever the probe cannot run
/// or the terminal does not answer: stdin/stdout is not a tty, raw mode is
/// unavailable, or the deadline passes without replies. Raw mode is entered
/// for the probe's duration (so replies are neither echoed nor line-buffered)
/// and restored to its prior state before returning.
pub fn probe_terminal_colors() -> ProbedTerminalColors {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return ProbedTerminalColors::default();
    }
    let was_raw = terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw && terminal::enable_raw_mode().is_err() {
        return ProbedTerminalColors::default();
    }
    let colors = run_probe().unwrap_or_default();
    if !was_raw {
        let _ = terminal::disable_raw_mode();
    }
    colors
}

/// Writes the queries and reads replies until the DA1 sentinel or deadline.
#[cfg(unix)]
fn run_probe() -> io::Result<ProbedTerminalColors> {
    use std::io::Write;

    use instant::Instant;

    let mut stdout = io::stdout();
    stdout.write_all(b"\x1b]10;?\x07\x1b]11;?\x07\x1b[c")?;
    stdout.flush()?;

    let _nonblocking = NonBlockingStdin::enable()?;
    let deadline = Instant::now() + PROBE_DEADLINE;
    let mut replies = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if !poll_stdin(deadline - now)? {
            break;
        }
        // SAFETY: reads into a valid, live local buffer of the given length.
        let read =
            unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr().cast(), chunk.len()) };
        match read {
            0 => break,
            read if read < 0 => {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    continue;
                }
                return Err(error);
            }
            read => {
                replies.extend_from_slice(&chunk[..read as usize]);
                if contains_da1_reply(&replies) {
                    break;
                }
            }
        }
    }
    Ok(parse_replies(&replies))
}

/// Non-unix hosts skip the probe: conhost does not answer OSC 10/11, and a
/// non-blocking console read needs a different mechanism (see Codex's
/// `GetConsoleScreenBufferInfoEx` fallback for a possible follow-up). Callers
/// land on the dark default via [`BackgroundLuminance::Unknown`].
#[cfg(not(unix))]
fn run_probe() -> io::Result<ProbedTerminalColors> {
    Ok(ProbedTerminalColors::default())
}

/// Restores stdin's original file-status flags on drop.
#[cfg(unix)]
struct NonBlockingStdin {
    original_flags: libc::c_int,
}

#[cfg(unix)]
impl NonBlockingStdin {
    fn enable() -> io::Result<Self> {
        // SAFETY: fcntl on the always-valid stdin fd with valid arguments.
        let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: as above; sets the flags just read plus O_NONBLOCK.
        if unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            original_flags: flags,
        })
    }
}

#[cfg(unix)]
impl Drop for NonBlockingStdin {
    fn drop(&mut self) {
        // SAFETY: restores the flags read in `enable` on the stdin fd.
        unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, self.original_flags) };
    }
}

/// Waits up to `timeout` for stdin to become readable.
#[cfg(unix)]
fn poll_stdin(timeout: Duration) -> io::Result<bool> {
    let mut pollfd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_ms = timeout.as_millis().clamp(1, libc::c_int::MAX as u128) as libc::c_int;
    // SAFETY: polls a single valid pollfd for the stdin fd.
    let ready = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
    if ready < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            // Treat an interrupted poll as "maybe readable": the non-blocking
            // read reports WouldBlock and the loop re-polls.
            return Ok(true);
        }
        return Err(error);
    }
    Ok(ready > 0)
}

// The reply parsers below are pure and platform-independent, but only the
// unix probe produces reply bytes; keep them compiled for tests on every
// platform so the parsing logic stays covered on non-unix CI.

/// Extracts the OSC 10/11 color replies from the probe's raw reply bytes.
#[cfg(any(unix, test))]
fn parse_replies(replies: &[u8]) -> ProbedTerminalColors {
    let text = String::from_utf8_lossy(replies);
    ProbedTerminalColors {
        fg: parse_osc_color_reply(&text, 10),
        bg: parse_osc_color_reply(&text, 11),
    }
}

/// Finds the reply to an `OSC <code> ; ?` query and parses its color payload.
/// Replies look like `ESC ] 11 ; rgb:RRRR/GGGG/BBBB` terminated by BEL or ST.
#[cfg(any(unix, test))]
fn parse_osc_color_reply(text: &str, code: u8) -> Option<ProbedRgb> {
    let prefix = format!("\x1b]{code};");
    let payload = &text[text.find(&prefix)? + prefix.len()..];
    let end = payload.find(['\x07', '\x1b']).unwrap_or(payload.len());
    parse_x11_color(&payload[..end])
}

/// Parses an XParseColor-style payload: `rgb:R/G/B` with 1–4 hex digits per
/// component, or `rgba:` with a trailing alpha component that is ignored.
#[cfg(any(unix, test))]
fn parse_x11_color(payload: &str) -> Option<ProbedRgb> {
    let components = payload
        .strip_prefix("rgba:")
        .or_else(|| payload.strip_prefix("rgb:"))?;
    let mut components = components.split('/');
    let r = parse_scaled_component(components.next()?)?;
    let g = parse_scaled_component(components.next()?)?;
    let b = parse_scaled_component(components.next()?)?;
    Some(ProbedRgb { r, g, b })
}

/// Scales a 1–4 digit hex component to 8 bits.
#[cfg(any(unix, test))]
fn parse_scaled_component(component: &str) -> Option<u8> {
    if component.is_empty() || component.len() > 4 {
        return None;
    }
    let value = u32::from_str_radix(component, 16).ok()?;
    let max = (1u32 << (4 * component.len() as u32)) - 1;
    Some((value * 255 / max) as u8)
}

/// Whether the bytes contain a DA1 reply (`CSI ? ... c`), the probe's
/// end-of-replies sentinel.
#[cfg(any(unix, test))]
fn contains_da1_reply(replies: &[u8]) -> bool {
    let mut search = replies;
    while let Some(start) = find_subsequence(search, b"\x1b[?") {
        let rest = &search[start + 3..];
        match rest.iter().find(|byte| byte.is_ascii_alphabetic()) {
            Some(b'c') => return true,
            Some(_) => search = rest,
            None => return false,
        }
    }
    false
}

#[cfg(any(unix, test))]
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Classifies a single probed background, falling back to the `COLORFGBG`
/// environment variable when the terminal did not answer the OSC query. This
/// is the live-probe counterpart of [`ProbedTerminalColors::background_luminance`]
/// (which classifies a combined fg/bg startup probe); callers should likewise
/// treat [`BackgroundLuminance::Unknown`] as dark.
pub fn background_luminance(background: Option<ProbedRgb>) -> BackgroundLuminance {
    match background {
        Some(background) if background.is_light() => BackgroundLuminance::Light,
        Some(_) => BackgroundLuminance::Dark,
        None => match std::env::var("COLORFGBG") {
            Ok(value) => colorfgbg_luminance(&value),
            Err(_) => BackgroundLuminance::Unknown,
        },
    }
}

type TuiProbeEligibilityProvider = dyn Fn() -> bool + Send + Sync;
type TuiProbeQueryWriter = dyn Fn(&mut dyn Write) -> io::Result<()> + Send + Sync;
type TuiProbeReplyReader = dyn Fn() -> Option<ProbedRgb> + Send + Sync;

/// Registration for a focus-triggered background re-probe on the TUI input
/// reader thread: that thread is the sole owner of stdin, so it alone can
/// write an OSC 11 query and read the reply without racing normal input.
pub struct TuiProbe {
    pub(super) is_enabled: Arc<TuiProbeEligibilityProvider>,
    pub(super) write_query: Arc<TuiProbeQueryWriter>,
    pub(super) read_reply: Arc<TuiProbeReplyReader>,
    pub(super) results: async_channel::Sender<Option<ProbedRgb>>,
}

impl TuiProbe {
    /// Creates a reader-thread probe registration. `is_enabled` is polled
    /// before every focus-gained event and must be cheap (e.g. an atomic
    /// load); `write_query`/`read_reply` run inline on the reader thread when
    /// a probe fires, so they must not block indefinitely.
    pub fn new(
        is_enabled: impl Fn() -> bool + Send + Sync + 'static,
        results: async_channel::Sender<Option<ProbedRgb>>,
        write_query: impl Fn(&mut dyn Write) -> io::Result<()> + Send + Sync + 'static,
        read_reply: impl Fn() -> Option<ProbedRgb> + Send + Sync + 'static,
    ) -> Self {
        Self {
            is_enabled: Arc::new(is_enabled),
            write_query: Arc::new(write_query),
            read_reply: Arc::new(read_reply),
            results,
        }
    }
}

/// Queries the host terminal for its current default background color, for
/// use by a live (mid-session) re-probe. Unlike [`probe_terminal_colors`],
/// this does not manage raw mode itself: the TUI is already in raw mode by
/// the time a live probe can fire, so callers own that lifecycle.
///
/// Returns `None` whenever the probe cannot run or the terminal does not
/// answer within `deadline`: stdin/stdout is not a tty, the write fails, or
/// the deadline passes without a reply.
pub fn probe_terminal_background() -> Option<ProbedRgb> {
    let mut stdout = io::stdout();
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return None;
    }
    let was_raw = terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw && terminal::enable_raw_mode().is_err() {
        return None;
    }
    let background = write_terminal_background_query(&mut stdout)
        .map(|()| read_terminal_background_reply(PROBE_DEADLINE))
        .unwrap_or_default();
    if !was_raw {
        let _ = terminal::disable_raw_mode();
    }
    background
}

/// Writes and flushes the OSC 11 background query and DA1 sentinel.
#[cfg(unix)]
pub fn write_terminal_background_query<W: Write + ?Sized>(writer: &mut W) -> io::Result<()> {
    writer.write_all(b"\x1b]11;?\x07\x1b[c")?;
    writer.flush()
}

/// Non-unix hosts skip the query because conhost does not answer OSC 11.
#[cfg(not(unix))]
pub fn write_terminal_background_query<W: Write + ?Sized>(_writer: &mut W) -> io::Result<()> {
    Ok(())
}

/// Reads the background reply until a complete OSC 11 reply, the DA1
/// sentinel, or `deadline_duration` elapses.
///
/// Callers must already own stdin (raw mode enabled, no concurrent reader).
#[cfg(unix)]
pub fn read_terminal_background_reply(deadline_duration: Duration) -> Option<ProbedRgb> {
    if !io::stdin().is_terminal() {
        return None;
    }
    read_background_reply(deadline_duration).unwrap_or_default()
}

/// Non-unix hosts skip the reply read because a non-blocking console read
/// needs a different mechanism. Callers land on the dark default via
/// [`BackgroundLuminance::Unknown`].
#[cfg(not(unix))]
pub fn read_terminal_background_reply(_deadline_duration: Duration) -> Option<ProbedRgb> {
    None
}

/// What the background read loop should do after appending a chunk.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg(any(unix, test))]
enum ProbeRead {
    /// The DA1 sentinel arrived: every reply the terminal intends to send has
    /// been consumed, so stopping here leaves nothing behind in the tty.
    Sentinel,
    /// The OSC 11 reply just completed. The answer is in hand, but the DA1
    /// reply is still in flight and must be drained before returning (#580).
    Background(ProbedRgb),
    /// Nothing conclusive yet.
    Pending,
}

/// Decides what a freshly-extended reply buffer means.
///
/// Split out of the read loop so the ordering rule is testable without a tty:
/// **the sentinel outranks the colour**. Returning as soon as the colour parses
/// is the bug behind #580 — it leaves the DA1 reply unread, and the user's
/// shell prints it as `^[[?62;22;52c` once the TUI exits.
#[cfg(any(unix, test))]
fn classify_probe_read(replies: &[u8], background_known: bool) -> ProbeRead {
    if contains_da1_reply(replies) {
        return ProbeRead::Sentinel;
    }
    if !background_known
        && let Some(rgb) = parse_complete_background_reply(replies)
    {
        return ProbeRead::Background(rgb);
    }
    ProbeRead::Pending
}

#[cfg(unix)]
fn read_background_reply(deadline_duration: Duration) -> io::Result<Option<ProbedRgb>> {
    use instant::Instant;

    let _nonblocking = NonBlockingStdin::enable()?;
    let mut deadline = Instant::now() + deadline_duration;
    let mut replies = Vec::new();
    let mut chunk = [0u8; 512];
    let mut background = None;
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if !poll_stdin(deadline - now)? {
            break;
        }
        // SAFETY: reads into a valid, live local buffer of the given length.
        let read =
            unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr().cast(), chunk.len()) };
        match read {
            0 => break,
            read if read < 0 => {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    continue;
                }
                return Err(error);
            }
            read => {
                replies.extend_from_slice(&chunk[..read as usize]);
                match classify_probe_read(&replies, background.is_some()) {
                    ProbeRead::Sentinel => break,
                    ProbeRead::Background(rgb) => {
                        background = Some(rgb);
                        // Keep reading for the sentinel rather than returning
                        // here, but only briefly -- the answer we came for is
                        // already in hand, so this is purely draining (#580).
                        let grace = Instant::now() + DA1_DRAIN_GRACE;
                        if grace < deadline {
                            deadline = grace;
                        }
                    }
                    ProbeRead::Pending => {}
                }
            }
        }
    }
    Ok(background.or_else(|| parse_osc_color_reply(&String::from_utf8_lossy(&replies), 11)))
}

/// Extracts a *complete*, BEL- or ST-terminated OSC 11 background reply —
/// unlike [`parse_osc_color_reply`], an unterminated tail never matches, so a
/// partial read never gets misparsed as the final answer.
#[cfg(any(unix, test))]
fn parse_complete_background_reply(replies: &[u8]) -> Option<ProbedRgb> {
    let text = String::from_utf8_lossy(replies);
    let prefix = "\x1b]11;";
    let payload = &text[text.find(prefix)? + prefix.len()..];
    let end = payload.find('\x07').or_else(|| payload.find("\x1b\\"))?;
    parse_x11_color(&payload[..end])
}

/// Classifies the background from a `COLORFGBG` value (e.g. `"15;0"`): the
/// last `;`-separated field is the background's ANSI palette index, and
/// indices 0–6 and 8 are the dark palette entries. This is a coarse fallback
/// — only rxvt-likes set the variable reliably — so unparseable values are
/// `Unknown` rather than guessed.
fn colorfgbg_luminance(value: &str) -> BackgroundLuminance {
    let Some(background) = value.split(';').next_back() else {
        return BackgroundLuminance::Unknown;
    };
    match background.parse::<u8>() {
        Ok(index) if index <= 6 || index == 8 => BackgroundLuminance::Dark,
        Ok(_) => BackgroundLuminance::Light,
        Err(_) => BackgroundLuminance::Unknown,
    }
}

#[cfg(test)]
#[path = "terminal_probe_tests.rs"]
mod tests;
