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

        // The handler put the request at the FRONT and, the gate being open, wrote it and then the
        // in-band command that was already waiting, in that order (checked on the messages below);
        // nothing is left queued. (Batch 4 asserted one write left pending: the queue drains in
        // one pass, so that count was wrong -- the contract is the ORDER of the two messages.)
        controller.update(&mut app, |controller, ctx| {
            assert!(
                controller.pending_writes.is_empty(),
                "both writes went out in one pass, the request first."
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
                park: CommandPark::None,
                before_write_fn: None,
            });
            controller.pending_writes.push_back(PtyWrite::Command {
                command: "in-band".to_owned(),
                shell_type: ShellType::Zsh,
                in_band_command_id: Some("in-band-1".to_owned()),
                park: CommandPark::None,
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
            // The keys typed meanwhile (before the unproven activation) go out as typed.
            user_types(&mut app, &harness, b"echo hi");
            shell_runs_a_command(&harness.model, "sleep 30");
            line_editor_becomes_active_unproven(&mut app, &harness);
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

    /// #2187: a key typed right after a Ctrl-C, before the app saw the prompt, is in the shell's
    /// line (r27b: the `e` of `echo ...`; nu: the opening quote). The shell's prompt arrives and the
    /// fallback opens the write gate before the line editor's own signal. Handing input to the
    /// editor then stranded the key: the editor's command write killed the line and the shell ran
    /// `cho ...`. The decision waits for the proof instead (keys keep going to the pty, in typed
    /// order, no request to an unproven editor), and the signal asks for the line as before.
    #[test]
    fn a_key_typed_ahead_of_an_unproven_activation_is_not_stranded() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 30");
            user_types(&mut app, &harness, &[escape_sequences::C0::ETX]);
            user_types(&mut app, &harness, b"e");
            shell_runs_a_command(&harness.model, "sleep 30");
            line_editor_becomes_active_unproven(&mut app, &harness);
            assert!(
                matches!(
                    harness.model.lock().typeahead_handoff().state(),
                    HandoffState::PtyOwned { .. }
                ),
                "input must not move to the editor while a typed key sits in the shell's line"
            );
            assert!(!written(&harness).iter().any(|w| w == &REQUEST.to_vec()));
            // The rest of the line is typed meanwhile: it follows the key to the pty.
            user_types(&mut app, &harness, b"cho");
            // The shell's own signal proves its line editor reads: the line is asked for.
            harness.controller.update(&mut app, |controller, ctx| {
                controller.activate_line_editor_on_ready_signal(false, ctx)
            });
            assert_eq!(written(&harness).last(), Some(&REQUEST.to_vec()));
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::AwaitingReport
            );
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

    /// The prompt's hook of a shell that signals opened the write gate (#1825); its request
    /// decision still waits for the shell's ready signal.
    fn activation_pending(app: &mut App, harness: &Harness) {
        harness.line_editor_status.update(app, |status, ctx| {
            status.set_line_editor_active_for_test(false);
            status.arm_activation_for_test(ctx);
        });
        harness.line_editor_status.read(&*app, |status, _| {
            assert!(status.is_line_editor_active(), "the prompt opened the gate");
            assert!(
                status.decision_awaits_signal(),
                "the decision waits for the signal"
            );
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
                    !status.decision_awaits_signal() && status.is_proven_reading(),
                    "the marker proves the reader and starts the decision"
                )
            });
        });
    }

    /// #1825 (`unset zle_bracketed_paste`): a session that signalled before and then stops
    /// signalling must not wedge the pane. The command the editor submits while the line editor is
    /// inactive is parked until the gate opens; the PROMPT EVENT opens it -- no signal, no timer --
    /// so the command is written at that prompt and runs. The controller and status are shared by
    /// the Legacy and Table input modes.
    #[test]
    fn a_command_submitted_to_a_session_that_stopped_signalling_is_written_at_its_prompt() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            harness.line_editor_status.update(&mut app, |status, ctx| {
                status.set_ready_signal_seen_for_test(true, ctx);
                status.set_line_editor_active_for_test(false);
            });
            submit(&mut app, &harness, "echo after-unset");
            let command = bytes_to_execute_command("echo after-unset", SHELL, false);
            assert!(
                !written(&harness).iter().any(|w| w == &command),
                "parked behind the closed gate"
            );
            // The next prompt event, with no ready signal at all.
            harness
                .line_editor_status
                .update(&mut app, |status, ctx| status.prompt_hook_for_test(ctx));
            assert!(
                written(&harness).iter().any(|w| w == &command),
                "the prompt event opened the gate and the command went out"
            );
            // No request went to a reader nothing proved (#1798).
            assert!(!written(&harness).iter().any(|w| w == &REQUEST.to_vec()));
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

    /// #1823: the local shell signals its line editor; the editor then submits a command that
    /// starts a nested session (`ssh fake`) in the same pane, and the NEW session's first prompt
    /// event must open the gate so the command parked "until the line editor is active" is written
    /// -- the remote has not signalled, so it is not waited for. Returning to the local shell
    /// keeps its learned signalling. The controller and status are shared by Legacy and Table.
    #[test]
    fn a_nested_session_that_never_signalled_gets_the_parked_command_at_its_prompt() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            // The local shell (session 0) signalled earlier.
            harness.line_editor_status.update(&mut app, |status, ctx| {
                status.set_ready_signal_seen_for_test(true, ctx);
                status.set_line_editor_active_for_test(false);
            });
            // The editor submits the nested ssh: parked until a line editor is active.
            submit(&mut app, &harness, "ssh fake");
            assert!(
                !written(&harness)
                    .iter()
                    .any(|w| w == &bytes_to_execute_command("ssh fake", SHELL, false)),
                "parked behind the closed gate"
            );
            // ... a different active session (the nested shell) reaches its first prompt.
            harness.model_events.update(&mut app, |dispatcher, _| {
                dispatcher.set_active_session_id(SessionId::from(1))
            });
            harness
                .line_editor_status
                .update(&mut app, |status, ctx| status.prompt_hook_for_test(ctx));
            assert!(
                written(&harness)
                    .iter()
                    .any(|w| w == &bytes_to_execute_command("ssh fake", SHELL, false)),
                "the nested session's prompt event opened the gate and the command went out"
            );
            // The local session keeps what it learned.
            harness.model_events.update(&mut app, |dispatcher, _| {
                dispatcher.set_active_session_id(SessionId::from(0))
            });
            harness.line_editor_status.update(&mut app, |status, ctx| {
                assert!(status.active_session_signals_for_test(ctx));
            });
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
                .read(&app, |status, _| assert!(status.decision_awaits_signal()));
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
                    status.decision_awaits_signal(),
                    "an unanswered line keeps the decision pending"
                )
            });
        });
    }

    /// Queues the app's own in-band command, as an executor request would.
    fn queue_in_band(
        app: &mut App,
        harness: &Harness,
        command: &str,
        id: &str,
    ) -> async_channel::Receiver<InBandCommandCancelledEvent> {
        let (cancel_tx, cancel_rx) = async_channel::unbounded();
        harness.controller.update(app, |controller, ctx| {
            controller.is_user_command_executing = false;
            controller.queue_in_band_command(command, SHELL, id.to_owned(), cancel_tx, ctx);
        });
        cancel_rx
    }

    /// #2182 (e): nothing held, nothing in the line: the in-band command goes out at once.
    #[test]
    fn an_inband_command_with_no_keys_held_is_written_at_once() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-e");
            assert_eq!(
                written(&harness),
                vec![bytes_to_execute_command("echo in-band", SHELL, false)]
            );
            assert!(cancel_rx.try_recv().is_err());
        });
    }

    /// #2182 (a): the original loss. Raw user keys are in the shell's line after a command's
    /// precmd; the app's in-band command is queued. It is NOT written while the keys are in the
    /// line (its kill prefix would wipe them); the request that covers them goes out first, and
    /// the command is written once, behind it.
    #[test]
    fn an_inband_command_waits_for_raw_keys_in_the_line_and_goes_out_behind_the_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            submit(&mut app, &harness, "sleep 1");
            user_types(&mut app, &harness, b"ex");
            shell_runs_a_command(&harness.model, "sleep 1");
            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-a");
            assert_eq!(
                written(&harness),
                vec![
                    bytes_to_execute_command("sleep 1", SHELL, false),
                    b"ex".to_vec()
                ],
                "the in-band command must not be written while the user's keys are in the line"
            );
            harness.controller.read(&app, |controller, _| {
                assert_eq!(controller.pending_writes.len(), 1);
                assert!(controller.handoff_request_waiting);
            });

            // The line editor's activation decides the request that covers the keys.
            line_editor_becomes_active(&mut app, &harness);
            let written = written(&harness);
            let in_band = bytes_to_execute_command("echo in-band", SHELL, false);
            assert_eq!(
                written.iter().filter(|bytes| **bytes == in_band).count(),
                1,
                "the command is written exactly once: {written:?}"
            );
            let keys_at = written.iter().position(|bytes| bytes == b"ex").unwrap();
            let request_at = written.iter().position(|bytes| *bytes == REQUEST.to_vec());
            let command_at = written.iter().position(|bytes| *bytes == in_band).unwrap();
            assert!(
                request_at.is_some_and(|at| keys_at < at && at < command_at),
                "the request covering the keys precedes the command: {written:?}"
            );
            assert!(cancel_rx.try_recv().is_err());
        });
    }

    /// #2182 (b, c): keys the view holds keep the in-band command unwritten and the hand-off
    /// untouched (no flip, no counted Enter); when the view releases them the command is
    /// written once.
    #[test]
    fn an_inband_command_waits_for_the_views_held_keys_and_goes_out_on_release() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let cell = harness.model.lock().typeahead_handoff().view_keys_cell();
            cell.publish(true);
            let owner_before = harness.model.lock().typeahead_handoff().editor_owns_input();
            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-b");
            assert!(
                written(&harness).is_empty(),
                "held keys: nothing is written"
            );
            assert_eq!(
                harness.model.lock().typeahead_handoff().editor_owns_input(),
                owner_before,
                "the in-band command never flips the owner under held keys"
            );

            // The view's queue released its keys: the wake writes the command, once.
            cell.publish(false);
            Timer::after(Duration::from_millis(50)).await;
            assert_eq!(
                written(&harness),
                vec![bytes_to_execute_command("echo in-band", SHELL, false)]
            );
            assert!(cancel_rx.try_recv().is_err());
        });
    }

    /// #2182 (kill switch): with the wait off, the in-band command goes out as before.
    #[test]
    fn the_kill_switch_writes_the_inband_command_despite_held_keys() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            harness
                .model
                .lock()
                .typeahead_handoff_mut()
                .set_inband_wait_for_keys_enabled(false);
            harness
                .model
                .lock()
                .typeahead_handoff()
                .view_keys_cell()
                .publish(true);
            queue_in_band(&mut app, &harness, "echo in-band", "id-k");
            assert_eq!(
                written(&harness),
                vec![bytes_to_execute_command("echo in-band", SHELL, false)]
            );
        });
    }

    /// #2182 (d): held keys that never drain: the wait's give-up cancels the in-band command
    /// (its requester is told, nothing is written) and leaves the keys alone.
    #[test]
    fn the_give_up_of_the_wait_cancels_the_inband_command_and_leaves_the_keys() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let cell = harness.model.lock().typeahead_handoff().view_keys_cell();
            cell.publish(true);
            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-d");
            let generation = harness.controller.read(&app, |controller, _| {
                assert!(controller.in_band_wait_armed);
                controller.in_band_wait_generation
            });
            harness.controller.update(&mut app, |controller, ctx| {
                controller.give_up_in_band_wait(generation, ctx)
            });
            let event = cancel_rx.try_recv().expect("the requester is told");
            assert_eq!(event.command_id, "id-d");
            assert!(written(&harness).is_empty());
            assert!(cell.is_held(), "the keys are untouched");
            harness.controller.read(&app, |controller, _| {
                assert!(controller.pending_writes.is_empty());
            });
        });
    }

    // ---- #2182 (b): a submitted command is never dropped behind the app's own side call --------
    //
    // The harness is a LOCAL zsh session: `write_command` has no remote branch, so the refusal
    // the Windows lab found on a remote shell (where the side call's round trip makes the window
    // wide) is the same code path at a local prompt, and these tests are the local-shell variant.

    /// Counts the controller's hand-off events, as the terminal view would receive them.
    #[derive(Default)]
    struct HandoffEvents {
        changed: usize,
        not_engaged: usize,
    }

    impl Entity for HandoffEvents {
        type Event = ();
    }

    fn record_handoff_events(app: &mut App, harness: &Harness) -> ModelHandle<HandoffEvents> {
        let controller = harness.controller.clone();
        app.add_model(|ctx: &mut ModelContext<HandoffEvents>| {
            ctx.subscribe_to_model(&controller, |me, event, _| match event {
                PtyControllerEvent::TypeaheadHandoffChanged => me.changed += 1,
                PtyControllerEvent::TypeaheadHandoffNotEngaged => me.not_engaged += 1,
                _ => {}
            });
            HandoffEvents::default()
        })
    }

    fn command_bytes(command: &str) -> Vec<u8> {
        bytes_to_execute_command(command, SHELL, false)
    }

    /// The texts of the commands in `pending_writes`, front first.
    fn queued_commands(app: &App, harness: &Harness) -> Vec<String> {
        harness.controller.read(app, |controller, _| {
            controller
                .pending_writes
                .iter()
                .filter_map(|write| match write {
                    PtyWrite::Command { command, .. } => Some(command.clone()),
                    _ => None,
                })
                .collect()
        })
    }

    /// The app's side call `echo side-1` is written and occupies the active block: before its
    /// Preexec (`Submitted`), or running (`executing`). A second one, `echo side-2`, is queued
    /// behind it. Returns the cancel channels of both.
    fn side_call_holds_the_block(
        app: &mut App,
        harness: &Harness,
        executing: bool,
    ) -> (
        async_channel::Receiver<InBandCommandCancelledEvent>,
        async_channel::Receiver<InBandCommandCancelledEvent>,
    ) {
        let first = queue_in_band(app, harness, "echo side-1", "side-1");
        assert_eq!(written(harness), vec![command_bytes("echo side-1")]);
        harness.line_editor_status.update(app, |status, _| {
            status.set_line_editor_active_for_test(false)
        });
        let second = queue_in_band(app, harness, "echo side-2", "side-2");
        if executing {
            harness.model.lock().preexec(PreexecValue {
                command: "echo side-1".to_owned(),
                session_id: None,
            });
        }
        assert!(
            harness
                .model
                .lock()
                .block_list()
                .active_block()
                .is_in_band_command_block(),
            "the side call holds the active block"
        );
        assert_eq!(queued_commands(app, harness), vec!["echo side-2"]);
        (first, second)
    }

    /// The side call that held the block ends and the shell is at its prompt again.
    fn side_call_ends(app: &mut App, harness: &Harness, executing: bool) {
        if executing {
            harness.model.lock().finish_block();
        } else {
            shell_runs_a_command(&harness.model, "echo side-1");
        }
        line_editor_becomes_active(app, harness);
    }

    /// The r30f loss (`lab_connect_typeahead`, Windows lab): `exit` + Enter while the app's side
    /// call held the active block was refused and never written. It is parked at the front,
    /// written at the side call's prompt BEFORE the side call queued earlier, its block and the
    /// hand-off's bookkeeping start at that write, the queued side call is not cancelled, and
    /// the view is never told "not engaged".
    fn a_command_submitted_behind_a_side_call_is_written_first_at_its_prompt(executing: bool) {
        App::test((), move |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let events = record_handoff_events(&mut app, &harness);
            let (_first, second) = side_call_holds_the_block(&mut app, &harness, executing);

            // `submit` asserts the outcome is `Accepted`: the view restores nothing.
            submit(&mut app, &harness, "exit");
            assert_eq!(
                written(&harness),
                vec![command_bytes("echo side-1")],
                "nothing is written over the side call"
            );
            assert_eq!(
                queued_commands(&app, &harness),
                vec!["exit", "echo side-2"],
                "parked at the front, ahead of the queued side call"
            );
            harness.controller.read(&app, |controller, _| {
                assert!(
                    controller
                        .pending_writes
                        .front()
                        .is_some_and(is_parked_user_command)
                );
                assert!(controller.is_user_command_executing);
            });
            {
                let model = harness.model.lock();
                assert!(
                    !matches!(
                        model.typeahead_handoff().state(),
                        HandoffState::PtyOwned { .. }
                    ),
                    "the hand-off is told at the write, not at the submit"
                );
                assert!(
                    model.block_list().active_block().is_in_band_command_block(),
                    "no block is started for the command yet"
                );
            }
            let changed_at_submit = events.read(&app, |events, _| events.changed);

            side_call_ends(&mut app, &harness, executing);
            assert_eq!(
                written(&harness),
                vec![command_bytes("echo side-1"), command_bytes("exit")],
                "the user's command is written once, ahead of the queued side call"
            );
            {
                let model = harness.model.lock();
                assert!(
                    matches!(
                        model.typeahead_handoff().state(),
                        HandoffState::PtyOwned { .. }
                    ),
                    "the hand-off engaged at the write: {:?}",
                    model.typeahead_handoff().state()
                );
                let block = model.block_list().active_block();
                assert!(block.started(), "its block started at the write");
                assert!(!block.is_in_band_command_block());
            }
            assert_eq!(queued_commands(&app, &harness), vec!["echo side-2"]);
            assert!(
                second.try_recv().is_err(),
                "the queued side call was not cancelled"
            );
            events.read(&app, |events, _| {
                assert_eq!(events.not_engaged, 0, "something does take the command");
                assert!(
                    events.changed > changed_at_submit,
                    "the view is told when the command is written"
                );
            });

            // The user's command ends: the queued side call follows it, never ahead of it and
            // never dropped (when exactly is the #2182 wait's business, not this change's).
            shell_runs_a_command(&harness.model, "exit");
            line_editor_becomes_active(&mut app, &harness);
            let all = written(&harness);
            let exit_at = all
                .iter()
                .position(|bytes| *bytes == command_bytes("exit"))
                .expect("the user's command was written");
            assert_eq!(
                all.iter()
                    .filter(|bytes| **bytes == command_bytes("exit"))
                    .count(),
                1,
                "written once"
            );
            match all
                .iter()
                .position(|bytes| *bytes == command_bytes("echo side-2"))
            {
                Some(side_at) => assert!(side_at > exit_at, "{all:?}"),
                None => assert_eq!(queued_commands(&app, &harness), vec!["echo side-2"]),
            }
            assert!(second.try_recv().is_err());
        });
    }

    #[test]
    fn a_command_submitted_before_a_side_calls_preexec_is_written_first_at_its_prompt() {
        a_command_submitted_behind_a_side_call_is_written_first_at_its_prompt(false);
    }

    #[test]
    fn a_command_submitted_while_a_side_call_runs_is_written_first_at_its_prompt() {
        a_command_submitted_behind_a_side_call_is_written_first_at_its_prompt(true);
    }

    /// Two submits behind one side call run in the order they were submitted, both ahead of the
    /// queued side call; the first prompt writes only the first.
    #[test]
    fn a_second_submit_parks_behind_the_first_and_ahead_of_the_side_calls() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let (_first, second) = side_call_holds_the_block(&mut app, &harness, false);
            submit(&mut app, &harness, "one");
            let outcome = harness.controller.update(&mut app, |controller, ctx| {
                controller.write_command("two", SHELL, CommandExecutionSource::QueuedCommand, ctx)
            });
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert_eq!(
                queued_commands(&app, &harness),
                vec!["one", "two", "echo side-2"]
            );

            side_call_ends(&mut app, &harness, false);
            assert_eq!(
                written(&harness),
                vec![command_bytes("echo side-1"), command_bytes("one")]
            );
            assert_eq!(queued_commands(&app, &harness), vec!["two", "echo side-2"]);
            assert!(second.try_recv().is_err());
        });
    }

    /// The block a parked command is annotated on (its cloud workflow) is its own, at the write:
    /// never the side call's block that was active at the submit.
    #[test]
    fn a_parked_command_annotates_its_own_block_at_the_write() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let _side_calls = side_call_holds_the_block(&mut app, &harness, false);
            let annotated_in_band_block = Arc::new(Mutex::new(None));
            let slot = annotated_in_band_block.clone();
            let outcome = harness.controller.update(&mut app, |controller, ctx| {
                controller.write_command_annotating_block(
                    "exit",
                    SHELL,
                    CommandExecutionSource::User,
                    move |model| {
                        *slot.lock() =
                            Some(model.block_list().active_block().is_in_band_command_block());
                    },
                    ctx,
                )
            });
            assert_eq!(outcome, StartCommandOutcome::Accepted);
            assert_eq!(
                *annotated_in_band_block.lock(),
                None,
                "not applied at the submit"
            );

            side_call_ends(&mut app, &harness, false);
            assert_eq!(
                *annotated_in_band_block.lock(),
                Some(false),
                "applied to the command's own block"
            );
        });
    }

    /// While a command is parked no new side call is accepted: its requester is told.
    #[test]
    fn a_side_call_requested_while_a_command_is_parked_is_refused() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let _side_calls = side_call_holds_the_block(&mut app, &harness, true);
            submit(&mut app, &harness, "exit");

            let (cancel_tx, cancel_rx) = async_channel::unbounded();
            harness.controller.update(&mut app, |controller, ctx| {
                controller.queue_in_band_command(
                    "echo side-3",
                    SHELL,
                    "side-3".to_owned(),
                    cancel_tx,
                    ctx,
                );
            });
            let event = cancel_rx.try_recv().expect("the requester is told");
            assert_eq!(event.command_id, "side-3");
            assert_eq!(queued_commands(&app, &harness), vec!["exit", "echo side-2"]);
        });
    }

    /// The line editor opens while the side call's block is still the active one: the start at
    /// the write is refused, the command goes back to the front (it is not dropped, and the
    /// drain does not spin on it) and is written at the next prompt.
    #[test]
    fn a_parked_command_refused_at_its_write_goes_back_to_the_front() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let events = record_handoff_events(&mut app, &harness);
            let _side_calls = side_call_holds_the_block(&mut app, &harness, false);
            submit(&mut app, &harness, "exit");

            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(written(&harness), vec![command_bytes("echo side-1")]);
            assert_eq!(queued_commands(&app, &harness), vec!["exit", "echo side-2"]);
            events.read(&app, |events, _| assert_eq!(events.not_engaged, 0));

            side_call_ends(&mut app, &harness, false);
            assert_eq!(
                written(&harness),
                vec![command_bytes("echo side-1"), command_bytes("exit")]
            );
        });
    }

    /// The shell exits with a command parked: it goes with the queue (the handler of
    /// `ModelEvent::Exit` calls this), and side calls are possible again.
    #[test]
    fn a_shell_exit_drops_a_parked_command() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let _side_calls = side_call_holds_the_block(&mut app, &harness, false);
            submit(&mut app, &harness, "exit");

            harness.controller.update(&mut app, |controller, _| {
                controller.drop_parked_user_commands("shell_exit");
                assert!(!controller.has_parked_user_command());
                assert!(!controller.is_user_command_executing);
            });
            assert_eq!(queued_commands(&app, &harness), vec!["echo side-2"]);
            assert_eq!(written(&harness), vec![command_bytes("echo side-1")]);
        });
    }

    /// A command of the USER's that is really running still refuses the next submit (the view
    /// then gives the line back to the editor, #1125): nothing is parked behind it, and the
    /// editor's keys are released by "not engaged" as before.
    #[test]
    fn a_submit_over_a_running_user_command_is_still_refused() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let events = record_handoff_events(&mut app, &harness);
            submit(&mut app, &harness, "sleep 5");
            harness.model.lock().preexec(PreexecValue {
                command: "sleep 5".to_owned(),
                session_id: None,
            });

            let outcome = harness.controller.update(&mut app, |controller, ctx| {
                controller.write_command("ls", SHELL, CommandExecutionSource::User, ctx)
            });
            assert_eq!(outcome, StartCommandOutcome::RejectedExecuting);
            harness.controller.read(&app, |controller, _| {
                assert!(!controller.has_parked_user_command());
                assert!(controller.pending_writes.is_empty());
            });
            assert_eq!(written(&harness), vec![command_bytes("sleep 5")]);
            events.read(&app, |events, _| assert_eq!(events.not_engaged, 1));
        });
    }

    // ---- r29: #2182 follow-ups (A1, A2) and held keys are never discarded (#2198, A4) ----------

    use crate::terminal::input_queue::{
        DeliverOutcome, DestFacts, Destination, DiscardReason, EditorKeyPayload, EntryKind,
        FollowUps, GateKind, GateSpec, GateToken, Host, InputEntry, Origin, PaneQueue,
    };

    /// A host for the pane's queue that records what it is handed, in order.
    #[derive(Default)]
    struct RecordingHost {
        delivered: Vec<(&'static str, Destination)>,
        discarded: usize,
    }

    impl Host for RecordingHost {
        fn facts(&self) -> DestFacts {
            DestFacts {
                pty_takes_input: true,
                input_box_visible: true,
                pty_open: true,
                ..DestFacts::default()
            }
        }

        fn deliver(
            &mut self,
            entry: &InputEntry,
            dest: Destination,
            _follow: &mut FollowUps,
        ) -> DeliverOutcome {
            self.delivered.push((entry.kind.tag(), dest));
            DeliverOutcome::Delivered
        }

        fn discarded(&mut self, _entry: &InputEntry, _why: DiscardReason) {
            self.discarded += 1;
        }

        fn arm_give_up(&mut self, _token: GateToken, _kind: GateKind, _after: Duration) {}

        fn warn(&mut self, _line: &str) {}
    }

    /// The pane's queue of a harness, publishing its held fact to the harness's hand-off as the
    /// terminal view wires it.
    fn pane_queue(harness: &Harness) -> PaneQueue {
        let pane = PaneQueue::new();
        pane.set_keys_held_sink(Some(
            harness.model.lock().typeahead_handoff().view_keys_cell(),
        ));
        pane
    }

    fn editor_key(key: crate::editor::HeldPasteKey) -> EntryKind {
        EntryKind::EditorKey(EditorKeyPayload::new(key))
    }

    /// #2182 (r29 A1): a Tab in a remote session opened the `RemoteTabCompletion` gate and the
    /// user typed on (`x`, Enter) in the input editor: editor keys, held behind the gate. The
    /// completer's own in-band listing is NOT held by them (it was: the listing waited for the
    /// keys that waited for the listing, and the gate's net ran the uncompleted line). It goes
    /// out at once; the answer then releases the keys in typed order, so the Enter runs the
    /// completed line.
    #[test]
    fn editor_keys_behind_a_remote_tab_gate_do_not_hold_the_completers_inband_command() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let pane = pane_queue(&harness);
            let mut host = RecordingHost::default();
            {
                let mut queue = pane.lock();
                queue.open_gate(&mut host, GateSpec::new(GateKind::RemoteTabCompletion, 1));
                queue.offer_with_origin(
                    &mut host,
                    editor_key(crate::editor::HeldPasteKey::Text("x".to_owned())),
                    Origin::Editor,
                );
                queue.offer_with_origin(
                    &mut host,
                    editor_key(crate::editor::HeldPasteKey::Action(
                        crate::editor::EditorAction::Enter,
                    )),
                    Origin::Editor,
                );
                assert_eq!(queue.queued_len(), 2, "both keys wait for the completion");
            }
            assert!(host.delivered.is_empty());
            assert!(
                !harness.model.lock().typeahead_handoff().view_holds_keys(),
                "keys bound for the editor are not published as held"
            );

            let cancel_rx = queue_in_band(&mut app, &harness, "ls -1ap", "id-tab");
            assert_eq!(
                written(&harness),
                vec![bytes_to_execute_command("ls -1ap", SHELL, false)],
                "the completer's listing goes out while the keys wait for its answer"
            );
            assert!(cancel_rx.try_recv().is_err(), "not cancelled");
            harness.controller.read(&app, |controller, _| {
                assert!(!controller.in_band_wait_armed);
                assert!(!controller.handoff_request_waiting);
            });

            // The answer: the keys are applied to the completed line in typed order.
            assert!(
                pane.lock()
                    .on_gate_event(&mut host, GateKind::RemoteTabCompletion, 1)
            );
            assert_eq!(
                host.delivered,
                vec![
                    ("editor_key", Destination::Editor),
                    ("editor_key", Destination::Editor)
                ]
            );
            assert_eq!(host.discarded, 0);
        });
    }

    /// #2182 (r29 A1): the wait of #2182 itself is kept for every key that may still reach the
    /// shell's line. A destination-free text entry behind the same gate holds the in-band
    /// command (its kill prefix would wipe the line that text is about to be typed into); the
    /// release of the key writes the command, once.
    #[test]
    fn a_destination_free_key_behind_a_gate_still_holds_an_inband_command() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let pane = pane_queue(&harness);
            let mut host = RecordingHost::default();
            {
                let mut queue = pane.lock();
                queue.open_gate(&mut host, GateSpec::new(GateKind::RemoteTabCompletion, 1));
                queue.offer(&mut host, EntryKind::Text("y".to_owned()));
            }
            assert!(harness.model.lock().typeahead_handoff().view_holds_keys());

            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-text");
            assert!(
                written(&harness).is_empty(),
                "a key that may reach the shell's line holds the in-band command"
            );
            harness.controller.read(&app, |controller, _| {
                assert!(controller.in_band_wait_armed);
                assert!(controller.handoff_request_waiting);
                assert!(controller.in_band_wait_set_waiting);
            });

            assert!(
                pane.lock()
                    .on_gate_event(&mut host, GateKind::RemoteTabCompletion, 1)
            );
            assert_eq!(host.delivered.len(), 1, "the key is delivered, not dropped");
            assert_eq!(host.discarded, 0);
            assert!(!harness.model.lock().typeahead_handoff().view_holds_keys());
            Timer::after(Duration::from_millis(50)).await;
            assert_eq!(
                written(&harness),
                vec![bytes_to_execute_command("echo in-band", SHELL, false)]
            );
            assert!(cancel_rx.try_recv().is_err());
        });
    }

    /// #2182 (r29 A2): the in-band wait marks the waiting it set, and the end of a wait whose
    /// command is dropped takes back exactly that: a cancel and the give-up both leave no
    /// `handoff_request_waiting` behind with nothing to wait for.
    #[test]
    fn a_dropped_inband_wait_takes_back_the_waiting_it_set() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);

            // Cancelled by its requester.
            let harness = harness(&mut app);
            let cell = harness.model.lock().typeahead_handoff().view_keys_cell();
            cell.publish(true);
            let _cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-c");
            harness.controller.update(&mut app, |controller, _| {
                assert!(controller.handoff_request_waiting);
                assert!(controller.in_band_wait_set_waiting);
                assert!(controller.in_band_wait_armed);
                controller.cancel_in_band_command("id-c");
                assert!(controller.pending_writes.is_empty());
                assert!(
                    !controller.handoff_request_waiting,
                    "nothing waits any more: the waiting the wait set is gone with it"
                );
                assert!(!controller.in_band_wait_set_waiting);
                assert!(!controller.in_band_wait_armed);
            });
            assert!(written(&harness).is_empty());
            assert!(cell.is_held(), "the keys are untouched");

            // Given up on.
            let harness = harness_with(&mut app, zsh_session_info());
            let cell = harness.model.lock().typeahead_handoff().view_keys_cell();
            cell.publish(true);
            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-g");
            harness.controller.update(&mut app, |controller, ctx| {
                assert!(controller.in_band_wait_set_waiting);
                let generation = controller.in_band_wait_generation;
                controller.give_up_in_band_wait(generation, ctx);
                assert!(!controller.handoff_request_waiting);
                assert!(!controller.in_band_wait_set_waiting);
                assert!(!controller.in_band_wait_armed);
            });
            assert_eq!(
                cancel_rx
                    .try_recv()
                    .expect("the requester is told")
                    .command_id,
                "id-g"
            );
        });
    }

    /// #2182 (r29 A2): `handoff_request_waiting` is shared. Once another owner has written it (a
    /// parked decision, #2187; `ActiveDecision::Wait`; a held command) the end of an in-band wait
    /// does not clear it: their wait is still on.
    #[test]
    fn a_dropped_inband_wait_never_clears_a_waiting_another_owner_set() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let cell = harness.model.lock().typeahead_handoff().view_keys_cell();
            cell.publish(true);
            let _first = queue_in_band(&mut app, &harness, "echo one", "id-1");
            let _second = queue_in_band(&mut app, &harness, "echo two", "id-2");
            harness.controller.update(&mut app, |controller, _| {
                assert!(controller.in_band_wait_set_waiting);
                assert_eq!(controller.pending_writes.len(), 2);
                // The front command is cancelled while another in-band command is behind it:
                // that one waits for the same keys, so the waiting stays.
                controller.cancel_in_band_command("id-1");
                assert_eq!(controller.pending_writes.len(), 1);
                assert!(controller.handoff_request_waiting);
                assert!(controller.in_band_wait_set_waiting);

                // A request decision now has to wait as well (what the #2187 park and
                // `ActiveDecision::Wait` do): the flag is theirs from here on.
                controller.set_handoff_request_waiting(true);
                assert!(!controller.in_band_wait_set_waiting);
                controller.cancel_in_band_command("id-2");
                assert!(controller.pending_writes.is_empty());
                assert!(
                    controller.handoff_request_waiting,
                    "the decision's waiting is not the in-band wait's to clear"
                );
            });
            assert!(written(&harness).is_empty());
        });
    }

    /// r29 A2: a command held behind the user's line empties the queued writes; the requester of
    /// an in-band command that was queued is told it will not run (it only saw its channel
    /// close before), and the wait that command was in is over.
    #[test]
    fn holding_a_command_tells_the_requesters_of_the_inband_commands_it_drops() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let cell = harness.model.lock().typeahead_handoff().view_keys_cell();
            cell.publish(true);
            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-h");
            assert!(written(&harness).is_empty());
            assert!(cancel_rx.try_recv().is_err(), "still queued");

            // The queued-prompts panel fires while the view holds the user's keys: its command
            // is held, and the queue it replaces is emptied.
            fire_queued_command(&mut app, &harness);
            assert_eq!(
                cancel_rx
                    .try_recv()
                    .expect("the requester is told")
                    .command_id,
                "id-h"
            );
            assert!(
                !written(&harness).contains(&bytes_to_execute_command(
                    "echo in-band",
                    SHELL,
                    false
                )),
                "the dropped in-band command is never written"
            );
            harness.controller.read(&app, |controller, _| {
                assert!(!controller.in_band_wait_armed, "its wait is over");
                assert!(!controller.in_band_wait_set_waiting);
                assert!(controller.in_band_cancels.is_empty());
            });
        });
    }

    /// The state of #2198: `frob` was typed into the pty while a command ran, the command's
    /// prompt was seen, and the Enter of `frob` is held as a young Enter (its keys are in the
    /// view's queue; the hand-off keeps the marker). Returns the hold's id.
    fn young_enter_held_after_a_command(app: &mut App, harness: &Harness) -> u64 {
        submit(app, harness, "sleep 1");
        user_types(app, harness, b"frob");
        shell_runs_a_command(&harness.model, "sleep 1");
        let mut model = harness.model.lock();
        let mut handoff = model.typeahead_handoff_mut();
        handoff.stream_undrained_for_test();
        assert_eq!(handoff.on_user_bytes(b"\r"), UserBytesRoute::Held);
        handoff.take_young_hold_started().expect("a hold started")
    }

    /// #2198 (r29 A4), the agent / queued-panel cause: a command held behind the user's line was
    /// written as soon as the request had gone out, with the young Enter still held behind that
    /// request; its `before_write_fn` told the hand-off, which ended the hold as `Dropped`, and
    /// the view discarded the Enter and every key typed after it. Now the command stays queued
    /// while the hold is live. The report runs the held Enter (the user's line), the keys typed
    /// behind it follow that command, and only then, at the prompt after them, is the held
    /// command written: nothing dropped, typed order kept.
    #[test]
    fn a_held_command_waits_for_a_young_enter_and_the_keys_behind_it() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let hold = young_enter_held_after_a_command(&mut app, &harness);
            fire_queued_command(&mut app, &harness);

            // The request that covers `frob` goes out; the held command does not.
            line_editor_becomes_active(&mut app, &harness);
            let queued = bytes_to_execute_command("queued", SHELL, false);
            assert_eq!(written(&harness).last(), Some(&REQUEST.to_vec()));
            assert!(
                !written(&harness).contains(&queued),
                "nothing of the app's is written over a held Enter"
            );
            let gate = {
                let mut model = harness.model.lock();
                let mut handoff = model.typeahead_handoff_mut();
                assert!(handoff.young_hold_queued(), "the hold is intact");
                assert!(
                    handoff.take_young_hold_ends().is_empty(),
                    "no end was recorded: nothing is discarded"
                );
                // The report: the held Enter runs in the editor, the keys behind it wait for it.
                assert_eq!(
                    handoff.on_report(),
                    ReportDisposition::InsertThenSubmitYoung
                );
                let gate = handoff.deferred_gate().expect("a deferred Enter");
                assert_eq!(
                    handoff.take_young_hold_ends(),
                    vec![(
                        hold,
                        YoungHoldEnd::SubmitOnReport {
                            deferred_gate: gate
                        }
                    )]
                );
                gate
            };

            // The Enter submits the reported line; the keys typed behind it (`ls`, Enter)
            // follow it, as the view writes them.
            submit(&mut app, &harness, "frob");
            {
                let mut model = harness.model.lock();
                let mut handoff = model.typeahead_handoff_mut();
                assert_eq!(
                    handoff.take_young_hold_ends(),
                    vec![(gate, YoungHoldEnd::FollowCommand)]
                );
                handoff.count_followed_young_hold(gate, b"ls\r");
            }
            harness.controller.update(&mut app, |controller, ctx| {
                controller.write_handoff_user_bytes(Cow::Borrowed(&b"ls\r"[..]), ctx)
            });
            assert!(!written(&harness).contains(&queued));

            // The shell runs both lines; the held command goes out at the prompt after them.
            for command in ["frob", "ls"] {
                assert!(!written(&harness).contains(&queued), "before {command}");
                shell_runs_a_command(&harness.model, command);
                line_editor_becomes_active(&mut app, &harness);
            }
            let written = written(&harness);
            let position = |needle: &[u8]| written.iter().position(|bytes| bytes == needle);
            let line = bytes_to_execute_command("frob", SHELL, false);
            let request_at = position(REQUEST.as_slice()).expect("the request");
            let line_at = position(line.as_slice()).expect("the user's line ran");
            let keys_at =
                position(b"ls\r".as_slice()).expect("the keys behind the Enter were written");
            let queued_at =
                position(queued.as_slice()).expect("the held command was written in the end");
            assert!(
                request_at < line_at && line_at < keys_at && keys_at < queued_at,
                "typed order, then the app's command: {written:?}"
            );
            assert_eq!(
                written.iter().filter(|bytes| **bytes == queued).count(),
                1,
                "the held command is written exactly once"
            );
        });
    }

    /// #2198 (r29 A4), the in-band cause: with the request out and a young Enter held behind
    /// it, nothing but the hold says "wait" (the pty no longer owns input, the view published
    /// nothing here). The app's in-band command waits for the hold and is written once the
    /// report has ended it. The hold is never ended by the command, so no key is discarded.
    #[test]
    fn an_inband_command_waits_for_a_young_enter_held_behind_the_request() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let hold = young_enter_held_after_a_command(&mut app, &harness);
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(written(&harness).last(), Some(&REQUEST.to_vec()));
            assert_eq!(
                harness.model.lock().typeahead_handoff().state(),
                &HandoffState::AwaitingReport
            );

            let in_band = bytes_to_execute_command("echo in-band", SHELL, false);
            let cancel_rx = queue_in_band(&mut app, &harness, "echo in-band", "id-y");
            assert!(
                !written(&harness).contains(&in_band),
                "the in-band command waits for the held Enter"
            );
            {
                let mut model = harness.model.lock();
                let mut handoff = model.typeahead_handoff_mut();
                assert!(handoff.young_hold_queued());
                assert!(handoff.take_young_hold_ends().is_empty());
                assert_eq!(
                    handoff.on_report(),
                    ReportDisposition::InsertThenSubmitYoung
                );
                let ends = handoff.take_young_hold_ends();
                assert_eq!(ends.len(), 1);
                assert_eq!(ends[0].0, hold);
                assert!(matches!(ends[0].1, YoungHoldEnd::SubmitOnReport { .. }));
            }
            // The hold has ended: the decision the view asks for lets the command out.
            harness.controller.update(&mut app, |controller, ctx| {
                controller.retry_typeahead_handoff_request(ctx)
            });
            let written = written(&harness);
            assert_eq!(
                written.iter().filter(|bytes| **bytes == in_band).count(),
                1,
                "written once, after the hold: {written:?}"
            );
            assert!(cancel_rx.try_recv().is_err());
        });
    }

    /// #2198 (r29 A4): with the kill switch of the in-band wait the command is written over the
    /// held Enter, as before #2182; even then the hold is not ended as `Dropped`: behind the
    /// request it stays, and the report runs the Enter.
    #[test]
    fn the_inband_kill_switch_never_discards_a_held_young_enter() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let harness = harness(&mut app);
            let hold = young_enter_held_after_a_command(&mut app, &harness);
            line_editor_becomes_active(&mut app, &harness);
            assert_eq!(written(&harness).last(), Some(&REQUEST.to_vec()));
            harness
                .model
                .lock()
                .typeahead_handoff_mut()
                .set_inband_wait_for_keys_enabled(false);

            queue_in_band(&mut app, &harness, "echo in-band", "id-k2");
            assert_eq!(
                written(&harness).last(),
                Some(&bytes_to_execute_command("echo in-band", SHELL, false)),
                "the switch writes the command at once"
            );
            let mut model = harness.model.lock();
            let mut handoff = model.typeahead_handoff_mut();
            assert!(handoff.young_hold_queued(), "the hold is intact");
            assert!(handoff.take_young_hold_ends().is_empty(), "nothing dropped");
            assert_eq!(
                handoff.on_report(),
                ReportDisposition::InsertThenSubmitYoung
            );
            let ends = handoff.take_young_hold_ends();
            assert_eq!(ends.len(), 1);
            assert_eq!(ends[0].0, hold);
            assert!(matches!(ends[0].1, YoungHoldEnd::SubmitOnReport { .. }));
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
                        park: CommandPark::None,
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

