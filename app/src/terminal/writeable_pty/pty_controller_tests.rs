use std::sync::Arc;
use std::time::Instant;

use parking_lot::{FairMutex, Mutex};
use warpui::App;

use super::*;
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::session::{SessionId, Sessions};
use crate::terminal::model::terminal_model::SubshellInitializationInfo;
use crate::terminal::shell::Shell;

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

/// A bootstrapped zsh session, registered as the active session so
/// `PtyController`'s `LineEditorStatus` subscription can resolve
/// `Shell::input_reporting_sequence()`.
///
/// The shell must be overridden: `SessionInfo::new_for_test` builds a bash session with no
/// version string, and `input_reporting_sequence` returns `None` for bash unless the version
/// parses at or above `BASH_INPUT_REPORTING_MINIMUM_VERSION`. Zsh returns `ESC i`
/// unconditionally, which is the byte pair the pin's tests assert on.
fn zsh_session_info() -> SessionInfo {
    let mut session_info = SessionInfo::new_for_test();
    session_info.shell = Shell::new(ShellType::Zsh, None, None, Default::default(), None);
    session_info
}

/// When the shell's line editor becomes active, `PtyController` writes the shell's input
/// reporting sequence (`ESC i` for zsh) -- the binding that makes the shell report its input
/// buffer back through the `InputBuffer` DCS hook.
///
/// This is the current-API analog of the orphaned
/// `test_pty_controller_writes_input_buffer_sequence_after_block_completed`. That test drove the
/// same write through `PtyController::set_state_after_block_completed(&BlockType::User(..),
/// true)`; `set_state_after_block_completed` no longer exists in *either* tree (it is absent
/// from the pin's `pty_controller.rs` at `42effe840` too -- the pin's
/// `pty_controller_tests.rs` is an orphan file that no `mod` declaration includes, so it has not
/// compiled upstream since the rewrite). The surviving mechanism is the `LineEditorStatusEvent::
/// Active` subscription in `PtyController::new`, which is what this pins.
///
/// The assertion is made after draining `pending_writes` by hand, per this file's harness note:
/// nothing here activates `LineEditorStatus`'s own `is_line_editor_active` flag, so
/// `execute_next_queued_write` is a no-op and the queued bytes must be sent explicitly.
#[test]
fn input_reporting_sequence_is_queued_when_the_line_editor_becomes_active() {
    App::test((), |mut app| async move {
        let model = terminal_model();
        let (model_events_tx, model_events_rx) = async_channel::unbounded();
        let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        sessions.update(&mut app, |sessions, _| {
            sessions.register_session_for_test(zsh_session_info())
        });
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        model_events.update(&mut app, |dispatcher, _| {
            dispatcher.set_active_session_id(SessionId::from(0))
        });
        let line_editor_status =
            app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
        let sender = TestEventLoopSender::default();
        let controller = app.add_model(|ctx| {
            PtyController::new(
                sender.clone(),
                model_events.clone(),
                line_editor_status.clone(),
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });

        line_editor_status.update(&mut app, |_, ctx| ctx.emit(LineEditorStatusEvent::Active));

        let sent = controller.update(&mut app, |controller, ctx| {
            assert_eq!(
                controller.pending_writes.len(),
                1,
                "activating the line editor should queue exactly the input reporting sequence."
            );
            let write = controller
                .pending_writes
                .pop_front()
                .expect("the input reporting sequence should be queued.");
            controller.send_write_to_event_loop(write, ctx)
        });

        assert!(sent);
        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 1);
        assert_input_matches(&messages[0], vec![escape_sequences::C0::ESC, b'i']);

        drop(model_events_tx);
    });
}

