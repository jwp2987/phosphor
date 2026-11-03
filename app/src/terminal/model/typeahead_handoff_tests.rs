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
    handoff.on_command_written();
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
    // The keys are entries behind the Enter's gate, which the view opens when it is deferred; the
    // hand-off keeps no bytes. A key that reaches it anyway is held for the view to put there.
    let gate = handoff.deferred_gate().expect("the Enter holds a gate");
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    assert_eq!(handoff.take_deferred_key_started(), Some(gate));
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter { released: false }
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
    let gate = handoff.deferred_gate().expect("the Enter holds a gate");

    // The report arrives: insert it, then run the deferred Enter. The keys typed meanwhile
    // (`pwd`, Enter, `l`, `s`) are entries behind the gate, still held behind it.
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert_eq!(
        handoff.deferred_gate(),
        Some(gate),
        "the report keeps the gate"
    );

    // The deferred command is written; the held keys follow it, counted as its typeahead.
    handoff.on_command_written();
    assert_eq!(handoff.deferred_gate(), None);
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(gate, YoungHoldEnd::FollowCommand)]
    );
    // Until the view has written them no request is decided.
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    handoff.count_followed_young_hold(gate, b"pwd\rls");
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
    handoff.on_command_written();
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
    handoff.on_command_written();
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
    handoff.on_command_written();
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
fn the_editor_takeover_window_ends_on_its_events_not_the_clock() {
    // #1439: a key 1.5 s after the hand-off, focus still not landed, is still the editor's.
    let start = Instant::now();
    let give_up = Duration::from_secs(10);
    let mut handoff = TypeaheadHandoff::default();
    assert!(!handoff.editor_takeover_open(give_up, start));
    handoff.note_editor_took_over(start);
    assert!(handoff.editor_takeover_open(give_up, start + Duration::from_millis(200)));
    assert!(handoff.editor_takeover_open(give_up, start + Duration::from_millis(1500)));
    // The focus event ends it.
    handoff.on_editor_focused();
    assert!(!handoff.editor_takeover_open(give_up, start + Duration::from_millis(1600)));
    // Only while the editor owns input.
    handoff.note_editor_took_over(start);
    handoff.on_command_written();
    assert!(!handoff.editor_takeover_open(give_up, start + Duration::from_millis(200)));
}

#[test]
fn the_editor_takeover_window_gives_up_when_focus_never_comes() {
    let start = Instant::now();
    let give_up = Duration::from_secs(10);
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_editor_took_over(start);
    assert!(handoff.editor_takeover_open(give_up, start + Duration::from_secs(9)));
    assert!(!handoff.editor_takeover_open(give_up, start + Duration::from_secs(11)));
    // Closed for good: the give-up is not re-evaluated against an earlier clock.
    assert!(!handoff.editor_takeover_open(give_up, start + Duration::from_secs(1)));
}

/// B1: the request is outstanding until the report arrives. Nothing gives up on it: the
/// editor holds only what was typed after the request, so a submit now would run a fragment.
#[test]
fn a_deferred_enter_waits_for_the_report_however_long_it_takes() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().unwrap();
    // Nothing but the report (or Ctrl-C, or the session ending) releases it.
    assert_eq!(handoff.release_after_unsubmitted_enter(), None);
    assert!(handoff.routes_keys_to_pty());
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    handoff.on_command_written();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(gate, YoungHoldEnd::FollowCommand)]
    );
}

/// S2: the report arrived and its submit wrote no command: the held keys are released at once,
/// shell lines to the pty and an unfinished line to the editor -- never shell lines to the
/// editor (#826 N1).
#[test]
fn keys_held_behind_an_unsubmitted_enter_are_released_where_they_belong() {
    // The hand-off hands the gate over; the view takes the entries out and decides by their
    // content (view tests: an Enter among them goes to the pty, an unfinished line to the editor).
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().unwrap();
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
    assert_eq!(handoff.release_after_unsubmitted_enter(), Some(gate));
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert_eq!(handoff.deferred_gate(), None);
    assert_eq!(handoff.release_after_unsubmitted_enter(), None, "once");
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
    assert!(!handoff.editor_takeover_open(Duration::from_secs(10), start));
}

#[test]
fn the_window_survives_focus_moved_by_a_key_it_applied() {
    let start = Instant::now();
    let window = Duration::from_secs(10);
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_editor_took_over(start);
    handoff.note_focus_moved_by_takeover_key();
    handoff.on_editor_focused();
    assert!(handoff.editor_takeover_open(window, start));
    // Any other focus change ends it.
    handoff.on_editor_focused();
    assert!(!handoff.editor_takeover_open(window, start));
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
    // The view turns the hold's gate into the deferred Enter's gate in place: the Enter runs in
    // the editor, the rest stays held behind it, in the queue.
    let gate = handoff
        .deferred_gate()
        .expect("the report made a deferred Enter");
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(
            id,
            YoungHoldEnd::SubmitOnReport {
                deferred_gate: gate
            }
        )]
    );
    assert_ne!(gate, id, "a new gate id");
    assert_eq!(queued.concat(), b"\rx".to_vec());
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter { released: true }
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
    handoff.on_command_written();
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
    let gate = handoff
        .deferred_gate()
        .expect("the report made a deferred Enter");
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(
            id,
            YoungHoldEnd::SubmitOnReport {
                deferred_gate: gate
            }
        )]
    );
    // The order typed is the order executed: the Enter, then d, e, f -- the Enter runs in the
    // editor, the rest stays in the queue behind the new gate.
    assert_eq!(queued.concat(), b"\rdef".to_vec());
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter { released: true }
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
    let gate = handoff.deferred_gate().unwrap();
    assert_eq!(handoff.give_up_report_deferral(id), Some(gate));
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
    // #1604: a large buffer's cap grows with its size (its linear classification runs in the
    // background), a typed line's does not.
    assert_eq!(young_detection_cap_for(40), YOUNG_DETECTION_CAP);
    let ten_mb = 10 * 1024 * 1024;
    assert!(young_detection_cap_for(ten_mb) >= YOUNG_DETECTION_CAP + Duration::from_millis(2500));
    assert_eq!(
        young_submit_decision_for(true, true, YOUNG_DETECTION_CAP, ten_mb),
        YoungSubmit::Wait
    );
    assert_eq!(
        young_submit_decision_for(true, true, young_detection_cap_for(ten_mb), ten_mb),
        YoungSubmit::SubmitWithoutVerdict
    );
    assert_eq!(
        young_submit_decision_for(true, false, Duration::ZERO, ten_mb),
        YoungSubmit::Submit
    );
    assert_eq!(
        young_submit_decision_for(true, true, YOUNG_DETECTION_CAP, 40),
        young_submit_decision(true, true, YOUNG_DETECTION_CAP)
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
    assert_eq!(
        queued.rtt_samples.len(),
        0,
        "a shell-started line is not the command"
    );
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
    handoff.on_command_written();
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
    // The signal ended the wait: the line editor is ready, so a key is written, not held.
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::WriteCounted);

    // With a key held behind the wait, the first signal names the hold, the second finds no wait.
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
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

// ---- the enter window of the ConPTY prompt hold (#1490, #1349) ---------------------------------

/// A ConPTY hand-off with the enter window on, at a shell that is ready: the precmd arrived, the
/// line editor's signal released the wait, and the shell reads a line.
fn conpty_ready_with_enter_window() -> TypeaheadHandoff {
    let mut handoff = conpty_awaiting_line_editor();
    handoff.set_conpty_enter_window_hold(true);
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    // No young Enter: the prompt is old enough that nothing is typed "before the app noticed it".
    handoff.age_last_activity_for_test();
    handoff
}

#[test]
fn keys_typed_between_the_enter_and_the_next_ready_signal_are_held_and_released_in_order() {
    let mut handoff = conpty_ready_with_enter_window();
    // Ready: a line is written as typed, and its Enter opens the window in which the shell is not
    // reading.
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    handoff.on_preexec(false);
    // Typed ahead while the command runs: held, not written raw to ConPTY (old behaviour:
    // `WriteCounted`, which lost or reordered them while the shell switched tty modes).
    let keys: [&[u8]; 3] = [b"e", b"c", b"\r"];
    assert_eq!(handoff.on_user_bytes(keys[0]), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    assert_eq!(
        handoff.writes_in_flight(),
        2,
        "only `ls` and its Enter reached the pty"
    );
    // The next prompt's precmd starts the wait for the line editor; it releases nothing itself.
    handoff.on_precmd();
    assert!(handoff.awaiting_line_editor_signal());
    assert!(handoff.has_prompt_hold());
    // The line editor's ready signal releases the keys, in typed order.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    let mut written = Vec::new();
    for key in keys {
        if handoff.route_released_chunk(key) {
            written.push(key.to_vec());
        }
    }
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(written, keys.map(|key| key.to_vec()).to_vec());
    // The released Enter submitted a line, so the window is open again for what follows it.
    assert_eq!(handoff.on_user_bytes(b"z"), UserBytesRoute::Held);
    assert!(handoff.take_prompt_hold_started().is_some());
}

#[test]
fn the_enter_window_is_off_unless_enabled() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    handoff.age_last_activity_for_test();
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::WriteCounted);
    assert!(!handoff.has_prompt_hold());
}

#[test]
fn the_enter_window_needs_conpty_and_an_armed_prompt_hold() {
    // Not ConPTY: the flag alone changes nothing.
    let mut handoff = young_after_submit();
    handoff.set_conpty_prompt_hold(true);
    handoff.set_conpty_prompt_hold_shell_signals(true);
    handoff.set_conpty_enter_window_hold(true);
    handoff.on_precmd();
    handoff.age_last_activity_for_test();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::WriteCounted);
    // A shell not known to signal is never held for.
    let mut handoff = young_after_submit();
    handoff.set_conpty_input(true);
    handoff.set_conpty_prompt_hold(true);
    handoff.set_conpty_enter_window_hold(true);
    handoff.on_precmd();
    handoff.age_last_activity_for_test();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::WriteCounted);
}

#[test]
fn a_command_written_by_the_app_opens_the_window_too() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_conpty_input(true);
    handoff.set_conpty_prompt_hold(true);
    handoff.set_conpty_prompt_hold_shell_signals(true);
    handoff.set_conpty_enter_window_hold(true);
    handoff.on_precmd();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    // The editor's Enter: the command is written to the shell that was reading its line.
    handoff.on_command_written();
    assert_eq!(
        handoff.peek_user_bytes_route(b"a", true, Instant::now()),
        UserBytesRoute::Held
    );
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    assert!(handoff.take_prompt_hold_started().is_some());
}

#[test]
fn a_command_that_becomes_interactive_gets_the_typeahead_in_order() {
    // Typeahead for a running interactive program is still delivered: the window ends, by the
    // event, and the held keys go to the program in typed order.
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"cat"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"1"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    assert_eq!(handoff.release_enter_window_for_program(), Some(id));
    assert!(handoff.route_released_chunk(b"1"));
    handoff.finish_queued_prompt_hold(id);
    // Nothing is held any more: the program reads what is typed.
    assert_eq!(handoff.on_user_bytes(b"2"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.release_enter_window_for_program(), None);
}

#[test]
fn a_running_programs_own_enter_never_opens_the_window() {
    let mut handoff = conpty_ready_with_enter_window();
    // The shell started a command: its keys belong to it.
    handoff.on_preexec(false);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::WriteCounted);
    assert!(!handoff.has_prompt_hold());
    assert_eq!(handoff.release_enter_window_for_program(), None);
}

