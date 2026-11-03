//! Group 4: a setup write must not land on a line that holds delivered typed text (#1562), and a
//! control key in a focus gap goes where the typed keys went (#1552). Child of `view_test.rs`
//! (private helpers come in through `super`).

use super::*;
use crate::terminal::input_queue::DestinationMode;
use warp_terminal::shell::ShellType;

/// Records every event that writes to the pty, by name.
fn record_setup_and_pty_writes(
    app: &mut App,
    terminal: &ViewHandle<TerminalView>,
) -> Rc<RefCell<Vec<&'static str>>> {
    let seen: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    app.update(|ctx| {
        ctx.subscribe_to_view(terminal, move |_, event, _| match event {
            Event::WriteLineClearSetupToPty { .. } => sink.borrow_mut().push("setup"),
            Event::WriteBytesToPty { .. } => sink.borrow_mut().push("bytes"),
            Event::WriteHandoffUserBytesToPty { .. } => sink.borrow_mut().push("handoff"),
            Event::WriteAgentInputToPty { .. } => sink.borrow_mut().push("agent"),
            _ => {}
        });
    });
    seen
}

/// The pty owns input and the user's delivered keys `ec` sit in the shell's line. The command
/// that was written is old enough that the keys are not a young line (#933): a young Enter is held
/// for the prompt instead of reaching the shell's line, which would not end it.
fn deliver_keys_to_an_unmanaged_prompt(view: &TerminalView) {
    let mut model = view.model.lock();
    let mut handoff = model.typeahead_handoff_mut();
    handoff.on_command_written();
    handoff.age_last_activity_for_test();
    handoff.on_user_bytes(b"ec");
    handoff.on_user_write_flushed();
}

/// The user's `bytes` (an Enter, ^U) reach the shell's line and end it. Aged first like the
/// keys: an Enter inside the young window is held for the prompt (#933), not written.
fn end_the_line(view: &TerminalView, bytes: &[u8]) {
    let mut model = view.model.lock();
    let mut handoff = model.typeahead_handoff_mut();
    handoff.age_last_activity_for_test();
    handoff.on_user_bytes(bytes);
    handoff.on_user_write_flushed();
}

/// #1562: the ssh warpify write is deferred while the line holds delivered keys (nothing is
/// written, nothing is killed), and goes out exactly once when the user's Enter ends the line.
#[test]
fn ssh_warpify_waits_for_a_line_holding_delivered_keys() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        // The warpify write loads the bundled probe script from the asset cache.
        App::test(crate::Assets, move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_setup_and_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                deliver_keys_to_an_unmanaged_prompt(view);
                assert!(view.setup_write_must_wait_for_the_line());
                view.warpify_ssh_session(ctx);
            });
            terminal.read(&app, |view, _| {
                assert!(view.ssh_warpify_deferred.is_some());
            });
            assert!(writes.borrow().is_empty(), "{:?}", writes.borrow());

            // A second trigger while waiting is still one request, still no write.
            terminal.update(&mut app, |view, ctx| {
                view.handle_typeahead_handoff_changed(ctx);
            });
            assert!(writes.borrow().is_empty(), "{:?}", writes.borrow());

            terminal.update(&mut app, |view, ctx| {
                end_the_line(view, b"\r");
                view.handle_typeahead_handoff_changed(ctx);
            });
            assert_eq!(*writes.borrow(), vec!["setup"], "written exactly once");
            terminal.update(&mut app, |view, ctx| {
                assert!(view.ssh_warpify_deferred.is_none());
                view.handle_typeahead_handoff_changed(ctx);
            });
            assert_eq!(writes.borrow().len(), 1, "never written twice");
        });
    }
}

/// #1562: a clean line writes at once, with the line clear (it kills an empty line).
#[test]
fn ssh_warpify_on_a_clean_line_writes_at_once() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        // The warpify write loads the bundled probe script from the asset cache.
        App::test(crate::Assets, move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let writes = record_setup_and_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model
                    .lock()
                    .typeahead_handoff_mut()
                    .on_command_written();
                view.warpify_ssh_session(ctx);
            });
            assert_eq!(*writes.borrow(), vec!["setup"]);
        });
    }
}

