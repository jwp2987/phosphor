use pathfinder_color::ColorU;
use settings::Setting as _;
use string_offset::ByteOffset;
use warp_core::features::FeatureFlag;
use warpui::{text_layout::TextStyle, App, SingletonEntity as _};

use crate::{
    appearance::Appearance,
    editor::TextRun,
    settings::InputSettings,
    terminal::{
        input::{
            decorations::{
                text_exceeds_background_job_token_cap, DecorationSet, InputBackgroundJobOptions,
                MAX_BACKGROUND_JOB_BUFFER_CHARS, MAX_BACKGROUND_JOB_BUFFER_TOKENS,
            },
            tests::{
                add_window_with_bootstrapped_terminal, initialize_app,
                simulate_directory_for_completion,
            },
        },
        model::session::SessionInfo,
    },
    themes::theme::AnsiColorIdentifier,
};
use warp_completer::completer::SuggestionTypeName;

#[test]
fn test_decorations_with_multibyte_chars() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);

        // Syntax highlighting and error underlining both default to `true`, but
        // they are persisted/synced settings; force them on for this app so the
        // test doesn't depend on ambient settings state that a concurrent test
        // could have toggled off (which would leave the input undecorated and
        // fail the assertions below).
        InputSettings::handle(&app).update(&mut app, |input_settings, ctx| {
            let _ = input_settings.syntax_highlighting.set_value(true, ctx);
            let _ = input_settings.error_underlining.set_value(true, ctx);
        });

        let terminal_colors_normal = app
            .get_singleton_model_handle::<Appearance>()
            .read(&app, |a, _| a.theme().terminal_colors().normal.to_owned());
        let error_underline_color = AnsiColorIdentifier::Red
            .to_ansi_color(&terminal_colors_normal)
            .into();
        let command_color = AnsiColorIdentifier::from(SuggestionTypeName::Command)
            .to_ansi_color(&terminal_colors_normal)
            .into();

        let session_info = SessionInfo::new_for_test();
        let session_id = session_info.session_id;

        let terminal =
            add_window_with_bootstrapped_terminal(&mut app, None, Some(session_info)).await;
        let input = terminal.read(&app, |view, _| view.input().clone());
        let editor = input.read(&app, |input, _| input.editor.clone());

        let session_id = terminal.update(&mut app, |terminal_view, ctx| {
            terminal_view
                .sessions_model()
                .update(ctx, |sessions, _ctx| {
                    // Seed the external command list directly. load_external_commands
                    // shells out (compgen/ls) via a real PTY, which doesn't run in the
                    // mocked unit-test env, so "echo" would never be recognized.
                    let session = sessions.get(session_id).expect("session should exist");
                    session.set_external_commands(["echo"]);
                });
            session_id
        });

        simulate_directory_for_completion(session_id, &terminal, &mut app, "/usr/bin");

        input
            .update(&mut app, |input, ctx| {
                input.clear_buffer_and_reset_undo_stack(ctx);
                input.user_insert("echo 'multibyte: שלום עולם'\nechoo hello world", ctx);

                // Trigger input decoration computation instead of waiting for the
                // debounced stream to trigger.
                input.run_input_background_jobs(
                    InputBackgroundJobOptions::default().with_command_decoration(),
                    ctx,
                );

                // Take the future handle from the input to ensure that it doesn't
                // get cancelled by the debounced decoration update stream.
                let future_handle = input
                    .decorations_future_handle
                    .take()
                    .expect("should have spawned decoration task");
                ctx.await_spawned_future(future_handle.future_id())
            })
            .await;

        let text_style_runs = editor.read(&app, |editor, ctx| {
            editor
                .text_style_runs(ctx)
                .map(|run| (run.text().to_owned(), run.text_style()))
                .collect::<Vec<_>>()
        });

        let expected = &[
            (
                "echo".to_string(),
                TextStyle::new().with_syntax_color(command_color),
            ),
            (" 'multibyte: שלום עולם'\n".to_string(), Default::default()),
            (
                "echoo".to_string(),
                TextStyle::new().with_error_underline_color(error_underline_color),
            ),
            (" hello world".to_string(), Default::default()),
        ];
        assert_eq!(
            text_style_runs, expected,
            "---- Expected ----\n{expected:#?}\n---- Actual ----\n{text_style_runs:#?}\n"
        );
    });
}

