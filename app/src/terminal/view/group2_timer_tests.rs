//! Group 2: timers close a segment and hand text back; they never write to a pty and never run a
//! command (#1577 / #1588). Child of `view_test.rs` (private helpers come in through `super`).

use super::*;
use crate::terminal::input_queue;
use crate::terminal::input_queue::DestinationMode;
use crate::terminal::view::paste_queue;
use crate::terminal::view::{AfterEnterInput, PowerShellSetupHold};

/// Records every event that writes to the pty, by name.
fn record_pty_writes(
    app: &mut App,
    terminal: &ViewHandle<TerminalView>,
) -> Rc<RefCell<Vec<&'static str>>> {
    let seen: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    app.update(|ctx| {
        ctx.subscribe_to_view(terminal, move |_, event, _| match event {
            Event::WriteLineClearSetupToPty { .. } => sink.borrow_mut().push("setup"),
            Event::WriteBytesToPty { .. } => sink.borrow_mut().push("bytes"),
            Event::WriteHandoffUserBytesToPty { .. } => sink.borrow_mut().push("handoff"),
            Event::WriteAgentInputToPty { .. } => sink.borrow_mut().push("agent"),
            Event::ExecuteCommand(_) => sink.borrow_mut().push("execute"),
            _ => {}
        });
    });
    seen
}

/// A1 (#1588): the 2 s net for a pending command with no prompt runs NOTHING. The command stays
/// pending (the prompt's own event runs it), its text is visible in the input, and the next Enter
/// is consumed once with the toast.
#[test]
fn pending_command_precmd_give_up_runs_nothing_and_keeps_the_command_pending() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                view.set_pending_command("echo late", ctx);
                // The state `BlockCompleted` leaves while it waits for the prompt.
                view.pending_command_awaits_precmd = true;
                // The user-visible text may have been cleared by something else: the give-up
                // shows it again.
                view.input.update(ctx, |input, ctx| {
                    input.clear_buffer_and_reset_undo_stack(ctx);
                });
                view.pending_command_precmd_give_up(ctx);
            });
            terminal.read(&app, |view, ctx| {
                assert!(
                    view.pending_command_awaits_precmd,
                    "the prompt's event still owns the run"
                );
                view.input.read(ctx, |input, ctx| {
                    assert!(input.has_pending_command());
                    assert_eq!(input.buffer_text(ctx), "echo late");
                    assert!(input.ssh_handover_enter_guard_armed());
                });
            });
            assert!(
                writes.borrow().is_empty(),
                "a timer ran or wrote something: {:?}",
                writes.borrow()
            );
        });
    }
}

/// The token of the open gate of `kind`, and the editor's buffer text.
fn gate_token_of(view: &TerminalView, kind: input_queue::GateKind) -> input_queue::GateToken {
    view.paste_queue
        .queue
        .borrow()
        .gates()
        .iter()
        .find(|gate| gate.kind == kind)
        .map(|gate| gate.token)
        .expect("the gate is open")
}

/// What a give-up must leave: the held key `expected` in the editor, the Enter guard armed,
/// nothing written to the pty, the queue idle.
fn assert_handed_back(
    terminal: &ViewHandle<TerminalView>,
    app: &App,
    writes: &Rc<RefCell<Vec<&'static str>>>,
    expected: &str,
) {
    assert!(
        writes.borrow().is_empty(),
        "a timer wrote to the pty: {:?}",
        writes.borrow()
    );
    terminal.read(app, |view, ctx| {
        view.input.read(ctx, |input, ctx| {
            assert_eq!(input.buffer_text(ctx), expected);
            assert!(input.ssh_handover_enter_guard_armed());
        });
        assert!(ssh_queue_is_idle(view), "the queue still holds entries");
    });
}

/// A3 (#1577): the 20 s leak net of a paced CLI-agent submit hands the keys typed behind it back
/// to the editor; it no longer drains them to the pty.
#[test]
fn cli_submit_cap_hands_the_keys_behind_it_back() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                view.begin_cli_submit_gate(ctx);
                assert!(view.write_user_bytes_to_pty(b"l".to_vec(), ctx));
                let token = gate_token_of(view, input_queue::GateKind::CliSubmit);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_handed_back(&terminal, &app, &writes, "l");
        });
    }
}

/// A3 (#1577): the cap of a pty paste whose clipboard read never answered discards the paste (as
/// before) and hands the keys typed behind it back; nothing is written.
#[test]
fn paste_cap_hands_the_keys_behind_the_unread_paste_back() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                let id = view
                    .paste_queue
                    .queue
                    .next_paste_id(input_queue::PasteOwner::Pty);
                {
                    let queue = view.paste_queue.queue.clone();
                    let mut host = paste_queue::PtyQueueHost::new(view, ctx);
                    queue.lock().offer_gated(
                        &mut host,
                        input_queue::EntryKind::Paste {
                            paste_id: id,
                            payload: None,
                        },
                        input_queue::GateSpec::new(input_queue::GateKind::PasteReading, id),
                    );
                }
                assert!(view.write_user_bytes_to_pty(b"b".to_vec(), ctx));
                let token = gate_token_of(view, input_queue::GateKind::PasteReading);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_handed_back(&terminal, &app, &writes, "b");
        });
    }
}

