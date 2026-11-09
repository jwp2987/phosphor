//! Best-effort probe for the host terminal's default colors, run on the TUI
//! driver's input-reader thread.
//!
//! The reader thread is the sole owner of stdin. A probe writes an OSC 11 (default
//! background) query and then keeps reading the SAME crossterm event stream the
//! keyboard uses: nothing here reads stdin behind crossterm's back, so a key typed
//! while the probe waits is an ordinary event, forwarded in order. What a terminal
//! answers to the query is not an event crossterm understands -- it parses
//! `ESC ] 1 1 ; rgb:... BEL` as Alt-`]` followed by plain characters -- so a small
//! [`ReplyFilter`] recognises exactly that shape, by content, in the event stream
//! and swallows it; every other event passes through untouched and in order. The
//! reply to the old DA1 sentinel needs no handling: crossterm consumes it as an
//! internal event and never shows it, so it cannot leak as typed bytes (#580), and
//! the sentinel is no longer sent.
//!
//! The probe stops when the OSC 11 reply completes. Terminals that never answer cost
//! the probe deadline and fall back to `COLORFGBG`, then to dark; a reply that
//! arrives after the deadline is still swallowed (once, and only while it is
//! outstanding) and delivered when it lands (#1748, #1749).

use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use instant::Instant;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// How long the startup probe waits for the terminal's OSC 11 reply before giving up.
/// Local terminals answer in single-digit milliseconds; the probe runs on the reader
/// thread after the first frame, so this bounds only how long the colors may arrive
/// late. A terminal that never answers costs this once and falls back to `COLORFGBG`.
// give-up-net: startup colour probe deadline on the reader thread: on expiry the colours stay unknown (COLORFGBG fallback); typed keys are forwarded in order, never discarded
pub(super) const PROBE_DEADLINE: Duration = Duration::from_millis(100);

/// How long after a probe's deadline a still-outstanding reply is recognised and
/// swallowed if it arrives. After this it is no longer expected, and a reply that
/// shows up anyway is forwarded as the keys it looks like (no worse than before).
// give-up-net: how long a late colour reply is still swallowed; only stops swallowing, releases held keys to the editor, writes nothing
const LATE_REPLY_WINDOW: Duration = Duration::from_secs(2);

/// The longest the filter withholds a key that might be the start of a reply (an Esc or
/// Alt-`]`) while the next event does not come. Replies arrive in one burst, so a gap
/// this long means the key was typed: it is handed on, in order.
// give-up-net: hands a withheld Esc/Alt-] and the keys after it on to the app in order; never drops or reorders a key
pub(super) const REPLY_HOLD_NET: Duration = Duration::from_millis(50);

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

// The reply parsers below are pure and platform-independent.

/// Extracts the OSC 10/11 color replies from raw reply bytes (tests only: the
/// live probe recognises replies in the event stream, see [`ReplyFilter`]).
#[cfg(test)]
fn parse_replies(replies: &[u8]) -> ProbedTerminalColors {
    let text = String::from_utf8_lossy(replies);
    ProbedTerminalColors {
        fg: parse_osc_color_reply(&text, 10),
        bg: parse_osc_color_reply(&text, 11),
    }
}

/// Finds the reply to an `OSC <code> ; ?` query and parses its color payload.
/// Replies look like `ESC ] 11 ; rgb:RRRR/GGGG/BBBB` terminated by BEL or ST.
#[cfg(test)]
fn parse_osc_color_reply(text: &str, code: u8) -> Option<ProbedRgb> {
    let prefix = format!("\x1b]{code};");
    let payload = &text[text.find(&prefix)? + prefix.len()..];
    let end = payload.find(['\x07', '\x1b']).unwrap_or(payload.len());
    parse_x11_color(&payload[..end])
}

