use std::iter;

use warp_util::path::CleanPathResult;

use super::super::TerminalView;
use super::{GridHighlightedLink, path_without_trailing_sentence_punctuation};
use crate::terminal::model::grid::grid_handler::PossiblePath;
use crate::terminal::model::index::Point;
use crate::terminal::model::terminal_model::WithinModel;
use crate::util::file::LinkValidationContext;

#[test]
fn strips_only_sentence_periods() {
    // A trailing period after a real file name is sentence punctuation.
    assert_eq!(
        path_without_trailing_sentence_punctuation("notes/README.md.").map(|trimmed| trimmed.path),
        Some("notes/README.md")
    );
    assert_eq!(
        path_without_trailing_sentence_punctuation(".gitignore.").map(|trimmed| trimmed.path),
        Some(".gitignore")
    );
    assert_eq!(
        path_without_trailing_sentence_punctuation("C:/Users/c/warp-md-test.md.")
            .map(|trimmed| trimmed.path),
        Some("C:/Users/c/warp-md-test.md")
    );

    // No trailing period -> nothing to trim.
    assert_eq!(
        path_without_trailing_sentence_punctuation("notes/README.md").map(|trimmed| trimmed.path),
        None
    );

    // `.`/`..` path components must be preserved, not treated as punctuation.
    assert_eq!(
        path_without_trailing_sentence_punctuation(".").map(|trimmed| trimmed.path),
        None
    );
    assert_eq!(
        path_without_trailing_sentence_punctuation("..").map(|trimmed| trimmed.path),
        None
    );
    assert_eq!(
        path_without_trailing_sentence_punctuation("foo/.").map(|trimmed| trimmed.path),
        None
    );
    assert_eq!(
        path_without_trailing_sentence_punctuation("foo/..").map(|trimmed| trimmed.path),
        None
    );
    assert_eq!(
        path_without_trailing_sentence_punctuation("foo..").map(|trimmed| trimmed.path),
        None
    );
}

#[test]
fn strips_trailing_fullwidth_sentence_punctuation() {
    let trimmed = path_without_trailing_sentence_punctuation("notes/README.md，")
        .expect("fullwidth comma should be stripped");
    assert_eq!(trimmed.path, "notes/README.md");
    assert_eq!(trimmed.removed_width, 2);

    let trimmed = path_without_trailing_sentence_punctuation("notes/README.md。！？")
        .expect("CJK sentence punctuation should be stripped");
    assert_eq!(trimmed.path, "notes/README.md");
    assert_eq!(trimmed.removed_width, 6);
}

// Regression test for https://github.com/warpdotdev/warp/issues/11477:
// a `.md` path at the end of a sentence captured the trailing period, so the
// resolved file and the highlight range ended in `.md.` and the file failed
// markdown classification. The trailing period must be excluded from both.
#[test]
fn compute_valid_paths_excludes_trailing_sentence_period() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("warp-md-test.md");
    std::fs::write(&file, "# Hello\n").unwrap();

    // The captured token as it would appear in `Drafted at <abs path>.`
    let token = format!("{}.", file.to_string_lossy());
    let end_col = token.chars().count() - 1;
    let candidate = WithinModel::AltScreen(PossiblePath {
        path: CleanPathResult {
            path: token,
            line_and_column_num: None,
        },
        range: Point { row: 0, col: 0 }..=Point {
            row: 0,
            col: end_col,
        },
    });

    let link = TerminalView::compute_valid_paths(
        dir.path().to_str().unwrap(),
        iter::once(candidate),
        1000,
        None,
        LinkValidationContext::Local,
    )
    .expect("the markdown file should be detected as a link");

    let GridHighlightedLink::File(file_link) = link else {
        panic!("expected a file link");
    };
    let file_link = file_link.get_inner();

    // The resolved file excludes the trailing period (so it classifies as `.md`)...
    assert_eq!(
        file_link.absolute_path.file_name().unwrap(),
        "warp-md-test.md"
    );
    // ...and the highlighted range stops before the trailing period.
    assert_eq!(
        *file_link.link.range().end(),
        Point {
            row: 0,
            col: end_col - 1,
        }
    );
}

#[test]
fn compute_valid_paths_excludes_trailing_fullwidth_sentence_punctuation() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("warp-md-test.md");
    std::fs::write(&file, "# Hello\n").unwrap();

    let token = format!("{}，", file.to_string_lossy());
    let punctuation_width = 2;
    let end_col = token.chars().count();
    let candidate = WithinModel::AltScreen(PossiblePath {
        path: CleanPathResult {
            path: token,
            line_and_column_num: None,
        },
        range: Point { row: 0, col: 0 }..=Point {
            row: 0,
            col: end_col,
        },
    });

    let link = TerminalView::compute_valid_paths(
        dir.path().to_str().unwrap(),
        iter::once(candidate),
        1000,
        None,
        LinkValidationContext::Local,
    )
    .expect("the markdown file should be detected as a link");

    let GridHighlightedLink::File(file_link) = link else {
        panic!("expected a file link");
    };
    let file_link = file_link.get_inner();

    assert_eq!(
        file_link.absolute_path.file_name().unwrap(),
        "warp-md-test.md"
    );
    assert_eq!(
        *file_link.link.range().end(),
        Point {
            row: 0,
            col: end_col - punctuation_width,
        }
    );
}

