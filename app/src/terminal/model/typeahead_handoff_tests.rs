use super::*;

/// A hand-off whose submitted command has been written, i.e. the state every hand-off starts
/// from.
fn after_submit() -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    assert!(handoff.on_command_written().is_empty());
    handoff
}

/// Delivers a routed user write all the way through `PtyController`.
fn type_and_flush(handoff: &mut TypeaheadHandoff, bytes: &[u8]) -> UserBytesRoute {
    let route = handoff.on_user_bytes(bytes);
    if route == UserBytesRoute::WriteCounted {
        handoff.on_user_write_flushed();
    }
    route
}

/// Drives a hand-off with typeahead in the shell's line to the point where it has written the
/// request.
fn awaiting_report() -> TypeaheadHandoff {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"echo");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    handoff
}

#[test]
fn starts_editor_owned_and_routes_like_before_the_handoff() {
    let mut handoff = TypeaheadHandoff::default();
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(!handoff.routes_keys_to_pty());
    assert!(handoff.editor_owns_input());
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::Write);
    assert_eq!(handoff.writes_in_flight(), 0);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Legacy,
        "a prompt the hand-off does not own keeps the legacy request"
    );
    assert_eq!(handoff.on_report(), ReportDisposition::InsertLegacy);
}

#[test]
fn every_key_class_goes_to_the_pty_while_it_owns_input() {
    let mut handoff = after_submit();
    assert!(handoff.routes_keys_to_pty());
    assert!(handoff.wants_terminal_focus());
    // Printable characters, Backspace, an arrow key, Enter, Ctrl-C and Ctrl-D.
    for bytes in [
        &b"ab"[..],
        &[0x7f][..],
        &b"\x1b[D"[..],
        &b"\r"[..],
        &[0x03][..],
        &[0x04][..],
    ] {
        assert_eq!(handoff.on_user_bytes(bytes), UserBytesRoute::WriteCounted);
    }
    assert_eq!(handoff.writes_in_flight(), 6);
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 2,
            precmds_seen: 0,
        }
    );
}

#[test]
fn awaiting_report_routes_to_the_editor_and_hold_holds() {
    let mut handoff = awaiting_report();
    assert!(!handoff.routes_keys_to_pty());
    assert!(handoff.editor_owns_input());
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::Write);

    assert!(handoff.defer_enter());
    assert!(handoff.routes_keys_to_pty());
    assert!(!handoff.editor_owns_input());
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter {
            held: b"ls\r".to_vec(),
            released: false,
        }
    );
}

#[test]
fn no_bytes_written_hands_back_at_the_prompt_without_a_request() {
    let mut handoff = after_submit();
    assert!(
        !handoff.try_commit_at_prompt(),
        "the submitted command has not reached its prompt yet"
    );
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert_eq!(handoff.take_report_deadline_to_arm(), None);
}

#[test]
fn no_bytes_written_commits_on_line_editor_active_without_a_request() {
    let mut handoff = after_submit();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn no_request_while_an_enter_terminated_line_is_still_queued() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"a\rb");
    handoff.on_precmd();
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    assert!(handoff.holds_queued_writes());
    assert!(handoff.is_pty_owned());

    // The shell runs `a` from the tty itself, then reaches the prompt with `b` in its line.
    assert!(!handoff.on_preexec(true));
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
}

#[test]
fn no_request_while_a_user_write_is_in_flight() {
    let mut handoff = after_submit();
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::WriteCounted);
    handoff.on_precmd();
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    assert_eq!(handoff.on_user_write_flushed(), 0);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

#[test]
fn no_request_unless_the_lifecycle_is_at_the_prompt() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"x");
    handoff.on_precmd();
    assert!(matches!(
        handoff.on_line_editor_active(false, true),
        ActiveDecision::Wait { .. }
    ));
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

#[test]
fn shell_without_input_reporting_commits_instead_of_requesting() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"x");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Committed
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn the_request_is_the_atomic_switch_to_the_editor() {
    let mut handoff = awaiting_report();
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
    // Every key after the request is routed away from the pty: no user byte follows a request.
    assert!(!handoff.routes_keys_to_pty());
    assert_eq!(handoff.on_user_bytes(b"\x7f\r"), UserBytesRoute::Write);
    assert_eq!(handoff.writes_in_flight(), 0);
    // The deadline is handed out exactly once.
    let generation = handoff.take_report_deadline_to_arm();
    assert!(generation.is_some());
    assert_eq!(handoff.take_report_deadline_to_arm(), None);
    // A second activation while the report is outstanding does not request again.
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Drain
    );
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn two_lines_typed_across_the_boundary_run_once_each_in_order() {
    let mut handoff = after_submit();
    // `a\rb\r` typed while the submitted command runs: both lines are the shell's to run.
    type_and_flush(&mut handoff, b"a\rb\r");
    let mut requests = 0;
    // The submitted command's prompt, then `a`'s, then `b`'s.
    for _ in 0..3 {
        handoff.on_precmd();
        match handoff.on_line_editor_active(true, true) {
            ActiveDecision::SendRequest => requests += 1,
            ActiveDecision::Committed => break,
            ActiveDecision::Drain => {}
            other => panic!("unexpected decision {other:?}"),
        }
        handoff.on_preexec(true);
    }
    assert_eq!(
        requests, 0,
        "a request ahead of a queued line would split it; the shell runs both lines itself"
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn deferred_enter_writes_report_then_command_then_held_keys() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    assert_eq!(handoff.on_user_bytes(b"pwd\rl"), UserBytesRoute::Held);

    // The report arrives: insert it, then run the deferred Enter.
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    // Keys typed while the deferred submit is on its way are still held behind it.
    assert_eq!(handoff.on_user_bytes(b"s"), UserBytesRoute::Held);

    // The deferred command is written; the held keys follow it, counted as its typeahead.
    assert_eq!(handoff.on_command_written(), b"pwd\rls".to_vec());
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 2,
            precmds_seen: 0,
        }
    );
}

