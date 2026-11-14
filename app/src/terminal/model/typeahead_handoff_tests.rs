use super::*;

/// A hand-off whose submitted command has been written, i.e. the state every hand-off starts
/// from.
fn after_submit() -> TypeaheadHandoff {
    let mut handoff = young_after_submit();
    // A command that has been running a while: the stream is fully applied, so an Enter is
    // typeahead for it (#933's race needs unparsed output).
    handoff.age_last_activity_for_test();
    handoff
}

/// A hand-off whose command was written just now and whose pty stream has unparsed output, so an
/// Enter the user types may be a line typed at a prompt whose Precmd is still in that output
/// (#933). Where nothing samples the shell the barrier decides, not a clock.
fn young_after_submit() -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_command_written();
    handoff.stream_undrained_for_test();
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

/// A shell that cannot report its line: with nothing typed in its line the editor takes over
/// without a request. With typed bytes in the line (#2197) it does NOT: this test asserted
/// `Committed` for that case until r29, which is the key loss of #2197 (the rest of the line
/// went to the editor and its command write killed the part in the shell).
#[test]
fn shell_without_input_reporting_commits_instead_of_requesting() {
    let mut handoff = after_submit();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Committed
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);

    let mut handoff = after_submit();
    type_and_flush(&mut handoff, b"x");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Drain,
        "no request is possible and the line holds a typed byte: the pty keeps it"
    );
    assert!(handoff.is_pty_owned());
    assert!(handoff.routes_keys_to_pty());
}

/// #2197 (r29 A3): the shell has no report binding (bash before 4.0, macOS `/bin/bash` 3.2). `ec`
/// was typed into the pty before the prompt was seen; the prompt arrives and the line editor
/// becomes active. Input is NOT handed to the editor over the two bytes in the shell's line: the
/// rest of the line (`ho hi`, Enter) follows them to the pty in typed order, so the shell's line
/// is `echo hi` and nothing is killed. The prompt after that line hands input back.
#[test]
fn a_typed_line_in_a_shell_without_a_report_binding_stays_with_the_pty_until_its_enter() {
    // The command was not the hand-off's (such a shell never engages it): the keys typed into
    // it are adopted, as the view does for any key written to a command it does not own.
    let mut handoff = TypeaheadHandoff::default();
    assert!(handoff.adopt_untracked_typeahead());
    let mut shell_line: Vec<u8> = Vec::new();
    for key in [&b"e"[..], b"c"] {
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted
        );
        shell_line.extend_from_slice(key);
    }
    handoff.on_precmd();
    // The stream is applied: nothing is in flight that could make the Enter below a young one.
    handoff.age_last_activity_for_test();
    assert!(
        !handoff.try_commit_at_prompt(),
        "typed bytes are in the shell's line"
    );
    for _ in 0..2 {
        // The activation, and any retry of it: the decision is the same every time.
        assert_eq!(
            handoff.on_line_editor_active(true, false),
            ActiveDecision::Drain
        );
        assert!(handoff.is_pty_owned(), "the pty keeps the line");
        assert!(handoff.routes_keys_to_pty());
        assert!(handoff.wants_terminal_focus());
        assert!(!handoff.editor_owns_input());
    }
    assert!(
        handoff.holds_queued_writes(),
        "a queued command's kill prefix must not wipe the typed line"
    );
    assert!(handoff.shell_line_may_hold_typed_text());

    // The rest of the line and its Enter: every key goes to the pty, none is held or split off.
    for key in [&b"h"[..], b"o", b" ", b"h", b"i", b"\r"] {
        assert_eq!(
            handoff.peek_user_bytes_route(key, true, Instant::now()),
            UserBytesRoute::WriteCounted,
            "{key:?}"
        );
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted,
            "{key:?}"
        );
        shell_line.extend_from_slice(key);
    }
    assert_eq!(shell_line, b"echo hi\r", "the shell's line, nothing killed");
    assert!(!handoff.young_hold_queued());
    assert!(
        !handoff.shell_line_may_hold_typed_text(),
        "the Enter ended it"
    );
    assert!(handoff.has_unanswered_lines());

    // The line's prompt: nothing typed is left in the shell's line, the editor takes input.
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

/// #2197 (r29 A3): the other line boundaries. ^U and ^C leave nothing typed in the line, so the
/// next decision hands input to the editor as for any empty line.
#[test]
fn a_line_kill_or_interrupt_ends_the_typed_line_a_shell_without_a_binding_kept() {
    for boundary in [0x15u8, 0x03] {
        let mut handoff = TypeaheadHandoff::default();
        assert!(handoff.adopt_untracked_typeahead());
        type_and_flush(&mut handoff, b"ec");
        handoff.on_precmd();
        handoff.age_last_activity_for_test();
        assert_eq!(
            handoff.on_line_editor_active(true, false),
            ActiveDecision::Drain
        );
        assert!(handoff.is_pty_owned());
        assert_eq!(
            type_and_flush(&mut handoff, &[boundary]),
            UserBytesRoute::WriteCounted,
            "{boundary:#x} goes to the pty, where the line is"
        );
        assert!(
            !handoff.shell_line_may_hold_typed_text(),
            "{boundary:#x} ended the line"
        );
        assert_eq!(
            handoff.on_line_editor_active(true, false),
            ActiveDecision::Committed,
            "{boundary:#x}: an empty line, the editor takes input"
        );
        assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    }
}

/// #2187: keys in the shell's line wait for a proven line editor; an empty line, or input the
/// editor owns, do not.
#[test]
fn a_typed_line_awaits_a_proven_editor_only_while_it_sits_in_the_shells_line() {
    let mut handoff = after_submit();
    assert!(!handoff.typed_line_awaits_proven_editor(), "nothing typed");
    type_and_flush(&mut handoff, b"x");
    handoff.on_precmd();
    assert!(handoff.typed_line_awaits_proven_editor());
    // No request is possible (r29 A3, #2197): the line stays in the shell and the pty keeps it.
    assert_eq!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Drain
    );
    assert!(handoff.typed_line_awaits_proven_editor());
    // The request collects the line; from then on it is not in the shell.
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(!handoff.typed_line_awaits_proven_editor());
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

/// Mac round 7: the user's command was written to a shell the hand-off does not engage for, with an
/// Enter deferred behind a report. The gate must end and the held keys follow the command.
#[test]
fn an_unengaged_user_command_releases_the_deferred_gate_and_its_held_keys() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().expect("the Enter holds a gate");
    assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);

    handoff.on_unengaged_user_command_written();
    handoff.on_uncounted_command_written();

    assert_eq!(handoff.deferred_gate(), None, "the gate is not left open");
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(gate, YoungHoldEnd::FollowCommand)],
        "the held keys follow the command, never dropped"
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

/// The young hold's keys follow an unengaged user command instead of being dropped. The Enter ends
/// a line with content (`frob`): a bare Enter on an empty shell line is never a young hold
/// (`should_hold_young_enter`), which is why this test, written without it, could not hold one.
#[test]
fn an_unengaged_user_command_makes_a_young_hold_follow_not_drop() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"frob");
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    handoff.on_unengaged_user_command_written();
    handoff.on_uncounted_command_written();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::FollowCommand)]
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

/// #1777: under ConPTY a Ctrl-C does not flush the lines queued behind a running command, so the
/// pty keeps input until a prompt has answered each of them; the editor does not take over after
/// the killed command's prompt alone.
#[test]
fn conpty_ctrl_c_keeps_the_queued_lines_counted_until_their_prompts_arrive() {
    let mut handoff = after_submit();
    handoff.set_conpty_input(true);
    type_and_flush(&mut handoff, b"a\rb\rc");
    type_and_flush(&mut handoff, &[0x03]);
    assert_eq!(handoff.on_interrupt(), InterruptOutcome::CommandRunning);
    // The killed command's prompt: the lines behind it are still owed their prompts.
    handoff.on_precmd();
    assert!(!handoff.try_commit_at_prompt());
    assert!(handoff.is_pty_owned());
    // `a` and `b` ran as commands, one prompt each: now the line is empty.
    handoff.on_precmd();
    assert!(!handoff.try_commit_at_prompt());
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
}

#[test]
fn the_interrupt_enter_count_is_kept_only_on_conpty() {
    // Running command (1 prompt owed) with two more lines queued behind it.
    assert_eq!(interrupt_enters_written(true, 3, 0), 3);
    assert_eq!(interrupt_enters_written(false, 3, 0), 1);
    // Nothing queued behind the command: the same on both.
    assert_eq!(interrupt_enters_written(true, 1, 0), 1);
    assert_eq!(interrupt_enters_written(false, 1, 0), 1);
    // At a prompt: the line is gone on both.
    assert_eq!(interrupt_enters_written(true, 2, 2), 2);
    assert_eq!(interrupt_enters_written(false, 2, 2), 2);
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
    // Left moves the line editor's cursor, so what is typed after it lands in front of the `s`
    // (#2199; this asserted "ls -l" until r29: the motion was dropped and the text appended, which
    // is text the user did not type). Up (SS3 A) recalls history: unknown here, dropped whole, and
    // so is a cursor key with a modifier.
    assert_eq!(text_for_editor(b"ls\x1b[D -l\x1bOA"), "l -ls");
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

/// Owner decision 2026-11-09 (reversing #1661): the leak net of the deferred Enter's gate gives
/// the DEFERRAL up, not only the gate: input goes back to the editor and the gate id is gone, so
/// the keys the view then writes to the pty are not held behind the same gate again (left in
/// `HoldAfterEnter`, every one of them was). Another gate's net changes nothing.
#[test]
fn the_leak_net_of_the_deferred_enters_gate_gives_the_deferral_up() {
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().unwrap();
    assert!(
        !handoff.give_up_deferred_gate(gate.wrapping_add(1)),
        "a net for another gate changes nothing"
    );
    assert!(matches!(
        handoff.state(),
        HandoffState::HoldAfterEnter { .. }
    ));
    assert_eq!(
        handoff.on_given_up_user_bytes(b"ls\r"),
        UserBytesRoute::Held,
        "while the Enter is deferred a key is held behind its gate, a given-up key too"
    );
    assert_eq!(handoff.take_deferred_key_started(), Some(gate));

    assert!(handoff.give_up_deferred_gate(gate));
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert_eq!(handoff.deferred_gate(), None);
    assert_eq!(
        handoff.on_given_up_user_bytes(b"ls\r"),
        UserBytesRoute::Write,
        "once the deferral is given up the keys are written, not held again"
    );
    assert_eq!(handoff.take_deferred_key_started(), None);
    assert!(!handoff.give_up_deferred_gate(gate), "over already");
}

/// Owner decision 2026-11-09 (reversing #1661): the keys a hold gave up on are written to the pty
/// and counted like any user write, but they never start a young Enter's hold: a typed Enter in
/// the window is held for its prompt, the same Enter given up by a net is written (holding it
/// again would arm another net for the same keys).
///
/// Test faults corrected after its first run (it was written without a compiler; the rule it
/// asserts is unchanged): both arms now type `frob` first, because only an Enter on a line with
/// content is ever a young hold (`should_hold_young_enter`), and the count of Enters is compared
/// with the count before the give-up, which already holds the command's own Enter (the literal 2
/// forgot it).
#[test]
fn keys_a_hold_gave_up_on_never_start_another_young_hold() {
    let enters_written = |handoff: &TypeaheadHandoff| match handoff.state() {
        HandoffState::PtyOwned { enters_written, .. } => *enters_written,
        other => panic!("the pty owns input: {other:?}"),
    };

    let mut typed = young_after_submit();
    type_and_flush(&mut typed, b"frob");
    assert_eq!(
        typed.on_user_bytes(b"\r"),
        UserBytesRoute::Held,
        "a typed Enter in the window is held"
    );
    assert!(typed.take_young_hold_started().is_some());

    let mut given_up = young_after_submit();
    type_and_flush(&mut given_up, b"frob");
    let enters_before = enters_written(&given_up);
    assert_eq!(
        given_up.on_given_up_user_bytes(b"\rls\r"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(given_up.take_young_hold_started(), None);
    assert!(!given_up.young_hold_queued());
    assert_eq!(
        enters_written(&given_up),
        enters_before + 2,
        "both Enters are counted as the lines they are: {:?}",
        given_up.state()
    );
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

/// #1793: the editor's command `command` written by `PtyController` (which names its text).
fn write_named_command(handoff: &mut TypeaheadHandoff, command: &str) {
    handoff.on_command_written();
    handoff.note_written_command_text(command);
}

/// #1793: a Preexec carrying `command` applied by the model (the text first, as
/// `TerminalModel::preexec` does).
fn apply_preexec(handoff: &mut TypeaheadHandoff, command: &str, shell_started: bool) {
    handoff.note_preexec_command(command);
    let _ = handoff.on_preexec(shell_started);
}

/// #1793 fixture: a command written behind `requests_before` legacy requests, of which
/// `answered` were reported (delivered in stream order and handled by the view) before the
/// command's own Preexec; then its prompt, which hands input back to the editor.
fn command_run_behind_requests(requests_before: u32, answered: u32) -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    for _ in 0..requests_before {
        handoff.on_legacy_request_written();
    }
    for _ in 0..answered {
        handoff.on_report_parsed();
        assert_eq!(handoff.on_report(), ReportDisposition::InsertLegacy);
    }
    write_named_command(&mut handoff, "ls -l");
    apply_preexec(&mut handoff, "ls -l", false);
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    handoff
}

/// #1793 (owner repro, a warpified ssh remote whose bash reports its line): a request counted but
/// never answered (dropped unwritten, or read by something other than the line editor) left
/// `requests_outstanding` above zero for good, so every later Enter that met untracked bytes
/// waited for a report no request would send, and the 3 s give-up dropped it with the keys typed
/// behind it. The command's own Preexec proves every request written before it was read: the
/// unanswered one is dropped from the count, and an Enter is not deferred.
#[test]
fn a_request_never_answered_is_settled_by_the_next_commands_own_preexec() {
    let mut handoff = command_run_behind_requests(1, 0);
    assert_eq!(handoff.requests_outstanding(), 0);
    handoff.note_untracked_line_bytes();
    assert!(
        !handoff.defer_enter(),
        "no request is on its way: the Enter must not wait for a report"
    );
    assert_eq!(handoff.take_uncovered_deferral_started(), None);
}

/// #1793: requests the shell answered are not settled twice: with every report in, nothing is
/// eaten, and a report delivered but not yet handled by the view still counts as answered (its
/// own `on_report` takes it off the count).
#[test]
fn answered_requests_are_not_settled_again_at_the_preexec() {
    let handoff = command_run_behind_requests(2, 2);
    assert_eq!(handoff.requests_outstanding(), 0);

    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    // Delivered before the command's Preexec, handled by the view only after it.
    handoff.on_report_parsed();
    write_named_command(&mut handoff, "ls");
    apply_preexec(&mut handoff, "ls", false);
    assert_eq!(
        handoff.requests_outstanding(),
        1,
        "the report is on its way to the view"
    );
    assert_eq!(handoff.on_report(), ReportDisposition::Stash);
    assert_eq!(handoff.requests_outstanding(), 0);
}

/// #1793 (c): a request written after the command (the prompt's own legacy request) is still on
/// its way, and an Enter that meets untracked bytes behind it waits for it exactly as before
/// (#1104): only requests written before a command whose own Preexec proved them read are
/// settled.
#[test]
fn a_request_written_after_the_command_still_defers_the_enter() {
    let mut handoff = command_run_behind_requests(1, 0);
    handoff.on_legacy_request_written();
    handoff.note_untracked_line_bytes();
    assert!(handoff.defer_enter(), "the prompt's request is on its way");
    assert!(handoff.take_uncovered_deferral_started().is_some());
    handoff.on_report_parsed();
    assert_eq!(handoff.on_report(), ReportDisposition::InsertKeepDeferred);
}

/// #1793: only the written command's own Preexec, told by its text, settles requests: not a line
/// the shell started itself, not a line queued ahead of the command (another text, #1104/#933),
/// not a command whose Preexec never came (an empty line, whose Precmd follows directly).
#[test]
fn only_the_commands_own_preexec_settles_requests() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    write_named_command(&mut handoff, "ls");
    apply_preexec(&mut handoff, "ls", true);
    assert_eq!(handoff.requests_outstanding(), 1, "a shell-started line");

    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    write_named_command(&mut handoff, "ls");
    apply_preexec(&mut handoff, "make", false);
    assert_eq!(handoff.requests_outstanding(), 1, "a line queued ahead");
    handoff.on_precmd();
    apply_preexec(&mut handoff, "ls", false);
    assert_eq!(
        handoff.requests_outstanding(),
        1,
        "the queued line's prompt dropped the snapshot"
    );

    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    write_named_command(&mut handoff, "ls");
    handoff.on_precmd();
    apply_preexec(&mut handoff, "ls", false);
    assert_eq!(
        handoff.requests_outstanding(),
        1,
        "no Preexec before the prompt"
    );

    // No text named (a write path that does not name it): nothing is settled.
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    handoff.on_command_written();
    apply_preexec(&mut handoff, "ls", false);
    assert_eq!(handoff.requests_outstanding(), 1, "no written text");

    // The PowerShell insert chord (#1581): the text went on the pipe, not the tty. A later
    // command's text does not take its place.
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    write_named_command(&mut handoff, "ls");
    handoff.forget_written_command_text();
    write_named_command(&mut handoff, "ls");
    apply_preexec(&mut handoff, "ls", false);
    assert_eq!(handoff.requests_outstanding(), 1, "the file-chord path");
}

/// #1793 (review finding 4): a command written while lines the user typed ahead are still queued
/// in the tty in front of it settles nothing. Typeahead `make\r`, then a request, then the
/// editor's `make`: the next Preexec is the typeahead line's and carries the same text, but the
/// request was written after that line and is not proven read by it. Nor does the command's own
/// Preexec settle it later: only a report (or a later command written with nothing queued ahead)
/// does.
#[test]
fn a_command_written_behind_lines_queued_ahead_settles_nothing() {
    let mut handoff = TypeaheadHandoff::default();
    write_named_command(&mut handoff, "sleep 1");
    // Not a young command: the Enter typed into it is written, not held (#933).
    handoff.age_last_activity_for_test();
    assert_eq!(
        type_and_flush(&mut handoff, b"make\r"),
        UserBytesRoute::WriteCounted
    );
    apply_preexec(&mut handoff, "sleep 1", false);
    handoff.on_precmd();
    assert!(
        matches!(
            handoff.state(),
            HandoffState::PtyOwned {
                enters_written: 2,
                precmds_seen: 1,
                ..
            }
        ),
        "`make` is still queued in the tty: {:?}",
        handoff.state()
    );
    handoff.on_legacy_request_written();
    write_named_command(&mut handoff, "make");
    // The typeahead line's Preexec, with the very same text.
    apply_preexec(&mut handoff, "make", false);
    assert_eq!(
        handoff.requests_outstanding(),
        1,
        "the typeahead line's Preexec settles nothing"
    );
    handoff.on_precmd();
    // The editor's command's own Preexec.
    apply_preexec(&mut handoff, "make", false);
    assert_eq!(
        handoff.requests_outstanding(),
        1,
        "a command written behind queued lines settles nothing"
    );
    let _ = handoff.on_report();
    assert_eq!(
        handoff.requests_outstanding(),
        0,
        "its report still answers it"
    );
}

/// #1793 (review finding 5): typeahead matched from echo (`TypeaheadMode::InputMatching`) is handed
/// to the view as the same event as a report, but it answers no request and the pty reader does not
/// count it as delivered: it takes nothing off `requests_outstanding` and is not counted as handled,
/// so the reports still on their way to the view are counted right at the next settle.
#[test]
fn input_matched_typeahead_neither_answers_nor_counts_as_a_handled_report() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_legacy_request_written();
    // A report delivered (counted by the pty reader), not yet handled by the view.
    handoff.on_report_parsed();
    assert_eq!(
        handoff.on_input_matched_typeahead(),
        ReportDisposition::InsertLegacy
    );
    assert_eq!(handoff.requests_outstanding(), 1, "it answered nothing");
    assert_eq!(
        handoff.reports_handled_total, 0,
        "it was not a handled report"
    );
    write_named_command(&mut handoff, "ls");
    apply_preexec(&mut handoff, "ls", false);
    assert_eq!(
        handoff.requests_outstanding(),
        1,
        "the delivered report is still on its way: nothing is settled"
    );
    assert_eq!(handoff.rtt_pending.len(), 1, "its RTT clock is kept");
    let _ = handoff.on_report();
    assert_eq!(handoff.requests_outstanding(), 0);
    assert_eq!(handoff.reports_handled_total, handoff.reports_parsed_total);
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

/// Owner rule (2026-11-08, a timer never decides): the wait of a young hold is no function of the
/// measured round trip or of a window any more. Events release the hold; this is only the net.
#[test]
fn the_young_hold_wait_is_the_event_net_whatever_the_round_trip() {
    let now = Instant::now();
    let mut handoff = young_after_submit();
    assert_eq!(handoff.young_hold_wait(now), EVENT_HOLD_GIVE_UP);
    for _ in 0..8 {
        handoff.record_rtt_for_test(Duration::from_millis(5));
    }
    assert_eq!(handoff.young_hold_wait(now), EVENT_HOLD_GIVE_UP);
    handoff.record_rtt_for_test(Duration::from_secs(30));
    assert_eq!(handoff.young_hold_wait(now), EVENT_HOLD_GIVE_UP);
}

/// The samples stay a diagnostic (logged); a stalled one ages out of the rolling window.
#[test]
fn one_stalled_sample_ages_out_of_the_round_trip_window() {
    let mut handoff = young_after_submit();
    for _ in 0..9 {
        handoff.record_rtt_for_test(Duration::from_millis(5));
    }
    handoff.record_rtt_for_test(Duration::from_secs(30));
    for _ in 0..16 {
        handoff.record_rtt_for_test(Duration::from_millis(5));
    }
    assert_eq!(handoff.rtt_samples.len(), 16);
    assert_eq!(handoff.rtt_p90(), Some(Duration::from_millis(5)));
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
/// pty pipeline, sampled for diagnostics from the session's first command on. A line the shell
/// started itself is not the command's preexec, and the command's own sample is dropped with it.
#[test]
fn a_command_write_to_preexec_round_trip_is_sampled_before_any_report() {
    let mut handoff = young_after_submit();
    assert!(!handoff.on_preexec(false));
    assert_eq!(handoff.rtt_samples.len(), 1);

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
    handoff.set_foreground_probe(
        ForegroundProbe::new(Arc::new(|| false)).with_stream_drained(Arc::new(|| false)),
    );
    type_and_flush(&mut handoff, b"frob");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert!(handoff.take_young_hold_started().is_some());
}

/// No window: how long ago the last activity was changes nothing. What decides an Enter typed
/// while a line is unanswered, where nothing samples the shell, is the stream barrier at the
/// moment of the Enter (owner rulings 2026-11-08/09): unparsed output holds it until the barrier's
/// publish; a fully applied stream delivers it (the command is running); no barrier at all (ConPTY
/// whose peek failed) is a plain terminal and delivers it.
#[test]
fn an_enter_is_held_by_the_stream_barrier_not_by_the_age_of_the_last_activity() {
    let now = Instant::now();
    for age in [0u64, 100, 500, 5000] {
        let mut undrained = young_after_submit();
        undrained.last_activity_at = Some(now - Duration::from_millis(age));
        type_and_flush(&mut undrained, b"frob");
        assert_eq!(
            undrained.on_user_bytes(b"\r"),
            UserBytesRoute::Held,
            "{age}"
        );
        assert_eq!(
            undrained.young_hold_evidence(),
            Some(YoungHoldEvidence::PrecmdInFlight),
            "{age}"
        );

        let mut drained = young_after_submit();
        drained.age_last_activity_for_test();
        drained.last_activity_at = Some(now - Duration::from_millis(age));
        type_and_flush(&mut drained, b"frob");
        assert_eq!(
            drained.on_user_bytes(b"\r"),
            UserBytesRoute::WriteCounted,
            "{age}"
        );
        assert!(!drained.has_young_hold(), "{age}");
    }
    // No barrier (the ConPTY peek failed, or none was installed): a plain terminal.
    let mut none = TypeaheadHandoff::default();
    none.on_command_written();
    none.set_foreground_probe(ForegroundProbe::default());
    type_and_flush(&mut none, b"frob");
    assert_eq!(none.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    // A barrier whose look failed answers `None`, the same.
    let mut unknown = TypeaheadHandoff::default();
    unknown.on_command_written();
    unknown.set_foreground_probe(
        ForegroundProbe::default().with_stream_drained_or_unknown(Arc::new(|| None)),
    );
    type_and_flush(&mut unknown, b"frob");
    assert_eq!(unknown.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    // The barrier's publish decides a held Enter: output applied with no Precmd in it makes the
    // Enter typeahead for the running command.
    let mut held = young_after_submit();
    type_and_flush(&mut held, b"frob");
    assert_eq!(held.on_user_bytes(b"\r"), UserBytesRoute::Held);
    held.foreground_probe.stream_drained = Some(Arc::new(|| true));
    assert_eq!(held.on_stream_applied(), StreamApplied::HoldEnded);
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
    // Owner rule (a timer never decides): no pause releases the request. The line is a STATE
    // (its keys are in the shell's line without an Enter); only a boundary event decides again --
    // here the line's Enter and its Precmd.
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Wait { .. }
    ));
    handoff.age_last_activity_for_test();
    type_and_flush(&mut handoff, b"\r");
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
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
/// The guard is a state fact now (no clock): a line in progress, or a live young hold, counts.
#[test]
fn conpty_midline_guard_counts_held_keys() {
    let mut handoff = conpty_young_after_submit();
    assert!(!handoff.typing_a_line_on_conpty());
    type_and_flush(&mut handoff, b"abc");
    assert!(handoff.typing_a_line_on_conpty());
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert!(handoff.young_hold_queued());
    assert!(handoff.typing_a_line_on_conpty());
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
    assert_eq!(handoff.rtt_samples.len(), 8);
    handoff.on_new_session();
    assert_eq!(handoff.rtt_samples.len(), 0);
    assert_eq!(handoff.young_hold_wait(now), EVENT_HOLD_GIVE_UP);
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

/// #1923: on a ConPTY session whose shell signals, the prompt's Precmd does not hand input to the
/// editor while the line editor's ready signal is awaited (a precmd function may be reading a
/// secret); keys typed meanwhile are held for the pty, and the signal ends the wait.
#[test]
fn the_precmd_does_not_commit_to_the_editor_before_the_line_editor_signals() {
    let mut handoff = conpty_awaiting_line_editor();
    assert!(handoff.awaiting_line_editor_signal());
    assert!(
        !handoff.try_commit_at_prompt(),
        "no commit while the signal is awaited"
    );
    assert!(handoff.is_pty_owned());
    // A key typed now is held for the pty, not routed to the editor.
    assert_eq!(handoff.on_user_bytes(b"p"), UserBytesRoute::Held);
    assert!(handoff.is_pty_owned());
}

#[test]
fn the_signal_ends_the_wait_that_deferred_the_precmd_commit() {
    let mut handoff = conpty_awaiting_line_editor();
    assert!(!handoff.try_commit_at_prompt());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(!handoff.awaiting_line_editor_signal());
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
    // #1697: it takes CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF unsignalled misses in a row.
    for miss in 0..CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF {
        if miss > 0 {
            handoff.on_precmd();
        }
        miss_an_unsignalled_prompt(&mut handoff);
    }
    // The next prompt is not delayed again.
    handoff.on_precmd();
    assert!(!handoff.awaiting_line_editor_signal());
    assert_eq!(handoff.on_user_bytes(b"b"), UserBytesRoute::WriteCounted);
    // A new shell (ssh) may signal.
    handoff.on_new_session();
    handoff.on_precmd();
    assert_eq!(handoff.on_user_bytes(b"c"), UserBytesRoute::Held);
}

/// #1697: one held prompt of a shell that has not signalled yet gives up with no signal: its key
/// is held, the hold expires and the keys are released, and the next prompt is awaited again (the
/// caller's precmd). Returns the hold's id.
fn miss_an_unsignalled_prompt(handoff: &mut TypeaheadHandoff) -> u64 {
    assert!(handoff.awaiting_line_editor_signal(), "a precmd's wait");
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(handoff.expire_queued_prompt_hold(id));
    assert!(handoff.route_released_chunk(b"a"));
    handoff.finish_queued_prompt_hold(id);
    id
}

/// #1697: ONE slow prompt (the session's first, on a loaded machine) before the shell has ever
/// signalled does not switch the hold off for the session; the next prompt is held for again.
/// Only CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF unsignalled misses in a row do (#1349's reason).
/// Old behaviour: the first miss switched it off, and the #1162 raw-write corruption came back for
/// the rest of the session.
#[test]
fn the_first_unsignalled_conpty_miss_does_not_switch_the_hold_off() {
    let mut handoff = conpty_awaiting_line_editor();
    for miss in 1..CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF {
        miss_an_unsignalled_prompt(&mut handoff);
        assert!(
            !handoff.prompt_hold_off_for_session(),
            "miss {miss} switched the hold off"
        );
        handoff.on_precmd();
        assert!(
            handoff.awaiting_line_editor_signal(),
            "after miss {miss} the next prompt is held for again"
        );
    }
    miss_an_unsignalled_prompt(&mut handoff);
    assert!(handoff.prompt_hold_off_for_session());
}

/// #1697: a ready signal shows the shell signals: the misses before it do not count towards
/// switching the hold off, and later misses are one-prompt give-ups.
#[test]
fn a_signal_resets_the_unsignalled_miss_count() {
    let mut handoff = conpty_awaiting_line_editor();
    for _ in 1..CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF {
        miss_an_unsignalled_prompt(&mut handoff);
        handoff.on_precmd();
    }
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(handoff.prompt_hold_unsignalled_misses, 0);
    for _ in 0..CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF {
        handoff.on_precmd();
        miss_an_unsignalled_prompt(&mut handoff);
    }
    assert!(!handoff.prompt_hold_off_for_session());
}

/// #1697: one hold counts as one miss, however often its cap fires while it is kept overdue and
/// whether it then expires.
#[test]
fn one_overdue_hold_counts_as_one_unsignalled_miss() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    for _ in 0..CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF + 1 {
        assert!(handoff.keep_overdue_prompt_hold(id));
    }
    assert!(handoff.expire_queued_prompt_hold(id));
    assert_eq!(handoff.prompt_hold_unsignalled_misses, 1);
    assert!(!handoff.prompt_hold_off_for_session());
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
/// deferred behind a busy queue) is not a give-up: it is checked again shortly. #1698: that holds
/// past twice the cap too (a busy queue on a loaded machine); the release is waited for up to the
/// 10 s net from its decision. Old behaviour: a give-up at twice the cap from the first key.
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
    let decided = handoff
        .prompt_hold_release_decided_at
        .expect("the release was decided");
    for at in [
        started + CONPTY_PROMPT_HOLD_GIVE_UP * 2,
        decided + Duration::from_secs(9),
    ] {
        assert_eq!(
            handoff.prompt_hold_cap_fired(id, at),
            PromptHoldCap::NotYet(PROMPT_HOLD_RELEASE_IN_FLIGHT_RECHECK),
            "a release in flight is not given up {:?} in",
            at.saturating_duration_since(started)
        );
    }
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, decided + YOUNG_HOLD_GIVE_UP),
        PromptHoldCap::GiveUp,
        "bounded by the 10 s net"
    );
    assert!(handoff.route_released_chunk(b"a"));
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::Gone
    );
}

/// #1698: a hold left queued with neither wait pending and NO release decided (a wait an event
/// cleared without releasing it) keeps the old bound, twice the cap from its first key -- unless
/// the hand-off's own writes are still in flight (the queue ahead of it is busy), which extends it
/// to the 10 s net.
#[test]
fn a_hold_with_no_wait_and_no_release_keeps_its_bound_unless_writes_are_in_flight() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    // The wait ended with no release (no event returned the hold).
    handoff.awaiting_line_editor_since = None;
    handoff.enter_window_since = None;
    // Nothing of the hand-off's own is on its way to the pty.
    handoff.writes_in_flight = 0;
    assert_eq!(handoff.prompt_hold_release_decided_at, None);
    let started = handoff.prompt_hold_started_at.expect("a hold started");
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, started + CONPTY_PROMPT_HOLD_GIVE_UP),
        PromptHoldCap::NotYet(PROMPT_HOLD_RELEASE_IN_FLIGHT_RECHECK)
    );
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, started + CONPTY_PROMPT_HOLD_GIVE_UP * 2),
        PromptHoldCap::GiveUp
    );
    handoff.writes_in_flight = 1;
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, started + CONPTY_PROMPT_HOLD_GIVE_UP * 2),
        PromptHoldCap::NotYet(PROMPT_HOLD_RELEASE_IN_FLIGHT_RECHECK),
        "writes in flight: the queue is busy"
    );
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, started + YOUNG_HOLD_GIVE_UP),
        PromptHoldCap::GiveUp,
        "bounded by the 10 s net"
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