#[test]
fn a_ready_signal_before_the_precmd_does_not_end_the_window() {
    // A program enabling bracketed paste itself (a REPL), or the shell's own signal from before the
    // Enter parsed late: neither is the line editor of the NEXT prompt.
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    // Still held, behind the same hold.
    assert_eq!(handoff.on_user_bytes(b"b"), UserBytesRoute::Held);
    assert!(handoff.has_prompt_hold());
    // The prompt's precmd and its signal release both.
    handoff.on_precmd();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
}

/// #1573: the give-up of an enter window whose Enter the shell never acted on (no preexec, no
/// precmd within the cap: a continuation prompt with no marker, a lost Enter) releases THAT
/// window's keys, in order, and nothing else. The old test here (`..._and_is_then_off`) encoded the
/// bug: the window's give-up switched the prompt hold off for the rest of the session, so every
/// later prompt wrote its keys raw to ConPTY again (#1162/#1569).
#[test]
fn the_enter_window_gives_up_for_its_own_keys_and_the_hold_stays_on() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    // No preexec, no precmd, no signal, no interactive command: a give-up for this window.
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::GiveUp
    );
    assert!(handoff.expire_queued_prompt_hold(id));
    assert!(handoff.route_released_chunk(b"a"));
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::Gone
    );
    assert_eq!(handoff.release_enter_window_for_program(), None);
    assert_eq!(handoff.on_user_bytes(b"c"), UserBytesRoute::WriteCounted);
    // The hold is NOT off: the next prompt holds its keys for the line editor again.
    assert!(!handoff.prompt_hold_off_for_session());
    handoff.on_precmd();
    assert!(handoff.awaiting_line_editor_signal());
    assert_eq!(handoff.on_user_bytes(b"d"), UserBytesRoute::Held);
    assert!(handoff.take_prompt_hold_started().is_some());
}

/// #1573, Git Bash-like ConPTY: `sleep 3` with keys typed in its first second. The cap (1.5 s,
/// armed at the first held key) fires while the command still runs: that is not a give-up, the
/// keys stay held and the hold stays on. The command's precmd starts the line editor wait with a
/// full cap of its own; the ready signal then releases every key once, in typed order; the next
/// prompt is held for again. Old behaviour: the cap wrote the keys raw mid-command and switched the
/// prompt hold off for the session.
#[test]
fn a_command_running_past_the_give_up_cap_keeps_its_keys_held_and_the_hold_on() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(
        handoff.on_user_bytes(b"sleep 3"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    handoff.on_preexec(false);
    // Typed within the first second of the command.
    let keys: [&[u8]; 7] = [b"e", b"c", b"h", b"o", b" ", b"x", b"\r"];
    assert_eq!(handoff.on_user_bytes(keys[0]), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    // The cap fires 1.5 s after the first key; `sleep 3` is still running.
    let cap_fires_at = Instant::now() + CONPTY_PROMPT_HOLD_GIVE_UP + Duration::from_millis(1);
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, cap_fires_at),
        PromptHoldCap::NotYet(CONPTY_PROMPT_HOLD_GIVE_UP),
        "a running command is not overdue"
    );
    assert!(handoff.has_prompt_hold());
    assert!(!handoff.prompt_hold_off_for_session());
    // The command's prompt: its precmd starts the wait for the line editor, with its own cap.
    handoff.on_precmd();
    assert!(handoff.awaiting_line_editor_signal());
    match handoff.prompt_hold_cap_fired(id, Instant::now()) {
        PromptHoldCap::NotYet(left) => assert!(
            left > CONPTY_PROMPT_HOLD_GIVE_UP - Duration::from_millis(500),
            "the precmd's wait gets a full cap: {left:?}"
        ),
        other => panic!("the re-armed cap right after the precmd gave {other:?}"),
    }
    // The precmd's wait still has its give-up.
    let precmd_at = handoff.awaiting_line_editor_since.expect("waiting");
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, precmd_at + CONPTY_PROMPT_HOLD_GIVE_UP),
        PromptHoldCap::GiveUp
    );
    // The line editor's signal releases every key once, in typed order.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    let mut written = Vec::new();
    for key in keys {
        if handoff.route_released_chunk(key) {
            written.push(key.to_vec());
        }
    }
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(written, keys.map(|key| key.to_vec()).to_vec());
    assert!(!handoff.prompt_hold_off_for_session());
    // The released line's prompt: a fast line typed there is held for its line editor again.
    handoff.on_precmd();
    assert!(handoff.awaiting_line_editor_signal());
    assert_eq!(handoff.on_user_bytes(b"l"), UserBytesRoute::Held);
    let next = handoff
        .take_prompt_hold_started()
        .expect("the hold is still on");
    assert_ne!(next, id);
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(next));
}

/// #1573: the enter window has no clock. A key typed into a command that has run longer than the
/// give-up cap, with nothing held yet, is still held for the shell's next line editor. Old
/// behaviour: the window "expired" after 1.5 s and the key was written raw to ConPTY mid-command
/// (#1490's lost / moved characters).
#[test]
fn a_key_typed_after_the_cap_into_a_running_command_is_still_held() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"make"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    handoff.on_preexec(false);
    // Nothing typed for longer than the give-up cap.
    handoff.enter_window_since =
        Some(Instant::now() - CONPTY_PROMPT_HOLD_GIVE_UP - Duration::from_millis(500));
    handoff.last_preexec_at = handoff.enter_window_since;
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::Held);
    assert!(handoff.take_prompt_hold_started().is_some());
}

/// #1573: the cap firing after an event already ended both waits (its release in flight, e.g.
/// deferred behind a busy queue) is not a give-up: it is checked again shortly. Bounded: a hold
/// left queued with no wait and no release is released past twice the cap.
#[test]
fn a_cap_firing_while_an_event_release_is_in_flight_is_not_a_give_up() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    // The command became interactive: the window ended, the view has not released the keys yet.
    assert_eq!(handoff.release_enter_window_for_program(), Some(id));
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::NotYet(PROMPT_HOLD_RELEASE_IN_FLIGHT_RECHECK)
    );
    let started = handoff.prompt_hold_started_at.expect("a hold started");
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, started + CONPTY_PROMPT_HOLD_GIVE_UP * 2),
        PromptHoldCap::GiveUp
    );
    assert!(handoff.route_released_chunk(b"a"));
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::Gone
    );
}

/// #1573: a give-up in the precmd's wait by a shell that HAS signalled this session is one missed
/// prompt (a loaded machine): its keys are released, and the next prompt is held for again. Only a
/// shell that never signalled turns the hold off for the session (the #1349 reason, kept:
/// `a_hold_that_gave_up_is_off_for_the_session_until_a_new_shell`).
#[test]
fn a_give_up_by_a_shell_that_signals_is_for_that_prompt_only() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    // The hold began in this wait: the cap was armed with the full give-up then.
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::GiveUp
    );
    assert!(handoff.expire_queued_prompt_hold(id));
    assert!(handoff.route_released_chunk(b"a"));
    handoff.finish_queued_prompt_hold(id);
    assert!(!handoff.prompt_hold_off_for_session());
    handoff.on_precmd();
    assert!(handoff.awaiting_line_editor_signal());
    assert_eq!(handoff.on_user_bytes(b"b"), UserBytesRoute::Held);
}

/// #1573: an Enter that ends a line the shell continues (PS2) never gets a precmd or a preexec;
/// the continuation prompt is the event that ends the window and releases its keys in order, not
/// the give-up (which used to switch the hold off for the session).
#[test]
fn a_continuation_prompt_ends_the_enter_window_and_releases_its_keys() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(
        handoff.on_user_bytes(b"for i in 1; do"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"e"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    handoff.on_continuation_prompt();
    assert_eq!(handoff.release_enter_window_for_continuation(), Some(id));
    assert!(handoff.route_released_chunk(b"e"));
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(handoff.release_enter_window_for_continuation(), None);
    assert!(!handoff.prompt_hold_off_for_session());
}

#[test]
fn a_signal_ends_the_window_and_the_held_keys_go_out_behind_it() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert_eq!(
        handoff.take_queued_prompt_hold_for_signal(b"\x03"),
        Some(id)
    );
    assert_eq!(handoff.release_enter_window_for_program(), None);
}

#[test]
fn the_enter_window_follows_the_table_mode_unless_the_environment_says_otherwise() {
    // Unset it follows the destination mode: on in `table` (the default), off in the others.
    assert!(!conpty_enter_hold_from(None, false));
    assert!(conpty_enter_hold_from(None, true));
    assert!(conpty_enter_hold_from(Some("1"), false));
    assert!(!conpty_enter_hold_from(Some("0"), true));
    assert!(
        !conpty_enter_hold_from(Some("true"), true),
        "only `1` turns it on"
    );
}

#[test]
fn a_new_session_closes_the_window() {
    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    handoff.on_new_session();
    assert_eq!(handoff.release_enter_window_for_program(), None);
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
    handoff.on_command_written();
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

/// A shell that sends sequence numbers on a platform that samples nothing (ConPTY): no probe at
/// all, after the editor's command was written.
fn seq_unsampled_after_submit() -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    handoff.on_precmd_seq(Some(7), Some(1));
    handoff.on_command_written();
    handoff
}

