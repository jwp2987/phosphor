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
    /// SQLite says the file is damaged (`SQLITE_CORRUPT` 11, `SQLITE_NOTADB`
    /// 26, or a table that has gone missing) while the app is running (#911).
    Corrupt,
    /// The file was deleted, or replaced by something else, while the app was
    /// running. The open connection now points at an unlinked file, so
    /// nothing written to it can ever be read back (#911).
    FileReplaced,
    /// Part of the saved state could not be restored at startup (#911): a
    /// missing table, or tabs whose rows do not read back. `skipped_tabs` is
    /// 0 when the whole read failed.
    PartialRestore { skipped_tabs: usize },
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

/// Pending notices, oldest first. A queue rather than one slot so a restore
/// notice from startup is not overwritten by a later write failure.
static PROBLEM_NOTICE: Mutex<Vec<DatabaseProblemNotice>> = Mutex::new(Vec::new());

/// Whether the previous write failed for a reason in [`DatabaseProblem`], so
/// a failing disk is reported once rather than on every event.
static WRITES_FAILING: AtomicBool = AtomicBool::new(false);

/// Set once a damaged database has been reported. Unlike a full disk, damage
/// does not heal, and a damaged database fails some events (blocks) while
/// others (snapshots) succeed, so a success must not re-arm the notice.
static CORRUPTION_REPORTED: AtomicBool = AtomicBool::new(false);

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
        if let Err(err) = std::fs::OpenOptions::new().write(true).open(db_path)
            && rejected(err.kind())
        {
            return true;
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
    const CORRUPT: &[&str] = &[
        "database disk image is malformed",
        "file is not a database",
        "malformed database schema",
        "no such table",
    ];
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
    } else if CORRUPT.iter().any(|needle| message.contains(needle)) {
        DatabaseProblem::Corrupt
    } else if OPEN_FAILURES.iter().any(|needle| message.contains(needle))
        && location_rejects_writes(db_path)
    {
        DatabaseProblem::ReadOnly
    } else {
        DatabaseProblem::Other(message.to_owned())
    }
}

/// Wakes the UI when a notice is recorded after startup (#860). The notice
/// slot is written from the SQLite writer thread, which cannot reach the UI.
static NOTICE_WAKE: Mutex<Option<async_channel::Sender<()>>> = Mutex::new(None);

/// Returns the receiver that gets a message whenever a notice is recorded, once.
/// The channel holds at most one message, so a burst wakes the UI once.
pub fn take_notice_wake_receiver() -> Option<async_channel::Receiver<()>> {
    let mut slot = NOTICE_WAKE.lock().ok()?;
    if slot.is_some() {
        return None;
    }
    let (tx, rx) = async_channel::bounded(1);
    *slot = Some(tx);
    Some(rx)
}

fn set_notice(notice: DatabaseProblemNotice) {
    if let Ok(mut pending) = PROBLEM_NOTICE.lock() {
        pending.push(notice);
    }
    if let Ok(wake) = NOTICE_WAKE.lock() && let Some(tx) = wake.as_ref() {
        let _ = tx.try_send(());
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

/// What the writer should do about the log after one failed write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteFailureLog {
    /// An error the user cannot act on: log every one, as before.
    Always,
    /// First failure of a stretch the user is told about: log it in full.
    First,
    /// A repeat within the same stretch: the notice and the log line were
    /// already given, so stay quiet.
    Repeat,
}

/// Records the outcome of one write by the SQLite writer. A read-only, full,
/// damaged or vanished database is reported once per failing stretch; other
/// write errors are bugs for the log, not something the user can act on.
pub fn record_write_outcome(db_path: &Path, failure_message: Option<&str>) -> WriteFailureLog {
    let Some(message) = failure_message else {
        WRITES_FAILING.store(false, Ordering::Relaxed);
        return WriteFailureLog::Always;
    };
    record_write_problem(db_path, classify_database_problem(message, db_path))
}

/// Like [`record_write_outcome`] for a problem the writer found itself.
pub fn record_write_problem(db_path: &Path, problem: DatabaseProblem) -> WriteFailureLog {
    if matches!(problem, DatabaseProblem::Other(_)) {
        return WriteFailureLog::Always;
    }
    if problem == DatabaseProblem::Corrupt && CORRUPTION_REPORTED.swap(true, Ordering::Relaxed) {
        return WriteFailureLog::Repeat;
    }
    if WRITES_FAILING.swap(true, Ordering::Relaxed) && problem != DatabaseProblem::Corrupt {
        return WriteFailureLog::Repeat;
    }
    set_notice(DatabaseProblemNotice {
        path: db_path.to_path_buf(),
        problem,
        persistence_disabled: false,
    });
    WriteFailureLog::First
}

/// Records that startup restored only part of the saved state (#911).
pub fn record_partial_restore(db_path: &Path, skipped_tabs: usize) {
    set_notice(DatabaseProblemNotice {
        path: db_path.to_path_buf(),
        problem: DatabaseProblem::PartialRestore { skipped_tabs },
        persistence_disabled: false,
    });
}

/// Identity of the database file when the writer opened it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    dev: u64,
    ino: u64,
}

