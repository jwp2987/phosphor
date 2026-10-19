//! What the user is told when `warp.sqlite` cannot be used or written (#860).
//!
//! A read-only database (file or folder), a full disk and any other failure to
//! open the database all used to leave the app running without saving
//! anything, with the reason only in the log. None of them is a reason to move
//! the database aside -- the data is fine, the environment is not -- so the
//! only remedy is to say so.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Why the database cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseProblem {
    /// The file or the folder holding it is read-only.
    ReadOnly,
    /// The disk holding the database is full.
    DiskFull,
    /// Anything else; carries SQLite's or the OS's message.
    Other(String),
}

/// A problem with the database that the UI should tell the user about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseProblemNotice {
    pub path: PathBuf,
    pub problem: DatabaseProblem,
    /// `true` when the database could not be opened at all and this session
    /// runs without saving state; `false` when it opened but writes fail.
    pub persistence_disabled: bool,
}

static PROBLEM_NOTICE: Mutex<Option<DatabaseProblemNotice>> = Mutex::new(None);

/// Whether the previous write failed for a reason in [`DatabaseProblem`], so
/// a failing disk is reported once rather than on every event.
static WRITES_FAILING: AtomicBool = AtomicBool::new(false);

/// Whether the file at `db_path`, or the folder it lives in, rejects writes
/// from this process.
fn location_rejects_writes(db_path: &Path) -> bool {
    use std::io::ErrorKind;
    let rejected = |kind: ErrorKind| {
        matches!(
            kind,
            ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
        )
    };
    if db_path.metadata().is_ok_and(|m| m.is_file()) {
        if db_path.metadata().is_ok_and(|m| m.permissions().readonly()) {
            return true;
        }
        if let Err(err) = std::fs::OpenOptions::new().write(true).open(db_path) {
            if rejected(err.kind()) {
                return true;
            }
        }
    }
    let Some(parent) = db_path.parent() else {
        return false;
    };
    if parent.metadata().is_ok_and(|m| m.permissions().readonly()) {
        return true;
    }
    let probe = parent.join(format!(".phosphor-write-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            false
        }
        Err(err) => rejected(err.kind()),
    }
}

/// Classifies a lowercased SQLite or OS error message about the database at
/// `db_path`.
pub fn classify_database_problem(message: &str, db_path: &Path) -> DatabaseProblem {
    const READ_ONLY: &[&str] = &["readonly database", "read-only", "read only"];
    const FULL: &[&str] = &["database or disk is full", "no space left", "disk full"];
    const OPEN_FAILURES: &[&str] = &[
        "unable to open database file",
        "permission denied",
        "cannot open",
    ];
    if READ_ONLY.iter().any(|needle| message.contains(needle)) {
        DatabaseProblem::ReadOnly
    } else if FULL.iter().any(|needle| message.contains(needle)) {
        DatabaseProblem::DiskFull
    } else if OPEN_FAILURES.iter().any(|needle| message.contains(needle))
        && location_rejects_writes(db_path)
    {
        DatabaseProblem::ReadOnly
    } else {
        DatabaseProblem::Other(message.to_owned())
    }
}

fn set_notice(notice: DatabaseProblemNotice) {
    if let Ok(mut slot) = PROBLEM_NOTICE.lock() {
        *slot = Some(notice);
    }
}

/// Records that the database could not be opened, so this session runs
/// without persistence. `message` is the lowercased root-cause message.
pub fn record_startup_failure(db_path: &Path, message: &str) {
    set_notice(DatabaseProblemNotice {
        path: db_path.to_path_buf(),
        problem: classify_database_problem(message, db_path),
        persistence_disabled: true,
    });
}

/// Records the outcome of one write by the SQLite writer. Only a read-only or
/// full database is reported (once per failing stretch); other write errors
/// are bugs for the log, not something the user can act on.
pub fn record_write_outcome(db_path: &Path, failure_message: Option<&str>) {
    let Some(message) = failure_message else {
        WRITES_FAILING.store(false, Ordering::Relaxed);
        return;
    };
    let problem = classify_database_problem(message, db_path);
    if matches!(problem, DatabaseProblem::Other(_)) {
        return;
    }
    if !WRITES_FAILING.swap(true, Ordering::Relaxed) {
        set_notice(DatabaseProblemNotice {
            path: db_path.to_path_buf(),
            problem,
            persistence_disabled: false,
        });
    }
}

/// Returns (once) the pending notice, so the UI can show it.
pub fn take_database_problem_notice() -> Option<DatabaseProblemNotice> {
    PROBLEM_NOTICE.lock().ok().and_then(|mut slot| slot.take())
}

#[cfg(test)]
#[path = "database_notice_tests.rs"]
mod tests;
