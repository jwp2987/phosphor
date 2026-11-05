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

/// F10 (#1598) / #1661 (owner decision 2026-11-05): a deferred Enter's give-up leaves its line in
/// the editor, unrun, and DISCARDS the keys typed after it (never put into the editor). The line
/// alone arms no Enter guard when nothing was discarded (the toast's "press Enter again" is the
/// Enter that sends it); a line with several lines would submit as ONE command, so its first
/// Enter is held; keys discarded behind a one-line buffer arm the glue guard (text typed onto it
/// next must not run glued to it). Both modes.
#[test]
fn deferred_enter_hand_back_guards_only_multi_line_buffers() {
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
        for (line, typed_after, guard) in [
            ("echo a", false, None),
            (
                "echo a\necho b",
                false,
                Some(EnterGuardKind::HandedBackText),
            ),
            ("echo a", true, Some(EnterGuardKind::Glue)),
        ] {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let terminal = add_window_with_terminal(&mut app, None);
                terminal.update(&mut app, |view, ctx| {
                    view.input.update(ctx, |input, ctx| {
                        input.replace_buffer_content(line, ctx);
                    });
                    let taken = if typed_after {
                        vec![typed(1, "l"), typed(2, "s")]
                    } else {
                        Vec::new()
                    };
                    view.hand_back_deferred_enter_keys_to_editor(taken, "verdict", ctx);
                });
                terminal.read(&app, |view, ctx| {
                    view.input.read(ctx, |input, ctx| {
                        assert_eq!(
                            input.buffer_text(ctx),
                            line,
                            "{mode:?}: the line stays and nothing typed after it came back"
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
