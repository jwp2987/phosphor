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
    assert!(
        matches!(message, Message::Input(bytes) | Message::CommandInput(bytes) if bytes.to_vec() == expected_bytes)
    );
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

        assert!(
            !model
                .lock()
                .block_list()
                .is_writing_or_executing_in_band_command()
        );

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

        assert!(
            model
                .lock()
                .block_list()
                .is_writing_or_executing_in_band_command()
        );

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
/// Since #1798 the request is sent only to a line editor the shell's ready signal proved
/// reading, so the test marks the status proven (and active, which opens the write gate); the
/// handler then writes the request itself.
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

        // #1798: the line-report request goes only to a line editor the shell's ready signal
        // PROVED reading (the unproven fallback opens the gate but sends no request).
        line_editor_status.update(&mut app, |status, _| {
            status.set_line_editor_active_for_test(true)
        });
        line_editor_status.update(&mut app, |_, ctx| ctx.emit(LineEditorStatusEvent::Active));

        // The gate is open, so the handler queued the request and wrote it at once.
        controller.update(&mut app, |controller, _| {
            assert!(
                controller.pending_writes.is_empty(),
                "activating a proven line editor should queue the input reporting sequence and write it."
            );
        });
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

        // #1798: only a PROVED line editor is sent the line-report request.
        line_editor_status.update(&mut app, |status, _| {
            status.set_line_editor_active_for_test(true)
        });
        line_editor_status.update(&mut app, |_, ctx| ctx.emit(LineEditorStatusEvent::Active));

        // The handler put the request at the FRONT and wrote it; the in-band command that was
        // already waiting is still queued behind it.
        controller.update(&mut app, |controller, ctx| {
            assert_eq!(
                controller.pending_writes.len(),
                1,
                "the in-band command should still wait behind the input reporting sequence."
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
/// The write is also wrapped in bracketed paste and submitted with CR (see #795): a raw
/// multi-line write with no submit byte just accumulates as keystrokes in a raw-mode line editor
/// (PSReadLine) instead of running.
///
/// This drives a bootstrap sized to span two 4KB chunks (a full first chunk
/// plus a short remainder) through the real `PtyController` -> event-loop path
/// and asserts on what actually reached `TestEventLoopSender`: exactly two
/// writes, arriving as two separate messages (not concatenated into the single
/// write the non-container path would produce -- see
/// `non_container_bootstrap_is_written_as_a_single_unchunked_write` below),
/// which is what a proxy that drops large single writes needs. The chunks'
/// bytes are also asserted to reassemble
/// `BRACKETED_PASTE_START + bootstrap + BRACKETED_PASTE_END + CR` exactly, so a
/// chunk-boundary bug -- including one that splits a marker across chunks --
/// would fail here even if the write count matched.
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
        let mut expected = escape_sequences::BRACKETED_PASTE_START.to_vec();
        expected.extend_from_slice(&bootstrap);
        expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
        expected.push(escape_sequences::C0::CR);
        assert_eq!(
            reassembled, expected,
            "the chunks must reassemble to the bootstrap wrapped in bracketed paste and \
             submitted with CR, byte-for-byte"
        );

        // The open marker must ride in the first chunk (not arrive as a write of its own ahead
        // of the bootstrap), and the close marker plus the submit CR must ride in the last
        // chunk, so #795's fix can never be defeated by something upstream of the PTY treating
        // an early or late fragment as a separate, differently-timed write.
        let Message::Input(first_message) = &messages[0] else {
            panic!(
                "unexpected message on the container bootstrap path: {:?}",
                messages[0]
            );
        };
        assert!(
            first_message.starts_with(escape_sequences::BRACKETED_PASTE_START),
            "the bracketed-paste open marker must be the front of the first chunk: {first_message:?}"
        );
        let Message::Input(last_message) = &messages[1] else {
            panic!(
                "unexpected message on the container bootstrap path: {:?}",
                messages[1]
            );
        };
        let last_bytes: &[u8] = last_message.as_ref();
        assert!(
            last_bytes.ends_with(&[escape_sequences::C0::CR]),
            "the submit CR must be the back of the last chunk: {last_bytes:?}"
        );
        assert!(
            last_bytes[..last_bytes.len() - 1].ends_with(escape_sequences::BRACKETED_PASTE_END),
            "the bracketed-paste close marker must immediately precede the submit CR: {last_bytes:?}"
        );

        drop(model_events_tx);
    });
}

/// Builds a controller on a recording sender, as the container-bootstrap tests do.
#[cfg(feature = "local_fs")]
fn paced_write_controller(
    app: &mut App,
    model: Arc<FairMutex<TerminalModel>>,
    sender: &TestEventLoopSender,
) -> (
    ModelHandle<PtyController<TestEventLoopSender>>,
    impl Sized + use<>,
) {
    let (model_events_tx, model_events_rx) = async_channel::unbounded();
    let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let model_events =
        app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
    let line_editor_status =
        app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
    let sender = sender.clone();
    let controller = app.add_model(|ctx| {
        PtyController::new(
            sender,
            model_events,
            line_editor_status,
            sessions,
            executor_command_rx,
            model,
            ctx,
        )
    });
    (controller, model_events_tx)
}

/// A ^C in the middle of a paced (container) bootstrap write stops it: the chunks after it, and
/// the closing CR that submits the script, are never written. Before, every chunk was its own
/// free-standing timer, so all of them were typed into the live shell after the interrupt.
#[cfg(feature = "local_fs")]
#[test]
fn ctrl_c_mid_bootstrap_writes_no_further_chunks() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        // Three chunks (2 x 4KB + a remainder).
        let bootstrap: Vec<u8> = (0..(2 * 4096 + 100)).map(|i| (i % 251) as u8).collect();
        let session_info = docker_exec_session_info();
        controller.update(&mut app, |controller, ctx| {
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                bootstrap.into(),
            );
            assert_eq!(controller.paced_writes_remaining(), 2, "first chunk is out");
            controller.write_bytes(&[escape_sequences::C0::ETX][..], ctx);
            assert_eq!(controller.paced_writes_remaining(), 0);
        });
        // Wait well past the pacing gaps: nothing else may be written.
        Timer::after(Duration::from_millis(400)).await;
        let messages = sender.messages.lock();
        let inputs: Vec<&Message> = messages
            .iter()
            .filter(|m| matches!(m, Message::Input(_)))
            .collect();
        assert_eq!(inputs.len(), 2, "first chunk and the ^C only: {messages:?}");
        assert!(
            !messages.iter().any(|m| matches!(
                m,
                Message::Input(bytes) if bytes.ends_with(&[escape_sequences::C0::CR])
            )),
            "the closing CR must never be typed after the interrupt"
        );
    });
}

/// Shutting the pty down (tab close) drops the rest of a paced bootstrap write too.
#[cfg(feature = "local_fs")]
#[test]
fn shutdown_mid_bootstrap_writes_no_further_chunks() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        let bootstrap: Vec<u8> = (0..(2 * 4096 + 100)).map(|i| (i % 251) as u8).collect();
        let session_info = docker_exec_session_info();
        controller.update(&mut app, |controller, ctx| {
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                bootstrap.into(),
            );
            controller.shutdown_pty(ctx);
        });
        Timer::after(Duration::from_millis(400)).await;
        let messages = sender.messages.lock();
        let inputs = messages
            .iter()
            .filter(|m| matches!(m, Message::Input(_)))
            .count();
        assert_eq!(
            inputs, 1,
            "only the chunk written before the close: {messages:?}"
        );
    });
}

/// Chunks go out in order, one per pacing step, and a second bootstrap replaces the first's
/// leftovers instead of interleaving with them.
#[cfg(feature = "local_fs")]
#[test]
fn a_newer_paced_write_replaces_the_unwritten_chunks_of_the_older_one() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        let session_info = docker_exec_session_info();
        let first: Vec<u8> = vec![b'a'; 2 * 4096 + 10];
        let second: Vec<u8> = vec![b'b'; 10];
        controller.update(&mut app, |controller, ctx| {
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                first.into(),
            );
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                second.into(),
            );
        });
        Timer::after(Duration::from_millis(400)).await;
        let messages = sender.messages.lock();
        let written: Vec<u8> = messages
            .iter()
            .filter_map(|m| match m {
                Message::Input(bytes) => Some(bytes.to_vec()),
                _ => None,
            })
            .flatten()
            .collect();
        let a_count = written.iter().filter(|b| **b == b'a').count();
        assert_eq!(
            a_count, 4096,
            "only the first chunk of the superseded write: {a_count}"
        );
        assert_eq!(written.iter().filter(|b| **b == b'b').count(), 10);
    });
}

/// The messages a bootstrap produces with the writer-owned job queue on (#1384): ONE
/// `SetupChunked` job for the whole paced script, instead of a timer-chained `Input` per chunk.
#[cfg(feature = "local_fs")]
#[test]
fn queued_container_bootstrap_is_one_setup_job_with_the_pacing_named() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        let bootstrap: Vec<u8> = (0..(4096 + 100)).map(|i| (i % 256) as u8).collect();
        let session_info = docker_exec_session_info();
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(true);
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                bootstrap.clone().into(),
            );
        });
        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 1, "one job, no timers: {messages:?}");
        let Message::SetupChunked { chunks, pacing, .. } = &messages[0] else {
            panic!("expected a setup job: {:?}", messages[0]);
        };
        assert!(matches!(
            pacing,
            Pacing::Gap { dur, .. } if *dur == Duration::from_millis(50)
        ));
        assert_eq!(chunks.len(), 2);
        let mut expected = escape_sequences::BRACKETED_PASTE_START.to_vec();
        expected.extend_from_slice(&bootstrap);
        expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
        expected.push(escape_sequences::C0::CR);
        assert_eq!(chunks.concat(), expected);
        assert!(chunks[0].starts_with(escape_sequences::BRACKETED_PASTE_START));
        assert!(chunks[1].ends_with(&[escape_sequences::C0::CR]));
    });
}

/// A ^C aborts the setup job before the interrupt goes out, and the submitted-command discard
/// still follows, as on the timer path.
#[cfg(feature = "local_fs")]
#[test]
fn queued_ctrl_c_aborts_the_setup_job_then_interrupts() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        let bootstrap: Vec<u8> = (0..(2 * 4096 + 100)).map(|i| (i % 251) as u8).collect();
        let session_info = docker_exec_session_info();
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(true);
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                bootstrap.into(),
            );
            controller.write_bytes(&[escape_sequences::C0::ETX][..], ctx);
        });
        let messages = sender.messages.lock();
        assert!(
            matches!(messages[0], Message::SetupChunked { .. }),
            "{messages:?}"
        );
        assert!(
            matches!(messages[1], Message::Abort(AbortScope::Setup)),
            "{messages:?}"
        );
        assert!(
            matches!(messages[2], Message::DiscardQueuedInput),
            "{messages:?}"
        );
        assert!(
            matches!(&messages[3], Message::Input(bytes) if bytes.as_ref() == [escape_sequences::C0::ETX]),
            "{messages:?}"
        );
        assert_eq!(messages.len(), 4);
    });
}

/// Shutting the pty down aborts the job before the shutdown.
#[cfg(feature = "local_fs")]
#[test]
fn queued_shutdown_aborts_the_setup_job() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        let session_info = docker_exec_session_info();
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(true);
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                vec![b'a'; 2 * 4096 + 10].into(),
            );
            controller.shutdown_pty(ctx);
        });
        let messages = sender.messages.lock();
        assert!(
            matches!(messages[0], Message::SetupChunked { .. }),
            "{messages:?}"
        );
        assert!(
            matches!(messages[1], Message::Abort(AbortScope::Setup)),
            "{messages:?}"
        );
        assert!(matches!(messages[2], Message::Shutdown), "{messages:?}");
        assert_eq!(messages.len(), 3);
    });
}

