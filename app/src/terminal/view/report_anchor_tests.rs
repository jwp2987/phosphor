//! #2012 (round 9 Windows, PowerShell pane, `giveup_keys_reach_shell` 2/20): the editor held `Ad`
//! when PowerShell sent a Preexec for an empty typed-ahead line. The pty took input, `d` and `-`
//! were written to it, focus came back, the editor took `Co`, and the shell's report `d-` was put
//! at the FRONT of the buffer: `d-AdCo`, submitted as `d-AdContent ...`. The report now goes
//! behind the text the editor held when the shell started that command: `Add-Co`. Child of
//! `view_test.rs`.

use super::*;
use crate::terminal::input_queue::DestinationMode;

/// The exact sequence of the round 9 log up to the report: the editor owns input and holds `Ad`;
/// the shell starts a command by itself (`on_preexec(true)`: the pty takes input, model thread);
/// with `view_handles_preexec` the Preexec event reaches the view (focus would move to the
/// terminal here); `d` and `-` are typed on the terminal and reach the pty; the command's prompt and the
/// line editor's ready signal decide `SendRequest`; the editor takes `C` and `o`.
fn type_a_line_across_a_shell_started_command(
    view: &mut TerminalView,
    view_handles_preexec: bool,
    ctx: &mut ViewContext<TerminalView>,
) {
    // Legacy writes typed text to the pty only for a session that is bootstrapped or pending.
    register_local_session_for_ssh_test(view, ctx);
    view.input
        .update(ctx, |input, ctx| input.system_insert("Ad", ctx));
    assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "Ad");
    assert!(
        view.model.lock().typeahead_handoff_mut().on_preexec(true),
        "the shell-started command takes input from the editor"
    );
    if view_handles_preexec {
        view.handle_terminal_event(&ModelEvent::Handler(AnsiHandlerEvent::Preexec), ctx);
    }
    view.typed_characters_on_terminal("d", ctx);
    view.typed_characters_on_terminal("-", ctx);
    {
        let mut model = view.model.lock();
        let mut handoff = model.typeahead_handoff_mut();
        // The two writes are flushed (a no-op when the controller already did).
        handoff.on_user_write_flushed();
        handoff.on_user_write_flushed();
        handoff.on_precmd();
        assert_eq!(
            handoff.on_line_editor_active(true, true),
            ActiveDecision::SendRequest,
            "the keys are in the shell's line: its report is asked for"
        );
    }
    view.handle_typeahead_handoff_changed(ctx);
    view.input
        .update(ctx, |input, ctx| input.system_insert("Co", ctx));
    assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "AdCo");
}

/// The round 9 sequence, both destination modes (Legacy is the kill switch): the report `d-` goes
/// behind `Ad`, in front of `Co`. Typed `Add-Co`, the buffer is `Add-Co`, the cursor is at its
/// end, and the pty received exactly `d-`.
#[test]
fn a_report_of_keys_typed_after_the_editors_text_goes_behind_that_text() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = collect_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                type_a_line_across_a_shell_started_command(view, true, ctx);
                assert!(
                    view.input.as_ref(ctx).has_handoff_report_anchor(),
                    "{mode:?}: the anchor waits for the report"
                );
                store_shell_report(view, "d-");
                view.handle_typeahead_report_with_handoff(true, ctx);
            });
            terminal.read(&app, |view, ctx| {
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "Add-Co",
                    "{mode:?}: the keys are in typed order"
                );
                assert_eq!(
                    view.model.lock().typeahead_handoff().state(),
                    &HandoffState::EditorOwned,
                    "{mode:?}"
                );
            });
            assert_eq!(
                String::from_utf8_lossy(&pty_writes.borrow().concat()),
                "d-",
                "{mode:?}: the two keys typed while the command ran reached the pty, once"
            );
            // The next key lands behind everything typed so far.
            terminal.update(&mut app, |view, ctx| {
                view.input
                    .update(ctx, |input, ctx| input.system_insert("n", ctx));
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "Add-Con",
                    "{mode:?}"
                );
            });
        })
    }
}

/// The premise: the same sequence without the view's Preexec handling records no anchor, and the
/// report goes to the front of the buffer, which is what round 9 logged (`after="d-AdCo"`). It is
/// also what every report without an anchor still does.
#[test]
fn without_the_anchor_the_same_sequence_puts_the_report_in_front() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
                type_a_line_across_a_shell_started_command(view, false, ctx);
                assert!(
                    !view.input.as_ref(ctx).has_handoff_report_anchor(),
                    "{mode:?}"
                );
                store_shell_report(view, "d-");
                view.handle_typeahead_report_with_handoff(true, ctx);
                assert_eq!(
                    view.input.as_ref(ctx).buffer_text(ctx),
                    "d-AdCo",
                    "{mode:?}: the round 9 result"
                );
            });
        })
    }
}

/// No key was typed while the shell-started command ran: its prompt hands input back with no
/// report on its way, and the anchor ends with that event. A report that comes later is about
/// later keys and goes to the front, as always.
#[test]
fn the_anchor_ends_when_the_editor_owns_input_again_with_no_report_coming() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            view.input
                .update(ctx, |input, ctx| input.system_insert("Ad", ctx));
            assert!(view.model.lock().typeahead_handoff_mut().on_preexec(true));
            view.handle_terminal_event(&ModelEvent::Handler(AnsiHandlerEvent::Preexec), ctx);
            assert!(view.input.as_ref(ctx).has_handoff_report_anchor());
            // A second Preexec event without a new take-over records nothing new and keeps it.
            view.handle_terminal_event(&ModelEvent::Handler(AnsiHandlerEvent::Preexec), ctx);
            assert!(view.input.as_ref(ctx).has_handoff_report_anchor());

            view.model.lock().typeahead_handoff_mut().on_precmd();
            view.commit_typeahead_handoff_at_prompt(ctx);
            assert_eq!(
                view.model.lock().typeahead_handoff().state(),
                &HandoffState::EditorOwned
            );
            assert!(
                !view.input.as_ref(ctx).has_handoff_report_anchor(),
                "nothing is left to place behind the editor's text"
            );
            assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "Ad");
        });
    })
}

/// The editor emptying (a submit, a clear) ends the anchor: the text it stood behind is gone.
#[test]
fn the_anchor_ends_when_the_editor_empties() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
        let terminal = add_window_with_terminal(&mut app, None);
        terminal.update(&mut app, |view, ctx| {
            view.input
                .update(ctx, |input, ctx| input.system_insert("Ad", ctx));
            assert!(view.model.lock().typeahead_handoff_mut().on_preexec(true));
            view.handle_terminal_event(&ModelEvent::Handler(AnsiHandlerEvent::Preexec), ctx);
            assert!(view.input.as_ref(ctx).has_handoff_report_anchor());
            view.input.update(ctx, |input, ctx| {
                input.clear_buffer_and_reset_undo_stack(ctx)
            });
            assert!(!view.input.as_ref(ctx).has_handoff_report_anchor());
        });
    })
}
