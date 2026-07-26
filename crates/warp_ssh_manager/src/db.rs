//! A single `Mutex<SqliteConnection>` shared process-wide for the SSH manager to use.
//!
//! Current state: openWarp's main write connection lives on a dedicated write
//! thread (see `app/src/persistence/sqlite.rs`), processed asynchronously via
//! a `ModelEvent` channel. Hooking the SSH manager into that event bus would
//! require adding 6+ enum variants plus exposing types across crates — too
//! expensive.
//!
//! Alternative: **SQLite WAL mode naturally supports multiple write
//! connections** (writes are mutually exclusive but retried with
//! busy_timeout), so this opens a separate write connection here, with
//! behavior fully localized to this crate. The SSH manager's writes are
//! user-driven (creating/deleting nodes), extremely low-frequency, and any
//! contention with the main write thread is negligible.
//!
//! The path is passed in by the caller at initialization time
//! (`set_database_path`), avoiding this crate directly depending on the app
//! layer's `database_file_path()`. When no path has been passed, `with_conn` returns `Err(NotInitialized)`.

use anyhow::{Result, anyhow};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static DB_PATH: OnceLock<PathBuf> = OnceLock::new();
static CONN: OnceLock<Mutex<SqliteConnection>> = OnceLock::new();

/// Called once by the app at startup, passing in the sqlite db file path.
/// Repeated calls are ignored (OnceLock semantics).
pub fn set_database_path(path: PathBuf) {
    let _ = DB_PATH.set(path);
}

fn open() -> Result<SqliteConnection> {
    let path = DB_PATH
        .get()
        .ok_or_else(|| anyhow!("warp_ssh_manager::db: database path not initialized"))?;
    let url = path.to_string_lossy();
    let mut conn = SqliteConnection::establish(&url)?;
    conn.batch_execute(
        "PRAGMA foreign_keys = ON; \
         PRAGMA busy_timeout = 2000; \
         PRAGMA journal_mode = WAL;",
    )?;
    Ok(conn)
}

/// Executes a closure while holding the lock. Lazily opens the connection on the first call; subsequent calls reuse it.
pub fn with_conn<R>(f: impl FnOnce(&mut SqliteConnection) -> Result<R>) -> Result<R> {
    let mtx = CONN.get_or_init(|| Mutex::new(open().expect("warp_ssh_manager db open")));
    let mut guard = mtx
        .lock()
        .map_err(|_| anyhow!("warp_ssh_manager db mutex poisoned"))?;
    f(&mut guard)
}
