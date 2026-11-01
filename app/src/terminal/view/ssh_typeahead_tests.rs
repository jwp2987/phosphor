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

#[test]
fn burst_continues_only_for_a_quick_follow_up() {
    let now = Instant::now();
    assert!(!burst_continues(None, now));
    assert!(burst_continues(Some(now), now));
    assert!(burst_continues(
        Some(now),
        now + SSH_TYPEAHEAD_BURST_GAP - Duration::from_millis(1)
    ));
    assert!(!burst_continues(Some(now), now + SSH_TYPEAHEAD_BURST_GAP));
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

/// #1345: a key typed at the hand-off could be erased by the reinitialization of the input editor
/// that the view's handling of the remote prompt's precmd performs. Delivery (and so the end of
/// the hold) waits for that handling, briefly, and never for good.
#[test]
fn delivery_waits_for_the_view_to_handle_the_prompt_but_only_up_to_the_cap() {
    assert!(delivery_waits_for_view_precmd(true, Duration::ZERO));
    assert!(delivery_waits_for_view_precmd(
        true,
        SSH_TYPEAHEAD_VIEW_PRECMD_WAIT_CAP - Duration::from_millis(1)
    ));
    assert!(
        !delivery_waits_for_view_precmd(true, SSH_TYPEAHEAD_VIEW_PRECMD_WAIT_CAP),
        "a miscount must not hold keys for good"
    );
    assert!(
        !delivery_waits_for_view_precmd(false, Duration::ZERO),
        "nothing pending, nothing to wait for"
    );
}