/// A newer bootstrap aborts the older job before it queues its own, and with no job outstanding
/// a ^C sends no abort at all.
#[cfg(feature = "local_fs")]
#[test]
fn queued_newer_bootstrap_aborts_the_older_job() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        let session_info = docker_exec_session_info();
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(true);
            // No job yet: a ^C must not send an abort.
            controller.write_bytes(&[escape_sequences::C0::ETX][..], ctx);
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                vec![b'a'; 2 * 4096 + 10].into(),
            );
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                vec![b'b'; 10].into(),
            );
        });
        let messages = sender.messages.lock();
        let kinds: Vec<&str> = messages
            .iter()
            .map(|m| match m {
                Message::Input(_) => "input",
                Message::DiscardQueuedInput => "discard",
                Message::SetupChunked { .. } => "setup",
                Message::Abort(_) => "abort",
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            ["discard", "input", "setup", "abort", "setup"],
            "{messages:?}"
        );
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

/// A container-subshell bootstrap short enough to fit in a single 4KB chunk must still come out
/// wrapped in bracketed paste and submitted with CR, all within that one chunk: the open marker,
/// the bootstrap, the close marker, then the CR, in that order.
///
/// `container_subshell_bootstrap_is_written_in_bounded_chunks` above cannot exercise this case --
/// it uses a bootstrap sized to span two chunks specifically so the open marker (chunk 0) and the
/// close marker plus CR (chunk 1) land in different writes. Here they all land in the same `Vec`,
/// which is the case where prepending the open marker and then appending the close marker and CR
/// could clobber each other if done in the wrong order or against the wrong end. See #795.
#[cfg(feature = "local_fs")]
#[test]
fn container_subshell_bootstrap_smaller_than_a_chunk_is_wrapped_and_submitted_with_cr() {
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

        let bootstrap: Vec<u8> = b"echo hi".to_vec();
        let session_info = docker_exec_session_info();

        controller.update(&mut app, |controller, ctx| {
            controller.write_bootstrap_script_to_shell(
                &session_info,
                ctx,
                ShellType::Bash,
                bootstrap.clone().into(),
            );
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while sender.messages.lock().is_empty() && Instant::now() < deadline {
            Timer::after(Duration::from_millis(10)).await;
        }

        let messages = sender.messages.lock();
        assert_eq!(
            messages.len(),
            1,
            "a bootstrap smaller than the chunk size must still be a single write"
        );
        let mut expected = escape_sequences::BRACKETED_PASTE_START.to_vec();
        expected.extend_from_slice(&bootstrap);
        expected.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
        expected.push(escape_sequences::C0::CR);
        assert_input_matches(&messages[0], expected);

        drop(model_events_tx);
    });
}

/// The rc-file bootstrap method sources a temp script with a line like ` . '<path>'`
/// (PowerShell) or ` source '<path>'` (other shells) and then submits it with
/// `write_terminating_bootstrap_bytes`. Submitting with LF only runs the line when the tty is
/// cooked and ICRNL translates LF to CR for the line discipline; a raw-mode line editor
/// (PSReadLine, for `pwsh`) reads the byte as-is and treats LF as Ctrl+J, so the sourced line
/// never ran on Linux/macOS even though `should_use_rc_file_bootstrap_method` picks this path for
/// PowerShell on every platform. See #794.
///
/// Not gated by target platform: `write_terminating_bootstrap_bytes` no longer has a
/// `cfg(unix)`/`cfg(windows)` branch to select between, so there is nothing left to run this test
/// under besides the one path -- which is the fix.
#[cfg(feature = "local_fs")]
#[test]
fn rc_file_bootstrap_line_is_terminated_with_carriage_return() {
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

        controller.update(&mut app, |controller, ctx| {
            controller.source_bootstrap_script(
                b"/tmp/phosphor-bootstrap.ps1".to_vec(),
                ShellType::PowerShell,
                ctx,
            );
        });

        let messages = sender.messages.lock();
        let mut written = Vec::new();
        for message in messages.iter() {
            match message {
                Message::Input(bytes) => written.extend_from_slice(bytes),
                other => panic!("unexpected message on the rc-file bootstrap path: {other:?}"),
            }
        }
        assert!(
            written.ends_with(&[escape_sequences::C0::CR]),
            "the rc-file bootstrap's source line must be submitted with CR, not LF: {written:?}"
        );
        assert!(
            !written.ends_with(&[escape_sequences::C0::LF]),
            "a trailing LF is Ctrl+J to a raw-mode line editor (PSReadLine), not a submit: \
             {written:?}"
        );

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
                    remote_first: false,
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
                    remote_first: false,
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
                    remote_first: false,
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

/// Regression test for TODO.md's follow-up (2), reworked after review: a `SendCompletionsPrompt`
/// event with no matching live `AwaitingPrompt` state -- e.g. a late reply for a request the
/// prompt watchdog already abandoned -- must write *nothing at all* to the PTY, not even a bare
/// terminator.
///
/// An earlier version of this fix answered such a late reply with a bare EOT, reasoning that the
/// shell must still be blocked in `read -d $'\4'` waiting for one. That is unsafe: the shell's
/// `read` can return for reasons the app cannot observe (most plausibly Ctrl-C, a very normal
/// reaction to a Tab that visibly did nothing after the 2s watchdog fires), and a blind EOT sent
/// after that point is Ctrl-D typed at a live, empty prompt -- which exits the shell. Recovering
/// the shell from a stuck handshake belongs to the shell side now (a bounded `read -t` in
/// `warp_read_completion_buffer`, `zsh_body.sh`); the app's only safe move is to never write
/// anything for a request it no longer has live state for.
#[test]
fn late_send_completions_prompt_with_no_live_state_writes_nothing_to_the_pty() {
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
        // there is no live state left for this event to match.
        controller.update(&mut app, |controller, _ctx| {
            controller.in_flight_native_completions_state = None;
        });

        // The shell's OSC reply finally arrives, late.
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt {
                token: Some("4242".to_owned()),
            });
        });

        assert!(
            sender.messages.lock().is_empty(),
            "a SendCompletionsPrompt event with no matching live AwaitingPrompt state must not \
             write anything to the PTY -- not completion text, and not a bare terminator either, \
             since the app cannot tell whether the shell is still blocked in read() or already \
             back at a live prompt."
        );

        controller.read(&app, |controller, _| {
            assert!(
                controller.in_flight_native_completions_state.is_none(),
                "handling a late reply must not resurrect the abandoned request."
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
        // pending behind that gate, and use a short threshold (rather than the real
        // multi-second `PTY_WRITE_GATE_STALL_LOG_THRESHOLD`) so this test does not need to sleep
        // for it -- the same reason the watchdog tests above pass short timeouts explicitly.
        let threshold = Duration::from_millis(20);
        controller.update(&mut app, |controller, ctx| {
            controller.pending_writes.push_back(PtyWrite::Bytes {
                bytes: Cow::Owned(vec![b'x']),
            });
            controller.track_pty_write_gate_stall(threshold, ctx);
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

        // Long enough for the threshold to elapse; drive the tracker again -- this is the same
        // call `execute_next_queued_write` makes on every subsequent queued write or
        // `LineEditorStatusEvent::Active`.
        Timer::after(Duration::from_millis(100)).await;
        controller.update(&mut app, |controller, ctx| {
            controller.track_pty_write_gate_stall(threshold, ctx);
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
            controller.track_pty_write_gate_stall(threshold, ctx);
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
            controller.track_pty_write_gate_stall(threshold, ctx);
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

/// Regression test for phosphor#770 follow-up review, item 2: `run_native_shell_completions` must
/// not start a new native-completions request while one is already in flight, in either phase.
///
/// `AwaitingResults` does not gate `can_write_to_pty` (typing must not block on completions
/// results), so on the old code a second request -- e.g. from `CompletionsTrigger::AsYouType`
/// firing again while the first request's results are still pending -- would be queued and could
/// be dequeued and dispatched immediately, unconditionally overwriting
/// `in_flight_native_completions_state`. That drops the first request's `results_tx` and, since
/// neither event carries a request id, hands the first request's later, now-phase-matching
/// `CompletionsFinished` reply to the *second* request instead.
///
/// This drives `run_native_shell_completions` directly with a request already in flight (rather
/// than via `execute_next_queued_write`'s line-editor gate, which this harness's line editor
/// never clears -- see the file-level comment) because the defect is in
/// `run_native_shell_completions` itself queuing a second request at all, not in when queued
/// writes get dispatched.
#[test]
fn run_native_shell_completions_does_not_start_a_second_request_while_one_is_in_flight() {
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

        // Request A is already in flight, past the prompt phase and waiting on results -- the
        // phase that does not gate `can_write_to_pty`.
        let (a_results_tx, a_results_rx) = async_channel::unbounded();
        controller.update(&mut app, |controller, _ctx| {
            controller.in_flight_native_completions_state =
                Some(NativeShellCompletionsState::AwaitingResults {
                    results_tx: a_results_tx,
                });
        });

        // Request B tries to start -- e.g. the next as-you-type keystroke -- while A is still
        // live.
        let (b_results_tx, b_results_rx) = async_channel::unbounded();
        controller.update(&mut app, |controller, ctx| {
            controller.run_native_shell_completions(
                "request b".to_owned(),
                NativeCompletionsRequestOptions {
                    remote_first: false,
                },
                b_results_tx,
                ctx,
            );
        });

        controller.read(&app, |controller, _| {
            assert!(
                matches!(
                    controller.in_flight_native_completions_state,
                    Some(NativeShellCompletionsState::AwaitingResults { .. })
                ),
                "starting request B while request A is in flight must not touch A's state at \
                 all -- not even queue something that could later clobber it."
            );
            assert!(
                controller.pending_writes.is_empty(),
                "request B must not be queued while a request is already in flight -- queuing it \
                 would let it be dequeued and dispatched behind A's back the moment the line \
                 editor gate (which AwaitingResults does not touch) allows it."
            );
        });
        assert!(
            b_results_rx.try_recv().is_err(),
            "request B must be dropped outright (its results_tx closed) rather than queued or \
             silently kept waiting forever."
        );

        // Request A's own, later reply must still reach request A, not have been redirected to B.
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::CompletionsFinished(vec![]));
        });
        assert!(
            a_results_rx.try_recv().is_ok(),
            "request A's own CompletionsFinished reply must still reach request A."
        );

        drop(model_events_tx);
    });
}

/// The stall log must fire on its own once the threshold elapses, even if nothing else ever
/// drives `execute_next_queued_write` again -- e.g. the user stops typing entirely once the gate
/// sticks, which is a very plausible reaction to a pane that stopped responding. Without a
/// self-armed timer, the log line would depend on some future keystroke or `LineEditorStatus`
/// transition that may never come.
#[test]
fn pty_write_gate_stall_logs_itself_without_further_input() {
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

        // Start a stall (line editor inactive, a write queued behind it) with a short threshold
        // -- rather than the real multi-second `PTY_WRITE_GATE_STALL_LOG_THRESHOLD` -- so this
        // test does not need to sleep for it, the same reason the watchdog tests above pass short
        // timeouts explicitly.
        let threshold = Duration::from_millis(20);
        controller.update(&mut app, |controller, ctx| {
            controller.pending_writes.push_back(PtyWrite::Bytes {
                bytes: Cow::Owned(vec![b'x']),
            });
            controller.track_pty_write_gate_stall(threshold, ctx);
        });

        controller.read(&app, |controller, _| {
            assert!(
                !controller.has_logged_current_pty_write_stall,
                "must not have logged yet -- only the timer armed when the stall began should \
                 do that, and it has not fired yet."
            );
        });

        // Nothing further drives `execute_next_queued_write` or `track_pty_write_gate_stall` from
        // here -- only the one-shot timer armed when the stall began should fire and log this.
        Timer::after(Duration::from_millis(200)).await;

        controller.read(&app, |controller, _| {
            assert!(
                controller.has_logged_current_pty_write_stall,
                "the self-armed timer must log the stall on its own, with no further input."
            );
        });

        drop(model_events_tx);
    });
}

/// #848: closing a pane drops an agent command still parked in the write queue (waiting for
/// the line editor), so it never runs in the closed pane. Only non-in-band commands go:
/// queued keystrokes and in-band commands stay.
#[test]
fn discard_pending_command_writes_drops_only_non_in_band_commands() {
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
                bytes: b"keystrokes".to_vec().into(),
            });
            controller.pending_writes.push_back(PtyWrite::Command {
                command: "touch ~/closed-tab-ran".to_owned(),
                shell_type: ShellType::Zsh,
                in_band_command_id: None,
                before_write_fn: None,
            });
            controller.pending_writes.push_back(PtyWrite::Command {
                command: "in-band".to_owned(),
                shell_type: ShellType::Zsh,
                in_band_command_id: Some("in-band-1".to_owned()),
                before_write_fn: None,
            });

            controller.discard_pending_command_writes();

            assert_eq!(controller.pending_writes.len(), 2);
            assert!(matches!(
                &controller.pending_writes[0],
                PtyWrite::Bytes { bytes } if &bytes[..] == b"keystrokes"
            ));
            assert!(matches!(
                &controller.pending_writes[1],
                PtyWrite::Command { in_band_command_id: Some(id), .. } if id == "in-band-1"
            ));
        });
        assert!(sender.messages.lock().is_empty());

        drop(model_events_tx);
    });
}

