//! Owner decision 2026-11-09 (REVERSING #1661): a hold that gives up writes its keys to the pty
//! in typed order; nothing is dropped and no discard toast is shown (`view/give_up_to_pty.rs`).
//! Second ruling of that day, KEEP THEM TOGETHER: the keys typed after an Enter go where the
//! line before it went. A line that did not run and stays in the editor keeps them with it (text
//! on the lines after it, nothing at the pty); a line that is certainly a shell line (an unready
//! shell's net) goes to the pty with them, in typed order. Both shapes are tested here, at the
//! view's two functions, and through the give-up families in the files named next.
//! The give-up families themselves (an Enter gate's nets, a young Enter's caps, the CLI-agent
//! submit's cap, the deferred Enter's patiences, the ssh connect give-ups) are in
//! `group2_timer_tests.rs`, `view_test.rs` and `input_test.rs`, changed for the decision; this
//! file holds the edge cases the decision names. Legacy and Table. Child of `view_test.rs`.

use super::*;
use crate::terminal::input_queue;
use crate::terminal::input_queue::DestinationMode;
use crate::terminal::view::AfterEnterInput;

fn pty_key(id: u64, bytes: &[u8]) -> input_queue::InputEntry {
    input_queue::InputEntry {
        id: input_queue::EntryId(id),
        kind: input_queue::EntryKind::PtyBytes {
            bytes: bytes.to_vec(),
            takes_over_agent_control: true,
        },
        focus_epoch: 0,
        arrived_at: std::time::Instant::now(),
        origin: input_queue::Origin::Keyboard,
        stamp: input_queue::Stamp::default(),
    }
}

fn typed(id: u64, text: &str) -> input_queue::InputEntry {
    input_queue::InputEntry {
        kind: input_queue::EntryKind::Text(text.to_owned()),
        ..pty_key(id, b"")
    }
}

fn enter(id: u64) -> input_queue::InputEntry {
    input_queue::InputEntry {
        kind: input_queue::EntryKind::Enter,
        ..pty_key(id, b"")
    }
}

fn written(pty: &Rc<RefCell<Vec<Vec<u8>>>>) -> String {
    String::from_utf8_lossy(&pty.borrow().concat()).into_owned()
}

fn toast_texts(app: &mut App) -> Rc<RefCell<Vec<String>>> {
    let texts = Rc::new(RefCell::new(Vec::new()));
    let sink = texts.clone();
    app.update(|ctx| {
        let stack = crate::workspace::ToastStack::handle(ctx);
        ctx.subscribe_to_model(&stack, move |_, event, _| {
            if let crate::workspace::ToastStackEvent::AddEphemeralToast { toast, .. } = event {
                sink.borrow_mut()
                    .push(toast.main_text_for_test().to_string());
            }
        });
    });
    texts
}

/// Every form a held key can take (text and an Enter typed in the editor, pty keys, a second
/// Enter) goes to the pty as the bytes a plain terminal would have sent, in typed order, with the
/// held Enters in their places. Nothing goes to the editor and no toast is shown.
#[test]
fn given_up_keys_of_every_form_reach_the_pty_in_typed_order_with_no_toast() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let toasts = toast_texts(&mut app);
            terminal.update(&mut app, |view, ctx| {
                let entries = vec![
                    typed(1, "echo a"),
                    enter(2),
                    pty_key(3, b"echo b"),
                    pty_key(4, b"\r"),
                    typed(5, "tail"),
                ];
                assert_eq!(view.write_given_up_entries_to_pty(entries, "test", ctx), 5);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
            });
            assert_eq!(written(&pty), "echo a\recho b\rtail", "{mode:?}");
            assert!(
                toasts.borrow().is_empty(),
                "{mode:?}: no toast: {:?}",
                toasts.borrow()
            );
        });
    }
}

/// A secret read is up (the tty is canonical with echo off: a password prompt) when the hold
/// gives up. The held keys still go to the pty, to whatever reads it, as in a plain terminal;
/// they are never shown as text in the editor.
#[test]
fn given_up_keys_go_to_the_pty_while_a_secret_is_being_read() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                view.model
                    .lock()
                    .set_secret_read_probe(std::sync::Arc::new(|| true));
                assert!(view.model.lock().tty_reading_secret(), "{mode:?}");
                let entries = vec![pty_key(1, b"hunter2"), pty_key(2, b"\r")];
                view.write_given_up_entries_to_pty(entries, "test", ctx);
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "",
                    "{mode:?}: a secret is never put into the editor"
                );
            });
            assert_eq!(written(&pty), "hunter2\r", "{mode:?}");
        });
    }
}

