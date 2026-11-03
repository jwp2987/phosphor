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

/// A kitty flag the program popped in the middle of a parse slice (so the published copy is one
/// slice stale) never puts CSI-u bytes into the shell: the key is encoded with the modes of the
/// moment they changed, with the parser still holding the model.
#[test]
fn a_kitty_pop_mid_slice_is_seen_by_the_next_enter() {
    use crate::terminal::model::key_facts::PublishedKeyModes;
    App::test((), |mut app| async move {
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
            // The copy published after the last slice says the program is in kitty mode ...
            let mut stale = view.key_model.load();
            stale.key_modes = PublishedKeyModes {
                keyboard_protocol: true,
                report_all_as_escape: true,
                ..PublishedKeyModes::default()
            };
            view.key_model.store(stale);
            assert_eq!(
                pane_key_down(view, "enter", "\r", ctx),
                PaneKeyOutcome::Consumed
            );
            // ... the program pops its flags (the parser stores the eager word mid-slice).
            view.key_model
                .store_eager_modes(PublishedKeyModes::default(), false);
            assert_eq!(
                pane_key_down(view, "enter", "\r", ctx),
                PaneKeyOutcome::Consumed
            );
            done_tx.send(()).unwrap();
            parser.join().unwrap();
        });
        assert_eq!(
            pty_writes.borrow().concat(),
            b"\x1b[13u\r".to_vec(),
            "CSI-u while the program is in kitty mode, a plain CR the moment it is not"
        );
    });
}

/// Tab, Escape, F1 and an Alt-letter reach the running command through the sink.
#[test]
fn tab_escape_f1_and_alt_letter_are_translated_by_the_sink() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            view.model.lock().simulate_long_running_block("cat", "");
            for (keystroke, chars) in [
                ("tab", "\t"),
                ("escape", "\u{1b}"),
                ("f1", ""),
                ("alt-b", "b"),
            ] {
                assert_eq!(
                    pane_key_down(view, keystroke, chars, ctx),
                    PaneKeyOutcome::Consumed,
                    "{keystroke}"
                );
            }
        });
        assert_eq!(pty_writes.borrow().concat(), b"\t\x1b\x1bOP\x1bb".to_vec());
    });
}

/// The Ctrl-C binding (it fires before the sink) takes no model lock either: with the parser
/// inside the model, a ^C into a running command reaches the pty at once, and the focus
/// redetermination it wants is left for the next wakeup.
#[test]
fn ctrl_c_binding_takes_no_model_lock() {
    App::test((), |mut app| async move {
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
            view.handle_ctrl_c_input_event(0, ctx);
            let waited = started.elapsed();
            let locks = crate::terminal::model_lock_trace::key_path_lock_count() - locks_before;
            let deferred = view.global_focus_redetermine_deferred;
            done_tx.send(()).unwrap();
            parser.join().unwrap();
            assert!(
                waited < Duration::from_secs(1),
                "the Ctrl-C waited {waited:?} for the parser"
            );
            assert_eq!(locks, 0);
            assert!(deferred, "the focus redetermination waits for the wakeup");
        });
        assert_eq!(pty_writes.borrow().concat(), vec![0x03]);
    });
}

/// A selection bit the published copy still carries -- the alt screen's selection scrolled away
/// in the slice being parsed (`AltScreen::rotate_selection` sends no event), or the previous ^C's
/// clear still sits in the model's inbox -- never turns a Ctrl-C into a copy of nothing (on Windows
/// a copy swallows the interrupt). With the parser holding the model and no real selection, the ^C
/// confirms under the lock, exactly once, and reaches the pty.
#[test]
fn a_stale_selection_bit_never_swallows_a_ctrl_c_as_a_copy() {
    App::test((), |mut app| async move {
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
            // The copy says the alt screen has a selection; the model has none.
            view.key_model.store_selections(false, true);
            assert!(view.key_model.load_exact_modes().has_alt_screen_selection);
            // The parser holds the model for the rest of its slice, then lets go.
            let model = view.model.clone();
            let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
            let parser = std::thread::spawn(move || {
                let _parsing = model.lock();
                held_tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(200));
            });
            held_rx.recv().unwrap();
            let locks_before = crate::terminal::model_lock_trace::key_path_lock_count();
            view.handle_ctrl_c_input_event(0, ctx);
            let locks = crate::terminal::model_lock_trace::key_path_lock_count() - locks_before;
            parser.join().unwrap();
            assert_eq!(locks, 1, "the selection was confirmed under the lock, once");
            assert!(
                !view.key_model.load_exact_modes().has_alt_screen_selection,
                "the confirmation corrected the published copy"
            );
        });
        assert_eq!(
            pty_writes.borrow().concat(),
            vec![0x03],
            "the ^C interrupted the command; nothing was copied"
        );
    });
}

