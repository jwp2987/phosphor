//! Group 1: queue yield latch (#1421 follow-up), ssh hand-over after a release without an event,
//! the mid-drain prompt hand-over (#1575), the gate-liveness check (#1578) and the held-text
//! hand-back (#1576). Child of `view_test.rs` (private helpers come in through `super`).

use super::*;
use crate::terminal::input_queue::DestinationMode;

/// 1e: a focus change while the hand-over drain is yielded must not deliver (or insert) what is
/// queued behind the yielding entry. `ls` waits behind a test gate, the Enter's command is
/// `ssh host`, the gate's release hands `ls` to the new connect hold, and a focus change in the
/// same update leaves it held.
#[test]
fn focus_change_during_a_yield_does_not_deliver() {
    App::test((), |mut app| async move {
        let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(
            DestinationMode::Table,
        );
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            hold_text_behind_test_gate(view, "ls", ctx);
            submit_command(view, "ssh host", ctx);
            release_test_gate(view, ctx);
            view.redetermine_terminal_focus(ctx);
            assert_eq!(queued_texts(view), vec!["ls".to_owned()]);
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "");
        });
        for _ in 0..16 {
            futures_lite::future::yield_now().await;
        }
        assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
    })
}

/// 1d (ii): the connect window was released without an event handler (what
/// `release_ssh_gate_for_test` does); the next live key hands the held text over FIRST (to the
/// remote shell's input, whose prompt was seen) and is typed behind it: once, in typed order.
#[test]
fn release_without_event_hands_over_before_the_next_key() {
    App::test((), |mut app| async move {
        let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(
            DestinationMode::Table,
        );
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            submit_command(view, "ssh host", ctx);
            view.typed_characters_on_terminal("echo QW", ctx);
            assert!(view.ssh_typeahead_holding);
            view.ssh_remote_prompt_seen = true;
            first_precmd_processed_for_test(view, ctx);
            release_ssh_gate_for_test(view);
            view.typed_characters_on_terminal("ERTY", ctx);
            assert!(!view.ssh_typeahead_holding);
            assert!(ssh_queue_is_idle(view));
            assert!(!view.ssh_handover_after_release);
        });
        // The remote prompt was seen and its session is registered (first Precmd processed): the
        // editor owns typing there (the sibling `ssh_editor_keys_typed_before_delivery_stay_behind_
        // the_held_keys` ends in the editor and writes nothing to the pty), so the held text and
        // the key typed behind it both land in the editor: once, in typed order, never run and
        // never split between the editor and the pty.
        terminal.read(&app, |view, ctx| {
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "echo QWERTY");
        });
        let written = String::from_utf8_lossy(&pty_writes.borrow().concat()).into_owned();
        assert_eq!(written, "", "nothing of the line reaches the pty");
    })
}

/// 1d (iii): an agent write after a silent release (a give-up) schedules the hand-over for the
/// next turn; with no remote prompt the held text goes back to the editor, unexecuted, guarded,
/// and nothing of it reaches the pty.
#[test]
fn an_agent_write_after_a_silent_release_schedules_the_handover() {
    App::test((), |mut app| async move {
        let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(
            DestinationMode::Table,
        );
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::workspace::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            submit_command(view, "ssh host", ctx);
            view.typed_characters_on_terminal("echo QW", ctx);
            assert!(view.ssh_typeahead_holding);
            view.ssh_connect_gate = None;
            view.note_ssh_gate_released(crate::terminal::input_queue::SshRelease::GiveUpCeiling);
            assert!(view.ssh_handover_after_release);
            view.write_agent_bytes_to_pty(b"y\r".to_vec(), &AIAgentPtyWriteMode::Raw, ctx);
        });
        for _ in 0..16 {
            futures_lite::future::yield_now().await;
        }
        terminal.read(&app, |view, ctx| {
            assert!(!view.ssh_typeahead_holding);
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "echo QW");
            assert!(view.input.as_ref(ctx).ssh_handover_enter_guard_armed());
        });
        let written = String::from_utf8_lossy(&pty_writes.borrow().concat()).into_owned();
        assert!(!written.contains("echo QW"), "{written:?}");
    })
}

