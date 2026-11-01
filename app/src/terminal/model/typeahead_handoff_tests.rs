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

/// The keys of a young hold as the view's input queue holds them: the hand-off keeps no bytes, so
/// every key after the one that started the hold goes to the queue (as one chunk per key event)
/// and never reaches the hand-off.
fn hold_young_enter(handoff: &mut TypeaheadHandoff, enter: &[u8]) -> (u64, Vec<Vec<u8>>) {
    assert_eq!(handoff.on_user_bytes(enter), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    (id, vec![enter.to_vec()])
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
    // The Enter is held, not written (#933); the keys behind it are the queue's.
    let (id, mut queued) = hold_young_enter(&mut handoff, b"\r");
    assert_eq!(handoff.take_young_hold_started(), None);
    queued.push(b"x".to_vec());
    assert_eq!(handoff.writes_in_flight(), 0);
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    // The give-up of a hold the report has not run yet keeps waiting for the report.
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Keep);
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    // The view splits the queue's keys at the Enter: the Enter runs in the editor, the rest is
    // held behind it (`prepend_held_keys` until the gate carries it).
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::SubmitOnReport)]
    );
    assert!(handoff.prepend_held_keys(&queued.concat()[1..]));
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
    let (id, mut queued) = hold_young_enter(&mut handoff, b"\r");
    queued.push(b"d".to_vec());
    assert_eq!(
        handoff.expire_queued_young_hold(id),
        YoungHoldExpiry::Release
    );
    assert_eq!(
        handoff.route_expired_young_chunks(queued),
        vec![b"\rd".to_vec()]
    );
    // Counted as the routed write it now is, and its Enter is queued for the shell.
    assert_eq!(handoff.writes_in_flight(), 1);
    assert!(handoff.has_unanswered_lines());
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);
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
        assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);
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
    let (first, _) = hold_young_enter(&mut handoff, b"\r");
    handoff.on_user_bytes(&[0x03]);
    handoff.on_user_write_flushed();
    assert_eq!(handoff.on_interrupt(), InterruptOutcome::CommandRunning);
    type_and_flush(&mut handoff, b"def");
    let (second, queued) = hold_young_enter(&mut handoff, b"\r");
    assert_ne!(first, second);
    assert_eq!(
        handoff.expire_queued_young_hold(first),
        YoungHoldExpiry::Gone
    );
    assert!(handoff.has_young_hold());
    assert_eq!(
        handoff.expire_queued_young_hold(second),
        YoungHoldExpiry::Release
    );
    assert_eq!(
        handoff.route_expired_young_chunks(queued),
        vec![b"\r".to_vec()]
    );
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

    // A command from the editor takes the held keys behind it (the view writes them).
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    assert!(handoff.on_command_written().is_empty());
    assert!(!handoff.has_young_hold());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::FollowCommand)]
    );

    // A hold in a state that cannot own one is dropped.
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    handoff.reset_on_exit();
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);
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
/// #934: the view's handling of a prompt reinitializes the editor. A request (which hands input
/// to the editor) written before that would let the editor take keys that are then erased, so
/// the decision waits for the view, and keys keep going to the pty meanwhile.
#[test]
fn no_request_while_the_view_has_yet_to_handle_the_prompt() {
    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"echo");
    handoff.on_precmd();
    handoff.track_view_precmds();
    handoff.note_precmd_pending_view();
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    assert!(
        handoff.routes_keys_to_pty(),
        "the pty keeps the keys until the view has the prompt"
    );
    assert_eq!(
        type_and_flush(&mut handoff, b"x"),
        UserBytesRoute::WriteCounted
    );
    handoff.on_precmd_viewed();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

#[test]
fn the_editor_does_not_take_over_before_the_view_has_the_prompt() {
    let mut handoff = after_submit();
    handoff.on_precmd();
    handoff.track_view_precmds();
    handoff.note_precmd_pending_view();
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    assert!(handoff.routes_keys_to_pty());
    handoff.on_precmd_viewed();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

#[test]
fn a_view_event_without_a_pending_prompt_is_harmless() {
    let mut handoff = after_submit();
    handoff.on_precmd_viewed();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

#[test]
fn keys_typed_while_the_request_is_outstanding_queue_behind_a_held_enter() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, mut queued) = hold_young_enter(&mut handoff, b"\r");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    // The request is out; the editor owns input in general, but not ahead of the held Enter.
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
    assert!(handoff.routes_keys_to_pty());
    assert!(handoff.wants_terminal_focus());
    // The keys queue behind the Enter in the view's queue, never reaching the hand-off.
    for key in [&b"d"[..], b"e", b"f"] {
        queued.push(key.to_vec());
    }
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::SubmitOnReport)]
    );
    assert!(handoff.prepend_held_keys(&queued.concat()[1..]));
    // The order typed is the order executed: the Enter, then d, e, f.
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter {
            held: b"def".to_vec(),
            released: true,
        }
    );
}

#[test]
fn without_a_held_enter_the_editor_owns_keys_while_the_request_is_outstanding() {
    let handoff = awaiting_report();
    assert!(!handoff.routes_keys_to_pty());
    assert!(!handoff.wants_terminal_focus());
}

/// #1215: an Enter deferred behind the hand-off's own report arms a patience deferral; giving it
/// up hands input back to the editor with the held keys, without running anything. A report that
/// arrives first ends the deferral, so a late give-up finds nothing.
#[test]
fn an_enter_behind_an_unanswered_report_is_given_up_without_running() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let id = handoff
        .take_report_deferral_started()
        .expect("the plain report deferral arms a patience timer");
    assert_eq!(handoff.take_report_deferral_started(), None, "armed once");
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    assert_eq!(handoff.give_up_report_deferral(id), Some(b"ls".to_vec()));
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert_eq!(handoff.give_up_report_deferral(id), None, "over already");

    let mut answered = awaiting_report();
    assert!(answered.defer_enter());
    let id = answered.take_report_deferral_started().expect("armed");
    assert_eq!(answered.on_report(), ReportDisposition::InsertThenSubmit);
    assert_eq!(
        answered.give_up_report_deferral(id),
        None,
        "the report released the Enter; the late timer must not touch the submit"
    );
}

#[test]
fn a_young_line_is_submitted_only_on_an_actual_verdict() {
    let wait = YOUNG_DETECTION_CAP / 2;
    // The verdict is pending: wait, however long it takes up to the cap.
    assert_eq!(
        young_submit_decision(true, true, Duration::ZERO),
        YoungSubmit::Wait
    );
    assert_eq!(young_submit_decision(true, true, wait), YoungSubmit::Wait);
    assert_eq!(
        young_submit_decision(true, true, YOUNG_DETECTION_CAP - Duration::from_millis(1)),
        YoungSubmit::Wait
    );
    // It landed: submit, with no further delay.
    assert_eq!(
        young_submit_decision(true, false, Duration::ZERO),
        YoungSubmit::Submit
    );
    // No classifier at all: keep the text.
    assert_eq!(
        young_submit_decision(false, false, Duration::ZERO),
        YoungSubmit::KeepInEditor
    );
    assert_eq!(
        young_submit_decision(false, true, wait),
        YoungSubmit::KeepInEditor
    );
    // #1174: a classifier that is running but silent past the cap no longer swallows the
    // Enter: it submits on the Enter-time rules.
    assert_eq!(
        young_submit_decision(true, true, YOUNG_DETECTION_CAP),
        YoungSubmit::SubmitWithoutVerdict
    );
}

