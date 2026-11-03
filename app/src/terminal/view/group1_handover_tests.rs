//! Group 1: queue yield latch (#1421 follow-up), ssh hand-over after a release without an event,
//! the mid-drain prompt hand-over (#1575), the gate-liveness check (#1578) and the held-text
//! hand-back (#1576). Child of `view_test.rs` (private helpers come in through `super`).

use super::*;
use crate::terminal::input_queue::DestinationMode;

/// 1e: a focus change while the hand-over drain is yielded must not deliver (or insert) what is
/// queued behind the yielding entry. `ls` waits behind a test gate, the Enter's command is
/// `ssh host`, the gate's release hands `ls` to the new connect hold, and a focus change in the
/// same update leaves it held.
#[test]
fn focus_change_during_a_yield_does_not_deliver() {
    App::test((), |mut app| async move {
        let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(
            DestinationMode::Table,
        );
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            hold_text_behind_test_gate(view, "ls", ctx);
            submit_command(view, "ssh host", ctx);
            release_test_gate(view, ctx);
            view.redetermine_terminal_focus(ctx);
            assert_eq!(queued_texts(view), vec!["ls".to_owned()]);
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "");
        });
        for _ in 0..16 {
            futures_lite::future::yield_now().await;
        }
        assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
    })
}

/// 1d (ii): the connect window was released without an event handler (what
/// `release_ssh_gate_for_test` does); the next live key hands the held text over FIRST (to the
/// remote shell, whose prompt was seen) and is typed behind it: once, in typed order.
#[test]
fn release_without_event_hands_over_before_the_next_key() {
    App::test((), |mut app| async move {
        let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(
            DestinationMode::Table,
        );
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            submit_command(view, "ssh host", ctx);
            view.typed_characters_on_terminal("echo QW", ctx);
            assert!(view.ssh_typeahead_holding);
            view.ssh_remote_prompt_seen = true;
            first_precmd_processed_for_test(view, ctx);
            release_ssh_gate_for_test(view);
            view.typed_characters_on_terminal("ERTY", ctx);
            assert!(!view.ssh_typeahead_holding);
            assert!(ssh_queue_is_idle(view));
            assert!(!view.ssh_handover_after_release);
        });
        let written = String::from_utf8_lossy(&pty_writes.borrow().concat()).into_owned();
        assert_eq!(written, "echo QWERTY");
    })
}

/// 1d (iii): an agent write after a silent release (a give-up) schedules the hand-over for the
/// next turn; with no remote prompt the held text goes back to the editor, unexecuted, guarded,
/// and nothing of it reaches the pty.
#[test]
fn an_agent_write_after_a_silent_release_schedules_the_handover() {
    App::test((), |mut app| async move {
        let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(
            DestinationMode::Table,
        );
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            submit_command(view, "ssh host", ctx);
            view.typed_characters_on_terminal("echo QW", ctx);
            assert!(view.ssh_typeahead_holding);
            view.ssh_connect_gate = None;
            view.note_ssh_gate_released(crate::terminal::input_queue::SshRelease::GiveUpCeiling);
            assert!(view.ssh_handover_after_release);
            view.write_agent_bytes_to_pty(b"y\r".to_vec(), &AIAgentPtyWriteMode::Raw, ctx);
        });
        for _ in 0..16 {
            futures_lite::future::yield_now().await;
        }
        terminal.read(&app, |view, ctx| {
            assert!(!view.ssh_typeahead_holding);
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "echo QW");
            assert!(view.input.as_ref(ctx).ssh_handover_enter_guard_armed());
        });
        let written = String::from_utf8_lossy(&pty_writes.borrow().concat()).into_owned();
        assert!(!written.contains("echo QW"), "{written:?}");
    })
}

/// #1575: the inner shell's prompt appears while the hand-over drain is yielded after the nested
/// `ssh other` Enter. The input delivered next (`echo B`) must not split from the held line: it
/// joins the hold, and the hand-over runs on the next turn, in typed order. The line reaches the
/// inner shell once and never the editor or a local run. Legacy runs only to prove it does not
/// panic.
#[test]
fn nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell() {
    for mode in [DestinationMode::Table, DestinationMode::Legacy] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            let executed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = executed.clone();
            app.update(|ctx| {
                ctx.subscribe_to_view(&terminal, move |_, event, _| {
                    if let Event::ExecuteCommand(e) = event {
                        sink.borrow_mut().push(e.command.clone());
                    }
                });
            });
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                submit_command(view, "ssh host", ctx);
                view.typed_characters_on_terminal("ssh other", ctx);
                view.keydown_on_terminal("\r", ctx);
                view.typed_characters_on_terminal("echo B", ctx);
                view.keydown_on_terminal("\r", ctx);
                if mode == DestinationMode::Table {
                    assert_eq!(view.ssh_held_count(), 4);
                }
                view.ssh_remote_prompt_seen = true;
                first_precmd_processed_for_test(view, ctx);
                release_ssh_gate_for_test(view);
                view.deliver_ssh_typeahead_if_ready("precmd", ctx);
                // The inner prompt becomes visible before the drain continues.
                view.model.lock().process_bytes("inner$ ");
                view.ssh_observe(ctx);
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            if mode != DestinationMode::Table {
                return;
            }
            terminal.read(&app, |view, ctx| {
                let writes = String::from_utf8_lossy(&pty_writes.borrow().concat()).into_owned();
                assert_eq!(writes.matches("echo B").count(), 1, "{writes:?}");
                let at = writes.find("echo B").unwrap_or(0);
                assert!(writes[at..].contains('\r'), "{writes:?}");
                assert!(
                    !view.input.as_ref(ctx).buffer_text(ctx).contains("echo B"),
                    "the editor took the line"
                );
                assert!(
                    !executed.borrow().iter().any(|c| c.contains("echo B")),
                    "{:?}",
                    executed.borrow()
                );
                assert!(!view.ssh_typeahead_holding);
            });
        })
    }
}
