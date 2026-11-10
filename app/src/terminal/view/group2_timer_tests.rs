//! Group 2: what a timer does when it closes a segment (#1577 / #1588). A timer never runs a
//! command through the editor. Since the owner's decision of 2026-11-09 (REVERSING #1661) a
//! give-up of a hold for the shell (an Enter gate, a young Enter's hold, a paced CLI-agent submit)
//! WRITES the keys it held to the pty in typed order, what a plain terminal would have sent all
//! along; the tests below that expected those keys discarded (#1661) or handed back with a toast
//! (#1577) were changed for that decision. The setup gates and a paste's read cap still hand
//! their keys back to the editor. Child of `view_test.rs` (private helpers come in through
//! `super`).

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

/// Owner decision 2026-11-09 (REVERSING #1661, under which this test expected a discard): the
/// 20 s leak net of a paced CLI-agent submit WRITES the keys typed behind it to the pty, in typed
/// order (two lines with their Enters here), for the program that reads the tty. Nothing goes to
/// the editor and nothing is dropped. Both modes.
#[test]
fn cli_submit_cap_writes_the_keys_behind_it_to_the_pty_in_typed_order() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                view.begin_cli_submit_gate(ctx);
                for key in [&b"l"[..], b"s", b"\r", b"pwd", b"\r"] {
                    assert!(view.write_user_bytes_to_pty(key.to_vec(), ctx));
                }
                assert!(
                    pty_writes_are_empty(&pty),
                    "{mode:?}: held behind the submit"
                );
                let token = gate_token_of(view, input_queue::GateKind::CliSubmit);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_given_up_to_pty(&terminal, &app, &pty, b"ls\rpwd\r");
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

/// The text of every toast shown from now on. The `ToastStack` singleton must be registered.
fn record_toasts(app: &mut App) -> Rc<RefCell<Vec<String>>> {
    let texts = Rc::new(RefCell::new(Vec::new()));
    let sink = texts.clone();
    app.update(|ctx| {
        let stack = crate::workspace::ToastStack::handle(ctx);
        ctx.subscribe_to_model(&stack, move |_, event, _| {
            if let crate::workspace::ToastStackEvent::AddEphemeralToast { toast, .. } = event {
                sink.borrow_mut()
                    .push(toast.main_text_for_test().to_string());
            }
        });
    });
    texts
}

/// Whether nothing has been written to the pty yet.
fn pty_writes_are_empty(pty: &Rc<RefCell<Vec<Vec<u8>>>>) -> bool {
    pty.borrow().iter().all(|write| write.is_empty())
}

/// What a give-up of a hold for the shell must leave (owner decision 2026-11-09, reversing #1661):
/// the held keys at the pty, `expected` byte for byte and in typed order; nothing of them in the
/// editor and no guard (nothing came back); no toast about discarded keys; the queue idle.
fn assert_given_up_to_pty(
    terminal: &ViewHandle<TerminalView>,
    app: &App,
    pty: &Rc<RefCell<Vec<Vec<u8>>>>,
    expected: &[u8],
) {
    assert_eq!(
        String::from_utf8_lossy(&pty.borrow().concat()),
        String::from_utf8_lossy(expected),
        "the held keys reach the pty in typed order, none dropped"
    );
    terminal.read(app, |view, ctx| {
        view.input.read(ctx, |input, ctx| {
            assert_eq!(input.buffer_text(ctx), "", "nothing went to the editor");
            assert!(!input.ssh_handover_enter_guard_armed());
            assert!(!input.enter_queue_holds_keys(ctx));
        });
        assert!(ssh_queue_is_idle(view), "the queue still holds entries");
    });
}