/// #1614 changed one expectation here, a hold no event could release (the audit of #1614 and the
/// macOS r18e4 logs): a sampled `Unknown` took the window hold although the shell's own group was
/// not in front (the inner-shell case, 28 give-ups per pass). The no-barrier case (fish) keeps its
/// 2 s event hold by owner decision 2026-11-03, named `PrecmdMaybeInFlight`. The window stays only
/// where nothing can be sampled.
#[test]
fn a_young_enter_is_held_or_written_by_what_the_shell_is_doing_not_by_elapsed_time() {
    // The command is still counted as running (no Precmd parsed) and the line editor reads. The
    // barrier says the stream is not applied up to now, so the prompt's Precmd may be in it: held
    // for it, outside any window, until the barrier's next publish decides.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut reading = seq_after_submit(ShellPromptFact::LineEditorReading);
    probe.install(&mut reading);
    type_and_flush(&mut reading, b"explain");
    reading.age_last_activity_for_test();
    reading.young_line = false;
    assert_eq!(reading.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        reading.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert_eq!(reading.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);

    // The same sample with no barrier installed (fish, which writes its Precmd after raw mode):
    // whether a Precmd is in flight cannot be told, and the hold is kept as before #1614 (owner
    // decision 2026-11-03), outside any window, released by the Precmd's request and report.
    let mut unbarriered = seq_after_submit(ShellPromptFact::LineEditorReading);
    type_and_flush(&mut unbarriered, b"explain");
    unbarriered.age_last_activity_for_test();
    unbarriered.young_line = false;
    assert_eq!(unbarriered.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        unbarriered.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdMaybeInFlight)
    );
    assert_eq!(
        unbarriered.young_hold_wait(Instant::now()),
        EVENT_HOLD_GIVE_UP
    );
    unbarriered.on_precmd();
    assert_eq!(
        unbarriered.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        unbarriered.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );

    // A program owns the foreground: the Enter is for it, even inside the window.
    let mut program = seq_after_submit(ShellPromptFact::NotAtPrompt);
    type_and_flush(&mut program, b"explain");
    assert_eq!(program.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!program.has_young_hold());

    // A sampled `Unknown` is a fact: the shell's own group is not in front with a raw tty (an inner
    // shell, an ssh, a cooked builtin), so no prompt of the hooked shell is coming for this line.
    // Delivered, inside the window too.
    let mut unknown = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut unknown, b"explain");
    assert_eq!(unknown.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!unknown.has_young_hold());

    // Nothing sampled at all (ConPTY): the window still decides, as before.
    let mut unsampled = seq_unsampled_after_submit();
    type_and_flush(&mut unsampled, b"explain");
    assert_eq!(unsampled.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        unsampled.young_hold_evidence(),
        Some(YoungHoldEvidence::TimingWindow)
    );
    assert_ne!(
        unsampled.young_hold_wait(Instant::now()),
        EVENT_HOLD_GIVE_UP
    );
    let mut unsampled_late = seq_unsampled_after_submit();
    type_and_flush(&mut unsampled_late, b"explain");
    unsampled_late.age_last_activity_for_test();
    unsampled_late.young_line = false;
    assert_eq!(
        unsampled_late.on_user_bytes(b"\r"),
        UserBytesRoute::WriteCounted
    );
}

/// A session with no platform sample whose counts say the prompt is back, then a command starts
/// (a Preexec parsed after that Precmd).
fn unsampled_command_started_after_prompt(markers: bool) -> TypeaheadHandoff {
    let mut handoff = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut handoff, b"explain");
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    if markers {
        assert!(handoff.on_line_editor_reading(Some(7), Some(3)));
    }
    handoff.on_preexec(false);
    handoff.age_last_activity_for_test();
    handoff.young_line = false;
    handoff
}

/// #1390: with live markers a Preexec parsed after the counts' Precmd means a command runs, so a
/// young Enter is for it and is written, not held until a prompt that is not coming. Without
/// markers (an older bootstrap) the counts still decide, as before.
#[test]
fn with_live_markers_a_running_command_is_not_held_for_a_prompt_on_an_unsampled_session() {
    let mut running = unsampled_command_started_after_prompt(true);
    assert_eq!(running.shell_activity(), ShellActivity::Running);
    assert_eq!(running.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!running.has_young_hold());
    // The peek (destination table) agrees with the commit.
    let peek = unsampled_command_started_after_prompt(true);
    assert_eq!(
        peek.peek_user_bytes_route(b"\r", true, Instant::now()),
        UserBytesRoute::WriteCounted
    );

    // No markers: the counts say the prompt is back, held by event as before.
    let mut no_markers = unsampled_command_started_after_prompt(false);
    assert_eq!(no_markers.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        no_markers.young_hold_wait(Instant::now()),
        EVENT_HOLD_GIVE_UP
    );
}

/// #1390: the stream lags the shell, so Running never turns a held Enter into a delivery while a
/// line is outstanding on a session that samples nothing (ConPTY): that case still takes the
/// window (a hold the timer only hands back). #1614: where the platform does sample and says the
/// shell's group is not in front (`Unknown` from an installed probe), the Enter is delivered: the
/// window hold it took before had no event that could release it (the inner-shell failure).
#[test]
fn an_outstanding_line_with_live_markers_takes_the_window_hold_only_unsampled() {
    let mut handoff = seq_unsampled_after_submit();
    handoff.on_line_editor_reading(Some(7), Some(1));
    handoff.on_preexec(false);
    assert_eq!(handoff.shell_activity(), ShellActivity::Running);
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::TimingWindow)
    );
    assert_ne!(handoff.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);

    let mut sampled = seq_after_submit(ShellPromptFact::Unknown);
    sampled.on_line_editor_reading(Some(7), Some(1));
    sampled.on_preexec(false);
    type_and_flush(&mut sampled, b"explain");
    assert_eq!(sampled.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!sampled.has_young_hold());
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
    let gate = handoff
        .deferred_gate()
        .expect("the report made a deferred Enter");
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(
            id,
            YoungHoldEnd::SubmitOnReport {
                deferred_gate: gate
            }
        )]
    );
    assert!(!handoff.young_hold_queued());
    // The view turns the hold's gate into the deferred Enter's, dropping the Enter that runs.
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter { released: true }
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
    handoff.on_command_written();
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
    handoff.on_command_written();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Dropped)]
    );
    // Nothing follows the command, so nothing waits for the view to write it.
    assert!(handoff.young_hold_follow_pending.is_none());
}

// ---- the keys behind a deferred Enter are entries behind a `HandoffReport` gate ------------------
//
// The hand-off keeps no bytes for them: it names the gate and records every way the hold can end
// for the view to apply to the queue.

#[test]
fn a_deferred_enter_names_its_gate_and_every_end_is_recorded_for_the_view() {
    // Ctrl-C cancels the Enter and voids the keys behind it.
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().expect("a gate");
    assert!(handoff.cancel_deferred_enter());
    assert_eq!(handoff.deferred_gate(), None);
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(gate, YoungHoldEnd::Dropped)]
    );

    // So does the session ending, and the next gate never reuses an id.
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let first = handoff.deferred_gate().unwrap();
    handoff.reset_on_exit();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(first, YoungHoldEnd::Dropped)]
    );
    let mut handoff2 = awaiting_report();
    assert!(handoff2.defer_enter());
    assert!(handoff2.deferred_gate().is_some());
}

#[test]
fn a_young_hold_and_a_deferred_enter_never_share_a_gate_id() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (hold, _) = hold_young_enter(&mut handoff, b"\r");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    let gate = handoff
        .deferred_gate()
        .expect("the report made a deferred Enter");
    assert_ne!(gate, hold);
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(
            hold,
            YoungHoldEnd::SubmitOnReport {
                deferred_gate: gate
            }
        )]
    );
}

#[test]
fn a_chunk_absorbed_by_a_deferred_enter_is_flagged_for_the_view_not_stored() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().unwrap();
    // A released prompt-hold chunk (or a key the queue could not take) reaches the deferred state:
    // it is not written, the view is told to put it behind the gate.
    assert!(!handoff.route_released_chunk(b"x"));
    assert_eq!(handoff.take_deferred_key_started(), Some(gate));
    assert_eq!(handoff.take_deferred_key_started(), None, "once");
}

#[test]
fn a_given_up_report_deferral_hands_over_its_gate_and_forgets_it() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().unwrap();
    let id = handoff.take_report_deferral_started().unwrap();
    assert_eq!(handoff.give_up_report_deferral(id), Some(gate));
    assert_eq!(handoff.deferred_gate(), None);
    assert!(
        !handoff.young_hold_end_pending(),
        "the view takes the entries synchronously; no end is left behind"
    );
}

// ---- the shell's line-editor markers (#1390, #1391, #1392) -----------------------------------

/// A hand-off in prompt-sequence mode that has parsed a Precmd, `seq` of the session.
fn at_prompt_with_seq(session: u64, seq: u64) -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(session), Some(seq));
    handoff
}

#[test]
fn the_markers_are_applied_in_stream_order_and_a_second_one_for_a_prompt_changes_nothing() {
    let mut handoff = at_prompt_with_seq(7, 3);
    assert_eq!(handoff.shell_activity(), ShellActivity::Prompt);
    assert!(!handoff.line_editor_markers_live());

    assert!(handoff.on_line_editor_reading(Some(7), Some(3)));
    assert_eq!(handoff.shell_activity(), ShellActivity::Reading);
    assert!(handoff.line_editor_markers_live());
    // Idempotent per prompt: the same sequence number again is not news.
    assert!(!handoff.on_line_editor_reading(Some(7), Some(3)));
    assert_eq!(handoff.shell_activity(), ShellActivity::Reading);

    // The user's line runs: Preexec says a command (or a builtin) is running.
    handoff.on_preexec(true);
    assert_eq!(handoff.shell_activity(), ShellActivity::Running);
    // A late duplicate of the OLD prompt's marker is older than the last Precmd's number only
    // once the next Precmd is parsed; here it is the same prompt, which changes nothing new.
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(5));
    assert_eq!(handoff.shell_activity(), ShellActivity::Prompt);
    assert!(
        !handoff.on_line_editor_reading(Some(7), Some(3)),
        "a marker older than the last Precmd is stale"
    );
    assert_eq!(handoff.shell_activity(), ShellActivity::Prompt);
    assert!(handoff.on_line_editor_reading(Some(7), Some(5)));
    assert_eq!(handoff.shell_activity(), ShellActivity::Reading);
}

#[test]
fn a_raw_read_after_preexec_has_no_marker_so_the_stream_still_says_running() {
    // #1392: a builtin raw read (`read -k`) runs after the command's Preexec and never starts the
    // line editor, so no `LineEditorReading` follows it.
    let mut handoff = at_prompt_with_seq(7, 1);
    handoff.on_line_editor_reading(Some(7), Some(1));
    handoff.on_preexec(true);
    assert_eq!(handoff.shell_activity(), ShellActivity::Running);
    assert!(handoff.line_editor_markers_live());
}

#[test]
fn a_new_session_forgets_the_markers_of_the_last_one() {
    let mut handoff = at_prompt_with_seq(7, 1);
    handoff.on_line_editor_reading(Some(7), Some(1));
    handoff.on_new_session();
    assert_eq!(handoff.shell_activity(), ShellActivity::Unknown);
    assert!(!handoff.line_editor_markers_live());
}

/// #1391: a continued line (PS2) answers the Enter that ended the line so far with neither Precmd
/// nor Preexec; the marker re-baselines the counts, once per continuation prompt.
#[test]
fn a_continuation_prompt_answers_one_unanswered_enter_by_event() {
    let mut handoff = young_after_submit();
    handoff.age_last_activity_for_test();
    // `echo "a` Enter (the command), then `b"` Enter typed into the continued line.
    type_and_flush(&mut handoff, b"b\"\r");
    assert!(handoff.has_unanswered_lines());
    let generation = match handoff.on_line_editor_active(true, true) {
        ActiveDecision::DrainAndWait {
            activity_generation,
        }
        | ActiveDecision::Wait {
            activity_generation,
        } => activity_generation,
        other => panic!("unexpected decision {other:?}"),
    };

    assert!(handoff.on_continuation_prompt());
    assert_eq!(handoff.shell_activity(), ShellActivity::Continuation);
    assert!(handoff.line_editor_markers_live());
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: 2,
            precmds_seen: 1,
        }
    );
    // The stale-count reset armed before the marker is void: the counts are not "all consumed".
    assert!(!handoff.reset_stale_counts(generation));
    // With nothing unanswered a continuation prompt changes no count.
    let mut balanced = after_submit();
    balanced.on_precmd();
    assert!(!balanced.on_continuation_prompt());
}

