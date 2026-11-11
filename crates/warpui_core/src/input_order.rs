//! Debug logging for the order in which input reaches the app (#1016).
//!
//! Everything logs to the `warp::input_order` target at debug level, so a tester can confirm the
//! order keys were received, typed characters were inserted and a command was submitted with
//! `RUST_LOG=warp::input_order=debug`.
//!
//! Keystrokes can be passwords, so text is redacted to its length unless the tester opts in with
//! `WARP_INPUT_ORDER_LOG_TEXT=1`. Named keys (`enter`, `backspace`, ...) are always logged by name.
//!
//! Two more rules keep a secret out of an opted-in log (#2040):
//! - While something reads a secret or a prompt's reader has the keys (a `read -s`, `sudo`, an
//!   ssh password; see [`register_secret_input_probe`]) every text is logged by its length, with
//!   every opt-in on. `PHOSPHOR_LOG_SECRET_GUARD=0` turns this rule off.
//! - The bytes of a user key on its way to the pty (`hop=pty_write_bytes class=UserKey`,
//!   `hop=hand_route`) are logged by their length unless the tester ALSO sets
//!   `WARP_INPUT_ORDER_LOG_KEY_BYTES_MAY_INCLUDE_PASSWORDS=1`: the app cannot see every secret
//!   read (ssh.exe on ConPTY, keys typed before the reader turned echo off), and these lines are
//!   exactly what was typed.
//!
//! The same target also times the rest of a keystroke's round trip (#1092), so a slow echo can be
//! placed without guessing: `pty_write` when user bytes reach the pty, `pty_read` for the first
//! output read after them (with the gap since the write), and `frame` for the first frame built
//! after that output. Only byte counts and times are logged, never the bytes.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The log target every input-order line uses.
pub const TARGET: &str = "warp::input_order";

/// Whether input-order lines would be written at all.
pub fn enabled() -> bool {
    log::log_enabled!(target: TARGET, log::Level::Debug)
}

fn env_flag_set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| !value.is_empty() && value != "0")
}

fn text_opted_in() -> bool {
    static OPTED_IN: OnceLock<bool> = OnceLock::new();
    *OPTED_IN.get_or_init(|| env_flag_set("WARP_INPUT_ORDER_LOG_TEXT"))
}

/// The extra opt-in for the bytes of user keys on their way to the pty (#2040). Its name says
/// what it costs: with it a password typed where the app sees no secret read is in the log.
pub const KEY_BYTES_ENV: &str = "WARP_INPUT_ORDER_LOG_KEY_BYTES_MAY_INCLUDE_PASSWORDS";

fn key_bytes_opted_in() -> bool {
    static OPTED_IN: OnceLock<bool> = OnceLock::new();
    *OPTED_IN.get_or_init(|| env_flag_set(KEY_BYTES_ENV))
}

/// Kill switch of the secret guard (#2040): set to `0`, text is logged during a secret read as
/// it was before (for a synthetic test of the guard itself, never with real input).
pub const SECRET_GUARD_ENV: &str = "PHOSPHOR_LOG_SECRET_GUARD";

fn secret_guard_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var(SECRET_GUARD_ENV).map_or(true, |value| value.trim() != "0"))
}

/// Says whether a pane's keys may be a secret right now. Called on any thread, with the caller's
/// locks held: it must take no lock (an atomic load, one `tcgetattr`).
pub type SecretInputProbe = Arc<dyn Fn() -> bool + Send + Sync>;

static SECRET_INPUT_PROBES: Mutex<Vec<(u64, SecretInputProbe)>> = Mutex::new(Vec::new());
static NEXT_SECRET_INPUT_PROBE: AtomicU64 = AtomicU64::new(1);

/// Keeps a probe of [`register_secret_input_probe`] installed; dropping it removes the probe.
pub struct SecretInputProbeRegistration(u64);

impl std::fmt::Debug for SecretInputProbeRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretInputProbeRegistration({})", self.0)
    }
}

impl SecretInputProbeRegistration {
    /// What this registration's own probe answers right now (`false` once it is gone).
    pub fn says_secret(&self) -> bool {
        SECRET_INPUT_PROBES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|(id, probe)| *id == self.0 && probe())
    }
}

impl Drop for SecretInputProbeRegistration {
    fn drop(&mut self) {
        SECRET_INPUT_PROBES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|(id, _)| *id != self.0);
    }
}

/// Installs a pane's answer to "may the keys typed now be a secret?" (#2040): the tty is
/// canonical with echo off, a command announced a secret read, a prompt's reader has the keys.
/// While ANY installed probe says yes, no text is logged, whichever pane the line is about (a
/// key is logged before the app knows where it goes). The probes are asked only for a line that
/// would otherwise show text, so nothing is sampled unless the tester opted in.
pub fn register_secret_input_probe(probe: SecretInputProbe) -> SecretInputProbeRegistration {
    let id = NEXT_SECRET_INPUT_PROBE.fetch_add(1, Ordering::Relaxed);
    SECRET_INPUT_PROBES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push((id, probe));
    SecretInputProbeRegistration(id)
}

/// Whether any installed probe says a secret may be typed right now.
pub fn secret_input_active() -> bool {
    SECRET_INPUT_PROBES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .any(|(_, probe)| probe())
}

/// Whether a log line outside this module must hold back the user's text right now (#2040): the
/// secret guard is on and a probe says a secret may be typed. For a line that shows a piece of
/// input without the text opt-in (the routing decision's first word).
pub fn secret_guard_withholds_text() -> bool {
    secret_guard_enabled() && secret_input_active()
}

/// What a log line may show of the user's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextPolicy {
    /// `WARP_INPUT_ORDER_LOG_TEXT=1`.
    text: bool,
    /// [`KEY_BYTES_ENV`] is set as well.
    key_bytes: bool,
    /// The secret guard is on (it is, unless [`SECRET_GUARD_ENV`] says `0`).
    secret_guard: bool,
}

impl TextPolicy {
    fn from_env() -> Self {
        Self {
            text: text_opted_in(),
            key_bytes: key_bytes_opted_in(),
            secret_guard: secret_guard_enabled(),
        }
    }

    /// Whether text (an editor buffer, typed characters, a command's bytes) is shown.
    /// `secret_active` is asked last, and only when the answer depends on it.
    fn shows_text(self, secret_active: impl FnOnce() -> bool) -> bool {
        self.text && !(self.secret_guard && secret_active())
    }

    /// Whether the bytes of a user key on its way to the pty are shown.
    fn shows_key_bytes(self, secret_active: impl FnOnce() -> bool) -> bool {
        self.key_bytes && self.shows_text(secret_active)
    }
}

/// The guard held text back at the last line that asked (for the edge lines below).
static WITHHOLDING_FOR_SECRET: AtomicBool = AtomicBool::new(false);

/// [`secret_input_active`], and one line at each change of the answer:
/// `input_order hop=secret_text_withheld active=<bool> t=...`, so a reader of the log knows why
/// the lines between the two carry lengths (and a test can tell the guard worked).
fn secret_active_logged() -> bool {
    let active = secret_input_active();
    if WITHHOLDING_FOR_SECRET.swap(active, Ordering::Relaxed) != active && enabled() {
        log::debug!(target: TARGET, "{}", secret_text_withheld_line(active));
    }
    active
}