#[test]
fn report_deadline_submits_and_a_late_report_is_stashed() {
    let mut handoff = awaiting_report();
    let generation = handoff.take_report_deadline_to_arm().unwrap();
    assert!(handoff.defer_enter());
    assert_eq!(
        handoff.on_report_deadline(generation),
        DeadlineOutcome::SubmitWithoutReport
    );
    assert!(handoff.on_command_written().is_empty());
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::Stash,
        "a report that arrives after the deferred command was written must not be inserted \
         into the editor that still shows that command"
    );
    // The command's deadline fires again: its request was answered by the command write.
    assert_eq!(
        handoff.on_report_deadline(generation),
        DeadlineOutcome::Stale
    );
}

#[test]
fn report_deadline_without_enter_just_releases_the_editor() {
    let mut handoff = awaiting_report();
    let generation = handoff.take_report_deadline_to_arm().unwrap();
    assert_eq!(
        handoff.on_report_deadline(generation),
        DeadlineOutcome::ReleaseEditor
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn deferred_enter_that_writes_no_command_gives_the_held_keys_back() {
    let mut handoff = awaiting_report();
    let generation = handoff.take_report_deadline_to_arm().unwrap();
    assert!(handoff.defer_enter());
    handoff.on_user_bytes(b"xy");
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert_eq!(
        handoff.on_report_deadline(generation),
        DeadlineOutcome::GiveUp {
            held: b"xy".to_vec()
        }
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn a_deadline_armed_for_an_earlier_request_is_void() {
    let mut handoff = awaiting_report();
    let first = handoff.take_report_deadline_to_arm().unwrap();
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    handoff.on_command_written();
    type_and_flush(&mut handoff, b"x");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(handoff.on_report_deadline(first), DeadlineOutcome::Stale);
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
}

#[test]
fn stale_counts_reset_after_a_quiet_period_and_not_after_activity() {
    let mut handoff = after_submit();
    // `read x` consumes the Enter; no prompt follows it.
    type_and_flush(&mut handoff, b"y\rz");
    handoff.on_precmd();
    let ActiveDecision::Wait {
        activity_generation,
    } = handoff.on_line_editor_active(true, true)
    else {
        panic!("a queued line must block the request");
    };

    let mut busy = TypeaheadHandoff::default();
    busy.on_command_written();
    type_and_flush(&mut busy, b"y\rz");
    busy.on_precmd();
    let ActiveDecision::Wait {
        activity_generation: busy_generation,
    } = busy.on_line_editor_active(true, true)
    else {
        panic!("a queued line must block the request");
    };
    busy.on_preexec(true);
    assert!(
        !busy.reset_stale_counts(busy_generation),
        "a preexec since the wait proves the shell is still working through the queue"
    );

    assert!(handoff.reset_stale_counts(activity_generation));
    assert_eq!(handoff.writes_in_flight(), 0);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

#[test]
fn ctrl_c_during_a_command_hands_back_at_its_prompt() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"a\rb\rc");
    type_and_flush(&mut handoff, &[0x03]);
    assert_eq!(handoff.on_interrupt(), InterruptOutcome::CommandRunning);
    assert!(!handoff.holds_queued_writes());
    // The tty dropped `a` and `b`: only the killed command's prompt follows.
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
}

#[test]
fn ctrl_c_at_the_prompt_discards_the_line() {
    let mut handoff = after_submit();
    handoff.on_precmd();
    type_and_flush(&mut handoff, b"abc");
    type_and_flush(&mut handoff, &[0x03]);
    assert_eq!(handoff.on_interrupt(), InterruptOutcome::AtPrompt);
    assert!(handoff.try_commit_at_prompt());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert_eq!(
        TypeaheadHandoff::default().on_interrupt(),
        InterruptOutcome::NotOwned
    );
}

#[test]
fn nothing_is_held_or_counted_when_nothing_was_typed() {
    let mut handoff = after_submit();
    assert!(!handoff.holds_queued_writes());
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert!(handoff.on_command_written().is_empty());
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: 1,
            precmds_seen: 0,
        }
    );
}

#[test]
fn a_command_the_shell_starts_itself_gives_the_pty_input() {
    let mut handoff = TypeaheadHandoff::default();
    assert!(!handoff.on_preexec(false));
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(handoff.on_preexec(true));
    assert!(handoff.routes_keys_to_pty());
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
}

#[test]
fn a_command_written_while_lines_are_queued_keeps_their_count() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"a\r");
    assert!(handoff.on_command_written().is_empty());
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: 3,
            precmds_seen: 0,
        }
    );
}
