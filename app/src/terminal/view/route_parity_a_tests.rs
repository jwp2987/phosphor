//! Table-vs-legacy input-route parity (audit `docs/design/input-route-parity-audit.md`): #1852
//! (^Z / ^\ are not an interrupt of a held Enter), #1854 (the secret-read check at delivery),
//! #1855 (synced input is passive), #1856 (^C arms the ssh connect gate). Child of `view_test.rs`.
//! Every case runs in Legacy and Table.

use super::*;
use crate::terminal::input_queue::DestinationMode;
use crate::terminal::input_queue::{Origin, Stamp};
use crate::terminal::view::dest_table::Replayed;

/// #1852: with an Enter waiting (a refused Enter of an unready shell holds the gate, keys typed
/// behind it wait), ^Z and ^\ leave the gate and the keys behind it alone, as legacy does; only
/// ^C / ^D void the Enter.
#[test]
fn suspend_and_quit_do_not_void_a_waiting_enter_interrupt_does() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        for (key, voids) in [(0x1au8, false), (0x1c, false), (0x03, true)] {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
                let terminal = add_window_with_terminal(&mut app, None);
                let _pty_writes = collect_pty_writes(&mut app, &terminal);
                terminal.update(&mut app, |view, ctx| {
                    hold_after_deferred_enter(view);
                    view.write_user_bytes_to_pty(b"ls -l".to_vec(), ctx);
                    view.handle_typeahead_report_with_handoff(true, ctx);
                    assert!(
                        view.input.as_ref(ctx).unready_enter_gate_open(),
                        "{mode:?}: the Enter waits"
                    );
                    view.control_sequence_on_terminal(&[key], ctx);
                    assert_eq!(
                        !view.input.as_ref(ctx).unready_enter_gate_open(),
                        voids,
                        "{mode:?} key {key:#x}: the waiting Enter is void only for ^C"
                    );
                });
            });
        }
    }
}

/// #1854: a delivered key runs the secret-read check like a live one: with a secret read open
/// while the subshell setup is written, the setup is cancelled before the key is judged.
#[test]
fn a_delivered_key_cancels_a_written_setup_when_a_secret_is_read() {
    use crate::terminal::model::block::interactive_latch::SecretReadProbe;
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                let probe: SecretReadProbe = std::sync::Arc::new(|| true);
                view.subshell_setup_state = SubshellSetupState::Written(WrittenSubshellSetup {
                    trigger: SubshellPromptTrigger::RcFile {
                        shell_type: warp_terminal::shell::ShellType::Zsh,
                        uname: None,
                        disable_tmux: true,
                    },
                    shell_type: Some(warp_terminal::shell::ShellType::Zsh),
                    session_id: SessionId::from(1),
                    block_id: BlockId::from("secret-test".to_string()),
                    written_at: Instant::now(),
                    baseline: (0, 0, 0),
                    relay: false,
                    progress_seen: false,
                    marker_count_at_write: 0,
                    fresh_prompts_after_echo: 0,
                    last_prompt_position: None,
                    echo_end_position: None,
                    secret_probe: Some(probe),
                    generation: 1,
                    stage_two_pending: None,
                });
                match mode {
                    DestinationMode::Table => {
                        let _ = view.decide_delivery(
                            Replayed::Typed("pw"),
                            Origin::Held,
                            Stamp::default(),
                            ctx,
                        );
                    }
                    _ => {
                        view.write_user_bytes_to_pty(b"pw".to_vec(), ctx);
                    }
                }
                assert!(
                    !matches!(view.subshell_setup_state, SubshellSetupState::Written(_)),
                    "{mode:?}: the setup is cancelled once a secret is read"
                );
            });
        });
    }
}

/// #1855 / #1430: synced input is passive. The agent drives a full-screen program in this pane;
/// typing in the synced pane writes nothing to it and does not take the command over.
#[test]
fn synced_input_does_not_take_over_an_agent_driven_command() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _agent_view_guard = FeatureFlag::AgentView.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                {
                    let mut model = view.model.lock();
                    model.init_shell(InitShellValue {
                        pid: None,
                        session_id: 0.into(),
                        shell: "zsh".to_owned(),
                        ..Default::default()
                    });
                    model.bootstrapped(BootstrappedValue {
                        shell: "zsh".to_owned(),
                        ..Default::default()
                    });
                    model.simulate_long_running_block("less file", "");
                }
                let conversation_id =
                    view.agent_view_controller().update(ctx, |controller, ctx| {
                        controller
                            .try_enter_inline_agent_view(
                                None,
                                AgentViewEntryOrigin::LongRunningCommand,
                                ctx,
                            )
                            .expect("inline agent view should create a conversation")
                    });
                view.model
                    .lock()
                    .block_list_mut()
                    .active_block_mut()
                    .set_is_agent_tagged_in(true);
                let task_id = TaskId::new("synced-test-task".to_owned());
                view.model
                    .lock()
                    .block_list_mut()
                    .active_block_mut()
                    .set_agent_interaction_mode_for_agent_monitored_command(
                        &task_id,
                        conversation_id,
                    )
                    .expect("tagged-in command should transition to agent-monitored");
                view.model.lock().process_bytes("\x1b[?1049h");
                assert!(
                    view.model
                        .lock()
                        .block_list()
                        .active_block()
                        .is_agent_in_control(),
                    "{mode:?}: the agent drives the command"
                );
                pty_writes.borrow_mut().clear();
                view.write_to_pty_for_syncing_long_running_commands(b"q".to_vec(), ctx);
                assert!(
                    view.model
                        .lock()
                        .block_list()
                        .active_block()
                        .is_agent_in_control(),
                    "{mode:?}: synced input does not take the command over"
                );
            });
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: nothing reached the agent's program: {:?}",
                pty_writes.borrow()
            );
        });
    }
}

/// #1856 / #1001: ^C right after Enter on `ssh host`, before any edge armed the connect gate, is
/// SIGINT to the foreground group, not a raw byte. Legacy and Table.
#[test]
fn ctrl_c_before_the_connect_gate_is_armed_is_sigint() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_pty_writes(&mut app, &terminal);
            let sigints = Rc::new(RefCell::new(0usize));
            let counter = sigints.clone();
            app.update(|ctx| {
                ctx.subscribe_to_view(&terminal, move |_, event, _| {
                    if matches!(event, Event::SigintForegroundToPty) {
                        *counter.borrow_mut() += 1;
                    }
                });
            });
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                view.model
                    .lock()
                    .simulate_long_running_block("ssh host", "");
                view.warpify_state
                    .set_pending_ssh_host("ssh host".to_owned(), Some("host".to_owned()));
                view.keydown_on_terminal("\u{3}", ctx);
            });
            assert_eq!(*sigints.borrow(), 1, "{mode:?}: ^C is SIGINT to ssh");
            assert!(
                !pty_writes.borrow().iter().any(|w| w.as_slice() == b"\x03"),
                "{mode:?}: no raw ^C byte: {:?}",
                pty_writes.borrow()
            );
        });
    }
}