/// A3 (#1577): the 10 s leak net of an Enter's gate hands the text typed behind the Enter back;
/// the queue's own give-up used to deliver it through the stages.
#[test]
fn enter_gate_leak_net_hands_the_keys_behind_the_enter_back() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            let input = terminal.read(&app, |view, _| view.input.clone());
            input.update(&mut app, |input, ctx| {
                input.clear_buffer_and_reset_undo_stack(ctx);
                input.open_submitted_enter_gate_for_test(
                    vec![AfterEnterInput::Text("x".to_owned())],
                    false,
                    ctx,
                );
            });
            terminal.update(&mut app, |view, ctx| {
                let token = gate_token_of(view, input_queue::GateKind::EnterVerdict);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_handed_back(&terminal, &app, &writes, "x");
        });
    }
}

/// A3 (#1577): the cap of the PowerShell setup gate treats setup as over and hands the held keys
/// back; it no longer writes them to the pty.
#[test]
fn powershell_setup_gate_cap_hands_the_held_keys_back() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                announce_powershell_setup(view);
                assert!(view.write_user_bytes_to_pty(b"l".to_vec(), ctx));
                let token = gate_token_of(view, input_queue::GateKind::PowershellSetup);
                view.on_paste_queue_give_up(token, ctx);
                assert_eq!(view.powershell_setup_hold, PowerShellSetupHold::Off);
            });
            assert_handed_back(&terminal, &app, &writes, "l");
        });
    }
}

/// A3 (#1577): the cap of the bootstrap-write gate, once the paced lines are out, hands the keys
/// typed behind them back (while lines are still being written it keeps waiting, #1399).
#[test]
fn bootstrap_write_gate_cap_hands_the_held_keys_back_once_the_lines_are_out() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                start_subshell_bootstrap(view, false);
                assert!(view.write_user_bytes_to_pty(b"e".to_vec(), ctx));
                let token = gate_token_of(view, input_queue::GateKind::SubshellBootstrapWrite);
                view.subshell_bootstrap_in_flight = None;
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_handed_back(&terminal, &app, &writes, "e");
        });
    }
}

/// A4 (#1577): a young Enter hold whose prompt never comes hands the Enter and the keys behind it
/// back to the editor (guard armed, nothing written), in both destination modes.
#[test]
fn young_hold_cap_hands_the_enter_and_keys_back_with_the_guard_armed() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                young_enter_setup(view);
                assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
                assert!(young_gate_open(view), "the Enter is held");
                let token = gate_token_of(view, input_queue::GateKind::PromptSeen);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_handed_back(&terminal, &app, &writes, "\nls");
        });
    }
}

/// A6 (#1577): the uncovered-report patience (3 s) no longer RUNS the deferred Enter. The text
/// stays in the editor, the keys held behind the Enter follow it, nothing is written or executed.
#[test]
fn uncovered_report_patience_keeps_the_text_instead_of_running_the_enter() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                let id = {
                    let mut model = view.model.lock();
                    let mut handoff = model.typeahead_handoff_mut();
                    handoff.on_command_written();
                    handoff.on_user_bytes(b"ec");
                    handoff.on_user_write_flushed();
                    handoff.on_precmd();
                    assert_eq!(
                        handoff.on_line_editor_active(true, true),
                        ActiveDecision::SendRequest
                    );
                    handoff.note_untracked_line_bytes();
                    // The Enter is deferred behind the outstanding request (the report deferral);
                    // that request's report misses the untracked bytes, so the Enter stays
                    // deferred behind an UNCOVERED report and arms the patience timer (#1104;
                    // `defer_enter` arms it directly only from the editor-owned state).
                    assert!(handoff.defer_enter());
                    assert_eq!(
                        handoff.on_report(),
                        crate::terminal::model::typeahead_handoff::ReportDisposition::InsertKeepDeferred
                    );
                    handoff
                        .take_uncovered_deferral_started()
                        .expect("the patience timer is armed")
                };
                view.input
                    .update(ctx, |input, ctx| input.replace_buffer_content("ho 47", ctx));
                view.write_user_bytes_to_pty(b"ls".to_vec(), ctx);
                view.uncovered_report_patience_fired(id, ctx);
                // A second firing changes nothing.
                view.uncovered_report_patience_fired(id, ctx);
                let buffer = view.input.as_ref(ctx).buffer_text(ctx);
                assert!(buffer.starts_with("ho 47"), "{buffer:?}");
                assert!(
                    buffer.contains("ls"),
                    "the key behind the Enter is back: {buffer:?}"
                );
                assert!(view.paste_queue.queue.borrow().is_idle());
            });
            assert!(
                writes.borrow().is_empty(),
                "nothing ran or was written: {:?}",
                writes.borrow()
            );
        });
    }
}

