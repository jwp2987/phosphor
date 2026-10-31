use std::time::Duration;

use super::FileBackedUserPreferences;
use crate::user_preferences::UserPreferences;

/// A debounce long enough that only an explicit flush or drop can put data on disk.
const NEVER: Duration = Duration::from_secs(3600);

fn background_store(dir: &tempfile::TempDir) -> (FileBackedUserPreferences, std::path::PathBuf) {
    let path = dir.path().join("user_preferences.json");
    let store = FileBackedUserPreferences::new(path.clone())
        .unwrap()
        .with_background_writer_debounce(NEVER);
    (store, path)
}

#[test]
fn writes_do_not_hit_the_disk_on_the_calling_thread() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = background_store(&dir);
    store.write_value("k", "\"v\"".into()).unwrap();
    // The old code wrote (and fsynced) before returning.
    assert!(!path.exists(), "write_value must only queue the write");
    store.flush_pending().unwrap();
    assert!(path.exists());
}

#[test]
fn a_read_sees_the_write_before_it_reaches_the_disk() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = background_store(&dir);
    store.write_value("k", "1".into()).unwrap();
    store.write_value("k", "2".into()).unwrap();
    assert_eq!(store.read_value("k").unwrap().as_deref(), Some("2"));
    store.remove_value("k").unwrap();
    assert_eq!(store.read_value("k").unwrap(), None);
    assert!(!path.exists());
}

#[test]
fn a_burst_is_coalesced_and_the_latest_value_wins() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = background_store(&dir);
    for i in 0..200 {
        store.write_value("k", i.to_string()).unwrap();
    }
    store.flush_pending().unwrap();
    let reloaded = FileBackedUserPreferences::new(path).unwrap();
    assert_eq!(reloaded.read_value("k").unwrap().as_deref(), Some("199"));
}

#[test]
fn flush_on_drop_never_loses_a_pending_write() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = background_store(&dir);
    store.write_value("a", "\"x\"".into()).unwrap();
    store.write_value("b", "\"y\"".into()).unwrap();
    drop(store);
    let reloaded = FileBackedUserPreferences::new(path).unwrap();
    assert_eq!(reloaded.read_value("a").unwrap().as_deref(), Some("\"x\""));
    assert_eq!(reloaded.read_value("b").unwrap().as_deref(), Some("\"y\""));
}

#[test]
fn flush_pending_writes_trait_method_puts_data_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = background_store(&dir);
    store.write_value("k", "7".into()).unwrap();
    UserPreferences::flush_pending_writes(&store);
    assert!(std::fs::read_to_string(path).unwrap().contains("\"k\""));
}

#[test]
fn an_unchanged_value_is_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = background_store(&dir);
    store.write_value("k", "1".into()).unwrap();
    store.flush_pending().unwrap();
    std::fs::remove_file(&path).unwrap();
    store.write_value("k", "1".into()).unwrap();
    store.flush_pending().unwrap();
    // The old code rewrote the file on every call, even for the same value.
    assert!(!path.exists(), "an identical write must not touch the disk");
    store.write_value("k", "2".into()).unwrap();
    store.flush_pending().unwrap();
    assert!(path.exists());
}

#[test]
fn removing_an_absent_key_is_not_a_write() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = background_store(&dir);
    store.remove_value("missing").unwrap();
    store.flush_pending().unwrap();
    assert!(!path.exists());
}

#[test]
fn the_inline_store_still_writes_before_returning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("user_preferences.json");
    let store = FileBackedUserPreferences::new(path.clone()).unwrap();
    store.write_value("k", "1".into()).unwrap();
    assert!(path.exists());
}
