//! #2138 (owner decision 2026-11-11): a prompt suggestion that arrives late moves no visible
//! output row. It is a chip on the editor's first line, so the input is exactly as tall with it
//! as without it. Child of `view_test.rs` (private helpers come in through `super`).
//! Design: `docs/design/r21-chip.md`.

use super::*;
use pathfinder_geometry::rect::RectF;

/// Where one frame laid things out, in window pixels.
struct Frame {
    /// The block list: a visible output row is drawn at this rectangle plus its scroll position,
    /// so a block list rectangle that did not change is rows that did not move.
    block_list: RectF,
    /// The whole input.
    input: RectF,
    /// The editor row of the input.
    editor: RectF,
    /// The prompt-suggestion chip, when one was drawn.
    chip: Option<RectF>,
}

/// Builds one frame of the window from scratch (every view rendered again, a new presenter, so
/// no rectangle is left over from an earlier frame).
fn frame(app: &mut App, window_id: WindowId, terminal: &ViewHandle<TerminalView>) -> Frame {
    let (size_info, block_list_id, input_id, editor_id, chip_id) =
        terminal.read(app, |view, ctx| {
            let input = view.input().as_ref(ctx);
            (
                *view.size_info,
                view.content_element_position_id.clone(),
                input.save_position_id(),
                input.editor_save_position_id(),
                input.suggestion_chip_save_position_id(),
            )
        });
    app.update(move |ctx| {
        let invalidation = WindowInvalidation {
            updated: ctx.view_ids_for_window(window_id).into_iter().collect(),
            ..Default::default()
        };
        let mut presenter = Presenter::new(window_id);
        presenter.invalidate(invalidation, ctx);
        presenter.build_scene(
            vec2f(size_info.pane_width_px, size_info.pane_height_px),
            1.,
            None,
            ctx,
        );
        let positions = presenter.position_cache();
        Frame {
            block_list: positions
                .get_position(&block_list_id)
                .expect("the block list was laid out"),
            input: positions
                .get_position(&input_id)
                .expect("the input was laid out"),
            editor: positions
                .get_position(&editor_id)
                .expect("the editor row was laid out"),
            chip: positions.get_position(&chip_id),
        }
    })
}

fn suggestion(label: &str) -> AgentModePromptSuggestion {
    AgentModePromptSuggestion::Success(PromptSuggestion {
        id: "suggestion".to_owned(),
        label: Some(label.to_owned()),
        prompt: label.to_owned(),
        coding_query_context: None,
        static_prompt_suggestion_name: None,
        should_start_new_conversation: false,
    })
}

/// The chip is drawn, inside the editor row, and takes at most half of the input's width.
fn assert_chip_is_inside_the_editor_row(frame: &Frame, pane_width_px: f32, when: &str) {
    let chip = frame.chip.unwrap_or_else(|| {
        panic!(
            "{when}: no chip was drawn for the suggestion (pane width {pane_width_px} px: with an \
             empty buffer the chip gets half the editor's width and needs 16 cells)"
        )
    });
    assert!(
        chip.width() > 0. && chip.height() > 0.,
        "{when}: the chip has no size: {chip:?}"
    );
    assert!(
        chip.min_x() >= frame.editor.min_x() - 0.5
            && chip.max_x() <= frame.editor.max_x() + 0.5
            && chip.min_y() >= frame.editor.min_y() - 0.5
            && chip.max_y() <= frame.editor.max_y() + 0.5,
        "{when}: the chip {chip:?} is not inside the editor row {:?}",
        frame.editor
    );
    assert!(
        chip.width() <= frame.input.width() * 0.5 + 0.5,
        "{when}: the chip {chip:?} is wider than half the input {:?}",
        frame.input
    );
}

#[test]
fn a_late_prompt_suggestion_is_a_chip_that_moves_neither_the_output_nor_the_input() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let (window_id, terminal) = add_window_with_id_and_terminal(&mut app, None);

        // `TerminalView::new_for_test` builds a 7 x 10.5 px pane (every platform): no editor line
        // that narrow has room for a chip (it needs 16 cells), so the chip was never drawn. Give
        // the pane the size of a real window the way a layout does (#2138, r22c on macOS).
        terminal.update(&mut app, |view, ctx| {
            let update = crate::terminal::view::SizeUpdateBuilder::after_layout(
                *view.size_info,
                vec2f(800., 600.),
            )
            .build(view, ctx);
            view.resize_internal(update, ctx);
            assert_eq!(view.size_info.pane_width_px, 800.);
        });

        let (block_id, pane_width_px) = terminal.update(&mut app, |view, _ctx| {
            let pane_width_px = view.size_info.pane_width_px;
            let mut model = view.model.lock();
            model.simulate_block("ls", "output");
            let last_completed_block_index = BlockIndex(model.block_list().blocks().len() - 2);
            let block_id = model
                .block_list()
                .block_at(last_completed_block_index)
                .unwrap()
                .id()
                .clone();
            (block_id, pane_width_px)
        });

        // The command finished, no suggestion yet.
        let before = frame(&mut app, window_id, &terminal);
        assert!(before.chip.is_none(), "a chip is drawn with no suggestion");

        // Seconds later the model answers.
        terminal.update(&mut app, |view, ctx| {
            view.on_legacy_prompt_suggestion_generated(
                suggestion("Do something"),
                block_id.clone(),
                "ls".to_owned(),
                0,
                ctx,
            );
            let banner = view
                .inline_banners_state
                .prompt_suggestions_banner
                .as_ref()
                .expect("the suggestion is set");
            assert!(!banner.should_hide, "the suggestion shows");
        });
        let showing = frame(&mut app, window_id, &terminal);
        assert_eq!(
            before.block_list, showing.block_list,
            "the block list (every visible output row) moved when the suggestion appeared"
        );
        assert_eq!(
            before.input, showing.input,
            "the input changed its place or height when the suggestion appeared"
        );
        assert_eq!(
            before.editor, showing.editor,
            "the editor row changed its place or height when the suggestion appeared"
        );
        assert_chip_is_inside_the_editor_row(&showing, pane_width_px, "a short suggestion");

        // A suggestion far longer than the line: the chip is cut, nothing grows.
        terminal.update(&mut app, |view, ctx| {
            view.on_legacy_prompt_suggestion_generated(
                suggestion(&"Explain every line of this output in detail ".repeat(12)),
                block_id.clone(),
                "ls".to_owned(),
                0,
                ctx,
            );
        });
        let long = frame(&mut app, window_id, &terminal);
        assert_eq!(
            before.block_list, long.block_list,
            "the block list moved for a long suggestion"
        );
        assert_eq!(
            before.input, long.input,
            "the input changed its place or height for a long suggestion"
        );
        assert_chip_is_inside_the_editor_row(&long, pane_width_px, "a long suggestion");

        // The suggestion goes (Escape, Enter, a view switch all end here).
        terminal.update(&mut app, |view, ctx| {
            view.clear_prompt_suggestions(ctx);
            assert!(
                view.inline_banners_state
                    .prompt_suggestions_banner
                    .is_none()
            );
        });
        let gone = frame(&mut app, window_id, &terminal);
        assert_eq!(
            showing.block_list, gone.block_list,
            "the block list (every visible output row) moved when the suggestion went"
        );
        assert_eq!(
            showing.input, gone.input,
            "the input changed its place or height when the suggestion went"
        );
        assert!(
            gone.chip.is_none(),
            "the chip is still drawn after the suggestion went"
        );
    })
}