/// #868: a buffer over `MAX_BACKGROUND_JOB_BUFFER_CHARS` (a huge history line recalled with
/// Up) spawns no parse, carries no decorations, and later edits of it do not clear again.
#[test]
fn test_oversized_buffer_skips_background_jobs() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        InputSettings::handle(&app).update(&mut app, |input_settings, ctx| {
            let _ = input_settings.syntax_highlighting.set_value(true, ctx);
            let _ = input_settings.error_underlining.set_value(true, ctx);
        });

        let session_info = SessionInfo::new_for_test();
        let session_id = session_info.session_id;
        let terminal =
            add_window_with_bootstrapped_terminal(&mut app, None, Some(session_info)).await;
        let input = terminal.read(&app, |view, _| view.input().clone());
        let editor = input.read(&app, |input, _| input.editor.clone());

        // The decoration parse needs a completion session context, which needs the active
        // block's working directory; without one `run_input_background_jobs` takes the
        // empty-context branch and spawns nothing even for a normal buffer.
        simulate_directory_for_completion(session_id, &terminal, &mut app, "/usr/bin");
        assert!(
            input.read(&app, |input, ctx| input
                .completion_session_context(ctx)
                .is_some()),
            "test setup: the input should have a completion session context"
        );

        let decorate = InputBackgroundJobOptions::default().with_command_decoration();

        input.update(&mut app, |input, ctx| {
            // A normal buffer is parsed as usual.
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert("echo hi", ctx);
            input.run_input_background_jobs(decorate, ctx);
            assert!(
                input.decorations_future_handle.is_some(),
                "a normal buffer should spawn a decoration parse"
            );
            assert!(!input.decorations_cleared_for_oversized_buffer);

            // An oversized one is not.
            assert_eq!(MAX_BACKGROUND_JOB_BUFFER_CHARS, 4096);
            // Boundary (#1009, #1174): one char under and exactly at the cap are parsed, one
            // over is not.
            let cap = MAX_BACKGROUND_JOB_BUFFER_CHARS;
            for (len, parsed) in [(cap - 1, true), (cap, true), (cap + 1, false)] {
                let text = format!("echo {}", "x".repeat(len - 5));
                input.clear_buffer_and_reset_undo_stack(ctx);
                input.user_insert(&text, ctx);
                input.run_input_background_jobs(decorate, ctx);
                assert_eq!(
                    input.decorations_future_handle.is_some(),
                    parsed,
                    "{len} chars"
                );
            }
            let huge = format!("echo {}", "x".repeat(MAX_BACKGROUND_JOB_BUFFER_CHARS));
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert(&huge, ctx);
            input.run_input_background_jobs(decorate, ctx);
            assert!(
                input.decorations_future_handle.is_none(),
                "an oversized buffer must not spawn a parse"
            );
            assert!(input.decorations_cleared_for_oversized_buffer);

            // Running again on the same oversized buffer stays a no-op.
            input.run_input_background_jobs(decorate, ctx);
            assert!(input.decorations_future_handle.is_none());
            assert!(input.decorations_cleared_for_oversized_buffer);
        });

        let all_plain = editor.read(&app, |editor, ctx| {
            editor
                .text_style_runs(ctx)
                .all(|run| run.text_style() == TextStyle::default())
        });
        assert!(all_plain, "an oversized buffer should carry no decorations");

        // Back under the limit, the next run parses again and re-arms the clear.
        input.update(&mut app, |input, ctx| {
            // A different text than the first parse, so the "already parsed" short-circuit
            // cannot apply.
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert("echo bye", ctx);
            input.run_input_background_jobs(decorate, ctx);
            assert!(input.decorations_future_handle.is_some());
            assert!(!input.decorations_cleared_for_oversized_buffer);
        });
    });
}