/// #1575, redone by #1611/#1612/#1621: the inner ssh takes the tty while the hand-over drain is
/// yielded after the nested `ssh other` Enter. The input delivered next (`echo B`) must not split
/// from the held line: it joins the nested connect's hold, and the ownership event hands it over
/// (deferred to the drain's next operation when the queue is busy), in typed order. The line reaches
/// the inner shell once and never the editor or a local run; what the inner shell prints decides
/// nothing. Legacy runs only to prove it does not panic.
#[test]
fn nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell() {
    for mode in [DestinationMode::Table, DestinationMode::Legacy] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            let executed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = executed.clone();
            app.update(|ctx| {
                ctx.subscribe_to_view(&terminal, move |_, event, _| {
                    if let Event::ExecuteCommand(e) = event {
                        sink.borrow_mut().push(e.command.clone());
                    }
                });
            });
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                submit_command(view, "ssh host", ctx);
                view.typed_characters_on_terminal("ssh other", ctx);
                view.keydown_on_terminal("\r", ctx);
                view.typed_characters_on_terminal("echo B", ctx);
                view.keydown_on_terminal("\r", ctx);
                if mode == DestinationMode::Table {
                    assert_eq!(view.ssh_held_count(), 4);
                }
                view.ssh_remote_prompt_seen = true;
                first_precmd_processed_for_test(view, ctx);
                release_ssh_gate_for_test(view);
                view.deliver_ssh_typeahead_if_ready("precmd", ctx);
            });
            // The inner ssh takes the tty while the drain is yielded after the `ssh other` Enter:
            // in a SECOND consecutive update (no await between), so the effects of the first one
            // (the Enter's ExecuteCommand, which arms the nested connect window) are applied
            // before the ownership event, as in production, and the spawned drain continuation
            // has not run yet.
            terminal.update(&mut app, |view, ctx| {
                // The nested ssh starts running (its preexec), then the latch's ownership event
                // for it. The inner shell's prompt is output like any other and decides nothing.
                view.model
                    .lock()
                    .simulate_long_running_block("ssh other", "");
                view.model.lock().process_bytes("inner$ ");
                view.model.lock().publish_key_facts();
                view.handle_terminal_wakeup(crate::terminal::event_listener::Wakeup::Output, ctx);
                ssh_tty_owned_for_test(view, ctx);
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            if mode != DestinationMode::Table {
                return;
            }
            terminal.read(&app, |view, ctx| {
                let writes = String::from_utf8_lossy(&pty_writes.borrow().concat()).into_owned();
                if writes.matches("echo B").count() != 1 {
                    // Tell a dropped line from one still queued (a stuck hand-over).
                    let queued: Vec<String> = view
                        .paste_queue
                        .queue
                        .peek()
                        .entries()
                        .filter_map(|entry| match &entry.kind {
                            input_queue::EntryKind::Text(text) => Some(text.clone()),
                            input_queue::EntryKind::Held { text, .. } => Some(text.clone()),
                            _ => None,
                        })
                        .collect();
                    let buffer = view.input.as_ref(ctx).buffer_text(ctx);
                    let still_queued = queued.iter().any(|text| text.contains("echo B"));
                    let in_editor = buffer.contains("echo B");
                    let ran_locally = executed.borrow().iter().any(|c| c.contains("echo B"));
                    let verdict = if still_queued {
                        "STILL QUEUED (the hand-over never ran after the nested window's \
                         ownership event)"
                    } else if in_editor || ran_locally {
                        "WENT TO THE WRONG DESTINATION (editor / local run)"
                    } else {
                        "DROPPED: not queued, not written, not in the editor (a held line must \
                         never be dropped on the way to the inner shell; the H4 row that once \
                         did, #1575, is gone with the screen latch, #1611/#1612/#1621)"
                    };
                    panic!(
                        "echo B was written {} time(s), expected 1: {verdict}; writes={writes:?} \
                         queued={queued:?} editor={buffer:?} executed={:?} holding={}",
                        writes.matches("echo B").count(),
                        executed.borrow(),
                        view.ssh_typeahead_holding
                    );
                }
                assert_eq!(writes.matches("echo B").count(), 1, "{writes:?}");
                let at = writes.find("echo B").unwrap_or(0);
                assert!(writes[at..].contains('\r'), "{writes:?}");
                assert!(
                    !view.input.as_ref(ctx).buffer_text(ctx).contains("echo B"),
                    "the editor took the line"
                );
                assert!(
                    !executed.borrow().iter().any(|c| c.contains("echo B")),
                    "{:?}",
                    executed.borrow()
                );
                assert!(!view.ssh_typeahead_holding);
            });
        })
    }
}

