//! Group 2: timers close a segment and hand text back; they never write to a pty and never run a
//! command (#1577 / #1588). Child of `view_test.rs` (private helpers come in through `super`).

use super::*;
use crate::terminal::input_queue::DestinationMode;

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