/// Parses an XParseColor-style payload: `rgb:R/G/B` with 1–4 hex digits per
/// component, or `rgba:` with a trailing alpha component that is ignored.
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
fn parse_scaled_component(component: &str) -> Option<u8> {
    if component.is_empty() || component.len() > 4 {
        return None;
    }
    let value = u32::from_str_radix(component, 16).ok()?;
    let max = (1u32 << (4 * component.len() as u32)) - 1;
    Some((value * 255 / max) as u8)
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

/// Registration for the background probes run on the TUI input-reader thread:
/// that thread is the sole owner of stdin, so it alone can write an OSC 11
/// query and watch for the reply without racing normal input. It probes once at
/// startup (if [`Self::with_startup_probe`] was set; the colors may land after the
/// first frame, which uses `COLORFGBG` or the dark default until they do) and on
/// every focus-gained event while `is_enabled` says so.
pub struct TuiProbe {
    pub(super) is_enabled: Arc<TuiProbeEligibilityProvider>,
    pub(super) write_query: Arc<TuiProbeQueryWriter>,
    pub(super) results: async_channel::Sender<Option<ProbedRgb>>,
    /// The reply deadline of a focus-triggered probe.
    pub(super) live_deadline: Duration,
    /// Whether the reader thread also probes once when it starts.
    pub(super) startup: bool,
}

impl TuiProbe {
    /// Creates a reader-thread probe registration. `is_enabled` is polled
    /// before every focus-gained event and must be cheap (e.g. an atomic
    /// load); `write_query` runs inline on the reader thread when a probe
    /// fires, so it must not block indefinitely. `live_deadline` bounds how
    /// long a focus-triggered probe waits for the reply.
    pub fn new(
        is_enabled: impl Fn() -> bool + Send + Sync + 'static,
        results: async_channel::Sender<Option<ProbedRgb>>,
        write_query: impl Fn(&mut dyn Write) -> io::Result<()> + Send + Sync + 'static,
        live_deadline: Duration,
    ) -> Self {
        Self {
            is_enabled: Arc::new(is_enabled),
            write_query: Arc::new(write_query),
            results,
            live_deadline,
            startup: false,
        }
    }

    /// Also probes once, with [`PROBE_DEADLINE`], when the reader thread starts.
    /// A startup miss sends nothing (it is not a missed focus probe); a reply that
    /// lands is sent as `Some`.
    pub fn with_startup_probe(mut self, startup: bool) -> Self {
        self.startup = startup;
        self
    }
}

/// Writes and flushes the OSC 11 background query. (No DA1 sentinel: crossterm
/// consumes its reply internally, so it can neither be waited for nor leak.)
#[cfg(unix)]
pub fn write_terminal_background_query<W: Write + ?Sized>(writer: &mut W) -> io::Result<()> {
    writer.write_all(b"\x1b]11;?\x07")?;
    writer.flush()
}

/// Non-unix hosts skip the query because conhost does not answer OSC 11.
#[cfg(not(unix))]
pub fn write_terminal_background_query<W: Write + ?Sized>(_writer: &mut W) -> io::Result<()> {
    Ok(())
}

/// A color reply recognised (and swallowed) by the [`ReplyFilter`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Reply {
    // The reader never asks for the foreground (only tests do); the variant keeps the
    // filter able to swallow an OSC 10 reply when one is expected.
    #[allow(dead_code)]
    Foreground(ProbedRgb),
    Background(ProbedRgb),
}

/// What the filter hands back for an input event, in the order events arrived.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Emit<E> {
    /// An ordinary event: forward it.
    Pass(E),
    /// A color reply: it was swallowed, and these are its colors.
    Reply(Reply),
}

/// How an input event looked on the wire, as far as the filter needs to know.
/// `None` (an event with no byte image the filter models: Enter, arrows, mouse,
/// resize, ...) can never be part of a reply and so ends any partial match.
pub(super) type Wire = Option<Vec<u8>>;