/// #1604: a buffer over the parse cap keeps its detection request pending and gets a verdict
/// without a parse: the routing rules run on the text in a background job started from the edit
/// path (the buffer is under `MAX_EAGER_ROUTING_BYTES`), and the verdict settles the request. It
/// used to be abandoned here, so the Enter ran the text on the inherited type.
#[test]
fn test_oversized_buffer_keeps_detection_pending_and_is_classified_in_the_background() {
    use crate::ai::blocklist::InputConfig;
    use crate::settings::AISettings;
    use input_classifier::InputType;

    App::test((), |mut app| async move {
        let _am_flag = FeatureFlag::AgentMode.override_enabled(true);
        initialize_app(&mut app);
        AISettings::handle(&app).update(&mut app, |ai_settings, ctx| {
            let _ = ai_settings
                .ai_autodetection_enabled_internal
                .set_value(true, ctx);
        });
        let session_info = SessionInfo::new_for_test();
        let session_id = session_info.session_id;
        let terminal =
            add_window_with_bootstrapped_terminal(&mut app, None, Some(session_info)).await;
        let input = terminal.read(&app, |view, _| view.input().clone());
        simulate_directory_for_completion(session_id, &terminal, &mut app, "/usr/bin");

        let line = format!("echo {}; echo END", "x".repeat(MAX_BACKGROUND_JOB_BUFFER_CHARS));
        input.update(&mut app, |input, ctx| {
            input.ai_input_model().update(ctx, |ai_input, ctx| {
                ai_input.set_input_config(
                    InputConfig {
                        input_type: InputType::AI,
                        is_locked: false,
                    },
                    true, /* is_input_buffer_empty */
                    ctx,
                );
            });
        });
        warpui::r#async::Timer::after(std::time::Duration::from_millis(400)).await;
        input.update(&mut app, |input, ctx| {
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert(&line, ctx);
            assert!(input.buffer_exceeds_background_job_cap(ctx));
            let text = input.buffer_text(ctx);
            input
                .ai_input_model()
                .update(ctx, |model, _| model.note_detection_requested(text));
            input.run_input_background_jobs(
                InputBackgroundJobOptions::default().with_ai_input_detection(),
                ctx,
            );
            assert!(
                input.decorations_future_handle.is_none(),
                "no completer parse for an over-cap buffer"
            );
            assert!(
                input.detection_pending_for_buffer(ctx),
                "the request is not abandoned for size"
            );
            assert!(
                input.ai_input_model().as_ref(ctx).detection_job_in_flight(),
                "the routing job runs in the background"
            );
        });
        warpui::r#async::Timer::after(std::time::Duration::from_millis(500)).await;
        input.read(&app, |input, ctx| {
            assert_eq!(
                input.ai_input_model().as_ref(ctx).input_type(),
                InputType::Shell,
                "the over-cap `echo ...; echo END` line is a shell command"
            );
            assert!(!input.detection_pending_for_buffer(ctx));
        });
    });
}

/// #1031: a new edit, a clear, and an oversized buffer each cancel the previous in-flight
/// decoration job cooperatively (its cancel flag is set, so the CPU-bound parse stops at its
/// next token), and only the newest job stays live.
#[test]
fn test_superseded_decoration_job_is_cancelled() {
    use std::sync::atomic::Ordering;

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        InputSettings::handle(&app).update(&mut app, |input_settings, ctx| {
            let _ = input_settings.syntax_highlighting.set_value(true, ctx);
            let _ = input_settings.error_underlining.set_value(true, ctx);
        });

        let session_info = SessionInfo::new_for_test();
        let session_id = session_info.session_id;
        let terminal =
            add_window_with_bootstrapped_terminal(&mut app, None, Some(session_info)).await;
        let input = terminal.read(&app, |view, _| view.input().clone());
        simulate_directory_for_completion(session_id, &terminal, &mut app, "/usr/bin");

        let decorate = InputBackgroundJobOptions::default().with_command_decoration();

        input.update(&mut app, |input, ctx| {
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert("echo one", ctx);
            input.run_input_background_jobs(decorate, ctx);
            let first = input
                .decorations_cancel
                .clone()
                .expect("a normal buffer should start a cancellable job");
            assert!(!first.load(Ordering::Relaxed));

            // The next keystroke supersedes it.
            input.user_insert(" two", ctx);
            input.run_input_background_jobs(decorate, ctx);
            let second = input.decorations_cancel.clone().expect("second job");
            assert!(
                first.load(Ordering::Relaxed),
                "the superseded job must be told to stop"
            );
            assert!(!second.load(Ordering::Relaxed));

            // Clearing the editor stops the live job too.
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.run_input_background_jobs(decorate, ctx);
            let third = input.decorations_cancel.clone().expect("third job");
            assert!(
                second.load(Ordering::Relaxed),
                "clearing the buffer must cancel the job for the old text"
            );

            // An oversized paste cancels the live job and starts none.
            let huge = format!("echo {}", "x".repeat(MAX_BACKGROUND_JOB_BUFFER_CHARS));
            input.user_insert(&huge, ctx);
            input.run_input_background_jobs(decorate, ctx);
            assert!(third.load(Ordering::Relaxed));
            assert!(input.decorations_cancel.is_none());
            assert!(input.decorations_future_handle.is_none());
        });
    });
}