/// #1575 (review of bbfe42460): as above, with the inner shell's echo of the line on the grid.
/// The inner shell's prompt appears while the hand-over drain is yielded after the nested
/// `ssh other` Enter. The input delivered next (`echo B`) must not split from the held line: it
/// joins the hold, and the hand-over runs on the next turn, in typed order. The line reaches the
/// inner shell once and never the editor or a local run. Legacy runs only to prove it does not
/// panic.
#[test]
fn nested_prompt_then_the_inner_echo_still_delivers_the_line_to_the_inner_shell() {
    for mode in [DestinationMode::Table, DestinationMode::Legacy] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            let executed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = executed.clone();
            app.update(|ctx| {
                ctx.subscribe_to_view(&terminal, move |_, event, _| {
                    if let Event::ExecuteCommand(e) = event {
                        sink.borrow_mut().push(e.command.clone());
                    }
                });
            });
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                submit_command(view, "ssh host", ctx);
                view.typed_characters_on_terminal("ssh other", ctx);
                view.keydown_on_terminal("\r", ctx);
                view.typed_characters_on_terminal("echo B", ctx);
                view.keydown_on_terminal("\r", ctx);
                if mode == DestinationMode::Table {
                    assert_eq!(view.ssh_held_count(), 4);
                }
                view.ssh_remote_prompt_seen = true;
                first_precmd_processed_for_test(view, ctx);
                release_ssh_gate_for_test(view);
                view.deliver_ssh_typeahead_if_ready("precmd", ctx);
            });
            // As above, with the inner shell's echo of the line arriving after the ownership
            // event: output before or after it never holds the line again or delivers it twice.
            terminal.update(&mut app, |view, ctx| {
                view.model
                    .lock()
                    .simulate_long_running_block("ssh other", "");
                view.model.lock().process_bytes("inner$ ");
                view.model.lock().publish_key_facts();
                view.handle_terminal_wakeup(crate::terminal::event_listener::Wakeup::Output, ctx);
                ssh_tty_owned_for_test(view, ctx);
                view.model.lock().process_bytes("echo B");
                view.model.lock().publish_key_facts();
                view.handle_terminal_wakeup(crate::terminal::event_listener::Wakeup::Output, ctx);
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            if mode != DestinationMode::Table {
                return;
            }
            terminal.read(&app, |view, ctx| {
                let writes = String::from_utf8_lossy(&pty_writes.borrow().concat()).into_owned();
                if writes.matches("echo B").count() != 1 {
                    // Tell a dropped line from one still queued (a stuck hand-over).
                    let queued: Vec<String> = view
                        .paste_queue
                        .queue
                        .peek()
                        .entries()
                        .filter_map(|entry| match &entry.kind {
                            input_queue::EntryKind::Text(text) => Some(text.clone()),
                            input_queue::EntryKind::Held { text, .. } => Some(text.clone()),
                            _ => None,
                        })
                        .collect();
                    let buffer = view.input.as_ref(ctx).buffer_text(ctx);
                    let still_queued = queued.iter().any(|text| text.contains("echo B"));
                    let in_editor = buffer.contains("echo B");
                    let ran_locally = executed.borrow().iter().any(|c| c.contains("echo B"));
                    let verdict = if still_queued {
                        "STILL QUEUED (the hand-over never ran after the nested window's \
                         ownership event)"
                    } else if in_editor || ran_locally {
                        "WENT TO THE WRONG DESTINATION (editor / local run)"
                    } else {
                        "DROPPED: not queued, not written, not in the editor (a held line must \
                         never be dropped on the way to the inner shell; the H4 row that once \
                         did, #1575, is gone with the screen latch, #1611/#1612/#1621)"
                    };
                    panic!(
                        "echo B was written {} time(s), expected 1: {verdict}; writes={writes:?} \
                         queued={queued:?} editor={buffer:?} executed={:?} holding={}",
                        writes.matches("echo B").count(),
                        executed.borrow(),
                        view.ssh_typeahead_holding
                    );
                }
                assert_eq!(writes.matches("echo B").count(), 1, "{writes:?}");
                let at = writes.find("echo B").unwrap_or(0);
                assert!(writes[at..].contains('\r'), "{writes:?}");
                assert!(
                    !view.input.as_ref(ctx).buffer_text(ctx).contains("echo B"),
                    "the editor took the line"
                );
                assert!(
                    !executed.borrow().iter().any(|c| c.contains("echo B")),
                    "{:?}",
                    executed.borrow()
                );
                assert!(!view.ssh_typeahead_holding);
            });
        })
    }
}