/// Owner decision 2026-11-09 (REVERSING #1661, under which this test expected a discard): the
/// leak net of an Enter's gate WRITES what was typed behind the Enter (text typed in the editor
/// and a pty key) to the pty in typed order; it is never put into the editor. Both modes.
#[test]
fn enter_gate_leak_net_writes_the_keys_behind_the_enter_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let input = terminal.read(&app, |view, _| view.input.clone());
            input.update(&mut app, |input, ctx| {
                input.clear_buffer_and_reset_undo_stack(ctx);
                input.open_submitted_enter_gate_for_test(
                    vec![AfterEnterInput::Text("x".to_owned())],
                    false,
                    ctx,
                );
                // A key typed for the pty behind the Enter (terminal focus).
                input.offer_pty_bytes_through_editor_host_for_test(b"y".to_vec(), ctx);
            });
            terminal.update(&mut app, |view, ctx| {
                let token = gate_token_of(view, input_queue::GateKind::EnterVerdict);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_given_up_to_pty(&terminal, &app, &pty, b"xy");
        });
    }
}

/// Owner decision 2026-11-09 (REVERSING #1661, under which this test expected a discard): the
/// leak net of an Enter gate the input no longer tracks writes what waited behind it to the pty
/// too, in typed order (the Enter as `\r`); it is never put into the editor. Both modes.
#[test]
fn orphan_enter_gate_leak_net_writes_the_keys_behind_it_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let input = terminal.read(&app, |view, _| view.input.clone());
            input.update(&mut app, |input, ctx| {
                input.clear_buffer_and_reset_undo_stack(ctx);
                input.open_submitted_enter_gate_for_test(
                    vec![
                        AfterEnterInput::Text("x".to_owned()),
                        AfterEnterInput::Enter,
                    ],
                    false,
                    ctx,
                );
                input.forget_enter_gate_for_test();
            });
            terminal.update(&mut app, |view, ctx| {
                let token = gate_token_of(view, input_queue::GateKind::EnterVerdict);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_given_up_to_pty(&terminal, &app, &pty, b"x\r");
        });
    }
}

/// "Keep them together" (owner decision 2026-11-09), the untracked-gate shape: the leak net of
/// an Enter gate the input no longer tracks, with a LINE in the editor. Whether that Enter ran
/// is not known any more, so what waited behind it must not run ahead of a line that may not
/// have: it stays with the line, on the line after it, in typed order, under the hand-back
/// guard, and nothing is written to the pty. (With an empty editor there is no such line and the
/// keys go to the pty: `orphan_enter_gate_leak_net_writes_the_keys_behind_it_to_the_pty`.) Both
/// modes.
#[test]
fn orphan_enter_gate_leak_net_keeps_the_keys_with_a_line_in_the_editor() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let input = terminal.read(&app, |view, _| view.input.clone());
            input.update(&mut app, |input, ctx| {
                input.replace_buffer_content("cd build", ctx);
                input.open_submitted_enter_gate_for_test(
                    vec![
                        AfterEnterInput::Text("make clean".to_owned()),
                        AfterEnterInput::Enter,
                    ],
                    false,
                    ctx,
                );
                input.forget_enter_gate_for_test();
            });
            terminal.update(&mut app, |view, ctx| {
                let token = gate_token_of(view, input_queue::GateKind::EnterVerdict);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert!(
                pty_writes_are_empty(&pty),
                "{mode:?}: `make clean` must not run without `cd build`: {:?}",
                pty.borrow()
            );
            terminal.read(&app, |view, ctx| {
                view.input.read(ctx, |input, ctx| {
                    assert_eq!(
                        input.buffer_text(ctx),
                        "cd build\nmake clean\n",
                        "{mode:?}: the keys stay with the line, in typed order"
                    );
                    assert!(input.ssh_handover_enter_guard_armed(), "{mode:?}");
                    assert!(!input.enter_queue_holds_keys(ctx), "{mode:?}");
                });
                assert!(
                    ssh_queue_is_idle(view),
                    "{mode:?}: the queue still holds entries"
                );
            });
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

/// Owner decision 2026-11-09 (REVERSING #1661 and, for this hold, #1577's hand-back, which this
/// test expected): a young Enter hold whose prompt never comes WRITES the Enter and the keys
/// behind it to the pty as typeahead, in typed order; nothing goes to the editor, no guard is
/// armed and no toast is shown. This pane's hand-off has no prompt sequence numbers, so the hold
/// is the window hold, whose cap is its normal end and stays short. Its hand-back put the Enter
/// into the editor as a newline with the "Input was waiting too long" toast, once per Enter at
/// such a prompt: the toast that kept popping on macOS at an empty remote prompt. Both modes.
#[test]
fn young_hold_cap_writes_the_enter_and_keys_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                young_enter_setup(view);
                assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
                assert!(young_gate_open(view), "the Enter is held");
                assert!(
                    view.handoff.read().young_hold_cap_is_normal_end(),
                    "{mode:?}: a window hold with no sequence numbers ends at its cap"
                );
                let token = gate_token_of(view, input_queue::GateKind::PromptSeen);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_given_up_to_pty(&terminal, &app, &pty, b"\rls");
        });
    }
}

