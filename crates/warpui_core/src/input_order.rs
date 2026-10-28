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
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Describes `text` for the log: the text itself only when the tester opted in.
pub fn describe_text(text: &str) -> String {
    if text_opted_in() {
        format!("{text:?}")
    } else {
        format!("<{} chars>", text.chars().count())
    }
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

fn key_down_line(key: &str, ctrl: bool, alt: bool, shift: bool, cmd: bool) -> String {
    format!(
        "{LINE_PREFIX} keydown key={} ctrl={ctrl} alt={alt} shift={shift} cmd={cmd} t={}",
        describe_key(key),
        now_ms()
    )
}

fn typed_characters_line(chars: &str) -> String {
    format!(
        "{LINE_PREFIX} typed_characters text={} t={}",
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
        log::debug!(target: TARGET, "{}", key_down_line(key, ctrl, alt, shift, cmd));
    }
}

/// Logs typed characters being dispatched.
pub fn log_typed_characters(chars: &str) {
    if enabled() {
        log::debug!(target: TARGET, "{}", typed_characters_line(chars));
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
    fn target_is_the_documented_name() {
        assert_eq!(TARGET, "warp::input_order");
    }

    #[test]
    fn lines_are_greppable_without_the_target_and_redact_text() {
        for line in [
            key_down_line("a", false, false, false, false),
            key_down_line("enter", false, false, false, false),
            typed_characters_line("hunter2"),
            submit_line("hunter2"),
        ] {
            assert!(line.starts_with("input_order "), "{line}");
            assert!(!line.contains("hunter2"), "{line}");
        }
        assert!(key_down_line("enter", false, false, false, false).contains("key=enter"));
        assert!(typed_characters_line("hunter2").contains("<7 chars>"));
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
    fn only_the_first_frame_after_pty_output_is_due() {
        let owed = AtomicBool::new(false);
        assert!(!take_frame_owed(&owed));
        owed.store(true, Ordering::Relaxed);
        assert!(take_frame_owed(&owed));
        assert!(!take_frame_owed(&owed));
    }
}