fn secret_text_withheld_line(active: bool) -> String {
    hop_line("secret_text_withheld", &format!("active={active}"))
}

/// Milliseconds since the Unix epoch, with microsecond resolution, for ordering log lines.
pub fn now_ms() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}.{:03}",
        elapsed.as_millis(),
        elapsed.subsec_micros() % 1000
    )
}

/// Text longer than this is logged by its count even when the tester opted in. r18u19 (macOS
/// `remote_heredoc_paste`): with a 600 KB here-doc in the editor every key logged the whole
/// buffer, ~50 MB of log in 13 s, and the rotation deleted the five minutes that showed what
/// happened to the paste. The count form is the one the harness already parses.
pub const MAX_LOGGED_TEXT_BYTES: usize = 64 * 1024;

/// Describes `text` for the log: the text itself only when the tester opted in, it is at most
/// [`MAX_LOGGED_TEXT_BYTES`] long and no secret is being read (#2040); else its length.
pub fn describe_text(text: &str) -> String {
    describe_text_with(
        text,
        TextPolicy::from_env().shows_text(secret_active_logged),
    )
}

fn describe_text_with(text: &str, shown: bool) -> String {
    if shown && text.len() <= MAX_LOGGED_TEXT_BYTES {
        format!("{text:?}")
    } else {
        format!("<{} chars>", text.chars().count())
    }
}

/// Describes raw input bytes for a log line (control sequences can carry typed text: Alt+letter is
/// ESC + letter, kitty/CSI-u sequences carry the character, a stray paste byte stream is text): the
/// bytes themselves only when the tester opted in with `WARP_INPUT_ORDER_LOG_TEXT=1` and no secret
/// is being read, else only their count (#1444, #2040).
pub fn describe_bytes(bytes: &[u8]) -> String {
    describe_bytes_with(
        bytes,
        TextPolicy::from_env().shows_text(secret_active_logged),
    )
}

fn describe_bytes_with(bytes: &[u8], shown: bool) -> String {
    if shown && bytes.len() <= MAX_LOGGED_TEXT_BYTES {
        format!("{bytes:?}")
    } else {
        format!("<{} bytes>", bytes.len())
    }
}

/// Describes raw input bytes as an escaped string (`echo\x1bi`) when the tester opted in with
/// `WARP_INPUT_ORDER_LOG_TEXT=1` and no secret is being read, else only their count (#1490,
/// #2040). For bytes the app composed for the pty (a submitted command, a paste, an agent's
/// input); a user key's bytes go through [`describe_user_key_bytes_escaped`].
pub fn describe_bytes_escaped(bytes: &[u8]) -> String {
    describe_bytes_escaped_with(
        bytes,
        TextPolicy::from_env().shows_text(secret_active_logged),
    )
}

/// As [`describe_bytes_escaped`] for the bytes of a user key on its way to the pty (#2040): their
/// length, unless the tester set BOTH `WARP_INPUT_ORDER_LOG_TEXT=1` and [`KEY_BYTES_ENV`], and
/// never while a secret is being read.
pub fn describe_user_key_bytes_escaped(bytes: &[u8]) -> String {
    describe_bytes_escaped_with(
        bytes,
        TextPolicy::from_env().shows_key_bytes(secret_active_logged),
    )
}

fn describe_bytes_escaped_with(bytes: &[u8], shown: bool) -> String {
    if shown && bytes.len() <= MAX_LOGGED_TEXT_BYTES {
        format!("\"{}\"", bytes.escape_ascii())
    } else {
        format!("<{} bytes>", bytes.len())
    }
}

/// What one log line may show of the user's text, decided once for the line (#2100). The describe
/// functions above are for the input-order hops; a line OUTSIDE this target (a typed action's
/// dump, a find query, a dictated sentence, the shell's hook JSON) builds its text with a
/// `TextShown` so that it follows the same rule: the length by default, the text only with
/// `WARP_INPUT_ORDER_LOG_TEXT=1`, and never while a secret is being read.
///
/// A line builder takes one as its last argument, so a unit test can build the line under each
/// policy ([`TextShown::fixed`], [`assert_line_withholds`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextShown(bool);

impl TextShown {
    /// The answer for a line written now: the tester's opt-in, and no secret being read.
    pub fn sample() -> Self {
        Self(TextPolicy::from_env().shows_text(secret_active_logged))
    }

    /// The answer under a given state (secret guard on), for a unit test of a line builder.
    pub fn fixed(text_opted_in: bool, secret_active: bool) -> Self {
        let policy = TextPolicy {
            text: text_opted_in,
            key_bytes: false,
            secret_guard: true,
        };
        Self(policy.shows_text(|| secret_active))
    }

    /// Whether the line shows text at all.
    pub fn shows(self) -> bool {
        self.0
    }

    /// `text` quoted, or `<N chars>`.
    pub fn text(self, text: &str) -> String {
        describe_text_with(text, self.0)
    }

    /// `Some("text")` / `Some(<N chars>)` / `None`: the shape `{:?}` gave an optional string.
    pub fn opt_text(self, text: Option<&str>) -> String {
        match text {
            Some(text) => format!("Some({})", self.text(text)),
            None => "None".to_string(),
        }
    }

    /// A `Debug` dump whose payload can be user text: the dump itself, or the name it starts
    /// with (a type or variant: `Insert`, `SearchConfig`) and the length of the rest.
    pub fn debug<T: std::fmt::Debug + ?Sized>(self, value: &T) -> String {
        describe_debug_with(&format!("{value:?}"), self.0)
    }

    /// A `Display` text that can quote user text (a regex error prints the pattern): the text
    /// itself, unquoted, or `<N chars>`.
    pub fn display<T: std::fmt::Display + ?Sized>(self, value: &T) -> String {
        let text = value.to_string();
        if self.0 && text.len() <= MAX_LOGGED_TEXT_BYTES {
            text
        } else {
            describe_text_with(&text, false)
        }
    }
}

/// The name a `Debug` dump starts with, when it is one: `Insert` of `Insert { text: "x" }`,
/// `Find` of `Find("x")`, the whole of a unit variant. Empty for anything else (a quoted string,
/// a list, a hand-written `Debug` that prints text first), so that no text passes as a name: the
/// name must start with an ASCII capital, hold only identifier characters and `::`, and be
/// followed by the end, `(`, ` {` or `{`.
fn debug_dump_name(dump: &str) -> &str {
    let end = dump
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(dump.len());
    let (name, rest) = dump.split_at(end);
    let is_name = name.starts_with(|c: char| c.is_ascii_uppercase())
        && (rest.is_empty()
            || rest.starts_with('(')
            || rest.starts_with(" {")
            || rest.starts_with('{'));
    if is_name { name } else { "" }
}

fn describe_debug_with(dump: &str, shown: bool) -> String {
    if shown && dump.len() <= MAX_LOGGED_TEXT_BYTES {
        return dump.to_string();
    }
    let name = debug_dump_name(dump);
    let rest = dump.len() - name.len();
    if rest == 0 {
        name.to_string()
    } else {
        // Counted in chars like every other withheld text.
        let chars = dump[name.len()..].chars().count();
        format!("{name}<{chars} chars>")
    }
}

