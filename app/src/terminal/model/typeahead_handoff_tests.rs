use super::*;

/// A hand-off whose submitted command has been written, i.e. the state every hand-off starts
/// from.
fn after_submit() -> TypeaheadHandoff {
    let mut handoff = young_after_submit();
    // A command that has been running longer than `YOUNG_ENTER_WINDOW` (#933).
    handoff.last_activity_at = None;
    handoff
}

/// A hand-off whose command was written just now, so an Enter the user types may be a line typed
/// at a prompt the app has not noticed yet (#933).
fn young_after_submit() -> TypeaheadHandoff {
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
            ActiveDecision::DrainAndWait { .. } => {}
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
fn a_report_after_the_deferred_command_was_written_is_stashed() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert!(handoff.on_command_written().is_empty());
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::Stash,
        "a report arriving while the editor still shows the submitted command must wait"
    );
}

/// A line-consuming program answered the user's Enter, so no prompt follows it: the quiet
/// period releases the pty back to the editor.
fn consumed_enter_is_released_after_a_quiet_period(handoff: &mut TypeaheadHandoff) {
    // The submitted command's own prompt.
    handoff.on_precmd();
    let ActiveDecision::DrainAndWait {
        activity_generation,
    } = handoff.on_line_editor_active(true, true)
    else {
        panic!("an unanswered Enter must arm the quiet period");
    };
    assert!(handoff.reset_stale_counts(activity_generation));
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

#[test]
fn read_answered_with_y_and_enter_does_not_keep_the_pty() {
    // `read x`, answered `y` Enter.
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"y\r");
    consumed_enter_is_released_after_a_quiet_period(&mut handoff);
}

#[test]
fn a_password_and_enter_do_not_keep_the_pty() {
    // `sudo true`: the password and Enter go to sudo, which echoes nothing.
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"hunter2\r");
    consumed_enter_is_released_after_a_quiet_period(&mut handoff);
}

#[test]
fn exit_from_a_subshell_does_not_keep_the_pty() {
    // `bash`, then `exit` Enter typed into the inner (unwarpified) shell.
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"exit\r");
    consumed_enter_is_released_after_a_quiet_period(&mut handoff);
}

#[test]
fn vim_write_quit_is_not_counted_as_a_shell_line() {
    // `vim`, then `:wq` Enter on the alternate screen.
    let mut handoff = after_submit();
    assert_eq!(
        handoff.on_user_bytes_to_full_screen_program(b":wq\r"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(handoff.on_user_write_flushed(), 0);
    handoff.on_precmd();
    assert!(
        handoff.try_commit_at_prompt(),
        "vim's own Enter is answered by no prompt and must not hold the pty"
    );
}

#[test]
fn a_stale_count_reset_is_void_after_activity() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"y\r");
    handoff.on_precmd();
    let ActiveDecision::DrainAndWait {
        activity_generation,
    } = handoff.on_line_editor_active(true, true)
    else {
        panic!("an unanswered Enter must arm the quiet period");
    };
    handoff.on_preexec(true);
    assert!(
        !handoff.reset_stale_counts(activity_generation),
        "a preexec since the wait proves the shell is still working through the queue"
    );
}

#[test]
fn a_stale_count_reset_with_user_bytes_in_the_line_requests() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"y\rz");
    handoff.on_precmd();
    let ActiveDecision::Wait {
        activity_generation,
    } = handoff.on_line_editor_active(true, true)
    else {
        panic!("a queued line must block the request");
    };
    assert!(handoff.reset_stale_counts(activity_generation));
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

#[test]
fn a_queued_command_written_behind_typeahead_keeps_its_precmd_from_running_ahead() {
    let mut handoff = after_submit();
    // `a` and Enter, then `b`, typed while the editor's command runs.
    type_and_flush(&mut handoff, b"a\rb");
    // The queued-prompts panel writes a command; its kill prefix lands on `b`'s line.
    handoff.on_uncounted_command_written();
    // The editor's command and `a` reach their prompts: the queued command has not.
    for _ in 0..2 {
        handoff.on_precmd();
        assert!(
            matches!(
                handoff.on_line_editor_active(true, true),
                ActiveDecision::DrainAndWait { .. }
            ),
            "a request now would be read in cooked mode, behind the queued command"
        );
    }
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

#[test]
fn an_uncounted_command_changes_nothing_unless_the_pty_owns_input() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_uncounted_command_written();
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn a_stale_count_reset_keeps_writes_in_flight() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"y\r");
    assert_eq!(handoff.on_user_bytes(b"z"), UserBytesRoute::WriteCounted);
    handoff.on_precmd();
    let ActiveDecision::Wait {
        activity_generation,
    } = handoff.on_line_editor_active(true, true)
    else {
        panic!("a queued line must block the request");
    };
    assert!(handoff.reset_stale_counts(activity_generation));
    assert_eq!(handoff.writes_in_flight(), 1);
    assert!(
        matches!(
            handoff.on_line_editor_active(true, true),
            ActiveDecision::Wait { .. }
        ),
        "`z` is still on its way to the pty, so a request now could overtake it"
    );
    assert_eq!(handoff.on_user_write_flushed(), 0);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

