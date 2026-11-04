//! #1606: a plain one-line command to a zsh whose line editor vouched for literal typing
//! (`typed_text_literal` on its `LineEditorReading` marker) is typed raw: the kill-buffer chord,
//! the text and Enter, without the bracketed-paste wrapper zle takes ~15 ms longer over on macOS.
//! Every other case is written exactly as before. These tests drive `send_write_to_event_loop`
//! the way the queue-draining path does and read the exact bytes that reach the event loop.

use std::sync::Arc;

use parking_lot::{FairMutex, Mutex};
use warpui::{App, ModelHandle};

use super::*;
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::input_queue::DestinationMode;
use crate::terminal::model::ansi::{Handler as _, LineEditorMarkerValue, PreexecValue};
use crate::terminal::model::session::{SessionId, Sessions};

const SESSION: u64 = 9_160_601;

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
    model: Arc<FairMutex<TerminalModel>>,
    sender: RecordingSender,
}

impl Rig {
    /// What the shell's `LineEditorReading` marker does to the model.
    fn mark(&self, typed_text_literal: bool) {
        self.model
            .lock()
            .line_editor_reading(LineEditorMarkerValue {
                session_id: Some(SESSION),
                seq: Some(1),
                typed_text_literal,
            });
    }

    /// Writes one command and returns the bytes of every message it produced, in order.
    fn submit(
        &self,
        app: &mut App,
        text: &str,
        shell_type: ShellType,
        in_band_command_id: Option<&str>,
    ) -> Vec<Vec<u8>> {
        self.sender.messages.lock().clear();
        let sent = self.controller.update(app, |controller, ctx| {
            controller.send_write_to_event_loop(
                PtyWrite::Command {
                    command: text.to_owned(),
                    shell_type,
                    in_band_command_id: in_band_command_id.map(str::to_owned),
                    before_write_fn: None,
                },
                ctx,
            )
        });
        assert!(sent, "{text:?} was not written");
        self.sender.messages.lock().iter().map(payload).collect()
    }
}

/// A controller whose shell has bracketed paste on, as zsh's zle does at its prompt.
fn rig(app: &mut App) -> Rig {
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
        dispatcher.set_active_session_id(SessionId::from(SESSION));
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
            model.clone(),
            ctx,
        )
    });
    controller.update(app, |controller, _| {
        controller.set_bracketed_paste_enabled(true);
    });
    Rig {
        controller,
        model,
        sender,
    }
}

/// The bytes of a wrapped command, exactly as before #1606.
fn wrapped(text: &str, shell_type: ShellType) -> Vec<u8> {
    let bytes = bytes_to_execute_command(text, shell_type, true);
    assert!(
        bytes
            .windows(escape_sequences::BRACKETED_PASTE_START.len())
            .any(|w| w == escape_sequences::BRACKETED_PASTE_START),
        "{text:?} is not wrapped"
    );
    bytes
}

/// `raw` for `text` to zsh where the raw path applies, i.e. off Windows; on a ConPTY the line
/// keeps the wrapper.
fn raw_off_windows(raw: &[u8], text: &str) -> Vec<u8> {
    if cfg!(windows) {
        wrapped(text, ShellType::Zsh)
    } else {
        raw.to_vec()
    }
}

const DESTINATION_MODES: [DestinationMode; 2] = [DestinationMode::Legacy, DestinationMode::Table];

