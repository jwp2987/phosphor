//! Group 3: an editor paste queued behind an Enter gate follows the line (#1585) and the
//! committed-delivery mark (#1589). Child of `view_test.rs` (private helpers come in through
//! `super`).

use super::*;
use crate::terminal::input::PasteSource;
use crate::terminal::input_queue::DestinationMode;

/// Offers an editor paste of `text` to the pane queue (behind whatever gate is open).
fn offer_editor_paste_for_test(
    view: &mut TerminalView,
    text: &str,
    ctx: &mut ViewContext<TerminalView>,
) {
    let content = warpui::clipboard::ClipboardContent {
        plain_text: text.to_owned(),
        ..Default::default()
    };
    view.input.update(ctx, |input, input_ctx| {
        input.offer_ready_paste(content, PasteSource::PlainTextPaste, input_ctx)
    });
}

/// Offers typed text to the pane queue (behind whatever gate is open).
fn offer_text_for_test(view: &mut TerminalView, text: &str, ctx: &mut ViewContext<TerminalView>) {
    let queue = view.paste_queue.queue.clone();
    let mut host = paste_queue::PtyQueueHost::new(view, ctx);
    queue
        .lock()
        .offer(&mut host, input_queue::EntryKind::Text(text.to_owned()));
}

/// #1585: `echo `, a paste of `X`, ` Y` are queued behind an Enter whose command is `ssh host`.
/// At the release the paste follows the line into the connect hold, in order, instead of landing
/// in the local editor (Table); the kill switch keeps the editor insert (Legacy).
#[test]
fn editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                hold_text_behind_test_gate(view, "echo ", ctx);
                offer_editor_paste_for_test(view, "X", ctx);
                offer_text_for_test(view, " Y", ctx);
                submit_command(view, "ssh host", ctx);
                release_test_gate(view, ctx);
                let buffer = view.input.as_ref(ctx).buffer_text(ctx);
                match mode {
                    DestinationMode::Table => {
                        assert_eq!(
                            queued_texts(view),
                            vec!["echo ".to_owned(), "X".to_owned(), " Y".to_owned()]
                        );
                        assert_eq!(buffer, "");
                        assert!(view.ssh_typeahead_holding);
                    }
                    DestinationMode::Legacy => {
                        assert!(
                            buffer.contains('X'),
                            "legacy keeps the editor insert: {buffer:?}"
                        );
                    }
                }
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
        })
    }
}

/// #1585: at an idle local prompt (no ssh) the paste still lands in the editor, in order.
#[test]
fn editor_paste_behind_an_enter_gate_at_an_idle_prompt_lands_in_the_editor() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            app.add_singleton_model(|_| crate::workspace::ToastStack);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = record_all_pty_writes(&mut app, &terminal);
            terminal.update(&mut app, |view, ctx| {
                register_local_session_for_ssh_test(view, ctx);
                hold_text_behind_test_gate(view, "echo ", ctx);
                offer_editor_paste_for_test(view, "X", ctx);
                offer_text_for_test(view, " Y", ctx);
                release_test_gate(view, ctx);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "echo X Y");
            });
            for _ in 0..16 {
                futures_lite::future::yield_now().await;
            }
            assert!(pty_writes.borrow().is_empty(), "{:?}", pty_writes.borrow());
        })
    }
}
