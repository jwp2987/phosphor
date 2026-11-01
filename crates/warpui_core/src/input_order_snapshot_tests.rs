//! Snapshot of the shape of the input-order trace for canonical sequences (#1016, #1092): which
//! lines a typed key, an Enter and a pasted chunk produce, in order, with their sequence numbers
//! and counts. Times are redacted. A change to a line's shape shows up as a reviewed snapshot
//! diff (`cargo insta review`).

use super::*;

macro_rules! snap {
    ($name:literal, $value:expr) => {
        insta::with_settings!({
            prepend_module_to_snapshot => false,
            snapshot_path => "snapshots",
        }, {
            insta::assert_snapshot!($name, $value);
        });
    };
}

/// Replaces every `t=<time>` field, the only part of a line that varies between runs.
fn redact(line: &str) -> String {
    line.split(' ')
        .map(|word| {
            if word.starts_with("t=") {
                "t=<t>"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn render(lines: Vec<String>) -> String {
    lines
        .iter()
        .map(|line| redact(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn input_order_canonical_sequences() {
    if text_opted_in() {
        // The tester asked for key text in the log; the snapshot is the redacted shape.
        return;
    }
    let none = (false, false, false, false);
    let key = |name: &str, seq: u64| key_down_line(name, none.0, none.1, none.2, none.3, seq);

    // A key typed at the prompt: down, characters, the pty write, the echo, the frame.
    let typed_key = render(vec![
        key("a", 1),
        typed_characters_line("a", 1),
        pty_write_line(1),
        pty_read_line(1, Duration::from_micros(800)),
        frame_line(false),
    ]);

    // Ctrl-C with a modifier.
    let ctrl_c = render(vec![
        key_down_line("c", true, false, false, false, 2),
        pty_write_line(1),
    ]);

    // Enter on a one-character line, held behind the hand-off gate and released.
    let enter = render(vec![
        key("enter", 3),
        submit_line("a"),
        hop_line("kh_enter_gate", "seq=3 held=1"),
        hop_line("kh_enter_release", "seq=3 chunks=1 bytes=2"),
        pty_write_line(2),
        pty_read_line(2, Duration::from_micros(12_345)),
        frame_line(true),
    ]);

    // Text with no keydown before it (an input method commit, a paste).
    let paste = render(vec![
        typed_characters_line("hello world", 4),
        pty_write_line(11),
        pty_read_line(11, Duration::from_micros(2_000)),
        frame_line(true),
    ]);

    snap!(
        "input_order_canonical_sequences",
        format!(
            "typed key\n{typed_key}\n\nctrl-c\n{ctrl_c}\n\nenter\n{enter}\n\ninput method commit\n{paste}"
        )
    );
}
