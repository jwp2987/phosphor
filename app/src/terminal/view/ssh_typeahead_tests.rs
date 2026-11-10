use super::*;

fn typed(s: &str) -> HeldInput {
    HeldInput::from_chars(s)
}

#[test]
fn keys_are_held_in_order_and_taken_whole() {
    let mut buffer = SshTypeahead::default();
    for key in ["e", "c", "h", "o", " ", "h", "i", "\r", "l", "s", "\r"] {
        assert_eq!(buffer.push(typed(key)), SshTypeaheadPush::Held);
    }
    assert!(!buffer.is_empty());
    assert_eq!(buffer.len_bytes(), 11);
    assert_eq!(
        buffer.take(),
        vec![
            HeldInput::Typed("echo hi".to_owned()),
            HeldInput::Key("\r".to_owned()),
            HeldInput::Typed("ls".to_owned()),
            HeldInput::Key("\r".to_owned()),
        ],
        "printable runs merge, Enter stays its own key, order is kept"
    );
    assert!(buffer.is_empty());
    assert_eq!(buffer.len_bytes(), 0);
}

#[test]
fn control_keys_and_pastes_are_kept_exactly() {
    let mut buffer = SshTypeahead::default();
    buffer.push(typed("ab"));
    buffer.push(typed("\u{7f}"));
    buffer.push(HeldInput::Paste("x\ny".to_owned()));
    buffer.push(typed("\u{15}"));
    assert_eq!(
        buffer.take(),
        vec![
            HeldInput::Typed("ab".to_owned()),
            HeldInput::Key("\u{7f}".to_owned()),
            HeldInput::Paste("x\ny".to_owned()),
            HeldInput::Key("\u{15}".to_owned()),
        ]
    );
}

#[test]
fn overflow_drops_everything_once_and_ignores_the_rest() {
    let mut buffer = SshTypeahead::default();
    let chunk = "a".repeat(SSH_TYPEAHEAD_MAX_BYTES / 2);
    assert_eq!(
        buffer.push(HeldInput::Paste(chunk.clone())),
        SshTypeaheadPush::Held
    );
    assert_eq!(
        buffer.push(HeldInput::Paste(chunk.clone())),
        SshTypeaheadPush::Held,
        "exactly the bound still fits"
    );
    assert_eq!(buffer.push(typed("z")), SshTypeaheadPush::Overflowed);
    assert!(buffer.is_empty(), "a truncated command line must never run");
    assert!(buffer.overflowed());
    assert_eq!(buffer.push(typed("y")), SshTypeaheadPush::Ignored);
    assert!(buffer.take().is_empty());
    buffer.clear();
    assert!(!buffer.overflowed());
    assert_eq!(buffer.push(typed("y")), SshTypeaheadPush::Held);
}

#[test]
fn debug_output_never_shows_the_held_text() {
    let mut buffer = SshTypeahead::default();
    buffer.push(typed("hunter2"));
    let shown = format!("{buffer:?}");
    assert!(!shown.contains("hunter2"), "{shown}");
}

#[test]
fn unexecuted_text_follows_edits_and_never_runs_anything() {
    let mut buffer = SshTypeahead::default();
    buffer.push(typed("echo hi"));
    buffer.push(typed("\r"));
    buffer.push(typed("lss"));
    buffer.push(typed("\u{7f}"));
    buffer.push(typed("\r"));
    assert_eq!(buffer.unexecuted_text(), "echo hi\nls");

    let mut buffer = SshTypeahead::default();
    buffer.push(typed("first"));
    buffer.push(typed("\r"));
    buffer.push(typed("wrong"));
    buffer.push(typed("\u{15}"));
    buffer.push(typed("one two"));
    buffer.push(typed("\u{17}"));
    buffer.push(typed("\u{1b}[A"));
    assert_eq!(buffer.unexecuted_text(), "first\none ");

    let mut buffer = SshTypeahead::default();
    buffer.push(HeldInput::Paste("a\r\nb".to_owned()));
    assert_eq!(buffer.unexecuted_text(), "a\nb");
}