#[test]
fn compute_valid_paths_keeps_trailing_fullwidth_punctuation_when_it_is_the_filename() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("warp-md-test.md，");
    std::fs::write(&file, "# Hello\n").unwrap();

    let token = file.to_string_lossy().to_string();
    let end_col = token.chars().count();
    let candidate = WithinModel::AltScreen(PossiblePath {
        path: CleanPathResult {
            path: token,
            line_and_column_num: None,
        },
        range: Point { row: 0, col: 0 }..=Point {
            row: 0,
            col: end_col,
        },
    });

    let link = TerminalView::compute_valid_paths(
        dir.path().to_str().unwrap(),
        iter::once(candidate),
        1000,
        None,
        LinkValidationContext::Local,
    )
    .expect("the file with fullwidth punctuation should be detected as a link");

    let GridHighlightedLink::File(file_link) = link else {
        panic!("expected a file link");
    };
    let file_link = file_link.get_inner();

    assert_eq!(
        file_link.absolute_path.file_name().unwrap(),
        "warp-md-test.md，"
    );
    assert_eq!(
        *file_link.link.range().end(),
        Point {
            row: 0,
            col: end_col,
        }
    );
}

#[test]
fn new_hover_cancels_previous_file_link_scan() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let (abort, registration) = futures::future::AbortHandle::new_pair();
    let cancelled = Arc::new(AtomicBool::new(false));
    let scan = super::FileLinkScan::new(abort.clone(), cancelled.clone());
    assert!(!abort.is_aborted());
    assert!(!cancelled.load(Ordering::Relaxed));

    scan.cancel();

    // The async wrapper is aborted and a blocking scan already running is told to stop.
    assert!(abort.is_aborted());
    assert!(cancelled.load(Ordering::Relaxed));
    let aborted = futures::executor::block_on(futures::future::Abortable::new(
        futures::future::pending::<()>(),
        registration,
    ));
    assert!(aborted.is_err());
}

fn alt_candidate(token: &str, line: Option<(usize, Option<usize>)>) -> WithinModel<PossiblePath> {
    WithinModel::AltScreen(PossiblePath {
        path: CleanPathResult {
            path: token.to_string(),
            line_and_column_num: line.map(|(line_num, column_num)| {
                warp_util::path::LineAndColumnArg {
                    line_num,
                    column_num,
                }
            }),
        },
        range: Point { row: 3, col: 2 }..=Point {
            row: 3,
            col: 2 + token.chars().count() - 1,
        },
    })
}

// #1347: paths printed by a full-screen TUI (alt-screen candidates) resolve against the session
// cwd, keep their :line:col, and produce alt-screen links.
#[test]
fn alt_screen_relative_path_resolves_against_cwd_with_line_and_column() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();

    let link = TerminalView::compute_valid_paths(
        dir.path().to_str().unwrap(),
        iter::once(alt_candidate("src/main.rs", Some((12, Some(4))))),
        1000,
        None,
        LinkValidationContext::Local,
    )
    .expect("relative alt-screen path should be detected");

    let GridHighlightedLink::File(file_link) = link else {
        panic!("expected a file link");
    };
    assert!(matches!(file_link, WithinModel::AltScreen(_)));
    let file_link = file_link.get_inner();
    assert!(file_link.absolute_path.ends_with("src/main.rs"));
    assert!(file_link.absolute_path.is_absolute());
    let line = file_link.line_and_column_num.expect("line and column kept");
    assert_eq!((line.line_num, line.column_num), (12, Some(4)));
}

#[test]
fn alt_screen_absolute_path_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    let link = TerminalView::compute_valid_paths(
        "/",
        iter::once(alt_candidate(file.to_str().unwrap(), None)),
        1000,
        None,
        LinkValidationContext::Local,
    );
    assert!(matches!(link, Some(GridHighlightedLink::File(_))));
}

#[test]
fn path_menu_has_open_reveal_and_copy_locally() {
    use crate::terminal::view::TerminalAction;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "x").unwrap();
    let link = TerminalView::compute_valid_paths(
        dir.path().to_str().unwrap(),
        iter::once(alt_candidate("a.txt", None)),
        1000,
        None,
        LinkValidationContext::Local,
    )
    .unwrap();

    let items = super::alt_screen_link_menu_items(&link, None, false);
    let actions: Vec<_> = items
        .iter()
        .filter_map(|item| item.fields()?.on_select_action().cloned())
        .collect();
    assert_eq!(actions.len(), 3);
    assert!(matches!(actions[0], TerminalAction::OpenGridLink(_)));
    assert!(matches!(actions[1], TerminalAction::ShowInFileExplorer(_)));
    assert!(matches!(actions[2], TerminalAction::ContextMenu(_)));

    // Remote session: a local reveal would show the wrong machine, so only Open + Copy.
    let remote = super::alt_screen_link_menu_items(&link, None, true);
    let remote_actions: Vec<_> = remote
        .iter()
        .filter_map(|item| item.fields()?.on_select_action().cloned())
        .collect();
    assert_eq!(remote_actions.len(), 2);
    assert!(
        !remote_actions
            .iter()
            .any(|a| matches!(a, TerminalAction::ShowInFileExplorer(_)))
    );
}