/// [`TextShown::opt_text`] for a line written now.
pub fn describe_opt_text(text: Option<&str>) -> String {
    TextShown::sample().opt_text(text)
}

/// [`TextShown::debug`] for a line written now.
pub fn describe_debug<T: std::fmt::Debug + ?Sized>(value: &T) -> String {
    TextShown::sample().debug(value)
}

/// [`TextShown::display`] for a line written now.
pub fn describe_display<T: std::fmt::Display + ?Sized>(value: &T) -> String {
    TextShown::sample().display(value)
}

/// The unit test of a log line that can carry user text (#2100): `line` is built under each
/// policy and must hold none of `texts` by default, none while a secret is read (with the text
/// opt-in on or off), and all of them with the opt-in and no secret, which proves the texts were
/// in the line's inputs at all. Panics with the line otherwise.
#[doc(hidden)]
pub fn assert_line_withholds(line: impl Fn(TextShown) -> String, texts: &[&str]) {
    assert!(!texts.is_empty(), "no text to look for");
    for (what, text_opted_in, secret_active) in [
        ("by default", false, false),
        ("during a secret read with the text opt-in", true, true),
        ("during a secret read", false, true),
    ] {
        let built = line(TextShown::fixed(text_opted_in, secret_active));
        for text in texts {
            assert!(
                !built.contains(text),
                "the line holds {text:?} {what}: {built}"
            );
        }
    }
    let shown = line(TextShown::fixed(true, false));
    for text in texts {
        assert!(
            shown.contains(text),
            "the line does not hold {text:?} with the text opt-in and no secret read, so the \
             checks above prove nothing: {shown}"
        );
    }
}

/// The three lines of a typed action's dispatch (#2100): the action's type, and its `Debug` dump
/// under the text rule (a search, tab-name or free-text action carries what the user typed).
pub(crate) fn describe_typed_action<T: std::fmt::Debug + ?Sized>(
    type_name: &str,
    action: &T,
    shown: TextShown,
) -> String {
    format!("{type_name}::{}", shown.debug(action))
}

/// A shell-ready signal the typeahead hand-off acts on (#1490), stamped so every pty write can say
/// how long after each one it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellSignal {
    /// The prompt's precmd hook was parsed.
    Precmd,
    /// A command started (preexec).
    Preexec,
    /// The line editor's ready signal (bracketed-paste enable) after a prompt.
    LineEditorReady,
    /// The input-report request (`ESC i`) was decided.
    RequestDecided,
    /// The shell's input report arrived.
    ReportReceived,
}

impl ShellSignal {
    const COUNT: usize = 5;

    fn index(self) -> usize {
        match self {
            ShellSignal::Precmd => 0,
            ShellSignal::Preexec => 1,
            ShellSignal::LineEditorReady => 2,
            ShellSignal::RequestDecided => 3,
            ShellSignal::ReportReceived => 4,
        }
    }

    fn label(index: usize) -> &'static str {
        match index {
            0 => "precmd",
            1 => "preexec",
            2 => "ready",
            3 => "request",
            _ => "report",
        }
    }
}

/// Epoch milliseconds of the last stamp per [`ShellSignal`]; 0 means never.
static SHELL_SIGNAL_MS: [AtomicU64; ShellSignal::COUNT] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Stamps `signal` as having just happened. One atomic store when input-order logging is on,
/// nothing when it is off. A `Preexec` also logs `hop=preexec_received` (#1583 W3): the stamp
/// alone only shows up as `since_preexec_ms` on the NEXT pty write, so the Enter latency split
/// (pty write of the line -> the shell's preexec) could not be measured from the log.
pub fn note_shell_signal(signal: ShellSignal) {
    if enabled() {
        SHELL_SIGNAL_MS[signal.index()].store(epoch_ms(), Ordering::Relaxed);
        if signal == ShellSignal::Preexec {
            log::debug!(target: TARGET, "{}", preexec_line());
        }
    }
}

/// `input_order hop=preexec_received signal=preexec t=...` (#1583 W3; parsed by the appci
/// harness's `enter_segments`). On Windows the line ends with the performance counter
/// (` qpc=<ticks> qpf=<ticks per second>`, after `t=`), the clock PowerShell's hook timing marks
/// use (#1401), so the shell's own marks can be placed on the app's timeline.
fn preexec_line() -> String {
    format!(
        "{}{}",
        hop_line("preexec_received", "signal=preexec"),
        qpc_suffix()
    )
}

/// ` qpc=<ticks> qpf=<ticks per second>`: the Windows performance counter now (#1401). The
/// PowerShell bootstrap's hook timing marks (`PHOSPHOR_PWSH_HOOK_TIMING`) are taken with
/// `Stopwatch.GetTimestamp()`, which reads the same counter, so a log line that carries both
/// `t=` and this calibrates them onto the app's epoch timeline. Empty elsewhere.
pub fn qpc_suffix() -> String {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn QueryPerformanceCounter(count: *mut i64) -> i32;
            fn QueryPerformanceFrequency(frequency: *mut i64) -> i32;
        }
        let (mut count, mut frequency) = (0i64, 0i64);
        // SAFETY: each call writes one i64 through a pointer to a live local.
        let ok = unsafe {
            QueryPerformanceCounter(&mut count) != 0
                && QueryPerformanceFrequency(&mut frequency) != 0
        };
        if ok {
            return format!(" qpc={count} qpf={frequency}");
        }
    }
    String::new()
}

/// `since_precmd_ms=12 since_preexec_ms=none ...`: how long ago each shell-ready signal was last
/// stamped, for the line of a pty write (#1490).
pub fn shell_signal_ages() -> String {
    let now = epoch_ms();
    let mut out = String::new();
    for (index, stamp) in SHELL_SIGNAL_MS.iter().enumerate() {
        let at = stamp.load(Ordering::Relaxed);
        if !out.is_empty() {
            out.push(' ');
        }
        if at == 0 {
            out.push_str(&format!("since_{}_ms=none", ShellSignal::label(index)));
        } else {
            out.push_str(&format!(
                "since_{}_ms={}",
                ShellSignal::label(index),
                now.saturating_sub(at)
            ));
        }
    }
    out
}

/// Describes a key name: multi-character names (`enter`) as they are, single characters like text.
pub fn describe_key(key: &str) -> String {
    if key.chars().count() > 1 {
        key.to_string()
    } else {
        describe_text(key)
    }
}

/// Every line starts with this, because release builds do not print the log target: without it a
/// tester cannot grep the log for these lines (#1034).
const LINE_PREFIX: &str = "input_order";

/// Sequence number of the key being dispatched (#1345). A physical key is a `keydown` event and,
/// for printable keys, a `typed_characters` event right after it: both carry the same number, and
/// every `kh_*` hop the key causes prints it, so one key's path can be followed end to end.
static KEY_SEQ: AtomicU64 = AtomicU64::new(0);
/// A `keydown` was numbered and its `typed_characters` has not arrived yet.
static KEYDOWN_OPEN: AtomicBool = AtomicBool::new(false);

/// The number of the key being dispatched now (0 before the first).
pub fn current_key_seq() -> u64 {
    KEY_SEQ.load(Ordering::Relaxed)
}

