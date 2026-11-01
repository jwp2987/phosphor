//! A coalescing background writer for the settings file (#1155).
//!
//! Writing `settings.toml` is atomic (temp file + fsync + rename), which costs
//! 0.5-5 ms on a fast disk and 20-100 ms on a slow or networked one. Done on the UI
//! thread per setting change, a key-repeat of zoom or font size paid that once per
//! repeat. The store renders the document to a string on the calling thread (a few
//! microseconds) and hands it here; a single thread writes only the latest string, at
//! most once per debounce window.
//!
//! Nothing here can tear the file: every write goes through the same atomic replace as
//! before. What changes is *when*: a change is on disk within the debounce window
//! rather than before the call returns, so [`BackgroundWriter::flush`] exists for the
//! places that need it on disk now (quit), and [`Drop`] flushes too.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// How long a pending write waits for newer ones to replace it.
pub(super) const WRITE_DEBOUNCE: Duration = Duration::from_millis(150);

/// How many of our own recent writes are remembered, so the file watcher's echo of an
/// older write is recognised even after a newer one has been queued.
const RECENT_WRITES_KEPT: usize = 8;

/// How long after a failed write the next attempt waits; doubled per consecutive failure up to
/// [`WRITE_RETRY_DELAY_MAX`]. A write that failed (disk full, a momentarily read-only mount) is
/// kept pending rather than dropped: the stores skip a value equal to the one in memory, so a
/// dropped write would never be made again unless some other setting changed.
const WRITE_RETRY_DELAY: Duration = Duration::from_secs(1);
const WRITE_RETRY_DELAY_MAX: Duration = Duration::from_secs(30);

type Digest32 = [u8; 32];

/// Writes `data` to the path, atomically.
pub(super) type WriteFn = Arc<dyn Fn(&Path, &str) -> std::io::Result<()> + Send + Sync>;
/// Called with the error message on each transition from "writes work" to "writes fail".
pub(super) type FailureFn = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Default)]
struct State {
    /// The newest rendered document not yet handed to `write`.
    pending: Option<String>,
    /// When `pending` first became non-empty; the debounce window runs from here.
    pending_since: Option<Instant>,
    /// A caller is waiting for the queue to drain: skip the debounce.
    flush_requested: bool,
    /// A write is running on the thread right now.
    in_flight: bool,
    /// When a write that failed may be tried again; `pending` holds its data meanwhile.
    retry_at: Option<Instant>,
    shutdown: bool,
    /// Digests of what we have written (or are writing), newest last.
    recent: VecDeque<Digest32>,
    /// The error of the most recent write, cleared by a successful one.
    last_error: Option<String>,
}

struct Shared {
    path: PathBuf,
    debounce: Duration,
    /// The first retry delay after a failed write (see [`WRITE_RETRY_DELAY`]).
    retry_delay: Duration,
    write: WriteFn,
    on_failure: FailureFn,
    state: Mutex<State>,
    changed: Condvar,
    /// False once the thread has exited (shutdown or panic).
    alive: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic inside `write` never holds this lock, but be robust anyway: the state is
        // plain data.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub(super) struct BackgroundWriter {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

fn digest(bytes: &[u8]) -> Digest32 {
    Sha256::digest(bytes).into()
}

impl BackgroundWriter {
    pub(super) fn new(
        path: PathBuf,
        debounce: Duration,
        write: WriteFn,
        on_failure: FailureFn,
    ) -> std::io::Result<Self> {
        Self::new_with_retry_delay(path, debounce, WRITE_RETRY_DELAY, write, on_failure)
    }