/// A8 (#1577): the ssh connect window's give-up timer hands the held keys back as text even when
/// the remote's prompt was seen (that prompt's own event hands over; a timer never delivers). The
/// held `echo hi` shows in the input, not run, and nothing is written to the pty.
#[test]
fn ssh_give_up_with_the_remote_prompt_seen_hands_back_instead_of_delivering() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                submit_command(view, "ssh host", ctx);
            });
            // The user's own Enter on `ssh host` emitted ExecuteCommand (delivered when the update
            // above ended, as every emit is): setup, not the timer. Only what happens from here on
            // is the give-up's.
            writes.borrow_mut().clear();
            terminal.update(&mut app, |view, ctx| {
                view.typed_characters_on_terminal("echo hi", ctx);
                assert!(view.ssh_typeahead_holding);
                // The remote prompt was seen: its first Precmd is processed and the session can take
                // a command (#1572). Without that the hand-over waits on the readiness predicate
                // (its own 15 s cap), which is not what this test is about.
                view.ssh_remote_prompt_seen = true;
                first_precmd_processed_for_test(view, ctx);
                let gate = view.ssh_connect_gate.as_mut().unwrap();
                gate.armed_at =
                    Instant::now() - SSH_CONNECT_SILENCE_CEILING - Duration::from_secs(1);
                view.ssh_give_up_fired(ctx);
                assert!(!view.ssh_typeahead_holding);
                assert!(!view.ssh_give_up_in_progress);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "echo hi");
                assert!(view.input.as_ref(ctx).ssh_handover_enter_guard_armed());
            });
            assert!(
                writes.borrow().is_empty(),
                "a timer delivered to the pty: {:?}",
                writes.borrow()
            );
        });
    }
}

/// A9 (#1577): the 2 s grace behind a running PowerShell held line writes nothing (log + toast
/// only); the command becoming interactive releases the keys typed after it, once, in order.
#[test]
fn powershell_held_line_grace_writes_nothing_and_the_interactive_event_releases() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let delivered = collect_delivered_input(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                announce_powershell_setup(view);
                view.powershell_setup_hold = PowerShellSetupHold::LineRunning { generation: 1 };
                view.hold_powershell_keys(b"ls".to_vec(), true, ctx);
                view.end_powershell_held_line_wait(1, ctx);
                assert_eq!(
                    view.powershell_setup_hold,
                    PowerShellSetupHold::LineRunning { generation: 1 },
                    "still waiting"
                );
            });
            assert!(
                delivered.borrow().is_empty(),
                "the grace wrote: {:?}",
                delivered.borrow()
            );
            terminal.update(&mut app, |view, ctx| {
                let block_id = view.model.lock().active_block_id().clone();
                view.handle_terminal_event(&ModelEvent::CommandBecameInteractive { block_id }, ctx);
                assert_eq!(view.powershell_setup_hold, PowerShellSetupHold::Off);
            });
            assert_eq!(*delivered.borrow(), b"ls".to_vec(), "once, in order");
        });
    }
}

/// A9 (#1577): the 5 s marker give-up after a Ctrl-C that skipped a PowerShell setup hands the
/// held keys back to the editor; it no longer writes them to the pty.
#[test]
fn powershell_abort_marker_give_up_hands_the_held_keys_back() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let delivered = collect_delivered_input(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                announce_powershell_setup(view);
                view.hold_powershell_keys(b"ls".to_vec(), true, ctx);
                view.powershell_setup_hold = PowerShellSetupHold::AfterAbort { generation: 1 };
                view.give_up_powershell_abort_wait(1, ctx);
                assert_eq!(view.powershell_setup_hold, PowerShellSetupHold::Off);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "ls");
                assert!(view.input.as_ref(ctx).ssh_handover_enter_guard_armed());
                // A late second firing for the same wait changes nothing.
                view.give_up_powershell_abort_wait(1, ctx);
            });
            assert!(
                delivered.borrow().is_empty(),
                "the give-up wrote: {:?}",
                delivered.borrow()
            );
        });
    }
}