/// The give-up cap: the hold's id guards it, it fires once, and (after the session's earlier
/// unsignalled misses, #1697) the hold is off for the session.
#[test]
fn the_queued_hold_give_up_is_id_guarded_and_switches_the_hold_off() {
    let mut handoff = conpty_awaiting_line_editor();
    for _ in 1..CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF {
        miss_an_unsignalled_prompt(&mut handoff);
        handoff.on_precmd();
    }
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

/// Keys queued behind a prompt hold are a line in progress: the hold is a state flag (no stamp,
/// no clock), and the guard sees it. Not on the prompt-sequence path.
#[test]
fn the_midline_guard_sees_keys_held_behind_a_prompt_hold() {
    let mut handoff = conpty_young_after_submit();
    handoff.age_last_activity_for_test();
    assert!(!handoff.typing_a_line_on_conpty());
    handoff.prompt_hold_queued = true;
    assert!(handoff.typing_a_line_on_conpty());
    handoff.set_prompt_seq_enabled(true);
    handoff.on_precmd_seq(Some(7), Some(1));
    assert!(
        !handoff.typing_a_line_on_conpty(),
        "the sequence path decides at queue boundaries"
    );
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
    handoff.stream_undrained_for_test();
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

    // Nothing sampled at all (ConPTY): the stream barrier decides, not a window. Unparsed output
    // holds the Enter for the barrier's publish.
    let mut unsampled = seq_unsampled_after_submit();
    type_and_flush(&mut unsampled, b"explain");
    assert_eq!(unsampled.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        unsampled.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert_eq!(
        unsampled.young_hold_give_up(Instant::now()),
        YOUNG_HOLD_GIVE_UP
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
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert_eq!(
        handoff.young_hold_give_up(Instant::now()),
        YOUNG_HOLD_GIVE_UP
    );

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
        assert_eq!(handoff.take_young_hold_ends(), vec![(id, flushed())]);
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
    assert_eq!(handoff.take_young_hold_ends(), vec![(first, flushed())]);
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
    // An uncounted command's kill prefix ends the line the Enter belongs to: the hold is over.
    // Its keys are NOT void (#2198; this asserted `Dropped` until r29, and the view discarded
    // them): they follow the command to the pty in typed order.
    let (mut handoff, id) = queued_young_hold();
    handoff.on_uncounted_command_written();
    assert!(!handoff.has_young_hold());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::FollowCommand)]
    );

    // A state that cannot own a hold ends it before the next key is queued behind it. #2209:
    // its keys are NOT void (this asserted `Dropped`, and the view discarded them): they go to
    // whatever reads the pty, in typed order.
    let (mut handoff, id) = queued_young_hold();
    handoff.state = HandoffState::EditorOwned;
    assert!(!handoff.young_hold_end_pending());
    assert!(handoff.check_queued_young_hold());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );
    // ... and while the hold is fine, the check leaves it alone.
    let (mut handoff, _) = queued_young_hold();
    assert!(!handoff.check_queued_young_hold());
    assert!(handoff.young_hold_queued());

    // The pane's pty dying ends it, and the next hold never reuses its gate's key. #2209: its
    // keys are NOT void (this asserted `Dropped`): the view is told to deliver them, and that
    // nothing reads the pty any more, so they become text in the input.
    let (mut handoff, id) = queued_young_hold();
    assert!(!handoff.session_gone());
    handoff.reset_on_exit();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );
    assert!(handoff.session_gone());
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
    assert_eq!(handoff.take_young_hold_ends(), vec![(id, flushed())]);
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