/// The bytes a key event stands for, or `None` for an event that cannot be part of
/// an OSC color reply. crossterm parses `ESC ] 1 1 ; rgb:... BEL` as Alt-`]`, then
/// the characters, then Ctrl-G (BEL) or Alt-`\` (ST), so those are the only shapes
/// modelled: plain/shifted characters, Esc, Alt-<char>, Ctrl-G.
pub(super) fn event_wire(event: &Event) -> Wire {
    let Event::Key(KeyEvent {
        code,
        modifiers,
        kind,
        ..
    }) = event
    else {
        return None;
    };
    if *kind != KeyEventKind::Press {
        return None;
    }
    let alt = modifiers.contains(KeyModifiers::ALT);
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let other =
        modifiers.difference(KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::CONTROL);
    if !other.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(5);
    if alt {
        bytes.push(0x1b);
    }
    match code {
        KeyCode::Esc if !ctrl => bytes.push(0x1b),
        KeyCode::Char('g') if ctrl => bytes.push(0x07),
        KeyCode::Char(c) if !ctrl => {
            let mut utf8 = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
        }
        _ => return None,
    }
    Some(bytes)
}

/// Where the byte-level recogniser is inside a possible reply.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Scan {
    Ground,
    /// Saw `ESC`.
    Esc,
    /// Saw `ESC ]`.
    Osc,
    /// Saw `ESC ] <digits>`.
    Code(u8),
    /// Saw `ESC ] <10|11> ;` and are collecting the payload.
    Body {
        code: u8,
        payload: Vec<u8>,
        saw_esc: bool,
    },
}

/// What one byte did to the recogniser.
enum Step {
    /// The byte is not part of a reply and nothing is partially matched.
    Pass,
    /// A possible reply is in progress: withhold the event.
    Hold,
    /// A complete reply ended on this byte.
    Complete(Reply),
    /// The byte broke a partial match (or nothing matched and an ESC is open).
    Reject,
}

/// Which replies the filter still expects. At most one of each (#1748, #1749): a
/// reply is swallowed once and then no longer expected, so a second one, or one
/// nobody asked for, is typed input.
#[derive(Clone, Copy, Debug, Default)]
struct Outstanding {
    foreground_until: Option<Instant>,
    background_until: Option<Instant>,
}

impl Outstanding {
    fn expects(&self, code: u8, now: Instant) -> bool {
        let until = match code {
            10 => self.foreground_until,
            11 => self.background_until,
            _ => None,
        };
        until.is_some_and(|until| now < until)
    }

    fn any(&self, now: Instant) -> bool {
        self.expects(10, now) || self.expects(11, now)
    }

    fn clear(&mut self, code: u8) {
        match code {
            10 => self.foreground_until = None,
            11 => self.background_until = None,
            _ => {}
        }
    }
}

/// Recognises the terminal's OSC 10 / OSC 11 color replies in a stream of input
/// events and swallows exactly those; every other event passes through, in the order
/// it arrived, with nothing withheld longer than [`REPLY_HOLD_NET`] without a
/// reason. It only ever matches a reply that is OUTSTANDING (asked for and not yet
/// received, within its window), by content and once, so with nothing outstanding
/// it is a pass-through that costs a branch per event.
///
/// Matching is incremental across events (a reply split over any number of reads
/// still matches). Events the filter withholds because they could start or continue
/// a reply are released, in order, the moment the next event shows they were not one
/// (or by [`Self::release_held`] when the hold net fires).
pub(super) struct ReplyFilter<E> {
    scan: Scan,
    /// Events withheld because they could be (the start of) a reply, oldest first.
    held: Vec<E>,
    outstanding: Outstanding,
}

impl<E> Default for ReplyFilter<E> {
    fn default() -> Self {
        Self {
            scan: Scan::Ground,
            held: Vec::new(),
            outstanding: Outstanding::default(),
        }
    }
}

impl<E> ReplyFilter<E> {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Expect the background reply (OSC 11) until `until`.
    pub(super) fn expect_background(&mut self, until: Instant) {
        self.outstanding.background_until = Some(until);
    }

    /// Expect the foreground reply (OSC 10) until `until`.
    #[cfg(test)]
    pub(super) fn expect_foreground(&mut self, until: Instant) {
        self.outstanding.foreground_until = Some(until);
    }

    /// Whether any event is currently withheld.
    pub(super) fn is_holding(&self) -> bool {
        !self.held.is_empty()
    }

    /// Whether any reply is still expected at `now`.
    #[cfg(test)]
    pub(super) fn is_expecting(&self, now: Instant) -> bool {
        self.outstanding.any(now)
    }