/// #1562: the subshell bootstrap waits for the line to end, then goes out once; ^U ends the line
/// too.
#[test]
fn subshell_bootstrap_waits_for_a_line_holding_delivered_keys() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        for ender in [&b"\r"[..], &b"\x15"[..]] {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
                let terminal = add_window_with_terminal(&mut app, None);
                let writes = record_setup_and_pty_writes(&mut app, &terminal);
                terminal.update(&mut app, |view, ctx| {
                    register_local_session_for_ssh_test(view, ctx);
                    deliver_keys_to_an_unmanaged_prompt(view);
                    view.trigger_subshell_bootstrap(Some(ShellType::Bash), false, ctx);
                });
                terminal.read(&app, |view, _| {
                    assert!(view.subshell_setup_deferred.is_some());
                });
                assert!(writes.borrow().is_empty(), "{:?}", writes.borrow());

                terminal.update(&mut app, |view, ctx| {
                    end_the_line(view, ender);
                    view.handle_typeahead_handoff_changed(ctx);
                });
                terminal.read(&app, |view, _| {
                    assert!(view.subshell_setup_deferred.is_none());
                });
                assert!(!writes.borrow().is_empty(), "the setup went out");
            });
        }
    }
}

/// An Enter key press delivered to the pane's key sink with `focus_chain` focused.
fn pane_enter_with_focus(
    view: &mut TerminalView,
    focus_chain: Vec<EntityId>,
    ctx: &mut ViewContext<TerminalView>,
) -> PaneKeyOutcome {
    let key = PaneKey {
        seq: 0,
        kind: PaneKeyKind::KeyDown {
            keystroke: warpui::keymap::Keystroke::parse("enter").expect("enter parses"),
            chars: "\r".to_owned(),
            is_composing: false,
            will_type: false,
            key_without_modifiers: None,
        },
    };
    view.on_pane_key_event(
        &key,
        &PaneKeyContext {
            focus_chain,
            leaf_takes_typed_text: false,
        },
        ctx,
    )
}

fn flat_bytes(writes: &Rc<RefCell<Vec<Vec<u8>>>>) -> Vec<u8> {
    writes.borrow().iter().flatten().copied().collect()
}

/// #1552: a running command, focus nowhere in the pane. The Enter is the pane's (not left to the
/// input's binding on an empty editor) and goes to the pty with the keys that preceded it.
#[test]
fn enter_in_a_focus_gap_follows_the_typed_keys_to_the_running_command() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            // Focus is on a REAL settled card: the sink only closes the control-key gap for a
            // settled AI target (or the ssh hold); a pane child with its own bindings keeps its keys.
            let (card, _ai_block, action_id, conversation_id) =
                setup_mcp_tool_card(&mut app, &terminal);
            finish_action_for_test(&mut app, &terminal, &action_id, conversation_id);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model.lock().simulate_long_running_block("cat", "");
                // Typed keys in the gap go to the pty (#1397) ...
                assert_eq!(pane_typed_in_gap(view, "a", ctx), PaneKeyOutcome::Consumed);
            });
            terminal.update(&mut app, |_, ctx| ctx.focus(&card));
            let outcome = terminal.update(&mut app, |view, ctx| {
                let chain = vec![view.view_id, card.id()];
                pane_enter_with_focus(view, chain, ctx)
            });
            assert_eq!(outcome, PaneKeyOutcome::Consumed, "the Enter is the pane's");
            assert_eq!(flat_bytes(&pty_writes), b"a\r".to_vec());
            terminal.read(&app, |view, ctx| {
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "");
            });
        });
    }
}

fn pane_typed_in_gap(
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
            focus_chain: Vec::new(),
            leaf_takes_typed_text: false,
        },
        ctx,
    )
}

/// #1552: at an idle prompt with an empty editor an Enter in a focus gap writes nothing and runs
/// nothing; it is dropped (and focus goes back to the input).
#[test]
fn enter_in_a_focus_gap_at_an_idle_prompt_writes_and_runs_nothing() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            let (card, _ai_block, action_id, conversation_id) =
                setup_mcp_tool_card(&mut app, &terminal);
            finish_action_for_test(&mut app, &terminal, &action_id, conversation_id);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
            });
            terminal.update(&mut app, |_, ctx| ctx.focus(&card));
            terminal.update(&mut app, |view, ctx| {
                let chain = vec![view.view_id, card.id()];
                pane_enter_with_focus(view, chain, ctx);
            });
            assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
            terminal.read(&app, |view, ctx| {
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "");
            });
        });
    }
}