/// #2209 (path 2; this asserted `Dropped`): a command written while the hold waits behind its
/// request (a submit that was not a queued key) cannot take the hold with it: the head of the
/// held Enter's line is in the shell's line, where the command's kill prefix wipes it. The Enter
/// and the keys behind it are kept WITH their line as text in the input, never dropped and
/// never written behind the command.
#[test]
fn a_command_written_in_a_state_that_cannot_own_the_hold_keeps_its_keys_with_the_line() {
    let (mut handoff, id) = queued_young_hold();
    handoff.state = HandoffState::AwaitingReport;
    handoff.on_command_written();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::WithLine)]
    );
    // Nothing follows the command, so nothing waits for the view to write it.
    assert!(handoff.young_hold_follow_pending.is_none());
    assert!(!handoff.has_young_hold());
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
    assert_eq!(handoff.take_young_hold_ends(), vec![(gate, flushed())]);

    // The pane's pty dying ends the deferral too, and the next gate never reuses an id.
    // #2209: the keys behind the deferred Enter are NOT void (this asserted `Dropped`): they
    // stay with the Enter's line, which is in the editor, unrun.
    let mut handoff = awaiting_report();
    assert!(handoff.defer_enter());
    let first = handoff.deferred_gate().unwrap();
    handoff.reset_on_exit();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(first, YoungHoldEnd::WithLine)]
    );
    assert!(handoff.session_gone());
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
///
/// Expectation changed by the owner decision of 2026-11-09 (#1923), not weakened: this session's
/// shell signals its line editor, so the Precmd alone no longer hands input to the editor (a precmd
/// function may be reading a secret); the commit is back at the line editor's signal. Until #1923
/// this test expected the commit at the Precmd.
#[test]
fn the_prompt_commits_to_the_editor_when_no_keys_are_held() {
    let mut handoff = conpty_command_running_in_the_enter_window();
    handoff.on_precmd();
    assert!(
        !handoff.try_commit_at_prompt(),
        "#1923: no commit while the line editor's signal is awaited"
    );
    assert!(handoff.is_pty_owned());
    // The signal, with no keys held: nothing to release, and the wait is over.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
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

/// #1883: Git Bash's `PROMPT_COMMAND` runs `read -s` after the prompt's precmd. The shell has
/// signalled at earlier prompts and is silent now, so the typed secret must NOT go back to the
/// editor as visible text at the give-up: the hold is kept for the prompt's signal and the keys go
/// to the pty in typed order when it comes. A shell that has never signalled still hands back.
#[test]
fn a_hold_in_a_shell_that_has_signalled_is_kept_never_handed_to_the_editor() {
    let mut handoff = conpty_awaiting_line_editor();
    handoff.line_editor_signal_seen = true;
    assert_eq!(handoff.on_user_bytes(b"p"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    assert!(
        !handoff.prompt_hold_can_hand_back(id),
        "a secret read must not become editor text"
    );
    assert!(!handoff.prompt_hold_shell_never_signalled(id));
    assert!(handoff.keep_overdue_prompt_hold(id));
    assert_eq!(
        handoff.release_queued_prompt_hold_on_signal(),
        Some(id),
        "the prompt's own signal releases the kept keys"
    );
    assert!(handoff.route_released_chunk(b"p"));
    handoff.finish_queued_prompt_hold(id);

    let mut never = conpty_awaiting_line_editor();
    assert_eq!(never.on_user_bytes(b"a"), UserBytesRoute::Held);
    let id = never.take_prompt_hold_started().unwrap();
    assert!(never.prompt_hold_can_hand_back(id));
}

// ---- #2038: a reader ahead of the line editor (round 11 Windows, `gitbash_precmd_secret`) -------

/// The hand-off of the round 11 Windows log at `hop=precmd_seen ... enters=1 precmds=0` after
/// `touch gbgo.txt`: Git Bash on ConPTY with prompt sequence numbers, a shell whose line editor
/// signalled at the prompt before, the command written by the app, its Preexec and its Precmd
/// parsed, the stream applied, and no signal yet for this prompt (a precmd function's
/// `read -s -p "pin: "` is reading).
fn conpty_prompt_after_a_command_in_a_shell_that_signalled() -> TypeaheadHandoff {
    let mut handoff = conpty_awaiting_line_editor();
    handoff.set_conpty_enter_window_hold(true);
    handoff.set_prompt_seq_enabled(true);
    handoff.on_precmd_seq(Some(7), Some(1));
    // The first prompt: its line editor signals and the editor takes input.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
    // `touch gbgo.txt` from the editor, its Preexec, its Precmd.
    handoff.on_command_written();
    handoff.on_preexec(false);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(2));
    handoff.age_last_activity_for_test();
    assert!(handoff.is_pty_owned());
    assert!(handoff.awaiting_line_editor_signal());
    handoff
}

/// The control: what the same keys did BEFORE the reader was told to the hand-off (#2038, read
/// from the code after the round 11 run). The released half of the PIN is the shell's line, so
/// the Enter that ends the `read` is a young Enter held for a line report that cannot be asked
/// for (10 s net), and once written it is a line no prompt answers.
#[test]
fn without_the_reader_the_pin_is_the_shells_line_and_its_enter_is_held_for_a_report() {
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert_eq!(handoff.on_user_bytes(b"p"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a prompt hold");
    // The cap releases the keys to the pty with nothing said about a reader.
    assert!(handoff.expire_queued_prompt_hold(id));
    assert!(handoff.route_released_chunk(b"p"));
    handoff.finish_queued_prompt_hold(id);
    assert!(
        handoff.shell_line_may_hold_typed_text(),
        "the PIN is counted as the shell's line"
    );
    assert_eq!(handoff.on_user_bytes(b"i"), UserBytesRoute::WriteCounted);
    assert_eq!(
        handoff.on_user_bytes(b"\r"),
        UserBytesRoute::Held,
        "the read's Enter is held as a young Enter"
    );
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
}

/// #2038, the real sequence of `gitbash_precmd_secret`: the first half of the PIN is typed inside
/// the prompt hold's cap and held; at the cap the screen asks for input, the view tells the
/// hand-off and releases the hold; the second half and the Enter are typed later. Every key goes
/// to the pty when it is typed (or released), in typed order; none is the shell's line, the Enter
/// is not a line a prompt must answer and opens no enter window. The line editor's signal for the
/// same prompt then hands input to the editor, and the next prompt holds again.
#[test]
fn a_reader_ahead_of_the_line_editor_gets_every_key_and_the_signal_hands_input_to_the_editor() {
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    // The first key starts the hold; the view's queue takes `i` and `n` behind it.
    assert_eq!(handoff.on_user_bytes(b"p"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a prompt hold");
    assert!(
        !handoff.prompt_reader_look_due(),
        "with a hold queued the hold's cap looks at the screen, not the key"
    );
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::GiveUp
    );
    assert!(
        !handoff.prompt_hold_can_hand_back(id),
        "#1883: never editor text in a shell that signalled"
    );
    // The view: `screen_tail_asks_for_input()` is true, so the reader is noted, then the release.
    assert!(handoff.note_prompt_reader_asks());
    assert!(handoff.expire_queued_prompt_hold(id));
    for key in [&b"p"[..], b"i", b"n"] {
        assert!(handoff.route_released_chunk(key), "released in typed order");
    }
    handoff.finish_queued_prompt_hold(id);
    assert_eq!(handoff.writes_in_flight(), 3);
    assert!(
        !handoff.shell_line_may_hold_typed_text(),
        "the reader's keys are not the shell's line"
    );
    assert!(
        !handoff.try_commit_at_prompt(),
        "the editor does not take input while the reader has the tty"
    );
    assert!(matches!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Wait { .. }
    ));
    // The second half, 2.7 s later, and the Enter that ends the `read`.
    for key in [&b"4"[..], b"2", b"\r"] {
        assert_eq!(
            handoff.peek_user_bytes_route(key, true, Instant::now()),
            UserBytesRoute::WriteCounted
        );
        assert_eq!(
            handoff.on_user_bytes(key),
            UserBytesRoute::WriteCounted,
            "{key:?} goes to the reader at once"
        );
    }
    assert!(!handoff.has_prompt_hold());
    assert!(!handoff.young_hold_queued(), "no young Enter");
    assert!(
        !handoff.has_unanswered_lines(),
        "the read's Enter is not a line a prompt must answer"
    );
    // The `read` returned: the precmd stage ends and the line editor signals (the marker and
    // the bracketed-paste enable both take this path; the second finds nothing to do).
    assert!(handoff.on_line_editor_reading(Some(7), Some(2)));
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(!handoff.prompt_reader_asks(), "the signal ends the reader");
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed,
        "an empty line at a signalled prompt: the editor takes input, nothing is requested"
    );
    assert!(handoff.editor_owns_input());
    // The next command and its prompt: held for its line editor as before.
    handoff.on_command_written();
    handoff.on_preexec(false);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert!(!handoff.prompt_reader_asks());
    assert_eq!(handoff.on_user_bytes(b"e"), UserBytesRoute::Held);
}

/// #2038, the PIN typed with no hold at all: the question is already on screen when the first key
/// comes (also later than the hold's cap, when no key is held any more), so the view's look at
/// that key notes the reader and the keys go straight to it. A key at a prompt with no question
/// is held for the line editor exactly as before.
#[test]
fn a_question_already_on_screen_sends_the_first_key_to_the_reader_without_a_hold() {
    for late in [false, true] {
        let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
        if late {
            handoff.awaiting_line_editor_since =
                Some(Instant::now() - CONPTY_PROMPT_HOLD_GIVE_UP - Duration::from_millis(1));
        }
        assert!(handoff.prompt_reader_look_due(), "late={late}");
        assert!(handoff.note_prompt_reader_asks(), "late={late}");
        assert!(!handoff.prompt_reader_look_due(), "one look per reader");
        for key in [&b"p"[..], b"i", b"n", b"\r"] {
            assert_eq!(
                handoff.on_user_bytes(key),
                UserBytesRoute::WriteCounted,
                "late={late} {key:?}"
            );
        }
        assert!(!handoff.has_prompt_hold(), "late={late}");
        assert!(!handoff.young_hold_queued(), "late={late}");
        assert!(!handoff.has_unanswered_lines(), "late={late}");
        assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
        assert_eq!(
            handoff.on_line_editor_active(true, true),
            ActiveDecision::Committed,
            "late={late}"
        );
    }
    // No question on screen: the view notes nothing and the key is held, as before.
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert!(handoff.prompt_reader_look_due());
    assert_eq!(handoff.on_user_bytes(b"l"), UserBytesRoute::Held);
}

/// #2038: where no reader can be ahead of the line editor, a question on screen changes nothing:
/// the editor owns input, the line editor already signalled (also when only the model thread has
/// parsed its marker so far), the session's shell does not signal, or the platform is not ConPTY.
#[test]
fn a_reader_is_noted_only_between_a_precmd_and_its_line_editors_signal() {
    let mut signalled = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert_eq!(signalled.release_queued_prompt_hold_on_signal(), None);
    assert!(!signalled.prompt_reader_look_due());
    assert!(!signalled.note_prompt_reader_asks());

    let mut marker_parsed = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert!(marker_parsed.on_line_editor_reading(Some(7), Some(2)));
    assert!(!marker_parsed.prompt_reader_look_due());
    assert!(!marker_parsed.note_prompt_reader_asks());

    let mut editor = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert_eq!(editor.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(
        editor.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
    assert!(!editor.note_prompt_reader_asks());

    let mut unix = after_submit();
    unix.on_precmd();
    assert!(!unix.prompt_reader_look_due());
    assert!(!unix.note_prompt_reader_asks());

    let mut silent_shell = young_after_submit();
    silent_shell.set_conpty_input(true);
    silent_shell.set_conpty_prompt_hold(true);
    silent_shell.on_precmd();
    assert!(!silent_shell.note_prompt_reader_asks());
}

/// #2038: keys written to the reader after its last Enter may be unread when it ends (typed
/// ahead of the prompt, or the question was not one); they are in the tty for the shell's line
/// editor. When the line editor signals they count as the shell's line, so the decision asks the
/// shell for them (a proven reader) instead of leaving them behind an editor that took over.
#[test]
fn keys_the_reader_left_unread_are_the_shells_line_once_the_line_editor_signals() {
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert!(handoff.note_prompt_reader_asks());
    for key in [&b"p"[..], b"\r", b"l", b"s"] {
        assert_eq!(handoff.on_user_bytes(key), UserBytesRoute::WriteCounted);
        handoff.on_user_write_flushed();
    }
    assert!(!handoff.shell_line_may_hold_typed_text());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(
        handoff.shell_line_may_hold_typed_text(),
        "`ls` was typed after the read's Enter: it is in the shell's line"
    );
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest,
        "the line is asked for, not left in the shell"
    );

    // A ^C to the reader ends its line: nothing is left for the shell.
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert!(handoff.note_prompt_reader_asks());
    for key in [&b"p"[..], b"\x03"] {
        assert_eq!(handoff.on_user_bytes(key), UserBytesRoute::WriteCounted);
    }
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(!handoff.shell_line_may_hold_typed_text());

    // A Preexec and a new session end the reader too.
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert!(handoff.note_prompt_reader_asks());
    handoff.on_preexec(true);
    assert!(!handoff.prompt_reader_asks());
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert!(handoff.note_prompt_reader_asks());
    handoff.on_new_session();
    assert!(!handoff.prompt_reader_asks());
}

// ---- #2047: a young Enter held while a reader ahead of the line editor has the tty -------------

/// #2047 on ConPTY (r13-pin2, race 3): this prompt's wait for the line editor is older than the
/// prompt hold's cap, and the look at the screen missed at every key of the answer (the model was
/// busy at each: #2038's key-time look takes it only if it is free). The first key is then the
/// shell's line, and the Enter that ends the `read -s` is held as a young Enter for a report
/// that cannot be asked for. Before the fix it sat for the 10 s net and was then written as a
/// line no prompt answers (the control above this section).
///
/// With a hold queued the key-time look is not due, so the HOLD says when the tty may be looked
/// at again (`young_hold_reader_look`); the view looks at a key behind the hold, at a wakeup and
/// at the hold's net. When the screen asks, the hold is given to the reader: its Enter is routed
/// as the reader's (in flight, not a line a prompt must answer, no enter window), and the line
/// editor's signal for the same prompt asks the shell for the line the first key left there.
#[test]
fn a_young_enter_that_ends_a_precmd_read_is_given_to_the_reader_the_screen_shows() {
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    handoff.age_line_editor_wait_past_the_cap_for_test();
    assert_eq!(handoff.young_hold_reader_look(), None, "no hold, no look");
    // The looks at these keys missed: the view noted no reader.
    assert_eq!(
        type_and_flush(&mut handoff, b"p"),
        UserBytesRoute::WriteCounted
    );
    assert!(handoff.shell_line_may_hold_typed_text());
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::RequestDue)
    );
    let hold = handoff
        .take_young_hold_started()
        .expect("the Enter is held as a young Enter");
    assert!(
        !handoff.prompt_reader_look_due(),
        "with a hold queued the key-time look is not due: the hold looks"
    );
    assert_eq!(
        handoff.young_hold_reader_look(),
        Some(YoungHoldReaderLook::Screen)
    );
    assert!(!handoff.tty_reads_secret(), "ConPTY samples no tty");

    // The view: the screen ends in `pin: `.
    assert_eq!(handoff.give_young_hold_to_prompt_reader(), Some(hold));
    assert!(!handoff.young_hold_queued(), "the hold is over");
    assert_eq!(handoff.young_hold_evidence(), None);
    assert!(handoff.prompt_reader_asks());
    assert_eq!(handoff.young_hold_reader_look(), None, "one look per hold");
    assert_eq!(handoff.give_young_hold_to_prompt_reader(), None);
    let in_flight = handoff.writes_in_flight();
    assert_eq!(
        handoff.route_expired_young_chunks(vec![b"\r".to_vec()]),
        vec![b"\r".to_vec()],
        "written as typed"
    );
    assert_eq!(handoff.writes_in_flight(), in_flight + 1);
    assert!(
        !handoff.has_unanswered_lines(),
        "the read's Enter is not a line a prompt must answer"
    );
    handoff.on_user_write_flushed();
    assert!(
        !handoff.try_commit_at_prompt(),
        "the editor does not take input while the reader has the tty"
    );

    // The `read` returned: the line editor signals for the same prompt. The `p` the missed look
    // left in the shell's line is asked for; nothing waits for an event that will not come.
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(!handoff.prompt_reader_asks(), "the signal ends the reader");
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
}

/// #2047 where the tty is sampled (Linux, macOS): a line typed ahead of a precmd function's
/// `read -s` and its Enter. While nothing reads a secret the Enter is an ordinary young Enter
/// (the request is due) and the look changes nothing; once the tty is canonical with echo off
/// the hold is the reader's.
#[test]
fn a_young_enter_is_given_to_a_secret_reader_where_the_tty_is_sampled() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handoff = sampled_prompt_after_a_command(&secret);
    assert_eq!(
        type_and_flush(&mut handoff, b"p"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let hold = handoff
        .take_young_hold_started()
        .expect("the Enter is held as a young Enter");
    assert_eq!(
        handoff.young_hold_reader_look(),
        Some(YoungHoldReaderLook::Tty)
    );
    assert!(
        !handoff.tty_reads_secret(),
        "nothing reads a secret: the view leaves the hold to its own events"
    );
    assert!(handoff.young_hold_queued());

    secret.store(true, Ordering::SeqCst);
    assert!(handoff.tty_reads_secret());
    assert_eq!(handoff.give_young_hold_to_prompt_reader(), Some(hold));
    assert!(handoff.prompt_reader_asks());
    assert!(handoff.secret_reader_has_the_keys());
    assert_eq!(
        handoff.route_expired_young_chunks(vec![b"\r".to_vec()]),
        vec![b"\r".to_vec()]
    );
    assert!(!handoff.has_unanswered_lines());
    assert!(!handoff.young_hold_queued());
}

/// #2047: the hold is given to a reader only in the states #2038 and #2050 allow a reader in.
/// At a prompt whose line editor has signalled (ConPTY, nothing sampled) a young Enter is a line
/// at the shell's own editor: no look is due, and its net writes it as the shell's line, counted,
/// as before. A hold given to a reader whose reader ended before the keys were routed (the
/// release waited in the view queue's mailbox) is the shell's line too.
#[test]
fn a_young_hold_is_not_a_readers_outside_the_states_a_reader_can_be_in() {
    let mut signalled = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert_eq!(signalled.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(
        type_and_flush(&mut signalled, b"l"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(signalled.on_user_bytes(b"\r"), UserBytesRoute::Held);
    let hold = signalled
        .take_young_hold_started()
        .expect("a young Enter at a signalled prompt");
    assert_eq!(signalled.young_hold_reader_look(), None);
    assert_eq!(signalled.give_young_hold_to_prompt_reader(), None);
    assert!(signalled.young_hold_queued(), "nothing changed");
    assert_eq!(
        signalled.expire_queued_young_hold(hold),
        YoungHoldExpiry::Release
    );
    signalled.route_expired_young_chunks(vec![b"\r".to_vec()]);
    assert!(
        signalled.has_unanswered_lines(),
        "the net's write is the shell's line, as before"
    );

    // Given to the reader, but the line editor signalled before the keys were routed.
    let mut late = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    late.age_line_editor_wait_past_the_cap_for_test();
    assert_eq!(
        type_and_flush(&mut late, b"p"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(late.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert!(late.take_young_hold_started().is_some());
    assert!(late.give_young_hold_to_prompt_reader().is_some());
    assert_eq!(late.release_queued_prompt_hold_on_signal(), None);
    assert!(!late.prompt_reader_asks());
    late.route_expired_young_chunks(vec![b"\r".to_vec()]);
    assert!(
        late.has_unanswered_lines(),
        "the line editor reads: the Enter submits the shell's line"
    );
}

// ---- #2050: a secret read ahead of the line editor where the tty is sampled (round 12 Linux) ---

/// The hand-off of the round 12 Linux log at `hop=precmd_seen ... enters=1 precmds=0` after
/// `touch pp_hp0_go.txt`: bash with prompt sequence numbers on a platform that samples the tty
/// (`secret` is what one `tcgetattr` would say: canonical, echo off), no prompt hold. The command
/// the editor wrote ran, its Preexec and its Precmd are parsed; the line editor's marker is not.
/// The pty still owns input: the view has not handled the Precmd yet.
fn sampled_prompt_after_a_command(secret: &Arc<std::sync::atomic::AtomicBool>) -> TypeaheadHandoff {
    let reads = secret.clone();
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    handoff.on_precmd_seq(Some(7), Some(1));
    handoff.set_foreground_probe(
        ForegroundProbe::default()
            .with_prompt_fact(Arc::new(|| ShellPromptFact::ShellCooked))
            .with_secret_read(Arc::new(move || reads.load(Ordering::SeqCst))),
    );
    handoff.on_command_written();
    handoff.on_preexec(false);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(2));
    assert!(handoff.is_pty_owned());
    assert_eq!(handoff.shell_activity(), ShellActivity::Prompt);
    handoff
}

/// As [`sampled_prompt_after_a_command`], after the view handled the Precmd: with no prompt hold
/// it hands input to the editor (`hop=handoff_commit_at_prompt committed=true` in that log).
fn sampled_prompt_the_editor_owns(secret: &Arc<std::sync::atomic::AtomicBool>) -> TypeaheadHandoff {
    let mut handoff = sampled_prompt_after_a_command(secret);
    assert!(handoff.try_commit_at_prompt());
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    handoff
}

/// #2050, the sequence of `precmd_secret_honor_ps1` on Linux: the Precmd gave input to the
/// editor, then a precmd function reads a secret. While nothing reads, the look changes nothing.
/// Once the tty is canonical with echo off the pty takes input back, and the PIN and its Enter go
/// to the reader at once, in typed order: not the shell's line, no young Enter, no line a prompt
/// must answer. The editor does not take input while the reader has the tty; the line editor's
/// signal for the same prompt hands it back with nothing requested.
#[test]
fn a_secret_read_at_a_prompt_takes_input_back_from_the_editor_and_gets_every_key() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert!(handoff.secret_reader_look_due());
    assert_eq!(
        handoff.take_input_for_secret_reader(),
        None,
        "nothing reads a secret: the editor keeps input"
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(!handoff.secret_reader_has_the_keys());

    secret.store(true, Ordering::SeqCst);
    assert_eq!(handoff.take_input_for_secret_reader(), Some(true));
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: 1,
            precmds_seen: 1,
        }
    );
    assert!(handoff.prompt_reader_asks());
    assert!(handoff.secret_reader_has_the_keys());
    assert!(
        handoff.wants_terminal_focus(),
        "focus follows to the terminal"
    );
    assert!(!handoff.secret_reader_look_due(), "one look per reader");
    assert_eq!(handoff.take_input_for_secret_reader(), None);
    for key in [&b"p"[..], b"i", b"n", b"4", b"2", b"\r"] {
        assert_eq!(
            handoff.peek_user_bytes_route(key, true, Instant::now()),
            UserBytesRoute::WriteCounted
        );
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted,
            "{key:?} goes to the reader at once"
        );
    }
    assert!(!handoff.young_hold_queued(), "no young Enter");
    assert!(!handoff.has_unanswered_lines());
    assert!(!handoff.shell_line_may_hold_typed_text());
    assert!(
        !handoff.try_commit_at_prompt(),
        "the editor does not take input while the reader has the tty"
    );
    assert!(matches!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Wait { .. }
    ));
    // The `read` returned: echo is back, the precmd stage ends, the line editor signals (the
    // marker, then the bracketed-paste enable; the second finds nothing to do).
    secret.store(false, Ordering::SeqCst);
    assert!(handoff.on_line_editor_reading(Some(7), Some(2)));
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(!handoff.prompt_reader_asks(), "the signal ends the reader");
    assert!(!handoff.secret_reader_has_the_keys());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed,
        "an empty line at a signalled prompt: the editor takes input, nothing is requested"
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(
        !handoff.secret_reader_look_due(),
        "the line editor reads: its own tty state is never asked about"
    );
}

/// #2050: the same read seen while the pty still owns input (the wakeup of the question ran
/// before the view handled the Precmd, or a typed-ahead key kept the line with the shell): the
/// reader is noted without a change of owner, and the Precmd's commit then leaves input with it.
#[test]
fn a_secret_read_seen_before_the_precmd_commits_keeps_input_with_the_pty() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut handoff = sampled_prompt_after_a_command(&secret);
    assert_eq!(handoff.take_input_for_secret_reader(), Some(false));
    assert!(handoff.is_pty_owned());
    assert!(!handoff.try_commit_at_prompt());
    for key in [&b"p"[..], b"\r"] {
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted
        );
    }
    assert!(!handoff.has_unanswered_lines());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

/// #2050: where the tty is not asked. No probe (ConPTY: #2038's screen fact stands in); the line
/// editor's marker is parsed (readline's own tty state is not a reader ahead of it); a command
/// runs (its password prompt is that command's, with its own hand-over); a reader is already
/// noted. In each the look is not due and a "secret" answer changes nothing.
#[test]
fn the_tty_is_asked_for_a_secret_read_only_between_a_precmd_and_its_line_editor() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(true));

    let mut unsampled = TypeaheadHandoff::default();
    unsampled.on_command_written();
    unsampled.on_preexec(false);
    unsampled.on_precmd();
    assert!(unsampled.try_commit_at_prompt());
    assert!(!unsampled.secret_reader_look_due());
    assert_eq!(unsampled.take_input_for_secret_reader(), None);
    assert!(!unsampled.secret_reader_has_the_keys());

    let mut reading = sampled_prompt_the_editor_owns(&secret);
    assert!(reading.on_line_editor_reading(Some(7), Some(2)));
    assert!(!reading.secret_reader_look_due());
    assert_eq!(reading.take_input_for_secret_reader(), None);
    assert_eq!(reading.state(), &HandoffState::EditorOwned);

    let mut running = sampled_prompt_the_editor_owns(&secret);
    running.on_command_written();
    running.on_preexec(false);
    assert!(!running.secret_reader_look_due());
    assert_eq!(running.take_input_for_secret_reader(), None);

    let mut noted = sampled_prompt_the_editor_owns(&secret);
    assert_eq!(noted.take_input_for_secret_reader(), Some(true));
    assert!(!noted.secret_reader_look_due());
    assert_eq!(noted.take_input_for_secret_reader(), None);
    assert!(noted.prompt_reader_asks());
}

/// #2040: the lock-free copy of "a reader is noted" (what the input-order log's secret guard
/// reads) is set where the reader is noted, from the tty or from the screen, and cleared where
/// it ends: it never differs from the hand-off's own answer.
#[test]
fn the_noted_reader_flag_follows_the_reader_for_the_logs_secret_guard() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    let noted = handoff.prompt_reader_noted_flag();
    assert!(!noted.load(Ordering::SeqCst));
    assert_eq!(handoff.take_input_for_secret_reader(), Some(true));
    assert!(handoff.prompt_reader_asks());
    assert!(noted.load(Ordering::SeqCst));
    // A PIN typed now is logged by `hand_route reason=prompt_reader bytes_len=..` only.
    assert_eq!(
        type_and_flush(&mut handoff, b"4711"),
        UserBytesRoute::WriteCounted
    );
    assert!(noted.load(Ordering::SeqCst));
    handoff.on_preexec(true);
    assert!(!handoff.prompt_reader_asks());
    assert!(!noted.load(Ordering::SeqCst));

    // The screen fact (ConPTY): noted by the view, ended by the line editor's signal.
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    let noted = handoff.prompt_reader_noted_flag();
    assert_eq!(handoff.on_user_bytes(b"p"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a prompt hold");
    assert_eq!(
        handoff.prompt_hold_cap_fired(id, Instant::now()),
        PromptHoldCap::GiveUp
    );
    assert!(!noted.load(Ordering::SeqCst));
    assert!(handoff.note_prompt_reader_asks());
    assert!(noted.load(Ordering::SeqCst));
    assert!(handoff.expire_queued_prompt_hold(id));
    assert!(handoff.route_released_chunk(b"p"));
    handoff.finish_queued_prompt_hold(id);
    let _ = handoff.on_line_editor_active(true, false);
    assert_eq!(noted.load(Ordering::SeqCst), handoff.prompt_reader_asks());
    handoff.on_preexec(true);
    assert!(!handoff.prompt_reader_asks());
    assert!(!noted.load(Ordering::SeqCst));

    // The session ends with a reader noted: the cell the pane registered is the one cleared (a
    // fresh cell would leave the log withholding text for the rest of the run).
    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    let noted = handoff.prompt_reader_noted_flag();
    secret.store(true, Ordering::SeqCst);
    assert_eq!(handoff.take_input_for_secret_reader(), Some(true));
    assert!(noted.load(Ordering::SeqCst));
    handoff.reset_on_exit();
    assert!(!handoff.prompt_reader_asks());
    assert!(!noted.load(Ordering::SeqCst));
    assert!(Arc::ptr_eq(&noted, &handoff.prompt_reader_noted_flag()));
}

/// #2050: the reader ends on the events of #2038 here too, and keys it left unread are the
/// shell's line. `ls` typed after the PIN's Enter is in the tty when the line editor starts: the
/// decision asks the shell for it. A Preexec and the next Precmd end the reader; after the next
/// Precmd the editor takes input again and the tty is asked again.
#[test]
fn a_secret_reader_ends_on_events_and_its_unread_keys_are_the_shells_line() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert_eq!(handoff.take_input_for_secret_reader(), Some(true));
    for key in [&b"p"[..], b"\r", b"l", b"s"] {
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted
        );
    }
    assert!(!handoff.shell_line_may_hold_typed_text());
    secret.store(false, Ordering::SeqCst);
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(handoff.shell_line_may_hold_typed_text());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest,
        "the line is asked for, not left in the shell"
    );

    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    secret.store(true, Ordering::SeqCst);
    assert_eq!(handoff.take_input_for_secret_reader(), Some(true));
    handoff.on_preexec(true);
    assert!(!handoff.prompt_reader_asks());
    assert!(
        handoff.is_pty_owned(),
        "the command the shell started has the pty"
    );

    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert_eq!(handoff.take_input_for_secret_reader(), Some(true));
    assert_eq!(
        type_and_flush(&mut handoff, b"\r"),
        UserBytesRoute::WriteCounted
    );
    // The read's Enter was an empty answer; the function's own `return` leads to a new prompt
    // cycle with no line-editor signal in between (an empty line runs no command).
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert!(!handoff.prompt_reader_asks());
    assert!(handoff.try_commit_at_prompt());
    assert!(handoff.secret_reader_look_due());
}

