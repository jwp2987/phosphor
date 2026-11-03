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