/// Numbers a `keydown` event.
fn next_key_seq_for_keydown() -> u64 {
    KEYDOWN_OPEN.store(true, Ordering::Relaxed);
    KEY_SEQ.fetch_add(1, Ordering::Relaxed) + 1
}

/// Numbers a `typed_characters` event: the one of the `keydown` before it, else a new one.
fn key_seq_for_typed_characters() -> u64 {
    if KEYDOWN_OPEN.swap(false, Ordering::Relaxed) {
        current_key_seq()
    } else {
        KEY_SEQ.fetch_add(1, Ordering::Relaxed) + 1
    }
}

fn key_down_line(key: &str, ctrl: bool, alt: bool, shift: bool, cmd: bool, seq: u64) -> String {
    format!(
        "{LINE_PREFIX} keydown key={} ctrl={ctrl} alt={alt} shift={shift} cmd={cmd} t={} seq={seq}",
        describe_key(key),
        now_ms()
    )
}

fn typed_characters_line(chars: &str, seq: u64) -> String {
    format!(
        "{LINE_PREFIX} typed_characters text={} t={} seq={seq}",
        describe_text(chars),
        now_ms()
    )
}

fn submit_line(buffer: &str) -> String {
    format!(
        "{LINE_PREFIX} submit buffer={} t={}",
        describe_text(buffer),
        now_ms()
    )
}

fn pty_write_line(bytes: usize) -> String {
    format!("{LINE_PREFIX} pty_write bytes={bytes} t={}", now_ms())
}

/// A hop line for the typeahead path (#1162): `what` names the hop, `detail` is counts and
/// states only, never typed text.
fn hop_line(what: &str, detail: &str) -> String {
    format!("{LINE_PREFIX} hop={what} {detail} t={}", now_ms())
}

/// A line for one layer an Return/Enter key press passes through (#1482): `layer` is `mac_enter`
/// (the Objective-C view/window/application) or `core_enter` (the keystroke matcher), `stage`
/// names the point reached, `detail` is flags, counts, times and type names only, never text.
/// Grep `hop=mac_enter` / `hop=core_enter` to see where a lost Enter stops.
fn enter_line(layer: &str, stage: &str, detail: &str) -> String {
    format!(
        "{LINE_PREFIX} hop={layer} stage={stage} {detail} t={}",
        now_ms()
    )
}

fn pty_read_line(bytes: usize, since_write: Duration) -> String {
    format!(
        "{LINE_PREFIX} pty_read bytes={bytes} since_write_ms={:.1} t={}",
        since_write.as_secs_f64() * 1000.,
        now_ms()
    )
}

fn frame_line(layout: bool) -> String {
    format!("{LINE_PREFIX} frame layout={layout} t={}", now_ms())
}

/// Set when pty output that followed user input was read, cleared by the next frame (#1092).
static FRAME_OWED_FOR_PTY_OUTPUT: AtomicBool = AtomicBool::new(false);
/// A key-down was logged and no frame has been built since (#1828): the next frame is logged as `hop=key_frame`. Separate from
/// [`FRAME_OWED_FOR_PTY_OUTPUT`] so the pty-echo `frame` pairing of the existing latency cases is unchanged.
static FRAME_OWED_FOR_KEY: AtomicBool = AtomicBool::new(false);

/// Logs user input (not a query reply) having been written to a pty, `bytes` long.
pub fn log_pty_write(bytes: usize) {
    if enabled() {
        log::debug!(target: TARGET, "{}", pty_write_line(bytes));
    }
}