/// #2050 with #1658: the pty owns input but the keys follow unsent text into the editor. A secret
/// read ends that: the keys are the reader's, the text stays in the editor.
#[test]
fn a_secret_read_ends_keys_following_the_editors_text() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut handoff = sampled_prompt_after_a_command(&secret);
    assert!(handoff.note_keys_follow_editor_if(true));
    assert!(handoff.keys_follow_editor_active());
    assert!(!handoff.routes_keys_to_pty());
    assert_eq!(handoff.take_input_for_secret_reader(), Some(false));
    assert!(!handoff.keys_follow_editor_active());
    assert!(handoff.routes_keys_to_pty());
    assert_eq!(handoff.on_user_bytes(b"p"), UserBytesRoute::WriteCounted);
}

// ---- #2052: the shell's marker for a precmd function's `read` (the third reader source) --------

/// #2052, the gap and its fix. A precmd function's `read -p "continue? " x` after the Precmd gave
/// input to the editor: the tty stays canonical with echo on, so the tty look of #2050 notes
/// nothing (the control: the answer would be typed into the editor). The shell's `TtyReaderAsks`
/// marker for this prompt takes input back for the pty and notes the reader; the answer and its
/// Enter go to the `read` at once, in typed order: not the shell's line, no young Enter, no line a
/// prompt must answer. The editor does not take input while the reader has the tty; the line
/// editor's signal for the same prompt hands it back with nothing requested.
#[test]
fn a_precmd_reads_marker_takes_input_back_from_the_editor_and_gets_every_key() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert_eq!(
        handoff.take_input_for_secret_reader(),
        None,
        "an echoing read: the tty says nothing, the editor keeps input (the #2052 gap)"
    );
    assert!(!handoff.marker_reader_news_pending());

    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: 1,
            precmds_seen: 1,
        }
    );
    assert!(handoff.prompt_reader_asks());
    assert!(handoff.secret_reader_has_the_keys());
    assert!(
        handoff.wants_terminal_focus(),
        "focus follows to the terminal"
    );
    assert!(handoff.marker_reader_news_pending());
    assert_eq!(
        handoff.take_marker_reader_news(),
        Some(true),
        "the view is told once, and that the editor had input"
    );
    assert_eq!(handoff.take_marker_reader_news(), None);
    assert!(handoff.secret_reader_has_the_keys(), "after the look too");
    assert!(
        !handoff.on_tty_reader_asks(Some(7), Some(2)),
        "a second read of the same stage finds the reader noted"
    );
    assert!(!handoff.marker_reader_news_pending(), "and is no news");
    assert_eq!(
        handoff.take_input_for_secret_reader(),
        None,
        "one reader per prompt: the tty is not asked again"
    );
    for key in [&b"y"[..], b"e", b"s", b"\r"] {
        assert_eq!(
            handoff.peek_user_bytes_route(key, true, Instant::now()),
            UserBytesRoute::WriteCounted
        );
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted,
            "{key:?} goes to the reader at once"
        );
    }
    assert!(!handoff.young_hold_queued(), "no young Enter");
    assert!(!handoff.has_unanswered_lines());
    assert!(!handoff.shell_line_may_hold_typed_text());
    assert!(
        !handoff.try_commit_at_prompt(),
        "the editor does not take input while the reader has the tty"
    );
    assert!(matches!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Wait { .. }
    ));
    // The `read` returned, the precmd stage ends, the line editor signals.
    assert!(handoff.on_line_editor_reading(Some(7), Some(2)));
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(!handoff.prompt_reader_asks(), "the signal ends the reader");
    assert!(!handoff.secret_reader_has_the_keys());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed,
        "an empty line at a signalled prompt: the editor takes input, nothing is requested"
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(
        !handoff.on_tty_reader_asks(Some(7), Some(2)),
        "the line editor reads: a marker now is not a reader ahead of it"
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
}

/// #2052 where nothing samples the tty at all (no probe installed: the kill switch of #2050, or a
/// platform without one): the marker alone notes the reader and gives it the pane's keys.
#[test]
fn the_readers_marker_needs_no_tty_sample() {
    let mut handoff = TypeaheadHandoff::default();
    handoff.on_precmd_seq(Some(7), Some(1));
    handoff.on_command_written();
    handoff.on_preexec(false);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(2));
    assert!(handoff.try_commit_at_prompt());
    assert!(!handoff.secret_reader_look_due());
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    assert!(handoff.is_pty_owned());
    assert!(handoff.secret_reader_has_the_keys());
    assert_eq!(handoff.take_marker_reader_news(), Some(true));
    assert_eq!(
        type_and_flush(&mut handoff, b"y"),
        UserBytesRoute::WriteCounted
    );
}

/// #2052: the marker is honoured only for the prompt the app is at. A marker of an older prompt,
/// of another session, without a sequence number, after the line editor's marker, while a command
/// runs (a program printing the OSC), before any Precmd, or with the kill switch set changes
/// nothing: the editor keeps input, no reader is noted, the view is told nothing.
#[test]
fn the_readers_marker_is_honoured_only_for_the_prompt_the_app_is_at() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let unchanged = |handoff: &mut TypeaheadHandoff, what: &str| {
        assert!(!handoff.prompt_reader_asks(), "{what}");
        assert!(!handoff.secret_reader_has_the_keys(), "{what}");
        assert!(!handoff.marker_reader_news_pending(), "{what}");
        assert_eq!(handoff.take_marker_reader_news(), None, "{what}");
    };

    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    for (session, seq, what) in [
        (Some(7), Some(1), "an older prompt's marker"),
        (Some(7), Some(3), "a marker ahead of the last Precmd parsed"),
        (Some(8), Some(2), "another session's marker"),
        (None, Some(2), "a marker without a session"),
        (Some(7), None, "a marker without a sequence number"),
    ] {
        assert!(!handoff.on_tty_reader_asks(session, seq), "{what}");
        assert_eq!(handoff.state(), &HandoffState::EditorOwned, "{what}");
        unchanged(&mut handoff, what);
    }

    let mut reading = sampled_prompt_the_editor_owns(&secret);
    assert!(reading.on_line_editor_reading(Some(7), Some(2)));
    assert!(!reading.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(reading.state(), &HandoffState::EditorOwned);
    unchanged(&mut reading, "after the line editor's marker");

    let mut running = sampled_prompt_the_editor_owns(&secret);
    running.on_command_written();
    running.on_preexec(false);
    let before = running.state().clone();
    assert!(!running.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(running.state(), &before);
    unchanged(&mut running, "while a command runs");

    let mut no_prompt_yet = TypeaheadHandoff::default();
    assert!(!no_prompt_yet.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(no_prompt_yet.state(), &HandoffState::EditorOwned);
    unchanged(&mut no_prompt_yet, "before any Precmd");

    let mut off = sampled_prompt_the_editor_owns(&secret);
    off.set_reader_marker_enabled(false);
    assert!(!off.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(off.state(), &HandoffState::EditorOwned);
    unchanged(&mut off, "the kill switch");
    off.set_reader_marker_enabled(true);
    assert!(off.on_tty_reader_asks(Some(7), Some(2)), "and back on");
}

/// #2052: the one kill switch, as both sides read it: only `0` turns the marker off (the
/// bootstraps test `${PHOSPHOR_READER_MARKER:-1} != 0`).
#[test]
fn the_reader_markers_kill_switch_is_off_only_for_zero() {
    assert_eq!(READER_MARKER_ENV, "PHOSPHOR_READER_MARKER");
    assert!(reader_marker_enabled_from(None));
    assert!(reader_marker_enabled_from(Some("")));
    assert!(reader_marker_enabled_from(Some("1")));
    assert!(reader_marker_enabled_from(Some("off")));
    assert!(!reader_marker_enabled_from(Some("0")));
    assert!(!reader_marker_enabled_from(Some(" 0 ")));
}

/// #2052: the marker parsed while the pty still owns input (the view has not handled the Precmd
/// yet): the reader is noted without a change of owner, the view is told the editor did not have
/// input, and the Precmd's commit then leaves input with the reader.
#[test]
fn a_readers_marker_parsed_before_the_precmd_commits_keeps_input_with_the_pty() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handoff = sampled_prompt_after_a_command(&secret);
    let before = handoff.state().clone();
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(handoff.state(), &before);
    assert_eq!(handoff.take_marker_reader_news(), Some(false));
    assert!(!handoff.try_commit_at_prompt());
    for key in [&b"y"[..], b"\r"] {
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted
        );
    }
    assert!(!handoff.has_unanswered_lines());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
}

/// #2052: a reader the marker noted ends on the events of #2038, and keys it left unread are the
/// shell's line: `ls` typed after the answer's Enter is in the tty when the line editor starts,
/// so the decision asks the shell for it. A Preexec, the next Precmd and a new session end the
/// reader, and with it the news the view has not looked at; after the next Precmd the marker of
/// the old prompt is stale and the new prompt's is honoured.
#[test]
fn a_marker_reader_ends_on_events_and_its_unread_keys_are_the_shells_line() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    for key in [&b"y"[..], b"\r", b"l", b"s"] {
        assert_eq!(
            type_and_flush(&mut handoff, key),
            UserBytesRoute::WriteCounted
        );
    }
    assert!(!handoff.shell_line_may_hold_typed_text());
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(handoff.shell_line_may_hold_typed_text());
    assert!(
        !handoff.marker_reader_news_pending(),
        "the reader ended before the view looked: there is nothing to look at"
    );
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest,
        "the line is asked for, not left in the shell"
    );

    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    handoff.on_preexec(true);
    assert!(!handoff.prompt_reader_asks());
    assert!(!handoff.marker_reader_news_pending());
    assert!(!handoff.secret_reader_has_the_keys());
    assert!(
        handoff.is_pty_owned(),
        "the command the shell started has the pty"
    );

    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    handoff.on_new_session();
    assert!(!handoff.prompt_reader_asks());
    assert!(!handoff.marker_reader_news_pending());

    let mut handoff = sampled_prompt_the_editor_owns(&secret);
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(
        type_and_flush(&mut handoff, b"\r"),
        UserBytesRoute::WriteCounted
    );
    // An empty answer; the next prompt cycle comes with no line-editor signal in between.
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert!(!handoff.prompt_reader_asks());
    assert!(handoff.try_commit_at_prompt());
    assert!(
        !handoff.on_tty_reader_asks(Some(7), Some(2)),
        "the old prompt's marker, parsed late, is stale"
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(handoff.on_tty_reader_asks(Some(7), Some(3)));
}

/// #2052 with #1658: the pty owns input but the keys follow unsent text into the editor. The
/// marker ends that: the keys are the reader's, the text stays in the editor.
#[test]
fn a_readers_marker_ends_keys_following_the_editors_text() {
    let secret = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handoff = sampled_prompt_after_a_command(&secret);
    assert!(handoff.note_keys_follow_editor_if(true));
    assert!(!handoff.routes_keys_to_pty());
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    assert!(!handoff.keys_follow_editor_active());
    assert!(handoff.routes_keys_to_pty());
    assert_eq!(handoff.on_user_bytes(b"y"), UserBytesRoute::WriteCounted);
}

/// #2052 on ConPTY (Git Bash sends the same marker): the pty owns input at a prompt whose line
/// editor has not signalled. The marker notes the reader at the question's output, so the first
/// key goes to it with no hold and no look at the screen, also for a `read` that prints no
/// question; the line editor's signal ends it as in #2038. Keys already held when the marker
/// comes were typed first: the marker changes nothing and their hold's cap decides, as before.
#[test]
fn on_conpty_the_readers_marker_sends_the_first_key_to_the_reader_without_a_hold() {
    let mut handoff = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert!(handoff.on_tty_reader_asks(Some(7), Some(2)));
    assert_eq!(handoff.take_marker_reader_news(), Some(false));
    assert!(handoff.is_pty_owned());
    assert!(!handoff.prompt_reader_look_due(), "the screen is not asked");
    for key in [&b"y"[..], b"\r"] {
        assert_eq!(handoff.on_user_bytes(key), UserBytesRoute::WriteCounted);
    }
    assert!(!handoff.has_prompt_hold());
    assert!(!handoff.young_hold_queued());
    assert!(!handoff.has_unanswered_lines());
    assert!(handoff.on_line_editor_reading(Some(7), Some(2)));
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), None);
    assert!(!handoff.prompt_reader_asks());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );

    let mut held = conpty_prompt_after_a_command_in_a_shell_that_signalled();
    assert_eq!(held.on_user_bytes(b"y"), UserBytesRoute::Held);
    let id = held.take_prompt_hold_started().expect("a prompt hold");
    assert!(
        !held.on_tty_reader_asks(Some(7), Some(2)),
        "keys are held: their hold's own events decide"
    );
    assert!(!held.prompt_reader_asks());
    assert!(!held.marker_reader_news_pending());
    assert!(held.has_prompt_hold());
    // The cap's screen look (#2038) still notes the reader for the held keys, and the shell's
    // word then stands for it.
    assert!(held.note_prompt_reader_asks());
    assert!(!held.on_tty_reader_asks(Some(7), Some(2)));
    assert!(held.expire_queued_prompt_hold(id));
    assert!(held.route_released_chunk(b"y"));
    held.finish_queued_prompt_hold(id);
    assert!(held.secret_reader_has_the_keys());
}

/// A hold kept past its cap (an Enter among its keys) keeps its wait, so the line editor's signal
/// still releases it; a shell that had never signalled is switched off meanwhile, and back on when
/// the signal shows it does signal.
#[test]
fn an_overdue_hold_kept_for_the_events_is_still_released_by_the_signal() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"ls"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().unwrap();
    // #1697: the first unsignalled miss does not switch the hold off; the session's earlier
    // misses do. Two such prompts went before this one.
    handoff.prompt_hold_unsignalled_misses = CONPTY_PROMPT_HOLD_MISSES_BEFORE_OFF - 1;
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

    /// As [`Self::install`] for a platform that samples nothing (ConPTY): only the barrier.
    fn install_unsampled(&self, handoff: &mut TypeaheadHandoff) {
        let mut probe = ForegroundProbe::default();
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
    pending.session_emits_preexec = true;
    type_and_flush(&mut pending, b"explain");
    assert_eq!(pending.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        pending.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdMaybeInFlight)
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

/// #1615 (measurement only): an Enter delivered because the group in front is not the sampled shell
/// (a warpified ssh's remote shell, an inner shell) while a line is unanswered is followed through
/// the hooks after it, so a run can tell the remote #933 race (the delivered line ran as a shell
/// command at the next prompt) from an Enter the running command took. No decision changes: the
/// Enter is delivered, no hold, no timer.
#[test]
fn an_enter_delivered_past_an_unsampled_group_is_followed_through_the_next_hooks() {
    // The remote #933 shape: the remote command ends, its Precmd is on its way, the user's line
    // is delivered and the remote shell then runs it itself.
    let mut remote = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut remote, b"foo");
    assert!(
        !remote.follows_unsampled_enter(),
        "only an Enter is followed"
    );
    assert_eq!(remote.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    remote.on_user_write_flushed();
    assert_eq!(remote.take_young_hold_started(), None, "no hold, no timer");
    assert!(remote.follows_unsampled_enter());
    remote.on_precmd();
    assert!(
        remote.follows_unsampled_enter(),
        "the Precmd after the Enter is noted, the line's fate is the next hook's"
    );
    remote.on_preexec(true);
    assert!(
        !remote.follows_unsampled_enter(),
        "settled: the delivered line ran as a shell command"
    );

    // The running command took the Enter: its Precmd, then the next prompt's Precmd with no
    // command between, settles it.
    let mut consumed = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut consumed, b"y");
    type_and_flush(&mut consumed, b"\r");
    assert!(consumed.follows_unsampled_enter());
    consumed.on_precmd();
    consumed.on_precmd();
    assert!(!consumed.follows_unsampled_enter());

    // The editor's next command settles it as well.
    let mut next = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut next, b"y");
    type_and_flush(&mut next, b"\r");
    assert!(next.follows_unsampled_enter());
    next.on_command_written();
    assert!(!next.follows_unsampled_enter());

    // ... and a new session (the remote shell of the ssh the Enter was typed into).
    let mut session = seq_after_submit(ShellPromptFact::Unknown);
    type_and_flush(&mut session, b"y");
    type_and_flush(&mut session, b"\r");
    assert!(session.follows_unsampled_enter());
    session.on_new_session();
    assert!(!session.follows_unsampled_enter());

    // Not followed: an Enter on an empty line, a hold the sample decides (the shell's own line
    // editor reads), and a platform that samples nothing (its window hold decides).
    let mut empty = seq_after_submit(ShellPromptFact::Unknown);
    assert_eq!(empty.on_user_bytes(b"\r"), UserBytesRoute::WriteCounted);
    assert!(!empty.follows_unsampled_enter());
    let mut local = seq_after_submit(ShellPromptFact::LineEditorReading);
    type_and_flush(&mut local, b"foo");
    local.on_user_bytes(b"\r");
    assert!(!local.follows_unsampled_enter());
    let mut unsampled = young_after_submit();
    type_and_flush(&mut unsampled, b"foo");
    assert_eq!(unsampled.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert!(!unsampled.follows_unsampled_enter());
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
    assert_eq!(
        probe.asked(),
        vec!["sample"],
        "the line's first byte samples whether it is young; the barrier is not asked"
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

    // #1889: `queued_young_hold` leaves the stream unapplied, and where nothing samples the shell
    // the latch leaves such a hold to the barrier (the command's Precmd may be in that output).
    // The latch ends the hold, as this test has always asserted, once the stream is applied.
    let (mut handoff, id) = queued_young_hold();
    handoff.note_command_interactive();
    assert!(
        handoff.young_hold_queued(),
        "output in flight: the latch leaves the hold to the barrier"
    );
    assert!(handoff.take_young_hold_ends().is_empty());
    RawReadProbe::new(ShellPromptFact::Unknown, Some(true)).install_unsampled(&mut handoff);
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

/// #1889: where nothing samples the shell (ConPTY), the 50 ms latch no longer ends a live young
/// hold while the stream barrier says output is still in flight; once the barrier is drained
/// (or absent) it ends the hold as before.
#[test]
fn the_unsampled_latch_leaves_a_young_hold_to_the_barrier_while_output_is_in_flight() {
    let (mut handoff, id) = queued_young_hold();
    let barrier = RawReadProbe::new(ShellPromptFact::Unknown, Some(false));
    barrier.install_unsampled(&mut handoff);
    handoff.note_command_interactive();
    assert!(
        handoff.young_hold_queued(),
        "output in flight: the hold stays"
    );
    assert!(handoff.take_young_hold_ends().is_empty());
    barrier.set_drained(true);
    handoff.note_command_interactive();
    assert!(!handoff.young_hold_queued());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::Typeahead)]
    );
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

