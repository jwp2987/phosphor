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

/// F10 (#1598): a deferred Enter's give-up leaves its line in the editor, unrun, and keeps the
/// keys typed after it WITH the line, on the line after it, in typed order; nothing is written to
/// the pty. The line alone arms no Enter guard when nothing was typed after it (the toast's
/// "press Enter again" is the Enter that sends it); a buffer with several lines would submit as
/// ONE command, so its first Enter is held: that is a line that already had several, and a line
/// with keys kept after it. Both modes.
///
/// Expectation history (re-decided by the owner, never weakened): #1661 expected the keys typed
/// after the Enter discarded; the first ruling of 2026-11-09 (#1996) expected them at the pty and
/// the one-line buffer under the glue guard; the second ruling of that day, "keep them together"
/// (a later line never runs without the line before it), keeps them with the unrun line.
#[test]
fn deferred_enter_give_up_guards_only_multi_line_buffers_and_keeps_later_keys_with_the_line() {
    use crate::terminal::input::{EnterGuardKind, HANDED_BACK_GUARD_TOAST};
    use crate::terminal::input_queue::{EntryId, EntryKind, InputEntry, Origin, Stamp};
    fn typed(id: u64, text: &str) -> InputEntry {
        InputEntry {
            id: EntryId(id),
            kind: EntryKind::Text(text.to_owned()),
            focus_epoch: 0,
            arrived_at: std::time::Instant::now(),
            origin: Origin::Keyboard,
            stamp: Stamp::default(),
        }
    }
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        for (line, typed_after, kept, guard) in [
            ("echo a", false, "echo a", None),
            (
                "echo a\necho b",
                false,
                "echo a\necho b",
                Some(EnterGuardKind::HandedBackText),
            ),
            (
                "echo a",
                true,
                "echo a\nls",
                Some(EnterGuardKind::HandedBackText),
            ),
        ] {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let terminal = add_window_with_terminal(&mut app, None);
                let pty = collect_pty_writes(&mut app, &terminal);
                terminal.update(&mut app, |view, ctx| {
                    view.input.update(ctx, |input, ctx| {
                        input.replace_buffer_content(line, ctx);
                    });
                    let taken = if typed_after {
                        vec![typed(1, "l"), typed(2, "s")]
                    } else {
                        Vec::new()
                    };
                    view.give_up_deferred_enter_keys(taken, "verdict", ctx);
                });
                assert_eq!(
                    String::from_utf8_lossy(&pty.borrow().concat()),
                    "",
                    "{mode:?} {line:?} typed_after={typed_after}: the Enter did not run, so \
                     nothing typed after it is written to the pty"
                );
                terminal.read(&app, |view, ctx| {
                    view.input.read(ctx, |input, ctx| {
                        assert_eq!(
                            input.buffer_text(ctx),
                            kept,
                            "{mode:?}: the line stays, and what was typed after its Enter stays \
                             with it on the line after it"
                        );
                        assert_eq!(
                            input.enter_guard_for_test().map(|(kind, _)| kind),
                            guard,
                            "{mode:?} {line:?} typed_after={typed_after}"
                        );
                        if guard == Some(EnterGuardKind::HandedBackText) {
                            assert_eq!(
                                input.enter_guard_for_test().map(|(_, toast)| toast),
                                Some(HANDED_BACK_GUARD_TOAST)
                            );
                        }
                    });
                });
            });
        }
    }
}