#[test]
fn editor_text_precedes_report_only_outside_plain_editor_ownership() {
    // #949: keys typed into the editor while the report is outstanding are followed by the
    // report in front of them, so the editor must not auto-pair them.
    assert!(!TypeaheadHandoff::default().editor_text_precedes_report());
    let mut handoff = awaiting_report();
    assert!(handoff.editor_text_precedes_report());
    handoff.on_report();
    assert!(!handoff.editor_text_precedes_report());
    assert!(after_submit().editor_text_precedes_report());
}

#[test]
fn a_report_for_a_line_begun_right_after_a_command_is_young() {
    // The typed keys went to the pty before the app knew the prompt was back; the report brings
    // them to the editor, where an Enter on them waits for a classification (#933).
    let mut young = young_after_submit();
    type_and_flush(&mut young, b"explain");
    young.on_precmd();
    assert_eq!(
        young.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(young.on_report(), ReportDisposition::InsertYoung);
    // Typeahead for a command that had been running a while is not.
    let mut old = awaiting_report();
    assert_eq!(old.on_report(), ReportDisposition::Insert);
}

#[test]
fn an_eof_flushes_a_held_enter_ahead_of_itself_but_other_signals_drop_it() {
    // ^D does not flush the tty's typeahead: `cat` typed into right after it started, then ^D,
    // must receive the lines (#933).
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"line one");
    let (id, mut queued) = hold_young_enter(&mut handoff, b"\r");
    queued.push(b"line two\r".to_vec());
    assert_eq!(
        handoff.take_queued_young_hold_for_signal(&[0x04]),
        Some((id, true)),
        "the keys go out in front of a lone EOF"
    );
    assert!(!handoff.has_young_hold());
    // Routed as one user write, counting both Enters, plus the submitted command's.
    let mut combined = queued.concat();
    combined.push(0x04);
    assert_eq!(combined, b"\rline two\r\x04".to_vec());
    assert_eq!(
        handoff.on_user_bytes(&combined),
        UserBytesRoute::WriteCounted
    );
    assert!(handoff.has_unanswered_lines());
    // ^C (and ^Z, ^\) flush the tty's typeahead anyway, and ^D with nothing held is a no-op.
    for bytes in [&[0x03][..], &[0x04, 0x03]] {
        let mut handoff = young_after_submit();
        type_and_flush(&mut handoff, b"x");
        let (id, _) = hold_young_enter(&mut handoff, b"\r");
        assert_eq!(
            handoff.take_queued_young_hold_for_signal(bytes),
            Some((id, false)),
            "the keys are void"
        );
    }
    assert_eq!(
        young_after_submit().take_queued_young_hold_for_signal(&[0x04]),
        None
    );
}

/// #1089: keys typed into a running command the hand-off does not own (here a queued command,
/// so the editor owned input when it started) are taken over like typeahead for an editor
/// command: counted, reported by a request written behind all of them, and inserted literally
/// by the editor until the report is in.
#[test]
fn keys_for_a_command_the_handoff_does_not_own_are_adopted() {
    let mut handoff = TypeaheadHandoff::default();
    assert!(handoff.adopt_untracked_typeahead());
    assert!(handoff.routes_keys_to_pty());
    assert!(
        handoff.editor_text_precedes_report(),
        "the editor cannot see the line the report will put in front of its text"
    );
    assert_eq!(
        type_and_flush(&mut handoff, b"echo TY$((20+1))-$("),
        UserBytesRoute::WriteCounted
    );
    assert!(
        handoff.holds_queued_writes(),
        "a queued command's kill prefix must not wipe the unreported line"
    );
    // Before the command's own prompt nothing can be asked for: the line may still be read by
    // the command.
    assert_eq!(
        handoff.on_line_editor_active(false, true),
        ActiveDecision::Wait {
            activity_generation: handoff.activity_generation,
        }
    );
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(handoff.editor_text_precedes_report());
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(!handoff.editor_text_precedes_report());
}

/// #1089: the legacy request goes out whenever the line editor is active, so it can be in the
/// tty ahead of keys the user types afterwards: a legacy request at a prompt, then a queued
/// command drained right behind it, and keys typed into that command. (The remote bootstrap tail
/// of #1085 is not adopted -- its block is from before the bootstrap precmd -- and #1085's
/// connect gate drops keys there.) Those keys are adopted: the legacy report that answers the early request does not end ownership
/// (it is stashed), and a new request written behind them collects the rest of the line --
/// nothing stays in the shell's line to prefix the next command (`)exit`).
#[test]
fn keys_written_behind_a_legacy_request_are_collected_by_a_second_request() {
    let mut handoff = TypeaheadHandoff::default();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Legacy
    );
    assert!(handoff.adopt_untracked_typeahead());
    assert_eq!(
        type_and_flush(&mut handoff, b")"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::Stash,
        "the early report is kept for the front of the line, not inserted ahead of `)`"
    );
    assert!(handoff.holds_queued_writes());
    handoff.on_precmd();
    assert!(handoff.needs_request_at_prompt());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert!(!handoff.holds_queued_writes());
}

/// #1089: an Enter typed in the editor while the adopted line's report is outstanding waits
/// for it, so the submitted line is the whole line, in typed order.
#[test]
fn an_editor_enter_waits_for_the_report_of_an_adopted_line() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.age_last_activity_for_test();
    assert!(handoff.adopt_untracked_typeahead());
    type_and_flush(&mut handoff, b"echo $(da");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(handoff.defer_enter());
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
}

/// #1089: only an editor-owned hand-off adopts; one that already owns the keys, or waits for a
/// report, keeps its own counts.
#[test]
fn adoption_never_disturbs_a_handoff_that_is_already_engaged() {
    let mut owned = after_submit();
    type_and_flush(&mut owned, b"ls");
    let before = owned.state().clone();
    assert!(!owned.adopt_untracked_typeahead());
    assert_eq!(owned.state(), &before);

    let mut awaiting = awaiting_report();
    assert!(!awaiting.adopt_untracked_typeahead());
    assert_eq!(awaiting.state(), &HandoffState::AwaitingReport);
}

/// #1089: user bytes the hand-off did not adopt (an agent command, a block from before the
/// bootstrap precmd) may be in the shell's line, and the legacy report puts them in front of the
/// editor's text. Until a report requested behind them is in, the editor inserts literally; a
/// report for a request written before them does not count, and a command's kill prefix ends
/// the line.
#[test]
fn untracked_line_bytes_keep_the_editor_literal_until_a_covering_report() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.note_untracked_line_bytes();
    assert_eq!(handoff.on_user_bytes(b"echo \"a b"), UserBytesRoute::Write);
    assert!(handoff.editor_text_precedes_report());
    // The report of the earlier request: the bytes are behind it in the tty.
    assert_eq!(handoff.on_report(), ReportDisposition::InsertLegacy);
    assert!(handoff.editor_text_precedes_report());
    // The next prompt's legacy request goes out behind them; its report brings them back.
    handoff.on_legacy_request_written();
    assert!(handoff.editor_text_precedes_report());
    assert_eq!(handoff.on_report(), ReportDisposition::InsertLegacy);
    assert!(!handoff.editor_text_precedes_report());

    // A command's kill prefix ends the line as well.
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_untracked_line_bytes();
    handoff.on_uncounted_command_written();
    assert!(!handoff.editor_text_precedes_report());
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_untracked_line_bytes();
    handoff.on_command_written();
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert!(!handoff.editor_text_precedes_report());

    // Keys typed behind an outstanding hand-off request are not covered by its report.
    let mut handoff = awaiting_report();
    handoff.note_untracked_line_bytes();
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert!(handoff.editor_text_precedes_report());

    // Noting is a no-op while the pty owns input: those keys are counted already.
    let mut handoff = after_submit();
    handoff.note_untracked_line_bytes();
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert!(!handoff.editor_text_precedes_report());
}