/// #1614 (design item 6, the property): over every combination of what the platform samples
/// (including a platform that samples nothing but has the stream barrier, ConPTY), what the
/// barrier says, what the hooks said last, whether the command is latched interactive and whether
/// the prompt-sequence path is on, a `Held` answer names evidence whose release is satisfiable
/// then and there: the request is due and the report runs the Enter; or the barrier's next
/// publish decides; or the Precmd (an event) sends the request. No clock decides (the window is
/// gone, 2026-11-09). The peek the destination table takes always agrees with the commit.
#[test]
fn every_hold_names_a_release_that_is_satisfiable_when_it_engages() {
    #[derive(Clone, Copy, Debug)]
    enum Probe {
        None,
        /// Nothing sampled; only the barrier (ConPTY's pipe peek).
        Unsampled(Option<bool>),
        Fact(ShellPromptFact, Option<bool>),
    }
    #[derive(Clone, Copy, Debug)]
    enum Hooks {
        None,
        PrecmdApplied,
        PreexecLast,
        ReadingAfterPreexec,
    }
    let mut probes = vec![
        Probe::None,
        Probe::Unsampled(Some(true)),
        Probe::Unsampled(Some(false)),
    ];
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
                {
                    for seq_mode in [false, true] {
                        let label = format!(
                            "{probe:?} {hooks:?} interactive={interactive} seq_mode={seq_mode}"
                        );
                        let raw = match probe {
                            Probe::None => None,
                            Probe::Unsampled(barrier) => {
                                Some(RawReadProbe::new(ShellPromptFact::Unknown, barrier))
                            }
                            Probe::Fact(fact, barrier) => Some(RawReadProbe::new(fact, barrier)),
                        };
                        let mut handoff = TypeaheadHandoff::default();
                        // The "own Preexec pending" holds need a session that emits Preexec.
                        handoff.session_emits_preexec = true;
                        handoff.set_prompt_seq_enabled(seq_mode);
                        handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
                        handoff.on_precmd_seq(Some(7), Some(1));
                        if let Some(raw) = &raw {
                            if matches!(probe, Probe::Unsampled(_)) {
                                raw.install_unsampled(&mut handoff);
                            } else {
                                raw.install(&mut handoff);
                            }
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
                        handoff.young_line = false;
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
                            !(interactive && matches!(probe, Probe::None | Probe::Unsampled(_))),
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
                                // `Probe::None` carries the test default (activity just now =
                                // an undrained barrier, `note_fresh_activity_for_test`).
                                assert!(
                                    matches!(
                                        probe,
                                        Probe::None
                                            | Probe::Fact(_, Some(false))
                                            | Probe::Unsampled(Some(false))
                                    ),
                                    "only an installed, undrained barrier: {label}"
                                );
                                assert_eq!(
                                    handoff.on_stream_applied(),
                                    StreamApplied::Nothing,
                                    "still unapplied: {label}"
                                );
                                match &raw {
                                    Some(raw) => raw.set_drained(true),
                                    None => {
                                        handoff.foreground_probe.stream_drained =
                                            Some(Arc::new(|| true))
                                    }
                                }
                                match handoff.on_stream_applied() {
                                    // The written command's own Preexec is still pending: the
                                    // hold waits for that Preexec (which does not end it) and
                                    // the Precmd that makes the request due.
                                    StreamApplied::Nothing => {
                                        assert!(
                                            matches!(hooks, Hooks::None),
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
                                    ) || (matches!(hooks, Hooks::None)
                                        && matches!(
                                            probe,
                                            Probe::Fact(
                                                ShellPromptFact::LineEditorReading,
                                                Some(true)
                                            ) | Probe::Fact(ShellPromptFact::ShellCooked, _)
                                        )),
                                    "the editor sampled reading with no barrier, or the written \
                                     command's own Preexec pending: {label}"
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

/// #1189: keys held behind a deferred Enter that wrote no command are released to the pty one
/// write per key event on ConPTY, so no write carries an Enter with the next line's keys behind
/// it; elsewhere they go out as one write, in typed order, as before.
#[test]
fn held_keys_released_after_an_unsubmitted_enter_keep_their_key_boundaries_on_conpty() {
    let keys = || {
        vec![
            b"e".to_vec(),
            b"c".to_vec(),
            Vec::new(),
            b"\r".to_vec(),
            b"\x1b[A".to_vec(),
            b"x".to_vec(),
            b"\r".to_vec(),
        ]
    };
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_conpty_input(true);
    assert_eq!(
        handoff.held_key_release_writes(keys()),
        vec![
            b"e".to_vec(),
            b"c".to_vec(),
            b"\r".to_vec(),
            b"\x1b[A".to_vec(),
            b"x".to_vec(),
            b"\r".to_vec(),
        ]
    );
    let plain = TypeaheadHandoff::default();
    assert_eq!(
        plain.held_key_release_writes(keys()),
        vec![b"ec\r\x1b[Ax\r".to_vec()]
    );
    assert_eq!(
        plain.held_key_release_writes(Vec::new()),
        Vec::<Vec<u8>>::new()
    );
}

/// The r18u23 fuzz-smoke crash input (kept as the seed
/// `fuzz/corpus/typeahead_handoff/regression_r18u23_signal_behind_deferred_enter`), decoded
/// event by event up to the key before the one the fuzz model rejected: a young hold's report
/// turned the hold into a deferred Enter (`SubmitOnReport`) and the keys typed after it reached
/// the hand-off. Returns the hand-off and the deferred Enter's gate.
fn replay_r18u23_fuzz_to_deferred_gate() -> (TypeaheadHandoff, u64) {
    let mut h = TypeaheadHandoff::default();
    let _ = h.on_report(); // 0
    let _ = h.adopt_untracked_typeahead(); // 1
    h.on_precmd_seq(Some(115), Some(26)); // 2
    h.set_conpty_input(true); // 3
    h.set_prompt_seq_enabled(true); // 4
    h.reset_on_exit(); // 5
    h.on_precmd_seq(Some(100), Some(24)); // 6
    let _ = h.cancel_deferred_enter(); // 7
    let _ = h.defer_enter(); // 8
    h.on_precmd(); // 9
    let _ = h.on_interrupt(); // 10
    let _ = h.adopt_untracked_typeahead(); // 11
    let _ = h.on_line_editor_reading(Some(116), Some(182)); // 12
    assert_eq!(h.on_user_bytes(b"\x1b[A"), UserBytesRoute::WriteCounted); // 13
    let _ = h.cancel_deferred_enter(); // 14
    h.on_new_session(); // 15
    h.set_conpty_prompt_hold_shell_signals(true); // 16
    let _ = h.on_stream_applied(); // 17
    assert_eq!(h.on_user_bytes(b"\r"), UserBytesRoute::Held); // 18
    assert!(h.take_young_hold_started().is_some());
    h.note_interactive_command_finished(); // 19
    h.set_prompt_seq_enabled(true); // 20
    let _ = h.cancel_deferred_enter(); // 21
    let _ = h.on_preexec(false); // 22: ends the hold as typeahead
    assert!(!h.young_hold_queued());
    let _ = h.try_commit_at_prompt(); // 23
    h.note_command_interactive(); // 24
    if let Some(id) = h.release_queued_prompt_hold_on_signal() {
        h.finish_queued_prompt_hold(id); // 25
    }
    h.set_conpty_prompt_hold(true); // 26
    let _ = h.on_report(); // 27
    let _ = h.on_user_bytes_to_full_screen_program(b"ls"); // 28
    h.set_prompt_seq_enabled(true); // 29
    let _ = h.release_after_unsubmitted_enter(); // 30
    h.note_interactive_command_finished(); // 31
    h.set_conpty_prompt_hold(false); // 32
    h.set_conpty_prompt_hold_shell_signals(false); // 33
    let _ = h.on_report(); // 34
    let _ = h.defer_enter(); // 35
    let _ = h.on_user_write_flushed(); // 36
    h.on_precmd(); // 37
    let _ = h.try_commit_at_prompt(); // 38
    let _ = h.cancel_deferred_enter(); // 39
    let _ = h.defer_enter(); // 40
    assert_eq!(h.on_user_bytes(b"\r"), UserBytesRoute::Held); // 41: a young hold
    assert!(h.take_young_hold_started().is_some());
    h.on_new_session(); // 42
    let _ = h.on_user_write_flushed(); // 43
    let _ = h.on_report(); // 44: the pty owns input; stashed
    assert_eq!(
        h.on_line_editor_active(true, true), // 45
        ActiveDecision::SendRequest
    );
    assert_eq!(h.on_report(), ReportDisposition::InsertThenSubmitYoung); // 46
    let gate = h.deferred_gate().expect("the report made a deferred Enter");
    let _ = h.adopt_untracked_typeahead(); // 47
    h.on_precmd_seq(Some(115), Some(26)); // 48
    for key in [&b"a"[..], &b"a"[..], &b"a"[..], &b"\r"[..]] {
        // 49..52: keys typed behind the deferred Enter, reported for its gate.
        assert_eq!(h.on_user_bytes(key), UserBytesRoute::Held);
        assert_eq!(h.take_deferred_key_started(), Some(gate));
    }
    assert_eq!(h.state(), &HandoffState::HoldAfterEnter { released: true });
    let _ = h.take_young_hold_ends();
    (h, gate)
}

/// r18u23 fuzz-smoke crash ("a signal was held: [4]", event 53): the fuzz model said a signal byte
/// is never held, but behind a deferred Enter's gate an EOF (and ^Z, ^\, which the legacy queue's
/// `signal_must_wait` also keeps behind a `HandoffReport` gate) is typed after the keys that gate
/// holds and must go out after them, in typed order (the gate table's `HandoffReport` + EOF =
/// `WaitInOrder`). The hand-off holds it and reports it for that gate, so it is not lost, and it
/// follows the deferred command with the other keys. The model was corrected, not the app.
#[test]
fn a_signal_behind_a_deferred_enter_waits_in_order_r18u23_fuzz() {
    for signal in [0x04u8, 0x1a, 0x1c] {
        let (mut h, gate) = replay_r18u23_fuzz_to_deferred_gate();
        assert_eq!(h.on_user_bytes(&[signal]), UserBytesRoute::Held); // 53
        assert_eq!(
            h.take_deferred_key_started(),
            Some(gate),
            "the held signal is reported for the deferred Enter's gate"
        );
        assert_eq!(h.deferred_gate(), Some(gate));
        assert_eq!(h.state(), &HandoffState::HoldAfterEnter { released: true });
        // The deferred command is written: the gate's keys, the signal last, follow it.
        h.on_command_written();
        assert_eq!(
            h.take_young_hold_ends(),
            vec![(gate, YoungHoldEnd::FollowCommand)]
        );
    }
    // ^C is never held there: the view cancels the deferred Enter first (`TerminalView::ctrl_c`),
    // which drops the gate's keys, and the ^C then goes out at once.
    let (mut h, gate) = replay_r18u23_fuzz_to_deferred_gate();
    assert!(h.cancel_deferred_enter());
    assert_eq!(h.take_young_hold_ends(), vec![(gate, flushed())]);
    assert_eq!(h.on_user_bytes(&[0x03]), UserBytesRoute::Write);
    assert_eq!(h.take_deferred_key_started(), None);
}

/// #1661 (owner decision 2026-11-05): two young holds end at their cap by design and keep their
/// short cap (and, in the view, the hand-back): the fish no-barrier hold and the barrier hold of a
/// session without prompt sequence numbers (what the window hold was). Every other young hold's
/// cap is a give-up: the 10 s net (which drops the keys).
#[test]
fn only_the_no_barrier_young_hold_keeps_its_short_cap() {
    // Fish, no barrier: `PrecmdMaybeInFlight`, the 2 s cap is its normal end.
    let mut fish = seq_after_submit(ShellPromptFact::LineEditorReading);
    type_and_flush(&mut fish, b"explain");
    fish.age_last_activity_for_test();
    fish.young_line = false;
    assert_eq!(fish.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        fish.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdMaybeInFlight)
    );
    assert!(fish.young_hold_cap_is_normal_end());
    assert_eq!(fish.young_hold_give_up(Instant::now()), EVENT_HOLD_GIVE_UP);

    // A barrier says a Precmd may be in flight: a sequence-path hold, the 10 s give-up net.
    let probe = RawReadProbe::new(ShellPromptFact::LineEditorReading, Some(false));
    let mut barriered = seq_after_submit(ShellPromptFact::LineEditorReading);
    probe.install(&mut barriered);
    type_and_flush(&mut barriered, b"explain");
    barriered.age_last_activity_for_test();
    barriered.young_line = false;
    assert_eq!(barriered.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        barriered.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert!(!barriered.young_hold_cap_is_normal_end());
    assert_eq!(
        barriered.young_hold_give_up(Instant::now()),
        YOUNG_HOLD_GIVE_UP
    );

    // Nothing sampled, prompt sequence numbers (ConPTY that sends them): the barrier says output
    // is unparsed -- a give-up, 10 s.
    let mut unsampled = seq_unsampled_after_submit();
    type_and_flush(&mut unsampled, b"explain");
    assert_eq!(unsampled.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        unsampled.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert!(!unsampled.young_hold_cap_is_normal_end());
    assert_eq!(
        unsampled.young_hold_give_up(Instant::now()),
        YOUNG_HOLD_GIVE_UP
    );

    // No prompt sequence numbers: the same barrier decides, and the cap is the hold's normal end
    // (the hand-back the window hold of such a session had, #1661).
    let mut legacy = young_after_submit();
    type_and_flush(&mut legacy, b"explain");
    assert_eq!(legacy.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        legacy.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdInFlight)
    );
    assert!(legacy.young_hold_cap_is_normal_end());
    assert_eq!(
        legacy.young_hold_give_up(Instant::now()),
        EVENT_HOLD_GIVE_UP
    );
}

/// #1661 review: a hold's class is decided when it starts. A no-barrier (fish) hold stays a
/// normal-end hold (short cap, hand-back) when a later sequence-numbered Precmd is parsed while it
/// waits; recomputed at the cap it was dropped.
#[test]
fn a_young_holds_normal_end_class_is_fixed_when_it_starts() {
    let mut handoff = seq_after_submit(ShellPromptFact::LineEditorReading);
    type_and_flush(&mut handoff, b"explain");
    assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
    assert_eq!(
        handoff.young_hold_evidence(),
        Some(YoungHoldEvidence::PrecmdMaybeInFlight)
    );
    assert!(handoff.young_hold_cap_is_normal_end());
    assert_eq!(
        handoff.young_hold_give_up(Instant::now()),
        EVENT_HOLD_GIVE_UP
    );

    // The shell sends another sequence number before the cap.
    handoff.on_precmd_seq(Some(7), Some(2));
    assert!(
        handoff.young_hold_cap_is_normal_end(),
        "the class the hold started with is kept: its cap hands back, it does not drop"
    );
}

/// #1793 fixture: the hand-off of a managed ssh pane after `echo hi` ran and the prompt's legacy
/// request was answered with an empty line: the editor owns input, nothing outstanding, and the
/// controller's write count is still the request's own write. `pty` stands in for
/// `PtyController`, which counts every write it hands the pty.
fn after_answered_empty_report() -> (TypeaheadHandoff, PtyWriteCount) {
    let mut handoff = TypeaheadHandoff::default();
    let pty = handoff.pty_write_count();
    handoff.on_command_written();
    pty.note_write_bytes(b"echo hi\r");
    handoff.note_written_command_text("echo hi");
    handoff.note_preexec_command("echo hi");
    let _ = handoff.on_preexec(false);
    handoff.on_precmd();
    assert!(handoff.try_commit_at_prompt());
    // The prompt's line editor: the legacy request (`ESC i`) is decided, then written.
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Legacy
    );
    handoff.on_legacy_request_written();
    pty.note_write_bytes(b"\x1bi");
    // Its report: parsed by the model, then handled by the view.
    handoff.on_report_parsed();
    assert_eq!(handoff.on_report(), ReportDisposition::InsertLegacy);
    handoff.set_ssh_escape_cr_line_start_rule(true);
    (handoff, pty)
}

/// The same hand-off under the old proof-based rule (`PHOSPHOR_SSH_ESCAPE_CR_RULE=0`).
fn legacy_rule_after_answered_empty_report() -> (TypeaheadHandoff, PtyWriteCount) {
    let (mut handoff, pty) = after_answered_empty_report();
    handoff.set_ssh_escape_cr_line_start_rule(false);
    (handoff, pty)
}

/// #1793: ssh takes `~.` only right after a newline in its input, and in a managed ssh session the
/// last bytes it read are the hand-off's request. Where the remote line is proven empty, the escape
/// path puts a `\r` ahead of the escape, and the hand-off counts the line it submits as a bare
/// Enter's: the pty owns input, the remote Precmd answers it, the request at that prompt hands input
/// back to the editor.
#[test]
fn an_ssh_escape_on_a_proven_empty_line_gets_a_carriage_return_counted_as_a_bare_enter() {
    let (mut handoff, pty) = after_answered_empty_report();
    assert_eq!(handoff.ssh_escape_line_start(), SshEscapeLineStart::AddCr);
    // The view routes `\r~?` (one write) through the hand-off, and the controller writes it.
    assert_eq!(
        type_and_flush(&mut handoff, b"\r~?"),
        UserBytesRoute::WriteCounted
    );
    pty.note_write_bytes(b"\r~?");
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 1,
            precmds_seen: 0,
        }
    );
    assert!(handoff.routes_keys_to_pty());
    // ssh consumed `~?` and stays at its line start: a second escape needs no `\r` while nothing
    // else was written.
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::AlreadyAtLineStart
    );
    pty.note_write_bytes(b"~?");

    // The remote shell's Precmd for the empty line, then its line editor: a request (the line may
    // hold what was typed behind the `\r`), whose report hands input back to the editor.
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    pty.note_write_bytes(b"\x1bi");
    handoff.on_report_parsed();
    assert_eq!(handoff.on_report(), ReportDisposition::Insert);
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert_eq!(handoff.requests_outstanding(), 0);
    // That request wrote `ESC i` after the escape: ssh is not at its line start, and the report
    // proved the line empty, so the next escape gets its own `\r`.
    assert_eq!(handoff.ssh_escape_line_start(), SshEscapeLineStart::AddCr);
}

/// #1793 (the rule the #2186 kill switch restores): the `\r` is never added where it could
/// submit remote text or reach a running command.
#[test]
fn old_rule_ssh_escape_gets_no_carriage_return_unless_the_remote_line_is_proven_empty() {
    // Untracked user bytes in the shell's line.
    let (mut handoff, _pty) = legacy_rule_after_answered_empty_report();
    handoff.note_untracked_line_bytes();
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("untracked_line_bytes")
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);

    // A request outstanding (written, no report yet).
    let (mut handoff, pty) = legacy_rule_after_answered_empty_report();
    handoff.on_legacy_request_written();
    pty.note_write_bytes(b"\x1bi");
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("request_outstanding")
    );

    // A report parsed that the view has not handled yet.
    let (mut handoff, _pty) = legacy_rule_after_answered_empty_report();
    handoff.on_report_parsed();
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("report_on_its_way")
    );

    // Anything written to the pty since the report (a key, an agent's write, a command).
    let (mut handoff, pty) = legacy_rule_after_answered_empty_report();
    pty.note_write();
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("pty_written_since_report")
    );

    // A command started since (its Preexec), or a new prompt not yet reported.
    let (mut handoff, _pty) = legacy_rule_after_answered_empty_report();
    let _ = handoff.on_preexec(true);
    assert_ne!(handoff.ssh_escape_line_start(), SshEscapeLineStart::AddCr);
    let (mut handoff, _pty) = legacy_rule_after_answered_empty_report();
    handoff.on_precmd();
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("no_report_since_prompt")
    );

    // The pty owns input (a command was written).
    let (mut handoff, _pty) = legacy_rule_after_answered_empty_report();
    handoff.on_command_written();
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("pty_owned")
    );

    // Nothing ever reported: nothing proves the line empty.
    let mut fresh = TypeaheadHandoff::default();
    fresh.set_ssh_escape_cr_line_start_rule(false);
    assert_eq!(
        fresh.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("no_report_since_prompt")
    );
    assert_eq!(fresh.state(), &HandoffState::EditorOwned);
}

/// #2186: ssh honours `~` only right after a `\r` or `\n` it read, and the app wrote every byte it
/// read. After the last `ESC i` request with no report (nu), after a terminal-query reply written
/// behind the last newline, the escape gets a `\r` whatever the report bookkeeping says.
#[test]
fn an_ssh_escape_after_a_request_or_reply_with_no_report_gets_a_carriage_return() {
    // A request written, its report never comes (nu).
    let (mut handoff, pty) = after_answered_empty_report();
    handoff.on_legacy_request_written();
    pty.note_write_bytes(b"\x1bi");
    assert_eq!(handoff.requests_outstanding(), 1);
    assert_eq!(handoff.ssh_escape_line_start(), SshEscapeLineStart::AddCr);

    // A reply written behind the last newline.
    let (mut handoff, pty) = after_answered_empty_report();
    pty.note_write_bytes(b"\r");
    assert!(pty.tail_is_newline());
    pty.note_tail(b"\x1b[24;1R");
    assert!(!pty.tail_is_newline());
    assert_eq!(handoff.ssh_escape_line_start(), SshEscapeLineStart::AddCr);

    // No report at all since the prompt.
    let (mut handoff, _pty) = after_answered_empty_report();
    handoff.on_precmd();
    assert_eq!(handoff.ssh_escape_line_start(), SshEscapeLineStart::AddCr);
}