#[test]
fn a_key_reaching_the_hand_off_under_a_live_young_hold_is_held_not_written() {
    // #1422: the queue and the state disagree (a write bypassed the queue's gate). The state says
    // a hold is live, so the key is held behind it again, never written ahead of the held Enter.
    let mut handoff = young_after_submit();
    assert_eq!(handoff.on_user_bytes(b"frob"), UserBytesRoute::WriteCounted);
    handoff.on_user_write_flushed();
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    let in_flight = handoff.writes_in_flight();
    assert_eq!(handoff.on_user_bytes(b"x"), UserBytesRoute::Held);
    assert_eq!(
        handoff.take_young_hold_started(),
        Some(id),
        "the view is told to put the key behind the same hold's gate"
    );
    assert_eq!(handoff.writes_in_flight(), in_flight, "nothing was written");
}

#[test]
fn a_key_reaching_the_hand_off_under_a_live_prompt_hold_is_held_not_written() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"f"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    assert_eq!(handoff.on_user_bytes(b"o"), UserBytesRoute::Held);
    assert_eq!(handoff.take_prompt_hold_started(), Some(id));
    assert_eq!(handoff.writes_in_flight(), 0);
}

// ---- peek_user_bytes_route (#1424): the pure form of on_user_bytes ----------------------------

/// The states the route depends on, built fresh so a case may commit.
fn peek_states() -> Vec<(&'static str, fn() -> TypeaheadHandoff)> {
    vec![
        ("editor owned", TypeaheadHandoff::default),
        ("pty owned, command running", after_submit),
        ("pty owned, command just written", young_after_submit),
        ("awaiting the report", awaiting_report),
        ("queued young hold", || queued_young_hold().0),
    ]
}

const PEEK_KEYS: [&[u8]; 6] = [b"a", b"\r", b"\x03", b"\x04", b"\x1a", b"\x1b[D"];

/// A peek changes nothing: the whole state prints the same before and after, however often it is
/// asked.
#[test]
fn peeking_the_route_mutates_nothing() {
    for (name, make) in peek_states() {
        for bytes in PEEK_KEYS {
            for reaches_shell_line in [true, false] {
                let handoff = make();
                let before = format!("{handoff:?}");
                let _ = handoff.peek_user_bytes_route(bytes, reaches_shell_line, Instant::now());
                let _ = handoff.peek_user_bytes_route(bytes, reaches_shell_line, Instant::now());
                assert_eq!(
                    before,
                    format!("{handoff:?}"),
                    "{name}: a peek of {bytes:?} changed the hand-off"
                );
            }
        }
    }
}

/// What a peek answers is what the commit that follows it answers (when nothing else moves the
/// state in between: the pty reader thread can, and the shadow counts those).
#[test]
fn a_peek_predicts_the_commit() {
    for (name, make) in peek_states() {
        for bytes in PEEK_KEYS {
            for reaches_shell_line in [true, false] {
                let mut handoff = make();
                let peeked =
                    handoff.peek_user_bytes_route(bytes, reaches_shell_line, Instant::now());
                let committed = if reaches_shell_line {
                    handoff.on_user_bytes(bytes)
                } else {
                    handoff.on_user_bytes_to_full_screen_program(bytes)
                };
                assert_eq!(
                    peeked, committed,
                    "{name}: peek and commit disagree for {bytes:?} (reaches_shell_line={reaches_shell_line})"
                );
            }
        }
    }
}

// ---- #1569: the prompt does not hand input to the editor over keys held for the shell's line ----

/// A ConPTY hand-off whose command was written just now by the app, with the enter window open:
/// the keys typed from here are held until the next prompt's line editor signals.
fn conpty_command_running_in_the_enter_window() -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_conpty_input(true);
    handoff.set_conpty_prompt_hold(true);
    handoff.set_conpty_prompt_hold_shell_signals(true);
    handoff.set_conpty_enter_window_hold(true);
    handoff.on_precmd();
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    handoff.on_command_written();
    handoff
}

/// r18cz Windows Git Bash: a line typed while a command runs is held by the enter window; the
/// command's prompt (precmd) arrived while the keys were still held. The held keys are not counted
/// (the hand-off keeps no bytes), so the shell's line looked empty and the prompt handed input to
/// the editor: the keys typed next went to the editor while the held ones were written raw to the
/// pty, and the report merged the two lines. The pty keeps the line until the release has counted
/// what it holds.
#[test]
fn the_prompt_does_not_commit_to_the_editor_over_keys_held_for_the_shell() {
    let mut handoff = conpty_command_running_in_the_enter_window();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    // The command finished: its precmd arrives before the line editor's signal.
    handoff.on_precmd();
    assert!(
        !handoff.try_commit_at_prompt(),
        "keys are held for the shell's line: the editor must not take over"
    );
    assert!(handoff.is_pty_owned());
    assert!(!handoff.editor_owns_input());
    assert!(
        handoff.routes_keys_to_pty(),
        "the next key is still the pty's, behind the held ones"
    );
    // Nothing is decided until the release has counted the keys.
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    // The line editor's signal releases the keys, a whole line and the start of the next.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    let keys: [&[u8]; 3] = [b"a", b"\r", b"b"];
    for key in keys {
        assert!(handoff.route_released_chunk(key));
    }
    handoff.finish_queued_prompt_hold(id);
    // The release counted them: an Enter is unanswered and the line holds `b`, so neither the
    // prompt nor the line editor hands input to the editor with the shell's line unreported.
    assert!(!handoff.try_commit_at_prompt());
    assert!(handoff.is_pty_owned());
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    assert!(!handoff.editor_owns_input());
}

/// The commit at the prompt is back as soon as no hold is live: nothing typed can be in the line.
#[test]
fn the_prompt_commits_to_the_editor_when_no_keys_are_held() {
    let mut handoff = conpty_command_running_in_the_enter_window();
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

// ---- #1574: a report the hand-off cannot account for --------------------------------------------

/// A pty-owned hand-off whose line holds `ec` at a prompt, ready to ask for the shell's report.
fn line_awaiting_a_request() -> TypeaheadHandoff {
    let mut handoff = after_submit();
    assert_eq!(handoff.on_user_bytes(b"ec"), UserBytesRoute::WriteCounted);
    handoff.on_user_write_flushed();
    handoff.on_precmd();
    handoff
}

/// A line this pane delivered to the shell as final (#1494's `stty sane` path) stays in the shell:
/// while the mark is set no report is asked for (a report clears the shell's line, so the text
/// would leave the shell), the pty keeps input, and the mark is still set. Without the mark the
/// same line is asked for as before.
#[test]
fn a_committed_delivery_is_never_asked_for_and_stays_in_the_shell() {
    let mut handoff = line_awaiting_a_request();
    handoff.note_committed_delivery();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Drain
    );
    assert!(handoff.is_pty_owned());
    assert!(handoff.committed_delivery_pending());
    assert!(!handoff.try_commit_at_prompt(), "the line is not empty");
    // The user's Enter, typed after the young-Enter window (#933) has passed, submits the line
    // where it is: the mark ends, nothing was reported.
    handoff.age_last_activity_for_test();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!handoff.committed_delivery_pending());

    // Control: without the mark the line is asked for.
    let mut handoff = line_awaiting_a_request();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

/// The mark ends as soon as the shell's line cannot hold the delivered bytes any more, so a later
/// report of keys the hand-off owns is never refused for it.
#[test]
fn the_committed_delivery_mark_ends_when_its_line_cannot_hold_it() {
    // A user Enter submits the line.
    let mut handoff = after_submit();
    handoff.note_committed_delivery();
    assert_eq!(handoff.on_user_bytes(b"x\r"), UserBytesRoute::WriteCounted);
    assert!(!handoff.committed_delivery_pending());
    // A command written from the editor (its kill prefix ends the line).
    let mut handoff = TypeaheadHandoff::default();
    handoff.note_committed_delivery();
    handoff.on_command_written();
    assert!(!handoff.committed_delivery_pending());
    // The prompt hands input to the editor: the line is empty.
    let mut handoff = after_submit();
    handoff.note_committed_delivery();
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert!(!handoff.committed_delivery_pending());
    // ... and a later report of keys the hand-off owns is accounted.
    let mut handoff = line_awaiting_a_request();
    handoff.note_committed_delivery();
    handoff.age_last_activity_for_test();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!handoff.committed_delivery_pending());
}

/// #1589: the delivered bytes may still be deferred behind the queue's gate (nothing written
/// yet), so the line looks empty: the pty keeps input and nothing commits to the editor until
/// the bytes land.
#[test]
fn committed_delivery_pins_the_pty_until_the_bytes_land() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_command_written();
    handoff.note_committed_delivery();
    assert!(!handoff.try_commit_at_prompt());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Drain
    );
    assert!(handoff.is_pty_owned());
    assert!(handoff.committed_delivery_pending());
    assert_eq!(
        handoff.on_user_bytes(b"echo hi\r"),
        UserBytesRoute::WriteCounted
    );
    // The Enter ended the line (and the mark): its prompt is still owed, so nothing commits.
    assert!(!handoff.committed_delivery_pending());
    assert!(!matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    ));
    assert!(handoff.is_pty_owned());
    handoff.on_command_written();
    assert!(!handoff.committed_delivery_pending());
}

/// #1589 liveness: the bytes never landed (the remote exited) and no line is queued; the outer
/// shell's prompt is the event that clears the mark and lets the editor take input.
#[test]
fn committed_delivery_clears_at_a_prompt_with_an_empty_line() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_command_written();
    handoff.note_committed_delivery();
    assert!(!handoff.try_commit_at_prompt());
    handoff.on_precmd();
    assert!(!handoff.committed_delivery_pending());
    assert!(handoff.try_commit_at_prompt());
}

// ---- #1584: a give-up never writes to the pty ----------------------------------------------------

/// Keys held in the precmd's wait at a prompt whose line holds nothing typed may go back to the
/// editor in order; keys held by an enter window (the shell's state unknown) may not.
#[test]
fn only_a_hold_at_a_clean_prompt_can_go_back_to_the_editor() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(handoff.prompt_hold_can_hand_back(id));
    assert!(!handoff.prompt_hold_can_hand_back(id + 1));
    assert!(handoff.expire_queued_prompt_hold(id));
    handoff.finish_queued_prompt_hold(id);
    assert!(
        handoff.try_commit_at_prompt(),
        "the editor takes input over"
    );

    let mut handoff = conpty_ready_with_enter_window();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(!handoff.prompt_hold_can_hand_back(id));
}

/// A hold kept past its cap (an Enter among its keys) keeps its wait, so the line editor's signal
/// still releases it; a shell that had never signalled is switched off meanwhile, and back on when
/// the signal shows it does signal.
#[test]
fn an_overdue_hold_kept_for_the_events_is_still_released_by_the_signal() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(handoff.keep_overdue_prompt_hold(id));
    assert!(handoff.prompt_hold_off_for_session());
    assert!(handoff.awaiting_line_editor_signal());
    assert!(handoff.has_prompt_hold());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    assert!(!handoff.prompt_hold_off_for_session());
    assert!(!handoff.keep_overdue_prompt_hold(id + 1));
}

