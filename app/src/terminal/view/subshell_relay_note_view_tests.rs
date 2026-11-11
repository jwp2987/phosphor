//! #2142 (owner decision 2026-11-11): what a person is told when a click on Phosphorize waits
//! behind a relay (`sudo -s`, ssh). The judge's rules are untouched: nothing is written at the
//! idle prompt, an Enter on an empty line brings the setup, a typed command cancels it. These
//! tests hold the two notes: each shown once, the wait note gone when the setup is written or the
//! attempt ends, the cancel note with a way back that exists. Child of `view_test.rs` (private
//! helpers come in through `super`).

use super::*;
use crate::terminal::view::subshell_relay_note;

/// The toasts asked of the workspace from now on, by kind.
#[derive(Default)]
struct Toasts {
    ephemeral: Vec<String>,
    persistent: Vec<String>,
    removed: Vec<String>,
}

/// Records every toast event. The `ToastStack` singleton must be registered.
fn record_toast_events(app: &mut App) -> Rc<RefCell<Toasts>> {
    let toasts = Rc::new(RefCell::new(Toasts::default()));
    let sink = toasts.clone();
    app.update(|ctx| {
        let stack = crate::workspace::ToastStack::handle(ctx);
        ctx.subscribe_to_model(&stack, move |_, event, _| match event {
            crate::workspace::ToastStackEvent::AddEphemeralToast { toast, .. } => sink
                .borrow_mut()
                .ephemeral
                .push(toast.main_text_for_test().to_string()),
            crate::workspace::ToastStackEvent::AddPersistentToast { toast, .. } => sink
                .borrow_mut()
                .persistent
                .push(toast.main_text_for_test().to_string()),
            crate::workspace::ToastStackEvent::RemoveToast { identifier, .. } => {
                sink.borrow_mut().removed.push(identifier.clone())
            }
        });
    });
    toasts
}

/// `sudo -s` runs, its shell has drawn its prompt, the Phosphorize button is offered: the state
/// in which a person clicks it. Returns the shared tty sample.
fn sudo_at_its_prompt_with_the_button(
    view: &mut TerminalView,
    ctx: &mut ViewContext<TerminalView>,
) -> Arc<warpui_core::sync_shim::Mutex<SubshellTtySample>> {
    view.model.lock().simulate_long_running_block("sudo -s", "");
    view.model.lock().process_bytes("# ");
    let tty = install_subshell_tty_for_test(view, sudo_relay(false));
    view.use_agent_footer.update(ctx, |footer, ctx| {
        footer.set_warpify_mode(WarpificationMode::subshell("sudo -s".to_owned()), ctx);
    });
    tty
}

fn cancel_notes(toasts: &Toasts) -> Vec<String> {
    toasts
        .ephemeral
        .iter()
        .filter(|text| text.starts_with("Phosphor setup cancelled"))
        .cloned()
        .collect()
}

/// The click at an idle prompt behind a relay writes nothing and says what brings the setup; a
/// second click adds no second note; the Enter on an empty line brings the prompt the judge
/// waits for, the setup is written and the note goes. No cancel note on that path.
#[test]
fn a_click_behind_a_relay_shows_the_wait_note_once_until_the_setup_is_written() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _footer = FeatureFlag::WarpifyFooter.override_enabled(true);
        app.add_singleton_model(|_| crate::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let written = subshell_pty_writes(&mut app, &terminal);
        let toasts = record_toast_events(&mut app);
        let note_id = terminal.update(&mut app, |view, ctx| {
            sudo_at_its_prompt_with_the_button(view, ctx);
            view.arm_subshell_setup_from_banner(None, ctx);
            assert_eq!(
                readiness_state(view),
                "armed",
                "premise: RelayNoPrompt, the prompt was drawn before the click"
            );
            let id = view
                .subshell_relay_wait_note_id
                .clone()
                .expect("the wait note shows");
            // A second click (Ctrl-I pressed twice) while the note shows.
            view.arm_subshell_setup_from_banner(None, ctx);
            assert_eq!(readiness_state(view), "armed");
            assert_eq!(view.subshell_relay_wait_note_id.as_ref(), Some(&id));
            id
        });
        {
            let toasts = toasts.borrow();
            assert_eq!(
                toasts.persistent,
                vec![subshell_relay_note::ARMED_WAIT_TEXT.to_owned()],
                "the wait note, once"
            );
            assert!(
                toasts.ephemeral.is_empty(),
                "not the old 'starts when the shell shows its prompt' beside it: {:?}",
                toasts.ephemeral
            );
            assert!(toasts.removed.is_empty(), "it stays while the wait lasts");
        }
        assert!(written.borrow().is_empty(), "the click writes nothing");
        terminal.update(&mut app, |view, ctx| {
            assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
            assert_eq!(readiness_state(view), "armed", "an empty line re-arms");
            assert!(
                view.subshell_relay_wait_note_id.is_some(),
                "still waiting: the note stays"
            );
            view.model.lock().process_bytes("\r\n# ");
            output_wakeup(view, ctx);
            assert_eq!(readiness_state(view), "written", "the new prompt");
            assert!(
                view.subshell_relay_wait_note_id.is_none(),
                "gone when the setup is written"
            );
        });
        let toasts = toasts.borrow();
        assert_eq!(toasts.persistent.len(), 1, "never shown again");
        assert_eq!(toasts.removed, vec![note_id]);
        assert!(cancel_notes(&toasts).is_empty());
        let all = String::from_utf8_lossy(&written.borrow()).into_owned();
        assert!(
            all.contains("__hc0"),
            "the sh probe's prelude went out: {all:?}"
        );
    })
}