/// The owner's macOS report: a BARE Enter at an empty prompt of a session without prompt sequence
/// numbers, again and again. Each one is held for its window and then, at the cap, goes to the
/// pty as the `\r` it is; none is turned into editor text and no toast is shown, however many
/// times it happens. Both modes.
#[test]
fn bare_enters_at_a_prompt_without_sequence_numbers_reach_the_pty_with_no_toast() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let toasts = record_toasts(&mut app);
            let mut held_enters = 0usize;
            for _ in 0..3 {
                let held = terminal.update(&mut app, |view, ctx| {
                    young_enter_setup(view);
                    assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                    if !young_gate_open(view) {
                        // Not in the window this time: the Enter went straight to the pty.
                        return false;
                    }
                    let token = gate_token_of(view, input_queue::GateKind::PromptSeen);
                    view.on_paste_queue_give_up(token, ctx);
                    true
                });
                held_enters += usize::from(held);
            }
            assert!(held_enters > 0, "{mode:?}: the first Enter is held");
            assert_given_up_to_pty(&terminal, &app, &pty, b"\r\r\r");
            assert!(
                toasts.borrow().is_empty(),
                "{mode:?}: no toast for an Enter that reached the shell: {:?}",
                toasts.borrow()
            );
        });
    }
}

/// Owner decision 2026-11-09 (REVERSING #1661 and #1577's hand-back, which this test expected):
/// the fish no-barrier hold (`PrecmdMaybeInFlight`) ends at its short cap by design, and at that
/// cap the Enter and the keys behind it are WRITTEN TO THE PTY in typed order (nothing in the
/// editor, no guard, nothing dropped). Both modes.
#[test]
fn young_hold_fish_no_barrier_cap_writes_the_keys_to_the_pty() {
    use crate::terminal::model::typeahead_handoff::{EVENT_HOLD_GIVE_UP, YoungHoldEvidence};
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                young_enter_setup(view);
                assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
                let mut model = view.model.lock();
                let mut handoff = model.typeahead_handoff_mut();
                handoff.force_young_hold_evidence_for_test(YoungHoldEvidence::PrecmdMaybeInFlight);
                assert!(handoff.young_hold_cap_is_normal_end(), "{mode:?}");
                assert_eq!(
                    handoff.young_hold_give_up(std::time::Instant::now()),
                    EVENT_HOLD_GIVE_UP,
                    "{mode:?}: the short 2 s cap is kept"
                );
            });
            terminal.update(&mut app, |view, ctx| {
                let token = gate_token_of(view, input_queue::GateKind::PromptSeen);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_given_up_to_pty(&terminal, &app, &pty, b"\rls");
        });
    }
}

/// Marks the open young hold as one on the prompt-sequence path (`RequestDue`): its cap is a
/// GIVE-UP, the 10 s net (#1661).
fn make_young_hold_a_sequence_path_hold(view: &TerminalView) {
    use crate::terminal::model::typeahead_handoff::{YOUNG_HOLD_GIVE_UP, YoungHoldEvidence};
    let mut model = view.model.lock();
    let mut handoff = model.typeahead_handoff_mut();
    handoff.force_young_hold_evidence_for_test(YoungHoldEvidence::RequestDue);
    assert!(!handoff.young_hold_cap_is_normal_end());
    assert_eq!(
        handoff.young_hold_give_up(std::time::Instant::now()),
        YOUNG_HOLD_GIVE_UP
    );
}

