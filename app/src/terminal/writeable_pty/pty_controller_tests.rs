use std::sync::Arc;

use parking_lot::{FairMutex, Mutex};
use warpui::App;

use super::*;
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::session::Sessions;

#[derive(Clone, Default)]
struct TestEventLoopSender {
    messages: Arc<Mutex<Vec<Message>>>,
}

impl EventLoopSender for TestEventLoopSender {
    fn send(&self, message: Message) -> Result<(), EventLoopSendError> {
        self.messages.lock().push(message);
        Ok(())
    }
}

fn terminal_model() -> Arc<FairMutex<TerminalModel>> {
    Arc::new(FairMutex::new(TerminalModel::mock(
        None,
        Some(ChannelEventListener::new_for_test()),
    )))
}

fn assert_input_matches(message: &Message, expected_bytes: Vec<u8>) {
    assert!(matches!(message, Message::Input(bytes) if bytes.to_vec() == expected_bytes));
}

// Every test below builds its own `PtyController` wired to a fresh, idle `TerminalModel` and a
// `TestEventLoopSender` that records every message it is asked to send -- the same harness shape
// used in pty_controller_lifecycle_tests.rs. The line editor starts out inactive (this is
// `LineEditorStatus`'s default state, and nothing here drives the precmd/end-prompt hooks that
// would activate it), so `PtyController` queues writes in `pending_writes` rather than sending
// them immediately. Tests that care about what actually reaches the event loop drain
// `pending_writes` themselves and call `send_write_to_event_loop` directly -- the same function
// the real queue-draining path (`execute_next_queued_write`) calls once the line editor becomes
// active. This mirrors the pattern `rejected_queued_in_band_start_is_cancelled_without_writing_bytes`
// already uses in pty_controller_lifecycle_tests.rs.

/// `queue_in_band_command` formats its bytes the same way a user command does: the shell's
/// kill-buffer sequence, then the command text, then the shell's execute-command sequence.
///
/// This exercises the current in-band write path end-to-end (`queue_in_band_command` ->
/// `send_write_to_event_loop` -> `bytes_to_execute_command`), which is the closest surviving
/// analog to the orphaned `test_pty_controller_writes_in_band_command`; that test called a
/// `write_in_band_command` method that no longer exists anywhere in the crate.
#[test]
fn queue_in_band_command_sends_expected_bytes_to_event_loop() {
    App::test((), |mut app| async move {
        let model = terminal_model();
        let (model_events_tx, model_events_rx) = async_channel::unbounded();
        let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let line_editor_status =
            app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
        let sender = TestEventLoopSender::default();
        let controller = app.add_model(|ctx| {
            PtyController::new(
                sender.clone(),
                model_events,
                line_editor_status,
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });
        let (cancel_tx, cancel_rx) = async_channel::unbounded();
        let shell_type = ShellType::Zsh;

        let sent = controller.update(&mut app, |controller, ctx| {
            controller.queue_in_band_command(
                "echo foo",
                shell_type,
                "command-id".to_owned(),
                cancel_tx,
                ctx,
            );
            let write = controller
                .pending_writes
                .pop_front()
                .expect("the in-band command should be queued while the line editor is inactive.");
            controller.send_write_to_event_loop(write, ctx)
        });

        assert!(
            sent,
            "an in-band command accepted by the model should be written to the event loop."
        );
        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 1);
        assert_input_matches(
            &messages[0],
            bytes_to_execute_command("echo foo", shell_type, false),
        );
        assert!(
            cancel_rx.try_recv().is_err(),
            "an accepted in-band command must not be cancelled."
        );

        drop(model_events_tx);
    });
}

