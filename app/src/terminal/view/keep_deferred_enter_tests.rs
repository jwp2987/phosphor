//! #2000 (owner decision 2026-11-11): an EVENT that ends a deferred Enter without running it
//! (`TerminalView::keep_deferred_enter_text`: the editor refused the report as another line's
//! text, the young line cannot be classified, the young wait keeps the line in the editor) keeps
//! the keys typed behind that Enter WITH its line, as the timer paths do. Before, a complete
//! second line was written to the pty and ran while the first stayed unrun in the input. Child of
//! `view_test.rs` (private helpers come in through `super`).

use super::*;
use crate::terminal::input_queue::{DestinationMode, GateKeysThen, GateKind};
use crate::terminal::view::paste_queue::{
    gate_keys_may_follow_a_waiting_enter, kept_enter_keys_stay_with_line,
    unrun_deferred_enter_keys_then,
};

/// The three callers of `keep_deferred_enter_text`, by the reason each passes.
const KEEP_REASONS: [&str; 3] = [
    "report_refused",
    "young_cannot_classify",
    "young_keep_in_editor",
];

/// The pure rules, for both values of the kill switch `PHOSPHOR_KEEP_DEFERRED_ENTER_KEYS_WITH_LINE`
/// (read once per process, so it is an argument here).
#[test]
fn an_unrun_deferred_enter_keeps_its_keys_with_its_line_unless_switched_off() {
    for why in KEEP_REASONS {
        assert_eq!(
            unrun_deferred_enter_keys_then(true, why),
            GateKeysThen::KeepWithLine(why)
        );
        assert_eq!(
            unrun_deferred_enter_keys_then(false, why),
            GateKeysThen::Release,
            "switch off: released as before the decision"
        );
    }
    // With the line: only when there is one, and never while the tty reads a secret (#1957).
    assert!(kept_enter_keys_stay_with_line(false, false));
    assert!(
        !kept_enter_keys_stay_with_line(true, false),
        "a bare Enter has no unrun line: today's release"
    );
    assert!(
        !kept_enter_keys_stay_with_line(false, true),
        "a secret read gets the keys, they are never shown in the editor"
    );
    assert!(!kept_enter_keys_stay_with_line(true, true));
    // Behind a waiting Enter of the input is the typed order for a release and a keep alike; a
    // timer's give-up places its keys itself.
    assert!(gate_keys_may_follow_a_waiting_enter(GateKeysThen::Release));
    assert!(gate_keys_may_follow_a_waiting_enter(
        GateKeysThen::KeepWithLine("report_refused")
    ));
    assert!(!gate_keys_may_follow_a_waiting_enter(GateKeysThen::GiveUp(
        "report_patience"
    )));
}

/// The issue's case, once per caller, Legacy and Table: `cd build`, Enter (deferred behind the
/// shell's report), `make clean`, Enter typed behind it. The report arrives and the path that
/// keeps the Enter unrun fires. Nothing reaches the pty and nothing runs; both lines are in the
/// input, line one first, the Enters as line breaks; the hand-back guard holds the first Enter
/// on them; the deferred Enter's gate is gone and the queue is idle.
///
/// Before #2000 this wrote `make clean\r` to the pty while `cd build` stayed in the input.
#[test]
fn a_line_typed_behind_a_deferred_enter_kept_unrun_stays_with_that_line_in_the_input() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        for why in KEEP_REASONS {
            App::test((), move |mut app| async move {
                let _mode =
                    crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
                initialize_app_for_terminal_view(&mut app);
                let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
                let terminal = add_window_with_terminal(&mut app, None);
                let pty_writes = collect_pty_writes(&mut app, &terminal);
                let executed = count_executed_commands(&mut app, &terminal);

                terminal.update(&mut app, |view, ctx| {
                    awaiting_typeahead_report(view);
                    view.input.update(ctx, |input, ctx| {
                        input.replace_buffer_content("cd build", ctx)
                    });
                    view.input.update(ctx, |input, ctx| input.input_enter(ctx));
                    view.open_deferred_enter_gate(ctx);
                    // The next line and its Enter, typed on the terminal behind the deferred
                    // Enter.
                    view.write_user_bytes_to_pty(b"make clean\r".to_vec(), ctx);
                    assert_eq!(
                        view.paste_queue.queue.borrow().queued_len(),
                        1,
                        "{mode:?} {why}: line two waits behind the deferred Enter"
                    );
                    // The report arrives (the hold is released for the submit) and the Enter is
                    // kept unrun instead.
                    let _ = view.model.lock().typeahead_handoff_mut().on_report();
                    view.keep_deferred_enter_text(why, ctx);

                    let input = view.input.as_ref(ctx);
                    assert_eq!(
                        input.buffer_text(ctx),
                        "cd build\nmake clean\n",
                        "{mode:?} {why}: line one, then what was typed after its Enter"
                    );
                    assert!(
                        input.ssh_handover_enter_guard_armed(),
                        "{mode:?} {why}: the first Enter on the two lines is held"
                    );
                    let queue = view.paste_queue.queue.borrow();
                    assert!(
                        !queue.has_gate_kind(GateKind::HandoffReport),
                        "{mode:?} {why}: the deferred Enter's gate is closed"
                    );
                    assert!(queue.is_idle(), "{mode:?} {why}");
                });
                // Checked after the update, when the write events it emitted have been delivered.
                assert!(
                    pty_writes.borrow().is_empty(),
                    "{mode:?} {why}: line two must not reach the pty while line one is unrun: {:?}",
                    pty_writes.borrow()
                );
                assert_eq!(*executed.borrow(), 0, "{mode:?} {why}: nothing ran");
            });
        }
    }
}