mod typeahead_handoff {
    //! `FeatureFlag::TypeaheadHandoff` (#826/#827): the request (`ESC i`) is written only once no
    //! Enter-terminated user line can still be queued, user bytes never overtake the editor's
    //! command, and nothing that could kill the user's unreported line is drained meanwhile.

    use super::*;
    use crate::terminal::model::ansi::{Handler, PreexecValue};
    use crate::terminal::model::terminal_model::ExitReason;
    use crate::terminal::model::typeahead_handoff::{
        HandoffState, InterruptOutcome, ReportDisposition, UserBytesRoute, YoungHoldEnd,
        YoungHoldExpiry,
    };

    const SHELL: ShellType = ShellType::Zsh;
    const REQUEST: [u8; 2] = [escape_sequences::C0::ESC, b'i'];

    struct Harness {
        model: Arc<FairMutex<TerminalModel>>,
        controller: ModelHandle<PtyController<TestEventLoopSender>>,
        line_editor_status: ModelHandle<LineEditorStatus>,
        model_events: ModelHandle<ModelEventDispatcher>,
        sender: TestEventLoopSender,
        _model_events_tx: async_channel::Sender<crate::terminal::event::Event>,
    }

    /// A zsh session at its prompt, with the line editor active.
    fn harness(app: &mut App) -> Harness {
        harness_with(app, zsh_session_info())
    }