/// #2186: while the hand-off waits for the shell's report, the remote line may hold the user's
/// typed-ahead bytes (it asks only then, or after an interrupt): a `\r` could run them, so the
/// escape goes unprefixed even though the last byte written (the request) is not a newline.
#[test]
fn an_ssh_escape_while_a_report_is_awaited_for_typed_bytes_gets_no_carriage_return() {
    let mut handoff = awaiting_report();
    handoff.set_ssh_escape_cr_line_start_rule(true);
    assert!(!handoff.pty_write_count().tail_is_newline());
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("awaiting_report")
    );
}

/// #2186: where the last byte written was a newline ssh is at its line start: the escape goes
/// alone, no extra `\r`.
#[test]
fn an_ssh_escape_right_after_a_newline_is_sent_alone() {
    let (mut handoff, pty) = after_answered_empty_report();
    pty.note_write_bytes(b"\r");
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::TailIsNewline
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    // The escape is the next write; ssh stays at its line start after it.
    pty.note_write_bytes(b"~?");
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::AlreadyAtLineStart
    );
}

/// #2186: `\n` ends a line like `\r`, and an empty write changes nothing.
#[test]
fn the_pty_write_tail_follows_the_last_byte_written() {
    let pty = PtyWriteCount::default();
    assert!(!pty.tail_is_newline());
    pty.note_write_bytes(b"ls\n");
    assert!(pty.tail_is_newline());
    pty.note_write_bytes(b"");
    assert!(pty.tail_is_newline());
    pty.note_write_bytes(b"\x1bi");
    assert!(!pty.tail_is_newline());
    pty.note_write_bytes(b"\r");
    assert!(pty.tail_is_newline());
    assert_eq!(pty.get(), 4);
}

/// #2186: the `\r` is still withheld where the remote line may hold the user's bytes.
#[test]
fn an_ssh_escape_gets_no_carriage_return_where_the_remote_line_may_hold_user_bytes() {
    // Untracked user bytes in the shell's line.
    let (mut handoff, _pty) = after_answered_empty_report();
    handoff.note_untracked_line_bytes();
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("untracked_line_bytes")
    );

    // The pty owns input and the user's bytes were written.
    let (mut handoff, _pty) = after_answered_empty_report();
    handoff.on_command_written();
    handoff.note_written_command_text("");
    assert_eq!(
        type_and_flush(&mut handoff, b"ab"),
        UserBytesRoute::WriteCounted
    );
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("pty_owned_user_bytes")
    );
}

/// #2186: keys held behind a young Enter keep the escape as typed.
#[test]
fn an_ssh_escape_gets_no_carriage_return_while_keys_are_held() {
    let mut handoff = young_after_submit();
    handoff.set_ssh_escape_cr_line_start_rule(true);
    // A bare Enter on an empty line is never a young hold (`should_hold_young_enter`): the line
    // needs content first, as in every other young-hold test.
    type_and_flush(&mut handoff, b"frob");
    let _ = hold_young_enter(&mut handoff, b"\r");
    // Held keys always sit behind user bytes in the line; either blocker keeps the `\r` off.
    let got = handoff.ssh_escape_line_start();
    assert!(
        matches!(
            got,
            SshEscapeLineStart::Skipped("keys_held" | "pty_owned_user_bytes")
        ),
        "{got:?}: no `\\r` may run the user's line while keys are held"
    );
}

/// #2186: the kill switch restores the old rule exactly: no report since the prompt is a skip.
#[test]
fn the_ssh_escape_kill_switch_restores_the_proof_based_rule() {
    let (mut handoff, pty) = legacy_rule_after_answered_empty_report();
    pty.note_write_bytes(b"\r");
    // Even right after a newline, the old rule wants a proof (and nothing written since): none.
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("pty_written_since_report")
    );
    let (mut handoff, _pty) = legacy_rule_after_answered_empty_report();
    handoff.on_precmd();
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("no_report_since_prompt")
    );
}

/// #2186: a new session knows nothing of what ssh last read.
#[test]
fn the_pty_write_tail_is_unknown_after_a_reset_on_exit() {
    let mut handoff = TypeaheadHandoff::default();
    let pty = handoff.pty_write_count();
    pty.note_write_bytes(b"\r");
    assert!(pty.tail_is_newline());
    handoff.reset_on_exit();
    assert!(!pty.tail_is_newline());
}

/// #1793: the write count is shared with the controller across a session's end (the controller
/// keeps the cell it took at its start).
#[test]
fn the_pty_write_count_survives_a_reset_on_exit() {
    let mut handoff = TypeaheadHandoff::default();
    let pty = handoff.pty_write_count();
    handoff.reset_on_exit();
    pty.note_write();
    assert_eq!(handoff.pty_write_count().get(), 1);
}

#[test]
fn a_report_is_pending_before_editor_text_until_it_lands() {
    // #1659: a Tab pressed now would complete only the editor's fragment.
    assert!(!TypeaheadHandoff::default().report_pending_before_editor_text());
    let mut handoff = awaiting_report();
    assert!(handoff.report_pending_before_editor_text());
    // An Enter deferred behind the report: the report is still to come.
    assert!(handoff.defer_enter());
    assert!(handoff.report_pending_before_editor_text());
    handoff.on_report();
    assert!(!handoff.report_pending_before_editor_text());
    // Keys the pty owns are not the editor's text at all.
    assert!(!after_submit().report_pending_before_editor_text());
}

/// `read x` finished on a platform that samples nothing (ConPTY): no probe, no barrier.
fn read_x_finished_unsampled() -> TypeaheadHandoff {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_prompt_seq_enabled(true);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd_seq(Some(7), Some(1));
    handoff.on_command_written();
    handoff.note_shell_hook_parsed(ParsedShellHook::Preexec);
    handoff.on_preexec(false);
    assert_eq!(
        type_and_flush(&mut handoff, b"tok\r"),
        UserBytesRoute::WriteCounted
    );
    handoff.note_shell_hook_parsed(ParsedShellHook::CommandFinished);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    handoff.on_precmd();
    handoff.on_precmd_seq(Some(7), Some(3));
    assert!(handoff.counts_diverged());
    handoff
}

/// Owner rule (a timer never resets counts or decides): on ConPTY (no probe, no barrier)
/// PowerShell's `LineEditorReading` marker, parsed after the last counted Enter, is the proof that
/// its line editor reads again, so a consumed `read`/stuck PS2 re-baselines on that EVENT; with
/// no marker, a marker from before the Enter, another shell, or the kill switch off, nothing is
/// proven and the pty keeps input.
#[test]
fn a_powershell_marker_after_the_last_enter_proves_stale_counts_without_a_probe() {
    // No marker yet: not provable; and the proof flag alone is not enough.
    let mut handoff = read_x_finished_unsampled();
    handoff.set_conpty_marker_proof(true);
    assert!(handoff.has_unanswered_lines_for_activation());
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));

    // The marker after the Enter, PowerShell, switch on: proven, the editor takes input.
    let mut handoff = read_x_finished_unsampled();
    handoff.set_conpty_marker_proof(true);
    handoff.on_line_editor_reading(None, None);
    assert!(!handoff.has_unanswered_lines_for_activation());
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::Committed
    );
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);

    // Not PowerShell (or the kill switch off): the marker proves nothing.
    let mut handoff = read_x_finished_unsampled();
    handoff.on_line_editor_reading(None, None);
    assert!(handoff.has_unanswered_lines_for_activation());
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));

    // An Enter counted after the marker clears it: the marker may predate that Enter.
    let mut handoff = read_x_finished_unsampled();
    handoff.set_conpty_marker_proof(true);
    handoff.on_line_editor_reading(None, None);
    assert_eq!(
        type_and_flush(&mut handoff, b"z\r"),
        UserBytesRoute::WriteCounted
    );
    assert!(handoff.has_unanswered_lines_for_activation());
}

/// #1891 (secret): `Read-Host -AsSecureString` typed as the command. The prompt's own
/// `LineEditorReading` marker (stamped with that prompt's sequence number) is parsed AFTER the
/// Enter was counted. It must not re-baseline the counts: the editor does not read, `Read-Host`
/// does, and the next keys are the secret. Only a marker past the answered prompt proves it.
#[test]
fn a_powershell_marker_stamped_with_the_answered_prompt_proves_nothing() {
    let mut handoff = read_x_finished_unsampled();
    handoff.set_conpty_marker_proof(true);
    // The marker of the prompt the Enter answered (seq 1), parsed late.
    handoff.on_line_editor_reading(Some(7), Some(1));
    assert!(
        handoff.has_unanswered_lines_for_activation(),
        "a late marker of the answered prompt must not re-baseline"
    );
    assert!(matches!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::DrainAndWait { .. }
    ));

    // The next prompt's marker (seq 3) proves it.
    let mut handoff = read_x_finished_unsampled();
    handoff.set_conpty_marker_proof(true);
    handoff.on_line_editor_reading(Some(7), Some(3));
    assert!(!handoff.has_unanswered_lines_for_activation());
}

/// #1882: a command that announced a tty read (`Read-Host`) is the reader until it says it ended
/// or the next Precmd/Preexec: the counts are not proven stale meanwhile, whatever markers arrive.
#[test]
fn an_announced_tty_read_keeps_the_pty_the_owner_until_it_ends() {
    let mut handoff = read_x_finished_unsampled();
    handoff.set_conpty_marker_proof(true);
    handoff.on_tty_read(true);
    assert_eq!(handoff.tty_read_marker(), Some(true));
    handoff.on_line_editor_reading(Some(7), Some(3));
    assert!(handoff.has_unanswered_lines_for_activation());
    handoff.on_tty_read_end();
    assert_eq!(handoff.tty_read_marker(), None);
    assert!(!handoff.has_unanswered_lines_for_activation());

    // A Precmd/Preexec ends the announcement too (the command moved on or was killed).
    let mut handoff = read_x_finished_unsampled();
    handoff.on_tty_read(false);
    handoff.note_shell_hook_parsed(ParsedShellHook::Precmd);
    assert_eq!(handoff.tty_read_marker(), None);
}

/// The outcome set of `young_submit_decision_for`, read in full (2026-11-09): no classifier ->
/// `KeepInEditor`; a landed verdict -> `Submit`; a pending verdict past the compute cap ->
/// `SubmitWithoutVerdict` (the view hands the text back for an undecided type, it never submits on
/// the clock); otherwise `Wait`. Nothing else.
#[test]
fn the_young_submit_decision_has_exactly_four_outcomes() {
    let cap = young_detection_cap_for(10);
    for waited in [Duration::ZERO, cap - Duration::from_millis(1), cap, cap * 4] {
        assert_eq!(
            young_submit_decision_for(false, true, waited, 10),
            YoungSubmit::KeepInEditor
        );
        assert_eq!(
            young_submit_decision_for(false, false, waited, 10),
            YoungSubmit::KeepInEditor
        );
        assert_eq!(
            young_submit_decision_for(true, false, waited, 10),
            YoungSubmit::Submit
        );
        let pending = young_submit_decision_for(true, true, waited, 10);
        assert_eq!(
            pending,
            if waited >= cap {
                YoungSubmit::SubmitWithoutVerdict
            } else {
                YoungSubmit::Wait
            }
        );
    }
}

// ---- #1658: keys follow the editor's unsent text while the pty still owns input ----

#[test]
fn a_kept_line_makes_keys_follow_the_editor_not_the_pty() {
    let mut handoff = after_submit();
    assert!(handoff.is_pty_owned());
    assert!(handoff.routes_keys_to_pty());
    assert!(handoff.note_keys_follow_editor());
    assert!(handoff.is_pty_owned(), "ownership is unchanged");
    assert!(!handoff.routes_keys_to_pty(), "a typed key is the editor's");
    assert!(!handoff.wants_terminal_focus());
    assert!(handoff.editor_takeover_would_be_open());
    assert!(handoff.editor_takeover_open(Duration::from_secs(10), Instant::now()));
}

#[test]
fn keys_follow_the_editor_only_where_the_pty_owns_input_and_the_switch_is_on() {
    // Nothing kept (the editor already owns input): plain routing, nothing set.
    let mut editor_owned = TypeaheadHandoff::default();
    assert!(!editor_owned.note_keys_follow_editor());
    assert!(!editor_owned.keys_follow_editor_active());
    // Kill switch off: today's behaviour, keys keep going to the pty.
    let mut handoff = after_submit();
    assert!(!handoff.note_keys_follow_editor_if(false));
    assert!(handoff.routes_keys_to_pty());
    assert!(!handoff.editor_takeover_would_be_open());
}

#[test]
fn the_ten_second_takeover_give_up_never_clears_keys_follow_editor() {
    let start = Instant::now();
    let mut handoff = after_submit();
    handoff.note_keys_follow_editor();
    handoff.note_editor_took_over(start);
    let give_up = Duration::from_secs(10);
    assert!(handoff.editor_takeover_open(give_up, start + Duration::from_secs(11)));
    assert!(handoff.editor_takeover_open(give_up, start + Duration::from_secs(3600)));
    assert!(!handoff.routes_keys_to_pty());
    // The focus event ends the window only, not the fact.
    handoff.on_editor_focused();
    assert!(handoff.keys_follow_editor_active());
}