/// The pty is gone (the shell exited) when the hold gives up: nothing can read a write. The keys
/// become text in THIS pane's input, not run and guarded, never dropped silently; nothing is
/// written, here or in another pane.
#[test]
fn given_up_keys_become_editor_text_when_the_pty_is_gone() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let other = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let other_pty = collect_pty_writes(&mut app, &other);
            terminal.update(&mut app, |view, ctx| {
                view.pty_gone = true;
                let entries = vec![pty_key(1, b"ls"), pty_key(2, b"\r"), pty_key(3, b"pwd")];
                assert_eq!(view.write_given_up_entries_to_pty(entries, "test", ctx), 3);
                view.input.read(ctx, |input, ctx| {
                    assert_eq!(
                        input.buffer_text(ctx),
                        "ls\npwd",
                        "{mode:?}: in typed order, the Enter a newline, nothing run"
                    );
                    assert!(
                        input.ssh_handover_enter_guard_armed(),
                        "{mode:?}: the first Enter on it is held"
                    );
                });
            });
            assert_eq!(written(&pty), "", "{mode:?}: nothing reads the pty");
            assert_eq!(written(&other_pty), "", "{mode:?}: never another pane");
            other.read(&app, |view, ctx| {
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
            });
        });
    }
}

/// A give-up writes into the pty of the pane that held the keys, never into another pane's.
#[test]
fn a_give_up_writes_only_to_its_own_panes_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let other = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let other_pty = collect_pty_writes(&mut app, &other);
            terminal.update(&mut app, |view, ctx| {
                view.begin_cli_submit_gate(ctx);
                assert!(view.write_user_bytes_to_pty(b"mine".to_vec(), ctx));
                let token = view.paste_queue.queue.borrow().gates()[0].token;
                view.on_paste_queue_give_up(token, ctx);
            });
            assert_eq!(written(&pty), "mine", "{mode:?}");
            assert_eq!(
                written(&other_pty),
                "",
                "{mode:?}: the other pane got nothing"
            );
        });
    }
}

/// #1768: an Enter gate's give-up on a BUSY queue (a peek out, so the take is deferred to the
/// mailbox) writes the same keys to the pty that a free queue's give-up writes, once the queue is
/// free; a key typed meanwhile follows them. Nothing is discarded on either path.
#[test]
fn an_enter_gate_give_up_on_a_busy_queue_writes_the_same_keys_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        for busy in [false, true] {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let terminal = add_window_with_terminal(&mut app, None);
                let pty = collect_pty_writes(&mut app, &terminal);
                let input = terminal.read(&app, |view, _| view.input.clone());
                input.update(&mut app, |input, ctx| {
                    input.clear_buffer_and_reset_undo_stack(ctx);
                    input.open_submitted_enter_gate_for_test(
                        vec![
                            AfterEnterInput::Text("ls".to_owned()),
                            AfterEnterInput::Enter,
                            AfterEnterInput::Text("pwd".to_owned()),
                        ],
                        true,
                        ctx,
                    );
                });
                let queue = terminal.read(&app, |view, _| view.paste_queue.queue.clone());
                {
                    let _held = busy.then(|| queue.peek());
                    input.update(&mut app, |input, ctx| {
                        // The gate's key is the wait's id (1 in the test seam).
                        input.give_up_enter_route_wait(1, ctx);
                    });
                    if busy {
                        assert!(queue.has_deferred(), "{mode:?}: the take waits");
                        assert_eq!(written(&pty), "", "{mode:?}: nothing before the take");
                    }
                }
                terminal.update(&mut app, |view, ctx| view.flush_deferred_queue_ops(ctx));
                for _ in 0..200 {
                    if !queue.has_deferred() && written(&pty).ends_with("pwd") {
                        break;
                    }
                    warpui::r#async::Timer::after(std::time::Duration::from_millis(5)).await;
                }
                assert_eq!(
                    written(&pty),
                    "ls\rpwd",
                    "{mode:?} busy={busy}: typed order, once, nothing dropped"
                );
                terminal.read(&app, |view, ctx| {
                    let input = view.input.as_ref(ctx);
                    assert_eq!(input.buffer_text(ctx), "", "{mode:?} busy={busy}");
                    assert!(!input.enter_queue_holds_keys(ctx), "{mode:?} busy={busy}");
                    assert!(
                        view.paste_queue.queue.borrow().is_idle(),
                        "{mode:?} busy={busy}"
                    );
                });
            });
        }
    }
}