/// #1578: the ssh gate vanished behind the view's back and an UNRELATED op waits in the mailbox.
/// The next held key must re-open a gate (not be offered to a gateless queue and delivered at
/// once, mid-connect).
#[test]
fn an_unrelated_deferred_op_does_not_make_a_missing_ssh_gate_look_alive() {
    use crate::terminal::input_queue::{Deferred, GateKind};
    for mode in [DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                submit_command(view, "ssh host", ctx);
                view.typed_characters_on_terminal("a", ctx);
                assert!(view.ssh_typeahead_holding, "{mode:?}");
                let key = view.ssh_iq.gate.expect("the hold opened a gate");
                {
                    let q = view.paste_queue.queue.clone();
                    let _host = paste_queue::PtyQueueHost::new(view, ctx);
                    q.lock().drop_gate(GateKind::SshConnect, key);
                }
                view.paste_queue.queue.defer(Deferred::FocusChanged);
                view.typed_characters_on_terminal("b", ctx);
                assert!(ssh_gate_open(view), "{mode:?}: a gate was re-opened");
                assert_eq!(view.ssh_held_count(), 1, "{mode:?}");
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
            });
            assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
        })
    }
}

/// #1576: handed-back ssh text is never discarded. While a command owns the pty it is kept on the
/// input and restored, guarded, when that block completes; an Enter on it is consumed, not run.
#[test]
fn hand_back_into_a_busy_pane_is_kept_and_restored_at_block_end() {
    use crate::terminal::event::{BlockCompletedEvent, BlockType, UserBlockCompleted};
    use crate::terminal::model::block::{BlockId, SerializedBlock};
    use crate::terminal::model::terminal_model::BlockIndex;
    use std::sync::Arc;
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let executed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = executed.clone();
            app.update(|ctx| {
                ctx.subscribe_to_view(&terminal, move |_, event, _| {
                    if let Event::ExecuteCommand(e) = event {
                        sink.borrow_mut().push(e.command.clone());
                    }
                });
            });
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model.lock().simulate_long_running_block("sleep 9", "");
                view.model
                    .lock()
                    .block_list()
                    .active_block()
                    .interactive_latch()
                    .force(true);
                assert!(view.is_long_running(), "{mode:?}");
                view.hand_back_unexecuted_ssh_text("echo hi".to_owned(), ctx);
                assert_eq!(
                    view.input.as_ref(ctx).handed_back_text_for_test(),
                    Some("echo hi"),
                    "{mode:?}"
                );
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
                // The block ends: the text comes back, guarded.
                view.input.update(ctx, |input, ctx| {
                    input.set_deferred_ops_latest_block_for_test(BlockId::new());
                    input.handle_block_completed_event(
                        BlockCompletedEvent {
                            block_latency_data: None,
                            block_type: BlockType::User(UserBlockCompleted::new_for_test(
                                BlockIndex::zero(),
                                Arc::new(SerializedBlock::new_for_test(
                                    b"sleep 9".to_vec(),
                                    vec![],
                                )),
                                "sleep 9".to_owned(),
                                "sleep 9".to_owned(),
                                String::new(),
                                String::new(),
                                false,
                                None,
                                0,
                                0,
                            )),
                            num_secrets_obfuscated: 0,
                            block_index: BlockIndex::zero(),
                            eviction_stamp: Default::default(),
                            block_id: BlockId::new(),
                            session_id: None,
                            restored_block_was_local: None,
                        },
                        ctx,
                    );
                });
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "echo hi",
                    "{mode:?}"
                );
                assert!(
                    view.input.as_ref(ctx).ssh_handover_enter_guard_armed(),
                    "{mode:?}"
                );
                editor_key(view, EditorAction::Enter, ctx);
            });
            terminal.update(&mut app, |view, ctx| {
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "echo hi",
                    "{mode:?}"
                );
            });
            assert!(
                !executed.borrow().iter().any(|c| c.contains("echo hi")),
                "{mode:?}: {:?}",
                executed.borrow()
            );
        })
    }
}