/// Owner decision 2026-11-09 (REVERSING #1661, under which this test expected a discard): a
/// sequence-path young hold whose events never come gives up at the 10 s net and WRITES the Enter
/// and the keys typed after it to the pty, in typed order, as the typeahead they are: nothing in
/// the editor, no guard, nothing dropped. Both modes.
#[test]
fn young_hold_net_writes_the_enter_and_the_keys_after_it_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                young_enter_setup(view);
                assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
                assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                assert!(young_gate_open(view), "the Enter is held");
                make_young_hold_a_sequence_path_hold(view);
                let token = gate_token_of(view, input_queue::GateKind::PromptSeen);
                view.on_paste_queue_give_up(token, ctx);
            });
            // Both held Enters, in their places.
            assert_given_up_to_pty(&terminal, &app, &pty, b"\rls\r");
        });
    }
}

/// The line typed BEFORE a sequence-path young Enter is not in the hold. The hand-off holds a
/// young Enter only on a write that STARTS with the Enter (`should_hold_young_enter_sampled`);
/// `ls` typed before it went to the pty as typed, into the shell's own line. Owner decision
/// 2026-11-09 (REVERSING #1661, under which this test expected the Enter and `x` discarded): the
/// give-up writes the Enter and `x` typed after it behind that line, so the pty has received
/// exactly what was typed, `ls\rx`, in order. Nothing in the editor, no guard. Both modes.
#[test]
fn young_hold_net_sends_the_enter_after_the_line_already_at_the_shell() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                young_enter_setup(view);
                assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
                assert!(!young_gate_open(view), "{mode:?}: the line is not held");
            });
            assert_eq!(pty.borrow().concat(), b"ls".to_vec(), "{mode:?}");
            terminal.update(&mut app, |view, ctx| {
                assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                assert!(view.write_user_bytes_to_pty(b"x".to_vec(), ctx));
                assert!(young_gate_open(view), "{mode:?}: the Enter is held");
                make_young_hold_a_sequence_path_hold(view);
                let token = gate_token_of(view, input_queue::GateKind::PromptSeen);
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_given_up_to_pty(&terminal, &app, &pty, b"ls\rx");
        });
    }
}

/// #1661 review (MEDIUM), owner decision 2026-11-09 (REVERSING #1661: what the gate held is now
/// written to the pty, where this test expected it discarded): a give-up that finds the queue
/// busy is ONE deferred operation applied in FIFO order. A key typed after the give-up, while
/// the queue is still busy, waits in the mailbox behind it and is offered once the gate is gone:
/// it reaches the pty AFTER what the gate held, in typed order (a 100 ms timer retry let it land
/// behind the still-open gate). The generic give-up write is used here (a gate the hand-off no
/// longer knows): the hand-off must not hold its Enter for another young-hold net. Both modes.
#[test]
fn a_key_typed_after_a_busy_young_hold_give_up_follows_the_held_keys_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                young_enter_setup(view);
                assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
                assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
                assert!(young_gate_open(view), "the Enter is held");
                make_young_hold_a_sequence_path_hold(view);
                let hold_id = view
                    .paste_queue
                    .queue
                    .borrow()
                    .gates()
                    .iter()
                    .find(|gate| gate.kind == input_queue::GateKind::PromptSeen)
                    .map(|gate| gate.key)
                    .expect("the young gate");
                let queue = view.paste_queue.queue.clone();
                {
                    let _busy = queue.peek();
                    view.give_up_young_hold_keys_to_pty(
                        hold_id,
                        "young_hold_gone",
                        None,
                        false,
                        ctx,
                    );
                    assert!(
                        queue.has_deferred(),
                        "{mode:?}: the take waits for the queue"
                    );
                    // Typed after the give-up, while the queue is still busy.
                    view.write_user_bytes_to_pty(b"x".to_vec(), ctx);
                }
                view.flush_deferred_queue_ops(ctx);
                assert!(!young_gate_open(view), "{mode:?}");
            });
            for _ in 0..200 {
                if terminal.read(&app, |view, _| !view.paste_queue.queue.has_deferred()) {
                    break;
                }
                warpui::r#async::Timer::after(std::time::Duration::from_millis(5)).await;
            }
            let written: Vec<u8> = pty_writes.borrow().iter().flatten().copied().collect();
            let buffer = terminal.read(&app, |view, ctx| view.input.as_ref(ctx).buffer_text(ctx));
            assert_eq!(
                String::from_utf8_lossy(&written),
                "\rlsx",
                "{mode:?}: what the gate held, then the key typed after the give-up, in typed \
                 order (editor {buffer:?})"
            );
            assert_eq!(buffer, "", "{mode:?}: nothing went to the editor");
        });
    }
}