/// #1174: the completer parse is ~quadratic in the token count, so a buffer of many short words
/// is over the cap even when it is under the char cap.
#[test]
fn test_token_cap_counts_words_not_chars() {
    let words = |n: usize| vec!["a"; n].join(" ");
    assert!(!text_exceeds_background_job_token_cap(&words(
        MAX_BACKGROUND_JOB_BUFFER_TOKENS
    )));
    assert!(text_exceeds_background_job_token_cap(&words(
        MAX_BACKGROUND_JOB_BUFFER_TOKENS + 1
    )));
    // A single enormous word is one token; the char cap handles it.
    assert!(!text_exceeds_background_job_token_cap(&"x".repeat(100_000)));
    // Under the char cap, so only the token cap can reject it.
    assert!(words(MAX_BACKGROUND_JOB_BUFFER_TOKENS + 1).len() <= MAX_BACKGROUND_JOB_BUFFER_CHARS);
}

fn range(start: usize, end: usize) -> std::ops::Range<ByteOffset> {
    ByteOffset::from(start)..ByteOffset::from(end)
}

fn run(text: &str, start: usize, style: TextStyle) -> TextRun {
    TextRun::new(text.to_string(), style, range(start, start + text.len()))
}

fn syntax_style(color: ColorU) -> TextStyle {
    TextStyle {
        syntax_color: Some(color),
        ..Default::default()
    }
}