/// #2153: an ambiguous Tab's exchange is answered with its prompt but never with results (the
/// user's ^C interrupts the hook). The ^C ends it, so the next Tab (different text) starts a
/// fresh exchange instead of being refused as "already in flight"; the abandoned requester sees
/// "no answer"; a late reply of the old hook is ignored and never reaches the new request.
#[test]
fn a_ctrl_c_ends_an_exchange_awaiting_results_so_the_next_tab_starts_fresh() {
    App::test((), |mut app| async move {
        let (controller, model_events, _status, sender, _tx) =
            native_completions_controller(&mut app);
        let first = controller.update(&mut app, |c, ctx| start_exchange(c, "cd amb", ctx));
        model_events.update(&mut app, |_d, ctx| {
            ctx.emit(ModelEvent::SendCompletionsPrompt {
                token: Some("1".to_owned()),
            });
        });
        controller.read(&app, |c, _| {
            assert!(matches!(
                c.in_flight_native_completions_state,
                Some(NativeShellCompletionsState::AwaitingResults { .. })
            ));
        });
        // The abandoned Tab: ^C.
        controller.update(&mut app, |c, ctx| c.write_bytes(&[0x03][..], ctx));
        controller.read(&app, |c, _| {
            assert!(c.in_flight_native_completions_state.is_none());
            assert!(!c.native_cancel_owed);
        });
        assert!(
            first.recv().await.is_err(),
            "the abandoned Tab sees no answer"
        );

        // The next Tab, other text: a fresh exchange (its own ^Y), not a refusal.
        let second = controller.update(&mut app, |c, ctx| start_exchange(c, "cd zzq", ctx));
        Timer::after(Duration::from_millis(20)).await;
        assert!(!second.is_closed(), "the new Tab is not refused");
        controller.read(&app, |c, _| {
            assert!(matches!(
                c.in_flight_native_completions_state,
                Some(NativeShellCompletionsState::AwaitingPrompt { .. })
            ));
        });
        assert_eq!(
            written(&sender)
                .iter()
                .filter(|b| b.as_slice() == [0x19])
                .count(),
            2,
            "one ^Y per exchange"
        );
        // The old hook's late reply arrives while the new exchange waits for its prompt: ignored.
        model_events.update(&mut app, |_d, ctx| {
            ctx.emit(ModelEvent::CompletionsFinished(vec![]));
        });
        assert!(
            second.try_recv().is_err(),
            "the late old reply is not the new answer"
        );
        controller.read(&app, |c, _| {
            assert!(matches!(
                c.in_flight_native_completions_state,
                Some(NativeShellCompletionsState::AwaitingPrompt { .. })
            ));
        });
    });
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
    native_completions_controller_with_sessions(app, Vec::new())
}

