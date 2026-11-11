//! #1658: after an Enter gate gives up and keeps the line typed before the Enter, the hand-off is
//! still pty-owned until the shell decides; every later key follows that line into the editor
//! (never to the pty ahead of it), and events (not age) return keys to the pty. Child of
//! `view_test.rs`.

use super::*;

/// A pty-owned hand-off, then a give-up that keeps the line typed before the Enter: `echo hi` is
/// the line of a deferred Enter whose timer gave up (`give_up_deferred_enter_keys`), with nothing
/// typed after it.
///
/// Until the owner's decision of 2026-11-09 (reversing #1661) this helper used the young-hold /
/// CLI-submit give-up, which kept the line in front of the Enter and DISCARDED `ls` typed after
/// it. That give-up now writes everything it held to the pty (`give_up_to_pty_tests.rs` covers
/// it) and keeps no line, so the kept line here comes from the give-up that still keeps one.
fn give_up_keeping_a_line(view: &mut TerminalView, ctx: &mut ViewContext<TerminalView>) {
    view.model
        .lock()
        .typeahead_handoff_mut()
        .on_command_written();
    assert!(view.model.lock().typeahead_handoff().is_pty_owned());
    view.input
        .update(ctx, |input, ctx| input.system_insert("echo hi", ctx));
    view.give_up_deferred_enter_keys(Vec::new(), "test", ctx);
}

/// A kept line plus later keys and a hand-off that has not yet decided (the report is late): the
/// later keys land after the line in the editor, in order, and nothing reaches the pty. Both
/// modes, printable keys and control keys.
#[test]
fn keys_typed_after_a_kept_line_follow_it_into_the_editor_while_the_report_is_late() {
    App::test((), move |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = collect_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            give_up_keeping_a_line(view, ctx);
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "echo hi");
            let handoff = view.model.lock();
            assert!(handoff.typeahead_handoff().keys_follow_editor_active());
            assert!(handoff.typeahead_handoff().is_pty_owned());
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
                "the later keys follow the line in order"
            );
        });
        assert!(pty_writes.borrow().is_empty(), "nothing reached the pty");
    })
}

/// The events return keys to the pty: a Preexec (a program starts: vim, a password prompt) and
/// the editor emptying. Both modes.
#[test]
fn a_program_or_an_empty_editor_returns_keys_to_the_pty() {
    App::test((), move |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = collect_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            // Legacy writes typed text to the pty only for a bootstrapped session (the stalled
            // setup no longer opens that door by age, #1711); the Table mode decides from its
            // facts. A key reaching the program needs the session, as in a real pane.
            register_local_session_for_ssh_test(view, ctx);
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
            "a key after a Preexec reaches the program"
        );
    });
    App::test((), move |mut app| async move {
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
                    .keys_follow_editor_active()
            );
            assert!(view.model.lock().typeahead_handoff().routes_keys_to_pty());
        });
    })
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