/// #1701 / #1702: an Enter deferred behind a report, with a key typed after it, is still held 3.5 s
/// on (past the old 3 s patience; the give-up writes the keys after the Enter to the pty, #1996):
/// both patiences are the 10 s net, the text and the key behind the Enter are intact, and the late
/// report is the event that releases the Enter. Two terminals: one waits on the hand-off's own
/// outstanding report (#1215, `TYPEAHEAD_REPORT_PATIENCE`), the other on a report that must cover
/// untracked bytes (#1104, `UNCOVERED_REPORT_PATIENCE`). Both modes.
#[test]
fn a_report_several_seconds_late_still_releases_the_deferred_enter() {
    use crate::terminal::model::typeahead_handoff::YOUNG_HOLD_GIVE_UP;
    assert!(TYPEAHEAD_REPORT_PATIENCE >= YOUNG_HOLD_GIVE_UP);
    assert!(UNCOVERED_REPORT_PATIENCE >= YOUNG_HOLD_GIVE_UP);
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let held = HandoffState::HoldAfterEnter { released: false };
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let awaiting = add_window_with_terminal(&mut app, None);
            let uncovered = add_window_with_terminal(&mut app, None);
            let awaiting_writes = record_pty_writes(&mut app, &awaiting);
            let uncovered_writes = record_pty_writes(&mut app, &uncovered);
            // The hand-off's own report is outstanding; the Enter is deferred behind it and its
            // patience timer is armed (#1215).
            awaiting.update(&mut app, |view, ctx| {
                awaiting_typeahead_report(view);
                view.input
                    .update(ctx, |input, ctx| input.replace_buffer_content("ho 47", ctx));
                view.input.update(ctx, |input, ctx| input.input_enter(ctx));
                view.keydown_on_terminal("x", ctx);
            });
            // A report that misses untracked line bytes keeps the Enter deferred and arms the
            // uncovered-report patience (#1104).
            uncovered.update(&mut app, |view, ctx| {
                awaiting_typeahead_report(view);
                view.model
                    .lock()
                    .typeahead_handoff_mut()
                    .note_untracked_line_bytes();
                view.input
                    .update(ctx, |input, ctx| input.replace_buffer_content("ho 47", ctx));
                view.input.update(ctx, |input, ctx| input.input_enter(ctx));
                view.keydown_on_terminal("x", ctx);
                view.handle_typeahead_report_with_handoff(true, ctx);
            });
            // Past the old 3 s patience, well short of the 10 s net.
            warpui::r#async::Timer::after(std::time::Duration::from_millis(3500)).await;
            for (name, terminal) in [("report", &awaiting), ("uncovered", &uncovered)] {
                terminal.read(&app, |view, ctx| {
                    assert_eq!(
                        view.model.lock().typeahead_handoff().state(),
                        &held,
                        "{mode:?} {name}: the Enter is still deferred, not given up"
                    );
                    assert!(
                        view.input.as_ref(ctx).buffer_text(ctx).ends_with("ho 47"),
                        "{mode:?} {name}: the text is intact: {:?}",
                        view.input.as_ref(ctx).buffer_text(ctx)
                    );
                    assert_eq!(
                        view.paste_queue.queue.borrow().queued_len(),
                        1,
                        "{mode:?} {name}: the key typed after the Enter still waits behind it"
                    );
                });
            }
            assert!(awaiting_writes.borrow().is_empty(), "{mode:?}");
            assert!(uncovered_writes.borrow().is_empty(), "{mode:?}");
            // The late report arrives: the event releases the Enter.
            awaiting.update(&mut app, |view, ctx| {
                view.handle_typeahead_report_with_handoff(true, ctx);
                assert_ne!(
                    view.model.lock().typeahead_handoff().state(),
                    &held,
                    "{mode:?}: the late report released the Enter"
                );
            });
        });
    }
}

