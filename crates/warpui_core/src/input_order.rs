//! Debug logging for the order in which input reaches the app (#1016).
//!
//! Everything logs to the `warp::input_order` target at debug level, so a tester can confirm the
//! order keys were received, typed characters were inserted and a command was submitted with
//! `RUST_LOG=warp::input_order=debug`.
//!
//! Keystrokes can be passwords, so text is redacted to its length unless the tester opts in with
//! `WARP_INPUT_ORDER_LOG_TEXT=1`. Named keys (`enter`, `backspace`, ...) are always logged by name.
//!
//! The same target also times the rest of a keystroke's round trip (#1092), so a slow echo can be
//! placed without guessing: `pty_write` when user bytes reach the pty, `pty_read` for the first
//! output read after them (with the gap since the write), and `frame` for the first frame built
//! after that output. Only byte counts and times are logged, never the bytes.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The log target every input-order line uses.
pub const TARGET: &str = "warp::input_order";

/// Whether input-order lines would be written at all.
pub fn enabled() -> bool {
    log::log_enabled!(target: TARGET, log::Level::Debug)
}

fn text_opted_in() -> bool {
    static OPTED_IN: OnceLock<bool> = OnceLock::new();
    *OPTED_IN.get_or_init(|| {
        std::env::var("WARP_INPUT_ORDER_LOG_TEXT")
            .is_ok_and(|value| !value.is_empty() && value != "0")
    })
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

/// Describes `text` for the log: the text itself only when the tester opted in (and it is at most
/// [`MAX_LOGGED_TEXT_BYTES`] long).
pub fn describe_text(text: &str) -> String {
    describe_text_with(text, text_opted_in())
}

fn describe_text_with(text: &str, opted_in: bool) -> String {
    if opted_in && text.len() <= MAX_LOGGED_TEXT_BYTES {
        format!("{text:?}")
    } else {
        format!("<{} chars>", text.chars().count())
    }
}

/// Describes raw input bytes for a log line (control sequences can carry typed text: Alt+letter is
/// ESC + letter, kitty/CSI-u sequences carry the character, a stray paste byte stream is text): the
/// bytes themselves only when the tester opted in with `WARP_INPUT_ORDER_LOG_TEXT=1`, else only
/// their count (#1444).
pub fn describe_bytes(bytes: &[u8]) -> String {
    if text_opted_in() && bytes.len() <= MAX_LOGGED_TEXT_BYTES {
        format!("{bytes:?}")
    } else {
        format!("<{} bytes>", bytes.len())
    }
}

/// Describes raw input bytes as an escaped string (`echo\x1bi`) when the tester opted in with
/// `WARP_INPUT_ORDER_LOG_TEXT=1`, else only their count (#1490). The exact bytes of a pty write,
/// readable next to the shell's own record of the line.
pub fn describe_bytes_escaped(bytes: &[u8]) -> String {
    if text_opted_in() && bytes.len() <= MAX_LOGGED_TEXT_BYTES {
        format!("\"{}\"", bytes.escape_ascii())
    } else {
        format!("<{} bytes>", bytes.len())
    }
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
}

/// Logs a key-down being dispatched.
pub fn log_key_down(key: &str, ctrl: bool, alt: bool, shift: bool, cmd: bool) {
    if enabled() {
        let seq = next_key_seq_for_keydown();
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
    fn hop_lines_are_greppable_and_carry_the_detail() {
        let line = hop_line("pty_write_partial", "wrote=1 of=5");
        assert!(
            line.starts_with("input_order hop=pty_write_partial wrote=1 of=5 t="),
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