/// The user's own command cancels the attempt (R2, unchanged): the wait note goes, one cancel
/// note says so and names the button, the button is offered again at the next look of the pane
/// (Ctrl-I has its context back), and a second click starts a new attempt with its own note.
/// Nothing of the setup is written by any of it.
#[test]
fn a_typed_command_cancels_with_a_note_and_the_button_comes_back() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _footer = FeatureFlag::WarpifyFooter.override_enabled(true);
        app.add_singleton_model(|_| crate::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let written = subshell_pty_writes(&mut app, &terminal);
        let toasts = record_toast_events(&mut app);
        let note_id = terminal.update(&mut app, |view, ctx| {
            sudo_at_its_prompt_with_the_button(view, ctx);
            view.arm_subshell_setup_from_banner(None, ctx);
            assert_eq!(readiness_state(view), "armed");
            assert!(
                view.pending_subshell_warpify_command(ctx).is_none(),
                "premise: the click took the button away"
            );
            view.subshell_relay_wait_note_id
                .clone()
                .expect("the wait note shows")
        });
        terminal.update(&mut app, |view, ctx| {
            assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
            assert_eq!(readiness_state(view), "armed", "no line end yet");
            assert!(
                view.subshell_relay_wait_note_id.is_some(),
                "typing alone changes nothing"
            );
            assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
            assert_eq!(readiness_state(view), "idle", "UserDrove");
            assert!(
                view.subshell_relay_wait_note_id.is_none(),
                "gone when the attempt is cancelled"
            );
            assert!(
                view.pending_subshell_warpify_command(ctx).is_none(),
                "the button is not offered on the key path"
            );
        });
        {
            let toasts = toasts.borrow();
            assert_eq!(toasts.removed, vec![note_id.clone()]);
            let cancels = cancel_notes(&toasts);
            assert_eq!(cancels.len(), 1, "one cancel note: {:?}", toasts.ephemeral);
            assert!(
                cancels[0].contains("you ran a command before it started")
                    && cancels[0].contains("click Phosphorize subshell"),
                "{:?}",
                cancels[0]
            );
        }
        let before = String::from_utf8_lossy(&written.borrow()).into_owned();
        assert!(
            before.starts_with("ls") && !before.contains("__hc0"),
            "the user's keys, nothing of the setup: {before:?}"
        );
        terminal.update(&mut app, |view, ctx| {
            view.model.lock().process_bytes("ls\r\nfile\r\n# ");
            output_wakeup(view, ctx);
            assert_eq!(readiness_state(view), "idle", "a look revives nothing");
            assert_eq!(
                view.pending_subshell_warpify_command(ctx).as_deref(),
                Some("sudo -s"),
                "the button is offered again for the same command"
            );
            output_wakeup(view, ctx);
        });
        assert_eq!(
            cancel_notes(&toasts.borrow()).len(),
            1,
            "later looks add no note"
        );
        assert_eq!(
            String::from_utf8_lossy(&written.borrow()),
            before,
            "the note and the button write nothing"
        );
        // The way back the note named: a second click is a new attempt, told the same way, and
        // an Enter on an empty line brings its setup.
        terminal.update(&mut app, |view, ctx| {
            view.arm_subshell_setup_from_banner(None, ctx);
            assert_eq!(readiness_state(view), "armed");
            assert!(view.subshell_relay_wait_note_id.is_some());
            assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
            view.model.lock().process_bytes("\r\n# ");
            output_wakeup(view, ctx);
            assert_eq!(readiness_state(view), "written");
            assert!(view.subshell_relay_wait_note_id.is_none());
        });
        let toasts = toasts.borrow();
        assert_eq!(
            toasts.persistent.len(),
            2,
            "one wait note per attempt: {:?}",
            toasts.persistent
        );
        assert_eq!(toasts.removed, vec![note_id.clone(), note_id]);
        assert_eq!(cancel_notes(&toasts).len(), 1);
    })
}

