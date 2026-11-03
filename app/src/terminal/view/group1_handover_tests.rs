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