#[test]
fn a_plain_zsh_line_is_typed_raw_once_per_marker_in_both_destination_modes() {
    for mode in DESTINATION_MODES {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            let rig = rig(&mut app);

            rig.mark(true);
            // ^P (the kill-buffer chord), the text, zsh's Enter: no ESC [ 200 ~.
            assert_eq!(
                rig.submit(&mut app, "ls -la", ShellType::Zsh, None),
                vec![raw_off_windows(b"\x10ls -la\n", "ls -la")],
                "{mode:?}"
            );
            // The same line as a bracketed paste would have been this; only the wrapper went.
            let mut expected_wrapped = b"\x10".to_vec();
            expected_wrapped.extend_from_slice(escape_sequences::BRACKETED_PASTE_START);
            expected_wrapped.extend_from_slice(b"ls -la");
            expected_wrapped.extend_from_slice(escape_sequences::BRACKETED_PASTE_END);
            expected_wrapped.push(b'\n');
            assert_eq!(wrapped("ls -la", ShellType::Zsh), expected_wrapped);

            // The marker vouched for one line: the next command, before the next prompt's
            // marker, keeps the wrapper.
            assert_eq!(
                rig.submit(&mut app, "ls -la", ShellType::Zsh, None),
                vec![expected_wrapped.clone()],
                "{mode:?}"
            );

            // The harness's latency stamp line is plain too.
            let stamp = "zmodload zsh/datetime; print -r -- $EPOCHREALTIME ${__pa_px:-0} ${__pa_lf:-0} > lat_3";
            rig.mark(true);
            let mut expected = b"\x10".to_vec();
            expected.extend_from_slice(stamp.as_bytes());
            expected.push(b'\n');
            assert_eq!(
                rig.submit(&mut app, stamp, ShellType::Zsh, None),
                vec![raw_off_windows(&expected, stamp)]
            );
        });
    }
}

#[test]
fn lines_that_are_not_plain_keep_the_wrapper_exactly_as_before() {
    for mode in DESTINATION_MODES {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            let rig = rig(&mut app);
            let long = format!("echo {}", "a".repeat(RAW_SUBMIT_MAX_BYTES));
            let cases = [
                "echo a\necho b",
                "echo a\r\necho b",
                "echo a\tb",
                "echo \u{1b}x",
                "echo \u{7f}",
                "echo hi!",
                "!!",
                "^foo^bar",
                "echo caf\u{e9}",
                long.as_str(),
            ];
            for text in cases {
                rig.mark(true);
                assert_eq!(
                    rig.submit(&mut app, text, ShellType::Zsh, None),
                    vec![wrapped(text, ShellType::Zsh)],
                    "{mode:?} {text:?}"
                );
            }
        });
    }
}

#[test]
fn without_the_line_editors_word_the_wrapper_stays() {
    for mode in DESTINATION_MODES {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            let rig = rig(&mut app);
            // No marker yet (a bootstrap without the field, markers off, a builtin read).
            assert_eq!(
                rig.submit(&mut app, "ls", ShellType::Zsh, None),
                vec![wrapped("ls", ShellType::Zsh)]
            );
            // A marker that found a binding or a self-insert wrapper.
            rig.mark(false);
            assert_eq!(
                rig.submit(&mut app, "ls", ShellType::Zsh, None),
                vec![wrapped("ls", ShellType::Zsh)]
            );
            // A Preexec after the marker: the line it vouched for is gone.
            rig.mark(true);
            rig.model.lock().preexec(PreexecValue {
                command: "sleep 5".to_owned(),
                session_id: Some(SESSION),
            });
            assert_eq!(
                rig.submit(&mut app, "ls", ShellType::Zsh, None),
                vec![wrapped("ls", ShellType::Zsh)]
            );
        });
    }
}

#[test]
fn the_kill_switch_keeps_the_wrapper() {
    for mode in DESTINATION_MODES {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            let rig = rig(&mut app);
            rig.controller.update(&mut app, |controller, _| {
                controller.enter_bracketed_always_for_test = true;
            });
            rig.mark(true);
            assert_eq!(
                rig.submit(&mut app, "ls -la", ShellType::Zsh, None),
                vec![wrapped("ls -la", ShellType::Zsh)]
            );
        });
    }
}

