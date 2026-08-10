//! Behaviour pins for [`super::all_working_directories`].
//!
//! There were no tests over this function while it lived privately in
//! `ai/outline/native.rs`. It feeds directory-scoped features, so a silent
//! change to which directories it reports would be hard to trace back to
//! here -- these tests exist to make such a change loud.
//!
//! Driving a real `TerminalView` all the way to a non-`None` `pwd()` means
//! synthesising active block metadata through the model/OSC-7 path, which is a
//! much larger harness than this function warrants. The set-building rules are
//! therefore pinned through the `insert_working_directory` seam, and the whole
//! function is exercised against a real (empty) app.

use std::collections::HashSet;
use std::path::PathBuf;

use warpui::App;

use super::{all_working_directories, insert_working_directory};

/// A terminal with no working directory yet contributes nothing -- in
/// particular it must not contribute an empty path.
#[test]
fn views_without_a_working_directory_are_skipped() {
    let mut working_directories = HashSet::new();
    insert_working_directory(&mut working_directories, None);
    assert!(working_directories.is_empty());

    insert_working_directory(&mut working_directories, Some("/home/zap".to_string()));
    insert_working_directory(&mut working_directories, None);
    assert_eq!(
        working_directories,
        HashSet::from([PathBuf::from("/home/zap")])
    );
}

/// The same directory open in several terminals is reported once.
#[test]
fn repeated_directories_are_deduplicated() {
    let mut working_directories = HashSet::new();
    for _ in 0..3 {
        insert_working_directory(&mut working_directories, Some("/home/zap".to_string()));
    }
    assert_eq!(
        working_directories,
        HashSet::from([PathBuf::from("/home/zap")])
    );
}

/// Distinct directories all survive, and each path is the terminal's string
/// taken verbatim -- not canonicalized, not resolved, not made absolute, and
/// not filtered to paths that exist locally.
#[test]
fn distinct_directories_are_kept_verbatim() {
    let mut working_directories = HashSet::new();
    insert_working_directory(&mut working_directories, Some("/home/zap".to_string()));
    insert_working_directory(&mut working_directories, Some("/home/zap/src".to_string()));
    insert_working_directory(&mut working_directories, Some("relative/dir".to_string()));

    assert_eq!(
        working_directories,
        HashSet::from([
            PathBuf::from("/home/zap"),
            PathBuf::from("/home/zap/src"),
            PathBuf::from("relative/dir"),
        ])
    );
}

/// An app with no windows at all yields an empty set rather than panicking.
#[test]
fn no_windows_yields_no_directories() {
    App::test((), |app| async move {
        assert!(app.read(|ctx| all_working_directories(ctx)).is_empty());
    })
}
