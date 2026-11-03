//! The PowerShell insert chord (#1581): a single-line command is written to the session's command
//! file and the pty gets exactly `ESC 3`; the shell reads the file. These tests drive
//! `send_write_to_event_loop` the way the queue-draining path does and read what reaches the
//! event loop.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::{FairMutex, Mutex};
use warpui::{App, ModelHandle};

use super::*;
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::session::{SessionId, Sessions};

#[derive(Clone, Default)]
struct RecordingSender {
    messages: Arc<Mutex<Vec<Message>>>,
}

impl EventLoopSender for RecordingSender {
    fn send(&self, message: Message) -> Result<(), EventLoopSendError> {
        self.messages.lock().push(message);
        Ok(())
    }
}

fn payload(message: &Message) -> Vec<u8> {
    match message {
        Message::Input(bytes) | Message::CommandInput(bytes) => bytes.to_vec(),
        _ => panic!("unexpected message"),
    }
}

struct Rig {
    controller: ModelHandle<PtyController<RecordingSender>>,
    sender: RecordingSender,
}

fn rig(app: &mut App, session: u64, powershell_local: bool) -> Rig {
    let model = Arc::new(FairMutex::new(TerminalModel::mock(
        None,
        Some(ChannelEventListener::new_for_test()),
    )));
    let (_events_tx, events_rx) = async_channel::unbounded();
    let (_executor_tx, executor_rx) = async_channel::unbounded();
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let model_events =
        app.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));
    model_events.update(app, |dispatcher, _| {
        dispatcher.set_active_session_id(SessionId::from(session));
    });
    let line_editor_status =
        app.add_model(|ctx| LineEditorStatus::new(model_events.clone(), sessions.clone(), ctx));
    let sender = RecordingSender::default();
    let controller = app.add_model(|ctx| {
        PtyController::new(
            sender.clone(),
            model_events,
            line_editor_status,
            sessions,
            executor_rx,
            model,
            ctx,
        )
    });
    controller.update(app, |controller, _| {
        controller.runs_long_commands_from_file = powershell_local;
    });
    Rig { controller, sender }
}

fn command(text: &str, counter: Option<Arc<AtomicUsize>>) -> PtyWrite {
    PtyWrite::Command {
        command: text.to_owned(),
        shell_type: ShellType::PowerShell,
        in_band_command_id: None,
        before_write_fn: counter.map(|counter| {
            Box::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                StartCommandOutcome::Accepted
            }) as Box<dyn FnOnce() -> StartCommandOutcome + Send + 'static>
        }),
    }
}

fn typed_bytes(text: &str) -> Vec<u8> {
    bytes_to_execute_command(text, ShellType::PowerShell, false)
}

#[test]
fn a_single_line_powershell_command_is_one_chord_write_and_a_file() {
    App::test((), |mut app| async move {
        let session = 9_100_001;
        let rig = rig(&mut app, session, true);
        let written = Arc::new(AtomicUsize::new(0));
        let sent = rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command("Get-Date", Some(written.clone())), ctx)
        });
        assert!(sent);
        assert_eq!(
            written.load(Ordering::SeqCst),
            1,
            "on_command_written runs once"
        );

        let messages = rig.sender.messages.lock();
        assert_eq!(messages.len(), 1, "exactly one pty write, not ESC 2 + text");
        assert!(matches!(&messages[0], Message::CommandInput(_)));
        assert_eq!(payload(&messages[0]), b"\x1b3".to_vec());

        let path = long_command_file::session_file_path(&std::env::temp_dir(), session);
        let bytes = std::fs::read(&path).expect("the command file exists");
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(&bytes[3..], b"Get-Date");
        drop(messages);
        rig.controller.read(&app, |controller, _| {
            assert_eq!(controller.file_insert_pending.as_deref(), Some("Get-Date"));
        });
        let _ = std::fs::remove_file(&path);
    });
}

#[test]
fn multi_line_long_and_control_character_commands_keep_the_typed_path() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app, 9_100_002, true);
        let long = format!("echo '{}'", "a".repeat(long_command_file::MAX_TYPED_BYTES));
        let cases = [
            "echo a\necho b",
            "echo a\r\necho b",
            "echo a\tb",
            "echo \u{1b}x",
        ];
        for text in cases {
            rig.sender.messages.lock().clear();
            rig.controller.update(&mut app, |controller, ctx| {
                controller.send_write_to_event_loop(command(text, None), ctx)
            });
            let messages = rig.sender.messages.lock();
            // ESC 2 on its own, then the text and the Enter.
            assert_eq!(messages.len(), 2, "{text:?}");
            assert_eq!(payload(&messages[0]), b"\x1b2".to_vec());
            let expected = typed_bytes(text);
            assert_eq!(payload(&messages[1]), expected[2..].to_vec());
        }
        // Past the typed limit the #1106 launcher path still applies (a typed launcher line).
        rig.sender.messages.lock().clear();
        rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command(&long, None), ctx)
        });
        let messages = rig.sender.messages.lock();
        assert_eq!(messages.len(), 2);
        assert!(payload(&messages[1]).starts_with(b". (Warp-Run-LongCommand '"));
        rig.controller.read(&app, |controller, _| {
            assert!(controller.file_insert_pending.is_none());
        });
    });
}

