//! Follow-ups of the final review (F6 #1594, F10 #1598). Child of `view_test.rs` (private helpers
//! come in through `super`).

use super::*;
use crate::editor::InteractionState;
use crate::terminal::input_queue::DestinationMode;

/// F6 (#1594): a pending command shown after the give-up is read-only, because the prompt's event
/// runs the requested text and never an edit of it (#1435).
#[test]
fn pending_command_shown_after_give_up_is_read_only() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let terminal = add_window_with_terminal(&mut app, None);
            terminal.update(&mut app, |view, ctx| {
                view.set_pending_command("echo late", ctx);
                view.pending_command_awaits_precmd = true;
                view.input.update(ctx, |input, ctx| {
                    input.clear_buffer_and_reset_undo_stack(ctx);
                });
                view.pending_command_precmd_give_up(ctx);
            });
            terminal.read(&app, |view, ctx| {
                view.input.read(ctx, |input, ctx| {
                    assert_eq!(input.buffer_text(ctx), "echo late", "{mode:?}");
                    assert_eq!(
                        input.editor().as_ref(ctx).interaction_state(ctx),
                        InteractionState::Selectable,
                        "{mode:?}: the shown pending command cannot be edited"
                    );
                });
            });
        });
    }
}

/// F10 (#1598): a handed-back burst of one line arms no Enter guard (the toast's "press Enter
/// again" is the Enter that sends it); a burst with several lines would submit as ONE command, so
/// its first Enter is held.
#[test]
fn deferred_enter_hand_back_guards_only_multi_line_buffers() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        for (held, guarded) in [(&b"echo a"[..], false), (&b"echo a\necho b"[..], true)] {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let terminal = add_window_with_terminal(&mut app, None);
                terminal.update(&mut app, |view, ctx| {
                    view.hand_back_deferred_enter_keys_to_editor(held.to_vec(), "verdict", ctx);
                });
                terminal.read(&app, |view, ctx| {
                    view.input.read(ctx, |input, ctx| {
                        assert!(!input.buffer_text(ctx).is_empty(), "{mode:?}");
                        assert_eq!(
                            input.ssh_handover_enter_guard_armed(),
                            guarded,
                            "{mode:?} {held:?}"
                        );
                    });
                });
            });
        }
    }
}