/// A preexec while keys wait in the precmd's wait (the shell started a line it already had) turns
/// the hold into an enter window: not overdue while the command runs, released by its latch.
#[test]
fn a_preexec_turns_a_waiting_prompt_hold_into_an_enter_window() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    handoff.on_preexec(true);
    assert!(!handoff.awaiting_line_editor_signal());
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::NotYet(CONPTY_PROMPT_HOLD_GIVE_UP)
    );
    assert_eq!(handoff.release_enter_window_for_program(), Some(id));
}

// ---- #1562: a setup write must not land on a line that holds delivered typed text --------------

#[test]
fn the_shell_line_holds_typed_text_from_a_delivered_key_until_enter_or_a_kill() {
    let mut handoff = after_submit();
    assert!(!handoff.shell_line_may_hold_typed_text());
    type_and_flush(&mut handoff, b"ec");
    assert!(handoff.shell_line_may_hold_typed_text());
    type_and_flush(&mut handoff, b"\r");
    assert!(!handoff.shell_line_may_hold_typed_text(), "Enter ends it");

    // ^C and ^U end the line as the tty does; neither is counted as an Enter.
    for kill in [b"\x03", b"\x15"] {
        let mut handoff = after_submit();
        type_and_flush(&mut handoff, b"ec");
        assert!(handoff.shell_line_may_hold_typed_text());
        let enters_before = handoff.counts_diverged();
        type_and_flush(&mut handoff, kill);
        assert!(!handoff.shell_line_may_hold_typed_text());
        assert_eq!(handoff.counts_diverged(), enters_before);
    }

    // Untracked bytes in the line count too.
    let mut untracked = TypeaheadHandoff::default();
    assert!(!untracked.shell_line_may_hold_typed_text());
    untracked.note_untracked_line_bytes();
    assert!(untracked.shell_line_may_hold_typed_text());
}

/// F7 (#1595): PSReadLine does not bind ^U, so on PowerShell a raw ^U leaves the line dirty in the
/// hand-off's accounting; ^C still ends the line, and every other shell keeps ^U as a kill.
#[test]
fn ctrl_u_is_not_a_line_kill_on_powershell() {
    let mut handoff = after_submit();
    handoff.set_ctrl_u_kills_line(false);
    type_and_flush(&mut handoff, b"ec");
    assert!(handoff.shell_line_may_hold_typed_text());
    type_and_flush(&mut handoff, b"\x15");
    assert!(
        handoff.shell_line_may_hold_typed_text(),
        "^U left the PowerShell line dirty"
    );
    type_and_flush(&mut handoff, b"\x03");
    assert!(
        !handoff.shell_line_may_hold_typed_text(),
        "^C still ends it"
    );

    let mut posix = after_submit();
    posix.set_ctrl_u_kills_line(true);
    type_and_flush(&mut posix, b"ec");
    type_and_flush(&mut posix, b"\x15");
    assert!(!posix.shell_line_may_hold_typed_text());
}

// ---- #1392: a raw read after Preexec, told from the #933 race by the reader-side barrier -------

/// What the probe of [`raw_read_handoff`] answers, and what it was asked, in order.
#[derive(Clone)]
struct RawReadProbe {
    fact: ShellPromptFact,
    /// What the barrier answers; `None`: no barrier installed.
    drained: Option<Arc<std::sync::atomic::AtomicBool>>,
    asked: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl RawReadProbe {
    fn new(fact: ShellPromptFact, drained: Option<bool>) -> Self {
        Self {
            fact,
            drained: drained.map(|value| Arc::new(std::sync::atomic::AtomicBool::new(value))),
            asked: Arc::default(),
        }
    }

    fn install(&self, handoff: &mut TypeaheadHandoff) {
        let (fact, asked) = (self.fact, self.asked.clone());
        let mut probe = ForegroundProbe::default().with_prompt_fact(Arc::new(move || {
            asked.lock().unwrap().push("sample");
            fact
        }));
        if let Some(drained) = self.drained.clone() {
            let asked = self.asked.clone();
            probe = probe.with_stream_drained(Arc::new(move || {
                asked.lock().unwrap().push("barrier");
                drained.load(Ordering::SeqCst)
            }));
        }
        handoff.set_foreground_probe(probe);
    }

    fn asked(&self) -> Vec<&'static str> {
        std::mem::take(&mut *self.asked.lock().unwrap())
    }

    /// The reader applied (or stopped applying) the stream: what the barrier answers from now on.
    fn set_drained(&self, drained: bool) {
        self.drained
            .as_ref()
            .expect("a barrier is installed")
            .store(drained, Ordering::SeqCst);
    }
}

/// zsh `read -k` typed as the editor's command: the prompt's Precmd and `LineEditorReading` (when
/// `markers`), the command written, its Preexec parsed, and a key typed into the running read.
/// The platform then samples the shell's group in front with a raw tty.
fn raw_read_handoff(probe: &RawReadProbe, markers: bool) -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd_seq(Some(7), Some(1));
    if markers {
        assert!(handoff.on_line_editor_reading(Some(7), Some(1)));
    }
    probe.install(&mut handoff);
    handoff.on_command_written();
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    type_and_flush(&mut handoff, b"y");
    probe.asked();
    handoff
}

/// The case #1392 is about: the hook account is provably current (the barrier is drained after
/// the sample) and says a command runs, so the raw tty is that command's read. The Enter goes to
/// the pty at once instead of waiting 2 s for a prompt that is not coming.
#[test]
fn a_raw_read_after_preexec_gets_the_enter_once_the_stream_is_provably_current() {
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = raw_read_handoff(&probe, true);
    assert_eq!(handoff.shell_activity(), ShellActivity::Running);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!handoff.has_young_hold());
    assert_eq!(
        probe.asked(),
        vec!["sample", "barrier"],
        "the barrier is checked AFTER the sample, never before"
    );

    // The peek (destination table) agrees with the commit.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let peek = raw_read_handoff(&probe, true);
    assert_eq!(
        peek.peek_user_bytes_route(b"\r", true, Instant::now()),
        UserBytesRoute::WriteCounted
    );
}

/// The #933 race looks the same to the sample and the stream (raw tty, Running), but the shell's
/// Precmd is still unread or unparsed: the barrier is not drained, so the Enter is held for that
/// prompt exactly as before.
#[test]
fn an_undrained_stream_keeps_the_hold_for_the_prompt() {
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = raw_read_handoff(&probe, true);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert_eq!(handoff.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);
    assert_eq!(probe.asked(), vec!["sample", "barrier"]);

    // No barrier installed (a shell that writes its Precmd after raw mode): as before #1614, held
    // for the prompt with the 2 s cap as the net (owner decision 2026-11-03).
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, None);
    let mut handoff = raw_read_handoff(&probe, true);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdMaybeInFlight)
    );
    assert_eq!(handoff.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);
}

/// #1614 (owner decision 2026-11-03, `ShellCooked`): the shell's own group in front with a cooked
/// tty is the shell between a command's end and its line editor (`PROMPT_COMMAND`, PS1
/// substitutions), or a cooked builtin. With a line unanswered and the stream not applied, its
/// Precmd may be in it: held for the barrier, which releases it into the request or a delivery.
/// With the stream applied (or no barrier), delivered. Another process group in front (an inner
/// shell, ssh) stays `Unknown` and is delivered at once.
#[test]
fn a_cooked_shell_in_front_holds_only_while_the_stream_is_unapplied() {
    let probe = RawReadProbe::new(ShellPromptFact::ShellCooked, Some(false));
    let mut handoff = seq_after_submit(ShellPromptFact::ShellCooked);
    probe.install(&mut handoff);
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(
        handoff.peek_user_bytes_route(b"\r", true, Instant::now()),
        UserBytesRoute::Held
    );
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert_eq!(handoff.on_stream_applied(), StreamApplied::Nothing);
    // The slice held the Precmd: the request is due.
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd();
    probe.set_drained(true);
    assert_eq!(handoff.on_stream_applied(), StreamApplied::RetryRequest);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);

    // Applied with no Precmd in it (a cooked `read x` took the line): typeahead, by the barrier.
    let probe = RawReadProbe::new(ShellPromptFact::ShellCooked, Some(false));
    let mut handoff = seq_after_submit(ShellPromptFact::ShellCooked);
    probe.install(&mut handoff);
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    probe.set_drained(true);
    assert_eq!(handoff.on_stream_applied(), StreamApplied::HoldEnded);
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Typeahead)]
    );

    // Already applied at the Enter: delivered. No barrier: delivered.
    for barrier in [Some(true), None] {
        let probe = RawReadProbe::new(ShellPromptFact::ShellCooked, barrier);
        let mut handoff = seq_after_submit(ShellPromptFact::ShellCooked);
        probe.install(&mut handoff);
        handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
        handoff.on_preexec(false);
        type_and_flush(&mut handoff, b"explain");
        assert_eq!(
            handoff.on_user_bytes(b"\r"),
            UserBytesRoute::WriteCounted,
            "{barrier:?}"
        );
        assert!(!handoff.has_young_hold());
    }

    // The counts say the prompt is back (its Precmd parsed, the editor not raw yet): the request
    // is due once the line editor is active, as for an unsampled `Unknown`.
    let mut cooked_at_prompt = seq_after_submit(ShellPromptFact::ShellCooked);
    cooked_at_prompt.on_precmd();
    type_and_flush(&mut cooked_at_prompt, b"explain");
    assert_eq!(cooked_at_prompt.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        cooked_at_prompt.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
    assert_eq!(
        cooked_at_prompt.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );

    // Another process group in front (an inner shell, ssh): `Unknown`, delivered at once.
    let mut inner = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut inner, b"explain");
    assert_eq!(inner.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);

    // The written command's own Preexec not yet parsed (R2b): the cooked shell in front may be
    // about to read it; the window, as for the line editor.
    let probe = RawReadProbe::new(ShellPromptFact::ShellCooked, Some(false));
    let mut pending = seq_after_submit(ShellPromptFact::ShellCooked);
    probe.install(&mut pending);
    type_and_flush(&mut pending, b"explain");
    assert_eq!(pending.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        pending.young_hold_evidence(),
        Some(YoungHoldEvidence::TimingWindow)
    );
}