/// The input reporting sequence goes to the *front* of the queue, so it reaches the shell before
/// an in-band command that was already waiting -- the shell must have its input-buffer binding
/// installed before the command that will consume the line editor is written.
///
/// This is the current-API analog of the orphaned
/// `test_pty_controller_writes_in_band_command_after_input_buffer_sequence`, which asserted the
/// same two-message ordering through the removed
/// `set_state_after_block_completed` + `write_in_band_command` pair. The ordering is produced
/// here by the `pending_writes.push_front(..)` in the `LineEditorStatusEvent::Active` handler;
/// a `push_back` there would leave the two writes in the wrong order and this test is what
/// catches it.
#[test]
fn input_reporting_sequence_is_written_before_an_already_queued_in_band_command() {
    App::test((), |mut app| async move {
        let model = terminal_model();
        let (model_events_tx, model_events_rx) = async_channel::unbounded();
        let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        sessions.update(&mut app, |sessions, _| {
            sessions.register_session_for_test(zsh_session_info())
        });
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        model_events.update(&mut app, |dispatcher, _| {
            dispatcher.set_active_session_id(SessionId::from(0))
        });
        let line_editor_status =
            app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
        let sender = TestEventLoopSender::default();
        let controller = app.add_model(|ctx| {
            PtyController::new(
                sender.clone(),
                model_events.clone(),
                line_editor_status.clone(),
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });
        let (cancel_tx, cancel_rx) = async_channel::unbounded();
        let shell_type = ShellType::Zsh;

        controller.update(&mut app, |controller, ctx| {
            controller.queue_in_band_command(
                "echo foo",
                shell_type,
                "command-id".to_owned(),
                cancel_tx,
                ctx,
            );
        });

        line_editor_status.update(&mut app, |_, ctx| ctx.emit(LineEditorStatusEvent::Active));

        controller.update(&mut app, |controller, ctx| {
            assert_eq!(
                controller.pending_writes.len(),
                2,
                "the input reporting sequence should be queued alongside the in-band command."
            );
            while let Some(write) = controller.pending_writes.pop_front() {
                controller.send_write_to_event_loop(write, ctx);
            }
        });

        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 2);
        assert_input_matches(&messages[0], vec![escape_sequences::C0::ESC, b'i']);
        assert_input_matches(
            &messages[1],
            bytes_to_execute_command("echo foo", shell_type, false),
        );
        assert!(
            cancel_rx.try_recv().is_err(),
            "an accepted in-band command must not be cancelled."
        );

        drop(model_events_tx);
    });
}

/// A session bootstrapped inside a `docker`/`podman exec -it` subshell, matching
/// `bootstrap::is_container_subshell`'s predicate.
#[cfg(feature = "local_fs")]
fn docker_exec_session_info() -> SessionInfo {
    let mut session_info = SessionInfo::new_for_test();
    session_info.shell = Shell::new(ShellType::Bash, None, None, Default::default(), None);
    session_info.subshell_info = Some(SubshellInitializationInfo {
        spawning_command: "docker exec -it my-container bash".to_owned(),
        was_triggered_by_rc_file_snippet: false,
        env_var_collection_name: None,
        ssh_connection_info: None,
    });
    session_info
}

/// The double-PTY proxy behind `docker`/`podman exec -it` drops data on large
/// writes, so a container subshell's bootstrap must go out in bounded chunks
/// with gaps between them rather than as one write -- see
/// `bootstrap::is_container_subshell`'s doc comment and the pin
/// (`4111d08f9:app/src/terminal/writeable_pty/pty_controller.rs:444-454`).
///
/// This drives a bootstrap sized to span two 4KB chunks (a full first chunk
/// plus a short remainder) through the real `PtyController` -> event-loop path
/// and asserts on what actually reached `TestEventLoopSender`: exactly two
/// writes, arriving as two separate messages (not concatenated into the single
/// write the non-container path would produce -- see
/// `non_container_bootstrap_is_written_as_a_single_unchunked_write` below),
/// which is what a proxy that drops large single writes needs. The chunks'
/// bytes are also asserted to reassemble the original bootstrap exactly, so a
/// chunk-boundary bug would fail here even if the write count matched.
#[cfg(feature = "local_fs")]
#[test]
fn container_subshell_bootstrap_is_written_in_bounded_chunks() {
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

        // One full 4KB chunk plus a short remainder: exercises the chunk
        // boundary, not just "small enough to fit in one write anyway".
        let bootstrap: Vec<u8> = (0..(4096 + 100)).map(|i| (i % 256) as u8).collect();
        let session_info = docker_exec_session_info();

        controller.update(&mut app, |controller, ctx| {
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                bootstrap.clone().into(),
            );
        });

        // The chunks are dispatched via spawned timers (0ms, 50ms gap); poll
        // rather than assert on an exact instant.
        let deadline = Instant::now() + Duration::from_secs(5);
        while sender.messages.lock().len() < 2 && Instant::now() < deadline {
            Timer::after(Duration::from_millis(10)).await;
        }

        let messages = sender.messages.lock();
        assert_eq!(
            messages.len(),
            2,
            "a 4196-byte container bootstrap should be written as two 4KB-bounded chunks, not one write"
        );
        let mut reassembled = Vec::new();
        for message in messages.iter() {
            match message {
                Message::Input(bytes) => reassembled.extend_from_slice(bytes),
                other => panic!("unexpected message on the container bootstrap path: {other:?}"),
            }
        }
        assert_eq!(
            reassembled, bootstrap,
            "the chunks must reassemble the original bootstrap byte-for-byte"
        );

        drop(model_events_tx);
    });
}

