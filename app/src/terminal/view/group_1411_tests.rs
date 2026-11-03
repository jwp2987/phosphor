//! #1411: control keys typed while the terminal has focus are translated and routed by the pane key
//! sink, from the published key snapshot, without the model's lock. Child of `view_test.rs`
//! (private helpers come in through `super`).

use super::*;
use crate::terminal::input_queue::DestinationMode;

/// A key down as the pane's key sink receives it, with focus on the terminal view itself.
fn pane_key_down(
    view: &mut TerminalView,
    keystroke: &str,
    chars: &str,
    ctx: &mut ViewContext<TerminalView>,
) -> PaneKeyOutcome {
    let key = PaneKey {
        seq: 0,
        kind: PaneKeyKind::KeyDown {
            keystroke: warpui::keymap::Keystroke::parse(keystroke).expect("keystroke parses"),
            chars: chars.to_owned(),
            is_composing: false,
            will_type: false,
            key_without_modifiers: None,
        },
    };
    view.on_pane_key_event(
        &key,
        &PaneKeyContext {
            focus_chain: vec![view.view_id],
            leaf_takes_typed_text: false,
        },
        ctx,
    )
}

fn pane_typed_on_terminal(
    view: &mut TerminalView,
    chars: &str,
    ctx: &mut ViewContext<TerminalView>,
) -> PaneKeyOutcome {
    let key = PaneKey {
        seq: 0,
        kind: PaneKeyKind::Typed {
            chars: chars.to_owned(),
            ime_commit: false,
        },
    };
    view.on_pane_key_event(
        &key,
        &PaneKeyContext {
            focus_chain: vec![view.view_id],
            leaf_takes_typed_text: false,
        },
        ctx,
    )
}

/// A burst of typed keys, an Enter and a Ctrl-C reaches the running command in exactly the typed
/// order, whichever way the destination is decided: nothing lost, duplicated or reordered.
#[test]
fn typed_keys_enter_and_ctrl_c_keep_their_order_through_the_sink() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model.lock().simulate_long_running_block("cat", "");
                for c in ["l", "s", " ", "-", "l"] {
                    assert_eq!(
                        pane_typed_on_terminal(view, c, ctx),
                        PaneKeyOutcome::Consumed
                    );
                }
                assert_eq!(
                    pane_key_down(view, "enter", "\r", ctx),
                    PaneKeyOutcome::Consumed
                );
                for c in ["x", "y"] {
                    assert_eq!(
                        pane_typed_on_terminal(view, c, ctx),
                        PaneKeyOutcome::Consumed
                    );
                }
                assert_eq!(
                    pane_key_down(view, "ctrl-c", "\u{3}", ctx),
                    PaneKeyOutcome::Consumed
                );
            });
            assert_eq!(
                pty_writes.borrow().concat(),
                b"ls -l\rxy\x03".to_vec(),
                "{mode:?}: every key once, in typed order"
            );
        });
    }
}

/// Arrows, Tab and Enter are translated by the sink: an arrow has no control characters at all.
#[test]
fn arrows_and_tab_are_translated_by_the_sink_in_both_modes() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model.lock().simulate_long_running_block("cat", "");
                for (keystroke, chars) in [("up", ""), ("tab", "\t"), ("down", ""), ("enter", "\r")]
                {
                    assert_eq!(
                        pane_key_down(view, keystroke, chars, ctx),
                        PaneKeyOutcome::Consumed,
                        "{keystroke}"
                    );
                }
            });
            assert_eq!(
                pty_writes.borrow().concat(),
                b"\x1b[A\t\x1b[B\r".to_vec(),
                "{mode:?}"
            );
        });
    }
}

/// A printable key down (its typed event follows) with nothing to translate is not taken.
#[test]
fn a_plain_printable_key_down_is_left_for_its_typed_event() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        let outcome = terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            view.model.lock().simulate_long_running_block("cat", "");
            pane_key_down(view, "a", "a", ctx)
        });
        assert_eq!(outcome, PaneKeyOutcome::NotMine);
        assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
    });
}

/// The control-key path takes no model lock: with the pty reader inside the model's lock, arrows,
/// Tab, Enter, Ctrl-D and Ctrl-C still reach the pty at once and in order, and none of them counts
/// a blocking acquisition on the key path.
#[test]
fn control_keys_with_terminal_focus_take_no_model_lock() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                {
                    let mut model = view.model.lock();
                    model.simulate_long_running_block("cat", "");
                    model.publish_key_facts();
                }
                let model = view.model.clone();
                let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
                let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
                let parser = std::thread::spawn(move || {
                    let _parsing = model.lock();
                    held_tx.send(()).unwrap();
                    let _ = done_rx.recv_timeout(Duration::from_secs(3));
                });
                held_rx.recv().unwrap();
                let locks_before = crate::terminal::model_lock_trace::key_path_lock_count();
                let started = Instant::now();
                for (keystroke, chars) in [
                    ("up", ""),
                    ("tab", "\t"),
                    ("enter", "\r"),
                    ("ctrl-d", "\u{4}"),
                    ("ctrl-c", "\u{3}"),
                ] {
                    assert_eq!(
                        pane_key_down(view, keystroke, chars, ctx),
                        PaneKeyOutcome::Consumed,
                        "{keystroke}"
                    );
                }
                let waited = started.elapsed();
                let locks = crate::terminal::model_lock_trace::key_path_lock_count() - locks_before;
                done_tx.send(()).unwrap();
                parser.join().unwrap();
                assert!(
                    waited < Duration::from_secs(1),
                    "{mode:?}: the control keys waited {waited:?} for the parser"
                );
                assert_eq!(locks, 0, "{mode:?}: no blocking model lock on the key path");
            });
            assert_eq!(
                pty_writes.borrow().concat(),
                b"\x1b[A\t\r\x04\x03".to_vec(),
                "{mode:?}: every key once, in order"
            );
        });
    }
}