/// The decision table for the barrier itself, through a real [`PtyStreamBarrier`]: only equal
/// counts AND an empty master let the Enter through.
#[test]
fn only_equal_counts_and_an_empty_master_let_the_enter_through() {
    use crate::terminal::model::pty_stream_barrier::PtyStreamBarrier;
    use std::sync::atomic::AtomicI64;

    // `unread`: -1 the master cannot tell, else the byte count FIONREAD / TIOCOUTQ answers.
    let route = |read: usize, applied: u64, unread: i64| {
        let barrier = Arc::new(PtyStreamBarrier::default());
        barrier.begin_read();
        barrier.end_read(read);
        barrier.publish_applied(applied);
        let unread = Arc::new(AtomicI64::new(unread));
        let mut handoff = raw_read_handoff(
            &RawReadProbe::new(ShellPromptFact::LineEditorReading, None),
            true,
        );
        handoff.set_foreground_probe(
            ForegroundProbe::default()
                .with_prompt_fact(Arc::new(|| ShellPromptFact::LineEditorReading))
                .with_stream_drained(Arc::new(move || {
                    barrier.drained_with(|| match unread.load(Ordering::SeqCst) {
                        -1 => None,
                        count => Some(count != 0),
                    })
                })),
        );
        handoff.on_user_bytes(b"\r")
    };
    // read == applied, nothing unread: the stream is current.
    assert_eq!(route(512, 512, 0), UserBytesRoute::WriteCounted);
    // read > applied: a Precmd may be read but not parsed yet.
    assert_eq!(route(512, 400, 0), UserBytesRoute::Held);
    // Bytes still on the master: a Precmd may not be read yet.
    assert_eq!(route(512, 512, 37), UserBytesRoute::Held);
    assert_eq!(route(512, 400, 37), UserBytesRoute::Held);
    // The master cannot be asked.
    assert_eq!(route(512, 512, -1), UserBytesRoute::Held);
}

/// The marker states, with the barrier drained. #1614 changed these expectations: with the counts
/// queuing a line and the editor sampled reading, the barrier is now always asked (after the
/// sample), and a drained stream whose last hook is a Preexec proves a command's read, with or
/// without markers (held 2 s before, for a prompt the running command could not produce before it
/// ends -- the lifecycle is not at a prompt, so no request can be sent): `read -k`, or zle inside
/// `vared`, which sends the line-editor marker too. A Precmd as the last hook means the editor
/// reads a prompt line and the counts are stale: the request is due.
#[test]
fn the_veto_needs_live_markers_and_a_preexec_as_the_last_hook() {
    // No markers from this bootstrap: the stream account cannot say "running, not the editor",
    // but the barrier can: every Precmd is applied and the last hook parsed is a Preexec. A
    // command's read; delivered.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = raw_read_handoff(&probe, false);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(probe.asked(), vec!["sample", "barrier"]);

    // The prompt's Precmd applied after the Preexec (activity Prompt): the editor is back, the
    // counts agree, the request is due. The barrier is not asked.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = raw_read_handoff(&probe, true);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd();
    assert_eq!(handoff.shell_activity(), ShellActivity::Prompt);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
    assert_eq!(probe.asked(), vec!["sample"]);

    // `LineEditorReading` after the Preexec with no Precmd since (zsh `vared`: zle inside a
    // running command): the block is running, so no request can be sent before it ends. The
    // Enter is the command's; delivered, the counts untouched.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = raw_read_handoff(&probe, true);
    handoff.on_line_editor_reading(Some(7), Some(2));
    assert_eq!(handoff.shell_activity(), ShellActivity::Reading);
    assert!(handoff.counts_diverged());
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(handoff.counts_diverged(), "nothing proven stale");
    assert_eq!(probe.asked(), vec!["sample", "barrier"]);

    // A Preexec that no written line accounts for (every line answered by a Precmd): the
    // counts disagree with "the written command runs", so the barrier is not asked.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = raw_read_handoff(&probe, true);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd();
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    assert_eq!(handoff.shell_activity(), ShellActivity::Running);
    assert!(!handoff.counts_diverged());
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(probe.asked(), vec!["sample"]);

    // A continuation prompt after it (activity Continuation) with no Precmd since: the same, a
    // running command's read.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = raw_read_handoff(&probe, true);
    handoff.on_continuation_prompt();
    assert_eq!(handoff.shell_activity(), ShellActivity::Continuation);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!handoff.has_young_hold());
    assert_eq!(probe.asked(), vec!["sample", "barrier"]);
}

/// A Precmd (or CommandFinished) the block lifecycle ignored leaves the activity at Running, but
/// the shell did go back to its prompt: the parsed-hook record voids the veto. #1614: the barrier
/// is then asked (the counts queue a line the editor's reading contradicts), and the hold stands
/// on the request being due.
#[test]
fn a_precmd_the_lifecycle_ignored_still_voids_the_veto() {
    for hook in [ParsedShellHook::Precmd, ParsedShellHook::CommandFinished] {
        let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
        let mut handoff = raw_read_handoff(&probe, true);
        handoff.note_shell_hook_parsed(hook);
        assert_eq!(handoff.shell_activity(), ShellActivity::Running);
        assert_eq!(
            handoff.on_user_bytes(b"\r"),
            UserBytesRoute::Held,
            "{hook:?}"
        );
        assert_eq!(
            handoff.young_hold_evidence(),
            Some(YoungHoldEvidence::RequestDue)
        );
        assert_eq!(handoff.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);
        assert_eq!(probe.asked(), vec!["sample", "barrier"]);
    }

    // A new session forgets the record (and the markers) of the last one.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = raw_read_handoff(&probe, true);
    handoff.on_new_session();
    assert_eq!(handoff.last_parsed_hook, None);
}

/// The barrier only touches the `LineEditorReading` sample: a program in front still gets the
/// Enter, barrier unasked. #1614: a sampled `Unknown` (the shell's group is not in front with a
/// raw tty) with the line outstanding is delivered too, barrier unasked; before, it took the
/// window hold of #1390, which no event could release (the inner-shell failure of #1614).
#[test]
fn the_barrier_changes_nothing_for_the_other_samples() {
    let probe = RawReadProbe::new(ShellPromptFact::NotAtPrompt, Some(false));
    let mut handoff = raw_read_handoff(&probe, true);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(probe.asked(), vec!["sample"]);

    let probe = RawReadProbe::new(ShellPromptFact::Unknown, Some(true));
    let mut handoff = raw_read_handoff(&probe, true);
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!handoff.has_young_hold());
    assert_eq!(probe.asked(), vec!["sample"]);
}

/// #1609, redone by #1614: an Enter typed on a line the user began right after submitting `ssh` is
/// for the ssh session. The local shell's counts (one line written, no Precmd yet) made it a
/// "young" line, held for a local prompt that never comes, whose give-up handed the line back to
/// the editor instead of the remote shell. No flag names ssh any more; the foreground facts do.
/// Where the platform samples, the local tty shows a login hop in front, never the local shell's
/// line editor (`Unknown` from an installed probe); where it samples nothing (ConPTY, the Windows
/// r18e4 failure) the block latch calls the `ssh` command interactive. Either delivers.
#[test]
fn an_enter_typed_into_an_ssh_is_delivered_by_the_foreground_facts_not_a_flag() {
    // Control: the same sequence with nothing known about the foreground is held (#933).
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"echo T");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert!(handoff.take_young_hold_started().is_some());

    // A sampled platform: ssh owns the foreground.
    let mut sampled = seq_after_submit(ShellPromptFact::Unknown);
    assert_eq!(
        type_and_flush(&mut sampled, b"echo T"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(sampled.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(sampled.take_young_hold_started(), None, "no hold, no timer");
    // The peek the destination table's shadow takes agrees.
    assert_eq!(
        sampled.peek_user_bytes_route(b"\r", true, Instant::now()),
        UserBytesRoute::WriteCounted
    );

    // ConPTY: nothing sampled, but the block latch said the ssh is interactive.
    let mut latched = young_after_submit();
    latched.note_command_interactive();
    type_and_flush(&mut latched, b"echo T");
    assert_eq!(latched.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert_eq!(latched.take_young_hold_started(), None, "no hold, no timer");
    assert_eq!(
        latched.peek_user_bytes_route(b"\r", true, Instant::now()),
        UserBytesRoute::WriteCounted
    );

    // When the ssh block ends (its Precmd) the local shell owns the pty again and the fact is
    // gone; the next command's block earns its own. (The Enter typed into ssh left the counts
    // diverged by one, enters 2 / precmds 1, which the stale-count rules settle.)
    latched.on_precmd();
    assert!(!latched.command_interactive());
    assert!(latched.counts_diverged());
    // A fresh command after a clean prompt: the young hold applies as before (the control above).
    let mut local = young_after_submit();
    local.note_command_interactive();
    local.on_precmd();
    local.on_precmd();
    local.on_command_written();
    assert!(!local.command_interactive());
    type_and_flush(&mut local, b"echo T");
    assert_eq!(local.on_user_bytes(b"\r"), UserBytesRoute::Held);
}

// ---- #1614: the young hold engages on positive evidence and names its release -----------------

/// The macOS r18e4 `read_x` shape: `read x` was written, the token line's Enter went to the pty and
/// `read` consumed it, and the Precmd after `read` came with the interactive latch unset (the log
/// says `consumed=false`), so the counts queue a line (enters 2, precmds 1) that no prompt will
/// ever answer. The shell is at its line editor.
fn read_x_finished(probe: &RawReadProbe) -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd_seq(Some(7), Some(1));
    probe.install(&mut handoff);
    // `read x` runs: the token line is typed into it and counted as a line for the shell.
    handoff.on_command_written();
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    assert_eq!(
        type_and_flush(&mut handoff, b"tok\r"),
        UserBytesRoute::WriteCounted
    );
    // `read` returns; the shell's Precmd. No latch, so no correction.
    handoff.note_shell_hook_parsed(ParsedShellHook::CommandFinished);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert!(
        handoff.counts_diverged(),
        "enters 2, precmds 1: the shape of #1614"
    );
    probe.asked();
    handoff
}

/// read_x (#1614): the next line's Enter is never held for a prompt that is not coming. With the
/// echo of the typed keys still unapplied the hold waits on the barrier; the barrier's publish
/// then proves the counts stale (the editor reads with every Precmd applied), corrects them and
/// makes the request due: the report runs the Enter in the editor. Before, the 2 s cap handed the
/// Enter back and the 5 s stale-count give-up sent the request too late.
#[test]
fn read_x_finished_then_the_next_enter_is_held_only_for_a_request_that_is_due() {
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = read_x_finished(&probe);
    // The line editor became active while the stream was still being applied: drain and wait,
    // as the log showed.
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));
    probe.asked();
    assert_eq!(
        type_and_flush(&mut handoff, b"printf x"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert_eq!(probe.asked(), vec!["sample", "barrier"]);
    // The reader applies the echo; nothing else was in the stream.
    probe.set_drained(true);
    assert_eq!(handoff.on_stream_applied(), StreamApplied::RetryRequest);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
    assert!(
        !handoff.counts_diverged(),
        "corrected at once, not after the 5 s give-up"
    );
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.expire_queued_young_hold(id),
        YoungHoldExpiry::Keep,
        "the report is coming"
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert!(matches!(
        handoff.take_young_hold_ends().as_slice(),
        [(end_id, YoungHoldEnd::SubmitOnReport { .. })] if *end_id == id
    ));

    // Typed with the stream already applied at the Enter: the stale counts are proven there.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = read_x_finished(&probe);
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));
    type_and_flush(&mut handoff, b"printf x");
    probe.set_drained(true);
    probe.asked();
    assert_eq!(
        handoff.peek_user_bytes_route(b"\r", true, Instant::now()),
        UserBytesRoute::Held
    );
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
    assert!(!handoff.counts_diverged());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
}