/// #1089: the hand-off's own request covers untracked bytes written before it, like a legacy
/// one: its report ends literal insertion. Reports count in request order, so the report of a
/// request written before the bytes never does.
#[test]
fn the_handoffs_own_request_covers_untracked_bytes_written_before_it() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_untracked_line_bytes();
    // The shell runs a typeahead line itself: the pty owns input from here.
    assert!(handoff.on_preexec(true));
    // A line typed into a command that has been running a while (not young, #933).
    handoff.age_last_activity_for_test();
    type_and_flush(&mut handoff, b"ls");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(handoff.untracked_bytes_requested());
    assert!(handoff.editor_text_precedes_report());
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert!(!handoff.editor_text_precedes_report());

    // Two requests outstanding, the bytes between them: only the second report covers them.
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.note_untracked_line_bytes();
    handoff.on_legacy_request_written();
    handoff.on_report();
    assert!(handoff.editor_text_precedes_report());
    handoff.on_report();
    assert!(!handoff.editor_text_precedes_report());
}

/// #1104: a report answers a request written before untracked bytes: those bytes stay in the
/// shell's line, so one more request is due behind them, once, and its report covers them.
#[test]
fn a_report_that_missed_untracked_bytes_is_followed_by_one_more_request() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.note_untracked_line_bytes();
    assert_eq!(handoff.on_report(), ReportDisposition::InsertLegacy);
    assert!(handoff.drain_pending());
    // Not at a prompt, or no way to ask: no request, and nothing is recorded.
    assert!(!handoff.drain_request_due(false, true));
    assert!(!handoff.drain_request_due(true, false));
    assert!(handoff.drain_request_due(true, true));
    assert!(!handoff.drain_pending());
    assert!(
        !handoff.drain_request_due(true, true),
        "one request per uncovered stretch"
    );
    assert!(handoff.editor_text_precedes_report());
    assert_eq!(handoff.on_report(), ReportDisposition::InsertLegacy);
    assert!(!handoff.editor_text_precedes_report());

    // Nothing untracked, nothing to drain.
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.on_report();
    assert!(!handoff.drain_pending());
    assert!(!handoff.drain_request_due(true, true));
}

/// #1104: queued commands (an agent's) are held while untracked bytes in the shell's line have
/// no request behind them, and released once one is written.
#[test]
fn queued_commands_are_held_behind_uncovered_untracked_bytes() {
    let mut handoff = TypeaheadHandoff::default();
    assert!(!handoff.holds_queued_writes());
    handoff.note_untracked_line_bytes();
    assert!(handoff.holds_queued_writes());
    handoff.on_legacy_request_written();
    assert!(!handoff.holds_queued_writes());

    // A request that was already outstanding does not cover bytes typed behind it.
    let mut handoff = awaiting_report();
    handoff.note_untracked_line_bytes();
    assert!(handoff.holds_queued_writes());
}

/// #1104: an Enter in the editor while a report is outstanding that does not cover the untracked
/// bytes in the shell's line is deferred, kept deferred past that report, and runs on the report
/// of a request written behind the bytes. Submitting at once wrote the kill prefix and the
/// command behind the request, so the line ran without its head.
#[test]
fn enter_waits_for_a_report_that_covers_untracked_bytes() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.note_untracked_line_bytes();
    assert!(handoff.defer_enter());
    assert!(handoff.holds_keys_behind_deferred_enter());
    let id = handoff
        .take_uncovered_deferral_started()
        .expect("a patience timer is armed");
    assert_eq!(handoff.take_uncovered_deferral_started(), None);

    // The first report missed the bytes: insert it, keep the Enter deferred, ask again.
    assert_eq!(handoff.on_report(), ReportDisposition::InsertKeepDeferred);
    assert!(handoff.holds_keys_behind_deferred_enter());
    assert!(handoff.take_uncovered_deferral_started().is_some());
    assert_eq!(
        handoff.on_line_editor_active(false, true),
        ActiveDecision::Drain,
        "not at a prompt: no request yet"
    );
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert!(handoff.deferred_submit_pending());
    // The timer armed for the first deferral is void now.
    assert!(!handoff.give_up_uncovered_deferral(id));
}

/// #1104: when the request behind the bytes is already outstanding, its report ends the
/// deferral; the one before it only inserts.
#[test]
fn enter_deferred_behind_a_covering_request_runs_on_its_report() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.note_untracked_line_bytes();
    handoff.on_legacy_request_written();
    assert!(handoff.defer_enter());
    assert_eq!(handoff.on_report(), ReportDisposition::InsertKeepDeferred);
    // Already covered: no further request.
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Drain
    );
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
}

/// #1104: no request outstanding, or no untracked bytes: the Enter is not deferred, as before.
#[test]
fn enter_is_not_deferred_without_an_uncovered_report() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_untracked_line_bytes();
    assert!(!handoff.defer_enter(), "no request is outstanding");
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    assert!(!handoff.defer_enter(), "nothing untracked is in the line");
}

/// #1104: a request a program ate never reports; the deferred Enter gives up when its timer
/// fires, once, and only for the deferral that armed it.
#[test]
fn a_deferral_whose_report_never_comes_is_given_up() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.note_untracked_line_bytes();
    assert!(handoff.defer_enter());
    let id = handoff.take_uncovered_deferral_started().unwrap();
    assert!(!handoff.give_up_uncovered_deferral(id + 1));
    assert!(handoff.give_up_uncovered_deferral(id));
    assert!(handoff.deferred_submit_pending());
    // While the released Enter is pending the hand-off is still in `HoldAfterEnter`, so the
    // editor's text is (by definition) not yet the whole line; that is not what gives up.
    // Giving up forgets the untracked bytes: once the submit ran and the hold is released,
    // nothing pins the editor literal any more.
    assert!(handoff.editor_text_precedes_report());
    assert!(handoff.release_after_unsubmitted_enter().is_some());
    assert!(!handoff.editor_text_precedes_report());
    assert!(!handoff.give_up_uncovered_deferral(id));
}

#[test]
fn an_enter_into_a_plain_foreground_program_is_not_held() {
    // The command was written just now, but `cat` owns the foreground: the shell is not at a
    // prompt, so the Enter is for `cat` and must not wait for one (#1131).
    let mut handoff = young_after_submit();
    handoff.set_foreground_probe(ForegroundProbe::new(Arc::new(|| true)));
    type_and_flush(&mut handoff, b"hello");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.take_young_hold_started(), None);
    assert!(!handoff.has_young_hold());
}

#[test]
fn a_password_enter_in_a_subshell_is_released_at_once_and_needs_no_timer() {
    // `sudo -i` / `ksu` inside a Phosphorized shell: the foreground is a login hop, but the tty
    // is reading a password (canonical, echo off), which the probe reports as "not at a prompt".
    let mut handoff = young_after_submit();
    handoff.set_foreground_probe(ForegroundProbe::new(Arc::new(|| true)));
    type_and_flush(&mut handoff, b"hunter2");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    // No hold was started, so the view arms no timer for it.
    assert_eq!(handoff.take_young_hold_started(), None);
}

#[test]
fn a_prompt_arriving_first_resolves_a_held_enter_without_its_timer() {
    // #933: the precmd is the event that settles the race; the timer only backs it up.
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"frob");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().unwrap();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    // The request went out: the report will run the Enter, so the give-up keeps waiting for it.
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Keep);
}