/// #1580: user bytes already decided for the pty but still in the queue (a drain stopped at a
/// foreign entry) go out BEFORE an agent write that follows them; the agent bytes are never held.
#[test]
fn an_agent_write_does_not_overtake_user_bytes_waiting_in_the_queue() {
    App::test((), |mut app| async move {
        let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(
            DestinationMode::Table,
        );
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let order: Rc<RefCell<Vec<(&'static str, Vec<u8>)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = order.clone();
        app.update(|ctx| {
            ctx.subscribe_to_view(&terminal, move |_, event, _| match event {
                Event::WriteBytesToPty { bytes } | Event::WriteHandoffUserBytesToPty { bytes } => {
                    sink.borrow_mut().push(("user", bytes.to_vec()));
                }
                Event::WriteAgentInputToPty { bytes, .. } => {
                    sink.borrow_mut().push(("agent", bytes.to_vec()));
                }
                _ => {}
            });
        });
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            view.model.lock().simulate_long_running_block("sleep 9", "");
            view.model
                .lock()
                .block_list()
                .active_block()
                .interactive_latch()
                .force(true);
            view.input.update(ctx, |input, ictx| {
                input.offer_pty_bytes_through_editor_host_for_test(b"u".to_vec(), ictx);
            });
            // The editor host stopped at the pty entry: it is still queued.
            assert_eq!(view.paste_queue.queue.peek().queued_len(), 1);
            view.write_agent_bytes_to_pty(b"a".to_vec(), &AIAgentPtyWriteMode::Raw, ctx);
        });
        let order = order.borrow();
        let user_at = order
            .iter()
            .position(|(who, b)| *who == "user" && b == b"u");
        let agent_at = order
            .iter()
            .position(|(who, b)| *who == "agent" && b == b"a");
        assert!(
            user_at.is_some() && agent_at.is_some() && user_at < agent_at,
            "user bytes first, then the agent's: {order:?}"
        );
    })
}