#[test]
fn ctrl_c_at_the_prompt_then_typing_needs_a_request() {
    let mut handoff = after_submit();
    handoff.on_precmd();
    type_and_flush(&mut handoff, b"abc");
    type_and_flush(&mut handoff, &[0x03]);
    assert_eq!(handoff.on_interrupt(), InterruptOutcome::AtPrompt);
    assert!(!handoff.needs_request_at_prompt());
    // Typed before the settle timer, with no prompt cycle from the shell.
    type_and_flush(&mut handoff, b"ls");
    assert!(!handoff.try_commit_at_prompt());
    assert!(handoff.needs_request_at_prompt());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

#[test]
fn ctrl_c_while_holding_cancels_the_deferred_enter_and_held_keys() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    handoff.on_user_bytes(b"rm -rf build\r");
    assert!(handoff.cancel_deferred_enter());
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
    // The report still lands in the editor, which keeps input.
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert!(!handoff.cancel_deferred_enter());

    // After the deferred submit ran without writing a command.
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    handoff.on_user_bytes(b"x");
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert!(handoff.cancel_deferred_enter());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

#[test]
fn ctrl_d_reaches_the_pty_only_when_something_reads_it() {
    let mut handoff = after_submit();
    assert!(handoff.ctrl_d_reaches_a_reader(), "the command is running");
    handoff.on_precmd();
    assert!(
        !handoff.ctrl_d_reaches_a_reader(),
        "EOT on the shell's empty line would exit the shell"
    );
    type_and_flush(&mut handoff, b"ab");
    assert!(handoff.ctrl_d_reaches_a_reader());
    assert!(!TypeaheadHandoff::default().ctrl_d_reaches_a_reader());
}

#[test]
fn given_back_keys_apply_line_edits_and_drop_escape_sequences() {
    // Arrow keys (CSI, SS3) and a cursor-key report with parameters are dropped whole.
    assert_eq!(text_for_editor(b"ls\x1b[D -l\x1bOA"), "ls -l");
    assert_eq!(text_for_editor("é\x1b[1;5C".as_bytes()), "é");
    // An OSC sequence, terminated by BEL or by ST.
    assert_eq!(text_for_editor(b"a\x1b]0;title\x07b\x1b]0;t\x1b\\c"), "abc");
    // Line edits apply instead of vanishing: Backspace, ^H, ^U, ^W.
    assert_eq!(text_for_editor(b"lss\x7f -la\x08"), "ls -l");
    assert_eq!(text_for_editor(b"rm -rf /\x15echo ok"), "echo ok");
    assert_eq!(
        text_for_editor(b"git commit --amend \x17-m"),
        "git commit -m"
    );
    // Enter becomes a newline and Tab stays, so a multi-line paste survives.
    assert_eq!(text_for_editor(b"one\rtwo\n\tthree"), "one\ntwo\n\tthree");
    // ^U only erases the current line.
    assert_eq!(text_for_editor(b"one\ntwo\x15"), "one\n");
    // Backspace does not join lines.
    assert_eq!(text_for_editor(b"one\n\x7f\x7ftwo"), "one\ntwo");
    // A lone ESC does not take the next character with it; other controls are dropped.
    assert_eq!(text_for_editor(b"a\x1bb\x03c\x1b"), "abc");
}

#[test]
fn unanswered_lines_are_tracked_while_the_pty_owns_input() {
    let mut handoff = after_submit();
    assert!(
        handoff.has_unanswered_lines(),
        "the submitted command has not reached its prompt"
    );
    handoff.on_precmd();
    assert!(!handoff.has_unanswered_lines());
    type_and_flush(&mut handoff, b"a\r");
    assert!(handoff.has_unanswered_lines());
    assert!(!TypeaheadHandoff::default().has_unanswered_lines());
}

/// #826 N1: under CPU starvation the held keys were handed back to the editor with every Enter
/// turned into a newline, and the next Enter sent the typed shell lines to the agent. Held keys
/// with an Enter in them are shell lines: they never go to the editor.
#[test]
fn the_editor_takeover_window_is_tracked() {
    let start = Instant::now();
    let window = Duration::from_millis(500);
    let mut handoff = TypeaheadHandoff::default();
    assert!(!handoff.editor_took_over_within(window, start));
    handoff.note_editor_took_over(start);
    assert!(handoff.editor_took_over_within(window, start + Duration::from_millis(200)));
    assert!(!handoff.editor_took_over_within(window, start + Duration::from_millis(800)));
    // Only while the editor owns input.
    handoff.on_command_written();
    assert!(!handoff.editor_took_over_within(window, start + Duration::from_millis(200)));
}

/// B1: the request is outstanding until the report arrives. Nothing gives up on it: the
/// editor holds only what was typed after the request, so a submit now would run a fragment.
#[test]
fn a_deferred_enter_waits_for_the_report_however_long_it_takes() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    handoff.on_user_bytes(b"echo 1\recho 2\r");
    // Nothing but the report (or Ctrl-C, or the session ending) releases it.
    assert_eq!(handoff.release_after_unsubmitted_enter(), None);
    assert!(handoff.routes_keys_to_pty());
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert_eq!(handoff.on_command_written(), b"echo 1\recho 2\r".to_vec());
}