#[test]
fn every_event_returns_the_keys_to_the_pty() {
    fn set() -> TypeaheadHandoff {
        let mut handoff = after_submit();
        assert!(handoff.note_keys_follow_editor());
        handoff
    }
    type Event = (&'static str, fn(&mut TypeaheadHandoff));
    let events: [Event; 7] = [
        ("preexec", |h| {
            h.on_preexec(false);
        }),
        (
            "command became interactive (a program, alt screen or ssh owns the tty)",
            |h| h.note_command_interactive(),
        ),
        ("interrupt", |h| {
            h.on_interrupt();
        }),
        ("click or selection", |h| h.end_editor_takeover_window()),
        ("a command written", |h| h.on_command_written()),
        (
            "the view ends it (editor empty, alt screen, password)",
            |h| {
                assert!(h.end_keys_follow_editor("test"));
            },
        ),
        ("session end", |h| h.reset_on_exit()),
    ];
    for (name, event) in events {
        let mut handoff = set();
        event(&mut handoff);
        assert!(!handoff.keys_follow_editor_active(), "{name}");
        if !matches!(name, "session end") {
            assert!(
                handoff.is_pty_owned() || handoff.editor_owns_input(),
                "{name}"
            );
        }
    }
    // A preexec leaves the pty owning input and routing keys to it.
    let mut handoff = set();
    handoff.on_preexec(false);
    assert!(handoff.routes_keys_to_pty());
}

#[test]
fn a_state_change_ends_keys_follow_editor_and_a_new_pty_owned_state_does_not_inherit_it() {
    let mut handoff = after_submit();
    handoff.note_keys_follow_editor();
    handoff.on_precmd();
    // The request is written (AwaitingReport) or the prompt commits (EditorOwned): both leave
    // `PtyOwned`, and the fact with it.
    let _ = handoff.on_line_editor_active(true, true);
    assert!(!handoff.keys_follow_editor_active());
    handoff.on_command_written();
    assert!(handoff.is_pty_owned());
    assert!(
        handoff.routes_keys_to_pty(),
        "a new command's pty-owned state starts clean"
    );
}

/// #2012 (round 9 Windows, PowerShell: `Add-Co` became `d-AdCo`): a Preexec of a command the shell
/// started itself takes input from the editor, which may hold text typed before it. That
/// transition, and only that one, is marked for the view (once), so the report of the keys typed
/// while that command runs is placed behind the editor's text. A Preexec of the editor's own
/// command, or one that changes no owner, marks nothing.
#[test]
fn a_shell_started_preexec_that_takes_input_from_the_editor_is_marked_once() {
    let mut handoff = TypeaheadHandoff::default();
    assert!(!handoff.take_shell_took_input_from_editor());
    assert!(handoff.on_preexec(true), "the pty takes input over");
    assert!(handoff.is_pty_owned());
    assert!(handoff.take_shell_took_input_from_editor());
    assert!(!handoff.take_shell_took_input_from_editor(), "taken once");
    // The pty already owns input: a second shell-started line changes no owner.
    assert!(!handoff.on_preexec(true));
    assert!(!handoff.take_shell_took_input_from_editor());

    // The editor's own command: the editor was emptied by its submit.
    let mut own = after_submit();
    assert!(!own.on_preexec(false));
    assert!(!own.take_shell_took_input_from_editor());

    // A Preexec the app's own command caused while the editor owns input changes nothing.
    let mut editor_owned = TypeaheadHandoff::default();
    assert!(!editor_owned.on_preexec(false));
    assert!(!editor_owned.take_shell_took_input_from_editor());
}

/// A session that has never emitted a Preexec (a shell or program without our hooks) must not hold
/// an Enter for "the written command's own Preexec": that event cannot come, the 2 s cap would hand
/// the Enter back and the running program would never get it (Mac terminal-mode run, 2026-11-09).
/// The Enter is delivered as plain typeahead at once; nothing is handed back. A session that has
/// emitted one keeps the hold. The hand-off is shared by Legacy and Table.
#[test]
fn an_unproven_session_gets_its_enter_at_once_not_a_hold_for_a_preexec_that_cannot_come() {
    for fact in [
        ShellPromptFact::LineEditorReading,
        ShellPromptFact::ShellCooked,
    ] {
        let probe = RawReadProbe::new(fact, Some(true));
        let mut unproven = seq_after_submit(fact);
        probe.install(&mut unproven);
        // Command written (its Preexec pending), keys typed behind it, then the Enter.
        assert!(!unproven.session_emits_preexec);
        type_and_flush(&mut unproven, b"explain");
        assert_eq!(
            unproven.on_user_bytes(b"\r"),
            UserBytesRoute::WriteCounted,
            "{fact:?}: delivered at once"
        );
        assert!(
            !unproven.has_young_hold(),
            "{fact:?}: nothing held, nothing to hand back"
        );

        // The same session after it emitted a Preexec once keeps today's hold.
        let probe = RawReadProbe::new(fact, Some(true));
        let mut proven = seq_after_submit(fact);
        probe.install(&mut proven);
        proven.session_emits_preexec = true;
        type_and_flush(&mut proven, b"explain");
        assert_eq!(
            proven.on_user_bytes(b"\r"),
            UserBytesRoute::Held,
            "{fact:?}"
        );
        assert_eq!(
            proven.young_hold_evidence(),
            Some(YoungHoldEvidence::PrecmdMaybeInFlight),
            "{fact:?}"
        );
        // A new session forgets it.
        proven.on_new_session();
        assert!(!proven.session_emits_preexec);
    }
}

/// #2182: keys the view holds (a prompt hold in its queue) make the pty controller wait with the
/// app's own in-band command; the flag is the view's publication, a switch turns it off.
#[test]
fn the_view_holding_keys_holds_queued_writes_until_it_publishes_none() {
    let mut handoff = conpty_awaiting_line_editor();
    let cell = handoff.view_keys_cell();
    assert!(!handoff.holds_queued_writes());
    assert_eq!(handoff.on_user_bytes(b"e"), UserBytesRoute::Held);
    cell.publish(true);
    assert!(handoff.view_holds_keys());
    assert!(handoff.holds_queued_writes());
    cell.publish(false);
    assert!(!handoff.view_holds_keys());
    assert!(!handoff.holds_queued_writes());
}

/// #2182: the kill switch restores the old behaviour: held keys do not hold queued writes.
#[test]
fn the_inband_wait_kill_switch_ignores_the_views_held_keys() {
    let mut handoff = conpty_awaiting_line_editor();
    handoff.view_keys_cell().publish(true);
    assert!(handoff.inband_waits_for_keys());
    assert!(handoff.holds_queued_writes());
    handoff.set_inband_wait_for_keys_enabled(false);
    assert!(!handoff.inband_waits_for_keys());
    assert!(!handoff.view_holds_keys());
    assert!(!handoff.holds_queued_writes());
}

/// #2182: the regression of the reverted first fix. 'e' and 'x' are held (a precmd seen, the line
/// editor's signal awaited); the app's in-band command waits (no write, so nothing ends or flips
/// the hold); 'i', 't' and Enter queue behind the same gate. `exit\r` reaches ONE destination,
/// intact and in typed order, with the switch on and off.
#[test]
fn keys_held_before_an_inband_command_stay_one_ordered_run() {
    for switch_on in [true, false] {
        let mut handoff = conpty_awaiting_line_editor();
        handoff.set_inband_wait_for_keys_enabled(switch_on);
        let keys: [&[u8]; 5] = [b"e", b"x", b"i", b"t", b"\r"];
        assert_eq!(handoff.on_user_bytes(keys[0]), UserBytesRoute::Held);
        let id = handoff.take_prompt_hold_started().expect("a hold started");
        handoff.view_keys_cell().publish(true);
        assert_eq!(handoff.on_user_bytes(keys[1]), UserBytesRoute::Held);
        // The in-band command waits for the hold: it is never written, so the hand-off is never
        // told of it and nothing flips the owner under the held keys.
        assert_eq!(handoff.holds_queued_writes(), switch_on);
        for key in &keys[2..] {
            assert_eq!(
                handoff.on_user_bytes(key),
                UserBytesRoute::Held,
                "switch_on={switch_on}: a later key never overtakes the held ones"
            );
        }
        assert!(
            !handoff.editor_owns_input(),
            "switch_on={switch_on}: the owner did not flip under the held keys"
        );
        assert!(handoff.is_pty_owned(), "switch_on={switch_on}");
        assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
        let mut written = Vec::new();
        for key in keys {
            if handoff.route_released_chunk(key) {
                written.push(key.to_vec());
            }
        }
        handoff.finish_queued_prompt_hold(id);
        assert_eq!(
            written,
            keys.map(|key| key.to_vec()).to_vec(),
            "switch_on={switch_on}"
        );
    }
}

/// #2182: after the held keys are written to the pty the line holds them; the in-band command
/// still waits (their request is due), and goes out once a request covered them.
#[test]
fn released_held_keys_keep_queued_writes_held_until_a_request_covers_them() {
    let mut handoff = conpty_awaiting_line_editor();
    assert_eq!(handoff.on_user_bytes(b"e"), UserBytesRoute::Held);
    let id = handoff.take_prompt_hold_started().expect("a hold started");
    handoff.view_keys_cell().publish(true);
    assert_eq!(handoff.release_queued_prompt_hold_on_signal(), Some(id));
    assert!(handoff.route_released_chunk(b"e"));
    handoff.finish_queued_prompt_hold(id);
    // The view's queue drained: nothing is held there; the byte sits in the shell's line.
    handoff.view_keys_cell().publish(false);
    assert!(
        handoff.holds_queued_writes(),
        "the shell's line holds the released byte"
    );
}

// ---- r29: held keys are never discarded (#2198), hand-back keeps cursor motion (#2199) ----------

/// #2198 (r29 A4): a held young Enter and the keys behind it hold every queued write of the
/// app's, so no command is written ahead of them; the kill switch of the in-band wait does not
/// turn that off. Shown behind the request, where nothing else holds queued writes (the line was
/// asked for, the pty no longer owns input): only the hold does, until the report ends it.
#[test]
fn a_queued_young_hold_holds_queued_writes_with_and_without_the_inband_switch() {
    for switch_on in [true, false] {
        let (mut handoff, _) = queued_young_hold();
        handoff.set_inband_wait_for_keys_enabled(switch_on);
        handoff.on_precmd();
        assert_eq!(
            handoff.on_line_editor_active(true, true),
            ActiveDecision::SendRequest
        );
        assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
        assert!(!handoff.view_holds_keys(), "nothing published by a view");
        assert!(handoff.young_hold_queued());
        assert!(
            handoff.holds_queued_writes(),
            "switch_on={switch_on}: nothing of the app's is written over a held Enter"
        );
        // The hold ends by its own event, the report; from then on it holds nothing.
        assert_eq!(
            handoff.on_report(),
            ReportDisposition::InsertThenSubmitYoung
        );
        assert!(!handoff.young_hold_queued());
        assert!(!handoff.holds_queued_writes(), "switch_on={switch_on}");
    }
}

/// #2198 (r29 A4), both causes of the loss at the hand-off: a command the hand-off does not own
/// (an in-band command; an agent or queued-panel command, whose `before_write_fn` makes the same
/// call) is written while a young Enter is held with keys behind it. The hold's end is
/// `FollowCommand`, never `Dropped`: the view writes the Enter and the keys behind it, in typed
/// order, behind the command, and they are counted as the lines and bytes they are.
#[test]
fn a_young_enter_and_the_keys_behind_it_survive_an_uncounted_command_in_order() {
    for cause in ["in_band_command", "held_agent_command"] {
        let (mut handoff, id) = queued_young_hold();
        // What the view's queue holds behind the gate: the Enter, then the keys typed after it.
        let held: Vec<Vec<u8>> = vec![b"\r".to_vec(), b"l".to_vec(), b"s".to_vec()];
        handoff.on_uncounted_command_written();
        assert!(!handoff.young_hold_queued(), "{cause}: the hold is over");
        let ends = handoff.take_young_hold_ends();
        assert_eq!(
            ends,
            vec![(id, YoungHoldEnd::FollowCommand)],
            "{cause}: the keys follow the command"
        );
        assert!(
            !ends.iter().any(|(_, end)| *end == flushed()),
            "{cause}: nothing is discarded"
        );
        // Until the view has written them nothing is decided over a line that looks shorter
        // than it will be.
        assert!(
            matches!(
                handoff.on_line_editor_active(true, true),
                ActiveDecision::Wait { .. }
            ),
            "{cause}"
        );
        // The view writes them as one ordered run and reports it.
        let written = held.concat();
        assert_eq!(written, b"\rls", "{cause}: typed order");
        handoff.count_followed_young_hold(id, &written);
        assert_eq!(
            handoff.state(),
            &HandoffState::PtyOwned {
                wrote_user_bytes: true,
                // The editor's command, the uncounted command, the held Enter.
                enters_written: 3,
                precmds_seen: 0,
            },
            "{cause}"
        );
        assert_eq!(handoff.writes_in_flight(), 1, "{cause}");
        assert_eq!(handoff.expire_queued_young_hold(id), YoungHoldExpiry::Gone);
    }
}

/// #2198 (r29 A4): while the request is out and the Enter is held behind it (`AwaitingReport`),
/// an uncounted command does not touch the hold: the request is ahead of the command in the tty,
/// so its report still runs the held Enter in the editor with the keys typed after it behind it.
/// (It used to record `Dropped` here too.)
#[test]
fn an_uncounted_command_behind_the_request_does_not_discard_a_held_enter() {
    let (mut handoff, id) = queued_young_hold();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(handoff.young_hold_queued());
    handoff.on_uncounted_command_written();
    assert!(handoff.young_hold_queued(), "the hold stays");
    assert!(handoff.take_young_hold_ends().is_empty(), "nothing ended");
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
}

/// #2199 (r29 A6): cursor motion in keys handed back to the editor is applied, in typed order,
/// as the shell's line editor applies it. `ab`, Left, `X` is `aXb` (it came back as `abX`).
#[test]
fn handed_back_keys_keep_cursor_motion() {
    // Left, in its CSI and SS3 forms and as ^B.
    assert_eq!(text_for_editor(b"ab\x1b[DX"), "aXb");
    assert_eq!(text_for_editor(b"ab\x1bODX"), "aXb");
    assert_eq!(text_for_editor(b"ab\x02X"), "aXb");
    // Right, and both clamped at the ends of the line.
    assert_eq!(text_for_editor(b"abc\x1b[D\x1b[D\x1b[CX"), "abXc");
    assert_eq!(text_for_editor(b"abc\x02\x02\x06X"), "abXc");
    assert_eq!(text_for_editor(b"ab\x1b[D\x1b[D\x1b[D\x1b[DX"), "Xab");
    assert_eq!(text_for_editor(b"ab\x1b[C\x1b[CX"), "abX");
    // Home and End keys.
    assert_eq!(text_for_editor(b"bc\x1b[Ha\x1b[Fd"), "abcd");
    assert_eq!(text_for_editor(b"bc\x1bOHa\x1bOFd"), "abcd");
    assert_eq!(text_for_editor(b"bc\x1b[1~a\x1b[4~d"), "abcd");
    // Backspace and Delete act at the cursor.
    assert_eq!(text_for_editor(b"abc\x1b[D\x7f"), "ac");
    assert_eq!(text_for_editor(b"abc\x1b[D\x1b[D\x1b[3~"), "ac");
    assert_eq!(text_for_editor(b"abc\x1b[3~"), "abc", "nothing under it");
    // Multi-byte characters are one cursor step each.
    assert_eq!(text_for_editor("éü\x1b[Dx".as_bytes()), "éxü");
    // Motion never leaves the line being typed, and Enter takes the whole line wherever the
    // cursor is.
    assert_eq!(text_for_editor(b"one\rtwo\x01\x02X"), "one\nXtwo");
    assert_eq!(text_for_editor(b"ab\x1b[D\rc"), "ab\nc");
    // The cursor the line editor is left with.
    assert_eq!(
        line_for_editor(b"ab\x1b[DX"),
        HandedBackLine {
            text: "aXb".to_owned(),
            cursor_back: 1,
        }
    );
    assert_eq!(line_for_editor(b"abc").cursor_back, 0);
    assert_eq!(line_for_editor(b"abc\x01").cursor_back, 3);
    assert_eq!(line_for_editor(b"abc\x01\x05").cursor_back, 0);
}

/// #2199 (r29 A6): ^A, ^E and ^K are applied (they were dropped), and ^U / ^W act at the cursor.
#[test]
fn handed_back_keys_apply_ctrl_a_ctrl_e_and_ctrl_k() {
    // ^A: what is typed next goes to the start of the line.
    assert_eq!(text_for_editor(b"bc\x01a"), "abc");
    assert_eq!(text_for_editor(b"world\x01hello "), "hello world");
    // ^E: back to the end.
    assert_eq!(text_for_editor(b"bc\x01a\x05d"), "abcd");
    // ^K: from the cursor to the end of the line.
    assert_eq!(text_for_editor(b"abcd\x1b[D\x1b[D\x0b"), "ab");
    assert_eq!(text_for_editor(b"abcd\x01\x0b"), "");
    assert_eq!(
        text_for_editor(b"abcd\x0b"),
        "abcd",
        "nothing after the cursor"
    );
    assert_eq!(text_for_editor(b"rm -rf x\x01\x0becho ok"), "echo ok");
    // ^K only kills the line being typed.
    assert_eq!(text_for_editor(b"one\rtwo\x01\x0b"), "one\n");
    // ^U: from the cursor back to the start of the line; ^W: the word before the cursor.
    assert_eq!(text_for_editor(b"abcd\x1b[D\x15"), "d");
    assert_eq!(
        text_for_editor(b"git commit now\x1b[D\x1b[D\x1b[D\x17"),
        "git now"
    );
    // ^D deletes the character under the cursor, as Delete does.
    assert_eq!(text_for_editor(b"abc\x01\x04"), "bc");
    // A hand-back that is only motion inserts nothing.
    assert!(!forces_shell_submit(&text_for_editor(
        b"\x01\x05\x1b[D\x0b"
    )));
}

/// #2202 fixture: an empty Enter's command was written (`\r`), then `keys` went to the pty raw one
/// at a time before its prompt (each noted as the view notes a raw user key, then counted and
/// flushed as `PtyController` does), then the prompt's request was written.
fn raw_keys_then_request(keys: &[&[u8]]) -> (TypeaheadHandoff, PtyWriteCount) {
    let mut handoff = TypeaheadHandoff::default();
    handoff.set_ssh_escape_cr_line_start_rule(true);
    let pty = handoff.pty_write_count();
    handoff.on_command_written();
    handoff.note_written_command_text("");
    pty.note_write_bytes(b"\r");
    handoff.age_last_activity_for_test();
    for key in keys {
        assert_eq!(handoff.on_user_bytes(key), UserBytesRoute::WriteCounted);
        handoff.note_raw_user_write(key);
        pty.note_write_bytes(key);
        handoff.on_user_write_flushed();
    }
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    pty.note_write_bytes(b"\x1bi");
    (handoff, pty)
}

/// #2202: a lone `~` that went to the pty raw right after a newline was read by ssh at its line
/// start; the hand-off remembers that until the report of the line is judged, once. T7: after that
/// report (it emptied the remote line) an escape written now reaches ssh at its line start, behind
/// the `\r` the escape path adds.
#[test]
fn a_raw_tilde_right_after_a_newline_is_remembered_until_its_report() {
    let (mut handoff, _pty) = raw_keys_then_request(&[b"~"]);
    handoff.on_report_parsed();
    assert!(matches!(
        handoff.on_report(),
        ReportDisposition::Insert | ReportDisposition::InsertYoung
    ));
    assert!(
        handoff.ssh_escape_would_reach_line_start(),
        "the report emptied the remote line: `\\r~X` starts a line for ssh"
    );
    assert!(
        handoff.take_raw_tilde_at_ssh_line_start(),
        "ssh read the `~` right after the Enter's newline"
    );
    assert!(
        !handoff.take_raw_tilde_at_ssh_line_start(),
        "the fact is used once"
    );
    // T7 (passes before the fix too): the existing rule adds the `\r`.
    assert_eq!(handoff.ssh_escape_line_start(), SshEscapeLineStart::AddCr);
}

/// #2202 (review A4): `a`, Backspace, `~` typed raw leave `~` in the remote line, but ssh never
/// saw that `~` at its line start. So does a `~` followed by another raw key, a key that is not a
/// lone `~`, and a `~` behind bytes the app wrote itself (a terminal-query reply).
#[test]
fn a_raw_tilde_that_ssh_did_not_read_at_its_line_start_is_not_remembered() {
    let cases: [&[&[u8]]; 4] = [&[b"a", b"\x7f", b"~"], &[b"~", b"x"], &[b"~~"], &[b"x"]];
    for keys in cases {
        let (mut handoff, _pty) = raw_keys_then_request(keys);
        assert!(
            !handoff.take_raw_tilde_at_ssh_line_start(),
            "{keys:?}: not a lone `~` right after a newline"
        );
    }

    // A reply the app wrote behind the newline: ssh last read that, not the newline.
    let mut handoff = TypeaheadHandoff::default();
    let pty = handoff.pty_write_count();
    handoff.on_command_written();
    pty.note_write_bytes(b"\r");
    pty.note_tail(b"\x1b[24;1R");
    assert_eq!(handoff.on_user_bytes(b"~"), UserBytesRoute::WriteCounted);
    handoff.note_raw_user_write(b"~");
    assert!(!handoff.take_raw_tilde_at_ssh_line_start());

    // A command written since (its own newline starts a new line; the `~` was that line's text).
    let (mut handoff, pty) = raw_keys_then_request(&[b"~"]);
    handoff.on_command_written();
    pty.note_write_bytes(b"ls\r");
    assert!(
        !handoff.take_raw_tilde_at_ssh_line_start(),
        "a command written since ends the fact"
    );
}

/// #2202 (review A3, item 4): while an Enter is deferred behind the report with keys held behind
/// its gate, an escape written now would not reach ssh at its line start (the escape path refuses
/// the `\r` there, `hold_after_enter`): the report stays text and the deferred Enter runs the line
/// as typed, in order.
#[test]
fn an_ssh_escape_would_not_reach_the_line_start_while_an_enter_is_deferred() {
    let (mut handoff, _pty) = raw_keys_then_request(&[b"~"]);
    assert!(handoff.defer_enter());
    assert!(!handoff.ssh_escape_would_reach_line_start());
    handoff.on_report_parsed();
    assert!(matches!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmit | ReportDisposition::InsertThenSubmitYoung
    ));
    assert!(
        !handoff.ssh_escape_would_reach_line_start(),
        "the deferred Enter's gate is live"
    );
    assert_eq!(
        handoff.ssh_escape_line_start(),
        SshEscapeLineStart::Skipped("hold_after_enter")
    );
}

/// #2207 fixture: `~` went to ssh raw at its line start, the prompt's request is out, an Enter
/// was deferred behind it, and the report arrived (the deferred submit is due). Returns the
/// hand-off, the pty write count and the deferred Enter's gate.
fn deferred_enter_behind_a_reported_tilde() -> (TypeaheadHandoff, PtyWriteCount, u64) {
    let (mut handoff, pty) = raw_keys_then_request(&[b"~"]);
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().expect("the Enter holds a gate");
    handoff.on_report_parsed();
    assert!(matches!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmit | ReportDisposition::InsertThenSubmitYoung
    ));
    assert!(handoff.take_raw_tilde_at_ssh_line_start());
    (handoff, pty, gate)
}

/// #2207: `~` raw, then its escape character and an Enter in the editor before the report. The
/// escape cannot wait for the Enter, and the Enter with the keys behind it must not go out in the
/// same run (ssh drops what it read together with `~.`). The one transition: the `\r~X` is
/// counted as a bare Enter's line, exactly as `ssh_escape_line_start`'s `AddCr` counts it, and
/// the deferred Enter becomes a young hold: it and the keys behind its gate wait for the next
/// prompt event. That hold's ends are the young hold's own: at the next prompt the request is
/// written and its report runs the Enter in the editor, with the keys held behind it.
#[test]
fn a_deferred_enter_behind_a_reported_ssh_escape_waits_for_the_next_prompt() {
    let (mut handoff, pty, gate) = deferred_enter_behind_a_reported_tilde();
    assert!(
        handoff.deferred_enter_could_wait_for_next_prompt(),
        "the report arrived, the deferred gate is the only thing in the way"
    );
    let (taken_gate, hold_id) = handoff
        .hold_deferred_enter_for_next_prompt(*b"~.")
        .expect("the deferred Enter waits for the next prompt");
    assert_eq!(taken_gate, gate, "the view turns this gate into the hold's");
    assert_eq!(handoff.deferred_gate(), None);
    assert!(handoff.has_young_hold());
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 1,
            precmds_seen: 0,
        },
        "the `\\r` of `\\r~.` is a bare Enter's line"
    );
    assert_eq!(
        handoff.writes_in_flight(),
        1,
        "`\\r~.` is one counted write"
    );
    assert!(
        !handoff.deferred_enter_could_wait_for_next_prompt(),
        "once: the deferral is over"
    );
    assert_eq!(handoff.hold_deferred_enter_for_next_prompt(*b"~."), None);

    // The controller writes `\r~.`; the next prompt comes (the remote shell's after `~?`, the
    // local shell's after `~.`): the request, then its report runs the held Enter in the editor.
    pty.note_write_bytes(b"\r~.");
    handoff.on_user_write_flushed();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(handoff.take_young_hold_ends().is_empty(), "still held");
    handoff.on_report_parsed();
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    let ends = handoff.take_young_hold_ends();
    assert_eq!(ends.len(), 1, "{ends:?}");
    assert_eq!(ends[0].0, hold_id);
    assert!(
        matches!(ends[0].1, YoungHoldEnd::SubmitOnReport { .. }),
        "{ends:?}"
    );
}

/// #2207 (owner decision 2026-11-13): the hold made for a reported ssh escape is never dropped.
/// When the session ends, or the state leaves the pty's ownership with the hold still queued (the
/// local shell's prompt after `~.`), its Enter and the keys behind it are written in typed order
/// to whatever owns input now, as a plain terminal delivers what was typed after ssh exits.
#[test]
fn the_hold_behind_a_reported_ssh_escape_is_written_not_dropped_when_its_session_goes() {
    // The session ends.
    let (mut handoff, _pty, _gate) = deferred_enter_behind_a_reported_tilde();
    let (_, hold_id) = handoff
        .hold_deferred_enter_for_next_prompt(*b"~.")
        .expect("the deferred Enter waits for the next prompt");
    handoff.reset_on_exit();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(hold_id, YoungHoldEnd::Typeahead)],
        "written in typed order, not dropped"
    );

    // The state left the pty's ownership with the hold still queued; the next key finds it.
    let (mut handoff, _pty, _gate) = deferred_enter_behind_a_reported_tilde();
    let (_, hold_id) = handoff
        .hold_deferred_enter_for_next_prompt(*b"~.")
        .expect("the deferred Enter waits for the next prompt");
    handoff.state = HandoffState::EditorOwned;
    assert!(handoff.check_queued_young_hold());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(hold_id, YoungHoldEnd::Typeahead)],
        "written in typed order, not dropped"
    );
}

/// #2209 (this was the #2207 guard `other_young_holds_are_still_dropped_when_their_session_goes`
/// and asserted `Dropped` twice): no young hold is dropped on those paths any more. One whose
/// pty died, or whose state left the pty's ownership, goes to the reader (`ToReader`: the pty in
/// typed order, or text in the input when the pty is gone); the escape's hold keeps its own end
/// (`Typeahead`, the tests above). A Ctrl-C still flushes the escape's hold, as the tty flushes
/// typeahead on an interrupt: that is the only discard left.
#[test]
fn other_young_holds_go_to_the_reader_when_their_session_goes() {
    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    handoff.reset_on_exit();
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );

    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    handoff.state = HandoffState::EditorOwned;
    assert!(handoff.check_queued_young_hold());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );

    // A signal: the user's interrupt voids what was typed ahead, the escape's hold included.
    // (Until #2209 this arm sat under an `if let Some` and passed without asserting anything
    // when the hold was not made.)
    let (mut handoff, _pty, _gate) = deferred_enter_behind_a_reported_tilde();
    let (_, hold_id) = handoff
        .hold_deferred_enter_for_next_prompt(*b"~.")
        .expect("the deferred Enter waits for the next prompt");
    let _ = handoff.on_user_bytes(b"\x03");
    assert_eq!(handoff.take_young_hold_ends(), vec![(hold_id, flushed())]);
}