    /// A session of `session_info`'s shell at its prompt, with the line editor active.
    fn harness_with(app: &mut App, session_info: SessionInfo) -> Harness {
        let model = terminal_model();
        model
            .lock()
            .block_list_mut()
            .early_output_mut()
            .init_session(&session_info);
        model.lock().start_command_execution();
        shell_runs_a_command(&model, "bootstrap");
        let (model_events_tx, model_events_rx) = async_channel::unbounded();
        let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let sessions = app.add_model(|_| Sessions::new_for_test());
        sessions.update(app, |sessions, _| {
            sessions.register_session_for_test(session_info)
        });
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
        model_events.update(app, |dispatcher, _| {
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
        line_editor_status.update(app, |status, _| {
            status.set_line_editor_active_for_test(true)
        });
        Harness {
            model,
            controller,
            line_editor_status,
            model_events,
            sender,
            _model_events_tx: model_events_tx,
        }
    }

    /// The shell runs `command` (from the editor or from typeahead) and returns to its prompt.
    fn shell_runs_a_command(model: &Arc<FairMutex<TerminalModel>>, command: &str) {
        let mut model = model.lock();
        model.preexec(PreexecValue {
            command: command.to_owned(),
            session_id: None,
        });
        model.finish_block();
    }

    /// The shell's line editor becomes active again after a prompt.
    fn line_editor_becomes_active(app: &mut App, harness: &Harness) {
        harness.line_editor_status.update(app, |status, ctx| {
            status.set_line_editor_active_for_test(true);
            ctx.emit(LineEditorStatusEvent::Active);
        });
    }

    /// What the terminal view does with a key while the pty owns input.
    fn user_types(app: &mut App, harness: &Harness, bytes: &'static [u8]) {
        assert_eq!(
            harness
                .model
                .lock()
                .typeahead_handoff_mut()
                .on_user_bytes(bytes),
            UserBytesRoute::WriteCounted
        );
        harness.controller.update(app, |controller, ctx| {
            controller.write_handoff_user_bytes(Cow::Borrowed(bytes), ctx)
        });
    }

    fn submit(app: &mut App, harness: &Harness, command: &str) {
        let outcome = harness.controller.update(app, |controller, ctx| {
            controller.write_command(command, SHELL, CommandExecutionSource::User, ctx)
        });
        assert_eq!(outcome, StartCommandOutcome::Accepted);
    }

    /// The queued-prompts panel runs a command.
    fn fire_queued_command(app: &mut App, harness: &Harness) {
        let outcome = harness.controller.update(app, |controller, ctx| {
            controller.write_command("queued", SHELL, CommandExecutionSource::QueuedCommand, ctx)
        });
        assert_eq!(outcome, StartCommandOutcome::Accepted);
    }

    fn written(harness: &Harness) -> Vec<Vec<u8>> {
        harness
            .sender
            .messages
            .lock()
            .iter()
            .filter_map(|message| {
                if let Message::Input(bytes) | Message::CommandInput(bytes) = message {
                    Some(bytes.to_vec())
                } else {
                    None
                }
            })
            .collect()
    }

    /// #1384 P4: the hand-off's accounting is the same on the legacy route and on the write
    /// queue's. One write per chunk (a paste-sized chunk is a `PasteInput` on the queue and a
    /// `CommandInput` without), and each retires exactly one in-flight count.
    #[test]
    fn handoff_user_write_accounting_is_the_same_with_and_without_the_write_queue() {
        for queue in [false, true] {
            App::test((), |mut app| async move {
                let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
                let harness = harness(&mut app);
                harness.controller.update(&mut app, |controller, _| {
                    controller.force_write_queue(queue)
                });
                // A command is running, so the pty owns input and user writes are counted (an
                // editor-owned write is not, and `user_types` asserts the counted route).
                submit(&mut app, &harness, "sleep 1");
                harness.sender.messages.lock().clear();
                let paste: &'static [u8] =
                    Box::leak(vec![b'x'; PASTE_WRITE_MIN_BYTES].into_boxed_slice());
                user_types(&mut app, &harness, paste);
                user_types(&mut app, &harness, b"k");
                let messages = harness.sender.messages.lock();
                assert_eq!(messages.len(), 2, "queue={queue}: {messages:?}");
                match (&messages[0], queue) {
                    (Message::PasteInput(bytes), true) | (Message::CommandInput(bytes), false) => {
                        assert_eq!(&bytes[..], paste)
                    }
                    (other, _) => panic!("queue={queue}: unexpected {other:?}"),
                }
                assert!(matches!(&messages[1], Message::Input(bytes) if &bytes[..] == b"k"));
                assert_eq!(
                    harness.model.lock().typeahead_handoff().writes_in_flight(),
                    0,
                    "queue={queue}: each counted write retires one in-flight count"
                );
            });
        }
    }

    #[test]
    fn request_waits_for_queued_lines_and_holds_queued_writes_meanwhile() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 1");
            // `a` and Enter, then `b`, typed while `sleep 1` runs.
            user_types(&mut app, &harness, b"a\rb");

            shell_runs_a_command(&harness.model, "sleep 1");
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(
                written(&harness),
                vec![
                    bytes_to_execute_command("sleep 1", SHELL, false),
                    b"a\rb".to_vec()
                ],
                "the shell is about to run `a` from the tty, so a request now would land in the \
                 middle of the user's lines"
            );

            // An in-band command queued meanwhile would kill `b` with its kill prefix.
            let (cancel_tx, _cancel_rx) = async_channel::unbounded();
            harness.controller.update(&mut app, |controller, ctx| {
                controller.is_user_command_executing = false;
                controller.queue_in_band_command(
                    "echo in-band",
                    SHELL,
                    "command-id".to_owned(),
                    cancel_tx,
                    ctx,
                );
                assert_eq!(controller.pending_writes.len(), 1);
            });
            assert_eq!(written(&harness).len(), 2);

            // The shell runs `a` and returns to its prompt with `b` in its line.
            shell_runs_a_command(&harness.model, "a");
            line_editor_becomes_active(&mut app, &harness);
            let written = written(&harness);
            assert_eq!(written.len(), 4);
            assert_eq!(written[2], REQUEST.to_vec());
            assert_eq!(
                written[3],
                bytes_to_execute_command("echo in-band", SHELL, false)
            );
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::AwaitingReport
            );
            assert!(
                !harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .routes_keys_to_pty()
            );
        });
    }

    #[test]
    fn nothing_typed_hands_back_without_a_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "true");
            assert!(
                harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .routes_keys_to_pty()
            );

            shell_runs_a_command(&harness.model, "true");
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(
                written(&harness),
                vec![bytes_to_execute_command("true", SHELL, false)]
            );
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned
            );
        });
    }

    #[test]
    fn keys_typed_behind_a_queued_command_follow_it() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            harness.line_editor_status.update(&mut app, |status, _| {
                status.set_line_editor_active_for_test(false)
            });
            submit(&mut app, &harness, "pwd");
            user_types(&mut app, &harness, b"ls\r");
            assert!(
                written(&harness).is_empty(),
                "the keys must not overtake the queued command into the shell's line"
            );

            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(
                written(&harness),
                vec![
                    bytes_to_execute_command("pwd", SHELL, false),
                    b"ls\r".to_vec()
                ]
            );
            assert_eq!(
                harness.model.lock().typeahead_handoff().writes_in_flight(),
                0
            );
        });
    }

    #[test]
    fn keys_held_behind_a_deferred_enter_follow_the_command() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 1");
            // Typeahead for a command that has been running a while (not a line begun right
            // after it, #933).
            harness
                .model
                .lock()
                .typeahead_handoff_mut()
                .age_last_activity_for_test();
            user_types(&mut app, &harness, b"ec");
            shell_runs_a_command(&harness.model, "sleep 1");
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(written(&harness).last(), Some(&REQUEST.to_vec()));

            // Enter in the editor before the report: deferred, and later keys are held (as
            // entries of the view's queue; the controller keeps no bytes for them).
            let gate = {
                let mut model = harness.model.lock();
                let mut handoff = model.typeahead_handoff_mut();
                assert!(handoff.defer_enter());
                let gate = handoff.deferred_gate().expect("a gate");
                assert_eq!(handoff.on_report(), ReportDisposition::InsertThenSubmit);
                gate
            };
            // The deferred submit, with the report (`ec`) inserted in front of the editor text.
            submit(&mut app, &harness, "echo 1");
            let written = written(&harness);
            assert_eq!(
                written.last(),
                Some(&bytes_to_execute_command("echo 1", SHELL, false))
            );
            // The keys behind the Enter follow the command: the hand-off records the end for the
            // view, which writes them behind the command and counts them.
            let mut model = harness.model.lock();
            let mut handoff = model.typeahead_handoff_mut();
            assert_eq!(
                handoff.take_young_hold_ends(),
                vec![(gate, YoungHoldEnd::FollowCommand)]
            );
            handoff.count_followed_young_hold(gate, b"pwd\r");
            assert_eq!(
                handoff.state(),
                &HandoffState::PtyOwned {
                    wrote_user_bytes: false,
                    enters_written: 2,
                    precmds_seen: 0,
                }
            );
        });
    }

    /// A command from the queued-prompts panel is not the hand-off's, but its prompt is still
    /// counted; without counting its Enter the request would go out while `a` is still queued
    /// and land in cooked mode behind it.
    #[test]
    fn a_queued_command_does_not_let_the_request_overtake_queued_lines() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 1");
            user_types(&mut app, &harness, b"a\r");
            shell_runs_a_command(&harness.model, "sleep 1");
            line_editor_becomes_active(&mut app, &harness);

            let outcome = harness.controller.update(&mut app, |controller, ctx| {
                controller.write_command(
                    "queued",
                    SHELL,
                    CommandExecutionSource::QueuedCommand,
                    ctx,
                )
            });
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert_eq!(
                written(&harness).last(),
                Some(&bytes_to_execute_command("queued", SHELL, false)),
                "nothing typed is in the shell's line, so the command goes out at once"
            );

            for command in ["a", "queued"] {
                shell_runs_a_command(&harness.model, command);
                line_editor_becomes_active(&mut app, &harness);
                assert!(
                    !written(&harness).contains(&REQUEST.to_vec()),
                    "no request while the shell still has lines to run"
                );
            }
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned
            );
        });
    }

    /// A queued-prompts-panel command fired while the user's unreported typeahead may be in the
    /// shell's line waits for the request, which goes out ahead of it, instead of killing that
    /// typeahead with its kill prefix.
    #[test]
    fn a_queued_command_waits_behind_an_unreported_user_line() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 1");
            user_types(&mut app, &harness, b"a\rb");
            shell_runs_a_command(&harness.model, "sleep 1");
            line_editor_becomes_active(&mut app, &harness);

            let outcome = harness.controller.update(&mut app, |controller, ctx| {
                controller.write_command(
                    "queued",
                    SHELL,
                    CommandExecutionSource::QueuedCommand,
                    ctx,
                )
            });
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert_eq!(
                written(&harness).len(),
                2,
                "the command must not reach the shell's line while `b` is unreported"
            );

            // The shell runs `a`; at its prompt `b` is the line: request it, then the command.
            shell_runs_a_command(&harness.model, "a");
            line_editor_becomes_active(&mut app, &harness);
            let written = written(&harness);
            assert_eq!(
                &written[2..],
                &[
                    REQUEST.to_vec(),
                    bytes_to_execute_command("queued", SHELL, false)
                ]
            );
        });
    }

    /// Ctrl-C at the prompt, then typing: zsh runs no prompt cycle, so no activation comes. The
    /// view's settle timer asks the controller to retry, which writes the request.
    #[test]
    fn a_retry_after_ctrl_c_at_the_prompt_writes_the_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "true");
            shell_runs_a_command(&harness.model, "true");
            harness.line_editor_status.update(&mut app, |status, _| {
                status.set_line_editor_active_for_test(true)
            });
            user_types(&mut app, &harness, b"abc");
            user_types(&mut app, &harness, &[escape_sequences::C0::ETX]);
            assert_eq!(
                harness.model.lock().typeahead_handoff_mut().on_interrupt(),
                InterruptOutcome::AtPrompt
            );
            user_types(&mut app, &harness, b"ls");
            assert!(
                !harness
                    .model
                    .lock()
                    .typeahead_handoff_mut()
                    .try_commit_at_prompt()
            );

            harness.controller.update(&mut app, |controller, ctx| {
                controller.retry_typeahead_handoff_request(ctx)
            });
            assert_eq!(written(&harness).last(), Some(&REQUEST.to_vec()));
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::AwaitingReport
            );
        });
    }

    /// The shell's line editor is declared active by the fallback, not by its ready signal.
    fn line_editor_becomes_active_unproven(app: &mut App, harness: &Harness) {
        harness.line_editor_status.update(app, |status, ctx| {
            status.set_line_editor_active_unproven_for_test();
            ctx.emit(LineEditorStatusEvent::Active);
        });
    }

    /// #1798/#1672: after a Ctrl-C the prompt's Precmd arrives while the shell is still
    /// recovering and the fallback opens the write gate before the line editor reads. No
    /// line-report request (ESC i) may be written then: the tty would echo it as `^[i`. The keys
    /// typed meanwhile go out as typed, in order. The hand-off and its gate are the pty
    /// controller's, shared by the Legacy and Table input modes.
    #[test]
    fn an_unproven_line_editor_after_ctrl_c_gets_no_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 30");
            user_types(&mut app, &harness, b"ab");
            // Ctrl-C (twice: the second one finds the prompt cycle not yet parsed).
            user_types(&mut app, &harness, &[escape_sequences::C0::ETX]);
            user_types(&mut app, &harness, &[escape_sequences::C0::ETX]);
            shell_runs_a_command(&harness.model, "sleep 30");
            line_editor_becomes_active_unproven(&mut app, &harness);
            user_types(&mut app, &harness, b"echo hi");
            harness.controller.update(&mut app, |controller, ctx| {
                controller.retry_typeahead_handoff_request(ctx)
            });
            let written = written(&harness);
            assert!(
                !written.iter().any(|w| w == &REQUEST.to_vec()),
                "no request may be written to a line editor the shell has not signalled: {written:?}"
            );
            let joined: Vec<u8> = written.concat();
            let pos = |needle: &[u8]| joined.windows(needle.len()).position(|w| w == needle);
            assert!(
                pos(b"ab") < pos(b"echo hi") && pos(b"echo hi").is_some(),
                "keys keep their typed order"
            );
        });
    }

    /// The same prompt, once the shell's own signal has proven its line editor reads, is asked
    /// for its line as before (a late signal after the fallback proves it too).
    #[test]
    fn a_proven_line_editor_after_ctrl_c_still_gets_the_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 30");
            user_types(&mut app, &harness, b"ab");
            shell_runs_a_command(&harness.model, "sleep 30");
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(written(&harness).last(), Some(&REQUEST.to_vec()));
        });
    }

    /// The queued-prompts panel (or the agent) fires right after the prompt, before the line
    /// editor is active, while `foo` -- typed during the last command, no Enter -- sits in the
    /// shell's line. The command waits without starting its block, so the prompt still reads as
    /// a prompt and the request goes out at activation, ahead of the command.
    #[test]
    fn a_command_fired_before_activation_waits_for_the_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 2");
            user_types(&mut app, &harness, b"foo");
            shell_runs_a_command(&harness.model, "sleep 2");

            fire_queued_command(&mut app, &harness);
            assert_eq!(written(&harness).len(), 2);
            assert!(
                !harness.model.lock().block_list().active_block().started(),
                "a held command must not start its block before it is written"
            );

            line_editor_becomes_active(&mut app, &harness);
            let written = written(&harness);
            assert_eq!(
                &written[2..],
                &[
                    REQUEST.to_vec(),
                    bytes_to_execute_command("queued", SHELL, false)
                ]
            );
            assert!(harness.model.lock().block_list().active_block().started());
        });
    }

    /// Ctrl-C discards the shell's line, so the reason to hold the command is gone: it goes out
    /// behind the Ctrl-C (and so the agent or panel still gets its command run) with no request.
    #[test]
    fn ctrl_c_releases_a_command_held_behind_the_user_line() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 2");
            user_types(&mut app, &harness, b"foo");
            shell_runs_a_command(&harness.model, "sleep 2");
            fire_queued_command(&mut app, &harness);

            user_types(&mut app, &harness, &[escape_sequences::C0::ETX]);
            assert_eq!(
                harness.model.lock().typeahead_handoff_mut().on_interrupt(),
                InterruptOutcome::AtPrompt
            );
            line_editor_becomes_active(&mut app, &harness);
            let written = written(&harness);
            assert_eq!(
                &written[2..],
                &[
                    vec![escape_sequences::C0::ETX],
                    bytes_to_execute_command("queued", SHELL, false)
                ]
            );
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned
            );
        });
    }

    /// PowerShell's marker, as the model thread applies it (stream order) and then as the
    /// controller receives it.
    fn powershell_marks_line_editor_reading(app: &mut App, harness: &Harness) {
        harness
            .model
            .lock()
            .typeahead_handoff_mut()
            .on_line_editor_reading(None, None);
        shell_marks_line_editor_reading(app, harness);
    }

    /// An Enter consumed by a program (`read x` answered `y` Enter) is answered by no prompt.
    /// Owner rule (a timer never resets counts or commits): no quiet period releases it any more
    /// -- the pty keeps input, a plain terminal -- and the shell's own event does: PowerShell's
    /// `LineEditorReading` marker (ConPTY has no probe) proves the editor reads again, the counts
    /// are re-baselined by the decision that marker triggers, and the editor takes input back.
    /// The hand-off and this controller are shared by the Legacy and Table input modes.
    #[test]
    fn a_consumed_enter_is_released_by_the_line_editor_marker_not_a_quiet_period() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness_with(&mut app, powershell_session_info());
            submit(&mut app, &harness, "read x");
            user_types(&mut app, &harness, b"y\r");
            shell_runs_a_command(&harness.model, "read x");
            line_editor_becomes_active(&mut app, &harness);
            assert!(harness.model.lock().typeahead_handoff().is_pty_owned());

            // Well past the old 250 ms reset: a timer decides nothing.
            Timer::after(Duration::from_millis(600)).await;
            assert!(
                harness.model.lock().typeahead_handoff().is_pty_owned(),
                "the pty keeps input until an event"
            );
            assert!(!written(&harness).contains(&REQUEST.to_vec()));

            activation_pending(&mut app, &harness);
            powershell_marks_line_editor_reading(&mut app, &harness);
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned
            );
            assert!(!written(&harness).contains(&REQUEST.to_vec()));
        });
    }

    /// An in-band command waiting on an Enter that a program consumed is not released by a quiet
    /// period (no timer writes); the shell's ready event releases it, with no request.
    #[test]
    fn an_in_band_command_is_released_by_the_line_editor_marker_not_a_quiet_period() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness_with(&mut app, powershell_session_info());
            submit(&mut app, &harness, "read x");
            user_types(&mut app, &harness, b"y\r");
            shell_runs_a_command(&harness.model, "read x");
            line_editor_becomes_active(&mut app, &harness);

            let (cancel_tx, _cancel_rx) = async_channel::unbounded();
            harness.controller.update(&mut app, |controller, ctx| {
                controller.is_user_command_executing = false;
                controller.queue_in_band_command(
                    "echo in-band",
                    SHELL,
                    "command-id".to_owned(),
                    cancel_tx,
                    ctx,
                );
                assert_eq!(controller.pending_writes.len(), 1);
            });

            let in_band = bytes_to_execute_command("echo in-band", SHELL, false);
            Timer::after(Duration::from_millis(600)).await;
            assert_ne!(
                written(&harness).last(),
                Some(&in_band),
                "no timer writes the in-band command"
            );
            activation_pending(&mut app, &harness);
            powershell_marks_line_editor_reading(&mut app, &harness);
            assert_eq!(written(&harness).last(), Some(&in_band));
            assert!(!written(&harness).contains(&REQUEST.to_vec()));
        });
    }

    /// A held command whose start is rejected when it is finally written (a command started in
    /// the meantime) is dropped with a warning and leaves no executing-command state behind.
    #[test]
    fn a_held_command_rejected_at_write_time_is_dropped_cleanly() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 2");
            user_types(&mut app, &harness, b"foo");
            shell_runs_a_command(&harness.model, "sleep 2");
            fire_queued_command(&mut app, &harness);
            harness.model.lock().preexec(PreexecValue {
                command: "foo".to_owned(),
                session_id: None,
            });
            harness.line_editor_status.update(&mut app, |status, _| {
                status.set_line_editor_active_for_test(true)
            });

            harness.controller.update(&mut app, |controller, ctx| {
                controller.handoff_request_waiting = false;
                controller.execute_next_queued_write(ctx);
                assert!(controller.pending_writes.is_empty());
                assert!(!controller.handoff_held_command_pending);
                assert!(!controller.is_user_command_executing);
            });
            assert_eq!(written(&harness).len(), 2);
        });
    }

    /// #848's discard (tab close, an agent interrupt) drops every parked command, a held one
    /// included, and forgets it: the next command can be held again rather than coalesced
    /// behind a command that is gone.
    #[test]
    fn a_discard_drops_and_forgets_a_held_command() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 2");
            user_types(&mut app, &harness, b"foo");
            shell_runs_a_command(&harness.model, "sleep 2");
            fire_queued_command(&mut app, &harness);

            harness.controller.update(&mut app, |controller, _| {
                controller.discard_pending_command_writes();
                assert!(controller.pending_writes.is_empty());
                assert!(!controller.handoff_held_command_pending);
                assert!(!controller.is_user_command_executing);
            });

            fire_queued_command(&mut app, &harness);
            harness.controller.read(&app, |controller, _| {
                assert!(controller.handoff_held_command_pending);
                assert_eq!(controller.pending_writes.len(), 1);
            });
        });
    }

    /// A held agent command records its action, so a cancel of that action can find the
    /// command although no block holds it yet; writing it forgets the action.
    #[test]
    fn a_held_agent_command_records_its_action_until_written() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 2");
            user_types(&mut app, &harness, b"foo");
            shell_runs_a_command(&harness.model, "sleep 2");

            let action_id = crate::ai::agent::AIAgentActionId::from("agent-command".to_owned());
            let metadata = crate::terminal::model::block::AgentInteractionMetadata::new_hidden(
                action_id.clone(),
                crate::ai::agent::conversation::AIConversationId::new(),
            );
            let outcome = harness.controller.update(&mut app, |controller, ctx| {
                controller.write_command(
                    "agent",
                    SHELL,
                    CommandExecutionSource::AI { metadata },
                    ctx,
                )
            });
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert_eq!(
                harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .held_command_action(),
                Some(&action_id)
            );

            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(
                written(&harness).last(),
                Some(&bytes_to_execute_command("agent", SHELL, false))
            );
            assert_eq!(
                harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .held_command_action(),
                None
            );
        });
    }

    /// The targeted discard (a cancelled agent action, a stopped conversation) drops only the
    /// held command: a command from the editor queued after it stays queued, and the
    /// executing-command state, which is now that command's, is kept.
    #[test]
    fn a_targeted_discard_keeps_a_queued_editor_command() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 2");
            user_types(&mut app, &harness, b"foo");
            shell_runs_a_command(&harness.model, "sleep 2");
            fire_queued_command(&mut app, &harness);
            submit(&mut app, &harness, "ls");

            harness.controller.update(&mut app, |controller, _| {
                assert_eq!(controller.pending_writes.len(), 2);
                controller.discard_held_handoff_command();
                assert_eq!(controller.pending_writes.len(), 1);
                assert!(!is_held_handoff_command(&controller.pending_writes[0]));
                assert!(!controller.handoff_held_command_pending);
                assert!(
                    controller.is_user_command_executing,
                    "the editor command still owns the executing state"
                );
            });

            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(
                written(&harness).last(),
                Some(&bytes_to_execute_command("ls", SHELL, false))
            );
        });
    }

    /// A shell that cannot report its line keeps the pre-hand-off routing: nothing could hand
    /// keys left in its line back to the editor.
    #[test]
    fn a_shell_without_input_reporting_does_not_engage_the_handoff() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let mut bash_3_2 = SessionInfo::new_for_test();
            bash_3_2.shell = Shell::new(
                ShellType::Bash,
                Some("3.2".to_owned()),
                None,
                Default::default(),
                None,
            );
            harness
                .model
                .lock()
                .block_list_mut()
                .early_output_mut()
                .init_session(&bash_3_2);
            submit(&mut app, &harness, "sleep 1");
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned
            );
        });
    }

    /// Without the flag, routing is the pre-hand-off one: the request goes out on every
    /// activation, even while lines the user typed are still queued in the tty.
    #[test]
    fn without_the_flag_the_request_goes_out_on_every_activation() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(false);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 1");
            harness.controller.update(&mut app, |controller, ctx| {
                controller.write_bytes(&b"a\rb"[..], ctx)
            });
            shell_runs_a_command(&harness.model, "sleep 1");
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(
                written(&harness),
                vec![
                    bytes_to_execute_command("sleep 1", SHELL, false),
                    b"a\rb".to_vec(),
                    REQUEST.to_vec()
                ]
            );
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned
            );
        });
    }

    /// #1089: a legacy request (the editor owns input) is reported to the hand-off when it is
    /// written, so keys the hand-off did not adopt, written before it, count as covered by its
    /// report: the editor stays literal until that report is in, and no longer.
    #[test]
    fn a_legacy_request_covers_untracked_keys_written_before_it() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            harness
                .model
                .lock()
                .typeahead_handoff_mut()
                .note_untracked_line_bytes();
            assert!(
                !harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .untracked_bytes_requested()
            );

            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(
                written(&harness).last(),
                Some(&REQUEST.to_vec()),
                "the legacy request is written"
            );
            let mut model = harness.model.lock();
            let mut handoff = model.typeahead_handoff_mut();
            assert_eq!(handoff.state(), &HandoffState::EditorOwned);
            assert!(handoff.untracked_bytes_requested());
            assert!(handoff.editor_text_precedes_report());
            handoff.on_report();
            assert!(!handoff.editor_text_precedes_report());
        });
    }

    /// #1793: the request count fed through the real `TerminalModel` and controller in stream
    /// order. A request whose report the view never got is settled by the written command's own
    /// Preexec, told by its text; a report on its way to the view is not settled twice; a line
    /// queued ahead of the command (another text), a Preexec naming only the head of a continued
    /// (PS2) command, and a report dropped on the alt screen or outside `ShellReported` mode are
    /// each handled without settling a request that may still answer.
    #[test]
    fn requests_are_settled_only_by_the_written_commands_own_preexec_in_stream_order() {
        use crate::terminal::model::ansi::InputBufferValue;
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let outstanding =
                |h: &Harness| h.model.lock().typeahead_handoff().requests_outstanding();
            let preexec = |h: &Harness, command: &str| {
                h.model.lock().preexec(PreexecValue {
                    command: command.to_owned(),
                    session_id: None,
                });
            };

            // A request the shell never answers, then the command and its own Preexec.
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            assert_eq!(written(&h).last(), Some(&REQUEST.to_vec()));
            assert_eq!(outstanding(&h), 1);
            submit(&mut app, &h, "ls -l");
            preexec(&h, "ls -l");
            assert_eq!(outstanding(&h), 0, "never answered: settled");

            // Answered, its report delivered but not yet handled by the view: not settled.
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            h.model.lock().input_buffer(InputBufferValue::default());
            submit(&mut app, &h, "ls -l");
            preexec(&h, "ls -l");
            assert_eq!(outstanding(&h), 1, "its report is on its way to the view");
            let _ = h.model.lock().typeahead_handoff_mut().on_report();
            assert_eq!(outstanding(&h), 0);

            // A line queued ahead of the command: its Preexec carries another text.
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            submit(&mut app, &h, "ls -l");
            preexec(&h, "make");
            assert_eq!(outstanding(&h), 1, "a line ahead settles nothing");

            // Review finding 4: typeahead `make\r` still queued in the tty, then a request, then
            // the editor's `make`. The typeahead line's Preexec carries the same text but is not
            // the command's own: it must not settle the request written after it, and the command
            // written behind a queued line settles nothing at all.
            let h = harness(&mut app);
            submit(&mut app, &h, "sleep 1");
            user_types(&mut app, &h, b"make\r");
            shell_runs_a_command(&h.model, "sleep 1");
            assert_eq!(outstanding(&h), 0);
            // A request written behind the typeahead line (the controller writes none while a
            // line is queued; any path that does is what this guards against).
            h.model
                .lock()
                .typeahead_handoff_mut()
                .on_legacy_request_written();
            assert_eq!(outstanding(&h), 1);
            submit(&mut app, &h, "make");
            preexec(&h, "make");
            assert_eq!(
                outstanding(&h),
                1,
                "the typeahead line's Preexec settles nothing"
            );
            h.model.lock().finish_block();
            preexec(&h, "make");
            assert_eq!(
                outstanding(&h),
                1,
                "the command written behind a queued line settles nothing"
            );

            // PS2: a Preexec naming only the head of a continued command (bash's preexec also stops
            // at the first job-control operator) settles nothing. The whole multi-line text is the
            // command's own.
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            submit(&mut app, &h, "for i in 1; do\necho $i\ndone");
            preexec(&h, "for i in 1; do");
            assert_eq!(outstanding(&h), 1, "a PS2 head settles nothing");
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            submit(&mut app, &h, "for i in 1; do\necho $i\ndone");
            preexec(&h, "for i in 1; do\necho $i\ndone");
            assert_eq!(outstanding(&h), 0);

            // A report the view never gets (outside `ShellReported` mode) is not counted as an
            // answer: the request it answered is settled by the command's Preexec.
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            {
                let mut model = h.model.lock();
                let early_output = model.block_list_mut().early_output_mut();
                early_output.set_typeahead_mode_for_test(TypeaheadMode::InputMatching);
            }
            h.model.lock().input_buffer(InputBufferValue {
                buffer: "x".to_owned(),
                ..Default::default()
            });
            h.model
                .lock()
                .block_list_mut()
                .early_output_mut()
                .set_typeahead_mode_for_test(TypeaheadMode::ShellReported);
            submit(&mut app, &h, "ls -l");
            preexec(&h, "ls -l");
            assert_eq!(outstanding(&h), 0, "the dropped report answered nothing");

            // On the alt screen a report is not delivered to the view, so it is not counted
            // either: the request is settled by the command's Preexec.
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            let delivered = |h: &Harness| {
                h.model
                    .lock()
                    .block_list()
                    .early_output()
                    .reports_delivered()
            };
            let before = delivered(&h);
            h.model.lock().set_altscreen_active();
            h.model.lock().input_buffer(InputBufferValue::default());
            h.model.lock().set_altscreen_inactive_for_test();
            assert_eq!(delivered(&h), before, "not delivered on the alt screen");
            submit(&mut app, &h, "ls -l");
            preexec(&h, "ls -l");
            assert_eq!(outstanding(&h), 0, "the alt-screen report answered nothing");

            // An unsolicited report (a user's Alt-i, a pasted ESC i) is delivered and counted:
            // it can only make fewer requests look eaten, never settle one still on its way.
            let h = harness(&mut app);
            line_editor_becomes_active(&mut app, &h);
            h.model.lock().input_buffer(InputBufferValue::default());
            h.model.lock().input_buffer(InputBufferValue::default());
            submit(&mut app, &h, "ls -l");
            preexec(&h, "ls -l");
            assert_eq!(
                outstanding(&h),
                1,
                "nothing eaten; the view still handles its report"
            );
        });
    }

    /// #1104: keys written behind a legacy request stay in the shell's line when its report
    /// comes back; one more request is written behind them, and only once.
    #[test]
    fn a_report_that_missed_untracked_keys_is_followed_by_a_drain_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(written(&harness), vec![REQUEST.to_vec()]);
            {
                let mut model = harness.model.lock();
                let mut handoff = model.typeahead_handoff_mut();
                handoff.note_untracked_line_bytes();
                // The report of the first request: it missed the keys.
                handoff.on_report();
                assert!(handoff.drain_pending());
            }
            harness.controller.update(&mut app, |controller, ctx| {
                controller.retry_typeahead_handoff_request(ctx)
            });
            assert_eq!(written(&harness), vec![REQUEST.to_vec(), REQUEST.to_vec()]);
            harness.controller.update(&mut app, |controller, ctx| {
                controller.retry_typeahead_handoff_request(ctx)
            });
            assert_eq!(
                written(&harness).len(),
                2,
                "the second request covers the keys; no third"
            );
            let mut model = harness.model.lock();
            let mut handoff = model.typeahead_handoff_mut();
            assert!(handoff.editor_text_precedes_report());
            handoff.on_report();
            assert!(!handoff.editor_text_precedes_report());
        });
    }

    /// #1104: a command the hand-off does not own (an agent's, a queued prompt) must not kill
    /// untracked user keys in the shell's line before a request has collected them: the request
    /// is written first, the command right behind it.
    #[test]
    fn a_queued_command_goes_out_behind_a_request_that_collects_untracked_keys() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            harness
                .model
                .lock()
                .typeahead_handoff_mut()
                .note_untracked_line_bytes();
            fire_queued_command(&mut app, &harness);
            assert_eq!(
                written(&harness),
                vec![
                    REQUEST.to_vec(),
                    bytes_to_execute_command("queued", SHELL, false)
                ]
            );
        });
    }
    /// The shell's `LineEditorReading` marker for the active session.
    fn shell_marks_line_editor_reading(app: &mut App, harness: &Harness) {
        harness.model_events.update(app, |_, ctx| {
            ctx.emit(ModelEvent::LineEditorReading {
                session_id: SessionId::from(0),
                seq: None,
            })
        });
    }

    fn powershell_session_info() -> SessionInfo {
        let mut session_info = SessionInfo::new_for_test();
        session_info.shell =
            Shell::new(ShellType::PowerShell, None, None, Default::default(), None);
        session_info
    }

    /// The prompt's hook armed the activation fallback; the line editor is not active yet.
    fn activation_pending(app: &mut App, harness: &Harness) {
        harness.line_editor_status.update(app, |status, ctx| {
            status.set_line_editor_active_for_test(false);
            status.arm_activation_for_test(ctx);
        });
        harness.line_editor_status.read(&*app, |status, _| {
            assert!(!status.is_line_editor_active(), "the fallback is pending")
        });
    }

    /// #1401/#1402: PowerShell never enables bracketed paste, so its line editor's own
    /// `LineEditorReading` marker is its ready signal: an activation pending from the prompt's
    /// hook completes at once, as it does for readline's bracketed-paste enable (#1208), instead
    /// of at the end of the 50 ms fallback timer.
    #[test]
    fn the_line_editor_reading_marker_activates_a_pending_powershell_line_editor_at_once() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness_with(&mut app, powershell_session_info());
            activation_pending(&mut app, &harness);
            shell_marks_line_editor_reading(&mut app, &harness);
            harness.line_editor_status.read(&app, |status, _| {
                assert!(
                    status.is_line_editor_active(),
                    "the marker ends the activation wait"
                )
            });
        });
    }

    /// #1672/#1798: the activation's give-up net is a hand-back, not a release. When a
    /// signalling shell's signal does not come the gate stays shut, nothing is written to the pty,
    /// the controller tells the view to hand the held keys back, and a late signal then opens the
    /// gate normally. The controller and its gate are shared by the Legacy and Table input modes.
    #[test]
    fn the_signal_give_up_net_writes_nothing_and_leaves_the_gate_shut() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let gave_up = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            {
                let gave_up = gave_up.clone();
                let controller = harness.controller.clone();
                app.update(|ctx| {
                    ctx.subscribe_to_model(&controller, move |_, event, _| {
                        if matches!(event, PtyControllerEvent::LineEditorSignalGaveUp) {
                            gave_up.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    });
                });
            }
            let before = written(&harness).len();
            harness.line_editor_status.update(&mut app, |status, ctx| {
                status.set_line_editor_active_for_test(false);
                status.set_ready_signal_seen_for_test(true);
                status.set_signal_wait(Some(std::time::Duration::from_millis(5)));
                status.prompt_hook_for_test(ctx);
            });
            warpui::r#async::Timer::after(std::time::Duration::from_millis(150)).await;
            assert_eq!(
                gave_up.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "the view is told to hand the keys back"
            );
            harness.line_editor_status.read(&app, |status, _| {
                assert!(
                    !status.is_line_editor_active(),
                    "the net never opens the gate"
                )
            });
            assert_eq!(written(&harness).len(), before, "the net wrote nothing");
            // The next signal re-arms normally.
            harness
                .line_editor_status
                .update(&mut app, |status, ctx| status.note_shell_reading_line(ctx));
            harness.line_editor_status.read(&app, |status, _| {
                assert!(status.is_line_editor_active() && status.is_proven_reading())
            });
        });
    }

    /// Owner rule (a timer never commits): a Ctrl-C at a prompt on a session without prompt
    /// sequence numbers (here the default test harness) no longer commits the editor 250 ms later.
    /// Nothing changes with time; the line editor's ready signal (bracketed-paste enable) while it
    /// is already active -- zle restarting the line after ^C, no Precmd -- hands input back. The
    /// hand-off and controller are shared by the Legacy and Table input modes.
    #[test]
    fn a_ctrl_c_without_seq_numbers_is_settled_by_the_ready_signal_not_a_timer() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "true");
            // Text typed for the next prompt keeps the pty the owner (with nothing typed the
            // prompt's Precmd hands the editor the line at once: nothing to settle).
            user_types(&mut app, &harness, b"ab");
            shell_runs_a_command(&harness.model, "true");
            // The line editor is active (never went inactive): no `Active` event, no decision.
            harness.line_editor_status.update(&mut app, |status, _| {
                status.set_line_editor_active_for_test(true)
            });
            assert_eq!(
                harness
                    .model
                    .lock()
                    .typeahead_handoff_mut()
                    .on_interrupt_for_shell(false),
                InterruptOutcome::AtPrompt
            );
            assert!(
                harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .interrupt_settle_pending()
            );

            Timer::after(Duration::from_millis(600)).await;
            assert!(
                harness.model.lock().typeahead_handoff().is_pty_owned(),
                "no timer commits"
            );
            assert!(
                harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .interrupt_settle_pending()
            );

            harness.model_events.update(&mut app, |_, ctx| {
                ctx.emit(ModelEvent::Handler(AnsiHandlerEvent::SetBracketedPaste))
            });
            assert!(
                !harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .interrupt_settle_pending()
            );
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned,
                "the ready signal handed input back"
            );
        });
    }

    /// The other shells print the marker with their prompt, where the tty may still be cooked,
    /// and have the bracketed-paste signal: for them the marker leaves the activation alone.
    #[test]
    fn the_line_editor_reading_marker_leaves_a_zsh_activation_to_its_own_signals() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            activation_pending(&mut app, &harness);
            shell_marks_line_editor_reading(&mut app, &harness);
            harness
                .line_editor_status
                .read(&app, |status, _| assert!(!status.is_line_editor_active()));
        });
    }

    /// The marker cannot vouch for a line the shell has not answered yet (`b` typed behind `a`):
    /// the line editor would run that line first, so the activation keeps waiting, exactly as
    /// for the bracketed-paste signal.
    #[test]
    fn the_line_editor_reading_marker_waits_behind_an_unanswered_user_line() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness_with(&mut app, powershell_session_info());
            submit(&mut app, &harness, "sleep 1");
            user_types(&mut app, &harness, b"a\rb");
            shell_runs_a_command(&harness.model, "sleep 1");
            assert!(
                harness
                    .model
                    .lock()
                    .typeahead_handoff()
                    .has_unanswered_lines()
            );
            activation_pending(&mut app, &harness);
            shell_marks_line_editor_reading(&mut app, &harness);
            harness.line_editor_status.read(&app, |status, _| {
                assert!(
                    !status.is_line_editor_active(),
                    "an unanswered line keeps the activation pending"
                )
            });
        });
    }
}