/// S2: the report arrived and its submit wrote no command: the held keys are released at once,
/// shell lines to the pty and an unfinished line to the editor -- never shell lines to the
/// editor (#826 N1).
#[test]
fn keys_held_behind_an_unsubmitted_enter_are_released_where_they_belong() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    handoff.on_user_bytes(b"echo 1\recho 2\r");
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert_eq!(
        handoff.release_after_unsubmitted_enter(),
        Some(HeldKeysRelease::ToPty(b"echo 1\recho 2\r".to_vec()))
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);

    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    handoff.on_user_bytes(b"ls -l");
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert_eq!(
        handoff.release_after_unsubmitted_enter(),
        Some(HeldKeysRelease::ToEditor(b"ls -l".to_vec()))
    );
}

/// Released keys that reduce to no editor text must not arm the shell-submit flag, or a later
/// natural-language Enter would be forced to the shell.
#[test]
fn empty_released_text_does_not_force_shell_submit() {
    assert!(!forces_shell_submit(&text_for_editor(b"\x1b[D\x03")));
    assert!(!forces_shell_submit(""));
    assert!(forces_shell_submit(&text_for_editor(b"ls")));
}

#[test]
fn the_session_ending_releases_everything() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    handoff.on_user_bytes(b"ls");
    handoff.reset_on_exit();
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(!handoff.routes_keys_to_pty());
}

#[test]
fn the_editor_takeover_window_ends_early_on_request() {
    let start = Instant::now();
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_editor_took_over(start);
    handoff.end_editor_takeover_window();
    assert!(!handoff.editor_took_over_within(Duration::from_secs(1), start));
}

#[test]
fn the_window_survives_focus_moved_by_a_key_it_applied() {
    let start = Instant::now();
    let window = Duration::from_secs(1);
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_editor_took_over(start);
    handoff.note_focus_moved_by_takeover_key();
    handoff.on_editor_focused();
    assert!(handoff.editor_took_over_within(window, start));
    // Any other focus change ends it.
    handoff.on_editor_focused();
    assert!(!handoff.editor_took_over_within(window, start));
}

#[test]
fn an_enter_typed_right_after_a_command_is_held_until_the_prompt_and_run_in_the_editor() {
    let mut handoff = young_after_submit();
    assert_eq!(handoff.on_user_bytes(b"frob"), UserBytesRoute::WriteCounted);
    handoff.on_user_write_flushed();
    // The Enter and the keys behind it are held, not written (#933).
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    assert_eq!(handoff.take_young_hold_started(), None);
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::Held);
    assert_eq!(handoff.writes_in_flight(), 0);
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    // The timer of a hold the report already ran does nothing.
    assert_eq!(handoff.expire_young_hold(id), None);
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter {
            held: b"x".to_vec(),
            released: true,
        }
    );
}