/// A finished user block, as the input's block-completed handler receives it (#1596 tests).
fn kept_text_block_completed_for_test() -> crate::terminal::event::BlockCompletedEvent {
    use crate::terminal::event::{BlockCompletedEvent, BlockType, UserBlockCompleted};
    use crate::terminal::model::block::{BlockId, SerializedBlock};
    use crate::terminal::model::terminal_model::BlockIndex;
    use std::sync::Arc;
    BlockCompletedEvent {
        block_latency_data: None,
        block_type: BlockType::User(UserBlockCompleted::new_for_test(
            BlockIndex::zero(),
            Arc::new(SerializedBlock::new_for_test(b"ssh host".to_vec(), vec![])),
            "ssh host".to_owned(),
            "ssh host".to_owned(),
            String::new(),
            String::new(),
            false,
            None,
            0,
            0,
        )),
        num_secrets_obfuscated: 0,
        block_index: BlockIndex::zero(),
        eviction_stamp: Default::default(),
        block_id: BlockId::new(),
        session_id: None,
        restored_block_was_local: None,
    }
}

/// The ssh block ends: the view drops the banner, the input runs its end-of-block hand-back.
fn end_kept_text_block_for_test(view: &mut TerminalView, ctx: &mut ViewContext<TerminalView>) {
    use crate::terminal::model::block::BlockId;
    view.ssh_kept_text_block_ended(ctx);
    view.input.update(ctx, |input, ctx| {
        input.set_deferred_ops_latest_block_for_test(BlockId::new());
        input.handle_block_completed_event(kept_text_block_completed_for_test(), ctx);
    });
}

const KEPT_TEXT: &str = "echo hi\nls";

/// A pane whose ssh block runs, with text the ssh hand-back kept for it (#1596).
fn kept_text_pane_for_test(view: &mut TerminalView, ctx: &mut ViewContext<TerminalView>) {
    register_local_session_for_ssh_test(view, ctx);
    view.model
        .lock()
        .simulate_long_running_block("ssh host", "");
    view.model
        .lock()
        .block_list()
        .active_block()
        .interactive_latch()
        .force(true);
    assert!(view.is_long_running());
    // The hold this text comes from was opened for this (the ssh) block.
    view.ssh_hold_block_id = Some(view.model.lock().active_block_id().clone());
    view.hand_back_unexecuted_ssh_text(KEPT_TEXT.to_owned(), ctx);
}

fn click_kept_text_banner(
    app: &mut App,
    terminal: &ViewHandle<TerminalView>,
    action: crate::terminal::view::ssh_kept_text_banner::SshKeptTextBannerAction,
) {
    use warpui::TypedActionView as _;
    let banner = terminal
        .read(&*app, |view, _| view.ssh_kept_text_banner())
        .expect("the kept-text banner is up");
    banner.update(app, |banner, ctx| banner.handle_action(&action, ctx));
}

/// #1596: text kept while the ssh block runs is shown in a banner over that block, whole, with
/// its char count; it is not in the editor and nothing is written.
#[test]
fn kept_ssh_text_is_shown_in_a_banner_over_the_running_block() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                kept_text_pane_for_test(view, ctx);
                let banner = view
                    .ssh_kept_text_banner()
                    .unwrap_or_else(|| panic!("{mode:?}: a banner is up"));
                let active = view.model.lock().active_block_id().clone();
                let banner = banner.as_ref(ctx);
                assert_eq!(banner.text(), KEPT_TEXT, "{mode:?}");
                assert_eq!(banner.block_id(), &active, "{mode:?}: over the ssh block");
                assert!(
                    banner
                        .title()
                        .contains("(10 chars; 1 line runs when typed)"),
                    "{mode:?}: {}",
                    banner.title()
                );
                assert_eq!(banner.note(), None, "{mode:?}");
                assert_eq!(
                    view.input.as_ref(ctx).handed_back_text_for_test(),
                    Some(KEPT_TEXT),
                    "{mode:?}: the input still keeps it"
                );
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
            });
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: {:?}",
                pty_writes.borrow()
            );
        })
    }
}