#[test]
fn the_hold_scales_with_the_measured_round_trip() {
    let now = Instant::now();
    let mut handoff = young_after_submit();
    // No samples yet: the unmeasured behaviour.
    handoff.last_activity_at = Some(now - Duration::from_millis(100));
    assert_eq!(handoff.young_hold_wait(now), Duration::from_millis(500));
    // A local-ish 5 ms trip: 3 x 5 ms = 15 ms, raised to the floor.
    for _ in 0..8 {
        handoff.record_rtt_for_test(Duration::from_millis(5));
    }
    assert_eq!(handoff.young_hold_wait(now), YOUNG_ENTER_FLOOR);
    // A remote 80 ms trip scales it: 240 ms.
    let mut remote = young_after_submit();
    remote.last_activity_at = Some(now - Duration::from_millis(100));
    for _ in 0..8 {
        remote.record_rtt_for_test(Duration::from_millis(80));
    }
    assert_eq!(remote.young_hold_wait(now), Duration::from_millis(240));
    // Never past what is left of the window.
    remote.last_activity_at = Some(now - Duration::from_millis(500));
    assert_eq!(remote.young_hold_wait(now), Duration::from_millis(100));
}

#[test]
fn one_stalled_sample_does_not_inflate_the_hold_and_ages_out() {
    let now = Instant::now();
    let mut handoff = young_after_submit();
    handoff.last_activity_at = Some(now);
    for _ in 0..9 {
        handoff.record_rtt_for_test(Duration::from_millis(5));
    }
    handoff.record_rtt_for_test(Duration::from_secs(30));
    assert_eq!(handoff.young_hold_wait(now), YOUNG_ENTER_FLOOR);
    for _ in 0..16 {
        handoff.record_rtt_for_test(Duration::from_millis(5));
    }
    assert_eq!(handoff.rtt_samples.len(), 16);
    assert_eq!(handoff.young_hold_wait(now), YOUNG_ENTER_FLOOR);
}

#[test]
fn a_request_to_report_round_trip_is_sampled() {
    let mut handoff = awaiting_report();
    assert_eq!(handoff.rtt_samples.len(), 0);
    handoff.on_report();
    assert_eq!(handoff.rtt_samples.len(), 1);
    assert!(handoff.rtt_pending.is_empty());
}

/// #1131: the editor's command write to its preexec is a round trip through the same shell and
/// pty pipeline, so it seeds the hold from the session's first command on; before any sample the
/// first young Enter waited at least `YOUNG_ENTER_MIN_HOLD`. A line the shell started itself is
/// not the command's preexec, and the command's own sample is dropped with it.
#[test]
fn a_command_write_to_preexec_round_trip_seeds_the_hold_before_any_report() {
    let now = Instant::now();
    let mut handoff = young_after_submit();
    assert_eq!(
        handoff.young_hold_wait(now),
        YOUNG_ENTER_WINDOW,
        "no sample yet"
    );
    assert!(!handoff.on_preexec(false));
    assert_eq!(handoff.rtt_samples.len(), 1);
    assert!(
        handoff.young_hold_wait(Instant::now()) < YOUNG_ENTER_MIN_HOLD,
        "a local loop of microseconds sizes the hold at its floor"
    );

    let mut queued = young_after_submit();
    assert!(!queued.on_preexec(true));
    assert_eq!(queued.rtt_samples.len(), 0, "a shell-started line is not the command");
    assert!(!queued.on_preexec(false));
    assert_eq!(
        queued.rtt_samples.len(),
        0,
        "the command's own preexec after a queued line would include that line's run"
    );
}

#[test]
fn an_enter_is_still_held_while_the_shell_may_own_the_foreground() {
    // #933: the probe says "not provably a plain program" (the shell's own group, an ssh hop, or
    // unknown), so the young Enter keeps waiting for the prompt.
    let mut handoff = young_after_submit();
    handoff.set_foreground_probe(ForegroundProbe::new(Arc::new(|| false)));
    type_and_flush(&mut handoff, b"frob");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert!(handoff.take_young_hold_started().is_some());
}

#[test]
fn a_held_enter_waits_only_for_what_is_left_of_the_window() {
    let mut handoff = young_after_submit();
    let now = Instant::now();
    handoff.last_activity_at = Some(now - Duration::from_millis(500));
    assert_eq!(handoff.young_hold_wait(now), YOUNG_ENTER_MIN_HOLD);
    handoff.last_activity_at = Some(now - Duration::from_millis(100));
    assert_eq!(handoff.young_hold_wait(now), Duration::from_millis(500));
    handoff.last_activity_at = Some(now);
    assert_eq!(handoff.young_hold_wait(now), YOUNG_ENTER_WINDOW);
    handoff.last_activity_at = None;
    assert_eq!(handoff.young_hold_wait(now), YOUNG_ENTER_MIN_HOLD);
}

/// A hand-off on a ConPTY session whose command was written just now (#1162).
fn conpty_young_after_submit() -> TypeaheadHandoff {
    let mut handoff = young_after_submit();
    handoff.set_conpty_input(true);
    handoff
}

#[test]
fn conpty_writes_a_held_run_back_one_chunk_per_key_event() {
    let mut handoff = conpty_young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, mut queued) = hold_young_enter(&mut handoff, b"\r");
    for key in [&b"e"[..], b"c", b"h", b"\x1b[A", b"o", b"\r", b"x"] {
        queued.push(key.to_vec());
    }
    assert_eq!(
        handoff.expire_queued_young_hold(id),
        YoungHoldExpiry::Release
    );
    let chunks = handoff.route_expired_young_chunks(queued);
    // Old behaviour (one write, `\recho\x1b[Ao\rx`, one in-flight count) coalesced the Enters.
    assert_eq!(
        chunks,
        vec![
            b"\r".to_vec(),
            b"e".to_vec(),
            b"c".to_vec(),
            b"h".to_vec(),
            b"\x1b[A".to_vec(),
            b"o".to_vec(),
            b"\r".to_vec(),
            b"x".to_vec(),
        ]
    );
    // Each chunk is its own routed write, and both Enters are queued for the shell.
    assert_eq!(handoff.writes_in_flight(), 8);
    assert!(handoff.has_unanswered_lines());
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);
}

#[test]
fn other_platforms_write_a_held_run_as_one_chunk() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, mut queued) = hold_young_enter(&mut handoff, b"\r");
    queued.push(b"d".to_vec());
    assert_eq!(
        handoff.expire_queued_young_hold(id),
        YoungHoldExpiry::Release
    );
    assert_eq!(
        handoff.route_expired_young_chunks(queued),
        vec![b"\rd".to_vec()]
    );
    assert_eq!(handoff.writes_in_flight(), 1);
}

#[test]
fn conpty_writes_no_request_while_a_line_is_still_being_typed() {
    let mut handoff = conpty_young_after_submit();
    handoff.age_last_activity_for_test();
    type_and_flush(&mut handoff, b"ec");
    handoff.on_precmd();
    // Old behaviour: `SendRequest` here, between two keys of the line being typed.
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    assert_eq!(
        handoff.state().clone(),
        HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 1,
            precmds_seen: 1,
        }
    );
    // Once the user paused, the request goes out.
    handoff.last_line_bytes_at = Some(Instant::now() - CONPTY_MIDLINE_QUIET * 2);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