#[test]
fn a_held_enter_is_written_as_typeahead_when_no_prompt_comes() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().unwrap();
    assert_eq!(handoff.on_user_bytes(b"d"), UserBytesRoute::Held);
    assert_eq!(handoff.expire_young_hold(id), Some(b"\rd".to_vec()));
    // Counted as the routed write it now is, and its Enter is queued for the shell.
    assert_eq!(handoff.writes_in_flight(), 1);
    assert!(handoff.has_unanswered_lines());
    assert_eq!(handoff.expire_young_hold(id), None);
}

#[test]
fn an_enter_is_not_held_on_an_empty_line_or_for_an_old_command() {
    let mut handoff = young_after_submit();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    let mut old = after_submit();
    type_and_flush(&mut old, b"abc");
    assert_eq!(old.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
}

#[test]
fn signal_and_eof_bytes_are_never_held_and_drop_a_held_enter() {
    for signal in [0x03u8, 0x04, 0x1a, 0x1c] {
        let mut handoff = young_after_submit();
        type_and_flush(&mut handoff, b"x");
        assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
        let id = handoff.take_young_hold_started().unwrap();
        // The byte goes out at once, counted, and the held Enter is dropped.
        assert_eq!(
            handoff.on_user_bytes(&[signal]),
            UserBytesRoute::WriteCounted
        );
        assert!(!handoff.has_young_hold());
        assert_eq!(handoff.expire_young_hold(id), None);
    }
    // Not even as the first byte of a would-be hold.
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"x");
    assert_eq!(
        handoff.on_user_bytes(b"\r\x03"),
        UserBytesRoute::WriteCounted
    );
    assert!(!handoff.has_young_hold());
}

#[test]
fn an_expired_timer_cannot_release_a_later_hold() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let first = handoff.take_young_hold_started().unwrap();
    handoff.on_user_bytes(&[0x03]);
    handoff.on_user_write_flushed();
    assert_eq!(handoff.on_interrupt(), InterruptOutcome::CommandRunning);
    type_and_flush(&mut handoff, b"def");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let second = handoff.take_young_hold_started().unwrap();
    assert_ne!(first, second);
    assert_eq!(handoff.expire_young_hold(first), None);
    assert!(handoff.has_young_hold());
    assert_eq!(handoff.expire_young_hold(second), Some(b"\r".to_vec()));
}

#[test]
fn a_hold_does_not_outlive_the_state_it_was_made_in() {
    // An uncounted command's kill prefix ends the line the Enter belongs to.
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    handoff.on_uncounted_command_written();
    assert!(!handoff.has_young_hold());
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::WriteCounted);

    // A command from the editor takes the held keys behind it.
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::Held);
    assert_eq!(handoff.on_command_written(), b"\rx".to_vec());
    assert!(!handoff.has_young_hold());

    // A hold in a state that cannot own one is dropped, not appended to.
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().unwrap();
    handoff.reset_on_exit();
    assert_eq!(handoff.expire_young_hold(id), None);
}

#[test]
fn only_a_line_begun_right_after_a_command_gets_the_routed_submit() {
    // Began young; the Enter is typed in the editor while the report is outstanding.
    let mut young = young_after_submit();
    type_and_flush(&mut young, b"frob");
    young.on_precmd();
    assert_eq!(
        young.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(young.defer_enter());
    assert_eq!(young.on_report(), ReportDisposition::InsertThenSubmitYoung);
    // Typed ahead of a command that had been running a while: shell typeahead, as before.
    let mut old = awaiting_report();
    assert!(old.defer_enter());
    assert_eq!(old.on_report(), ReportDisposition::InsertThenSubmit);
}

#[test]
fn an_enter_is_held_when_the_prompt_is_known_but_the_request_has_not_gone_out() {
    // The precmd was processed before the Enter, but the request waits for the typed bytes to
    // be flushed: the Enter must not run the line as shell typeahead (#933).
    let mut handoff = young_after_submit();
    assert_eq!(handoff.on_user_bytes(b"pwd"), UserBytesRoute::WriteCounted);
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    handoff.on_user_write_flushed();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
}

#[test]
fn the_deferred_submit_stays_pending_until_it_runs_or_is_cancelled() {
    // The view may wait for a detection verdict before running the Enter (#933); Ctrl-C meanwhile
    // cancels it, which the wait must see.
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"frob");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(!handoff.deferred_submit_pending());
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert!(handoff.deferred_submit_pending());
    assert!(handoff.cancel_deferred_enter());
    assert!(!handoff.deferred_submit_pending());
}