/// #1596: "Type into session" writes exactly the kept text, each line break as Enter, once; the
/// banner and the kept text are gone, so the block's end hands nothing back.
#[test]
fn type_into_session_writes_the_kept_text_once() {
    use crate::terminal::view::ssh_kept_text_banner::SshKeptTextBannerAction;
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| kept_text_pane_for_test(view, ctx));
            click_kept_text_banner(
                &mut app,
                &terminal,
                SshKeptTextBannerAction::TypeIntoSession,
            );
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert_eq!(
                pty_writes.borrow().concat(),
                b"echo hi\rls".to_vec(),
                "{mode:?}: {:?}",
                pty_writes.borrow()
            );
            terminal.update(&mut app, |view, ctx| {
                assert!(view.ssh_kept_text_banner().is_none(), "{mode:?}");
                assert_eq!(view.input.as_ref(ctx).handed_back_text_for_test(), None);
                // A second click (no banner) and the block's end deliver nothing again.
                view.type_ssh_kept_text_into_session(ctx);
                end_kept_text_block_for_test(view, ctx);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert_eq!(
                pty_writes.borrow().concat(),
                b"echo hi\rls".to_vec(),
                "{mode:?}: written once"
            );
        })
    }
}

/// #1596: "Discard" drops the kept text: nothing is written, the banner goes, and the block's end
/// hands nothing back.
#[test]
fn discard_drops_the_kept_text_and_writes_nothing() {
    use crate::terminal::view::ssh_kept_text_banner::SshKeptTextBannerAction;
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| kept_text_pane_for_test(view, ctx));
            click_kept_text_banner(&mut app, &terminal, SshKeptTextBannerAction::Discard);
            terminal.update(&mut app, |view, ctx| {
                assert!(view.ssh_kept_text_banner().is_none(), "{mode:?}");
                assert_eq!(view.input.as_ref(ctx).handed_back_text_for_test(), None);
                end_kept_text_block_for_test(view, ctx);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: {:?}",
                pty_writes.borrow()
            );
        })
    }
}

/// #1596: the block ends with the banner up: the banner goes and the existing end-of-block
/// hand-back puts the text into the editor, guarded, unchanged; a late "Type into session" finds
/// nothing, so nothing is delivered twice.
#[test]
fn block_end_with_the_banner_up_hands_the_text_back_once() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                kept_text_pane_for_test(view, ctx);
                assert!(view.ssh_kept_text_banner().is_some(), "{mode:?}");
                end_kept_text_block_for_test(view, ctx);
                assert!(view.ssh_kept_text_banner().is_none(), "{mode:?}");
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    KEPT_TEXT,
                    "{mode:?}"
                );
                assert!(
                    view.input.as_ref(ctx).ssh_handover_enter_guard_armed(),
                    "{mode:?}"
                );
                view.type_ssh_kept_text_into_session(ctx);
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    KEPT_TEXT,
                    "{mode:?}"
                );
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: {:?}",
                pty_writes.borrow()
            );
        })
    }
}

/// #1596 review B1: a second hand-back for the same block joins the kept text with a line break
/// nobody typed; "Type into session" would submit it, so it is refused: nothing is written, and at
/// the block's end the editor holds both texts.
#[test]
fn type_into_session_is_refused_for_text_joined_from_two_hand_backs() {
    use crate::terminal::view::ssh_kept_text_banner::{REFUSED_NOTE, SshKeptTextBannerAction};
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                kept_text_pane_for_test(view, ctx);
                view.hand_back_unexecuted_ssh_text("echo b".to_owned(), ctx);
                let banner = view.ssh_kept_text_banner().expect("one banner");
                assert!(banner.as_ref(ctx).merged(), "{mode:?}");
                assert_eq!(banner.as_ref(ctx).text(), "echo hi\nls\necho b", "{mode:?}");
            });
            click_kept_text_banner(
                &mut app,
                &terminal,
                SshKeptTextBannerAction::TypeIntoSession,
            );
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: {:?}",
                pty_writes.borrow()
            );
            terminal.update(&mut app, |view, ctx| {
                let banner = view.ssh_kept_text_banner().expect("still up");
                assert_eq!(banner.as_ref(ctx).note(), Some(REFUSED_NOTE), "{mode:?}");
                end_kept_text_block_for_test(view, ctx);
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "echo hi\nls\necho b",
                    "{mode:?}: both texts come back into the editor"
                );
            });
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: {:?}",
                pty_writes.borrow()
            );
        })
    }
}

