use markdown_parser::{FormattedImage, FormattedText, FormattedTextLine};

use super::*;

/// A base64 `data:` image whose payload exceeds the asset layer's render limit.
fn oversized_data_uri_image() -> FormattedImage {
    let payload = "A".repeat(asset_cache::MAX_DATA_URI_PAYLOAD_BYTES + 1);
    FormattedImage {
        alt_text: "output".to_string(),
        source: format!("data:image/png;base64,{payload}"),
        title: None,
    }
}

#[test]
fn replace_oversized_data_uri_images_swaps_in_placeholder() {
    // An in-limit image that must be left untouched.
    let small = FormattedImage {
        alt_text: "output".to_string(),
        source: "data:image/png;base64,iVBORw0KGgo=".to_string(),
        title: None,
    };

    let text = FormattedText::new(vec![
        FormattedTextLine::Image(oversized_data_uri_image()),
        FormattedTextLine::Image(small.clone()),
    ]);

    let result = replace_oversized_data_uri_images(text);
    let lines: Vec<_> = result.lines.into_iter().collect();

    // The oversized image becomes a visible placeholder text line ...
    assert!(matches!(&lines[0], FormattedTextLine::Line(_)));
    assert_eq!(
        lines[0].raw_text(),
        format!("{IMAGE_TOO_LARGE_PLACEHOLDER}\n")
    );

    // ... while an in-limit image is left untouched.
    assert_eq!(lines[1], FormattedTextLine::Image(small));
}

/// #2100: the trace lines of a core edit hold the editor's text only with the text opt-in and
/// never during a secret read.
mod log_lines_withhold_text {
    use warpui::input_order::{TextShown, assert_line_withholds};

    use super::super::{CoreEditorActionType, log_lines};

    #[test]
    fn the_applying_and_reverse_action_lines() {
        let action = CoreEditorActionType::StyleLink("https://example.test/hunter2pw".to_string());
        let hidden = TextShown::fixed(false, false);
        assert_eq!(
            log_lines::applying(&action, hidden),
            "Applying StyleLink<34 chars>"
        );
        assert_eq!(
            log_lines::reverse_action(&action, hidden),
            "=> Reverse action: StyleLink<34 chars>"
        );
        assert_line_withholds(|shown| log_lines::applying(&action, shown), &["hunter2pw"]);
        assert_line_withholds(
            |shown| log_lines::reverse_action(&action, shown),
            &["hunter2pw"],
        );
        // An action with no text keeps its whole name.
        assert_eq!(
            log_lines::applying(&CoreEditorActionType::UnstyleLink, hidden),
            "Applying UnstyleLink"
        );
    }

    #[test]
    fn the_buffer_dump_lines() {
        let dump = "<text>my pin is hunter2pw";
        assert_eq!(
            log_lines::buffer_dump("Initial", dump, TextShown::fixed(false, false)),
            "=> Initial buffer: [<25 chars>]"
        );
        assert_eq!(
            log_lines::buffer_dump("Updated", dump, TextShown::fixed(true, false)),
            "=> Updated buffer: [<text>my pin is hunter2pw]"
        );
        assert_line_withholds(
            |shown| log_lines::buffer_dump("Initial", dump, shown),
            &["hunter2pw", "my pin"],
        );
    }
}