    /// Feeds one event with its wire image; returns what to forward, in order.
    pub(super) fn push(&mut self, event: E, wire: Wire, now: Instant) -> Vec<Emit<E>> {
        let mut out = Vec::new();
        // Idle: nothing expected and nothing withheld -- the common case.
        if self.held.is_empty() && !self.outstanding.any(now) {
            out.push(Emit::Pass(event));
            return out;
        }
        let Some(bytes) = wire else {
            // Not modelled on the wire: it ends any partial match.
            self.release_held_into(&mut out);
            out.push(Emit::Pass(event));
            return out;
        };
        match self.feed(&bytes, now) {
            Fed::Hold => self.held.push(event),
            Fed::Pass => out.push(Emit::Pass(event)),
            Fed::Complete(reply) => {
                // The reply is swallowed whole: the withheld events and this one.
                self.held.clear();
                self.scan = Scan::Ground;
                out.push(Emit::Reply(reply));
            }
            Fed::Reject => {
                // The withheld events were typed: release them, then look at this
                // event again from scratch (an Alt-`]` can start a fresh reply).
                self.release_held_into(&mut out);
                match self.feed(&bytes, now) {
                    Fed::Hold => self.held.push(event),
                    Fed::Complete(reply) => out.push(Emit::Reply(reply)),
                    Fed::Pass | Fed::Reject => {
                        self.scan = Scan::Ground;
                        out.push(Emit::Pass(event));
                    }
                }
            }
        }
        out
    }

    /// Hands every withheld event on, oldest first (the hold net, or the end of the
    /// probe): they were typed, not a reply. Resets the partial match.
    pub(super) fn release_held(&mut self) -> Vec<Emit<E>> {
        let mut out = Vec::new();
        self.release_held_into(&mut out);
        out
    }

    fn release_held_into(&mut self, out: &mut Vec<Emit<E>>) {
        self.scan = Scan::Ground;
        out.extend(self.held.drain(..).map(Emit::Pass));
    }

    /// Runs the recogniser over one event's bytes. A reject anywhere rejects the
    /// whole event and leaves the scan in `Ground`.
    fn feed(&mut self, bytes: &[u8], now: Instant) -> Fed {
        for &byte in bytes {
            match self.step(byte, now) {
                Step::Pass | Step::Hold => {}
                Step::Complete(reply) => {
                    // Only the last byte of an event can end a reply; anything after
                    // it in the same event would be typed input glued to the reply,
                    // which no key event models.
                    return Fed::Complete(reply);
                }
                Step::Reject => {
                    self.scan = Scan::Ground;
                    return Fed::Reject;
                }
            }
        }
        if !matches!(self.scan, Scan::Ground) {
            Fed::Hold
        } else {
            Fed::Pass
        }
    }

    fn step(&mut self, byte: u8, now: Instant) -> Step {
        let scan = std::mem::replace(&mut self.scan, Scan::Ground);
        match scan {
            Scan::Ground => {
                if byte == 0x1b {
                    self.scan = Scan::Esc;
                    Step::Hold
                } else {
                    Step::Pass
                }
            }
            Scan::Esc => {
                if byte == b']' {
                    self.scan = Scan::Osc;
                    Step::Hold
                } else {
                    Step::Reject
                }
            }
            Scan::Osc => {
                if byte.is_ascii_digit() {
                    self.scan = Scan::Code(byte - b'0');
                    Step::Hold
                } else {
                    Step::Reject
                }
            }
            Scan::Code(code) => {
                if byte.is_ascii_digit() && code < 10 {
                    self.scan = Scan::Code(code * 10 + (byte - b'0'));
                    Step::Hold
                } else if byte == b';' && self.outstanding.expects(code, now) {
                    self.scan = Scan::Body {
                        code,
                        payload: Vec::new(),
                        saw_esc: false,
                    };
                    Step::Hold
                } else {
                    Step::Reject
                }
            }
            Scan::Body {
                code,
                mut payload,
                saw_esc,
            } => {
                if saw_esc {
                    if byte != b'\\' {
                        return Step::Reject;
                    }
                } else {
                    match byte {
                        // BEL terminates the reply.
                        0x07 => {}
                        // ESC may begin the ST terminator (`ESC \`).
                        0x1b => {
                            self.scan = Scan::Body {
                                code,
                                payload,
                                saw_esc: true,
                            };
                            return Step::Hold;
                        }
                        byte if is_color_payload_byte(byte) && payload.len() < MAX_PAYLOAD => {
                            payload.push(byte);
                            self.scan = Scan::Body {
                                code,
                                payload,
                                saw_esc: false,
                            };
                            return Step::Hold;
                        }
                        _ => return Step::Reject,
                    }
                }
                let color = std::str::from_utf8(&payload).ok().and_then(parse_x11_color);
                match (color, code) {
                    (Some(rgb), 10) => {
                        self.outstanding.clear(10);
                        Step::Complete(Reply::Foreground(rgb))
                    }
                    (Some(rgb), _) => {
                        self.outstanding.clear(11);
                        Step::Complete(Reply::Background(rgb))
                    }
                    (None, _) => Step::Reject,
                }
            }
        }
    }
}