/// The leak net of a deferred Enter's `HandoffReport` gate: the hand-off's deferral is given up
/// with the gate, the Enter's line stays in the editor (not run), and the keys typed behind it
/// stay WITH it, on the line after it, in typed order; nothing is written to the pty. A key typed
/// afterwards is not held behind the same gate again (the hand-off no longer defers), so the pane
/// does not go on holding its keys.
///
/// Expectation history (re-decided by the owner, never weakened): the first ruling of 2026-11-09
/// (#1996) wrote `ls`, Enter to the pty and left `ho 47` alone in the editor, which this test
/// expected (`..._writes_its_keys_and_ends_the_deferral`); the second ruling of that day, "keep
/// them together", keeps them with the unrun line.
#[test]
fn the_deferred_enters_gate_leak_net_keeps_its_keys_with_the_line_and_ends_the_deferral() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                hold_after_deferred_enter(view);
                view.input
                    .update(ctx, |input, ctx| input.replace_buffer_content("ho 47", ctx));
                view.open_deferred_enter_gate(ctx);
                view.write_user_bytes_to_pty(b"ls".to_vec(), ctx);
                view.write_user_bytes_to_pty(b"\r".to_vec(), ctx);
                assert_eq!(view.paste_queue.queue.borrow().queued_len(), 2, "{mode:?}");
                let token = view
                    .paste_queue
                    .queue
                    .borrow()
                    .gates()
                    .iter()
                    .find(|gate| gate.kind == input_queue::GateKind::HandoffReport)
                    .map(|gate| gate.token)
                    .expect("the deferred Enter's gate");
                view.on_paste_queue_give_up(token, ctx);
                assert_eq!(
                    view.model.lock().typeahead_handoff().state(),
                    &HandoffState::EditorOwned,
                    "{mode:?}: the deferral is given up with the gate"
                );
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "ho 47\nls\n",
                    "{mode:?}: the Enter's line stays, not run, and the keys typed behind it \
                     stay with it, in typed order, once"
                );
                assert!(
                    view.input.as_ref(ctx).ssh_handover_enter_guard_armed(),
                    "{mode:?}: the first Enter on the lines is held"
                );
                assert!(view.paste_queue.queue.borrow().is_idle(), "{mode:?}");
            });
            assert_eq!(
                written(&pty),
                "",
                "{mode:?}: `ls` must not run without the line before it: nothing at the pty"
            );
            terminal.read(&app, |view, _| {
                assert!(
                    !view
                        .paste_queue
                        .queue
                        .borrow()
                        .has_gate_kind(input_queue::GateKind::HandoffReport),
                    "{mode:?}: nothing holds keys behind the same gate again"
                );
            });
        });
    }
}

/// "Keep them together" (owner decision 2026-11-09), the shape where the line did NOT run and
/// stays in the editor (a verdict, Tab-completion or report give-up; an edit that voided the
/// Enter): `cd build`, Enter, `make clean`, Enter. The keys typed after the Enter, in every form
/// (text typed in the editor, a Backspace and keys typed for the pty, the Enters), stay WITH the
/// line as text on the lines after it, in typed order, the Enters as line breaks. NOTHING is
/// written to the pty (`make clean` never runs alone), nothing is dropped, no toast is shown for
/// them, and the hand-back guard holds the first Enter on the buffer.
#[test]
fn keys_typed_after_an_enter_that_did_not_run_stay_with_its_line_in_the_editor() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let toasts = toast_texts(&mut app);
            terminal.update(&mut app, |view, ctx| {
                view.input.update(ctx, |input, ctx| {
                    input.replace_buffer_content("cd build", ctx)
                });
                let entries = vec![
                    typed(1, "make clex"),
                    pty_key(2, &[0x7f]),
                    typed(3, "an"),
                    enter(4),
                    pty_key(5, b"ls"),
                    pty_key(6, b"\r"),
                ];
                assert_eq!(
                    view.keep_given_up_keys_with_line(entries, "test", ctx),
                    6,
                    "{mode:?}"
                );
                view.input.read(ctx, |input, ctx| {
                    assert_eq!(
                        input.buffer_text(ctx),
                        "cd build\nmake clean\nls\n",
                        "{mode:?}: the line, then what was typed after its Enter, in typed order"
                    );
                    assert!(
                        input.ssh_handover_enter_guard_armed(),
                        "{mode:?}: the first Enter on the lines is held, never run unseen"
                    );
                });
            });
            assert_eq!(
                written(&pty),
                "",
                "{mode:?}: nothing typed after an Enter that did not run reaches the pty"
            );
            assert!(
                toasts.borrow().is_empty(),
                "{mode:?}: no toast for the kept keys: {:?}",
                toasts.borrow()
            );
        });
    }
}

/// The same with an EMPTY editor (the line was cleared by the edit that voided the Enter): there
/// is no line to follow, so no line break is put in front; the keys are still kept as text, in
/// typed order, guarded, and nothing reaches the pty.
#[test]
fn keys_kept_in_an_empty_editor_get_no_leading_line_break() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                view.input.update(ctx, |input, ctx| {
                    input.clear_buffer_and_reset_undo_stack(ctx);
                });
                let entries = vec![typed(1, "ls"), enter(2), typed(3, "pwd")];
                assert_eq!(
                    view.keep_given_up_keys_with_line(entries, "test", ctx),
                    3,
                    "{mode:?}"
                );
                view.input.read(ctx, |input, ctx| {
                    assert_eq!(input.buffer_text(ctx), "ls\npwd", "{mode:?}");
                    assert!(input.ssh_handover_enter_guard_armed(), "{mode:?}");
                });
            });
            assert_eq!(written(&pty), "", "{mode:?}");
        });
    }
}