#[test]
fn in_band_commands_and_non_local_powershell_are_typed() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app, 9_100_003, true);
        rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(
                PtyWrite::Command {
                    command: "Get-Date".to_owned(),
                    shell_type: ShellType::PowerShell,
                    in_band_command_id: Some("id".to_owned()),
                    before_write_fn: None,
                },
                ctx,
            )
        });
        assert_eq!(rig.sender.messages.lock().len(), 2);

        let remote = self::rig(&mut app, 9_100_004, false);
        remote.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command("Get-Date", None), ctx)
        });
        let messages = remote.sender.messages.lock();
        assert_eq!(messages.len(), 2);
        assert_eq!(payload(&messages[0]), b"\x1b2".to_vec());
        assert_eq!(payload(&messages[1]), typed_bytes("Get-Date")[2..].to_vec());
    });
}

#[test]
fn the_failure_hook_retypes_the_command_once_and_only_once() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app, 9_100_005, true);
        let written = Arc::new(AtomicUsize::new(0));
        rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command("Get-Date", Some(written.clone())), ctx);
        });
        assert_eq!(rig.sender.messages.lock().len(), 1);

        rig.controller.update(&mut app, |controller, ctx| {
            controller.on_command_file_failed(ctx);
        });
        {
            let messages = rig.sender.messages.lock();
            assert_eq!(messages.len(), 3, "chord, then ESC 2, then the line");
            assert_eq!(payload(&messages[1]), b"\x1b2".to_vec());
            assert_eq!(payload(&messages[2]), typed_bytes("Get-Date")[2..].to_vec());
        }
        assert_eq!(
            written.load(Ordering::SeqCst),
            1,
            "the retype does not re-run on_command_written"
        );
        rig.controller.read(&app, |controller, _| {
            assert!(controller.file_insert_pending.is_none());
            assert!(!controller.force_typed_command);
        });

        // A second report has nothing to retype: no duplicate.
        rig.controller.update(&mut app, |controller, ctx| {
            controller.on_command_file_failed(ctx);
        });
        assert_eq!(rig.sender.messages.lock().len(), 3);
    });
}

#[test]
fn a_preexec_clears_the_outstanding_chord_so_a_late_failure_retypes_nothing() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app, 9_100_006, true);
        rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command("Get-Date", None), ctx);
            // What the Preexec subscription does.
            controller.file_insert_pending = None;
            controller.on_command_file_failed(ctx);
        });
        assert_eq!(rig.sender.messages.lock().len(), 1, "only the chord");
    });
}

#[test]
fn a_second_command_is_typed_while_a_chord_is_outstanding() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app, 9_100_007, true);
        rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command("one", None), ctx);
            controller.send_write_to_event_loop(command("two", None), ctx);
        });
        let messages = rig.sender.messages.lock();
        assert_eq!(messages.len(), 3);
        assert_eq!(payload(&messages[0]), b"\x1b3".to_vec());
        assert_eq!(payload(&messages[1]), b"\x1b2".to_vec());
    });
}

#[test]
fn a_refused_write_leaves_no_outstanding_chord() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app, 9_100_008, true);
        let sent = rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(
                PtyWrite::Command {
                    command: "Get-Date".to_owned(),
                    shell_type: ShellType::PowerShell,
                    in_band_command_id: None,
                    before_write_fn: Some(Box::new(|| StartCommandOutcome::RejectedExecuting)),
                },
                ctx,
            )
        });
        assert!(!sent);
        assert!(rig.sender.messages.lock().is_empty());
        rig.controller.read(&app, |controller, _| {
            assert!(controller.file_insert_pending.is_none());
        });
    });
}

#[test]
fn a_precmd_without_a_preexec_retypes_once_and_a_preexec_first_retypes_nothing() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app, 9_100_009, true);
        rig.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command("Get-Date", None), ctx);
            controller.on_precmd_with_chord_outstanding(ctx);
            // A second Precmd has nothing outstanding.
            controller.on_precmd_with_chord_outstanding(ctx);
        });
        {
            let messages = rig.sender.messages.lock();
            assert_eq!(messages.len(), 3, "chord, then one typed retype");
            assert_eq!(payload(&messages[1]), b"\x1b2".to_vec());
            assert_eq!(payload(&messages[2]), typed_bytes("Get-Date")[2..].to_vec());
        }

        // Normal flow: Preexec clears the chord, the Precmd after the command retypes nothing.
        let normal = self::rig(&mut app, 9_100_010, true);
        normal.controller.update(&mut app, |controller, ctx| {
            controller.send_write_to_event_loop(command("Get-Date", None), ctx);
            controller.file_insert_pending = None; // what the Preexec subscription does
            controller.on_precmd_with_chord_outstanding(ctx);
        });
        assert_eq!(normal.sender.messages.lock().len(), 1);
    });
}