/// `PHOSPHOR_LINE_EDITOR_READING_ACTIVATES=0` keeps the 50 ms activation timer (#1401).
#[test]
fn line_editor_reading_activation_kill_switch() {
    assert!(line_editor_reading_activates_from(None));
    assert!(line_editor_reading_activates_from(Some("1")));
    assert!(line_editor_reading_activates_from(Some("")));
    assert!(!line_editor_reading_activates_from(Some("0")));
    assert!(!line_editor_reading_activates_from(Some(" 0 ")));
    for value in ["off", "false", "no", " OFF ", "False"] {
        assert!(
            !line_editor_reading_activates_from(Some(value)),
            "{value:?}"
        );
    }
    assert!(line_editor_reading_activates_from(Some("on")));
}

/// nushell cannot eval a string, so its bootstrap is always a file: the app types a space-led
/// `source '<path>'` (the space keeps the line out of the history) and submits it with CR.
#[cfg(feature = "local_fs")]
#[test]
fn nu_rc_file_bootstrap_sources_the_file_with_a_space_led_line_and_cr() {
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

        controller.update(&mut app, |controller, ctx| {
            controller.source_bootstrap_script(
                b"/tmp/phosphor-bootstrap.nu".to_vec(),
                ShellType::Nu,
                ctx,
            );
        });

        let messages = sender.messages.lock();
        let mut written = Vec::new();
        for message in messages.iter() {
            match message {
                Message::Input(bytes) => written.extend_from_slice(bytes),
                other => panic!("unexpected message on the rc-file bootstrap path: {other:?}"),
            }
        }
        assert_eq!(written, b" source '/tmp/phosphor-bootstrap.nu'\r");

        drop(model_events_tx);
    });
}