#[test]
fn conpty_still_commits_without_a_request_at_a_line_boundary() {
    let mut handoff = conpty_young_after_submit();
    handoff.age_last_activity_for_test();
    type_and_flush(&mut handoff, b"ls");
    type_and_flush(&mut handoff, b"\r");
    handoff.on_precmd();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

/// #1189: keys that are Held behind a young Enter are still typing; the ConPTY mid-line guard
/// counted only keys written to the pty, so requests landed between held keys.
/// Old code: `typing_a_line_on_conpty` stayed false after the held key.
/// Not compiled/run when written.
#[test]
fn conpty_midline_guard_counts_held_keys() {
    let mut handoff = conpty_young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    handoff.last_line_bytes_at = Some(Instant::now() - CONPTY_MIDLINE_QUIET * 2);
    assert!(!handoff.typing_a_line_on_conpty(Instant::now()));
    // A key typed behind the hold goes to the view's queue, which stamps it for the guard.
    handoff.queued_line_bytes_stamp().note_now();
    assert!(handoff.typing_a_line_on_conpty(Instant::now()));
}

/// #1131: a new session's shell (the remote shell of an `ssh`) has its own loop; the previous
/// session's round trips must not size its hold. A subshell keeps them (same machine), which the
/// caller decides; this only checks the reset itself.
#[test]
fn a_new_session_drops_the_previous_sessions_round_trips() {
    let now = Instant::now();
    let mut handoff = young_after_submit();
    for _ in 0..8 {
        handoff.record_rtt_for_test(Duration::from_millis(5));
    }
    assert_eq!(handoff.young_hold_wait(now), YOUNG_ENTER_FLOOR);
    handoff.on_new_session();
    assert_eq!(handoff.rtt_samples.len(), 0);
    assert_eq!(
        handoff.young_hold_wait(now),
        YOUNG_ENTER_WINDOW,
        "unmeasured again until the new shell's first command"
    );
    // The new session's first command measures afresh.
    assert!(handoff.on_command_written().is_empty());
    assert!(!handoff.on_preexec(false));
    assert_eq!(handoff.rtt_samples.len(), 1);
}

/// A ConPTY hand-off, pty-owned, whose prompt's precmd arrived just now and whose line editor is
/// not active yet (#1162).
fn conpty_awaiting_line_editor() -> TypeaheadHandoff {
    let mut handoff = young_after_submit();
    handoff.set_conpty_input(true);
    handoff.set_conpty_prompt_hold(true);
    handoff.set_conpty_prompt_hold_shell_signals(true);
    handoff.on_precmd();
    handoff
}

#[test]
fn keys_between_precmd_and_active_are_held_and_released_in_order() {
    let mut handoff = conpty_awaiting_line_editor();
    // The first key starts the hold; the view's queue takes the keys behind it.
    let keys: [&[u8]; 5] = [b"f", b"o", b"\x1b[A", b"\r", b"x"];
    assert_eq!(handoff.on_user_bytes(keys[0]), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    assert_eq!(handoff.writes_in_flight(), 0);
    // Old behaviour: every key above was WriteCounted straight to the pty, ahead of readline.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    let mut written = Vec::new();
    for key in keys {
        if handoff.route_released_chunk(key) {
            written.push(key.to_vec());
        }
    }
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(written, keys.map(|key| key.to_vec()).to_vec());
    assert_eq!(handoff.writes_in_flight(), 5);
    assert!(!handoff.has_prompt_hold());
    // Once active nothing is held.
    assert_eq!(handoff.on_user_bytes(b"y"), UserBytesRoute::WriteCounted);
}

#[test]
fn the_hold_expiry_releases_the_keys_in_order() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(!handoff.expire_queued_prompt_hold(id + 1));
    assert!(handoff.expire_queued_prompt_hold(id));
    for key in [&b"a"[..], b"b", b"\r"] {
        assert!(handoff.route_released_chunk(key));
    }
    handoff.finish_queued_prompt_hold(id);
    assert!(!handoff.expire_queued_prompt_hold(id));
    assert_eq!(handoff.on_user_bytes(b"c"), UserBytesRoute::WriteCounted);
}

#[test]
fn a_precmd_older_than_the_give_up_cap_does_not_hold_new_keys() {
    let mut handoff = conpty_awaiting_line_editor();
    handoff.awaiting_line_editor_since =
        Some(Instant::now() - CONPTY_PROMPT_HOLD_GIVE_UP - Duration::from_millis(1));
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::WriteCounted);
}

#[test]
fn the_hold_releases_on_the_signal_and_not_before_however_long_it_takes() {
    // #1349: old behaviour released the keys 150 ms (70 ms) after the precmd whether or not the
    // line editor had signalled.
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    // A late prompt: the precmd is 1.4 s old (inside the give-up cap), still no signal.
    handoff.awaiting_line_editor_since = Some(Instant::now() - Duration::from_millis(1400));
    assert!(handoff.has_prompt_hold());
    assert!(handoff.awaiting_line_editor_signal());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    assert!(!handoff.awaiting_line_editor_signal());
}

#[test]
fn a_signal_with_no_wait_pending_releases_nothing() {
    // A program enabling bracketed paste itself, or a second signal for the same prompt.
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    // The wait is still pending for the held key: the first signal names the hold, the second
    // finds no wait.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
}

#[test]
fn a_hold_that_gave_up_is_off_for_the_session_until_a_new_shell() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(handoff.expire_queued_prompt_hold(id));
    assert!(handoff.route_released_chunk(b"a"));
    handoff.finish_queued_prompt_hold(id);
    // The next prompt is not delayed again.
    handoff.on_precmd();
    assert!(!handoff.awaiting_line_editor_signal());
    assert_eq!(handoff.on_user_bytes(b"b"), UserBytesRoute::WriteCounted);
    // A new shell (ssh) may signal.
    handoff.on_new_session();
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"c"), UserBytesRoute::Held);
}

#[test]
fn a_shell_not_known_to_signal_is_never_held_for() {
    // PowerShell 5.1, cmd, an unknown WSL shell: no signal could release the hold, so it would
    // only be a timer.
    let mut handoff = young_after_submit();
    handoff.set_conpty_input(true);
    handoff.set_conpty_prompt_hold(true);
    handoff.on_precmd();
    assert!(!handoff.awaiting_line_editor_signal());
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::WriteCounted);
    assert!(!handoff.has_prompt_hold());
}

#[test]
fn a_signal_goes_out_behind_the_held_keys() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert_eq!(
        handoff.take_queued_prompt_hold_for_signal(b"\x03"),
        Some(id)
    );
    assert!(!handoff.has_prompt_hold());
    assert_eq!(handoff.take_queued_prompt_hold_for_signal(b"a"), None);
}

#[test]
fn a_preexec_ends_the_wait_for_new_keys() {
    let mut handoff = conpty_awaiting_line_editor();
    handoff.on_preexec(false);
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::WriteCounted);
}

#[test]
fn the_kill_switch_and_other_platforms_never_hold() {
    // Prompt hold off (PHOSPHOR_CONPTY_PROMPT_HOLD=0).
    let mut handoff = young_after_submit();
    handoff.set_conpty_input(true);
    handoff.set_conpty_prompt_hold(false);
    handoff.set_conpty_prompt_hold_shell_signals(true);
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::WriteCounted);
    // Not a ConPTY session (Linux, macOS): the flag alone changes nothing.
    let mut handoff = young_after_submit();
    handoff.set_conpty_prompt_hold(true);
    handoff.set_conpty_prompt_hold_shell_signals(true);
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::WriteCounted);
    assert!(!handoff.has_prompt_hold());
}

#[test]
fn a_precmd_that_found_the_editor_flagged_active_still_gets_a_signal() {
    // #1343/#1349: the editor model stayed flagged active across an empty line (readline sends no
    // preexec for it), so it armed no activation for the next precmd and the hold fell back to
    // a 70 ms timer. The hold no longer depends on that flag: the bracketed-paste signal
    // releases it whatever the editor model thinks.
    let mut handoff = conpty_awaiting_line_editor();
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
}