/// The chunking decision is `is_container_subshell`, not "the bootstrap is
/// large": a non-container session with a bootstrap over the 4KB chunk size
/// must still go out as a single write, so this only depends on the container
/// predicate. Runs under either the `local_fs` or non-`local_fs` build of
/// `write_bootstrap_script_to_shell` (the non-`local_fs` stub always writes
/// unchunked), so it also guards the default-feature build this fork's own
/// precheck does not otherwise exercise for `local_fs` code.
#[test]
fn non_container_bootstrap_is_written_as_a_single_unchunked_write() {
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

        // Larger than the 4KB chunk size, but not a container subshell.
        let bootstrap: Vec<u8> = (0..(4096 + 100)).map(|i| (i % 256) as u8).collect();
        let session_info = zsh_session_info();

        controller.update(&mut app, |controller, ctx| {
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Zsh,
                bootstrap.clone().into(),
            );
        });

        let messages = sender.messages.lock();
        assert_eq!(
            messages.len(),
            1,
            "a non-container bootstrap must not be chunked even when it exceeds the chunk size"
        );
        assert_input_matches(&messages[0], bootstrap);

        drop(model_events_tx);
    });
}

/// The native-completions watchdog is what stands between an unanswered `^Y` handshake and a
/// permanently wedged pane (see `arm_native_completions_watchdog`'s doc comment): if the shell
/// never replies, `in_flight_native_completions_state` stays `AwaitingPrompt` forever, which
/// gates `can_write_to_pty` shut for good.
///
/// This drives `arm_native_completions_watchdog` directly rather than through
/// `run_native_shell_completions` -> `execute_next_queued_write`, because that path additionally
/// requires the line editor to be active (this harness's line editor never is -- see the
/// file-level comment), and the watchdog is the mechanism under test.
#[test]
fn native_completions_watchdog_recovers_an_unanswered_prompt_handshake() {
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

        let (results_tx, results_rx) = async_channel::unbounded();
        controller.update(&mut app, |controller, ctx| {
            controller.in_flight_native_completions_state =
                Some(NativeShellCompletionsState::AwaitingPrompt {
                    buffer_text: "echo hi".to_owned(),
                    results_tx,
                });
            controller.arm_native_completions_watchdog(Duration::from_millis(20), true, ctx);
        });

        controller.read(&app, |controller, _| {
            assert!(
                controller.in_flight_native_completions_state.is_some(),
                "the watchdog must not clear the state before its timeout elapses."
            );
        });

        // Long enough for the 20ms watchdog to fire and its callback to run.
        Timer::after(Duration::from_millis(200)).await;

        controller.read(&app, |controller, _| {
            assert!(
                controller.in_flight_native_completions_state.is_none(),
                "an unanswered handshake must be abandoned so `can_write_to_pty` reopens, \
                 not left wedging the pane forever."
            );
        });
        assert!(
            results_rx.recv().await.is_err(),
            "dropping the abandoned state must close `results_tx` so the requester's \
             `results_rx.recv().await.ok()` resolves to `None` instead of leaking the \
             channel and hanging forever."
        );

        drop(model_events_tx);
    });
}

/// A watchdog timer belongs to the generation it was armed under (`native_completions_generation`);
/// a stale timer from a superseded phase must not tear down a later, still-live handshake, but
/// the later phase's own watchdog must still fire on schedule. Without this guard a
/// slow-but-successful handshake could be torn down by its own predecessor's timeout, per
/// `arm_native_completions_watchdog`'s doc comment.
#[test]
fn native_completions_watchdog_generation_guard_ignores_a_superseded_timer() {
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

        // Generation 1: a prompt handshake with a short-fused watchdog.
        let (first_results_tx, _first_results_rx) = async_channel::unbounded();
        controller.update(&mut app, |controller, ctx| {
            controller.in_flight_native_completions_state =
                Some(NativeShellCompletionsState::AwaitingPrompt {
                    buffer_text: "first".to_owned(),
                    results_tx: first_results_tx,
                });
            controller.arm_native_completions_watchdog(Duration::from_millis(20), true, ctx);
        });

        // The shell replies before generation 1's watchdog fires: the handshake advances to
        // `AwaitingResults` and arms its own, longer-lived generation-2 watchdog -- exactly what
        // `ModelEvent::SendCompletionsPrompt` does in production.
        let (second_results_tx, second_results_rx) = async_channel::unbounded();
        controller.update(&mut app, |controller, ctx| {
            controller.in_flight_native_completions_state =
                Some(NativeShellCompletionsState::AwaitingResults {
                    results_tx: second_results_tx,
                });
            controller.arm_native_completions_watchdog(Duration::from_millis(500), false, ctx);
        });

        // Long enough for generation 1's 20ms timer to have fired; short of generation 2's 500ms.
        Timer::after(Duration::from_millis(150)).await;

        controller.read(&app, |controller, _| {
            assert!(
                controller.in_flight_native_completions_state.is_some(),
                "a stale timer from a superseded phase must not tear down the current handshake."
            );
        });
        assert!(
            second_results_rx.try_recv().is_err(),
            "the current handshake's channel must still be open."
        );

        // Generation 2's own watchdog must still fire and recover the pane.
        Timer::after(Duration::from_millis(500)).await;

        controller.read(&app, |controller, _| {
            assert!(
                controller.in_flight_native_completions_state.is_none(),
                "the current generation's own watchdog must still fire on schedule."
            );
        });

        drop(model_events_tx);
    });
}