/// #1614 (design item 4): the request decision itself recognises stale counts when the shell's
/// line editor reads with every Precmd applied, and commits the editor at once; the 5 s give-up
/// is left for the cases the barrier cannot vouch for. A raw read after a Preexec is not stale.
#[test]
fn stale_counts_are_recognised_in_the_request_decision_itself() {
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = read_x_finished(&probe);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert_eq!(probe.asked(), vec!["sample", "barrier"]);

    // The stream not yet applied: the barrier cannot vouch; wait as before.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = read_x_finished(&probe);
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));

    // No sample of the editor reading (an inner shell in front): not provably stale.
    let probe = RawReadProbe::new(ShellPromptFact::Unknown, Some(true));
    let mut handoff = read_x_finished(&probe);
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));

    // A command's raw read (`read -k` after its Preexec, no marker since): the line is queued for
    // real; not stale.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd_seq(Some(7), Some(1));
    probe.install(&mut handoff);
    handoff.on_command_written();
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    assert!(handoff.counts_diverged());
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));
}

/// subshells (#1614): `bash` was written and the inner shell's group owns the foreground (the
/// sample says `Unknown` from an installed probe); it sends no Precmd. Every Enter typed into it
/// is delivered. On macOS r18e4 each was held by the window and handed back 20 ms later, 28 times
/// per pass, because the hand-back left `young_line` set and every later Enter looked "in the
/// window".
#[test]
fn an_enter_typed_into_an_unhooked_inner_shell_is_delivered_every_time() {
    let mut handoff = seq_after_submit(ShellPromptFact::Unknown);
    for line in [&b"echo tok >> f"[..], b"sh -c 'ps' >> f", b"exit"] {
        assert_eq!(
            type_and_flush(&mut handoff, line),
            UserBytesRoute::WriteCounted
        );
        assert_eq!(
            handoff.peek_user_bytes_route(b"\r", true, Instant::now()),
            UserBytesRoute::WriteCounted
        );
        assert_eq!(
            type_and_flush(&mut handoff, b"\r"),
            UserBytesRoute::WriteCounted
        );
        assert_eq!(handoff.take_young_hold_started(), None);
    }
    // The inner shell's block latches interactive (it outlives the debounce with a shell in front):
    // the same answer on a platform that samples nothing.
    let mut latched = seq_unsampled_after_submit();
    latched.note_command_interactive();
    type_and_flush(&mut latched, b"echo tok >> f");
    assert_eq!(latched.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
}

/// #1614: the hand-back of a hold ends the "young line" too. Left set, the next Enter typed on the
/// same shell line was "in the window" forever, so the net fired on every Enter.
#[test]
fn a_hand_back_ends_the_young_line_so_the_net_cannot_fire_on_every_enter() {
    let (mut handoff, id) = queued_young_hold();
    handoff.young_line = true;
    assert_eq!(
        handoff.expire_queued_young_hold(id),
        YoungHoldExpiry::Release
    );
    handoff.finish_young_hold_handed_back(id);
    assert!(!handoff.young_line);
    assert_eq!(handoff.young_hold_evidence(), None);
    // Outside the window, the next Enter on the same shell line is written, not held again.
    handoff.age_last_activity_for_test();
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
}

/// #1614 (migration item 5): a hold for a request the shell cannot answer (no reporting binding)
/// is ended by the decision that finds so, as typeahead; the comment that said "the hold's expiry
/// writes the Enter" had been false since #1577 made the expiry a hand-back.
#[test]
fn a_hold_for_a_request_the_shell_cannot_answer_is_written_as_typeahead_by_the_decision() {
    // The prompt's Precmd is parsed but the line editor is not active yet (ConPTY, no sample):
    // the user types a line and its Enter; the request is due.
    let mut handoff = seq_unsampled_after_submit();
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    type_and_flush(&mut handoff, b"frob");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
    assert_eq!(handoff.young_hold_wait(Instant::now()), EVENT_HOLD_GIVE_UP);
    // No binding: the decision ends the hold now, by this event.
    assert_eq!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Drain
    );
    assert!(!handoff.young_hold_queued());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Typeahead)]
    );
    // The view writes the keys; they are counted as the user bytes they are, one write.
    let writes = handoff.route_expired_young_chunks(vec![b"\r".to_vec(), b"x".to_vec()]);
    assert_eq!(writes, vec![b"\rx".to_vec()]);
    assert_eq!(handoff.writes_in_flight(), 1);
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);

    // With the binding, the same hold is released by the report.
    let mut handoff = seq_unsampled_after_submit();
    handoff.on_precmd();
    type_and_flush(&mut handoff, b"frob");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
}

/// alt_screen (#1614): after `vim` / `less`, the next line's Enter is never lost. The keys typed
/// into the program reached it raw and uncounted; its exit brings the shell's Precmd, so the counts
/// agree and the request is due at the first Enter typed before the line editor is known active.
/// An Enter that WAS counted while the program ran (typed before the alternate screen was known)
/// leaves the counts stale, which the barrier proves at the Enter.
#[test]
fn after_a_full_screen_program_the_next_enter_is_never_lost() {
    for counted_enter_during_program in [false, true] {
        let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(true));
        let mut handoff = TypeaheadHandoff::default();
        handoff.set_prompt_seq_enabled(true);
        handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
        handoff.on_precmd_seq(Some(7), Some(1));
        probe.install(&mut handoff);
        handoff.on_command_written();
        handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
        handoff.on_preexec(false);
        if counted_enter_during_program {
            assert_eq!(
                type_and_flush(&mut handoff, b"\r"),
                UserBytesRoute::WriteCounted
            );
        }
        assert_eq!(
            handoff.on_user_bytes_to_full_screen_program(b":wq\r"),
            UserBytesRoute::WriteCounted
        );
        handoff.on_user_write_flushed();
        handoff.note_shell_hook_parsed(ParsedShellHook::CommandFinished);
        handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
        handoff.on_precmd();
        handoff.on_precmd_seq(Some(7), Some(3));
        assert_eq!(handoff.counts_diverged(), counted_enter_during_program);
        probe.asked();
        type_and_flush(&mut handoff, b"echo AFTER_ALT");
        assert_eq!(
            handoff.on_user_bytes(b"\r"),
            UserBytesRoute::Held,
            "counted={counted_enter_during_program}"
        );
        assert_eq!(
            handoff.young_hold_evidence(),
            Some(YoungHoldEvidence::RequestDue)
        );
        assert!(!handoff.counts_diverged());
        assert_eq!(
            handoff.on_line_editor_active(true, true),
            ActiveDecision::SendRequest
        );
        assert_eq!(
            handoff.on_report(),
            ReportDisposition::InsertThenSubmitYoung,
            "counted={counted_enter_during_program}"
        );
    }
}

/// #1614 (evidence B): the hold on "a Precmd may be in flight" lives exactly as long as the stream
/// is unapplied. Each publish of the barrier decides again: still unapplied, nothing; applied with
/// the Precmd in the slice (#933 proper), the request is due; applied with a program in front, the
/// keys are its typeahead.
#[test]
fn evidence_b_holds_only_while_the_stream_is_unapplied() {
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    probe.install(&mut handoff);
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert_eq!(handoff.on_stream_applied(), StreamApplied::Nothing);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    // The slice held the Precmd: the counts caught up.
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd();
    probe.set_drained(true);
    assert_eq!(handoff.on_stream_applied(), StreamApplied::RetryRequest);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);

    // Applied, and a program now owns the foreground: typeahead for it, by this event.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    probe.install(&mut handoff);
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let id = handoff.take_young_hold_started().expect("a hold started");
    RawReadProbe::new(ShellPromptFact::NotAtPrompt, Some(true)).install(&mut handoff);
    assert_eq!(handoff.on_stream_applied(), StreamApplied::HoldEnded);
    assert!(!handoff.young_hold_queued());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Typeahead)]
    );
    // Once the request is out, the barrier has nothing to decide: the report runs the Enter.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    probe.install(&mut handoff);
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    probe.set_drained(true);
    assert_eq!(handoff.on_stream_applied(), StreamApplied::Nothing);
    assert!(handoff.young_hold_queued());
}

/// #1614 (release item 2): a Preexec parsed after the hold opened means the shell started a
/// command, so no prompt answers the held Enter before it has run: the keys are its typeahead,
/// written by this event (the documented closing event of the `PromptSeen` gate, implemented).
/// Not the written command's OWN Preexec: a line typed and Entered right after the submit, before
/// that Preexec was parsed (a fast Enter, a paste, ConPTY's Preexec lag), is the next prompt's
/// line, held for it; ending it on the command's Preexec would run it unclassified. Once the
/// request is out a Preexec changes nothing: the report runs the Enter.
#[test]
fn a_preexec_after_the_hold_opened_writes_the_held_keys_as_typeahead() {
    // The hold opened after the command's own Preexec; a further Preexec is a line the shell
    // started itself (typeahead it already had).
    let mut handoff = young_after_submit();
    assert!(!handoff.on_preexec(false));
    type_and_flush(&mut handoff, b"frob");
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    assert!(!handoff.on_preexec(true));
    assert!(!handoff.young_hold_queued());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Typeahead)]
    );
    assert_eq!(
        handoff.expire_queued_young_hold(id),
        YoungHoldExpiry::Gone,
        "the cap finds nothing to hand back"
    );

    // The hold opened before the command's own Preexec: that Preexec does not end it; the
    // command's Precmd makes the request due and the report runs the Enter in the editor.
    let (mut handoff, _) = queued_young_hold();
    assert!(!handoff.on_preexec(false));
    assert!(handoff.young_hold_queued());
    assert!(handoff.take_young_hold_ends().is_empty());
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );

    let (mut handoff, _) = queued_young_hold();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(!handoff.on_preexec(true));
    assert!(handoff.young_hold_queued());
    assert!(handoff.take_young_hold_ends().is_empty());
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
}

