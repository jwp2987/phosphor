use warpui::input_order::{TextShown, assert_line_withholds};

/// #2100: the parse line holds the words of the buffer only with the text opt-in and never
/// during a secret read.
#[test]
fn the_parsing_line_withholds_the_words_of_the_buffer() {
    #[derive(Debug)]
    struct LiteCommand {
        _parts: Vec<&'static str>,
    }
    let command = LiteCommand {
        _parts: vec!["mysql", "-phunter2pw"],
    };
    let line = |shown| super::parsing_internal_command_line(&command, shown);
    assert_eq!(
        line(TextShown::fixed(false, false)),
        "parsing internal command LiteCommand<37 chars>"
    );
    assert_line_withholds(line, &["hunter2pw", "mysql"]);
}