const GREEN: ColorU = ColorU {
    r: 0,
    g: 255,
    b: 0,
    a: 255,
};
const RED: ColorU = ColorU {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

/// #1133: a parse whose decorations already sit in the buffer must compare equal to the
/// buffer's own style runs, so `apply_decorations` leaves the editor alone.
#[test]
fn test_decoration_set_equals_buffer_runs_when_unchanged() {
    // "echo hi": `echo` is a highlighted command, the rest is plain.
    let desired = DecorationSet::new(vec![(range(0, 4), GREEN)], vec![]);
    let current = DecorationSet::from_text_runs(
        vec![
            run("echo", 0, syntax_style(GREEN)),
            run(" hi", 4, TextStyle::default()),
        ]
        .into_iter(),
    );
    assert_eq!(desired, current);
}

/// Styles other than syntax color / error underline are not decorations; a run differing
/// only in them must not force a re-apply.
#[test]
fn test_decoration_set_ignores_non_decoration_styles() {
    let slash_prefix = TextStyle {
        foreground_color: Some(RED),
        ..Default::default()
    };
    let current = DecorationSet::from_text_runs(vec![run("/cmd", 0, slash_prefix)].into_iter());
    assert_eq!(current, DecorationSet::default());
}

/// Anything that changes what would be drawn must compare unequal: a changed range (the
/// token grew), a changed color (theme switch), a missing or extra error underline.
#[test]
fn test_decoration_set_detects_every_visible_difference() {
    let applied = DecorationSet::from_text_runs(
        vec![
            run("ech", 0, syntax_style(GREEN)),
            run("o", 3, TextStyle::default()),
        ]
        .into_iter(),
    );
    // Token grew by one character.
    assert_ne!(
        applied,
        DecorationSet::new(vec![(range(0, 4), GREEN)], vec![])
    );
    // Same range, new theme color.
    assert_ne!(
        applied,
        DecorationSet::new(vec![(range(0, 3), RED)], vec![])
    );
    // Same syntax but now also underlined as an error.
    assert_ne!(
        applied,
        DecorationSet::new(vec![(range(0, 3), GREEN)], vec![(range(0, 3), RED)])
    );
    // Decoration disabled (e.g. syntax highlighting turned off) while the buffer still has it.
    assert_ne!(applied, DecorationSet::default());
}

/// Stale-result guard: the comparison is against the styles the buffer has *now*, so after an
/// edit drops styles (undo, a clear) the recomputed set differs and is re-applied rather than
/// skipped.
#[test]
fn test_decoration_set_compares_against_current_buffer_not_history() {
    let desired = DecorationSet::new(vec![(range(0, 4), GREEN)], vec![]);
    let cleared =
        DecorationSet::from_text_runs(vec![run("echo ", 0, TextStyle::default())].into_iter());
    assert_ne!(desired, cleared);
}

/// The buffer reports adjacent same-styled fragments as one run and the parse reports one
/// range per token; touching same-color ranges and empty ranges normalize identically.
#[test]
fn test_decoration_set_normalizes_touching_and_empty_ranges() {
    let a = DecorationSet::new(
        vec![
            (range(0, 2), GREEN),
            (range(2, 5), GREEN),
            (range(7, 7), RED),
        ],
        vec![],
    );
    let b = DecorationSet::new(vec![(range(0, 5), GREEN)], vec![]);
    assert_eq!(a, b);
}

/// #1298: clearing decorations from a buffer that carries none must not touch the editor. Every
/// style edit makes the editor relayout, reset its cursor blink and request a frame even when it
/// changes nothing, and an AI-mode buffer was cleared on every keystroke, so each key drew a
/// second frame ~10 ms later that looked exactly like the first.
#[test]
fn test_clearing_an_undecorated_buffer_does_not_update_styles() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let terminal = add_window_with_bootstrapped_terminal(&mut app, None, None).await;
        let input = terminal.read(&app, |view, _| view.input().clone());
        let editor = input.read(&app, |input, _| input.editor.clone());
        let updates = |app: &App| editor.read(app, |editor, _| editor.styles_updated_count());

        input.update(&mut app, |input, ctx| {
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert("how do i list files", ctx);
        });

        // Nothing to clear: no style edit.
        let before = updates(&app);
        input.update(&mut app, |input, ctx| {
            assert!(!input.buffer_has_decorations(ctx));
            input.clear_decorations(ctx);
        });
        assert_eq!(
            updates(&app),
            before,
            "clearing a buffer without decorations must not report a style update"
        );

        // With a decoration present, the clear still runs and removes it.
        editor.update(&mut app, |editor, ctx| {
            editor.update_buffer_styles(
                vec![string_offset::CharOffset::from(0)..string_offset::CharOffset::from(3)],
                crate::editor::TextStyleOperation::default().set_syntax_color(GREEN),
                ctx,
            )
        });
        let decorated = updates(&app);
        assert_eq!(decorated, before + 1);
        input.update(&mut app, |input, ctx| {
            assert!(input.buffer_has_decorations(ctx));
            input.clear_decorations(ctx);
            assert!(!input.buffer_has_decorations(ctx));
        });
        assert_eq!(updates(&app), decorated + 1);
    });
}

/// #1298: in AI mode the background job clears decorations before anything else; for an
/// undecorated prompt that must be free (no style update, so no second frame per key).
#[test]
fn test_ai_mode_background_job_does_not_restyle_an_undecorated_prompt() {
    use crate::ai::blocklist::{InputConfig, InputType};

    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let session_info = SessionInfo::new_for_test();
        let session_id = session_info.session_id;
        let terminal =
            add_window_with_bootstrapped_terminal(&mut app, None, Some(session_info)).await;
        let input = terminal.read(&app, |view, _| view.input().clone());
        let editor = input.read(&app, |input, _| input.editor.clone());
        simulate_directory_for_completion(session_id, &terminal, &mut app, "/usr/bin");

        input.update(&mut app, |input, ctx| {
            input.ai_input_model().update(ctx, |ai_input, ctx| {
                ai_input.set_input_config(
                    InputConfig {
                        input_type: InputType::AI,
                        is_locked: true,
                    },
                    true, /* is_input_buffer_empty */
                    ctx,
                );
            });
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert("how do i list files", ctx);
        });

        let before = editor.read(&app, |editor, _| editor.styles_updated_count());
        input.update(&mut app, |input, ctx| {
            input.run_input_background_jobs(
                InputBackgroundJobOptions::default().with_command_decoration(),
                ctx,
            );
        });
        let after = editor.read(&app, |editor, _| editor.styles_updated_count());
        assert_eq!(
            after, before,
            "an AI-mode keystroke job must not restyle a buffer that has no decorations"
        );
    });
}
