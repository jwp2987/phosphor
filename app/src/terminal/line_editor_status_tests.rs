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

        // The signal could not open the gate itself: it is remembered, and the gate stays shut
        // until the hand-off's retry (an event) asks for it.
        status.update(&mut app, |status, ctx| {
            status.did_execute_command(ctx);
            status.arm_activation_for_test(ctx);
            status.expedite_pending_activation(ctx);
            assert!(!status.is_line_editor_active());
            assert!(status.mark_line_editor_active_abort_handle.is_some());
        });
    });
}

/// #1798/#1672: only the shell's own signal proves its line editor reads. A prompt event that
/// opens the gate for a shell that has not signalled proves nothing, so no line-report request
/// follows it; a late signal proves it, and from then on the shell is waited for.
#[test]
fn only_the_ready_signal_proves_the_line_editor_reads() {
    App::test((), |mut app| async move {
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let status = app.add_model(|ctx| LineEditorStatus::new(model_events, sessions, ctx));

        status.update(&mut app, |status, ctx| {
            assert!(!status.expects_ready_signal());
            status.arm_activation_for_test(ctx);
            status.note_shell_reading_line(ctx);
            assert!(status.is_proven_reading());
            // Once the shell has signalled, its prompt hooks wait for the signal.
            assert!(status.expects_ready_signal());
            // A command start (Ctrl-C included) drops the proof.
            status.did_execute_command(ctx);
            assert!(!status.is_proven_reading());
            // A gate opened without the signal is unproven; a late signal proves it.
            status.set_line_editor_active_unproven_for_test();
            assert!(status.is_line_editor_active() && !status.is_proven_reading());
            status.note_shell_reading_line(ctx);
            assert!(status.is_proven_reading());
        });
    });
}

/// Records what a `LineEditorStatus` emits.
struct EventLog {
    events: Vec<LineEditorStatusEvent>,
}

impl Entity for EventLog {
    type Event = ();
}

fn event_log(app: &mut App, status: &ModelHandle<LineEditorStatus>) -> ModelHandle<EventLog> {
    let status = status.clone();
    app.add_model(|ctx| {
        ctx.subscribe_to_model(&status, |me: &mut EventLog, event, _| {
            me.events.push(*event)
        });
        EventLog { events: Vec::new() }
    })
}

fn counts(log: &ModelHandle<EventLog>, app: &App) -> (usize, usize) {
    log.read(app, |log, _| {
        (
            log.events
                .iter()
                .filter(|e| matches!(e, LineEditorStatusEvent::Active))
                .count(),
            log.events
                .iter()
                .filter(|e| matches!(e, LineEditorStatusEvent::SignalGaveUp))
                .count(),
        )
    })
}

/// Owner rule (#1672/#1798, event-driven pipeline): a shell that has never sent the ready signal
/// opens the gate on the prompt hook itself -- no delay, unproven. This is state-only, so it holds
/// in both input modes.
#[test]
fn a_shell_that_never_signals_opens_the_gate_on_the_prompt_event() {
    App::test((), |mut app| async move {
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let status = app.add_model(|ctx| LineEditorStatus::new(model_events, sessions, ctx));
        let log = event_log(&mut app, &status);

        status.update(&mut app, |status, ctx| status.prompt_hook_for_test(ctx));
        status.read(&app, |status, _| {
            assert!(status.is_line_editor_active(), "opened on the event itself");
            assert!(!status.is_proven_reading());
        });
        assert_eq!(counts(&log, &app), (1, 0));
    });
}

/// A shell that has signalled before is waited for: the prompt hook opens nothing, and the
/// signal -- only the signal -- opens the gate, proven.
#[test]
fn a_signalling_shell_opens_the_gate_only_on_its_signal() {
    App::test((), |mut app| async move {
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let status = app.add_model(|ctx| LineEditorStatus::new(model_events, sessions, ctx));
        let log = event_log(&mut app, &status);

        status.update(&mut app, |status, ctx| {
            status.set_ready_signal_seen_for_test(true);
            status.prompt_hook_for_test(ctx);
            assert!(
                !status.is_line_editor_active(),
                "the hook alone opens nothing"
            );
        });
        assert_eq!(counts(&log, &app), (0, 0));
        status.update(&mut app, |status, ctx| status.note_shell_reading_line(ctx));
        status.read(&app, |status, _| assert!(status.is_proven_reading()));
        assert_eq!(counts(&log, &app), (1, 0));
    });
}

/// The give-up net of a signalling shell whose signal does not come: it reports `SignalGaveUp`
/// (the held keys go back to the editor) and NEVER opens the gate -- nothing can be written. A
/// late signal, or the next prompt hook, then re-arms normally.
#[test]
fn the_give_up_net_hands_back_and_never_opens_the_gate() {
    App::test((), |mut app| async move {
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let status = app.add_model(|ctx| LineEditorStatus::new(model_events, sessions, ctx));
        let log = event_log(&mut app, &status);

        status.update(&mut app, |status, ctx| {
            status.set_ready_signal_seen_for_test(true);
            status.set_signal_wait(Some(Duration::from_millis(5)));
            status.prompt_hook_for_test(ctx);
        });
        warpui::r#async::Timer::after(Duration::from_millis(150)).await;
        assert_eq!(
            counts(&log, &app),
            (0, 1),
            "the net reported, nothing opened"
        );
        status.read(&app, |status, _| {
            assert!(
                !status.is_line_editor_active(),
                "the net never opens the gate"
            );
            assert!(!status.is_proven_reading());
        });
        // The next signal re-arms normally.
        status.update(&mut app, |status, ctx| status.note_shell_reading_line(ctx));
        status.read(&app, |status, _| assert!(status.is_proven_reading()));
        assert_eq!(counts(&log, &app), (1, 1));
        // A command start, then the next prompt hook, waits for the signal again (no
        // `signal_wait` left: the shell is known not to signal at the cap, but it has signalled).
        status.update(&mut app, |status, ctx| {
            status.did_execute_command(ctx);
            status.prompt_hook_for_test(ctx);
            assert!(!status.is_line_editor_active());
        });
    });
}

/// A signal that could not open the gate when it came is remembered, and only an event (the
/// hand-off's retry, `activate_pending_signal`) opens it -- never a timer.
#[test]
fn a_remembered_signal_opens_the_gate_only_when_asked() {
    App::test((), |mut app| async move {
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let status = app.add_model(|ctx| LineEditorStatus::new(model_events, sessions, ctx));
        let log = event_log(&mut app, &status);

        status.update(&mut app, |status, ctx| {
            status.arm_activation_for_test(ctx);
            assert!(!status.activate_pending_signal(ctx), "no signal yet");
            status.expedite_pending_activation(ctx);
            assert!(!status.is_line_editor_active());
            assert!(status.signal_activation_pending());
        });
        warpui::r#async::Timer::after(Duration::from_millis(100)).await;
        assert_eq!(counts(&log, &app), (0, 0));
        status.update(&mut app, |status, ctx| {
            assert!(status.activate_pending_signal(ctx));
            assert!(status.is_proven_reading());
        });
        assert_eq!(counts(&log, &app), (1, 0));
    });
}