/// #1552: a PENDING approval card binds its own Enter (approve): the sink leaves the key to the
/// card, writes nothing and does not move focus to the input.
#[test]
fn enter_on_a_pending_approval_card_stays_with_the_card() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(crate::notebooks::editor::keys::NotebookKeybindings::new);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        // A card is the PENDING approval card only while its action is blocked in the action
        // model on the terminal's active conversation (as in `typing_while_an_approval_card_is_
        // focused_*`); a card without a queued action counts as settled, and the sink then
        // rightly takes its Enter (#1552).
        let (card, action_id, conversation_id, _) =
            setup_requested_command_card(&mut app, &terminal);
        let terminal_view_id = terminal.read(&app, |view, _| view.view_id);
        BlocklistAIHistoryModel::handle(&app).update(&mut app, |history, ctx| {
            history.set_active_conversation_id(conversation_id, terminal_view_id, ctx);
        });
        let action_model = terminal.read(&app, |view, _| view.ai_action_model.clone());
        action_model.update(&mut app, |action_model, ctx| {
            action_model.queue_action_for_test(
                crate::ai::agent::AIAgentAction {
                    id: action_id.clone(),
                    task_id: TaskId::new("task".to_owned()),
                    action: crate::ai::agent::AIAgentActionType::RequestCommandOutput {
                        command: "sleep 100000".to_owned(),
                        is_read_only: None,
                        is_risky: None,
                        rationale: None,
                        uses_pager: None,
                        wait_until_completion: true,
                        citations: vec![],
                    },
                    requires_result: false,
                },
                conversation_id,
                ctx,
            );
        });
        for _ in 0..64 {
            let is_blocked = action_model.read(&app, |action_model, _| {
                action_model
                    .get_action_status(&action_id)
                    .is_some_and(|status| status.is_blocked())
            });
            if is_blocked {
                break;
            }
            futures_lite::future::yield_now().await;
        }
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
        });
        terminal.update(&mut app, |_, ctx| ctx.focus(&card));
        terminal.read(&app, |view, ctx| {
            assert_eq!(
                view.approval_card_view_id(ctx),
                Some(card.id()),
                "precondition: the card is the pending approval card"
            );
        });
        let outcome = terminal.update(&mut app, |view, ctx| {
            let chain = vec![view.view_id, card.id()];
            pane_enter_with_focus(view, chain, ctx)
        });
        assert_eq!(outcome, PaneKeyOutcome::NotMine);
        assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
        terminal.read(&app, |view, ctx| {
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "");
        });
    });
}

/// #1552: an unknown pane child with focus (not a settled AI target) keeps its control keys.
#[test]
fn enter_on_an_unknown_pane_child_stays_with_the_child() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        let outcome = terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            let chain = vec![view.view_id, EntityId::new()];
            pane_enter_with_focus(view, chain, ctx)
        });
        assert_eq!(outcome, PaneKeyOutcome::NotMine);
        assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
    });
}

/// A typed key delivered to the pane's key sink with `focus_chain` focused.
fn pane_typed_with_focus(
    view: &mut TerminalView,
    chars: &str,
    focus_chain: Vec<EntityId>,
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
            focus_chain,
            leaf_takes_typed_text: false,
        },
        ctx,
    )
}