/// #2207 (review F4): the hold made for a reported ssh escape ends at the EDGE where the editor
/// takes input back (the local prompt after `~.`), not lazily at the next key: its Enter and keys
/// are written then, before any command written later. An ordinary young hold is not ended there
/// (its stale check stays where it was).
#[test]
fn the_escape_hold_ends_at_the_edge_where_the_editor_takes_input_back() {
    let (mut handoff, _pty, _gate) = deferred_enter_behind_a_reported_tilde();
    let (_, hold_id) = handoff
        .hold_deferred_enter_for_next_prompt(*b"~.")
        .expect("the deferred Enter waits for the next prompt");
    assert!(handoff.is_escape_hold(hold_id));
    assert!(
        !handoff.end_escape_hold_if_unowned(),
        "the pty owns input: the hold waits for its prompt"
    );
    assert!(handoff.take_young_hold_ends().is_empty());
    handoff.force_editor_owned_for_test();
    assert!(handoff.end_escape_hold_if_unowned());
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(hold_id, YoungHoldEnd::Typeahead)],
        "written in typed order at the edge"
    );
    assert!(!handoff.end_escape_hold_if_unowned(), "once");

    let mut handoff = young_after_submit();
    type_and_flush(&mut handoff, b"abc");
    let (id, _) = hold_young_enter(&mut handoff, b"\r");
    assert!(!handoff.is_escape_hold(id));
    handoff.force_editor_owned_for_test();
    assert!(!handoff.end_escape_hold_if_unowned());
    assert!(
        handoff.take_young_hold_ends().is_empty(),
        "an ordinary young hold is not ended at the edge"
    );
}

/// #2207 (review F2): the view must still know the escape's hold when it applies the end the
/// session's exit recorded (it writes that hold's editor-typed keys to the pty too), so the tag
/// survives `reset_on_exit`.
#[test]
fn the_escape_hold_is_still_known_after_its_session_ended() {
    let (mut handoff, _pty, _gate) = deferred_enter_behind_a_reported_tilde();
    let (_, hold_id) = handoff
        .hold_deferred_enter_for_next_prompt(*b"~.")
        .expect("the deferred Enter waits for the next prompt");
    handoff.reset_on_exit();
    assert!(handoff.is_escape_hold(hold_id));
    assert!(!handoff.is_escape_hold(hold_id.wrapping_add(1)));
}

// ---- #2209: no typed key is dropped when a hold ends ------------------------------------------
//
// A key the hand-off holds leaves its gate in one of three ways: to the reader
// (`YoungHoldEnd::ToReader` / `Typeahead` / `FollowCommand` / `SubmitOnReport`), kept with its line
// (`WithLine`), or flushed by a signal (`FlushedBySignal`). These tests pin the labels; what the
// view does with each label is pinned by the view tests (`input_test.rs`, "#2209").

/// The one end that discards held keys. Only a signal path of the hand-off can build it
/// ([`SignalFlush`] is private to the module; this test module is a child of it).
fn flushed() -> YoungHoldEnd {
    YoungHoldEnd::FlushedBySignal(SignalFlush(()))
}

/// A young hold whose request has been written: the hold waits behind it (`AwaitingReport`).
fn young_hold_behind_its_request() -> (TypeaheadHandoff, u64) {
    let (mut handoff, id) = queued_young_hold();
    handoff.on_precmd();
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
    assert!(handoff.young_hold_queued());
    (handoff, id)
}

/// #2209, the structural rule: from every state a hold can be in, NO event that is not a signal
/// records the flushed end. (The signal paths are `on_user_bytes` with a signal byte,
/// `on_interrupt` and `cancel_deferred_enter`; they are the only callers of the two functions
/// that build `SignalFlush`.)
#[test]
fn no_event_but_a_signal_voids_held_keys() {
    type Fixture = fn() -> TypeaheadHandoff;
    let fixtures: [(&str, Fixture); 6] = [
        ("young hold, pty owned", || queued_young_hold().0),
        ("young hold behind its request", || {
            young_hold_behind_its_request().0
        }),
        ("young hold, editor owned (stale)", || {
            let (mut handoff, _) = queued_young_hold();
            handoff.state = HandoffState::EditorOwned;
            handoff
        }),
        ("young hold stranded behind a deferred Enter", || {
            let (mut handoff, _) = young_hold_behind_its_request();
            handoff.report_deferral_id = Some(7);
            handoff.deferred_gate = Some(DEFERRED_GATE_ID_BIT | 1);
            handoff.state = HandoffState::HoldAfterEnter { released: false };
            handoff
        }),
        ("Enter deferred behind the report", || {
            let mut handoff = awaiting_report();
            assert!(handoff.defer_enter());
            handoff
        }),
        ("deferred Enter whose submit ran", || {
            let mut handoff = awaiting_report();
            assert!(handoff.defer_enter());
            let _ = handoff.on_report();
            handoff
        }),
    ];
    type Event = fn(&mut TypeaheadHandoff);
    let events: [(&str, Event); 19] = [
        ("reset_on_exit", |h| h.reset_on_exit()),
        ("on_command_written", |h| h.on_command_written()),
        ("on_unengaged_user_command_written", |h| {
            h.on_unengaged_user_command_written()
        }),
        ("on_uncounted_command_written", |h| {
            h.on_uncounted_command_written()
        }),
        ("check_queued_young_hold", |h| {
            let _ = h.check_queued_young_hold();
        }),
        ("expire_queued_young_hold", |h| {
            let id = h.young_hold_id;
            let _ = h.expire_queued_young_hold(id);
        }),
        ("on_precmd", |h| h.on_precmd()),
        ("on_preexec(shell)", |h| {
            let _ = h.on_preexec(true);
        }),
        ("on_preexec(app)", |h| {
            let _ = h.on_preexec(false);
        }),
        ("on_report", |h| {
            let _ = h.on_report();
        }),
        ("give_up_report_deferral", |h| {
            let _ = h.give_up_report_deferral(7);
            if let Some(id) = h.report_deferral_id {
                let _ = h.give_up_report_deferral(id);
            }
        }),
        ("give_up_deferred_gate", |h| {
            if let Some(gate) = h.deferred_gate() {
                let _ = h.give_up_deferred_gate(gate);
            }
        }),
        ("release_after_unsubmitted_enter", |h| {
            let _ = h.release_after_unsubmitted_enter();
        }),
        ("on_stream_applied", |h| {
            let _ = h.on_stream_applied();
        }),
        ("note_command_interactive", |h| h.note_command_interactive()),
        ("try_commit_at_prompt", |h| {
            let _ = h.try_commit_at_prompt();
        }),
        ("on_line_editor_active", |h| {
            let _ = h.on_line_editor_active(true, true);
        }),
        ("a typed key", |h| {
            let _ = h.on_user_bytes(b"x");
        }),
        ("a typed Enter", |h| {
            let _ = h.on_user_bytes(b"\r");
        }),
    ];
    for (state, fixture) in fixtures {
        for (name, event) in events {
            let mut handoff = fixture();
            let _ = handoff.take_young_hold_ends();
            event(&mut handoff);
            let ends = handoff.take_young_hold_ends();
            assert!(
                !ends
                    .iter()
                    .any(|(_, end)| matches!(end, YoungHoldEnd::FlushedBySignal(_))),
                "{state}: {name} recorded a flush although it is not a signal: {ends:?}"
            );
        }
    }
    // The signals do record it, from the same states that hold a young Enter.
    for signal in [0x03u8, 0x1a, 0x1c] {
        let (mut handoff, id) = queued_young_hold();
        let _ = handoff.on_user_bytes(&[signal]);
        assert_eq!(handoff.take_young_hold_ends(), vec![(id, flushed())]);
    }
    let (mut handoff, id) = young_hold_behind_its_request();
    assert!(handoff.cancel_deferred_enter());
    assert_eq!(handoff.take_young_hold_ends(), vec![(id, flushed())]);
}

/// #2209: the view's busy-queue path may record a flush only for the hold a signal just took
/// (`take_queued_young_hold_for_signal`); asked for any other hold, nothing is recorded.
#[test]
fn a_flush_is_restored_only_for_the_hold_a_signal_took() {
    let (mut handoff, id) = queued_young_hold();
    handoff.restore_flushed_young_hold(id);
    assert!(
        handoff.take_young_hold_ends().is_empty(),
        "no signal took the hold: nothing is flushed"
    );
    assert!(handoff.young_hold_queued());
    assert_eq!(
        handoff.take_queued_young_hold_for_signal(&[0x03]),
        Some((id, false))
    );
    handoff.restore_flushed_young_hold(id.wrapping_add(1));
    assert!(
        handoff.take_young_hold_ends().is_empty(),
        "another hold's id"
    );
    // (The wrong id consumed the permission: one signal, one flush.)
    let (mut handoff, id) = queued_young_hold();
    assert_eq!(
        handoff.take_queued_young_hold_for_signal(&[0x03]),
        Some((id, false))
    );
    handoff.restore_flushed_young_hold(id);
    assert_eq!(handoff.take_young_hold_ends(), vec![(id, flushed())]);
    handoff.restore_flushed_young_hold(id);
    assert!(handoff.take_young_hold_ends().is_empty(), "once");
}

/// #2209 (review M6): a lone EOF is not a discard signal: the tty keeps typeahead across it. The
/// hold it takes is handed to the view to write in front of the EOF, and NO flush may be
/// recorded for it (the view's busy-queue path records `ToReader` instead).
#[test]
fn a_lone_eof_never_flushes_the_hold_it_takes() {
    let (mut handoff, id) = queued_young_hold();
    assert_eq!(
        handoff.take_queued_young_hold_for_signal(&[0x04]),
        Some((id, true))
    );
    handoff.restore_flushed_young_hold(id);
    assert!(
        handoff.take_young_hold_ends().is_empty(),
        "an EOF's hold cannot be recorded as flushed"
    );
    handoff.restore_young_hold_end(id, YoungHoldEnd::ToReader);
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );
    // An EOF together with an interrupt is the interrupt's flush.
    let (mut handoff, id) = queued_young_hold();
    assert_eq!(
        handoff.take_queued_young_hold_for_signal(&[0x04, 0x03]),
        Some((id, false))
    );
    handoff.restore_flushed_young_hold(id);
    assert_eq!(handoff.take_young_hold_ends(), vec![(id, flushed())]);
}

/// #2209 (review N1): `defer_enter` refuses only where the Enter has a gate to queue behind (a
/// young hold waiting behind its request). A hold still queued while the editor owns input with
/// an uncovered report outstanding is stale: refusing there would let the editor submit the
/// fragment it holds. The stale hold is ended (its keys delivered) and the Enter is deferred.
#[test]
fn an_enter_behind_an_uncovered_report_is_deferred_although_a_stale_hold_is_queued() {
    let (mut handoff, id) = queued_young_hold();
    handoff.state = HandoffState::EditorOwned;
    handoff.untracked_line_bytes = true;
    handoff.requests_outstanding = 1;
    assert_eq!(handoff.enter_queues_behind(Instant::now()), None);
    assert!(
        handoff.defer_enter(),
        "the Enter waits for the covering report, it does not submit a fragment"
    );
    assert_eq!(
        handoff.state(),
        &HandoffState::HoldAfterEnter { released: false }
    );
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)],
        "the stale hold's keys are delivered, not left behind the deferral"
    );
    assert!(!handoff.has_young_hold());
}

/// #2209 (H5, the #1016 editor-first race): an Enter that reaches the editor while a young Enter
/// waits behind the request is NOT deferred. It used to be: the state became `HoldAfterEnter`
/// with the hold still queued, the report's arm for that state never took the hold, and every
/// later path dropped its keys. The hand-off names the gate the Enter queues behind instead, and
/// the report still runs the held Enter with the keys behind it.
#[test]
fn an_enter_that_reaches_the_editor_behind_a_young_hold_is_not_deferred() {
    let now = Instant::now();
    let (mut handoff, id) = young_hold_behind_its_request();
    assert!(
        !handoff.defer_enter(),
        "the Enter is queued behind the hold, never deferred over it"
    );
    assert_eq!(handoff.state(), &HandoffState::AwaitingReport);
    assert!(handoff.young_hold_queued());
    assert_eq!(handoff.deferred_gate(), None);
    assert!(
        matches!(
            handoff.enter_queues_behind(now),
            Some(EnterQueuesBehind::YoungHold { hold_id, .. }) if hold_id == id
        ),
        "the Enter queues behind the hold's `PromptSeen` gate"
    );
    // The report runs the held Enter; the hold's gate becomes the deferred Enter's in place, so
    // the Enter queued behind it is then an entry behind a deferred Enter (the #1016 shape).
    assert_eq!(
        handoff.on_report(),
        ReportDisposition::InsertThenSubmitYoung
    );
    let ends = handoff.take_young_hold_ends();
    assert!(
        matches!(
            ends.as_slice(),
            [(hold, YoungHoldEnd::SubmitOnReport { .. })] if *hold == id
        ),
        "{ends:?}"
    );

    // An Enter deferred behind the report (no young hold): later Enters queue behind its gate.
    let mut handoff = awaiting_report();
    assert_eq!(handoff.enter_queues_behind(now), None);
    assert!(handoff.defer_enter());
    let gate = handoff.deferred_gate().expect("a gate");
    assert_eq!(
        handoff.enter_queues_behind(now),
        Some(EnterQueuesBehind::DeferredEnter { gate })
    );
    // Once its submit ran, an Enter reaching the editor is that submit itself: not queued.
    let _ = handoff.on_report();
    assert_eq!(handoff.enter_queues_behind(now), None);
    // Nothing held at all.
    assert_eq!(TypeaheadHandoff::default().enter_queues_behind(now), None);
}

/// #2209 (H4, the net behind H5): a young hold stranded behind a deferred Enter (a state only a
/// queue / hand-off disagreement can still produce) is never void when that deferral is given
/// up, by the report patience or by the gate's leak net: its Enter and keys go to the reader,
/// once.
#[test]
fn a_young_hold_stranded_behind_a_deferred_enter_survives_the_deferrals_give_up() {
    let stranded = || {
        let (mut handoff, id) = young_hold_behind_its_request();
        let gate = DEFERRED_GATE_ID_BIT | 1;
        handoff.report_deferral_id = Some(7);
        handoff.deferred_gate = Some(gate);
        handoff.state = HandoffState::HoldAfterEnter { released: false };
        (handoff, id, gate)
    };
    let (mut handoff, id, gate) = stranded();
    assert_eq!(handoff.give_up_report_deferral(7), Some(gate));
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );
    assert!(!handoff.has_young_hold());
    assert_eq!(handoff.give_up_report_deferral(7), None, "once");
    assert!(handoff.take_young_hold_ends().is_empty());

    let (mut handoff, id, gate) = stranded();
    assert!(handoff.give_up_deferred_gate(gate));
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );
    assert!(!handoff.give_up_deferred_gate(gate), "once");
}

/// #2209 (owner decision 2026-11-14): the pane's pty died while something was reading a secret.
/// The fact is kept across the reset, for the view to discard (and count) the keys held then
/// instead of showing them.
#[test]
fn a_secret_read_at_the_ptys_death_is_remembered_across_the_reset() {
    let (mut handoff, id) = queued_young_hold();
    assert!(!handoff.session_gone_reading_secret());
    handoff.note_session_ended_reading_secret();
    handoff.reset_on_exit();
    assert!(handoff.session_gone());
    assert!(handoff.session_gone_reading_secret());
    // The end is still recorded: the VIEW decides (it discards with a counted log line).
    assert_eq!(
        handoff.take_young_hold_ends(),
        vec![(id, YoungHoldEnd::ToReader)]
    );
    // Without a secret reader nothing is marked.
    let (mut handoff, _) = queued_young_hold();
    handoff.reset_on_exit();
    assert!(!handoff.session_gone_reading_secret());
}

/// #2209 (owner decision 2026-11-14, "keep every key"): bytes typed for the pty and replayed in
/// the input editor are NAMED key by key, none dropped: `line_for_editor` drops Up / Down, a lone
/// ESC and `^C`, and applies the cursor keys to a line of its own (clamping them at its start).
#[test]
fn bytes_replayed_in_the_editor_keep_every_key() {
    use EditorReplayKey as K;
    assert_eq!(
        editor_keys_of_bytes(b"ab\x1b[Dc\x1b[A\x1b[B\x1bOC\x1b[H\x1b[F\x1b[3~"),
        vec![
            K::Text("ab".to_owned()),
            K::Left,
            K::Text("c".to_owned()),
            K::Up,
            K::Down,
            K::Right,
            K::Home,
            K::End,
            K::Delete,
        ]
    );
    assert_eq!(
        editor_keys_of_bytes(b"x\x7f\x15\x0b\x17\t\x03\x1b"),
        vec![
            K::Text("x".to_owned()),
            K::Backspace,
            K::KillToStart,
            K::KillToEnd,
            K::KillWord,
            K::Tab,
            K::Interrupt,
            K::Escape,
        ]
    );
    // An Enter is a line break, in place; the text after it is its own run.
    assert_eq!(
        editor_keys_of_bytes(b"ls\rpwd\n"),
        vec![
            K::Text("ls".to_owned()),
            K::LineBreak,
            K::Text("pwd".to_owned()),
            K::LineBreak,
        ]
    );
    // A sequence the editor has no key for is named with its length, never skipped: F5
    // (`ESC [ 1 5 ~`, 5 bytes), Ctrl-Left (`ESC [ 1 ; 5 D`, 6 bytes), `^Z`.
    assert_eq!(
        editor_keys_of_bytes(b"a\x1b[15~\x1b[1;5D\x1ab"),
        vec![
            K::Text("a".to_owned()),
            K::NoEditorKey(5),
            K::NoEditorKey(6),
            K::NoEditorKey(1),
            K::Text("b".to_owned()),
        ]
    );
    // Emacs mode's previous / next line are Up and Down (review M2); `^C` is named so the
    // caller can count it, and is never replayed (review M5).
    assert_eq!(
        editor_keys_of_bytes(b"a\x10\x0e\x03"),
        vec![K::Text("a".to_owned()), K::Up, K::Down, K::Interrupt]
    );
    // Owner decision 2026-11-14: Page Up, Page Down and Shift-Tab have editor actions and
    // replay as themselves; Insert (`ESC [ 2 ~`) and the modified forms stay counted.
    assert_eq!(
        editor_keys_of_bytes(b"a\x1b[5~\x1b[6~\x1b[Zb\x1b[2~\x1b[5;5~"),
        vec![
            K::Text("a".to_owned()),
            K::PageUp,
            K::PageDown,
            K::ShiftTab,
            K::Text("b".to_owned()),
            K::NoEditorKey(4),
            K::NoEditorKey(6),
        ]
    );
    // The cursor keys are NOT clamped here (that is #2115: four Lefts stay four Lefts, for the
    // editor to apply against the joined line), where `line_for_editor` on a one-character line
    // loses three of them.
    assert_eq!(
        editor_keys_of_bytes(b"t\x1b[D\x1b[D\x1b[D\x1b[Dy"),
        vec![
            K::Text("t".to_owned()),
            K::Left,
            K::Left,
            K::Left,
            K::Left,
            K::Text("y".to_owned()),
        ]
    );
    assert!(editor_keys_of_bytes(b"").is_empty());
}

/// r30f (`lab_connect_typeahead`, macOS lab, agent mode): the ssh connect hold was released at
/// the first remote prompt in the same pass that committed the prompt to the editor, and its
/// keys were delivered to the pty. The hand-off, editor-owned, only noted them as untracked
/// bytes; the rest of the line went to the editor and its Enter submitted that rest alone. The
/// delivered keys now take the line for the pty: they are counted as line content, the rest of
/// the line follows them to the pty, and the line is handed to the editor whole (the request and
/// its report) or stays the pty's until its Enter when no request can be made.
#[test]
fn delivered_ssh_typeahead_takes_the_prompts_line_for_the_pty() {
    let mut handoff = TypeaheadHandoff::default();
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);
    assert!(handoff.take_prompt_line_for_delivered_typeahead());
    assert!(handoff.routes_keys_to_pty(), "the line is the pty's");
    for key in b"echo T4_yng" {
        assert_eq!(
            type_and_flush(&mut handoff, &[*key]),
            UserBytesRoute::WriteCounted,
            "a delivered key is line content, not an untracked byte"
        );
    }
    assert_eq!(
        handoff.state(),
        &HandoffState::PtyOwned {
            wrote_user_bytes: true,
            enters_written: 0,
            precmds_seen: 0,
        }
    );
    // The rest of the line, typed live, follows the part already there.
    assert_eq!(
        type_and_flush(&mut handoff, b" >> ~/rl.txt"),
        UserBytesRoute::WriteCounted
    );
    // No request can be made (the line editor is not proven reading): the pty keeps the line,
    // it is never committed to the editor in two halves (#2197).
    assert_eq!(
        handoff.on_line_editor_active(true, false),
        ActiveDecision::Drain
    );
    assert!(handoff.is_pty_owned());
    assert!(
        !handoff.try_commit_at_prompt(),
        "a typed line is in the shell"
    );
    // With a request the whole line comes back in the shell's report.
    assert_eq!(
        handoff.on_line_editor_active(true, true),
        ActiveDecision::SendRequest
    );
    assert!(matches!(
        handoff.on_report(),
        ReportDisposition::Insert | ReportDisposition::InsertYoung
    ));
    assert_eq!(handoff.state(), &HandoffState::EditorOwned);

    // Only from an editor-owned prompt: a request outstanding, or a line the pty already owns,
    // is left as it is.
    let mut waiting = awaiting_report();
    assert!(!waiting.take_prompt_line_for_delivered_typeahead());
    assert_eq!(waiting.state(), &HandoffState::AwaitingReport);
    let mut owned = after_submit();
    let before = owned.state().clone();
    assert!(!owned.take_prompt_line_for_delivered_typeahead());
    assert_eq!(owned.state(), &before);
}