/// Writing an in-band command marks the block list as writing/executing an in-band command, via
/// the `before_write_fn` callback that `queue_in_band_command` attaches (which calls
/// `TerminalModel::start_in_band_command_execution`).
///
/// This is the current-API analog of the orphaned
/// `test_pty_controller_updates_block_list_when_writing_in_band_command`, which asserted the
/// same `BlockList::is_writing_or_executing_in_band_command()` flag through a
/// `write_in_band_command` method that no longer exists.
#[test]
fn queue_in_band_command_marks_block_list_as_writing_in_band_command() {
    App::test((), |mut app| async move {
        let model = terminal_model();
        let (model_events_tx, model_events_rx) = async_channel::unbounded();
        let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let line_editor_status =
            app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
        let sender = TestEventLoopSender::default();
        let controller = app.add_model(|ctx| {
            PtyController::new(
                sender.clone(),
                model_events,
                line_editor_status,
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });
        let (cancel_tx, _cancel_rx) = async_channel::unbounded();

        assert!(!model
            .lock()
            .block_list()
            .is_writing_or_executing_in_band_command());

        let sent = controller.update(&mut app, |controller, ctx| {
            controller.queue_in_band_command(
                "echo foo",
                ShellType::Zsh,
                "command-id".to_owned(),
                cancel_tx,
                ctx,
            );
            let write = controller
                .pending_writes
                .pop_front()
                .expect("the in-band command should be queued while the line editor is inactive.");
            controller.send_write_to_event_loop(write, ctx)
        });
        assert!(sent);

        assert!(model
            .lock()
            .block_list()
            .is_writing_or_executing_in_band_command());

        drop(model_events_tx);
    });
}

/// `write_command` unconditionally clears `pending_writes` before queueing the new command, so
/// issuing a user command drops whatever was previously queued.
///
/// This is the current-API analog of the orphaned
/// `test_pty_controller_cancels_async_writes_upon_user_command`. That test pinned a different
/// mechanism -- a delayed `AsyncPtyWrite` (queued via a since-removed `queue_async_write` API)
/// being cancelled by a subsequent user command. `AsyncPtyWrite`/`queue_async_write` no longer
/// exist anywhere in the crate; the surviving mechanism with the same intent -- a new user
/// command discards previously queued-but-not-yet-sent writes -- is the `pending_writes.clear()`
/// call in `write_command`, which this test pins instead.
#[test]
fn write_command_replaces_previously_pending_write() {
    App::test((), |mut app| async move {
        let model = terminal_model();
        let (model_events_tx, model_events_rx) = async_channel::unbounded();
        let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let line_editor_status =
            app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
        let sender = TestEventLoopSender::default();
        let controller = app.add_model(|ctx| {
            PtyController::new(
                sender.clone(),
                model_events,
                line_editor_status,
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });

        controller.update(&mut app, |controller, _| {
            controller.pending_writes.push_back(PtyWrite::Bytes {
                bytes: b"stale-pending-write".to_vec().into(),
            });
        });

        let outcome = controller.update(&mut app, |controller, ctx| {
            controller.write_command(
                "echo new",
                ShellType::Zsh,
                CommandExecutionSource::User,
                ctx,
            )
        });
        assert_eq!(outcome, StartCommandOutcome::Accepted);

        controller.read(&app, |controller, _| {
            assert_eq!(
                controller.pending_writes.len(),
                1,
                "write_command should have replaced the stale queued write, not appended to it."
            );
            assert!(matches!(
                &controller.pending_writes[0],
                PtyWrite::Command { command, .. } if command == "echo new"
            ));
        });
        // The line editor is inactive, so the new command stays queued rather than being sent.
        assert!(sender.messages.lock().is_empty());

        drop(model_events_tx);
    });
}

/// `write_command` builds a `PtyWrite::Command` whose bytes -- once actually written to the
/// event loop -- match `bytes_to_execute_command` for the given shell, the same formatting a
/// directly-queued in-band command goes through.
///
/// This is the current-API analog of the orphaned `test_pty_controller_writes_user_command`,
/// which called a `write_user_command` method that no longer exists; `write_command` (with
/// `CommandExecutionSource::User`) is the surviving equivalent.
#[test]
fn write_command_sends_expected_bytes_for_user_source() {
    App::test((), |mut app| async move {
        let model = terminal_model();
        let (model_events_tx, model_events_rx) = async_channel::unbounded();
        let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        let line_editor_status =
            app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
        let sender = TestEventLoopSender::default();
        let controller = app.add_model(|ctx| {
            PtyController::new(
                sender.clone(),
                model_events,
                line_editor_status,
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });
        let shell_type = ShellType::Zsh;

        let sent = controller.update(&mut app, |controller, ctx| {
            let outcome =
                controller.write_command("echo foo", shell_type, CommandExecutionSource::User, ctx);
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            let write = controller
                .pending_writes
                .pop_front()
                .expect("the user command should be queued while the line editor is inactive.");
            controller.send_write_to_event_loop(write, ctx)
        });

        assert!(
            sent,
            "an accepted user command should be written to the event loop."
        );
        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 1);
        assert_input_matches(
            &messages[0],
            bytes_to_execute_command("echo foo", shell_type, false),
        );

        drop(model_events_tx);
    });
}
