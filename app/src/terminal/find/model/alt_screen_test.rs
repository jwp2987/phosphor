use crate::terminal::{
    find::model::{alt_screen::run_find_on_alt_screen, FindOptions},
    model::index::Point,
    TerminalModel,
};
use crate::view_components::find::FindDirection;

#[test]
fn test_run_find_on_alt_screen() {
    let mut mock_terminal_model = TerminalModel::mock(None, None);
    mock_terminal_model.set_altscreen_active();
    mock_terminal_model.process_bytes("foo\r\nbar foo\r\nfoo");

    let run = run_find_on_alt_screen(
        FindOptions {
            query: Some("foo".to_owned().into()),
            is_regex_enabled: false,
            is_case_sensitive: false,
            ..Default::default()
        },
        mock_terminal_model.alt_screen(),
    );

    assert_eq!(
        run.matches(),
        &[
            Point { row: 2, col: 0 }..=Point { row: 2, col: 2 },
            Point { row: 1, col: 4 }..=Point { row: 1, col: 6 },
            Point { row: 0, col: 0 }..=Point { row: 0, col: 2 },
        ]
    );
    assert_eq!(run.focused_match_index(), Some(0));
}

#[test]
fn test_run_find_on_alt_screen_case_sensitive() {
    let mut mock_terminal_model = TerminalModel::mock(None, None);
    mock_terminal_model.set_altscreen_active();
    mock_terminal_model.process_bytes("foo\r\nbar foo\r\nFoo");

    let run = run_find_on_alt_screen(
        FindOptions {
            query: Some("Foo".to_owned().into()),
            is_regex_enabled: false,
            is_case_sensitive: true,
            ..Default::default()
        },
        mock_terminal_model.alt_screen(),
    );

    assert_eq!(
        run.matches(),
        &[Point { row: 2, col: 0 }..=Point { row: 2, col: 2 },]
    );
    assert_eq!(run.focused_match_index(), Some(0));
}

#[test]
fn test_run_find_on_alt_screen_regex() {
    let mut mock_terminal_model = TerminalModel::mock(None, None);
    mock_terminal_model.set_altscreen_active();
    mock_terminal_model.process_bytes("aoo\r\nbar foo\r\nboo");

    let run = run_find_on_alt_screen(
        FindOptions {
            query: Some("[ab]oo".to_owned().into()),
            is_regex_enabled: true,
            is_case_sensitive: false,
            ..Default::default()
        },
        mock_terminal_model.alt_screen(),
    );

    assert_eq!(
        run.matches(),
        &[
            Point { row: 2, col: 0 }..=Point { row: 2, col: 2 },
            Point { row: 0, col: 0 }..=Point { row: 0, col: 2 },
        ]
    );
    assert_eq!(run.focused_match_index(), Some(0));
}

#[test]
fn test_rerun_reports_a_change_only_when_the_results_differ() {
    let mut mock_terminal_model = TerminalModel::mock(None, None);
    mock_terminal_model.set_altscreen_active();
    mock_terminal_model.process_bytes("foo\r\nbar foo\r\nfoo");

    let run = run_find_on_alt_screen(
        FindOptions {
            query: Some("foo".to_owned().into()),
            ..Default::default()
        },
        mock_terminal_model.alt_screen(),
    );
    assert_eq!(run.matches().len(), 3);

    // Same screen, same results: not a change (#815).
    let (run, changed) = run.rerun(mock_terminal_model.alt_screen());
    assert!(!changed, "identical results are not a change");
    assert_eq!(run.matches().len(), 3);
    assert_eq!(run.focused_match_index(), Some(0));

    // A focus moved by the user survives a rerun and is not reported by it.
    let mut run = run;
    run.focus_next_match(FindDirection::Up);
    assert_eq!(run.focused_match_index(), Some(1));
    let (run, changed) = run.rerun(mock_terminal_model.alt_screen());
    assert!(!changed, "a rerun that keeps the focus is not a change");
    assert_eq!(run.focused_match_index(), Some(1));

    // A new match: the count differs.
    mock_terminal_model.process_bytes("\r\nfoo");
    let (run, changed) = run.rerun(mock_terminal_model.alt_screen());
    assert!(changed, "a different match count is a change");
    assert_eq!(run.matches().len(), 4);

    // Same count, different position (the last line becomes "xfoo"): the
    // highlight moves, so this is a change too.
    mock_terminal_model.process_bytes("\rxfoo");
    let (run, changed) = run.rerun(mock_terminal_model.alt_screen());
    assert!(changed, "a moved match is a change");
    assert_eq!(run.matches().len(), 4);
    assert_eq!(
        run.matches()[0],
        Point { row: 3, col: 1 }..=Point { row: 3, col: 3 }
    );
}
