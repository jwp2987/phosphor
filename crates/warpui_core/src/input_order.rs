//! Debug logging for the order in which input reaches the app (#1016).
//!
//! Everything logs to the `warp::input_order` target at debug level, so a tester can confirm the
//! order keys were received, typed characters were inserted and a command was submitted with
//! `RUST_LOG=warp::input_order=debug`.
//!
//! Keystrokes can be passwords, so text is redacted to its length unless the tester opts in with
//! `WARP_INPUT_ORDER_LOG_TEXT=1`. Named keys (`enter`, `backspace`, ...) are always logged by name.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

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
}