/// #1345: the view reads how many prompts it has yet to handle, to hold the ssh hand-off until
/// the editor reinitialization of the remote prompt has run.
#[test]
fn view_precmds_pending_counts_prompts_until_the_view_handles_them() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_precmd_pending_view();
    assert_eq!(handoff.precmds_pending_view(), 0, "nothing tracks them yet");
    handoff.track_view_precmds();
    handoff.note_precmd_pending_view();
    assert_eq!(handoff.precmds_pending_view(), 1);
    handoff.on_precmd_viewed();
    assert_eq!(handoff.precmds_pending_view(), 0);
}

// ---- #1353 phase 5: the prompt hold's keys live in the view's input queue ------------------

/// The queued hold keeps no bytes: the first key is Held and counted nowhere, the view puts it
/// in the queue, and the line editor's signal names the hold for the view to release.
#[test]
fn a_queued_prompt_hold_keeps_no_bytes_and_the_signal_names_it() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"f"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    assert!(handoff.prompt_hold_queued());
    assert_eq!(handoff.writes_in_flight(), 0);
    // Nothing is counted until the view releases the keys: the signal only names the hold.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    assert!(
        handoff.prompt_hold_queued(),
        "live until the view accounted the keys"
    );
    assert_eq!(handoff.writes_in_flight(), 0);
    // A second signal for the same prompt has no wait pending.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
}

/// Releasing the queued keys counts each one as the legacy buffer did: five keys, five writes in
/// flight, the Enter among them counted as a line terminator.
#[test]
fn releasing_the_queued_keys_counts_them_one_write_per_key() {
    let mut handoff = conpty_awaiting_line_editor();
    let keys: [&[u8]; 5] = [b"f", b"o", b"\x1b[A", b"\r", b"x"];
    // The view's queue takes the keys after the first, which never reach the hand-off.
    assert_eq!(handoff.on_user_bytes(keys[0]), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    let mut written = Vec::new();
    for key in keys {
        if handoff.route_released_chunk(key) {
            written.push(key.to_vec());
        }
    }
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(written, keys.map(|key| key.to_vec()).to_vec(), "same order");
    // Five keys, five writes in flight, the Enter counted as a line terminator.
    assert_eq!(handoff.writes_in_flight(), 5);
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 2,
            precmds_seen: 1,
        }
    );
    assert!(!handoff.prompt_hold_queued());
    assert_eq!(handoff.on_user_bytes(b"y"), UserBytesRoute::WriteCounted);
}

/// No request is decided while the keys wait in the queue: their Enter is not counted yet, so
/// the shell's line looks empty and the editor would take over ahead of them.
#[test]
fn no_request_is_decided_while_queued_keys_are_uncounted() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    handoff.release_queued_prompt_hold_on_signal();
    assert!(handoff.route_released_chunk(b"\r"));
    handoff.finish_queued_prompt_hold(id);
    // Decided again with the Enter counted: a line is queued ahead of the prompt, so no request.
    assert!(!matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest | ActiveDecision::Committed
    ));
}

/// The give-up cap: the hold's id guards it, it fires once, and the hold is off for the session.
#[test]
fn the_queued_hold_give_up_is_id_guarded_and_switches_the_hold_off() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(!handoff.expire_queued_prompt_hold(id + 1));
    assert!(handoff.expire_queued_prompt_hold(id));
    assert!(handoff.route_released_chunk(b"a"));
    handoff.finish_queued_prompt_hold(id);
    assert!(!handoff.expire_queued_prompt_hold(id), "one-shot");
    // The next prompt is not held for any more.
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"b"), UserBytesRoute::WriteCounted);
}

/// A signal takes the hold over: the view puts the held keys in front of it as one write.
#[test]
fn a_signal_takes_the_queued_hold_and_ends_the_wait() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"l"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert_eq!(handoff.take_queued_prompt_hold_for_signal(b"a"), None);
    assert!(handoff.prompt_hold_queued());
    assert_eq!(
        handoff.take_queued_prompt_hold_for_signal(b"\x03"),
        Some(id)
    );
    assert!(!handoff.prompt_hold_queued());
    assert!(!handoff.awaiting_line_editor_signal());
    // The combined write reaches the hand-off as one signal-carrying write: never held.
    assert_eq!(
        handoff.on_user_bytes(b"l\x03"),
        UserBytesRoute::WriteCounted
    );
}

/// Keys queued behind a prompt hold are still typing: the view stamps them lock-free and the
/// ConPTY mid-line guard sees the stamp (#1189 for the queue).
#[test]
fn the_midline_guard_sees_keys_the_view_queued() {
    let mut handoff = conpty_young_after_submit();
    handoff.age_last_activity_for_test();
    handoff.last_line_bytes_at = Some(Instant::now() - CONPTY_MIDLINE_QUIET * 2);
    assert!(!handoff.typing_a_line_on_conpty(Instant::now()));
    handoff.queued_line_bytes_stamp().note_now();
    assert!(handoff.typing_a_line_on_conpty(Instant::now()));
}

// ---- #1353 phase 6: prompt sequence numbers replace the timing guesses -------------------------

/// A hand-off whose shell sends prompt sequence numbers (the prompt-sequence path is on), with the
/// platform answering `fact` about the shell, after the editor's command was written.
fn seq_after_submit(fact: ShellPromptFact) -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    // The shell's first prompt, before any command.
    handoff.on_precmd_seq(Some(7), Some(1));
    handoff
        .set_foreground_probe(ForegroundProbe::default().with_prompt_fact(Arc::new(move || fact)));
    assert!(handoff.on_command_written().is_empty());
    handoff
}

#[test]
fn prompt_sequence_numbers_are_tracked_per_shell_and_older_bootstraps_change_nothing() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    assert!(!handoff.prompt_seq_mode(), "no number yet: the timing path");
    handoff.on_precmd_seq(Some(7), None);
    assert!(
        !handoff.prompt_seq_mode(),
        "a Precmd without a number changes nothing"
    );
    handoff.on_precmd_seq(Some(7), Some(1));
    assert!(handoff.prompt_seq_mode());
    // Preexec hooks count too: a gap of 2 between Precmds is two Preexecs.
    handoff.on_precmd_seq(Some(7), Some(4));
    assert_eq!(
        handoff.last_prompt_seq(),
        Some(PromptSeq {
            session: Some(7),
            seq: 4,
            preexecs_before: Some(2)
        })
    );
    // The next prompt followed no Preexec: an empty line was consumed.
    handoff.on_precmd_seq(Some(7), Some(5));
    assert_eq!(handoff.last_prompt_seq().unwrap().preexecs_before, Some(0));
    // Another shell instance has its own counter, a regression is not a gap.
    handoff.on_precmd_seq(Some(9), Some(2));
    assert_eq!(handoff.last_prompt_seq().unwrap().preexecs_before, None);
    // The kill switch keeps the old path whatever the shell sends.
    handoff.set_prompt_seq_enabled(false);
    assert!(!handoff.prompt_seq_mode());
    // A new session (an ssh) may predate the numbers.
    handoff.set_prompt_seq_enabled(true);
    handoff.on_new_session();
    assert!(!handoff.prompt_seq_mode());
}