#[test]
fn other_shells_and_in_band_commands_keep_the_wrapper() {
    for mode in DESTINATION_MODES {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            let rig = rig(&mut app);
            for shell_type in [ShellType::Bash, ShellType::Fish, ShellType::Nu] {
                rig.mark(true);
                assert_eq!(
                    rig.submit(&mut app, "ls -la", shell_type, None),
                    vec![wrapped("ls -la", shell_type)],
                    "{mode:?} {shell_type:?}"
                );
            }
            rig.mark(true);
            assert_eq!(
                rig.submit(&mut app, "ls -la", ShellType::Zsh, Some("in-band-1")),
                vec![wrapped("ls -la", ShellType::Zsh)]
            );
            // ...and the in-band write used up the marker: the user's command after it is
            // wrapped as well.
            assert_eq!(
                rig.submit(&mut app, "ls -la", ShellType::Zsh, None),
                vec![wrapped("ls -la", ShellType::Zsh)]
            );
        });
    }
}

/// A user write that can end the shell's line (Enter, ^C, ^D) ends what the marker vouched for;
/// a keystroke that cannot does not.
#[test]
fn a_line_ending_user_write_uses_up_the_marker() {
    for mode in DESTINATION_MODES {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            let rig = rig(&mut app);
            for ending in [
                &b"\r"[..],
                &b"\n"[..],
                &b"\x03"[..],
                &b"\x04"[..],
                &b"ab\rc"[..],
            ] {
                rig.mark(true);
                rig.controller.update(&mut app, |controller, ctx| {
                    controller.write_bytes(ending.to_vec(), ctx);
                });
                assert_eq!(
                    rig.submit(&mut app, "ls", ShellType::Zsh, None),
                    vec![wrapped("ls", ShellType::Zsh)],
                    "{mode:?} {ending:?}"
                );
            }
            rig.mark(true);
            rig.controller.update(&mut app, |controller, ctx| {
                controller.write_bytes(b"x".to_vec(), ctx);
            });
            assert_eq!(
                rig.submit(&mut app, "ls", ShellType::Zsh, None),
                vec![raw_off_windows(b"\x10ls\n", "ls")],
                "{mode:?}"
            );
        });
    }
}

/// With bracketed paste off nothing changes: typed as is, and the marker is still used up.
#[test]
fn without_bracketed_paste_the_command_is_typed_as_before() {
    App::test((), |mut app| async move {
        let rig = rig(&mut app);
        rig.controller.update(&mut app, |controller, _| {
            controller.set_bracketed_paste_enabled(false);
        });
        rig.mark(true);
        assert_eq!(
            rig.submit(&mut app, "printf 'a'\nprintf 'b'", ShellType::Zsh, None),
            vec![bytes_to_execute_command(
                "printf 'a'\nprintf 'b'",
                ShellType::Zsh,
                false
            )]
        );
        rig.controller.update(&mut app, |controller, _| {
            controller.set_bracketed_paste_enabled(true);
        });
        assert_eq!(
            rig.submit(&mut app, "ls", ShellType::Zsh, None),
            vec![wrapped("ls", ShellType::Zsh)]
        );
    });
}

#[test]
fn the_plain_line_predicate() {
    for plain in [
        "ls",
        "ls -la",
        " leading space",
        "a",
        "echo \"$HOME\" 'x' `pwd` $(date) ${a:-b} ~/x #c",
        "grep ^foo file",
        "a ^ b",
        "printf '%s\\n' x | sort > out; true && false || :",
        "~ ` @ # $ % & * ( ) _ + = { } [ ] | \\ : ; \" ' < > , . ? /",
    ] {
        assert!(line_is_plain_for_raw_submit(plain), "{plain:?}");
    }
    let at_cap = "a".repeat(RAW_SUBMIT_MAX_BYTES);
    assert!(line_is_plain_for_raw_submit(&at_cap));

    let over_cap = "a".repeat(RAW_SUBMIT_MAX_BYTES + 1);
    for not_plain in [
        "",
        "a\nb",
        "a\rb",
        "ls\n",
        "a\tb",
        "\u{1b}[A",
        "a\u{7f}",
        "a\u{0}b",
        "a\u{3}",
        "a\u{10}",
        "echo !!",
        "!ls",
        "a!b",
        "^a^b",
        "caf\u{e9}",
        "\u{1f600}",
        "a\u{85}b",
        over_cap.as_str(),
    ] {
        assert!(!line_is_plain_for_raw_submit(not_plain), "{not_plain:?}");
    }
}

