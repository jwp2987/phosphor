use warpui::input_order::{TextShown, assert_line_withholds};

/// #2100: the transcription line holds the dictated text only with the text opt-in and never
/// during a secret read.
#[test]
fn the_transcription_line_withholds_the_dictated_text() {
    let line = |shown| super::transcribed_voice_input_line("my pin is hunter2pw", shown);
    assert_eq!(
        line(TextShown::fixed(false, false)),
        "Transcribed voice input: <19 chars>"
    );
    assert_line_withholds(line, &["hunter2pw", "my pin"]);
}