/// [`native_completions_controller`] with `session_infos` registered as bootstrapped sessions.
fn native_completions_controller_with_sessions(
    app: &mut App,
    session_infos: Vec<SessionInfo>,
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
    sessions.update(app, |sessions, _| {
        for session_info in session_infos {
            sessions.register_session_for_test(session_info);
        }
    });
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

/// #2218: the id of the remote session the tests below bootstrap.
const REMOTE_SESSION: u64 = 1;

/// #2218: the local login shell (session 0) and one ssh session (session 1) per ssh kind: with a
/// ControlMaster socket, and on one connection (#1933).
fn local_and_ssh_session_infos() -> Vec<Vec<SessionInfo>> {
    let remote = || {
        SessionInfo::new_for_test()
            .with_id(REMOTE_SESSION)
            .with_spawning_session_id(SessionId::from(0))
    };
    vec![
        vec![
            SessionInfo::new_for_test(),
            remote().with_ssh_socket_path(std::path::PathBuf::from("/nonexistent/cm.sock")),
        ],
        vec![
            SessionInfo::new_for_test(),
            remote().with_single_connection_ssh(),
        ],
    ]
}

/// #2218: the user runs `command` from the editor at a prompt (the real write path).
fn editor_runs(
    app: &mut App,
    controller: &ModelHandle<PtyController<TestEventLoopSender>>,
    command: &str,
) {
    controller.update(app, |controller, ctx| {
        let outcome =
            controller.write_command(command, ShellType::Bash, CommandExecutionSource::User, ctx);
        assert_eq!(outcome, StartCommandOutcome::Accepted);
    });
}

/// #2218: the shell of `session_id` reports it has bootstrapped (the event the dispatcher emits
/// once the session is registered).
fn shell_bootstraps(
    app: &mut App,
    model_events: &ModelHandle<ModelEventDispatcher>,
    session_id: u64,
    is_subshell: bool,
) {
    model_events.update(app, |_dispatcher, ctx| {
        ctx.emit(ModelEvent::Handler(AnsiHandlerEvent::Bootstrapped {
            session_id: SessionId::from(session_id),
            is_subshell,
        }));
    });
}

/// #2218 (`lab_tab_remote_dir.p_slowrc`): `ssh host` from the editor sets
/// `is_user_command_executing`, and nothing cleared it until the first REMOTE command finished,
/// so the first Tab after the connect was refused ("a command is executing") and went to the
/// completer, whose side commands each source the remote user's slow rc. Once the remote shell
/// has bootstrapped, the first Tab asks it in-band: the `^Y` is written and the requester waits
/// for the answer. Both ssh kinds (ControlMaster socket, one connection).
#[test]
fn the_first_tab_after_an_ssh_connect_asks_the_remote_shell_in_band() {
    for session_infos in local_and_ssh_session_infos() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(false);
            let (controller, model_events, line_editor_status, sender, _tx) =
                native_completions_controller_with_sessions(&mut app, session_infos);

            editor_runs(&mut app, &controller, "ssh host");
            controller.read(&app, |c, _| {
                assert!(c.is_user_command_executing);
                assert!(c.pending_writes.is_empty());
            });
            // While ssh connects (no remote shell yet) a Tab is still refused.
            let refused = controller.update(&mut app, |c, ctx| start_exchange(c, "cd zzq", ctx));
            assert!(
                refused.is_closed(),
                "no in-band exchange while ssh connects"
            );
            assert!(!written(&sender).iter().any(|bytes| bytes == &[0x19]));

            // The remote shell bootstraps and reaches its first prompt.
            shell_bootstraps(&mut app, &model_events, REMOTE_SESSION, false);
            line_editor_status.update(&mut app, |status, _| {
                status.set_line_editor_active_for_test(true)
            });
            controller.read(&app, |c, _| {
                assert!(
                    !c.is_user_command_executing,
                    "the remote shell reads the pty now; `ssh host` no longer blocks requests"
                );
            });

            // The first Tab at the remote prompt, before any remote command has run.
            let first_tab = controller.update(&mut app, |c, ctx| start_exchange(c, "cd zzq", ctx));
            assert!(
                !first_tab.is_closed(),
                "the first Tab is not handed to the completer"
            );
            controller.read(&app, |c, _| {
                assert!(matches!(
                    c.in_flight_native_completions_state,
                    Some(NativeShellCompletionsState::AwaitingPrompt { .. })
                ));
            });
            assert_eq!(
                written(&sender)
                    .iter()
                    .filter(|bytes| bytes.as_slice() == [0x19])
                    .count(),
                1,
                "exactly one ^Y: the remote shell was asked"
            );
        });
    }
}