/// The #933 race with a stalled main thread: the shell finished the command and its Precmd was
/// parsed (the model thread counts it), but the Enter reaches the hand-off long after, outside any
/// window. The line counts say the prompt is back, so the Enter is matched to that prompt and held
/// for the request, however late it is typed.
#[test]
fn a_young_enter_is_matched_to_its_prompt_by_sequence_not_by_the_window() {
    let mut handoff = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut handoff, b"explain");
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    handoff.age_last_activity_for_test();
    handoff.young_line = false;
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert!(handoff.has_young_hold());
    let id = handoff.take_young_hold_started().expect("a hold started");
    assert_eq!(
        handoff.young_hold_wait(Instant::now()),
        EVENT_HOLD_GIVE_UP,
        "released by the prompt's request and report, the wait is only a give-up"
    );
    // The request goes out and the report runs the Enter in the editor.
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);

    // The old path (kill switch) decides by the window: outside it, the Enter is written.
    let mut legacy = young_after_submit();
    type_and_flush(&mut legacy, b"explain");
    legacy.on_precmd();
    legacy.age_last_activity_for_test();
    legacy.young_line = false;
    assert_eq!(legacy.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
}

#[test]
fn a_young_enter_is_held_or_written_by_what_the_shell_is_doing_not_by_elapsed_time() {
    // The command is still counted as running (no Precmd parsed). The line editor reading means
    // the prompt's Precmd is already in the stream: held for it, outside any window.
    let mut reading = seq_after_submit(ShellPromptFact::LineEditorReading);
    type_and_flush(&mut reading, b"explain");
    reading.age_last_activity_for_test();
    reading.young_line = false;
    assert_eq!(reading.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(reading.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);

    // A program owns the foreground: the Enter is for it, even inside the window.
    let mut program = seq_after_submit(ShellPromptFact::NotAtPrompt);
    type_and_flush(&mut program, b"explain");
    assert_eq!(program.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!program.has_young_hold());

    // Nothing is known (ssh, ConPTY): the window still decides, as before.
    let mut unknown = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut unknown, b"explain");
    assert_eq!(unknown.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_ne!(unknown.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);
    let mut unknown_late = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut unknown_late, b"explain");
    unknown_late.age_last_activity_for_test();
    unknown_late.young_line = false;
    assert_eq!(
        unknown_late.on_user_bytes(b"\r"),
        UserBytesRoute::WriteCounted
    );
}

#[test]
fn a_line_begun_while_the_line_editor_reads_is_young_whatever_the_time() {
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    handoff.age_last_activity_for_test();
    type_and_flush(&mut handoff, b"frob");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertYoung,
        "typed at a prompt the app had not noticed: its Enter must wait for a classification"
    );

    // The same line typed on the old path, outside the window, is plain typeahead.
    let mut legacy = after_submit();
    type_and_flush(&mut legacy, b"frob");
    legacy.on_precmd();
    assert_eq!(
        legacy.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(legacy.on_report(), ReportDisposition::Insert);
}

/// ^C at a zsh prompt, where no prompt cycle follows: the hand-back is the report of a request
/// written behind the interrupt, not a 250 ms guess.
#[test]
fn ctrl_c_at_a_zsh_prompt_with_no_cycle_hands_back_on_the_report() {
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert_eq!(
        handoff.on_interrupt_for_shell(false),
        InterruptOutcome::AtPrompt
    );
    assert!(handoff.interrupt_barrier_pending());
    // The line editor is reading: the request goes out behind the interrupt.
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(!handoff.interrupt_barrier_pending());
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);

    // Where the request cannot be written, input is handed back at once.
    let mut no_report = seq_after_submit(ShellPromptFact::LineEditorReading);
    no_report.on_precmd();
    no_report.on_interrupt_for_shell(false);
    assert_eq!(
        no_report.on_line_editor_active(true, false),
        ActiveDecision::Committed
    );
    assert_eq!(no_report.state(), &HandoffState::EditorOwned);

    // The old path commits at the line editor's activation without a request.
    let mut legacy = young_after_submit();
    legacy.on_precmd();
    legacy.on_interrupt();
    assert_eq!(
        legacy.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

/// ^C where the shell runs a prompt cycle (bash, fish, PowerShell, nu): input is handed back by
/// the cycle's Precmd. A line-editor activation seen before it (the status is stale) must not.
#[test]
fn ctrl_c_where_a_prompt_cycle_follows_hands_back_on_its_precmd() {
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    handoff.on_precmd();
    assert_eq!(
        handoff.on_interrupt_for_shell(true),
        InterruptOutcome::AtPrompt
    );
    assert!(!handoff.interrupt_barrier_pending());
    assert!(handoff.interrupt_awaiting_cycle());
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));

    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    handoff.on_precmd();
    handoff.on_interrupt_for_shell(true);
    // The cycle's Precmd arrives: it is the event.
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert!(!handoff.interrupt_awaiting_cycle());
    assert!(handoff.try_commit_at_prompt());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);

    // The give-up of the wait finds the flag still set only when no cycle came.
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    handoff.on_precmd();
    handoff.on_interrupt_for_shell(true);
    assert!(handoff.take_interrupt_awaiting_cycle());
    assert!(!handoff.take_interrupt_awaiting_cycle());
}

#[test]
fn enters_consumed_by_an_interactive_command_are_answered_by_its_prompt_not_a_quiet_period() {
    let mut handoff = seq_after_submit(ShellPromptFact::Unknown);
    // `cat` runs; two lines are typed into it.
    type_and_flush(&mut handoff, b"a\rb\r");
    assert!(handoff.counts_diverged());
    handoff.note_interactive_command_finished();
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert!(!handoff.counts_diverged(), "the program read both lines");
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );

    // A command that was not interactive leaves the lines queued for the shell.
    let mut queued = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut queued, b"a\rb\r");
    queued.on_precmd();
    queued.on_precmd_seq(Some(7), Some(3));
    assert!(queued.counts_diverged());
    assert!(matches!(
        queued.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));

    // Without the prompt-sequence path the flag changes nothing.
    let mut legacy = young_after_submit();
    type_and_flush(&mut legacy, b"a\rb\r");
    legacy.note_interactive_command_finished();
    legacy.on_precmd();
    assert!(legacy.counts_diverged());
}

#[test]
fn the_sampled_shell_state_resets_counts_without_a_quiet_period() {
    let mut handoff = seq_after_submit(ShellPromptFact::NotAtPrompt);
    type_and_flush(&mut handoff, b"a\r");
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    let ActiveDecision::DrainAndWait {
        activity_generation,
    } = handoff.on_line_editor_active(true, true)
    else {
        panic!("a queued line must wait");
    };
    // A program owns the terminal: the lines the counts consider queued are its.
    assert!(handoff.counts_diverged());
    assert_eq!(handoff.sample_prompt_fact(), ShellPromptFact::NotAtPrompt);
    assert!(handoff.reset_stale_counts(activity_generation));
    assert!(!handoff.counts_diverged());
}

/// #1387: the request is not delayed by a guess about the user having stopped typing. It waits for
/// what the hand-off can see: a write still in flight, a key held in the view's queue.
#[test]
fn the_conpty_request_waits_for_the_queue_boundary_not_for_a_quiet_period() {
    let mut handoff = seq_after_submit(ShellPromptFact::Unknown);
    handoff.set_conpty_input(true);
    // A key of the line is routed but not written yet: not a boundary.
    assert_eq!(handoff.on_user_bytes(b"e"), UserBytesRoute::WriteCounted);
    handoff.on_precmd();
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    // Written: the boundary. The key was typed just now, and a slow typist's next key is 200 ms
    // away, so there is nothing to wait for.
    handoff.on_user_write_flushed();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );

    // The old path still holds the request back for 120 ms after a key.
    let mut legacy = young_after_submit();
    legacy.set_conpty_input(true);
    type_and_flush(&mut legacy, b"e");
    legacy.on_precmd();
    assert!(matches!(
        legacy.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
}

// ---- #1353 phase 5b: a young Enter's hold in the view's input queue ----------------------------
//
// The same scenarios as the buffered hold above, with the keys in the view's queue: the hand-off
// keeps no bytes, and every way the hold can end is recorded for the view to apply to the gate.

/// Types `frob`, then an Enter that is held, and returns the hold's id.
fn queued_young_hold() -> (TypeaheadHandoff, u64) {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"frob");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    (handoff, id)
}