/// A6 (#1577): the uncovered-report patience (10 s since #1701) no longer RUNS the deferred
/// Enter: its text stays in the editor and nothing is executed. The keys held behind the Enter
/// stay WITH the text, on the line after it, in typed order, under the hand-back guard; nothing
/// is written to the pty.
///
/// Expectation history (re-decided by the owner, never weakened): #1661 expected the key
/// discarded and nothing written; the first ruling of 2026-11-09 (#1996) expected `ls` at the
/// pty and the text alone in the editor; the second ruling of that day, "keep them together" (a
/// later line never runs without the line before it), keeps the keys with the unrun line.
#[test]
fn uncovered_report_patience_keeps_the_text_and_the_keys_behind_the_enter_together() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            let pty = collect_pty_writes(&mut app, &terminal);
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
                assert_eq!(
                    buffer, "ho 47\nls",
                    "the text stays and the keys behind its Enter stay with it, on the line \
                     after it, once (the second firing added nothing)"
                );
                assert!(
                    view.input.as_ref(ctx).ssh_handover_enter_guard_armed(),
                    "the first Enter on the two lines is held"
                );
                assert!(view.paste_queue.queue.borrow().is_idle());
            });
            assert!(
                !writes.borrow().contains(&"execute"),
                "{mode:?}: a timer ran the Enter: {:?}",
                writes.borrow()
            );
            assert_eq!(
                String::from_utf8_lossy(&pty.borrow().concat()),
                "",
                "{mode:?}: the Enter did not run, so nothing typed behind it reaches the pty"
            );
        });
    }
}

/// #1793's shape, owner decision 2026-11-09: a BARE Enter (an empty editor line) deferred behind
/// a report that never comes, with `~.` typed behind it. There is no visible line to keep, so at
/// the patience's give-up the Enter itself goes to the pty as `\r`, in front of the keys typed
/// after it: `\r~.`, what a plain terminal would have sent. Nothing is discarded, nothing goes to
/// the editor, and no toast is shown. Both modes.
#[test]
fn a_bare_deferred_enter_and_the_keys_behind_it_reach_the_pty_at_the_give_up() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            let pty = collect_pty_writes(&mut app, &terminal);
            let toasts = record_toasts(&mut app);
            terminal.update(&mut app, |view, ctx| {
                view.input.update(ctx, |input, ctx| {
                    input.clear_buffer_and_reset_undo_stack(ctx);
                });
                let taken = vec![pty_entry(1, b"~"), pty_entry(2, b".")];
                view.give_up_deferred_enter_keys(taken, "uncovered_report", ctx);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
            });
            assert!(
                !writes.borrow().contains(&"execute"),
                "{mode:?}: the editor ran nothing: {:?}",
                writes.borrow()
            );
            assert_eq!(
                String::from_utf8_lossy(&pty.borrow().concat()),
                "\r~.",
                "{mode:?}: the Enter, then the keys typed after it"
            );
            assert!(
                toasts.borrow().is_empty(),
                "{mode:?}: no toast: {:?}",
                toasts.borrow()
            );
        });
    }
}