/// A selection clear a Ctrl-C left in the model's inbox (the parser held the model) lands at the
/// next output wakeup at the latest, not "at the next byte or key": a selection the user makes in
/// the meantime cannot be wiped by a clear that waited for the pty.
#[test]
fn a_selection_clear_left_in_the_inbox_lands_at_the_next_wakeup() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            {
                let mut model = view.model.lock();
                model.alt_screen_mut().start_selection(
                    Point::new(1, 1),
                    SelectionType::Simple,
                    Side::Left,
                );
                assert!(model.alt_screen().selection().is_some());
            }
            // The ^C found the parser inside the model and left the clear for it; no bytes follow.
            view.model_inbox.post(ModelCommand::ClearAltScreenSelection);
            assert!(view.model_inbox.has_pending());
            view.handle_terminal_wakeup(crate::terminal::event_listener::Wakeup::Output, ctx);
            assert!(
                !view.model_inbox.has_pending(),
                "the wakeup applied the inbox"
            );
            assert!(
                view.model.lock().alt_screen().selection().is_none(),
                "the clear landed at the wakeup"
            );
        });
    });
}

/// A stand-in for the pty reader: holds the model's lock for `hold`, as a parse slice does, then
/// lets go. Returns once the lock is held.
fn parser_holds_model(
    model: &Arc<FairMutex<TerminalModel>>,
    hold: Duration,
) -> std::thread::JoinHandle<()> {
    let model = model.clone();
    let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
    let parser = std::thread::spawn(move || {
        let _parsing = model.lock();
        held_tx.send(()).unwrap();
        std::thread::sleep(hold);
    });
    held_rx.recv().unwrap();
    parser
}

/// The first Ctrl-C on an agent-driven command hands the command to the user and sends nothing;
/// the second interrupts. Both read "is the agent in control" from the published copy while the
/// pty reader holds the model, so the take-over must reach that copy under the lock that made it:
/// otherwise the second ^C, one slice later, found "agent in control" again, tried a second
/// take-over (`InvalidTakeOver`) and the interrupt was lost.
#[test]
fn a_second_ctrl_c_after_the_take_over_interrupts_while_the_parser_holds_the_model() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            let conversation_id = BlocklistAIHistoryModel::handle(ctx)
                .update(ctx, |history, ctx| {
                    history.start_new_conversation(view.view_id, false, false, ctx)
                });
            {
                let mut model = view.model.lock();
                model.simulate_long_running_block("sleep 20", "running");
                let task_id = crate::ai::agent::task::TaskId::new("test-cli-subagent".to_owned());
                model
                    .block_list_mut()
                    .active_block_mut()
                    .set_agent_interaction_mode_for_agent_monitored_command(
                        &task_id,
                        conversation_id,
                    )
                    .expect("command should become agent monitored");
                model.publish_key_facts();
            }
            assert!(view.key_model.load().input_box.block_agent_in_control);

            // ^C #1 with the parser inside the model: it waits for the slice and takes over.
            let parser = parser_holds_model(&view.model, Duration::from_millis(150));
            view.handle_ctrl_c_input_event(0, ctx);
            parser.join().unwrap();
            assert!(
                !view
                    .model
                    .lock()
                    .block_list()
                    .active_block()
                    .is_agent_in_control(),
                "the first ^C handed the command to the user"
            );
            assert!(
                pty_writes.borrow().is_empty(),
                "the first ^C hands over, it does not interrupt"
            );
            let copy = view.key_model.load();
            assert!(
                !copy.input_box.block_agent_in_control,
                "the take-over was published under the lock that made it, before any slice end"
            );
            assert_eq!(copy.conversation_to_stop, Some(conversation_id));
            assert!(!view.key_facts.load().agent_in_control);

            // ^C #2, the parser inside the model again: it must interrupt, not take over twice.
            let parser = parser_holds_model(&view.model, Duration::from_millis(150));
            view.handle_ctrl_c_input_event(0, ctx);
            parser.join().unwrap();
        });
        assert_eq!(
            pty_writes.borrow().concat(),
            vec![0x03],
            "the second ^C reached the pty"
        );
    });
}
