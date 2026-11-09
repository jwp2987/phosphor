use warpui::App;

use super::*;
use crate::terminal::model::session::SessionId;

fn new_status(
    app: &mut App,
) -> (
    ModelHandle<LineEditorStatus>,
    ModelHandle<ModelEventDispatcher>,
) {
    let (_model_events_tx, model_events_rx) = async_channel::unbounded();
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let model_events =
        app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
    let status =
        app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
    (status, model_events)
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

/// (`GateOpened`, `Active`) events emitted so far.
fn counts(log: &ModelHandle<EventLog>, app: &App) -> (usize, usize) {
    log.read(app, |log, _| {
        (
            log.events
                .iter()
                .filter(|e| matches!(e, LineEditorStatusEvent::GateOpened))
                .count(),
            log.events
                .iter()
                .filter(|e| matches!(e, LineEditorStatusEvent::Active))
                .count(),
        )
    })
}

/// #1825: the prompt event ALWAYS opens the write gate. A shell that never signalled decides at
/// the event too (`Active`, unproven), exactly as before.
#[test]
fn the_prompt_event_opens_the_gate_of_a_shell_that_never_signalled() {
    App::test((), |mut app| async move {
        let (status, _events) = new_status(&mut app);
        let log = event_log(&mut app, &status);
        status.update(&mut app, |status, ctx| status.prompt_hook_for_test(ctx));
        status.read(&app, |status, _| {
            assert!(status.is_line_editor_active());
            assert!(!status.is_proven_reading());
            assert!(!status.decision_awaits_signal());
        });
        assert_eq!(counts(&log, &app), (0, 1));
    });
}

/// #1825 (`unset zle_bracketed_paste`): a session that signalled before and then stops signalling
/// still gets its gate opened by the prompt event, so a command the editor submitted is written
/// at that prompt instead of being parked behind a signal that never comes. Only its request
/// decision waits for the signal; a prompt cycle that ends without one demotes the session and
/// its next prompt decides at the event.
#[test]
fn a_session_that_stops_signalling_still_has_its_gate_opened_by_the_prompt() {
    App::test((), |mut app| async move {
        let (status, _events) = new_status(&mut app);
        let log = event_log(&mut app, &status);

        // The session signals once (learned).
        status.update(&mut app, |status, ctx| {
            status.prompt_hook_for_test(ctx);
            status.note_shell_reading_line(ctx);
            assert!(status.is_proven_reading());
            assert!(status.active_session_signals_for_test(ctx));
            status.did_execute_command(ctx);
        });
        assert_eq!(counts(&log, &app), (0, 1));

        // The next prompt: gate open at once, the decision waits for the signal.
        status.update(&mut app, |status, ctx| {
            status.prompt_hook_for_test(ctx);
            assert!(
                status.is_line_editor_active(),
                "the gate is open on the event"
            );
            assert!(!status.is_proven_reading());
            assert!(status.decision_awaits_signal());
        });
        assert_eq!(
            counts(&log, &app),
            (1, 1),
            "GateOpened, no new decision yet"
        );

        // The signal never comes (the user unset it): a command starts. The cycle is a miss.
        status.update(&mut app, |status, ctx| status.did_execute_command(ctx));

        // The next prompt decides at the event, as a non-signalling shell, and the session is no
        // longer a signalling one.
        status.update(&mut app, |status, ctx| {
            status.prompt_hook_for_test(ctx);
            assert!(status.is_line_editor_active());
            assert!(!status.decision_awaits_signal());
            assert!(!status.active_session_signals_for_test(ctx), "demoted");
        });
        assert_eq!(counts(&log, &app), (1, 2));

        // A late signal learns it again and proves the reader; no second decision.
        status.update(&mut app, |status, ctx| {
            status.note_shell_reading_line(ctx);
            assert!(status.is_proven_reading());
            assert!(status.active_session_signals_for_test(ctx));
        });
        assert_eq!(counts(&log, &app), (1, 2));
    });
}

/// #1208 / #1798: for a session known to signal, the signal starts the request decision (`Active`,
/// proven) after the gate was opened by the prompt; a signal outside a prompt cycle proves
/// nothing, and a command start drops the proof.
#[test]
fn the_ready_signal_proves_the_reader_and_starts_the_decision() {
    App::test((), |mut app| async move {
        let (status, _events) = new_status(&mut app);
        let log = event_log(&mut app, &status);

        // No prompt cycle: the signal alone proves nothing.
        status.update(&mut app, |status, ctx| status.note_shell_reading_line(ctx));
        status.read(&app, |status, _| assert!(!status.is_line_editor_active()));

        status.update(&mut app, |status, ctx| {
            status.set_ready_signal_seen_for_test(true, ctx);
            status.prompt_hook_for_test(ctx);
            assert!(status.is_line_editor_active());
            assert!(!status.is_proven_reading(), "unproven until the signal");
            status.note_shell_reading_line(ctx);
            assert!(status.is_proven_reading());
            status.did_execute_command(ctx);
            assert!(
                !status.is_proven_reading(),
                "a command start drops the proof"
            );
        });
        assert_eq!(
            counts(&log, &app),
            (1, 1),
            "GateOpened at the prompt, Active at the signal"
        );
    });
}

/// A signal that could not start the decision when it came (a typeahead line is queued ahead of
/// the hand-off) is remembered, and only an event (`activate_pending_signal`, from the hand-off's
/// retry) starts it -- never a timer.
#[test]
fn a_remembered_signal_starts_the_decision_only_when_asked() {
    App::test((), |mut app| async move {
        let (status, _events) = new_status(&mut app);
        let log = event_log(&mut app, &status);

        status.update(&mut app, |status, ctx| {
            status.arm_activation_for_test(ctx);
            assert!(!status.activate_pending_signal(ctx), "no signal yet");
            status.expedite_pending_activation(ctx);
            assert!(status.signal_activation_pending());
        });
        warpui::r#async::Timer::after(Duration::from_millis(100)).await;
        assert_eq!(counts(&log, &app), (1, 0));
        status.update(&mut app, |status, ctx| {
            assert!(status.activate_pending_signal(ctx));
            assert!(status.is_proven_reading());
        });
        assert_eq!(counts(&log, &app), (1, 1));
    });
}

/// ConPTY prompt hold (#1349): the hold's wait makes the request decision wait for the signal like
/// a learned session's, while the gate still opens on the prompt event.
#[test]
fn the_conpty_signal_wait_defers_only_the_decision() {
    App::test((), |mut app| async move {
        let (status, _events) = new_status(&mut app);
        let log = event_log(&mut app, &status);
        status.update(&mut app, |status, ctx| {
            status.set_signal_wait(Some(Duration::from_secs(60)));
            status.prompt_hook_for_test(ctx);
            assert!(status.is_line_editor_active());
            assert!(status.decision_awaits_signal());
            status.note_shell_reading_line(ctx);
            assert!(status.is_proven_reading());
        });
        assert_eq!(counts(&log, &app), (1, 1));
    });
}

/// #1823: the ready signal is learned PER SESSION. The local shell signalled; the session an `ssh`
/// or `su` then starts in the same pane has not, so its prompt decides at the event. Returning to
/// the local session keeps what it learned: its decision waits for the signal again.
#[test]
fn the_ready_signal_is_learned_per_session() {
    App::test((), |mut app| async move {
        let (status, model_events) = new_status(&mut app);
        let local = SessionId::from(0);
        let remote = SessionId::from(1);

        model_events.update(&mut app, |dispatcher, _| {
            dispatcher.set_active_session_id(local)
        });
        status.update(&mut app, |status, ctx| {
            status.arm_activation_for_test(ctx);
            status.note_shell_reading_line(ctx);
            assert!(status.is_proven_reading());
            assert!(status.active_session_signals_for_test(ctx));
            status.did_execute_command(ctx);
        });

        model_events.update(&mut app, |dispatcher, _| {
            dispatcher.set_active_session_id(remote)
        });
        status.update(&mut app, |status, ctx| {
            assert!(!status.active_session_signals_for_test(ctx));
            status.prompt_hook_for_test(ctx);
            assert!(status.is_line_editor_active());
            assert!(!status.decision_awaits_signal(), "decides at the event");
            status.did_execute_command(ctx);
        });

        model_events.update(&mut app, |dispatcher, _| {
            dispatcher.set_active_session_id(local)
        });
        status.update(&mut app, |status, ctx| {
            assert!(status.active_session_signals_for_test(ctx));
            status.prompt_hook_for_test(ctx);
            assert!(
                status.is_line_editor_active(),
                "the gate is open on the event"
            );
            assert!(
                status.decision_awaits_signal(),
                "its decision waits for the signal"
            );
        });
    });
}
