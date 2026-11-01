use warpui::App;

use super::*;

/// #1208: the shell enabling bracketed paste means its line editor is reading, so an activation
/// pending from the prompt's hook completes at once instead of at the end of the 50 ms delay.
/// Without a pending activation the signal is ignored, and a command write makes the line editor
/// inactive again as before.
#[test]
fn bracketed_paste_activates_a_pending_line_editor_at_once() {
    App::test((), |mut app| async move {
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let status = app.add_model(|ctx| LineEditorStatus::new(model_events, sessions, ctx));

        // No activation is pending: the signal alone proves nothing.
        status.update(&mut app, |status, ctx| status.note_shell_reading_line(ctx));
        status.read(&app, |status, _| assert!(!status.is_line_editor_active()));

        // The prompt's hook armed the delay; the shell's own signal ends it now.
        status.update(&mut app, |status, ctx| {
            status.arm_activation_for_test(ctx);
            assert!(!status.is_line_editor_active());
            status.note_shell_reading_line(ctx);
            assert!(status.is_line_editor_active());
        });

        // A command write makes the line editor inactive; a later signal with no prompt hook
        // since is ignored.
        status.update(&mut app, |status, ctx| {
            status.did_execute_command(ctx);
            assert!(!status.is_line_editor_active());
            status.note_shell_reading_line(ctx);
            assert!(!status.is_line_editor_active());
        });
    });
}

/// #1349: with the ConPTY prompt hold the activation fallback waits for the shell's signal for
/// the long give-up cap, so the 50 ms timer cannot beat a slow signal; the signal itself still
/// ends the wait at once.
#[test]
fn a_signal_waiting_activation_is_not_the_default_fifty_ms_timer() {
    App::test((), |mut app| async move {
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let status = app.add_model(|ctx| LineEditorStatus::new(model_events, sessions, ctx));

        status.update(&mut app, |status, ctx| {
            status.set_signal_wait(Some(Duration::from_secs(60)));
            status.arm_activation_for_test(ctx);
            assert!(!status.is_line_editor_active());
            status.note_shell_reading_line(ctx);
            assert!(status.is_line_editor_active(), "the signal ends the wait");
        });

        // The signal could not end the wait itself: the activation drops to the default delay,
        // and stays pending (inactive) until it elapses.
        status.update(&mut app, |status, ctx| {
            status.did_execute_command(ctx);
            status.arm_activation_for_test(ctx);
            status.expedite_pending_activation(ctx);
            assert!(!status.is_line_editor_active());
            assert!(status.mark_line_editor_active_abort_handle.is_some());
        });
    });
}