/// A tri-state focus answer as a log word.
pub fn describe_opt(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

/// Logs one hop of a typed key's path (#1345): `kh_*` names, the number of the key being
/// dispatched, then flags and counts only (never the key). Grep `hop=kh_` and follow `seq=`.
pub fn log_key_hop(what: &str, detail: impl FnOnce() -> String) {
    if enabled() {
        log::debug!(
            target: TARGET,
            "{}",
            hop_line(what, &format!("seq={} {}", current_key_seq(), detail()))
        );
    }
}

/// Logs one hop of the typeahead -> pty path (#1162), for pinning down where a typed-ahead byte
/// is lost or reordered on a platform that cannot be debugged live (Windows ConPTY). Takes the
/// detail lazily so the formatting costs nothing when input-order logging is off.
pub fn log_hop(what: &str, detail: impl FnOnce() -> String) {
    if enabled() {
        log::debug!(target: TARGET, "{}", hop_line(what, &detail()));
    }
}

/// `input_order hop=notify_cause view=<view> cause=<name> t=...` (#1995): which call asked for a
/// re-render of `view`. `cause` is a source-chosen name (no spaces, no text of the user's).
fn notify_cause_line(view: &str, cause: &str) -> String {
    hop_line("notify_cause", &format!("view={view} cause={cause}"))
}

/// Logs the cause of one `ctx.notify()` of `view`. Both arguments are static names, so nothing
/// is allocated or formatted when input-order logging is off (`enabled()` is the only cost).
pub fn log_notify_cause(view: &'static str, cause: &'static str) {
    if enabled() {
        log::debug!(target: TARGET, "{}", notify_cause_line(view, cause));
    }
}

/// `input_order hop=desktop_notification title_chars=<n> body_chars=<n> t=...` (#2075, app test
/// `desktop_notification`): one line per desktop notification handed to the OS. Counts only, never
/// the title or the body (they hold the user's command line).
fn desktop_notification_line(title: &str, body: &str) -> String {
    hop_line(
        "desktop_notification",
        &format!(
            "title_chars={} body_chars={}",
            title.chars().count(),
            body.chars().count()
        ),
    )
}

/// Logs that a desktop notification is being handed to the OS. Trace only: no behaviour depends
/// on it, and nothing is counted or formatted when input-order logging is off.
pub fn log_desktop_notification(title: &str, body: &str) {
    if enabled() {
        log::debug!(target: TARGET, "{}", desktop_notification_line(title, body));
    }
}

/// The owner's budget for one item of main-thread UI work (#1191, #1219, #1301): a frame, an
/// event dispatch or an action dispatch over this is logged as a `hop=ui_item` line so a real-app
/// run can show which item broke the budget (the `SlowLog` lines only start at 16 ms).
pub const UI_ITEM_BUDGET: Duration = Duration::from_millis(4);

/// `input_order hop=ui_item kind=<kind> ms=<elapsed> <detail> t=...`; `detail` is counts, times
/// and type names only, never text.
fn ui_item_line(kind: &str, elapsed: Duration, detail: &str) -> String {
    hop_line(
        "ui_item",
        &format!(
            "kind={kind} ms={:.2} {detail}",
            elapsed.as_secs_f64() * 1000.0
        ),
    )
}

/// Logs one main-thread UI item that took at least [`UI_ITEM_BUDGET`] (#1191). Under the budget
/// it is one `Duration` compare; the detail is formatted only for a line that is written.
pub fn log_ui_item_if_over_budget(kind: &str, elapsed: Duration, detail: impl FnOnce() -> String) {
    if elapsed >= UI_ITEM_BUDGET && enabled() {
        log::debug!(target: TARGET, "{}", ui_item_line(kind, elapsed, &detail()));
    }
}

/// Logs one stage of an Enter key press (#1482); see [`enter_line`]. Takes the detail lazily.
pub fn log_enter(layer: &str, stage: &str, detail: impl FnOnce() -> String) {
    if enabled() {
        log::debug!(target: TARGET, "{}", enter_line(layer, stage, &detail()));
    }
}

/// `input_order hop=os_key_event age_ms=0.412 enter=false ctrl=true t=...` (#1606): how long a
/// key-down waited between the OS stamping it (the event's own timestamp) and the app dequeuing
/// it on the main thread. A main thread busy rendering (a flood's frames blocking in
/// `nextDrawable`) shows up here as a large age. Flags only, never the key itself (typed text
/// can be a password); Enter and Control are named because the latency split needs them.
fn os_key_event_line(age_ms: f64, enter: bool, ctrl: bool) -> String {
    hop_line(
        "os_key_event",
        &format!("age_ms={age_ms:.3} enter={enter} ctrl={ctrl}"),
    )
}

/// Logs a key-down's OS-to-app queueing age (#1606); see [`os_key_event_line`]. A negative or
/// absurd age (clock mismatch) is logged as it is: the harness filters, the app never guesses.
pub fn log_os_key_event(age_ms: f64, enter: bool, ctrl: bool) {
    if enabled() {
        log::debug!(target: TARGET, "{}", os_key_event_line(age_ms, enter, ctrl));
    }
}

/// `input_order hop=keydown_menu ctrl=true cmd=false t=...` (#2041): a key-down the main menu took
/// as the key equivalent of one of its items (macOS). Such a key never reaches the view's key
/// dispatch, so it leaves no `keydown` and no `keydown_keybound` line: Ctrl-C with the input
/// editor in front is the "Clear editor" item's key equivalent, and without this line a key the
/// app acted on read as one that reached the app and went nowhere. Flags only, never the key.
fn menu_key_event_line(ctrl: bool, cmd: bool) -> String {
    hop_line("keydown_menu", &format!("ctrl={ctrl} cmd={cmd}"))
}

/// Logs a key-down that triggered a main-menu item (#2041); see [`menu_key_event_line`]. Called
/// by the menu item's action, only while the event being handled is a key-down (a clicked item
/// logs nothing).
pub fn log_menu_key_event(ctrl: bool, cmd: bool) {
    if enabled() {
        log::debug!(target: TARGET, "{}", menu_key_event_line(ctrl, cmd));
    }
}

/// Epoch microseconds of the last `^C` pty write; 0 means none is being watched (#1838).
static CTRL_C_WRITE_US: AtomicU64 = AtomicU64::new(0);
/// Frames logged for the watched `^C` so far.
static CTRL_C_FRAMES: AtomicU64 = AtomicU64::new(0);
/// A watched `^C` stops being followed this long after its write.
const CTRL_C_WATCH_MS: u64 = 600;
/// At most this many frame lines per watched `^C` (a flood paints a frame every few ms).
const CTRL_C_WATCH_MAX_FRAMES: u64 = 24;

fn epoch_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Starts following the frames presented after a `^C` reached the pty (#1838), so the part of
/// the Ctrl-C overhead that sits after the shell reacted (the prompt waiting for a frame) can be
/// told from the part before it. One atomic store when input-order logging is off.
pub fn note_ctrl_c_written() {
    if enabled() {
        CTRL_C_FRAMES.store(0, Ordering::Relaxed);
        CTRL_C_WRITE_US.store(epoch_us(), Ordering::Relaxed);
    }
}

/// What to log for a frame presented `now_us` while a `^C` written at `write_us` is watched:
/// `Some((detail, last))`, `None` when the watch is over. `precmd_ms` is the epoch-ms stamp of the
/// last prompt hook (0 = never). `last` is true for the first frame at or after the prompt hook
/// (the frame that can carry the new prompt), the line the harness reads.
fn ctrl_c_frame_step(
    write_us: u64,
    now_us: u64,
    frames_logged: u64,
    precmd_ms: u64,
    presented: bool,
) -> Option<(String, bool)> {
    if write_us == 0
        || frames_logged >= CTRL_C_WATCH_MAX_FRAMES
        || now_us.saturating_sub(write_us) > CTRL_C_WATCH_MS * 1000
    {
        return None;
    }
    let precmd_us = precmd_ms.saturating_mul(1000);
    let precmd_seen = precmd_us >= write_us.saturating_sub(1000);
    let after_precmd = precmd_seen && now_us >= precmd_us;
    let since_precmd = if precmd_seen {
        format!("{:.1}", (now_us as f64 - precmd_us as f64) / 1000.0)
    } else {
        "none".to_string()
    };
    Some((
        format!(
            "since_write_ms={:.1} since_precmd_ms={since_precmd} presented={presented} after_precmd={after_precmd}",
            now_us.saturating_sub(write_us) as f64 / 1000.0
        ),
        after_precmd,
    ))
}

/// Logs a frame that reached the screen while a `^C` is watched (#1838) as
/// `input_order hop=ctrl_c_frame since_write_ms=.. since_precmd_ms=.. presented=.. after_precmd=..`.
/// The watch ends at the first frame after the prompt hook. Costs one atomic load when idle.
pub fn log_ctrl_c_frame_presented(presented: bool) {
    let write_us = CTRL_C_WRITE_US.load(Ordering::Relaxed);
    if write_us == 0 || !enabled() {
        return;
    }
    let frames = CTRL_C_FRAMES.fetch_add(1, Ordering::Relaxed);
    let precmd_ms = SHELL_SIGNAL_MS[ShellSignal::Precmd.index()].load(Ordering::Relaxed);
    match ctrl_c_frame_step(write_us, epoch_us(), frames, precmd_ms, presented) {
        Some((detail, last)) => {
            log::debug!(target: TARGET, "{}", hop_line("ctrl_c_frame", &detail));
            if last {
                CTRL_C_WRITE_US.store(0, Ordering::Relaxed);
            }
        }
        None => CTRL_C_WRITE_US.store(0, Ordering::Relaxed),
    }
}

/// Logs the first pty output read after user input was written, `since_write` after the write,
/// and arms [`log_frame_if_pty_output_pending`] for the frame that should show it.
pub fn log_pty_read_after_write(bytes: usize, since_write: Duration) {
    if enabled() {
        FRAME_OWED_FOR_PTY_OUTPUT.store(true, Ordering::Relaxed);
        log::debug!(target: TARGET, "{}", pty_read_line(bytes, since_write));
    }
}

/// Whether a frame line is owed, clearing the debt: only the first frame after the output counts.
fn take_frame_owed(owed: &AtomicBool) -> bool {
    owed.swap(false, Ordering::Relaxed)
}

/// Logs the first frame built after [`log_pty_read_after_write`]; `layout` is false for a
/// paint-only frame. Costs one atomic swap when input-order logging is off.
pub fn log_frame_if_pty_output_pending(layout: bool) {
    if take_frame_owed(&FRAME_OWED_FOR_PTY_OUTPUT) && enabled() {
        log::debug!(target: TARGET, "{}", frame_line(layout));
    }
    // #1828: a typed key that never reaches the pty (the agent input) has no echo, so the line above never fires for it. The first frame
    // after any key-down is logged on its own, `input_order hop=key_frame layout=<bool> t=<ms>`.
    if take_frame_owed(&FRAME_OWED_FOR_KEY) && enabled() {
        log::debug!(target: TARGET, "{}", hop_line("key_frame", &format!("layout={layout}")));
    }
}

/// A key-down was logged and the build split of the frame after it has not been logged yet
/// (`hop=key_frame_build`, docs/design/key-scene-build.md). Separate from [`FRAME_OWED_FOR_KEY`],
/// which the presenter clears in the middle of the build this one is read at the end of.
static BUILD_SPLIT_OWED_FOR_KEY: AtomicBool = AtomicBool::new(false);

/// Whether the next frame build is the one after a key press and its split will be logged, so
/// the build can time its views too. One atomic load; never true with input-order logging off.
pub(crate) fn key_frame_build_owed() -> bool {
    BUILD_SPLIT_OWED_FOR_KEY.load(Ordering::Relaxed)
}

/// Logs the split of the first frame build after a key-down as
/// `input_order hop=key_frame_build <detail> t=<ms>` (see `frame_split::KeyFrameBuild::detail`).
/// Costs one atomic swap for every other frame; the detail is only formatted for a line written.
pub(crate) fn log_key_frame_build_if_owed(detail: impl FnOnce() -> String) {
    if take_frame_owed(&BUILD_SPLIT_OWED_FOR_KEY) && enabled() {
        log::debug!(target: TARGET, "{}", hop_line("key_frame_build", &detail()));
    }
}

/// Logs a key-down being dispatched.
pub fn log_key_down(key: &str, ctrl: bool, alt: bool, shift: bool, cmd: bool) {
    if enabled() {
        let seq = next_key_seq_for_keydown();
        FRAME_OWED_FOR_KEY.store(true, Ordering::Relaxed);
        BUILD_SPLIT_OWED_FOR_KEY.store(true, Ordering::Relaxed);
        log::debug!(target: TARGET, "{}", key_down_line(key, ctrl, alt, shift, cmd, seq));
    }
}

/// Logs typed characters being dispatched.
pub fn log_typed_characters(chars: &str) {
    if enabled() {
        let seq = key_seq_for_typed_characters();
        log::debug!(target: TARGET, "{}", typed_characters_line(chars, seq));
    }
}

/// Logs the editor submitting `buffer` as a command.
pub fn log_submit(buffer: &str) {
    if enabled() {
        log::debug!(target: TARGET, "{}", submit_line(buffer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_write_bytes_and_signal_ages_do_not_leak_text_or_panic() {
        // Not opted in (the variable is not set by the test run): bytes are redacted to a count.
        if std::env::var("WARP_INPUT_ORDER_LOG_TEXT").is_err() {
            let shown = describe_bytes_escaped(b"hunter2\x1bi");
            assert_eq!(shown, "<9 bytes>");
        }
        let ages = shell_signal_ages();
        for label in ["precmd", "preexec", "ready", "request", "report"] {
            assert!(ages.contains(&format!("since_{label}_ms=")), "{ages}");
        }
    }

    /// r18u19: an opted-in log still gives a huge buffer by its count, the form the harness
    /// parses (`buffer=<N chars>`), so one key cannot write the whole of a 600 KB paste.
    #[test]
    fn opted_in_text_over_the_cap_is_logged_by_count() {
        assert_eq!(describe_text_with("a\"b", true), "\"a\\\"b\"");
        assert_eq!(describe_text_with("hunter2", false), "<7 chars>");
        let at_cap = "x".repeat(MAX_LOGGED_TEXT_BYTES);
        assert_eq!(
            describe_text_with(&at_cap, true).len(),
            MAX_LOGGED_TEXT_BYTES + 2
        );
        let over = "é".repeat(MAX_LOGGED_TEXT_BYTES / 2 + 1);
        assert_eq!(
            describe_text_with(&over, true),
            format!("<{} chars>", MAX_LOGGED_TEXT_BYTES / 2 + 1)
        );
    }

    const ALL_OPT_INS: TextPolicy = TextPolicy {
        text: true,
        key_bytes: true,
        secret_guard: true,
    };

    /// #2040: with every opt-in on, no describe form shows text while a secret is being read.
    #[test]
    fn no_secret_bytes_are_shown_while_a_secret_is_read_with_every_opt_in_on() {
        let secret = || true;
        let shown = [
            describe_text_with("hunter2", ALL_OPT_INS.shows_text(secret)),
            describe_bytes_with(b"hunter2", ALL_OPT_INS.shows_text(secret)),
            describe_bytes_escaped_with(b"hunter2\r", ALL_OPT_INS.shows_text(secret)),
            describe_bytes_escaped_with(b"hunter2\r", ALL_OPT_INS.shows_key_bytes(secret)),
        ];
        assert_eq!(
            shown,
            ["<7 chars>", "<7 bytes>", "<8 bytes>", "<8 bytes>"].map(String::from)
        );
        for line in &shown {
            // `describe_bytes` prints numbers: 104 is `h`.
            assert!(!line.contains("hunter2") && !line.contains("104"), "{line}");
        }
        // With no secret being read the same opt-ins show the text: the guard is what hid it.
        let idle = || false;
        assert_eq!(
            describe_text_with("hunter2", ALL_OPT_INS.shows_text(idle)),
            "\"hunter2\""
        );
        assert_eq!(
            describe_bytes_escaped_with(b"ls\r", ALL_OPT_INS.shows_key_bytes(idle)),
            "\"ls\\r\""
        );
    }

    /// #2040: the text opt-in alone never shows the bytes of a user key on its way to the pty.
    #[test]
    fn user_key_bytes_need_the_extra_opt_in() {
        let text_only = TextPolicy {
            key_bytes: false,
            ..ALL_OPT_INS
        };
        assert!(text_only.shows_text(|| false));
        assert!(!text_only.shows_key_bytes(|| false));
        assert_eq!(
            describe_bytes_escaped_with(b"hunter2", text_only.shows_key_bytes(|| false)),
            "<7 bytes>"
        );
        // The extra opt-in without the text opt-in shows nothing either.
        let key_bytes_only = TextPolicy {
            text: false,
            ..ALL_OPT_INS
        };
        assert!(!key_bytes_only.shows_key_bytes(|| false));
        assert!(!key_bytes_only.shows_text(|| false));
        assert_eq!(
            KEY_BYTES_ENV,
            "WARP_INPUT_ORDER_LOG_KEY_BYTES_MAY_INCLUDE_PASSWORDS"
        );
    }

    /// The tty is sampled only for a line that would show text: nothing is asked without the
    /// opt-in, or with the guard's kill switch.
    #[test]
    fn the_secret_probe_is_asked_only_when_the_answer_matters() {
        let never = || -> bool { panic!("the secret probe was sampled") };
        let not_opted_in = TextPolicy {
            text: false,
            ..ALL_OPT_INS
        };
        assert!(!not_opted_in.shows_text(never));
        assert!(!not_opted_in.shows_key_bytes(never));
        let guard_off = TextPolicy {
            secret_guard: false,
            ..ALL_OPT_INS
        };
        assert!(guard_off.shows_text(never));
        let text_only = TextPolicy {
            key_bytes: false,
            ..ALL_OPT_INS
        };
        assert!(!text_only.shows_key_bytes(never));
    }

    /// A probe counts while its registration lives, and any one probe saying yes is enough.
    #[test]
    fn secret_input_is_active_while_a_registered_probe_says_so() {
        let reading = Arc::new(AtomicBool::new(false));
        let flag = reading.clone();
        let registration =
            register_secret_input_probe(Arc::new(move || flag.load(Ordering::Relaxed)));
        let _idle = register_secret_input_probe(Arc::new(|| false));
        let mine = || registration.says_secret();
        assert!(!mine());
        reading.store(true, Ordering::Relaxed);
        assert!(mine());
        assert!(secret_input_active());
        let id = registration.0;
        drop(registration);
        assert!(
            !SECRET_INPUT_PROBES
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .any(|(probe_id, _)| *probe_id == id)
        );
    }

    #[test]
    fn the_secret_text_withheld_line_is_the_hop_the_harness_parses() {
        // script/realapp/appci/cases_precmd_secret.py WITHHELD_RE matches
        // `hop=secret_text_withheld active=<bool>`.
        let line = secret_text_withheld_line(true);
        assert!(
            line.starts_with("input_order hop=secret_text_withheld active=true t="),
            "{line}"
        );
    }

    /// #2100: a dump keeps the name it starts with and gives the rest by its length; nothing
    /// that is not a type or variant name passes as one.
    #[test]
    fn a_debug_dump_is_its_name_and_a_length_unless_text_is_shown() {
        #[derive(Debug)]
        enum Action {
            Insert { text: String },
            Find(String),
            Close,
        }
        let hidden = TextShown::fixed(false, false);
        let insert = Action::Insert {
            text: "hunter2".to_string(),
        };
        assert_eq!(hidden.debug(&insert), "Insert<20 chars>");
        assert_eq!(
            hidden.debug(&Action::Find("hunter2".to_string())),
            "Find<11 chars>"
        );
        assert_eq!(hidden.debug(&Action::Close), "Close");
        // Not names: a string, a list, text printed raw by a hand-written `Debug`.
        assert_eq!(hidden.debug("hunter2"), "<9 chars>");
        assert_eq!(hidden.debug(&vec!["hunter2"]), "<11 chars>");
        assert_eq!(
            describe_debug_with("Hunter2 is my password", false),
            "<22 chars>"
        );
        assert_eq!(describe_debug_with("ls -la", false), "<6 chars>");
        // The residue: a hand-written `Debug` that prints raw text shaped like a variant.
        assert_eq!(describe_debug_with("Open(x) now", false), "Open<7 chars>");
        let shown = TextShown::fixed(true, false);
        assert_eq!(shown.debug(&insert), "Insert { text: \"hunter2\" }");
        assert_line_withholds(|shown| shown.debug(&insert), &["hunter2"]);
    }

    /// #2100: the optional and `Display` forms follow the same rule.
    #[test]
    fn optional_and_display_text_follow_the_text_rule() {
        let hidden = TextShown::fixed(true, true);
        assert_eq!(hidden.opt_text(Some("hunter2")), "Some(<7 chars>)");
        assert_eq!(hidden.opt_text(None), "None");
        assert_eq!(hidden.display("bad (hunter2"), "<12 chars>");
        let shown = TextShown::fixed(true, false);
        assert_eq!(shown.opt_text(Some("Continue? ")), "Some(\"Continue? \")");
        assert_eq!(shown.display("bad (hunter2"), "bad (hunter2");
        assert!(!TextShown::fixed(false, false).shows());
        assert!(!hidden.shows() && shown.shows());
        assert_line_withholds(|shown| shown.opt_text(Some("hunter2")), &["hunter2"]);
        assert_line_withholds(|shown| shown.display("hunter2"), &["hunter2"]);
    }

    /// #2100: the typed-action lines name the action's type and withhold its payload.
    #[test]
    fn the_typed_action_lines_withhold_the_payload() {
        #[derive(Debug)]
        enum SearchAction {
            QueryChanged(String),
        }
        let action = SearchAction::QueryChanged("hunter2pw".to_string());
        let line = |shown| {
            format!(
                "dispatching typed action: {}",
                describe_typed_action("app::SearchAction", &action, shown)
            )
        };
        assert_eq!(
            line(TextShown::fixed(false, false)),
            "dispatching typed action: app::SearchAction::QueryChanged<13 chars>"
        );
        assert_eq!(
            line(TextShown::fixed(true, false)),
            "dispatching typed action: app::SearchAction::QueryChanged(\"hunter2pw\")"
        );
        assert_line_withholds(line, &["hunter2pw"]);
    }

    #[test]
    fn target_is_the_documented_name() {
        assert_eq!(TARGET, "warp::input_order");
    }

    #[test]
    fn lines_are_greppable_without_the_target_and_redact_text() {
        for line in [
            key_down_line("a", false, false, false, false, 1),
            key_down_line("enter", false, false, false, false, 2),
            typed_characters_line("hunter2", 3),
            submit_line("hunter2"),
        ] {
            assert!(line.starts_with("input_order "), "{line}");
            assert!(!line.contains("hunter2"), "{line}");
        }
        assert!(key_down_line("enter", false, false, false, false, 2).contains("key=enter"));
        assert!(typed_characters_line("hunter2", 3).contains("<7 chars>"));
        assert!(typed_characters_line("a", 9).ends_with(" seq=9"));
    }

    /// #1444: control-sequence bytes can carry typed text (Alt+letter is ESC + letter), so a log
    /// line describes them by count unless the tester opted in.
    #[test]
    fn raw_bytes_are_described_by_count_not_content() {
        let described = describe_bytes(b"\x1bp");
        assert!(!described.contains("112"), "{described}");
        assert_eq!(described, "<2 bytes>");
    }

    #[test]
    fn round_trip_lines_carry_counts_and_times_only() {
        let write = pty_write_line(1);
        assert!(
            write.starts_with("input_order pty_write bytes=1 t="),
            "{write}"
        );
        let read = pty_read_line(3, Duration::from_micros(12_345));
        assert!(
            read.starts_with("input_order pty_read bytes=3 since_write_ms=12.3 t="),
            "{read}"
        );
        let frame = frame_line(false);
        assert!(
            frame.starts_with("input_order frame layout=false t="),
            "{frame}"
        );
    }

    #[test]
    fn the_key_frame_line_is_the_hop_the_agent_latency_cases_parse() {
        // script/realapp/appci/cases_agent_latency.py KEY_FRAME_RE matches `input_order hop=key_frame layout=<bool> t=<ms>`.
        let line = hop_line("key_frame", "layout=true");
        assert!(
            line.starts_with("input_order hop=key_frame layout=true t="),
            "{line}"
        );
        // only the first frame after the key-down is owed
        let owed = AtomicBool::new(true);
        assert!(take_frame_owed(&owed));
        assert!(!take_frame_owed(&owed));
    }

    #[test]
    fn the_key_frame_build_line_is_the_hop_the_harness_parses() {
        // script/realapp/appci/render_hops.py _KEY_FRAME_BUILD matches
        // `input_order hop=key_frame_build passes=<n> ... t=<ms>`.
        let line = hop_line("key_frame_build", "passes=1 layout_passes=1");
        assert!(
            line.starts_with("input_order hop=key_frame_build passes=1 layout_passes=1 t="),
            "{line}"
        );
    }

    #[test]
    fn the_preexec_line_is_the_hop_the_latency_split_parses() {
        // script/realapp/appci/cases.py enter_segments matches `hop=preexec_received` and ` t=`.
        let line = preexec_line();
        assert!(
            line.starts_with("input_order hop=preexec_received signal=preexec t="),
            "{line}"
        );
        // #1401: the performance counter follows `t=` on Windows only (pwsh_timing.py parses it).
        assert_eq!(line.contains(" qpc="), cfg!(windows), "{line}");
        if cfg!(windows) {
            assert!(line.contains(" qpf="), "{line}");
        }
    }

    #[test]
    fn ui_item_lines_name_the_kind_and_the_time() {
        let line = ui_item_line(
            "frame",
            Duration::from_micros(4_250),
            "render_ms=1.00 layout_paint_ms=3.25",
        );
        assert!(
            line.starts_with(
                "input_order hop=ui_item kind=frame ms=4.25 render_ms=1.00 layout_paint_ms=3.25 t="
            ),
            "{line}"
        );
        assert_eq!(UI_ITEM_BUDGET, Duration::from_millis(4));
    }

    #[test]
    fn the_os_key_event_line_carries_the_age_and_flags_only() {
        // script/realapp/appci/cases.py os_key_ages matches `hop=os_key_event age_ms=`.
        let line = os_key_event_line(0.4126, true, false);
        assert!(
            line.starts_with("input_order hop=os_key_event age_ms=0.413 enter=true ctrl=false t="),
            "{line}"
        );
    }

    #[test]
    fn the_menu_key_event_line_carries_the_flags_only() {
        // script/realapp/appci/injacct.py _MENU matches `hop=keydown_menu ctrl=<bool> cmd=<bool> t=`.
        let line = menu_key_event_line(true, false);
        assert!(
            line.starts_with("input_order hop=keydown_menu ctrl=true cmd=false t="),
            "{line}"
        );
    }

    #[test]
    fn ctrl_c_frames_are_followed_until_the_first_frame_after_the_prompt_hook() {
        // 10 ms after the write, no prompt hook yet: logged, watch continues.
        let (detail, last) = ctrl_c_frame_step(1_000_000, 1_010_000, 0, 0, true).unwrap();
        assert!(detail.starts_with("since_write_ms=10.0 since_precmd_ms=none presented=true"));
        assert!(!last);
        // prompt hook at +20 ms (epoch ms 1_020), frame at +35 ms: last, 15 ms after the hook.
        let (detail, last) = ctrl_c_frame_step(1_000_000, 1_035_000, 3, 1_020, true).unwrap();
        assert!(detail.contains("since_precmd_ms=15.0"), "{detail}");
        assert!(detail.ends_with("after_precmd=true"), "{detail}");
        assert!(last);
        // a frame before the hook of this ^C is not "after".
        let (_, last) = ctrl_c_frame_step(1_000_000, 1_015_000, 1, 1_020, true).unwrap();
        assert!(!last);
        // an old prompt hook (before the write) does not count as this ^C's.
        let (detail, last) = ctrl_c_frame_step(5_000_000, 5_010_000, 0, 1_020, true).unwrap();
        assert!(detail.contains("since_precmd_ms=none"), "{detail}");
        assert!(!last);
    }

    #[test]
    fn the_ctrl_c_watch_ends_on_time_count_or_when_unarmed() {
        assert!(ctrl_c_frame_step(0, 1_000, 0, 0, true).is_none());
        assert!(ctrl_c_frame_step(1_000_000, 1_000_000 + 601_000, 0, 0, true).is_none());
        assert!(
            ctrl_c_frame_step(1_000_000, 1_010_000, CTRL_C_WATCH_MAX_FRAMES, 0, true).is_none()
        );
    }

    #[test]
    fn the_ctrl_c_frame_line_is_a_greppable_hop() {
        let line = hop_line("ctrl_c_frame", "since_write_ms=1.0");
        assert!(
            line.starts_with("input_order hop=ctrl_c_frame since_write_ms=1.0 t="),
            "{line}"
        );
    }

    #[test]
    fn hop_lines_are_greppable_and_carry_the_detail() {
        let line = hop_line("pty_write_partial", "wrote=1 of=5");
        assert!(
            line.starts_with("input_order hop=pty_write_partial wrote=1 of=5 t="),
            "{line}"
        );
    }

    #[test]
    fn desktop_notification_lines_carry_counts_and_no_text() {
        // The real-app harness counts these lines (script/realapp/appci/cases_desk.py, NOTIFY_HOP_RE).
        let line = desktop_notification_line("sleep 6 finished after 6s", "Latest output: né");
        let rest = line
            .strip_prefix("input_order hop=desktop_notification title_chars=25 body_chars=17 t=")
            .unwrap_or_else(|| panic!("{line}"));
        assert!(
            rest.contains('.') && rest.chars().all(|c| c.is_ascii_digit() || c == '.'),
            "{line}"
        );
        assert!(
            !line.contains("sleep") && !line.contains("Latest"),
            "{line}"
        );
    }

    #[test]
    fn notify_cause_lines_have_the_shape_render_hops_parses() {
        let line = notify_cause_line("Input", "handle_editor_event_3");
        let rest = line
            .strip_prefix("input_order hop=notify_cause view=Input cause=handle_editor_event_3 t=")
            .unwrap_or_else(|| panic!("{line}"));
        assert!(
            rest.contains('.') && rest.chars().all(|c| c.is_ascii_digit() || c == '.'),
            "{line}"
        );
    }

    #[test]
    fn enter_lines_name_the_layer_and_stage_and_carry_the_detail() {
        let line = enter_line("mac_enter", "key_down_impl_exit", "handled=false ts=1.5");
        assert!(
            line.starts_with(
                "input_order hop=mac_enter stage=key_down_impl_exit handled=false ts=1.5 t="
            ),
            "{line}"
        );
        let line = enter_line("core_enter", "match_none", "step=2");
        assert!(
            line.starts_with("input_order hop=core_enter stage=match_none step=2 t="),
            "{line}"
        );
    }

    #[test]
    fn a_keydown_and_its_typed_characters_share_a_number() {
        let keydown = next_key_seq_for_keydown();
        assert_eq!(key_seq_for_typed_characters(), keydown);
        // Typed characters with no keydown before them (an input method) get their own.
        assert!(key_seq_for_typed_characters() > keydown);
    }

    #[test]
    fn key_hops_carry_the_sequence_and_the_detail() {
        let line = hop_line("kh_x", "seq=4 focused=true");
        assert!(
            line.starts_with("input_order hop=kh_x seq=4 focused=true t="),
            "{line}"
        );
    }

    #[test]
    fn only_the_first_frame_after_pty_output_is_due() {
        let owed = AtomicBool::new(false);
        assert!(!take_frame_owed(&owed));
        owed.store(true, Ordering::Relaxed);
        assert!(take_frame_owed(&owed));
        assert!(!take_frame_owed(&owed));
    }
}

#[cfg(test)]
#[path = "input_order_snapshot_tests.rs"]
mod snapshot_tests;