/// #1046: a ^C written to the pty first tells the event loop to drop the input it has not
/// written yet (the unwritten lines of a big multi-line submission), then goes out itself.
#[test]
fn ctrl_c_discards_queued_input_before_the_interrupt_is_written() {
    App::test((), |mut app| async move {
        let model = terminal_model();
        let (_model_events_tx, model_events_rx) = async_channel::unbounded();
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

        controller.update(&mut app, |controller, ctx| {
            controller.write_bytes(&b"echo not an interrupt\r"[..], ctx);
            controller.write_bytes(&[escape_sequences::C0::ETX][..], ctx);
        });

        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert_input_matches(&messages[0], b"echo not an interrupt\r".to_vec());
        assert!(matches!(messages[1], Message::DiscardQueuedInput));
        assert_input_matches(&messages[2], vec![escape_sequences::C0::ETX]);
    });
}

/// #1046: only a user command's submitted buffer is tagged as cancellable by a ^C.
#[test]
fn user_command_is_sent_as_command_input_but_in_band_is_not() {
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

        controller.update(&mut app, |controller, ctx| {
            for in_band_command_id in [None, Some("in-band-1".to_owned())] {
                controller.send_write_to_event_loop(
                    PtyWrite::Command {
                        command: "echo hi".to_owned(),
                        shell_type: ShellType::Zsh,
                        in_band_command_id,
                        before_write_fn: None,
                    },
                    ctx,
                );
            }
        });

        let messages = sender.messages.lock();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::CommandInput(_))),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| matches!(m, Message::Input(_))),
            "{messages:?}"
        );
        assert_eq!(
            messages
                .iter()
                .filter(|m| matches!(m, Message::CommandInput(_)))
                .count(),
            1,
            "{messages:?}"
        );

        drop(model_events_tx);
    });
}

/// #1255: a paste-sized user write is tagged cancellable (a ^C drops what the event loop has not
/// yet written), while the bytes themselves are sent whole and unchanged: nothing is split,
/// capped or dropped on the way to the pty, whatever the size.
#[test]
fn paste_bytes_are_sent_whole_as_cancellable_input() {
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

        let paste: Vec<u8> = (0..PASTE_WRITE_MIN_BYTES * 3 + 17)
            .map(|i| b'a' + (i % 26) as u8)
            .collect();
        controller.update(&mut app, |controller, ctx| {
            controller.write_paste_bytes(paste.clone(), ctx);
            controller.write_bytes(&b"k"[..], ctx);
        });

        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 2, "{messages:?}");
        match &messages[0] {
            Message::CommandInput(bytes) => assert_eq!(&bytes[..], paste.as_slice()),
            other => panic!("a paste must be sent as cancellable input, got {other:?}"),
        }
        // A keystroke behind it stays ordinary input, so a cancel never takes typed keys.
        assert!(matches!(&messages[1], Message::Input(bytes) if &bytes[..] == b"k"));

        drop(model_events_tx);
    });
}

/// #1046: the input's multi-line confirmation asks `command_is_sent_as_bracketed_paste`, the
/// same decision `bytes_to_execute_command` wraps by, so the two cannot disagree.
#[test]
fn bracketed_paste_decision_is_shared_with_the_command_bytes() {
    for (enabled, command) in [(true, "a\nb"), (false, "a\nb"), (true, "")] {
        let bytes = bytes_to_execute_command(command, ShellType::Zsh, enabled);
        let wrapped = bytes
            .windows(escape_sequences::BRACKETED_PASTE_START.len())
            .any(|w| w == escape_sequences::BRACKETED_PASTE_START);
        assert_eq!(
            wrapped,
            command_is_sent_as_bracketed_paste(enabled, command),
            "enabled={enabled} command={command:?}"
        );
    }
}

// ---- #1384 P3: the ssh-warpify line-clear payload is one job ----

fn line_clear_prefix() -> Vec<u8> {
    vec![b' ', escape_sequences::C0::VT, escape_sequences::C0::NAK]
}

/// With the writer-owned queue the payload is ONE `SetupChunked` job of 1000-byte chunks whose
/// pacing is decided by the writer from the tty mode; nothing is timer-chained on the UI side.
#[test]
fn line_clear_payload_is_one_setup_job_paced_by_tty_mode() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        let payload: Vec<u8> = (0..4500).map(|i| (i % 251) as u8).collect();
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(true);
            controller.write_line_clear_setup(payload.clone(), ctx);
        });
        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 1, "one job, no timers: {messages:?}");
        let Message::SetupChunked { chunks, pacing, .. } = &messages[0] else {
            panic!("expected a setup job: {:?}", messages[0]);
        };
        assert!(matches!(
            pacing,
            Pacing::GapIfCanonical { dur, .. } if *dur == Duration::from_millis(10)
        ));
        assert!(chunks.iter().all(|c| c.len() <= 1000));
        let mut expected = line_clear_prefix();
        expected.extend_from_slice(&payload);
        assert_eq!(chunks.concat(), expected);
    });
}

/// A ^C aborts the line-clear job before it goes out, and a cancelled warpify does too.
#[test]
fn ctrl_c_and_cancel_abort_the_line_clear_job() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(true);
            controller.write_line_clear_setup(vec![b'x'; 3000], ctx);
            controller.write_bytes(&[escape_sequences::C0::ETX][..], ctx);
            controller.write_line_clear_setup(vec![b'y'; 3000], ctx);
            controller.cancel_paced_writes(ctx);
        });
        let messages = sender.messages.lock();
        assert!(
            matches!(messages[0], Message::SetupChunked { .. }),
            "{messages:?}"
        );
        assert!(
            matches!(messages[1], Message::Abort(AbortScope::Setup)),
            "{messages:?}"
        );
        let tail: Vec<_> = messages.iter().rev().take(2).collect();
        assert!(
            matches!(tail[0], Message::Abort(AbortScope::Setup)),
            "{messages:?}"
        );
        assert!(
            matches!(tail[1], Message::SetupChunked { .. }),
            "{messages:?}"
        );
    });
}

/// Kill switch: the timer chain of before, first chunk at once, the rest left to timers, and a
/// cancel drops them.
#[test]
fn line_clear_payload_without_the_queue_uses_the_timer_chain() {
    App::test((), |mut app| async move {
        let sender = TestEventLoopSender::default();
        let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(false);
            controller.write_line_clear_setup(vec![b'x'; 3500], ctx);
            assert_eq!(
                controller.paced_writes_remaining(),
                3,
                "4 chunks, 1 written"
            );
            controller.cancel_paced_writes(ctx);
            assert_eq!(controller.paced_writes_remaining(), 0);
        });
        let messages = sender.messages.lock();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert_input_matches(&messages[0], {
            let mut first = line_clear_prefix();
            first.extend_from_slice(&[b'x'; 997]);
            first
        });
    });
}