/// "Keep them together" (owner decision 2026-11-09), the shape where the line is certainly a
/// shell line: the net of an Enter held for an unready shell. `cd build`, Enter, `make clean`,
/// Enter all go to the pty in typed order, like a plain terminal; the editor is left empty (the
/// line left for the shell), unguarded, and no toast is shown.
#[test]
fn an_unready_shells_give_up_writes_the_line_its_enter_and_the_keys_in_typed_order() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            let toasts = toast_texts(&mut app);
            terminal.update(&mut app, |view, ctx| {
                view.input.update(ctx, |input, ctx| {
                    input.replace_buffer_content("cd build", ctx);
                    input.set_given_up_unready_line_for_test("cd build");
                });
                let entries = vec![typed(1, "make clean"), enter(2)];
                view.write_unready_line_and_keys_to_pty(entries, "unready_enter", ctx);
                view.input.read(ctx, |input, ctx| {
                    assert_eq!(
                        input.buffer_text(ctx),
                        "",
                        "{mode:?}: the line left for the shell"
                    );
                    assert!(!input.ssh_handover_enter_guard_armed(), "{mode:?}");
                });
                // A second call finds no line waiting and has nothing to write.
                view.write_unready_line_and_keys_to_pty(Vec::new(), "unready_enter", ctx);
            });
            assert_eq!(
                written(&pty),
                "cd build\rmake clean\r",
                "{mode:?}: the line, its Enter, then what was typed after it, once"
            );
            assert!(
                toasts.borrow().is_empty(),
                "{mode:?}: no toast: {:?}",
                toasts.borrow()
            );
        });
    }
}

/// The unready shell's give-up with the pty GONE: the line and what followed it are still kept
/// together, as text in this pane's input (the Enters line breaks), guarded; nothing is written.
#[test]
fn an_unready_shells_give_up_with_the_pty_gone_keeps_the_line_and_keys_together_as_text() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                view.pty_gone = true;
                view.input.update(ctx, |input, ctx| {
                    input.replace_buffer_content("cd build", ctx);
                    input.set_given_up_unready_line_for_test("cd build");
                });
                let entries = vec![typed(1, "make clean"), enter(2)];
                view.write_unready_line_and_keys_to_pty(entries, "unready_enter", ctx);
                view.input.read(ctx, |input, ctx| {
                    assert_eq!(
                        input.buffer_text(ctx),
                        "cd build\nmake clean\n",
                        "{mode:?}: together, in typed order, nothing run"
                    );
                    assert!(input.ssh_handover_enter_guard_armed(), "{mode:?}");
                });
            });
            assert_eq!(written(&pty), "", "{mode:?}: nothing reads the pty");
        });
    }
}

/// A connect give-up with a held Enter and keys from both sides of the pane: everything held
/// goes to the pty in typed order at the give-up, for the ssh process that still runs; nothing is
/// kept for the local shell after it, nothing is put into the editor.
#[test]
fn an_ssh_connect_give_up_writes_every_held_key_in_typed_order() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                submit_command(view, "ssh host", ctx);
            });
            // The submit's own write is setup; only the give-up's writes are judged.
            pty.borrow_mut().clear();
            terminal.update(&mut app, |view, ctx| {
                view.typed_characters_on_terminal("echo one", ctx);
                view.keydown_on_terminal("\r", ctx);
                view.typed_characters_on_terminal("echo two", ctx);
                assert!(view.ssh_typeahead_holding, "{mode:?}: held for the connect");
                let gate = view.ssh_connect_gate.as_mut().unwrap();
                gate.armed_at =
                    Instant::now() - SSH_CONNECT_NO_PROBE_GIVE_UP - Duration::from_secs(1);
                view.ssh_give_up_fired(ctx);
                assert!(!view.ssh_typeahead_holding, "{mode:?}: the hold ended");
                assert!(view.ssh_held_is_empty(), "{mode:?}");
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "",
                    "{mode:?}: nothing is put into the editor"
                );
                assert_eq!(
                    view.input.as_ref(ctx).kept_handed_back_text(),
                    None,
                    "{mode:?}: nothing is kept for after the ssh exit"
                );
            });
            assert_eq!(
                written(&pty),
                "echo one\recho two",
                "{mode:?}: typed order, the Enter in its place, once"
            );
        });
    }
}