/// A pty key as an entry taken out of a gate, for the give-up tails that take entries.
fn pty_entry(id: u64, bytes: &[u8]) -> input_queue::InputEntry {
    input_queue::InputEntry {
        id: input_queue::EntryId(id),
        kind: input_queue::EntryKind::PtyBytes {
            bytes: bytes.to_vec(),
            takes_over_agent_control: true,
        },
        focus_epoch: 0,
        arrived_at: std::time::Instant::now(),
        origin: input_queue::Origin::Keyboard,
        stamp: input_queue::Stamp::default(),
    }
}

/// A8 (#1577): the ssh connect window's give-up timer never delivers through the editor (it
/// submits nothing), even when the remote's prompt was seen. Owner decision 2026-11-09
/// (REVERSING #1661 and #1577's hand-back, which this test expected: `echo hi` back in the input
/// with the guard, nothing written): the held keys are WRITTEN TO THE PTY in typed order, for
/// what reads the tty now, and nothing goes to the editor.
#[test]
fn ssh_give_up_with_the_remote_prompt_seen_writes_the_held_keys_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_pty_writes(&mut app, &terminal);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                submit_command(view, "ssh host", ctx);
            });
            // The user's own Enter on `ssh host` emitted ExecuteCommand (delivered when the update
            // above ended, as every emit is): setup, not the timer. Only what happens from here on
            // is the give-up's.
            writes.borrow_mut().clear();
            pty.borrow_mut().clear();
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
                    Instant::now() - SSH_CONNECT_NO_PROBE_GIVE_UP - Duration::from_secs(1);
                view.ssh_give_up_fired(ctx);
                assert!(!view.ssh_typeahead_holding);
                assert!(!view.ssh_give_up_in_progress);
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "",
                    "{mode:?}: nothing is put into the editor"
                );
                assert!(!view.input.as_ref(ctx).ssh_handover_enter_guard_armed());
            });
            assert!(
                !writes.borrow().contains(&"execute"),
                "{mode:?}: a timer submitted a command: {:?}",
                writes.borrow()
            );
            assert_eq!(
                String::from_utf8_lossy(&pty.borrow().concat()),
                "echo hi",
                "{mode:?}: the held keys reach the pty in typed order, once"
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

/// #1119 / #1711: a given-up subshell setup is never silent. The setup net's give-up (`quiet_net`,
/// `ceiling`) shows its own counted notice (held keys or not), so its hand-back adds no second
/// toast; every other give-up keeps the generic one. The notice gives the COUNT of the held keys,
/// never their text; a PowerShell held line that never showed its prompt (`held_line`, review N1)
/// says so instead of "Phosphorize failed"; dropped abort keys are named (review F1).
#[test]
fn setup_net_hand_back_defers_to_its_own_counted_notice() {
    assert_eq!(paste_queue::gate_give_up_toast("quiet_net"), None);
    assert_eq!(paste_queue::gate_give_up_toast("ceiling"), None);
    assert!(paste_queue::gate_give_up_toast("subshell_setup_cap").is_some());
    assert!(paste_queue::gate_give_up_toast("bootstrap_write_cap").is_some());
    let notice = paste_queue::subshell_setup_given_up_toast("quiet_net", 3);
    assert!(notice.contains("Phosphorize failed"), "{notice}");
    assert!(
        notice.contains("3 keys typed during the setup are in the input, not run"),
        "{notice}"
    );
    let none = paste_queue::subshell_setup_given_up_toast("ceiling", 0);
    assert!(
        none.contains("Nothing was typed during the setup"),
        "{none}"
    );
    let one = paste_queue::subshell_setup_given_up_toast("quiet_net", 1);
    assert!(
        one.contains("1 key typed during the setup is in the input"),
        "{one}"
    );
    let held_line = paste_queue::subshell_setup_given_up_toast("held_line", 2);
    assert!(
        held_line.contains("has not shown its prompt"),
        "{held_line}"
    );
    assert!(!held_line.contains("Phosphorize failed"), "{held_line}");
    assert!(
        held_line.contains("2 keys typed after it are in the input, not run"),
        "{held_line}"
    );
}