#[test]
fn a_queued_young_hold_keeps_no_bytes_and_says_so() {
    let (handoff, _) = queued_young_hold();
    assert!(handoff.young_hold_queued());
    assert!(handoff.has_young_hold());
    assert!(!handoff.young_hold_end_pending());
    // The Enter is held behind nothing in the tty: nothing was counted or written.
    assert_eq!(handoff.writes_in_flight(), 0);
}

#[test]
fn the_report_of_a_queued_young_hold_runs_the_enter_and_asks_the_view_to_split_the_keys() {
    let (mut handoff, id) = queued_young_hold();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    // The give-up of a hold the request already went out for keeps waiting for the report.
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Keep);
    assert!(handoff.young_hold_queued());
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::SubmitOnReport)]
    );
    assert!(!handoff.young_hold_queued());
    // The view puts the keys after the Enter behind it, as the buffered hold's split did.
    assert!(handoff.prepend_held_keys(b"x"));
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter {
            held: b"x".to_vec(),
            released: true,
        }
    );
}

#[test]
fn a_signal_ends_a_queued_young_hold_as_it_ends_the_buffered_one() {
    for signal in [0x03u8, 0x04, 0x1a, 0x1c] {
        let (mut handoff, id) = queued_young_hold();
        assert_eq!(
            handoff.on_user_bytes(&[signal]),
            UserBytesRoute::WriteCounted
        );
        assert!(!handoff.has_young_hold());
        assert_eq!(
            handoff.take_young_hold_ends(),
            vec![(id, YoungHoldEnd::Dropped)]
        );
        // Nothing is left to expire.
        assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);
    }
    // The view ends the hold itself before the signal is routed: a lone EOF takes the keys with
    // it, the others void them, and no end is left behind.
    let (mut handoff, id) = queued_young_hold();
    assert_eq!(
        handoff.take_queued_young_hold_for_signal(&[0x04]),
        Some((id, true))
    );
    assert!(!handoff.has_young_hold());
    assert!(!handoff.young_hold_end_pending());
    for bytes in [&[0x03][..], &[0x04, 0x03], &[0x1a], &[0x1c]] {
        let (mut handoff, id) = queued_young_hold();
        assert_eq!(
            handoff.take_queued_young_hold_for_signal(bytes),
            Some((id, false))
        );
        assert!(!handoff.young_hold_end_pending());
    }
    // Not a signal, or nothing held: nothing happens.
    let (mut handoff, _) = queued_young_hold();
    assert_eq!(handoff.take_queued_young_hold_for_signal(b"x"), None);
    assert!(handoff.young_hold_queued());
    assert_eq!(
        young_after_submit().take_queued_young_hold_for_signal(&[0x04]),
        None
    );
}

#[test]
fn a_queued_young_hold_is_released_as_typeahead_when_no_prompt_comes() {
    let (mut handoff, id) = queued_young_hold();
    assert_eq!(
        handoff.expire_queued_young_hold(id),
        YoungHoldExpiry::Release
    );
    assert!(!handoff.has_young_hold());
    // One write elsewhere (the keys the view took from the gate, in order), counted as the routed
    // write it now is, and its Enter is queued for the shell.
    let writes = handoff.route_expired_young_chunks(vec![b"\r".to_vec(), b"d".to_vec()]);
    assert_eq!(writes, vec![b"\rd".to_vec()]);
    assert_eq!(handoff.writes_in_flight(), 1);
    assert!(handoff.has_unanswered_lines());
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);

    // On ConPTY the keys stay the separate writes they were typed in (#1162).
    let (mut conpty, id) = queued_young_hold();
    conpty.set_conpty_input(true);
    assert_eq!(
        conpty.expire_queued_young_hold(id),
        YoungHoldExpiry::Release
    );
    let writes = conpty.route_expired_young_chunks(vec![b"\r".to_vec(), b"d".to_vec()]);
    assert_eq!(writes, vec![b"\r".to_vec(), b"d".to_vec()]);
    assert_eq!(conpty.writes_in_flight(), 2);
}

#[test]
fn an_expired_timer_cannot_release_a_later_queued_hold() {
    let (mut handoff, first) = queued_young_hold();
    handoff.on_user_bytes(&[0x03]);
    handoff.on_user_write_flushed();
    assert_eq!(handoff.on_interrupt(), InterruptOutcome::CommandRunning);
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(first, YoungHoldEnd::Dropped)]
    );
    type_and_flush(&mut handoff, b"def");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let second = handoff.take_young_hold_started().unwrap();
    assert_ne!(first, second);
    assert_eq!(
        handoff.expire_queued_young_hold(first),
        YoungHoldExpiry::Gone
    );
    assert!(handoff.has_young_hold());
    assert_eq!(
        handoff.expire_queued_young_hold(second),
        YoungHoldExpiry::Release
    );
}

#[test]
fn a_queued_young_hold_does_not_outlive_the_state_it_was_made_in() {
    // An uncounted command's kill prefix ends the line the Enter belongs to.
    let (mut handoff, id) = queued_young_hold();
    handoff.on_uncounted_command_written();
    assert!(!handoff.has_young_hold());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Dropped)]
    );

    // A state that cannot own a hold ends it before the next key is queued behind it.
    let (mut handoff, id) = queued_young_hold();
    handoff.state = HandoffState::EditorOwned;
    assert!(!handoff.young_hold_end_pending());
    assert!(handoff.check_queued_young_hold());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Dropped)]
    );
    // ... and while the hold is fine, the check leaves it alone.
    let (mut handoff, _) = queued_young_hold();
    assert!(!handoff.check_queued_young_hold());
    assert!(handoff.young_hold_queued());

    // The session ending drops it, and the next hold never reuses its gate's key.
    let (mut handoff, id) = queued_young_hold();
    handoff.reset_on_exit();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Dropped)]
    );
    assert!(!handoff.has_young_hold());
    assert_eq!(handoff.young_hold_id, id);
}

#[test]
fn ctrl_c_cancels_a_queued_young_hold_behind_the_request() {
    let (mut handoff, id) = queued_young_hold();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(handoff.cancel_deferred_enter());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Dropped)]
    );
    assert!(!handoff.cancel_deferred_enter());
}

#[test]
fn a_command_written_behind_a_queued_young_hold_takes_its_keys_with_it() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().unwrap();
    assert_eq!(handoff.on_command_written(), Vec::<u8>::new());
    assert!(!handoff.young_hold_queued());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::FollowCommand)]
    );
    // Until the view has written them no request is decided: the line looks shorter than it is.
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    handoff.count_followed_young_hold(id, b"\rx");
    // The submitted command's Enter and the held Enter are queued for the shell, the line holds
    // `x`, and one write is in flight (the writer retires it).
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 3,
            precmds_seen: 0,
        }
    );
    assert_eq!(handoff.writes_in_flight(), 1);
    handoff.on_user_write_flushed();
    assert_eq!(handoff.writes_in_flight(), 0);
}

#[test]
fn a_command_written_in_a_state_that_cannot_own_the_hold_drops_it() {
    let (mut handoff, id) = queued_young_hold();
    handoff.state = HandoffState::AwaitingReport;
    assert_eq!(handoff.on_command_written(), Vec::<u8>::new());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Dropped)]
    );
    // Nothing follows the command, so nothing waits for the view to write it.
    assert!(handoff.young_hold_follow_pending.is_none());
}