/// #1332: the held Enter was delivered at the `Bootstrapped` hook, before the first prompt's Precmd
/// was processed, and refused (`HistoryNotAppendable`). Delivery waits while that is the only
/// obstacle, and never longer than the cap.
#[test]
fn delivery_waits_for_the_first_precmd_but_only_up_to_the_cap() {
    assert!(delivery_waits_for_first_precmd(true, Duration::ZERO));
    assert!(delivery_waits_for_first_precmd(
        true,
        SSH_TYPEAHEAD_PRECMD_WAIT_CAP - Duration::from_millis(1)
    ));
    assert!(
        !delivery_waits_for_first_precmd(true, SSH_TYPEAHEAD_PRECMD_WAIT_CAP),
        "the keys are never kept back for good"
    );
    assert!(
        !delivery_waits_for_first_precmd(false, Duration::ZERO),
        "a session that can take the command is not waited for"
    );
}

/// #1959: keys read back from the tty become the held inputs the live key path would have made
/// of them, and nothing else: their concatenation is the bytes, in order.
#[test]
fn tty_bytes_become_the_held_inputs_the_key_path_would_have_made() {
    let seq = |s: &str| HeldInput::Seq(s.to_owned());
    let key = |s: &str| HeldInput::Key(s.to_owned());
    let text = |s: &str| HeldInput::Typed(s.to_owned());
    assert_eq!(held_inputs_from_tty_bytes(b""), Some(vec![]));
    assert_eq!(
        held_inputs_from_tty_bytes("echo h\u{e9}\rls\u{7f}\u{15}".as_bytes()),
        Some(vec![
            text("echo h\u{e9}"),
            key("\r"),
            text("ls"),
            key("\u{7f}"),
            key("\u{15}"),
        ])
    );
    assert_eq!(
        held_inputs_from_tty_bytes(b"a\x1b[Ab\x1b[1;5Cc\x1bOPd\x1bfe\tf\x1b"),
        Some(vec![
            text("a"),
            seq("\u{1b}[A"),
            text("b"),
            seq("\u{1b}[1;5C"),
            text("c"),
            seq("\u{1b}OP"),
            text("d"),
            seq("\u{1b}f"),
            text("e"),
            seq("\t"),
            text("f"),
            seq("\u{1b}"),
        ])
    );
    // A `\r\n` pair is one Enter; an Escape before a control key is its own key; a sequence cut
    // off by a control key or by text outside its alphabet ends there.
    assert_eq!(
        held_inputs_from_tty_bytes("x\r\n\u{1b}\r\u{1b}[1\ry\u{1b}[\u{e9}".as_bytes()),
        Some(vec![
            text("x"),
            key("\r\n"),
            seq("\u{1b}"),
            key("\r"),
            seq("\u{1b}[1"),
            key("\r"),
            text("y"),
            seq("\u{1b}["),
            text("\u{e9}"),
        ])
    );
    // Every split keeps the bytes.
    let samples: [&[u8]; 3] = [
        b"echo hi\rls -la\x1b[D\x1b[3~\x7f\r",
        b"\x1b\x1b[\x1bO",
        b"\0\x01a\x1b\x00",
    ];
    for bytes in samples {
        let joined: Vec<u8> = held_inputs_from_tty_bytes(bytes)
            .expect("UTF-8")
            .into_iter()
            .flat_map(|input| input.into_parts().1.into_bytes())
            .collect();
        assert_eq!(joined, bytes);
    }
}

/// #1959: bytes that are not keys of ours, or more than a connect may hold, are not turned into
/// held inputs (the caller keeps them as bytes for the pty).
#[test]
fn tty_bytes_that_cannot_be_held_exactly_are_refused() {
    assert_eq!(held_inputs_from_tty_bytes(b"ls\xff\r"), None);
    assert_eq!(
        held_inputs_from_tty_bytes(&vec![b'a'; SSH_TYPEAHEAD_MAX_BYTES + 1]),
        None
    );
    assert!(held_inputs_from_tty_bytes(&vec![b'a'; SSH_TYPEAHEAD_MAX_BYTES]).is_some());
}