/// #2218: a shell command queued while the connection came up (written from the editor while no
/// line editor was reading) set the flag for itself; the remote bootstrap leaves the flag, and
/// the queued command, alone.
#[test]
fn the_ssh_bootstrap_leaves_the_flag_to_a_shell_command_still_queued() {
    for session_infos in local_and_ssh_session_infos() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(false);
            let (controller, model_events, line_editor_status, _sender, _tx) =
                native_completions_controller_with_sessions(&mut app, session_infos);
            // No line editor is reading (the connection is coming up): the command is queued.
            line_editor_status.update(&mut app, |status, _| {
                status.set_line_editor_active_for_test(false)
            });
            editor_runs(&mut app, &controller, "echo queued");
            controller.read(&app, |c, _| {
                assert!(c.is_user_command_executing);
                assert!(
                    c.has_queued_shell_command(),
                    "the command waits for a prompt"
                );
            });

            shell_bootstraps(&mut app, &model_events, REMOTE_SESSION, false);
            controller.read(&app, |c, _| {
                assert!(
                    c.is_user_command_executing,
                    "the queued command still owns the flag"
                );
                assert!(c.has_queued_shell_command(), "and it is still queued");
            });
        });
    }
}

/// #2218: only an ssh / remote shell's bootstrap is treated like a subshell's. The local login
/// shell's own Bootstrapped, and one for a session nothing registered, leave the flag as it is;
/// a subshell's clears it as before.
#[test]
fn only_a_nested_shells_bootstrap_clears_the_executing_flag() {
    for session_infos in local_and_ssh_session_infos() {
        App::test((), |mut app| async move {
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(false);
            let (controller, model_events, _status, sender, _tx) =
                native_completions_controller_with_sessions(&mut app, session_infos);
            editor_runs(&mut app, &controller, "sleep 5");

            // The local login shell (session 0), and a session id nothing knows.
            for session_id in [0, 99] {
                shell_bootstraps(&mut app, &model_events, session_id, false);
                controller.read(&app, |c, _| {
                    assert!(c.is_user_command_executing, "session {session_id}");
                });
            }
            let refused = controller.update(&mut app, |c, ctx| start_exchange(c, "cd zzq", ctx));
            assert!(refused.is_closed(), "the local command is still running");
            assert!(!written(&sender).iter().any(|bytes| bytes == &[0x19]));

            // A subshell: cleared, as before #2218.
            shell_bootstraps(&mut app, &model_events, 99, true);
            controller.read(&app, |c, _| assert!(!c.is_user_command_executing));
        });
    }
}
