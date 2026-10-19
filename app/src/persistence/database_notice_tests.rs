use super::*;

#[test]
fn full_disk_and_read_only_messages_are_classified() {
    let path = Path::new("/nonexistent/dir/warp.sqlite");
    assert_eq!(
        classify_database_problem("database or disk is full", path),
        DatabaseProblem::DiskFull
    );
    assert_eq!(
        classify_database_problem("no space left on device (os error 28)", path),
        DatabaseProblem::DiskFull
    );
    assert_eq!(
        classify_database_problem("attempt to write a readonly database", path),
        DatabaseProblem::ReadOnly
    );
    assert!(matches!(
        classify_database_problem("something else entirely", path),
        DatabaseProblem::Other(message) if message == "something else entirely"
    ));
}

#[cfg(unix)]
#[test]
fn unable_to_open_in_a_read_only_folder_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("warp.sqlite");
    std::fs::write(&db, b"").unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(dir.path().join("probe"), b"").is_ok() {
        // Running as root: permissions do not apply, nothing to check.
        return;
    }
    let problem = classify_database_problem("unable to open database file", &db);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(problem, DatabaseProblem::ReadOnly);
}

#[cfg(unix)]
#[test]
fn unable_to_open_a_read_only_file_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("warp.sqlite");
    std::fs::write(&db, b"").unwrap();
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o444)).unwrap();
    if std::fs::OpenOptions::new().write(true).open(&db).is_ok() {
        // Running as root: permissions do not apply, nothing to check.
        return;
    }
    let problem = classify_database_problem("unable to open database file", &db);
    assert_eq!(problem, DatabaseProblem::ReadOnly);
}

#[test]
fn unable_to_open_in_a_writable_folder_is_not_called_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("warp.sqlite");
    assert!(matches!(
        classify_database_problem("unable to open database file", &db),
        DatabaseProblem::Other(_)
    ));
}

/// One test owns the statics so parallel tests in one process cannot race.
#[test]
fn notices_are_recorded_once_and_taken_once() {
    let db = Path::new("/nonexistent/dir/warp.sqlite");
    let _ = take_database_problem_notice();

    record_startup_failure(db, "attempt to write a readonly database");
    let notice = take_database_problem_notice().expect("startup failure is noticed");
    assert_eq!(notice.problem, DatabaseProblem::ReadOnly);
    assert!(notice.persistence_disabled);
    assert!(take_database_problem_notice().is_none(), "taken once");

    // A failing disk is reported on the transition, not on every event.
    record_write_outcome(db, Some("database or disk is full"));
    let notice = take_database_problem_notice().expect("first full-disk write is noticed");
    assert_eq!(notice.problem, DatabaseProblem::DiskFull);
    assert!(!notice.persistence_disabled);
    record_write_outcome(db, Some("database or disk is full"));
    assert!(take_database_problem_notice().is_none());

    // Recovery re-arms it.
    record_write_outcome(db, None);
    record_write_outcome(db, Some("database or disk is full"));
    assert!(take_database_problem_notice().is_some());
    record_write_outcome(db, None);

    // Errors the user cannot act on are not surfaced.
    record_write_outcome(db, Some("no such column: x"));
    assert!(take_database_problem_notice().is_none());
}

/// #860: a notice recorded from the writer thread wakes the UI, which then
/// finds it in the slot.
#[test]
fn recording_a_write_failure_wakes_the_ui_and_leaves_the_notice() {
    let wake = take_notice_wake_receiver().expect("the receiver is available once");
    assert!(take_notice_wake_receiver().is_none());
    let db = Path::new("/nonexistent/dir/wake.sqlite");
    record_write_outcome(db, None);
    assert!(
        wake.try_recv().is_err(),
        "nothing recorded, nothing to wake"
    );

    record_write_outcome(db, Some("database or disk is full"));
    assert!(wake.try_recv().is_ok());
    let notice = take_database_problem_notice().expect("the notice is pending");
    assert_eq!(notice.problem, DatabaseProblem::DiskFull);
    assert!(!notice.persistence_disabled);
    record_write_outcome(db, None);
}