/// #1384 P4: on a pty with the write queue, agent input is its own class on the wire, a paste is
/// its own class, and a cancelled agent command aborts the agent's jobs only. Without the queue
/// (ConPTY, `PHOSPHOR_PTY_WRITE_QUEUE=0`) everything travels as before and nothing is aborted.
#[test]
fn agent_and_paste_writes_are_split_on_the_wire_only_with_the_write_queue() {
    for queue in [true, false] {
        App::test((), |mut app| async move {
            let sender = TestEventLoopSender::default();
            let (controller, _tx) = paced_write_controller(&mut app, terminal_model(), &sender);
            controller.update(&mut app, |controller, ctx| {
                controller.force_write_queue(queue);
                controller.write_paste_bytes(&b"paste"[..], ctx);
                controller.write_agent_bytes(&b"agent"[..], &AIAgentPtyWriteMode::Raw, ctx);
                controller.write_bytes(&b"k"[..], ctx);
                controller.cancel_agent_writes(ctx);
            });
            let messages = sender.messages.lock();
            if queue {
                assert_eq!(messages.len(), 4, "{messages:?}");
                assert!(matches!(&messages[0], Message::PasteInput(b) if &b[..] == b"paste"));
                assert!(matches!(&messages[1], Message::AgentInput(b) if &b[..] == b"agent"));
                assert!(matches!(&messages[2], Message::Input(b) if &b[..] == b"k"));
                assert!(matches!(&messages[3], Message::Abort(AbortScope::Agent)));
            } else {
                assert_eq!(messages.len(), 3, "{messages:?}");
                assert!(matches!(&messages[0], Message::CommandInput(b) if &b[..] == b"paste"));
                assert!(matches!(&messages[1], Message::Input(b) if &b[..] == b"agent"));
                assert!(matches!(&messages[2], Message::Input(b) if &b[..] == b"k"));
            }
        });
    }
}

/// Builds a controller for the #1801 tests below, with its line editor active (the exchange can
/// start), and its line-editor status so a test can switch it off.
fn native_completions_controller(
    app: &mut App,
) -> (
    ModelHandle<PtyController<TestEventLoopSender>>,
    ModelHandle<ModelEventDispatcher>,
    ModelHandle<LineEditorStatus>,
    TestEventLoopSender,
    impl Sized + use<>,
) {
    let model = terminal_model();
    let (model_events_tx, model_events_rx) = async_channel::unbounded();
    let (_executor_command_tx, executor_command_rx) = async_channel::unbounded();
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let model_events =
        app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
    let line_editor_status =
        app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
    line_editor_status.update(app, |status, _| {
        status.set_line_editor_active_for_test(true)
    });
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
    (
        controller,
        model_events,
        line_editor_status,
        sender,
        model_events_tx,
    )
}

/// The bytes the controller wrote, one entry per write, whatever their class on the wire.
fn written(sender: &TestEventLoopSender) -> Vec<Vec<u8>> {
    sender
        .messages
        .lock()
        .iter()
        .filter_map(|message| match message {
            Message::Input(bytes)
            | Message::CommandInput(bytes)
            | Message::PasteInput(bytes)
            | Message::AgentInput(bytes) => Some(bytes.to_vec()),
            _ => None,
        })
        .collect()
}

/// Starts a remote-first exchange through the real API.
fn start_exchange(
    controller: &mut PtyController<TestEventLoopSender>,
    text: &str,
    ctx: &mut ModelContext<PtyController<TestEventLoopSender>>,
) -> async_channel::Receiver<Vec<ShellCompletion>> {
    let (results_tx, results_rx) = async_channel::unbounded();
    controller.run_native_shell_completions(
        text.to_owned(),
        NativeCompletionsRequestOptions { remote_first: true },
        results_tx,
        ctx,
    );
    results_rx
}

fn position_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// #1801 design: the answer frame is `RS token : %HH... US` and the cancel frame `RS - : US`. A
/// frame never carries an EOT or a newline, so it can never end a line or a shell wherever it
/// lands, and between its delimiters there is only the frame alphabet.
#[test]
fn native_completions_frames_are_self_identifying_and_never_end_a_line() {
    assert_eq!(
        native_completions_answer("123", "cd é"),
        b"\x1e123:%63%64%20%C3%A9\x1f".to_vec()
    );
    assert_eq!(native_completions_cancel(), b"\x1e-:\x1f".to_vec());
    for frame in [
        native_completions_answer("9", "a\nb\u{4}c\r\x1e\x1f"),
        native_completions_cancel(),
    ] {
        assert_eq!(frame.first(), Some(&NATIVE_COMPLETIONS_FRAME_START));
        assert_eq!(frame.last(), Some(&NATIVE_COMPLETIONS_FRAME_END));
        let inner = &frame[1..frame.len() - 1];
        assert!(
            inner
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'%' | b':' | b'-')),
            "{frame:?}"
        );
    }
}

/// #1801 design point a: an exchange starts only when the write queue is empty, no command is
/// executing and the line editor is active; otherwise nothing is written (no `^Y`) and the
/// requester falls back to the completer at once.
#[test]
fn an_exchange_starts_only_when_nothing_is_queued_or_running() {
    App::test((), |mut app| async move {
        let (controller, _model_events, line_editor_status, sender, _tx) =
            native_completions_controller(&mut app);

        // The line editor is not active.
        line_editor_status.update(&mut app, |status, _| {
            status.set_line_editor_active_for_test(false)
        });
        let refused = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        assert!(refused.is_closed(), "the requester falls back at once");

        // A command is queued (written through the real API while the line editor is busy).
        controller.update(&mut app, |controller, ctx| {
            let outcome = controller.write_command(
                "echo q",
                ShellType::Bash,
                CommandExecutionSource::User,
                ctx,
            );
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert!(!controller.pending_writes.is_empty());
        });
        line_editor_status.update(&mut app, |status, _| {
            status.set_line_editor_active_for_test(true)
        });
        let refused = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        assert!(refused.is_closed(), "not behind a queued write");
        assert!(
            !written(&sender).iter().any(|bytes| bytes == &[0x19]),
            "no ^Y was ever written"
        );
        controller.read(&app, |controller, _| {
            assert!(controller.in_flight_native_completions_state.is_none());
        });
    });
}

/// #1801 design points b and c: nothing ever sits in the queue behind `^Y`. Written through the
/// real APIs during an exchange, an editor command and agent bytes each go out at once, in order,
/// after the cancel frame; the queue stays empty throughout.
#[test]
fn writes_during_an_exchange_go_out_in_order_after_the_cancel_frame() {
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let results_rx = controller.update(&mut app, |controller, ctx| {
            let results_rx = start_exchange(controller, "cd pro", ctx);
            assert!(controller.pending_writes.is_empty());
            let outcome = controller.write_command(
                "echo one",
                ShellType::Bash,
                CommandExecutionSource::User,
                ctx,
            );
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert!(
                controller.pending_writes.is_empty(),
                "nothing queued behind ^Y"
            );
            controller.write_agent_bytes(&b"agent two"[..], &AIAgentPtyWriteMode::Raw, ctx);
            assert!(
                controller.pending_writes.is_empty(),
                "nothing queued behind ^Y"
            );
            results_rx
        });
        let out = written(&sender);
        assert_eq!(out[0], vec![0x19], "{out:?}");
        assert_eq!(
            out[1],
            native_completions_cancel(),
            "the cancel frame comes first"
        );
        let rest: Vec<u8> = out[2..].concat();
        let one = position_of(&rest, b"echo one").expect("the editor command was written");
        let two = position_of(&rest, b"agent two").expect("the agent bytes were written");
        assert!(one < two, "in order: {rest:?}");
        assert_eq!(
            out.iter()
                .filter(|bytes| bytes.contains(&NATIVE_COMPLETIONS_FRAME_START))
                .count(),
            1,
            "exactly one frame"
        );
        assert!(
            results_rx.recv().await.is_err(),
            "the Tab falls back to the completer"
        );
        controller.read(&app, |controller, _| {
            assert!(controller.in_flight_native_completions_state.is_none());
        });
    });
}

/// #1801 design point b: a direct byte write (typed keys, a paste, a ^C) during an exchange: the
/// cancel frame, then the bytes.
#[test]
fn a_direct_byte_write_during_an_exchange_is_preceded_by_the_cancel_frame() {
    for kind in ["bytes", "paste", "etx"] {
        App::test((), move |mut app| async move {
            let (controller, _model_events, _status, sender, _tx) =
                native_completions_controller(&mut app);
            controller.update(&mut app, |controller, ctx| {
                let _results = start_exchange(controller, "cd pro", ctx);
                match kind {
                    "bytes" => controller.write_bytes(&b"k"[..], ctx),
                    "paste" => controller.write_paste_bytes(&b"pasted"[..], ctx),
                    _ => controller.write_bytes(&[0x03][..], ctx),
                }
                assert!(controller.pending_writes.is_empty(), "{kind}");
            });
            let out = written(&sender);
            assert_eq!(out[0], vec![0x19], "{kind}: {out:?}");
            assert_eq!(out[1], native_completions_cancel(), "{kind}: {out:?}");
            assert!(out.len() >= 3, "{kind}: the write followed: {out:?}");
        });
    }
}

/// #1801 design point c: the prompt arriving writes the answer frame; writes after it go out at
/// once (the hook consumed exactly its frame); the results end the exchange.
#[test]
fn the_prompt_is_answered_with_one_frame_and_later_writes_follow_it() {
    App::test((), |mut app| async move {
        let (controller, model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let results_rx = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt {
                token: Some("77".to_owned()),
            });
        });
        controller.update(&mut app, |controller, ctx| {
            controller.write_bytes(&b"k"[..], ctx);
            assert!(controller.pending_writes.is_empty());
        });
        assert_eq!(
            written(&sender),
            vec![
                vec![0x19],
                native_completions_answer("77", "cd pro"),
                b"k".to_vec()
            ],
            "the answer frame, then the later write; no cancel"
        );
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::CompletionsFinished(vec![]));
        });
        assert!(results_rx.try_recv().is_ok());
        controller.read(&app, |controller, _| {
            assert!(controller.in_flight_native_completions_state.is_none());
        });
    });
}

/// #1801 review M3: a prompt whose token was already used in this pane (a replay) is cancelled,
/// as is one with no token.
#[test]
fn a_reused_or_missing_prompt_token_is_cancelled() {
    for second_token in [Some("77"), None] {
        App::test((), move |mut app| async move {
            let (controller, model_events, _status, sender, _tx) =
                native_completions_controller(&mut app);
            controller.update(&mut app, |controller, ctx| {
                let _first = start_exchange(controller, "a", ctx);
            });
            model_events.update(&mut app, |_dispatcher, ctx| {
                ctx.emit(ModelEvent::SendCompletionsPrompt {
                    token: Some("77".to_owned()),
                });
            });
            model_events.update(&mut app, |_dispatcher, ctx| {
                ctx.emit(ModelEvent::CompletionsFinished(vec![]));
            });
            let second = controller.update(&mut app, |controller, ctx| {
                start_exchange(controller, "b", ctx)
            });
            model_events.update(&mut app, |_dispatcher, ctx| {
                ctx.emit(ModelEvent::SendCompletionsPrompt {
                    token: second_token.map(str::to_owned),
                });
            });
            let out = written(&sender);
            assert_eq!(
                out.last(),
                Some(&native_completions_cancel()),
                "{second_token:?}: {out:?}"
            );
            // The cancel is an answer of nothing (owner decision: an Enter held for the Tab then
            // runs the typed line); only a give-up is "no answer" (a closed channel).
            assert_eq!(second.recv().await.ok(), Some(vec![]), "{second_token:?}");
        });
    }
}

/// #1801: a prompt that does not come in time ends the exchange with the cancel frame; for a
/// remote-first request there is no "no shell integration" toast. With the real timeout.
#[test]
fn prompt_timeout_cancels_without_a_toast_for_a_remote_first_request() {
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let results_rx = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        Timer::after(NATIVE_COMPLETIONS_PROMPT_TIMEOUT + Duration::from_millis(300)).await;
        assert_eq!(
            written(&sender),
            vec![vec![0x19]],
            "the give-up timer writes nothing"
        );
        assert!(results_rx.recv().await.is_err(), "the requester falls back");
        controller.read(&app, |controller, _| {
            assert!(controller.in_flight_native_completions_state.is_none());
            assert!(controller.native_cancel_owed);
            assert!(!controller.last_native_cancel_toasted);
        });
    });
}

