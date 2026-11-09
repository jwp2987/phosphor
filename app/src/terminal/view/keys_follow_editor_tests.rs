//! #1658: after an Enter gate gives up and keeps the line typed before the Enter, the hand-off is
//! still pty-owned until the shell decides; every later key follows that line into the editor
//! (never to the pty ahead of it), and events (not age) return keys to the pty. Child of
//! `view_test.rs`.

use super::*;
use crate::terminal::input_queue::DestinationMode;
use crate::terminal::input_queue::{EntryId, EntryKind, GateKind, InputEntry, Origin, Stamp};

fn entry(id: u64, kind: EntryKind) -> InputEntry {
    InputEntry {
        id: EntryId(id),
        kind,
        focus_epoch: 0,
        arrived_at: std::time::Instant::now(),
        origin: Origin::Keyboard,
        stamp: Stamp::default(),
    }
}

/// The pty-owned hand-off a young-hold give-up leaves, then the give-up itself keeping `echo hi`
/// (typed before the Enter) with `ls` typed after it.
fn give_up_keeping_a_line(view: &mut TerminalView, ctx: &mut ViewContext<TerminalView>) {
    view.model
        .lock()
        .typeahead_handoff_mut()
        .on_command_written();
    assert!(view.model.lock().typeahead_handoff().is_pty_owned());
    let entries = vec![
        entry(1, EntryKind::Text("echo hi".to_owned())),
        entry(
            2,
            EntryKind::PtyBytes {
                bytes: b"\rls".to_vec(),
                takes_over_agent_control: false,
            },
        ),
    ];
    view.finish_discard_gate_keys_after_enter(entries, GateKind::CliSubmit, 1, "test", true, ctx);
}

/// A kept line plus later keys and a hand-off that has not yet decided (the report is late): the
/// later keys land after the line in the editor, in order, and nothing reaches the pty. Both
/// modes, printable keys and control keys.
#[test]
fn keys_typed_after_a_kept_line_follow_it_into_the_editor_while_the_report_is_late() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                give_up_keeping_a_line(view, ctx);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "echo hi");
                let handoff = view.model.lock();
                assert!(
                    handoff.typeahead_handoff().keys_follow_editor_active(),
                    "{mode:?}"
                );
                assert!(handoff.typeahead_handoff().is_pty_owned(), "{mode:?}");
            });
            terminal.update(&mut app, |view, ctx| {
                view.typed_characters_on_terminal("!", ctx);
                view.typed_characters_on_terminal("?", ctx);
                view.control_sequence_on_terminal(&[0x7f], ctx);
            });
            terminal.read(&app, |view, ctx| {
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "echo hi!",
                    "{mode:?}: the later keys follow the line in order"
                );
            });
            assert!(
                pty_writes.borrow().is_empty(),
                "{mode:?}: nothing reached the pty"
            );
        })
    }
}

/// The events return keys to the pty: a Preexec (a program starts: vim, a password prompt) and
/// the editor emptying. Both modes.
#[test]
fn a_program_or_an_empty_editor_returns_keys_to_the_pty() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                give_up_keeping_a_line(view, ctx);
                // A program starts and owns the tty (a Preexec): keys are its.
                view.model.lock().typeahead_handoff_mut().on_preexec(false);
                view.redetermine_terminal_focus(ctx);
                assert!(
                    !view
                        .model
                        .lock()
                        .typeahead_handoff()
                        .keys_follow_editor_active()
                );
                view.typed_characters_on_terminal("q", ctx);
            });
            assert_eq!(
                pty_writes.borrow().concat(),
                b"q".to_vec(),
                "{mode:?}: a key after a Preexec reaches the program"
            );
        });
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
                give_up_keeping_a_line(view, ctx);
                view.input.update(ctx, |input, ctx| {
                    input.clear_buffer_and_reset_undo_stack(ctx)
                });
                view.end_keys_follow_editor("editor_empty", ctx);
                assert!(
                    !view
                        .model
                        .lock()
                        .typeahead_handoff()
                        .keys_follow_editor_active(),
                    "{mode:?}"
                );
                assert!(view.model.lock().typeahead_handoff().routes_keys_to_pty());
            });
        })
    }
}

/// A program owning the tty (the block became interactive: vim, a password prompt, ssh) and the
/// alt screen both return keys to the pty, whatever the editor still holds.
#[test]
fn an_interactive_program_and_the_alt_screen_end_keys_follow_editor() {
    for alt_screen in [false, true] {
        App::test((), move |mut app| async move {
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
                give_up_keeping_a_line(view, ctx);
                assert!(
                    view.model
                        .lock()
                        .typeahead_handoff()
                        .keys_follow_editor_active()
                );
                if alt_screen {
                    view.handle_terminal_event(
                        &ModelEvent::TerminalModeSwapped(
                            crate::terminal::event::TerminalMode::AltScreen,
                        ),
                        ctx,
                    );
                } else {
                    let block_id = view.model.lock().active_block_id().clone();
                    view.handle_terminal_event(
                        &ModelEvent::CommandBecameInteractive { block_id },
                        ctx,
                    );
                }
                assert!(
                    !view
                        .model
                        .lock()
                        .typeahead_handoff()
                        .keys_follow_editor_active(),
                    "alt_screen={alt_screen}"
                );
            });
        })
    }
}

/// Ctrl-C is a signal: it ends the fact first and then goes where it always did.
#[test]
fn a_signal_key_ends_keys_follow_editor() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            give_up_keeping_a_line(view, ctx);
            view.keydown_on_terminal("\u{3}", ctx);
            assert!(
                !view
                    .model
                    .lock()
                    .typeahead_handoff()
                    .keys_follow_editor_active()
            );
        });
    })
}