/// #1552 (a), the Git Bash repro (r18co): a long command runs and focus is left on the input view
/// itself, or on a view inside the input, but not on its editor. The typed keys of that gap went
/// to the pty and the Enter to the empty editor. Now the Enter follows the typed line through
/// the terminal's routing: the pty gets `a` then `\r`, in order, and the editor stays empty.
#[test]
fn enter_with_focus_on_the_input_view_follows_the_typed_line_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        for inside_the_input in [false, true] {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let terminal = add_window_with_terminal(&mut app, None);
                let pty_writes = record_all_pty_writes(&mut app, &terminal);
                let chain = terminal.read(&app, |view, _| {
                    let mut chain = vec![view.view_id, view.input.id()];
                    if inside_the_input {
                        chain.push(EntityId::new());
                    }
                    chain
                });
                let (typed, enter) = terminal.update(&mut app, |view, ctx| {
                    register_local_session_for_ssh_test(view, ctx);
                    view.model.lock().simulate_long_running_block("cat", "");
                    let typed = pane_typed_with_focus(view, "a", chain.clone(), ctx);
                    let enter = pane_enter_with_focus(view, chain.clone(), ctx);
                    (typed, enter)
                });
                assert_eq!(
                    typed,
                    PaneKeyOutcome::Consumed,
                    "{mode:?} {inside_the_input}"
                );
                assert_eq!(
                    enter,
                    PaneKeyOutcome::Consumed,
                    "{mode:?} {inside_the_input}"
                );
                assert_eq!(
                    flat_bytes(&pty_writes),
                    b"a\r".to_vec(),
                    "{mode:?} {inside_the_input}"
                );
                terminal.read(&app, |view, ctx| {
                    assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "");
                });
            });
        }
    }
}

/// #1552 (a): with no command taking keys (an idle prompt), the line typed from the input view is
/// the editor's, and so is its Enter: the sink leaves it to the input's own handling and writes
/// nothing to the pty.
#[test]
fn enter_with_focus_on_the_input_view_at_an_idle_prompt_stays_with_the_input() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            let outcome = terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                let chain = vec![view.view_id, view.input.id()];
                pane_enter_with_focus(view, chain, ctx)
            });
            assert_eq!(outcome, PaneKeyOutcome::NotMine, "{mode:?}");
            assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
        });
    }
}

/// #1552 (a) / Fable B1: a pane child that is not inside the input (a card, any unknown child)
/// keeps its Enter while a command runs, in both destination modes: only the input view's gap
/// follows the line.
#[test]
fn enter_on_a_pane_child_outside_the_input_keeps_its_enter_while_a_command_runs() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            let outcome = terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model.lock().simulate_long_running_block("cat", "");
                let chain = vec![view.view_id, EntityId::new()];
                pane_enter_with_focus(view, chain, ctx)
            });
            assert_eq!(outcome, PaneKeyOutcome::NotMine, "{mode:?}");
            assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
        });
    }
}

/// #1552 (a), agent view: while the agent drives its command the user's keys are the agent's
/// input (#1179), never the command's. The Enter typed from the input view must not become a
/// byte for that pty either: it stays with the input, and nothing reaches the pty.
#[test]
fn enter_with_focus_on_the_input_view_during_an_agent_command_never_reaches_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let (pty_writes, _conversation_id) = setup_running_agent_command(&mut app, &terminal);
            let (typed, enter) = terminal.update(&mut app, |view, ctx| {
                assert!(view.agent_owns_command_pty());
                let chain = vec![view.view_id, view.input.id()];
                let typed = pane_typed_with_focus(view, "a", chain.clone(), ctx);
                let enter = pane_enter_with_focus(view, chain, ctx);
                (typed, enter)
            });
            assert_eq!(typed, PaneKeyOutcome::Consumed, "{mode:?}");
            assert_eq!(enter, PaneKeyOutcome::NotMine, "{mode:?}");
            assert!(
                pty_writes.borrow().is_empty(),
                "no user byte may reach the agent's command: {:?}",
                pty_writes.borrow()
            );
            terminal.read(&app, |view, ctx| {
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "a");
            });
        });
    }
}

/// #1552 (b): the prompt after a long-running command consumes the hand-back mark on its Precmd
/// (an event, never a timer) and does not move focus that is not in a gap: the terminal view
/// itself keeps it. The gap decision itself is `pane_key::hands_back_focus_at_prompt`.
#[test]
fn the_prompt_after_a_long_running_command_hands_focus_back_only_from_a_gap() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            ctx.focus_self();
        });
        terminal.update(&mut app, |view, ctx| {
            view.focus_hand_back_at_prompt = true;
            assert!(
                !view.hand_back_focus_at_prompt(ctx),
                "the terminal view is not a gap"
            );
            view.handle_terminal_event(&ModelEvent::Handler(AnsiHandlerEvent::Precmd), ctx);
            assert!(
                !view.focus_hand_back_at_prompt,
                "the Precmd consumes the mark"
            );
        });
    });
}