    /// [`Self::new`] with the first retry delay after a failed write given explicitly.
    pub(super) fn new_with_retry_delay(
        path: PathBuf,
        debounce: Duration,
        retry_delay: Duration,
        write: WriteFn,
        on_failure: FailureFn,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            path,
            debounce,
            retry_delay,
            write,
            on_failure,
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            alive: AtomicBool::new(true),
        });
        let thread_shared = shared.clone();
        let handle = std::thread::Builder::new()
            .name("Settings Writer".into())
            .spawn(move || {
                run(&thread_shared);
                thread_shared.alive.store(false, Ordering::SeqCst);
                thread_shared.changed.notify_all();
            })?;
        Ok(Self {
            shared,
            thread: Mutex::new(Some(handle)),
        })
    }

    /// Replaces whatever is pending with `data`. If the writer thread is gone the data is
    /// handed back so the caller can write it itself; nothing is silently dropped.
    pub(super) fn queue(&self, data: String) -> Result<(), String> {
        if !self.shared.alive.load(Ordering::SeqCst) {
            return Err(data);
        }
        let mut state = self.shared.lock();
        if state.shutdown {
            return Err(data);
        }
        state.pending = Some(data);
        state.pending_since.get_or_insert_with(Instant::now);
        drop(state);
        self.shared.changed.notify_all();
        Ok(())
    }

    /// Blocks until everything queued so far is on disk, skipping the debounce. Returns
    /// the error of the last write if it failed. Safe to call with nothing pending.
    pub(super) fn flush(&self) -> Result<(), String> {
        // Bounded: on quit this is a disk wait, and a stuck disk must not hold the exit past
        // the SIGTERM watchdog (the write is atomic, so giving up leaves the file intact) (#1253).
        const FLUSH_TIMEOUT: Duration = Duration::from_secs(3);
        let deadline = std::time::Instant::now() + FLUSH_TIMEOUT;
        let mut state = self.shared.lock();
        state.flush_requested = true;
        self.shared.changed.notify_all();
        loop {
            if state.pending.is_none() && !state.in_flight {
                state.flush_requested = false;
                return state.last_error.clone().map_or(Ok(()), Err);
            }
            if !self.shared.alive.load(Ordering::SeqCst) {
                // The thread died (it never does by design): write it here rather than lose it.
                state.flush_requested = false;
                let Some(data) = state.pending.take() else {
                    return Ok(());
                };
                state.pending_since = None;
                state.recent_push(digest(data.as_bytes()));
                drop(state);
                return (self.shared.write)(&self.shared.path, &data).map_err(|e| e.to_string());
            }
            if std::time::Instant::now() >= deadline {
                state.flush_requested = false;
                return Err(format!(
                    "timed out after {FLUSH_TIMEOUT:?} waiting for the settings write to finish"
                ));
            }
            let (guard, _) = self
                .shared
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap_or_else(|e| e.into_inner());
            state = guard;
        }
    }

    /// Drops the pending write (not one already running). Used when the file was changed
    /// by someone else and the in-memory document was replaced by it: writing the stale
    /// pending string would undo their edit.
    #[allow(dead_code)] // only the TOML store needs this
    pub(super) fn discard_pending(&self) {
        let mut state = self.shared.lock();
        state.pending = None;
        state.pending_since = None;
        state.retry_at = None;
    }

    /// Whether `contents` is byte-for-byte the file watcher's echo of our own write, which must
    /// not be reloaded: what this writer wrote last, or an older write of its own while a newer
    /// one is still pending or in flight (the file legitimately holds the older one until the
    /// newer rename lands). An older own write found on disk while nothing is pending was put
    /// there by someone else (a `git checkout` of the settings file, a restored backup) and is a
    /// foreign edit to reload, not an echo.
    #[allow(dead_code)] // only the TOML store needs this
    pub(super) fn is_recent_write(&self, contents: &[u8]) -> bool {
        let wanted = digest(contents);
        let state = self.shared.lock();
        let Some(newest) = state.recent.back() else {
            return false;
        };
        *newest == wanted
            || ((state.pending.is_some() || state.in_flight) && state.recent.contains(&wanted))
    }
}

impl State {
    fn recent_push(&mut self, digest: Digest32) {
        if self.recent.len() >= RECENT_WRITES_KEPT {
            self.recent.pop_front();
        }
        self.recent.push_back(digest);
    }
}

impl Drop for BackgroundWriter {
    fn drop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.shutdown = true;
            state.flush_requested = true;
        }
        self.shared.changed.notify_all();
        let handle = self.thread.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

fn run(shared: &Shared) {
    let mut state = shared.lock();
    let mut failing = false;
    let mut retry_delay = shared.retry_delay;
    loop {
        while state.pending.is_none() && !state.shutdown {
            state = shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        if state.pending.is_none() {
            return;
        }
        if !state.flush_requested && !state.shutdown {
            let mut deadline = state.pending_since.unwrap_or_else(Instant::now) + shared.debounce;
            if let Some(retry_at) = state.retry_at {
                deadline = deadline.max(retry_at);
            }
            let now = Instant::now();
            if now < deadline {
                state = shared
                    .changed
                    .wait_timeout(state, deadline - now)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
                continue;
            }
        }
        let Some(data) = state.pending.take() else {
            continue;
        };
        state.pending_since = None;
        state.retry_at = None;
        state.in_flight = true;
        // Recorded before the write: the watcher can report the rename before we get the
        // lock back.
        state.recent_push(digest(data.as_bytes()));
        drop(state);

        let result = (shared.write)(&shared.path, &data);

        state = shared.lock();
        state.in_flight = false;
        match result {
            Ok(()) => {
                failing = false;
                retry_delay = shared.retry_delay;
                state.last_error = None;
            }
            Err(err) => {
                let message = err.to_string();
                log::warn!(
                    "Failed to save settings file at {}: {message}",
                    shared.path.display()
                );
                state.last_error = Some(message.clone());
                // Kept pending and retried with a growing delay, unless a newer rendering has
                // replaced it meanwhile or a caller is waiting on this write: `flush` (and the
                // quit path behind it) is told through the `Err` and must not wait on a disk
                // that keeps failing.
                if state.pending.is_none() && !state.flush_requested && !state.shutdown {
                    state.pending = Some(data);
                    state.pending_since = Some(Instant::now());
                    state.retry_at = Some(Instant::now() + retry_delay);
                    retry_delay = retry_delay.saturating_mul(2).min(WRITE_RETRY_DELAY_MAX);
                }
                if !failing {
                    failing = true;
                    // Not under the lock: the callback may take its own locks.
                    drop(state);
                    (shared.on_failure)(message);
                    state = shared.lock();
                }
            }
        }
        shared.changed.notify_all();
    }
}

#[cfg(test)]
#[path = "background_writer_tests.rs"]
mod tests;
