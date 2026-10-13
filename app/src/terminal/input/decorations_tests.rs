use settings::Setting as _;
use warpui::{text_layout::TextStyle, App, SingletonEntity as _};

use crate::{
    appearance::Appearance,
    settings::InputSettings,
    terminal::{
        input::{
            decorations::{InputBackgroundJobOptions, MAX_BACKGROUND_JOB_BUFFER_CHARS},
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
        let terminal =
            add_window_with_bootstrapped_terminal(&mut app, None, Some(session_info)).await;
        let input = terminal.read(&app, |view, _| view.input().clone());
        let editor = input.read(&app, |input, _| input.editor.clone());
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
