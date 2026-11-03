use std::cell::RefCell;
use std::path::{Path, PathBuf};

use super::*;

#[test]
fn uri_percent_encodes_spaces_and_unicode() {
    assert_eq!(
        file_manager_uri(Path::new("/home/u/my docs/résumé #1.txt")).unwrap(),
        "file:///home/u/my%20docs/r%C3%A9sum%C3%A9%20%231.txt"
    );
}

#[test]
fn uri_rejects_relative_paths() {
    assert!(file_manager_uri(Path::new("rel/file.txt")).is_none());
}

#[test]
fn tilde_is_expanded() {
    let home = Path::new("/home/u");
    assert_eq!(
        expand_tilde(Path::new("~/a b/c.txt"), Some(home)),
        PathBuf::from("/home/u/a b/c.txt")
    );
    assert_eq!(
        expand_tilde(Path::new("~x/c"), Some(home)),
        PathBuf::from("~x/c")
    );
    assert_eq!(expand_tilde(Path::new("~/c"), None), PathBuf::from("~/c"));
}

#[test]
fn explorer_arg_quotes_and_uses_backslashes() {
    assert_eq!(
        explorer_select_arg(Path::new("C:/Users/jo hn/ünï.txt")),
        OsString::from("/select,\"C:\\Users\\jo hn\\ünï.txt\"")
    );
}

#[test]
fn existing_file_is_selected_missing_file_opens_parent() {
    let parent = Path::new("/p");
    let file = Path::new("/p/f.txt");
    assert_eq!(
        decide(file, false, true, Some((parent, true))),
        RevealAction::Select(file.to_path_buf())
    );
    assert_eq!(
        decide(file, false, false, Some((parent, true))),
        RevealAction::Open(parent.to_path_buf())
    );
    assert_eq!(
        decide(parent, true, true, None),
        RevealAction::Open(parent.to_path_buf())
    );
    assert_eq!(
        decide(file, false, true, Some((parent, false))),
        RevealAction::Nothing
    );
    assert_eq!(decide(file, false, true, None), RevealAction::Nothing);
}

#[test]
fn plan_selects_real_file_and_opens_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a b.txt");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(plan(&file), RevealAction::Select(file.clone()));
    assert_eq!(
        plan(dir.path()),
        RevealAction::Open(dir.path().to_path_buf())
    );
    let missing = dir.path().join("nope.txt");
    assert_eq!(plan(&missing), RevealAction::Open(dir.path().to_path_buf()));
}

#[test]
fn show_items_failure_falls_back_to_parent_folder() {
    let opened = RefCell::new(Vec::new());
    execute(
        RevealAction::Select(PathBuf::from("/p/f.txt")),
        |_| Err(ShowItemsError::Failed("no such service".into())),
        |d| opened.borrow_mut().push(d.to_path_buf()),
    );
    assert_eq!(*opened.borrow(), vec![PathBuf::from("/p")]);
}

#[test]
fn show_items_success_does_not_open_folder() {
    let selected = RefCell::new(Vec::new());
    execute(
        RevealAction::Select(PathBuf::from("/p/f.txt")),
        |p| {
            selected.borrow_mut().push(p.to_path_buf());
            Ok(())
        },
        |_| panic!("should not open folder"),
    );
    assert_eq!(selected.borrow().len(), 1);
}

#[test]
fn show_items_timeout_does_not_open_second_window() {
    execute(
        RevealAction::Select(PathBuf::from("/p/f.txt")),
        |_| Err(ShowItemsError::TimedOut),
        |_| panic!("a timeout must not fall back to opening the folder"),
    );
}

#[test]
fn fallback_decision() {
    assert!(should_fall_back(&Err(ShowItemsError::Failed(
        "org.freedesktop.DBus.Error.ServiceUnknown".into()
    ))));
    assert!(!should_fall_back(&Err(ShowItemsError::TimedOut)));
    assert!(!should_fall_back(&Ok(())));
}