/// A BARE deferred Enter (an empty input) kept unrun: there is no unrun line to keep the keys
/// with, so they are released as before the decision: the complete line typed behind it goes to
/// the pty as typed, once, and nothing becomes editor text. Legacy and Table.
#[test]
fn keys_behind_a_bare_deferred_enter_kept_unrun_are_released_as_before() {
    for mode in [DestinationMode::Legacy, DestinationMode::Table] {
        App::test((), move |mut app| async move {
            let _mode = crate::terminal::input_queue::facts::set_destination_mode_for_test(mode);
            initialize_app_for_terminal_view(&mut app);
            let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
            let terminal = add_window_with_terminal(&mut app, None);
            let pty_writes = collect_pty_writes(&mut app, &terminal);

            terminal.update(&mut app, |view, ctx| {
                hold_after_deferred_enter(view);
                view.write_user_bytes_to_pty(b"echo 1\r".to_vec(), ctx);
                let _ = view.model.lock().typeahead_handoff_mut().on_report();
                view.keep_deferred_enter_text("young_keep_in_editor", ctx);
                assert_eq!(view.input.as_ref(ctx).buffer_text(ctx), "", "{mode:?}");
                assert!(view.paste_queue.queue.borrow().is_idle(), "{mode:?}");
            });
            assert_eq!(
                String::from_utf8_lossy(&pty_writes.borrow().concat()),
                "echo 1\r",
                "{mode:?}: released to the pty as typed, once"
            );
        });
    }
}

/// #1957 is kept: with an unrun line in the input but the tty reading a secret, the held keys
/// are that reader's. They go to the pty in typed order and are never shown in the editor.
#[test]
fn keys_behind_an_unrun_deferred_enter_go_to_a_secret_read_not_into_the_input() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _flag = FeatureFlag::TypeaheadHandoff.override_enabled(true);
        let terminal = add_window_with_terminal(&mut app, None);
        let pty_writes = collect_pty_writes(&mut app, &terminal);

        terminal.update(&mut app, |view, ctx| {
            awaiting_typeahead_report(view);
            view.input.update(ctx, |input, ctx| {
                input.replace_buffer_content("cd build", ctx)
            });
            view.input.update(ctx, |input, ctx| input.input_enter(ctx));
            view.open_deferred_enter_gate(ctx);
            view.write_user_bytes_to_pty(b"hunter2\r".to_vec(), ctx);
            view.model
                .lock()
                .set_secret_read_probe(std::sync::Arc::new(|| true));
            let _ = view.model.lock().typeahead_handoff_mut().on_report();
            view.keep_deferred_enter_text("report_refused", ctx);
            assert_eq!(
                view.input.as_ref(ctx).buffer_text(ctx),
                "cd build",
                "the secret is never editor text"
            );
        });
        assert_eq!(
            String::from_utf8_lossy(&pty_writes.borrow().concat()),
            "hunter2\r",
            "the keys reach the reader of the tty, in typed order, once"
        );
    })
}