/// The cancel owed after a prompt timeout goes out exactly once, in front of the next real
/// write; a second write has no cancel in front of it.
#[test]
fn the_cancel_owed_after_a_timeout_precedes_the_next_write_exactly_once() {
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let _rx = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        Timer::after(NATIVE_COMPLETIONS_PROMPT_TIMEOUT + Duration::from_millis(300)).await;
        controller.update(&mut app, |controller, ctx| {
            controller.write_bytes(&b"k"[..], ctx);
            controller.write_bytes(&b"j"[..], ctx);
        });
        let out = written(&sender);
        assert_eq!(
            out,
            vec![
                vec![0x19],
                native_completions_cancel(),
                b"k".to_vec(),
                b"j".to_vec()
            ]
        );
        controller.read(&app, |controller, _| {
            assert!(!controller.native_cancel_owed);
        });
    });
}

/// A late prompt after the timeout (no write in between) gets the cancel frame then, once; a
/// following write adds no second cancel.
#[test]
fn a_late_prompt_after_the_timeout_gets_the_cancel_once() {
    App::test((), |mut app| async move {
        let (controller, model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let _rx = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        Timer::after(NATIVE_COMPLETIONS_PROMPT_TIMEOUT + Duration::from_millis(300)).await;
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt {
                token: Some("91".to_owned()),
            });
        });
        assert_eq!(
            written(&sender),
            vec![vec![0x19], native_completions_cancel()]
        );
        controller.update(&mut app, |controller, ctx| {
            controller.write_bytes(&b"k"[..], ctx);
        });
        assert_eq!(
            written(&sender),
            vec![vec![0x19], native_completions_cancel(), b"k".to_vec()],
            "no double cancel"
        );
    });
}

/// #1674 (busy-machine H14): the prompt timeout must never cost a key. Timeline of a slow shell:
/// the `^Y` goes out, the 2 s timeout passes (the cancel frame is written), the user's keys and a
/// command follow, and only THEN does the shell's hook answer with its prompt. The tty must read
/// `^Y`, cancel, keys, command in that order, the late prompt must write nothing (an answer
/// frame landing among the keys), and no write may have been held or reordered. Event-driven
/// reading of the tests above: the timer only ends the Tab's own answer; it holds nothing.
/// The pty layer is shared by Legacy and Table input, so this covers both.
#[test]
fn keys_after_the_prompt_timeout_and_a_late_prompt_lose_nothing() {
    App::test((), |mut app| async move {
        let (controller, model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let results_rx = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        Timer::after(NATIVE_COMPLETIONS_PROMPT_TIMEOUT + Duration::from_millis(300)).await;
        controller.update(&mut app, |controller, ctx| {
            controller.write_bytes(&b"k"[..], ctx);
            let outcome = controller.write_command(
                "echo one",
                ShellType::Zsh,
                CommandExecutionSource::User,
                ctx,
            );
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert!(controller.pending_writes.is_empty(), "nothing is held");
        });
        let before_late_prompt = written(&sender);
        model_events.update(&mut app, |_dispatcher, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt {
                token: Some("88".to_owned()),
            });
        });
        let out = written(&sender);
        assert_eq!(
            out, before_late_prompt,
            "the late prompt writes nothing among the user's keys"
        );
        assert_eq!(out[0], vec![0x19], "{out:?}");
        assert_eq!(out[1], native_completions_cancel(), "{out:?}");
        assert_eq!(out[2], b"k".to_vec(), "the key follows the cancel: {out:?}");
        assert!(
            position_of(&out[3..].concat(), b"echo one").is_some(),
            "the command follows the key: {out:?}"
        );
        assert_eq!(
            out.iter()
                .filter(|bytes| bytes.contains(&NATIVE_COMPLETIONS_FRAME_START))
                .count(),
            1,
            "exactly one frame (the cancel)"
        );
        assert!(results_rx.recv().await.is_err(), "the Tab falls back");
        controller.read(&app, |controller, _| {
            assert!(controller.in_flight_native_completions_state.is_none());
        });
    });
}

/// The same timeout for a request that is not remote-first (upstream's flag path) asks for the
/// toast: there a prompt that never comes means no shell integration.
#[test]
fn prompt_timeout_of_a_fallback_request_asks_for_the_toast() {
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, _sender, _tx) =
            native_completions_controller(&mut app);
        controller.update(&mut app, |controller, ctx| {
            let (results_tx, _results_rx) = async_channel::unbounded();
            controller.run_native_shell_completions(
                "x".to_owned(),
                NativeCompletionsRequestOptions {
                    remote_first: false,
                },
                results_tx,
                ctx,
            );
        });
        Timer::after(NATIVE_COMPLETIONS_PROMPT_TIMEOUT + Duration::from_millis(300)).await;
        controller.read(&app, |controller, _| {
            assert!(controller.last_native_cancel_toasted);
        });
    });
}

/// A fast Tab, Tab, Enter (20 ms gaps): the second Tab, for the same text, attaches to the
/// exchange in flight and gets ITS answer, so an Enter held for it runs the completed line. Only
/// one `^Y` goes out. The pty layer is shared by Legacy and Table input.
#[test]
fn a_second_tab_for_the_same_text_gets_the_first_exchanges_answer() {
    App::test((), |mut app| async move {
        let (controller, model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let first = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
        Timer::after(Duration::from_millis(20)).await;
        let second = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
        Timer::after(Duration::from_millis(20)).await;
        assert!(!second.is_closed(), "attached, not refused");
        model_events.update(&mut app, |_d, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt {
                token: Some("5".to_owned()),
            });
        });
        model_events.update(&mut app, |_d, ctx| {
            ctx.emit(ModelEvent::CompletionsFinished(vec![]));
        });
        assert_eq!(first.recv().await.ok(), Some(vec![]));
        assert_eq!(second.recv().await.ok(), Some(vec![]));
        assert_eq!(written(&sender)[0], vec![0x19], "one ^Y only");
        assert_eq!(
            written(&sender)
                .iter()
                .filter(|b| b.contains(&0x19))
                .count(),
            1
        );
    });
}

/// Tab, Tab where the first exchange is cancelled: the attached Tab gets the empty answer (an
/// Enter held for it runs the typed line). A different text is a real refusal; a give-up closes
/// both channels (no answer: the held Enter's line is handed back).
#[test]
fn an_attached_tab_follows_the_cancel_and_the_give_up_of_the_exchange() {
    App::test((), |mut app| async move {
        let (controller, model_events, _status, _sender, _tx) =
            native_completions_controller(&mut app);
        let _first = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
        let other = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pr", ctx));
        assert!(other.is_closed(), "a different text is refused");
        let second = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
        model_events.update(&mut app, |_d, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt { token: None });
        });
        assert_eq!(
            second.recv().await.ok(),
            Some(vec![]),
            "cancel: empty answer"
        );
    });
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, _sender, _tx) =
            native_completions_controller(&mut app);
        let _first = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
        let second = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
        Timer::after(NATIVE_COMPLETIONS_PROMPT_TIMEOUT + Duration::from_millis(300)).await;
        assert!(second.recv().await.is_err(), "give-up: no answer");
    });
}

/// The shell back at its prompt (Precmd, ExitShell) while the exchange waits for its RESULTS ends
/// it by that event, with no timer: the requester and an attached Tab see "no answer" and nothing
/// is written. The pty layer is shared by Legacy and Table input.
#[test]
fn the_shell_leaving_during_the_results_wait_ends_the_exchange_by_event() {
    for leaving in [
        ModelEvent::Handler(AnsiHandlerEvent::Precmd),
        ModelEvent::ExitShell {
            session_id: SessionId::from(9),
        },
    ] {
        App::test((), move |mut app| async move {
            let (controller, model_events, _status, sender, _tx) =
                native_completions_controller(&mut app);
            let first = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
            let second = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
            model_events.update(&mut app, |_d, ctx| {
                ctx.emit(ModelEvent::SendCompletionsPrompt {
                    token: Some("61".to_owned()),
                });
            });
            let before = written(&sender);
            model_events.update(&mut app, |_d, ctx| ctx.emit(leaving));
            assert!(first.recv().await.is_err(), "no answer");
            assert!(
                second.recv().await.is_err(),
                "no answer for the attached Tab"
            );
            assert_eq!(written(&sender), before, "nothing is written");
            controller.read(&app, |c, _| {
                assert!(c.in_flight_native_completions_state.is_none());
            });
        });
    }
}

/// The results net only drops the exchange: it writes nothing and releases no queued write.
#[test]
fn the_results_net_drops_the_exchange_and_writes_nothing() {
    App::test((), |mut app| async move {
        let (controller, model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let rx = controller.update(&mut app, |c, ctx| start_exchange(c, "cd pro", ctx));
        model_events.update(&mut app, |_d, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt {
                token: Some("62".to_owned()),
            });
        });
        let before = written(&sender);
        controller.update(&mut app, |c, ctx| {
            c.arm_native_completions_watchdog(Duration::from_millis(50), false, ctx);
        });
        Timer::after(Duration::from_millis(300)).await;
        assert!(rx.recv().await.is_err(), "no answer");
        assert_eq!(written(&sender), before, "the net writes nothing");
    });
}

/// #1801: a second request while one is in flight (a second Tab) is refused at once, and the
/// first exchange is untouched.
#[test]
fn a_second_request_while_one_is_in_flight_is_refused() {
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let (first, second) = controller.update(&mut app, |controller, ctx| {
            (
                start_exchange(controller, "first", ctx),
                start_exchange(controller, "second", ctx),
            )
        });
        assert!(second.is_closed());
        assert!(!first.is_closed());
        assert_eq!(written(&sender), vec![vec![0x19]], "one ^Y only");
    });
}

/// #1801 design point d: the shell that was asked leaves first (a ^C in its hook, an ssh that
/// died, the shell exiting): the exchange just ends, with no frame; nothing was held, so a
/// command written afterwards goes out at once.
#[test]
fn the_shell_leaving_ends_the_exchange_without_a_frame() {
    for leaving in [
        ModelEvent::Handler(AnsiHandlerEvent::Precmd),
        ModelEvent::ExitShell {
            session_id: SessionId::from(9),
        },
    ] {
        App::test((), move |mut app| async move {
            let (controller, model_events, _status, sender, _tx) =
                native_completions_controller(&mut app);
            let results_rx = controller.update(&mut app, |controller, ctx| {
                start_exchange(controller, "cd pro", ctx)
            });
            model_events.update(&mut app, |_dispatcher, ctx| ctx.emit(leaving));
            controller.read(&app, |controller, _| {
                assert!(controller.in_flight_native_completions_state.is_none());
            });
            assert!(results_rx.recv().await.is_err(), "the requester falls back");
            controller.update(&mut app, |controller, ctx| {
                controller.write_bytes(&b"k"[..], ctx)
            });
            assert_eq!(
                written(&sender),
                vec![vec![0x19], b"k".to_vec()],
                "no frame to a shell that was not asked"
            );
        });
    }
}

/// #1801 review M1: on the write-queue path a setup script is ONE `SetupChunked` job the writer
/// holds (`paced_write_queue` stays empty), so the start gate reads the job's liveness count: no
/// exchange starts while the writer still holds the job, and one may once it has dropped it.
#[test]
fn an_exchange_does_not_start_while_the_writer_holds_a_setup_job() {
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        controller.update(&mut app, |controller, ctx| {
            controller.force_write_queue(true);
            controller.write_line_clear_setup(b"setup".to_vec(), ctx);
            assert!(controller.paced_write_queue.is_empty());
        });
        let refused = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        assert!(refused.is_closed(), "not while the setup job is alive");
        assert!(
            !written(&sender).iter().any(|bytes| bytes == &[0x19]),
            "no ^Y behind the setup job"
        );

        // The writer drops the job (here: the test sender's copy of the message).
        sender.messages.lock().clear();
        let started = controller.update(&mut app, |controller, ctx| {
            start_exchange(controller, "cd pro", ctx)
        });
        assert!(
            !started.is_closed(),
            "the exchange starts once the job is gone"
        );
        assert_eq!(written(&sender), vec![vec![0x19]]);
    });
}

/// #1801 review M1: a setup job is a write like any other: one started while an exchange waits
/// for its prompt first sends the cancel frame.
#[test]
fn a_setup_write_during_an_exchange_is_preceded_by_the_cancel_frame() {
    App::test((), |mut app| async move {
        let (controller, _model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        controller.update(&mut app, |controller, ctx| {
            let _results = start_exchange(controller, "cd pro", ctx);
            controller.force_write_queue(true);
            controller.write_line_clear_setup(b"setup".to_vec(), ctx);
        });
        let messages = sender.messages.lock();
        assert!(
            matches!(&messages[0], Message::Input(b) if &b[..] == [0x19]),
            "{messages:?}"
        );
        assert!(
            matches!(&messages[1], Message::Input(b) if b.to_vec() == native_completions_cancel()),
            "the cancel frame before the setup job: {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::SetupChunked { .. })),
            "{messages:?}"
        );
    });
}
