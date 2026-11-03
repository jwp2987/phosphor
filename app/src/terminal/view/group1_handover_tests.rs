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