/// What feeding one event's bytes did to the filter.
enum Fed {
    Pass,
    Hold,
    Complete(Reply),
    Reject,
}

/// The longest color payload (`rgba:` + four 4-digit components + separators is 29).
const MAX_PAYLOAD: usize = 64;

/// Bytes a color payload (`rgb:ffff/ffff/ffff`, `rgba:`) can contain.
fn is_color_payload_byte(byte: u8) -> bool {
    byte.is_ascii_hexdigit() || matches!(byte, b'r' | b'g' | b'b' | b'a' | b':' | b'/')
}

/// How a probe window ended.
#[derive(Debug)]
pub(super) enum ProbeEnd {
    /// The OSC 11 reply completed, or the deadline passed (`None`).
    Done(Option<ProbedRgb>),
    /// The consumer of the events has gone away.
    Closed,
}

/// Runs one background probe's reading half on the reader thread. The query has
/// just been written. Reads crossterm events until the OSC 11 reply completes (stop
/// at once) or `deadline` passes; every event that is not the reply is handed to
/// `forward` in order as it arrives -- a key typed during the probe is delivered
/// immediately, not discarded. `forward` returns `false` when its consumer is gone.
/// On a miss the background reply stays outstanding for [`LATE_REPLY_WINDOW`], so a
/// late reply is swallowed by the reader's main loop.
pub(super) fn run_probe_window<E>(
    filter: &mut ReplyFilter<E>,
    deadline: Duration,
    mut poll: impl FnMut(Duration) -> io::Result<bool>,
    mut read: impl FnMut() -> io::Result<E>,
    wire_of: impl Fn(&E) -> Wire,
    mut forward: impl FnMut(Emit<E>) -> bool,
) -> io::Result<ProbeEnd> {
    let started = Instant::now();
    let end = started + deadline;
    filter.expect_background(end + LATE_REPLY_WINDOW);
    loop {
        let now = Instant::now();
        let remaining = end.saturating_duration_since(now);
        if remaining.is_zero() {
            break;
        }
        let timeout = if filter.is_holding() {
            remaining.min(REPLY_HOLD_NET)
        } else {
            remaining
        };
        if !poll(timeout)? {
            // Quiet for the timeout: whatever is withheld was typed.
            for emit in filter.release_held() {
                if !forward(emit) {
                    return Ok(ProbeEnd::Closed);
                }
            }
            continue;
        }
        let event = match read() {
            Ok(event) => event,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let wire = wire_of(&event);
        for emit in filter.push(event, wire, Instant::now()) {
            match emit {
                Emit::Reply(Reply::Background(rgb)) => {
                    // Complete: stop at once (the withheld events were part of it).
                    return Ok(ProbeEnd::Done(Some(rgb)));
                }
                emit => {
                    if !forward(emit) {
                        return Ok(ProbeEnd::Closed);
                    }
                }
            }
        }
    }
    for emit in filter.release_held() {
        if !forward(emit) {
            return Ok(ProbeEnd::Closed);
        }
    }
    Ok(ProbeEnd::Done(None))
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
