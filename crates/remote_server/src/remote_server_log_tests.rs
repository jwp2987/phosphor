//! Tests for [`super::RemoteServerLog`].
//!
//! The pin (`02b53fcd8`) ships `remote_server_log.rs` with no tests. The
//! bounded tail is the only part of it that can be wrong — it must keep the
//! *last* `LOG_TAIL_MAX_LINES` lines and drop the oldest — so that is what
//! these cover, along with the char-truncation and drain semantics.

use super::*;

#[test]
fn drain_returns_none_when_empty() {
    let log = RemoteServerLog::new();
    assert_eq!(log.drain(), None);
}

#[test]
fn drain_returns_pushed_lines_joined_by_newline() {
    let log = RemoteServerLog::new();
    log.push("first".to_owned());
    log.push("second".to_owned());

    assert_eq!(log.drain(), Some("first\nsecond".to_owned()));
}

#[test]
fn buffer_keeps_the_last_lines_and_drops_the_oldest() {
    let log = RemoteServerLog::new();
    for i in 0..(LOG_TAIL_MAX_LINES + 3) {
        log.push(format!("line {i}"));
    }

    let drained = log.drain().expect("buffer is not empty");
    let lines: Vec<&str> = drained.lines().collect();

    assert_eq!(
        lines.len(),
        LOG_TAIL_MAX_LINES,
        "buffer must retain at most {LOG_TAIL_MAX_LINES} lines, got {lines:?}"
    );
    // Pushed 0..=7 with a cap of 5, so the retained window is 3..=7 —
    // oldest-first within the window.
    let expected: Vec<String> = (3..(LOG_TAIL_MAX_LINES + 3))
        .map(|i| format!("line {i}"))
        .collect();
    assert_eq!(lines, expected);
}

#[test]
fn drain_empties_the_buffer() {
    let log = RemoteServerLog::new();
    log.push("only".to_owned());

    assert_eq!(log.drain(), Some("only".to_owned()));
    assert_eq!(log.drain(), None, "a second drain must see an empty buffer");
}

#[test]
fn clones_share_one_buffer() {
    let log = RemoteServerLog::new();
    let writer = log.clone();
    writer.push("written through the clone".to_owned());

    assert_eq!(log.drain(), Some("written through the clone".to_owned()));
}

#[test]
fn oversized_content_is_truncated_to_the_tail_with_an_ellipsis() {
    let log = RemoteServerLog::new();
    // One line longer than the char budget: the tail (end) must survive, the
    // head must not.
    let head = "H".repeat(LOG_TAIL_MAX_CHARS);
    let tail = "T".repeat(LOG_TAIL_MAX_CHARS);
    log.push(format!("{head}{tail}"));

    let drained = log.drain().expect("buffer is not empty");
    assert!(
        drained.starts_with('…'),
        "truncated output must be marked with a leading ellipsis"
    );
    assert_eq!(
        drained.chars().count(),
        LOG_TAIL_MAX_CHARS + 1,
        "truncated output is the ellipsis plus exactly {LOG_TAIL_MAX_CHARS} chars"
    );
    assert!(
        drained.ends_with(&tail),
        "truncation must keep the tail, which is the useful diagnostic context"
    );
    assert!(
        !drained.contains('H'),
        "truncation must drop the head of an oversized payload"
    );
}

#[test]
fn content_at_the_char_budget_is_not_truncated() {
    let log = RemoteServerLog::new();
    let exact = "x".repeat(LOG_TAIL_MAX_CHARS);
    log.push(exact.clone());

    assert_eq!(
        log.drain(),
        Some(exact),
        "only content strictly over the budget is truncated"
    );
}

#[test]
fn truncation_counts_characters_not_bytes() {
    let log = RemoteServerLog::new();
    // Multi-byte characters: a byte-based truncation would split one and
    // panic, and would keep the wrong number of characters.
    let line = "é".repeat(LOG_TAIL_MAX_CHARS + 10);
    log.push(line);

    let drained = log.drain().expect("buffer is not empty");
    assert_eq!(drained.chars().count(), LOG_TAIL_MAX_CHARS + 1);
    assert!(drained.starts_with('…'));
}
