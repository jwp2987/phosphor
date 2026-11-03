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

/// The pty owns input and the user's delivered keys `ec` sit in the shell's line.
fn deliver_keys_to_an_unmanaged_prompt(view: &TerminalView) {
    let mut model = view.model.lock();
    let mut handoff = model.typeahead_handoff_mut();
    handoff.on_command_written();
    handoff.on_user_bytes(b"ec");
    handoff.on_user_write_flushed();
}

fn end_the_line(view: &TerminalView, bytes: &[u8]) {
    let mut model = view.model.lock();
    let mut handoff = model.typeahead_handoff_mut();
    handoff.on_user_bytes(bytes);
    handoff.on_user_write_flushed();
}

/// #1562: the ssh warpify write is deferred while the line holds delivered keys (nothing is
/// written, nothing is killed), and goes out exactly once when the user's Enter ends the line.
#[test]
fn ssh_warpify_waits_for_a_line_holding_delivered_keys() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
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
        App::test((), move |mut app| async move {
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