/// #1614: the block latch saying the running command is interactive (a program, a blocked `read`,
/// an ssh, an inner shell reads the tty) delivers an Enter typed after it, on both paths, and ends
/// a hold live when it arrives as typeahead. The command's Precmd, or the next command written,
/// clears it.
#[test]
fn a_command_the_latch_calls_interactive_gets_its_enter_and_releases_a_live_hold() {
    for seq_mode in [false, true] {
        let mut handoff = if seq_mode {
            seq_unsampled_after_submit()
        } else {
            young_after_submit()
        };
        handoff.note_command_interactive();
        type_and_flush(&mut handoff, b"frob");
        assert_eq!(
            handoff.peek_user_bytes_route(b"\r", true, Instant::now()),
            UserBytesRoute::WriteCounted,
            "seq_mode={seq_mode}"
        );
        assert_eq!(
            handoff.on_user_bytes(b"\r"),
            UserBytesRoute::WriteCounted,
            "seq_mode={seq_mode}"
        );
        assert_eq!(handoff.take_young_hold_started(), None);
    }

    let (mut handoff, id) = queued_young_hold();
    handoff.note_command_interactive();
    assert!(!handoff.young_hold_queued());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Typeahead)]
    );
    assert!(handoff.command_interactive());
    handoff.on_precmd();
    assert!(!handoff.command_interactive());

    // Where the platform samples, the sample decides and the latch ends nothing (the latch is
    // stale by the length of the pipeline once the command exits: #933's moment).
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut sampled = seq_after_submit(ShellPromptFact::LineEditorReading);
    probe.install(&mut sampled);
    type_and_flush(&mut sampled, b"frob");
    let (_, _) = hold_young_enter(&mut sampled, b"\r");
    sampled.note_command_interactive();
    assert!(sampled.young_hold_queued());
    assert!(sampled.take_young_hold_ends().is_empty());

    let mut handoff = young_after_submit();
    handoff.note_command_interactive();
    handoff.on_command_written();
    assert!(!handoff.command_interactive());

    // A late latch event for a block whose request is already out changes nothing.
    let (mut handoff, _) = queued_young_hold();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    handoff.note_command_interactive();
    assert!(handoff.young_hold_queued());
    assert!(handoff.take_young_hold_ends().is_empty());
}

/// #1614 (review R1): the latch fires 50 ms into nearly any command and stays set until its
/// Precmd is parsed. After `make` exits, its Precmd in the stream unparsed and zle raw, the latch
/// is stale and the sample is current: #933 exactly. Where the platform samples, the latch decides
/// nothing: the Enter is held for the barrier, and the latch event ends no live hold.
#[test]
fn a_latched_command_that_exited_still_holds_the_933_enter() {
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    probe.install(&mut handoff);
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    handoff.note_command_interactive();
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    handoff.note_command_interactive();
    assert!(handoff.young_hold_queued());
    assert!(handoff.take_young_hold_ends().is_empty());
    // The Precmd lands: the request is due, the report runs the Enter.
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd();
    probe.set_drained(true);
    assert_eq!(handoff.on_stream_applied(), StreamApplied::RetryRequest);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );

    // A program still in front says so in the sample: delivered.
    let mut program = seq_after_submit(ShellPromptFact::NotAtPrompt);
    program.note_command_interactive();
    type_and_flush(&mut program, b"explain");
    assert_eq!(program.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);

    // A new session (the remote shell of an ssh) starts with no latch.
    let mut remote = young_after_submit();
    remote.note_command_interactive();
    remote.on_new_session();
    assert!(!remote.command_interactive());
}

/// #1614 (design item 6, the property): over every combination of what the platform samples, what
/// the barrier says, what the hooks said last, whether the command is latched interactive, whether
/// the Enter is in the window and whether the prompt-sequence path is on, a `Held` answer names
/// evidence whose release is satisfiable then and there: the request is due and the report runs
/// the Enter; or the barrier's next publish decides; or the window's Precmd (an event) sends the
/// request. The peek the destination table takes always agrees with the commit.
#[test]
fn every_hold_names_a_release_that_is_satisfiable_when_it_engages() {
    #[derive(Clone, Copy, Debug)]
    enum Probe {
        None,
        Fact(ShellPromptFact, Option<bool>),
    }
    #[derive(Clone, Copy, Debug)]
    enum Hooks {
        None,
        PrecmdApplied,
        PreexecLast,
        ReadingAfterPreexec,
    }
    let mut probes = vec![Probe::None];
    for fact in [
        ShellPromptFact::NotAtPrompt,
        ShellPromptFact::LineEditorReading,
        ShellPromptFact::ShellCooked,
        ShellPromptFact::Unknown,
    ] {
        for barrier in [None, Some(true), Some(false)] {
            probes.push(Probe::Fact(fact, barrier));
        }
    }
    let hooks_dim = [
        Hooks::None,
        Hooks::PrecmdApplied,
        Hooks::PreexecLast,
        Hooks::ReadingAfterPreexec,
    ];
    let (mut cases, mut held) = (0usize, 0usize);
    for probe in probes {
        for hooks in hooks_dim {
            for interactive in [false, true] {
                for in_window in [false, true] {
                    for seq_mode in [false, true] {
                        let label = format!(
                            "{probe:?} {hooks:?} interactive={interactive} \
                             in_window={in_window} seq_mode={seq_mode}"
                        );
                        let raw = match probe {
                            Probe::None => None,
                            Probe::Fact(fact, barrier) => Some(RawReadProbe::new(fact, barrier)),
                        };
                        let mut handoff = TypeaheadHandoff::default();
                        handoff.set_prompt_seq_enabled(seq_mode);
                        handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
                        handoff.on_precmd_seq(Some(7), Some(1));
                        if let Some(raw) = &raw {
                            raw.install(&mut handoff);
                        }
                        handoff.on_command_written();
                        match hooks {
                            Hooks::None => {}
                            Hooks::PrecmdApplied => {
                                handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
                                handoff.on_precmd();
                            }
                            Hooks::PreexecLast => {
                                handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
                                handoff.on_preexec(false);
                            }
                            Hooks::ReadingAfterPreexec => {
                                handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
                                handoff.on_preexec(false);
                                handoff.on_line_editor_reading(Some(7), Some(1));
                            }
                        }
                        if interactive {
                            handoff.note_command_interactive();
                        }
                        type_and_flush(&mut handoff, b"frob");
                        if !in_window {
                            handoff.age_last_activity_for_test();
                            handoff.young_line = false;
                        }
                        let peek = handoff.peek_user_bytes_route(b"\r", true, Instant::now());
                        let route = handoff.on_user_bytes(b"\r");
                        assert_eq!(peek, route, "the peek agrees with the commit: {label}");
                        cases += 1;
                        if route != UserBytesRoute::Held {
                            assert_eq!(handoff.take_young_hold_started(), None, "{label}");
                            continue;
                        }
                        held += 1;
                        let id = handoff
                            .take_young_hold_started()
                            .unwrap_or_else(|| panic!("a hold started: {label}"));
                        assert!(
                            !(interactive && matches!(probe, Probe::None)),
                            "never held into a latched command where nothing is sampled: {label}"
                        );
                        // The block lifecycle is at a prompt unless a Preexec started a block
                        // and no Precmd ended it (zsh `vared`, a raw read).
                        let at_prompt =
                            !matches!(hooks, Hooks::PreexecLast | Hooks::ReadingAfterPreexec);
                        let evidence = handoff
                            .young_hold_evidence()
                            .unwrap_or_else(|| panic!("a hold names its evidence: {label}"));
                        match evidence {
                            YoungHoldEvidence::RequestDue => {
                                assert!(!handoff.counts_diverged(), "no line queued: {label}");
                                assert_eq!(
                                    handoff.on_line_editor_active(at_prompt, true),
                                    ActiveDecision::SendRequest,
                                    "{label}"
                                );
                                assert_eq!(
                                    handoff.on_report(),
                                    ReportDisposition::InsertThenSubmitYoung,
                                    "{label}"
                                );
                            }
                            YoungHoldEvidence::PrecmdInFlight => {
                                let raw = raw.as_ref().unwrap_or_else(|| panic!("{label}"));
                                assert!(
                                    matches!(probe, Probe::Fact(_, Some(false))),
                                    "only an installed, undrained barrier: {label}"
                                );
                                assert_eq!(
                                    handoff.on_stream_applied(),
                                    StreamApplied::Nothing,
                                    "still unapplied: {label}"
                                );
                                raw.set_drained(true);
                                match handoff.on_stream_applied() {
                                    // The written command's own Preexec is still pending and the
                                    // Enter is in the window: the hold waits for that Preexec
                                    // (which does not end it) and the Precmd that makes the
                                    // request due.
                                    StreamApplied::Nothing => {
                                        assert!(
                                            matches!(hooks, Hooks::None) && in_window,
                                            "applied, undecided: {label}"
                                        );
                                        assert!(!handoff.on_preexec(false), "{label}");
                                        assert!(handoff.young_hold_queued(), "{label}");
                                        handoff.on_precmd();
                                        assert_eq!(
                                            handoff.on_line_editor_active(true, true),
                                            ActiveDecision::SendRequest,
                                            "{label}"
                                        );
                                        assert_eq!(
                                            handoff.on_report(),
                                            ReportDisposition::InsertThenSubmitYoung,
                                            "{label}"
                                        );
                                    }
                                    StreamApplied::HoldEnded => {
                                        assert_eq!(
                                            handoff.take_young_hold_ends(),
                                            vec![(id, YoungHoldEnd::Typeahead)],
                                            "{label}"
                                        );
                                    }
                                    StreamApplied::RetryRequest => {
                                        assert!(!handoff.counts_diverged(), "{label}");
                                        assert_eq!(
                                            handoff.on_line_editor_active(at_prompt, true),
                                            ActiveDecision::SendRequest,
                                            "{label}"
                                        );
                                        assert_eq!(
                                            handoff.on_report(),
                                            ReportDisposition::InsertThenSubmitYoung,
                                            "{label}"
                                        );
                                    }
                                }
                            }
                            // The no-barrier hold kept by owner decision 2026-11-03: released
                            // by the Precmd, then the request and the report.
                            YoungHoldEvidence::PrecmdMaybeInFlight => {
                                assert!(
                                    matches!(
                                        probe,
                                        Probe::Fact(ShellPromptFact::LineEditorReading, None)
                                    ),
                                    "only the editor sampled reading with no barrier: {label}"
                                );
                                handoff.on_precmd();
                                assert_eq!(
                                    handoff.on_line_editor_active(true, true),
                                    ActiveDecision::SendRequest,
                                    "{label}"
                                );
                                assert_eq!(
                                    handoff.on_report(),
                                    ReportDisposition::InsertThenSubmitYoung,
                                    "{label}"
                                );
                            }
                            YoungHoldEvidence::TimingWindow => {
                                assert!(in_window, "{label}");
                                assert!(
                                    !seq_mode
                                        || matches!(probe, Probe::None | Probe::Fact(_, None))
                                        || (matches!(hooks, Hooks::None)
                                            && matches!(
                                                probe,
                                                Probe::Fact(
                                                    ShellPromptFact::LineEditorReading,
                                                    Some(true)
                                                ) | Probe::Fact(ShellPromptFact::ShellCooked, _)
                                            )),
                                    "the window only where nothing can be asked, or while the \
                                     written command's own Preexec is pending: {label}"
                                );
                                // Its release: the prompt's Precmd, then the request and report.
                                handoff.on_precmd();
                                assert_eq!(
                                    handoff.on_line_editor_active(true, true),
                                    ActiveDecision::SendRequest,
                                    "{label}"
                                );
                                assert_eq!(
                                    handoff.on_report(),
                                    ReportDisposition::InsertThenSubmitYoung,
                                    "{label}"
                                );
                            }
                        }
                        assert_eq!(
                            handoff.expire_queued_young_hold(id),
                            YoungHoldExpiry::Gone,
                            "released, nothing for the cap: {label}"
                        );
                    }
                }
            }
        }
    }
    assert!(held > 0 && held < cases, "{held} of {cases} held");
}