/// The device and inode of the regular file at `path`, if it can be read.
pub fn file_identity(path: &Path) -> Option<FileIdentity> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(FileIdentity {
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        // No cheap stable identity here: only a missing file is detected.
        let _ = meta;
        Some(FileIdentity { dev: 0, ino: 0 })
    }
}

/// Whether the file at `path` is still the one the writer opened. Returns the
/// problem if it was deleted or swapped. An unreadable path (a permissions
/// error, say) is not judged: that is not evidence the file moved.
pub fn check_file_unchanged(path: &Path, opened: FileIdentity) -> Option<DatabaseProblem> {
    match std::fs::metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Some(DatabaseProblem::FileReplaced)
        }
        Err(_) => None,
        Ok(meta) if !meta.is_file() => Some(DatabaseProblem::FileReplaced),
        Ok(_) => match file_identity(path) {
            Some(now) if now != opened => Some(DatabaseProblem::FileReplaced),
            _ => None,
        },
    }
}

/// Whether the file at `path` is no longer a SQLite database: empty, or
/// without the 16-byte magic every SQLite file starts with. Truncating or
/// overwriting the file under a running connection does not make SQLite fail
/// at once (it keeps its open handle and cached pages), so the writer looks.
/// A missing or unreadable file is not judged here.
pub fn check_file_is_database(path: &Path) -> Option<DatabaseProblem> {
    use std::io::Read;
    const MAGIC: &[u8; 16] = b"SQLite format 3\0";
    let mut file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut head = [0u8; 16];
    let mut filled = 0;
    while filled < head.len() {
        match file.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => return None,
        }
    }
    (&head[..filled] != MAGIC).then_some(DatabaseProblem::Corrupt)
}

/// Decides whether a repeated SQLite error should be logged again. SQLite
/// reports each failing statement through its own log hook, so a damaged
/// file produced 10-30 identical lines (#911): log the first occurrence of
/// each error code and then at most one a minute.
pub fn sqlite_error_should_log(primary_code: i32, now_secs: u64) -> bool {
    static SEEN: Mutex<Vec<(i32, u64)>> = Mutex::new(Vec::new());
    let Ok(mut seen) = SEEN.lock() else {
        return true;
    };
    sqlite_error_rate_limit(&mut seen, primary_code, now_secs)
}

fn sqlite_error_rate_limit(seen: &mut Vec<(i32, u64)>, code: i32, now_secs: u64) -> bool {
    const WINDOW_SECS: u64 = 60;
    match seen.iter_mut().find(|(c, _)| *c == code) {
        Some((_, last)) if now_secs.saturating_sub(*last) < WINDOW_SECS => false,
        Some((_, last)) => {
            *last = now_secs;
            true
        }
        None => {
            seen.push((code, now_secs));
            true
        }
    }
}

/// Returns (once) the oldest pending notice, so the UI can show it.
pub fn take_database_problem_notice() -> Option<DatabaseProblemNotice> {
    let mut pending = PROBLEM_NOTICE.lock().ok()?;
    (!pending.is_empty()).then(|| pending.remove(0))
}

#[cfg(test)]
#[path = "database_notice_tests.rs"]
mod tests;