/// #1596 review B2: while the session is still connecting (a nested ssh hold, a remote bootstrap)
/// the pty write would bypass the hold: "Type into session" is refused and the text stays kept.
#[test]
fn type_into_session_is_refused_while_the_session_is_connecting() {
    use crate::terminal::view::ssh_kept_text_banner::{REFUSED_NOTE, SshKeptTextBannerAction};
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                kept_text_pane_for_test(view, ctx);
                // A nested connect holds keys now.
                view.ssh_typeahead_holding = true;
            });
            click_kept_text_banner(
                &mut app,
                &terminal,
                SshKeptTextBannerAction::TypeIntoSession,
            );
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: {:?}",
                pty_writes.borrow()
            );
            terminal.update(&mut app, |view, ctx| {
                let banner = view.ssh_kept_text_banner().expect("still up");
                assert_eq!(banner.as_ref(ctx).note(), Some(REFUSED_NOTE), "{mode:?}");
                assert_eq!(
                    view.input.as_ref(ctx).handed_back_text_for_test(),
                    Some(KEPT_TEXT),
                    "{mode:?}: still kept"
                );
            });
        })
    }
}

/// #1596 review (4): a hand-back for a block other than the one the ssh hold was opened for (a
/// late one, now over a local command) shows no banner; the text is still kept for the block end.
#[test]
fn no_kept_text_banner_over_a_block_the_hold_was_not_for() {
    use crate::terminal::model::block::BlockId;
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model.lock().simulate_long_running_block("sleep 9", "");
                view.model
                    .lock()
                    .block_list()
                    .active_block()
                    .interactive_latch()
                    .force(true);
                view.ssh_hold_block_id = Some(BlockId::new());
                view.hand_back_unexecuted_ssh_text("echo hi".to_owned(), ctx);
                assert!(view.ssh_kept_text_banner().is_none(), "{mode:?}");
                assert_eq!(
                    view.input.as_ref(ctx).handed_back_text_for_test(),
                    Some("echo hi"),
                    "{mode:?}"
                );
            });
        })
    }
}

/// #1596 final check: a hold opened before preexec records the submit-time block, which can differ
/// from the block that runs ssh; preexec corrects it (`note_ssh_running_block`, as for the gate),
/// so the banner shows over the running block.
#[test]
fn preexec_moves_the_hold_block_to_the_running_ssh_block() {
    use crate::terminal::model::block::BlockId;
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                // The hold opened before preexec, on a block that is not the one running ssh.
                view.ssh_hold_block_id = Some(BlockId::new());
                view.ssh_typeahead_holding = true;
                view.model
                    .lock()
                    .simulate_long_running_block("ssh host", "");
                view.model
                    .lock()
                    .block_list()
                    .active_block()
                    .interactive_latch()
                    .force(true);
                let running = view.model.lock().active_block_id().clone();
                assert_ne!(view.ssh_hold_block_id.as_ref(), Some(&running));
                // Preexec of the ssh command.
                view.note_ssh_running_block(running.clone());
                assert_eq!(view.ssh_hold_block_id.as_ref(), Some(&running), "{mode:?}");
                view.ssh_typeahead_holding = false;
                view.hand_back_unexecuted_ssh_text(KEPT_TEXT.to_owned(), ctx);
                let banner = view
                    .ssh_kept_text_banner()
                    .unwrap_or_else(|| panic!("{mode:?}: the banner is up"));
                assert_eq!(banner.as_ref(ctx).block_id(), &running, "{mode:?}");
            });
        })
    }
}