#[test]
fn the_submit_path_names_why_it_wraps() {
    let vouched = EnterSubmitFacts {
        bracketed_paste_enabled: true,
        user_command: true,
        line_editor_types_literally: true,
        tmux_control_mode: false,
        conpty: false,
        always_bracketed: false,
    };
    assert_eq!(
        enter_submit_path("ls", ShellType::Zsh, vouched),
        EnterSubmitPath::Raw
    );
    let cases = [
        (
            EnterSubmitFacts {
                bracketed_paste_enabled: false,
                ..vouched
            },
            ShellType::Zsh,
            "ls",
            EnterSubmitPath::Unbracketed,
        ),
        (vouched, ShellType::Zsh, "", EnterSubmitPath::Unbracketed),
        (
            EnterSubmitFacts {
                always_bracketed: true,
                ..vouched
            },
            ShellType::Zsh,
            "ls",
            EnterSubmitPath::Wrapped("kill_switch"),
        ),
        (
            EnterSubmitFacts {
                user_command: false,
                ..vouched
            },
            ShellType::Zsh,
            "ls",
            EnterSubmitPath::Wrapped("in_band"),
        ),
        (
            vouched,
            ShellType::Bash,
            "ls",
            EnterSubmitPath::Wrapped("shell"),
        ),
        (
            vouched,
            ShellType::Fish,
            "ls",
            EnterSubmitPath::Wrapped("shell"),
        ),
        (
            vouched,
            ShellType::PowerShell,
            "ls",
            EnterSubmitPath::Wrapped("shell"),
        ),
        (
            vouched,
            ShellType::Nu,
            "ls",
            EnterSubmitPath::Wrapped("shell"),
        ),
        (
            EnterSubmitFacts {
                tmux_control_mode: true,
                ..vouched
            },
            ShellType::Zsh,
            "ls",
            EnterSubmitPath::Wrapped("tmux"),
        ),
        (
            EnterSubmitFacts {
                conpty: true,
                ..vouched
            },
            ShellType::Zsh,
            "ls",
            EnterSubmitPath::Wrapped("conpty"),
        ),
        (
            EnterSubmitFacts {
                line_editor_types_literally: false,
                ..vouched
            },
            ShellType::Zsh,
            "ls",
            EnterSubmitPath::Wrapped("line_editor"),
        ),
        (
            vouched,
            ShellType::Zsh,
            "a\nb",
            EnterSubmitPath::Wrapped("text"),
        ),
    ];
    for (facts, shell_type, line, expected) in cases {
        let path = enter_submit_path(line, shell_type, facts);
        assert_eq!(path, expected, "{facts:?} {shell_type:?} {line:?}");
        // Bracketed exactly when the old rule would have bracketed and the path is not raw.
        assert_eq!(
            path.is_bracketed(),
            command_is_sent_as_bracketed_paste(facts.bracketed_paste_enabled, line)
                && path != EnterSubmitPath::Raw
        );
    }
}

#[test]
fn the_kill_switch_values() {
    for on in ["1", "on", "true", "yes", " TRUE ", "Yes"] {
        assert!(enter_bracketed_always_from(Some(on)), "{on:?}");
    }
    for off in [
        None,
        Some(""),
        Some("0"),
        Some("off"),
        Some("no"),
        Some("2"),
    ] {
        assert!(!enter_bracketed_always_from(off), "{off:?}");
    }
}
