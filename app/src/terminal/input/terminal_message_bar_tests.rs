use super::is_ai_detected;

#[test]
fn autodetected_requires_live_ai_input_type() {
    // Regression for #894: autodetection enabled + unlocked + non-empty buffer
    // while the live input type is Shell must NOT read as "autodetected AI".
    assert!(!is_ai_detected(true, true, false, false));
    assert!(is_ai_detected(true, true, false, true));
}

#[test]
fn autodetected_requires_other_conditions() {
    assert!(!is_ai_detected(false, true, false, true));
    assert!(!is_ai_detected(true, false, false, true));
    assert!(!is_ai_detected(true, true, true, true));
}

#[test]
fn typing_that_keeps_the_bar_the_same_does_not_rerender() {
    use super::{BufferSignature, buffer_event_needs_render};
    let ls = BufferSignature::of("ls");
    let lsa = BufferSignature::of("ls -a");
    // The first event always renders; then more of the same kind of text does not.
    assert!(buffer_event_needs_render(None, ls, true));
    assert!(!buffer_event_needs_render(Some(ls), lsa, true));
    // Emptiness and the command prefixes the bar reads still render.
    assert!(buffer_event_needs_render(
        Some(BufferSignature::of("")),
        ls,
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of(""),
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of("/agent"),
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of(" /plan x"),
        true
    ));
    assert!(buffer_event_needs_render(
        Some(ls),
        BufferSignature::of("/new"),
        true
    ));
    // Kill switch off: every event renders.
    assert!(buffer_event_needs_render(Some(ls), lsa, false));
}

/// #1962: an event that may change the row (the buffer became empty or non-empty, the
/// suggestions mode changed) re-renders the bar only when the row it would draw differs from the
/// row it drew.
#[test]
fn an_event_that_leaves_the_row_as_drawn_does_not_rerender() {
    use super::row_event_needs_render;
    assert!(!row_event_needs_render(Some("row"), Some("row"), true));
}

#[test]
fn an_event_that_changes_the_row_or_cannot_be_judged_rerenders() {
    use super::row_event_needs_render;
    // The row changed: an errored block's hint goes away with the first character, the inline
    // history menu opened, the input was autodetected as a prompt.
    assert!(row_event_needs_render(Some("row"), Some("other"), true));
    // Never rendered, or the drawn row had a click handler (the misroute recovery offer).
    assert!(row_event_needs_render(None, Some("row"), true));
    // The row now cannot be compared, or the terminal model was busy.
    assert!(row_event_needs_render(Some("row"), None, true));
    assert!(row_event_needs_render(None, None, true));
    // Kill switch: every such event renders.
    assert!(row_event_needs_render(Some("row"), Some("row"), false));
}

/// On a real terminal: the first character typed into an empty input leaves the default row as
/// it is (so the bar is not re-rendered for it), and a buffer the bar has its own hint for
/// changes the row (so it still is).
#[test]
fn the_first_typed_character_keeps_the_default_row_and_a_command_prefix_changes_it() {
    use warpui::App;

    use crate::search::slash_command_menu::static_commands::commands;
    use crate::test_util::terminal::{add_window_with_terminal, initialize_app_for_terminal_view};

    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let terminal = add_window_with_terminal(&mut app, None);
        let input = terminal.read(&app, |terminal, _| terminal.input().clone());
        let bar = input.read(&app, |input, _| input.terminal_input_message_bar.clone());
        let row = |app: &App| {
            bar.read(app, |bar, ctx| {
                let terminal_model = bar.terminal_model.lock();
                bar.message(&terminal_model, ctx).display_fingerprint()
            })
        };

        let empty_row = row(&app);
        assert!(empty_row.is_some(), "the default row is comparable");

        input.update(&mut app, |input, ctx| {
            input.user_insert("m", ctx);
        });
        assert_eq!(
            empty_row,
            row(&app),
            "one typed character leaves the default row as it is"
        );

        input.update(&mut app, |input, ctx| {
            input.clear_buffer_and_reset_undo_stack(ctx);
            input.user_insert(commands::AGENT.name, ctx);
        });
        assert_ne!(empty_row, row(&app), "the /agent hint is a different row");
    });
}
