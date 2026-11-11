//! #2080: the shifted navigation keys scroll the scrollback under a running command and write
//! nothing to it. Child of `view_test.rs` (private helpers come in through `super`).

use super::*;
use crate::terminal::view::scrollback_keys::ScrollbackKey;

#[test]
fn shifted_page_keys_scroll_under_a_running_command_and_write_nothing_to_it() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = record_all_pty_writes(&mut app, &terminal);
        terminal.update(&mut app, |view, ctx| {
            register_local_session_for_ssh_test(view, ctx);
            {
                let mut model = view.model.lock();
                model.simulate_block("ls", &"\n".repeat(1000));
                model.simulate_long_running_block("cat", "");
            }
            assert!(view.is_long_running());
            assert_eq!(
                view.scroll_position(),
                ScrollPosition::FollowsBottomOfMostRecentBlock
            );

            for key in [ScrollbackKey::PageUp, ScrollbackKey::Top] {
                view.update_scroll_position_locking(ScrollPositionUpdate::AfterEnd, ctx);
                assert_eq!(
                    view.scroll_position(),
                    ScrollPosition::FollowsBottomOfMostRecentBlock
                );
                view.handle_action(&TerminalAction::ScrollbackKey(key), ctx);
                assert!(
                    matches!(
                        view.scroll_position(),
                        ScrollPosition::FixedAtPosition { .. }
                    ),
                    "{key:?} leaves the bottom"
                );
            }
            // Page Down and End are scrolls too: whatever position they give, no byte is written.
            view.handle_action(&TerminalAction::ScrollbackKey(ScrollbackKey::PageDown), ctx);
            view.handle_action(&TerminalAction::ScrollbackKey(ScrollbackKey::Bottom), ctx);
        });
        assert!(
            pty_writes.borrow().is_empty(),
            "a scrollback key must not reach the running program: {:?}",
            pty_writes.borrow()
        );

        // Control: the plain key is still the program's.
        terminal.update(&mut app, |view, ctx| {
            view.handle_action(&TerminalAction::PageUp, ctx);
        });
        assert!(
            !pty_writes.borrow().is_empty(),
            "plain Page Up goes to the running program"
        );
    });
}