/// Regression test for the destructive take-before-match bug (TODO.md, "The shell lockup",
/// 2026-09-25 correction, follow-up (1)): a `CompletionsFinished` event that arrives in the
/// wrong phase must not destroy whatever request is actually in flight.
///
/// On the old code, the `CompletionsFinished` handler did
/// `let Some(AwaitingResults { .. }) = state.take() else { warn!(..); return };` -- `.take()`
/// unconditionally empties `in_flight_native_completions_state` and returns the old value
/// *before* the pattern match runs, so when the current state was actually `AwaitingPrompt` (a
/// late reply for some earlier, already-superseded request landing while a newer request B is
/// mid-handshake), the match failed, the `else` branch warned and returned -- but B's
/// `AwaitingPrompt` state, and its `results_tx`, were already gone. Since `AwaitingPrompt` is one
/// of `can_write_to_pty`'s two gates, and B's `results_tx` is now dropped with no request ever
/// having answered it, that is exactly the "leaves zsh inside `read -d $'\4'`" lockup this entry
/// describes: nothing will ever complete B's handshake, and every subsequent keystroke queues
/// forever.
#[test]
fn late_completions_finished_does_not_destroy_a_newer_awaiting_prompt_request() {
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
                model_events.clone(),
                line_editor_status,
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });

        // Request B is live, mid-handshake, waiting for the shell's OSC reply.
        let (b_results_tx, b_results_rx) = async_channel::unbounded();
        controller.update(&mut app, |controller, _ctx| {
            controller.in_flight_native_completions_state =
                Some(NativeShellCompletionsState::AwaitingPrompt {
                    buffer_text: "request b".to_owned(),
                    results_tx: b_results_tx,
                });
        });

        // A `CompletionsFinished` arrives -- e.g. a late reply for some earlier request A that
        // has already been superseded. It is the wrong phase for the current state
        // (`AwaitingPrompt`, not `AwaitingResults`), so it must be ignored without touching that
        // state. `ctx.emit` on the dispatcher's own context reaches `PtyController`'s
        // subscription synchronously (see `input_test.rs`'s direct-emit pattern for the same
        // dispatcher/subscriber relationship).
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::CompletionsFinished(Vec::new()));
        });

        controller.read(&app, |controller, _| {
            assert!(
                matches!(
                    controller.in_flight_native_completions_state,
                    Some(NativeShellCompletionsState::AwaitingPrompt { .. })
                ),
                "a CompletionsFinished event in the wrong phase must not destroy a live, \
                 still-in-progress AwaitingPrompt handshake."
            );
        });
        assert!(
            b_results_rx.try_recv().is_err(),
            "request B's results channel must still be open -- nothing has wrongly answered or \
             dropped it."
        );

        drop(model_events_tx);
    });
}