/// The 60 s ceiling (an existing net, judged at a look) ends the attempt: the wait note goes with
/// it and the existing "did not start" toast follows. No cancel note: the user ran nothing.
#[test]
fn the_ceiling_removes_the_wait_note() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let written = subshell_pty_writes(&mut app, &terminal);
        let toasts = record_toast_events(&mut app);
        terminal.update(&mut app, |view, ctx| {
            sudo_at_its_prompt_with_the_button(view, ctx);
            view.arm_subshell_setup_from_banner(None, ctx);
            assert!(view.subshell_relay_wait_note_id.is_some());
            backdate_armed(view, subshell_ready::PRE_READY_CEILING);
            output_wakeup(view, ctx);
            assert_eq!(readiness_state(view), "idle", "TooLong");
            assert!(view.subshell_relay_wait_note_id.is_none());
        });
        let toasts = toasts.borrow();
        assert_eq!(toasts.persistent.len(), 1);
        assert_eq!(toasts.removed.len(), 1);
        assert!(cancel_notes(&toasts).is_empty());
        assert!(
            toasts
                .ephemeral
                .iter()
                .any(|text| text.starts_with("Phosphor setup did not start")),
            "{:?}",
            toasts.ephemeral
        );
        assert!(written.borrow().is_empty());
    })
}

/// Control: a click that does not wait behind a relay keeps the old toast and the old silence
/// (the wait there is not ended by an Enter on an empty line, and the cancel is not the relay's).
#[test]
fn a_click_that_waits_for_something_else_keeps_the_old_toast_and_a_silent_cancel() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        app.add_singleton_model(|_| crate::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let _written = subshell_pty_writes(&mut app, &terminal);
        let toasts = record_toast_events(&mut app);
        let tty = terminal.update(&mut app, |view, ctx| {
            view.model.lock().simulate_long_running_block("bash", "");
            let tty = install_subshell_tty_for_test(view, vim_in_front());
            view.arm_subshell_setup_from_banner(Some(ShellType::Bash), ctx);
            assert_eq!(readiness_state(view), "armed");
            assert!(view.subshell_relay_wait_note_id.is_none());
            view.model.lock().process_bytes("bash-5.1$ ");
            output_wakeup(view, ctx);
            tty
        });
        terminal.update(&mut app, |view, ctx| {
            *tty.lock() = bash_reading();
            assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
            assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
            assert_eq!(readiness_state(view), "idle", "premise: UserDrove");
            assert!(view.subshell_reoffer_pending.is_none());
        });
        let toasts = toasts.borrow();
        assert!(toasts.persistent.is_empty(), "{:?}", toasts.persistent);
        assert_eq!(
            toasts.ephemeral,
            vec!["Phosphor setup starts when the shell shows its prompt.".to_owned()]
        );
    })
}

/// The kill switch (`PHOSPHOR_SUBSHELL_RELAY_NOTE=0`): the same relay scenario as the two tests
/// above shows the old toast at the click, nothing at the cancel, and offers no button again.
#[test]
fn the_switch_off_restores_the_old_toast_and_the_silent_cancel() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _footer = FeatureFlag::WarpifyFooter.override_enabled(true);
        app.add_singleton_model(|_| crate::ToastStack);
        let terminal = add_window_with_terminal(&mut app, None);
        let _written = subshell_pty_writes(&mut app, &terminal);
        let toasts = record_toast_events(&mut app);
        terminal.update(&mut app, |view, ctx| {
            view.subshell_relay_note_on = false;
            sudo_at_its_prompt_with_the_button(view, ctx);
            view.arm_subshell_setup_from_banner(None, ctx);
            assert_eq!(readiness_state(view), "armed");
            assert!(view.subshell_relay_wait_note_id.is_none());
            assert!(view.write_user_bytes_to_pty(b"ls".to_vec(), ctx));
            assert!(view.write_user_bytes_to_pty(b"\r".to_vec(), ctx));
            assert_eq!(readiness_state(view), "idle", "premise: UserDrove");
            assert!(view.subshell_reoffer_pending.is_none());
            view.model.lock().process_bytes("ls\r\nfile\r\n# ");
            output_wakeup(view, ctx);
            assert!(view.pending_subshell_warpify_command(ctx).is_none());
        });
        let toasts = toasts.borrow();
        assert!(toasts.persistent.is_empty());
        assert!(toasts.removed.is_empty());
        assert_eq!(
            toasts.ephemeral,
            vec!["Phosphor setup starts when the shell shows its prompt.".to_owned()]
        );
    })
}