/// #1438: after a Ctrl-C skipped a PowerShell setup that has no read marker (the timed setup),
/// the settle timer is a give-up: it hands the held keys back to the editor, never writes them.
#[test]
fn powershell_abort_settle_without_a_marker_hands_the_held_keys_back() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let delivered = collect_delivered_input(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                announce_powershell_setup(view);
                assert!(
                    view.powershell_setup_marker_token.is_none(),
                    "a timed setup"
                );
                view.hold_powershell_keys(b"ls".to_vec(), true, ctx);
                view.powershell_setup_hold = PowerShellSetupHold::AfterAbort { generation: 1 };
                // A stale firing for an earlier wait changes nothing.
                view.give_up_unmarked_powershell_abort_wait(0, ctx);
                assert_eq!(
                    view.powershell_setup_hold,
                    PowerShellSetupHold::AfterAbort { generation: 1 }
                );
                view.give_up_unmarked_powershell_abort_wait(1, ctx);
                assert_eq!(view.powershell_setup_hold, PowerShellSetupHold::Off);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "ls");
                assert!(view.input.as_ref(ctx).ssh_handover_enter_guard_armed());
            });
            assert!(
                delivered.borrow().is_empty(),
                "the settle give-up wrote: {:?}",
                delivered.borrow()
            );
        });
    }
}

/// #1438 (review): the same through the real path. Ctrl-C skips a stalled timed PowerShell setup
/// with `ls` held and `x` typed after the ^C; the settle timer fires and both come back to the
/// editor as `lsx`, in order. Only the ^C ever reaches the pty.
#[test]
fn ctrl_c_on_a_timed_powershell_setup_hands_the_held_and_later_keys_back_after_the_settle() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let delivered = collect_delivered_input(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                announce_powershell_setup(view);
                stall_powershell_setup(view);
                assert!(
                    view.powershell_setup_marker_token.is_none(),
                    "a timed setup"
                );
                view.typed_characters_on_terminal("ls", ctx);
                view.ctrl_c(ctx);
                view.typed_characters_on_terminal("x", ctx);
                assert!(
                    matches!(
                        view.powershell_setup_hold,
                        PowerShellSetupHold::AfterAbort { .. }
                    ),
                    "{mode:?}: waiting after the abort"
                );
            });
            assert_eq!(
                *delivered.borrow(),
                b"\x03".to_vec(),
                "{mode:?}: only the ^C"
            );
            warpui::r#async::Timer::after(POWERSHELL_ABORT_SETTLE + Duration::from_millis(300))
                .await;
            terminal.read(&app, |view, ctx| {
                assert_eq!(
                    view.powershell_setup_hold,
                    PowerShellSetupHold::Off,
                    "{mode:?}"
                );
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "lsx", "{mode:?}");
            });
            assert_eq!(
                *delivered.borrow(),
                b"\x03".to_vec(),
                "{mode:?}: the give-up wrote nothing"
            );
        });
    }
}

/// A10 (#1577): the stall ceiling (a subshell setup that never finishes) hands the keys held
/// through it back to the editor as text, resets the setup state and the waiting notice, and
/// writes nothing into the abandoned shell. (The Precmd event of a setup that does finish still
/// releases them in order: `keys_held_through_a_stall_are_released_once_...`.)
#[test]
fn stall_ceiling_hands_the_keys_held_through_the_setup_back() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                view.model
                    .lock()
                    .simulate_long_running_block("zsh", " set | command grep x; true\r\n");
                let block_id = view.model.lock().block_list().active_block().id().clone();
                view.subshell_bootstrap_announced = Some((block_id, Instant::now()));
                assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
                assert!(view.subshell_setup_gate_open(), "the keys are held");
                view.subshell_setup_waiting_toast_id = Some("waiting-test".to_string());
                view.give_up_setup_gates_to_editor("stall_ceiling", ctx);
                assert!(view.subshell_bootstrap_announced.is_none());
                assert!(view.subshell_setup_waiting_toast_id.is_none());
                assert!(!view.subshell_setup_gate_open());
                assert_eq!(view.powershell_setup_hold, PowerShellSetupHold::Off);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "ls");
                assert!(view.input.as_ref(ctx).ssh_handover_enter_guard_armed());
            });
            assert!(
                writes.borrow().is_empty(),
                "the ceiling wrote into the abandoned shell: {:?}",
                writes.borrow()
            );
        });
    }
}

/// #1119: an abandoned subshell bootstrap is never silent. The abandon path shows its own
/// "Phosphorize failed" notice (held keys or not), so the stall ceiling's hand-back adds no
/// second toast; every other give-up keeps the generic one.
#[test]
fn stall_ceiling_hand_back_defers_to_the_abandon_notice() {
    assert_eq!(paste_queue::gate_give_up_toast("stall_ceiling"), None);
    assert!(paste_queue::gate_give_up_toast("subshell_setup_cap").is_some());
    assert!(paste_queue::gate_give_up_toast("bootstrap_write_cap").is_some());
    let notice = paste_queue::SUBSHELL_BOOTSTRAP_ABANDONED_TOAST;
    assert!(notice.contains("Phosphorize failed"));
    assert!(notice.contains("in the input, not run"));
}