/// Regression test for TODO.md's follow-up (2): the prompt-phase watchdog can abandon a request
/// whose OSC reply was merely slow, not absent. When that late reply finally arrives, it must
/// answer the shell with a bare EOT terminator (so the shell's `read -d $'\4'` returns) instead
/// of being silently ignored -- which is what left the shell waiting in `read` forever before
/// this fix, since nothing else was ever going to send it a terminator once the app had already
/// dropped the request.
#[test]
fn late_send_completions_prompt_after_watchdog_abandons_it_answers_with_eot_only() {
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
                model_events.clone(),
                line_editor_status,
                sessions,
                executor_command_rx,
                model.clone(),
                ctx,
            )
        });

        // Simulate the prompt-phase watchdog having already fired and abandoned the request:
        // the state is gone, but the "a late reply might still show up" flag is set, exactly as
        // `arm_native_completions_watchdog`'s callback leaves it.
        controller.update(&mut app, |controller, _ctx| {
            controller.in_flight_native_completions_state = None;
            controller.has_pending_late_completions_prompt_reply = true;
        });

        // The shell's OSC reply finally arrives, late.
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt);
        });

        let messages = sender.messages.lock();
        assert_eq!(
            messages.len(),
            1,
            "the late reply must be answered with exactly one write: the bare terminator, and \
             nothing else -- no completion text, since the request that would have supplied it \
             was already dropped."
        );
        assert_input_matches(&messages[0], vec![escape_sequences::C0::EOT]);
        drop(messages);

        controller.read(&app, |controller, _| {
            assert!(
                controller.in_flight_native_completions_state.is_none(),
                "answering a late reply must not resurrect the abandoned request."
            );
            assert!(
                !controller.has_pending_late_completions_prompt_reply,
                "the late-reply flag must be consumed so a second, truly stray \
                 SendCompletionsPrompt does not also get answered with an EOT."
            );
        });

        drop(model_events_tx);
    });
}

/// Diagnostics for the still-unexplained field lockup in TODO.md ("The shell lockup"):
/// `track_pty_write_gate_stall` must notice when the PTY write gate has been shut with writes
/// queued for a while, log exactly once per unbroken stall (not once per queued write), and reset
/// once the stall ends -- either because the gate reopens or because the queue drains out from
/// under it.
#[test]
fn pty_write_gate_stall_is_tracked_and_logged_once_per_unbroken_stretch() {
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

        // The line editor starts out inactive (this harness's default -- see the file-level
        // comment), so `can_write_to_pty` is already shut. Queue a write so there is something
        // pending behind that gate.
        controller.update(&mut app, |controller, ctx| {
            controller.pending_writes.push_back(PtyWrite::Bytes {
                bytes: Cow::Owned(vec![b'x']),
            });
            controller.execute_next_queued_write(ctx);
        });

        controller.read(&app, |controller, _| {
            assert!(
                controller.pty_write_gate_blocked_since.is_some(),
                "a shut gate with writes queued must start tracking when the stall began."
            );
            assert!(
                !controller.has_logged_current_pty_write_stall,
                "a stall that just started must not have logged yet."
            );
        });

        // Fast-forward the tracked start time past the log threshold, as if the stall had been
        // running for a while, and drive the tracker again -- this is the same call
        // `execute_next_queued_write` makes on every subsequent queued write or
        // `LineEditorStatusEvent::Active`.
        controller.update(&mut app, |controller, ctx| {
            controller.pty_write_gate_blocked_since = Some(
                Instant::now() - (PTY_WRITE_GATE_STALL_LOG_THRESHOLD + Duration::from_secs(1)),
            );
            controller.track_pty_write_gate_stall(ctx);
        });

        controller.read(&app, |controller, _| {
            assert!(
                controller.has_logged_current_pty_write_stall,
                "a stall past the threshold must be logged."
            );
        });

        // Driving the tracker again while still stalled must not reset or re-log -- one log line
        // per unbroken stretch, not one per keystroke.
        let blocked_since_before = controller.read(&app, |controller, _| {
            controller
                .pty_write_gate_blocked_since
                .expect("still stalled")
        });
        controller.update(&mut app, |controller, ctx| {
            controller.track_pty_write_gate_stall(ctx);
        });
        controller.read(&app, |controller, _| {
            assert_eq!(
                controller.pty_write_gate_blocked_since,
                Some(blocked_since_before),
                "an already-logged, still-ongoing stall must not restart its clock."
            );
            assert!(
                controller.has_logged_current_pty_write_stall,
                "an already-logged, still-ongoing stall must stay logged, not reset."
            );
        });

        // The queue draining ends the stall and must reset the tracker, even though the gate
        // itself is still shut.
        controller.update(&mut app, |controller, ctx| {
            controller.pending_writes.clear();
            controller.track_pty_write_gate_stall(ctx);
        });
        controller.read(&app, |controller, _| {
            assert!(
                controller.pty_write_gate_blocked_since.is_none(),
                "an empty queue is not a stall, however long the gate has been shut."
            );
            assert!(
                !controller.has_logged_current_pty_write_stall,
                "the log-once flag must reset along with the stall."
            );
        });

        drop(model_events_tx);
    });
}
