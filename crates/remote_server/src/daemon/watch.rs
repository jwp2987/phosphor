//! The filesystem watcher of the helper (Wave H parity, family package P3, family `i`).
//!
//! * `notify`'s recommended watcher (inotify on Linux, FSEvents on macOS), with NO debouncer. The
//!   watcher callback turns every OS event into one [`Event::FsChange`] on the server's mpsc and
//!   does nothing else; coalescing is the consumers' business (a per-repo dirty flag and one
//!   refresh in flight, see `repo_meta.rs`), never a timer.
//! * [`WatchSet`] is the refcounted set of watched paths. Every consumer (the repo file tree
//!   today; buffers, diff repos and skill directories in later packages) calls
//!   [`WatchSet::acquire`] for a path it needs and [`WatchSet::release`] when it is done; the OS
//!   watch exists while the count is above zero. Watches are NON-recursive: a consumer asks for
//!   each directory it cares about (the repo tree asks for exactly the directories its ignore
//!   walker loaded).
//! * The set is capped ([`DEFAULT_WATCH_CAP`]). When the cap is reached, or the OS refuses a watch
//!   (inotify limit, ENOSPC, a network file system), `acquire` returns a [`WatchError`] and the
//!   consumer marks its repo UNWATCHED: it stops pushing live updates, logs it once, and the
//!   client falls back to refreshing on prompt events (design ruling R5).
//!
//! # Consumer API (for the buffers, diff-state and skills packages)
//!
//! ```ignore
//! // In the server core, which owns the one WatchSet:
//! match self.watches.acquire(dir) {
//!     Ok(()) => { /* live updates will arrive as Event::FsChange */ }
//!     Err(e) => { /* mark the consumer unwatched; e.is_capacity() says why */ }
//! }
//! // ... later
//! self.watches.release(dir);
//! // Event::FsChange { paths, kind } reaches Server::handle_event; add one line there that
//! // forwards it to your module (see `Server::on_fs_change`). `kind == FsKind::Rescan`
//! // means events may have been lost: treat everything you watch as changed.
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::server::Event;

/// The most distinct paths one daemon watches. Well under the usual inotify limit
/// (`fs.inotify.max_user_watches` is 8192 on older kernels) so the daemon leaves room for the
/// user's editors and other tools.
pub(super) const DEFAULT_WATCH_CAP: usize = 8_000;

/// What happened, coarsened to what the consumers branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FsKind {
    Create,
    Modify,
    Remove,
    /// A rename (reported with both paths when the OS pairs them).
    Rename,
    /// Anything else that is not a pure read (`Access` events are not forwarded).
    Other,
    /// The watcher lost events (queue overflow or a watcher error): every consumer must treat
    /// all of its state as possibly changed. `paths` is empty.
    Rescan,
}

/// Maps a `notify` event kind. `None` for kinds that cannot have changed anything (`Access`).
pub(super) fn classify(kind: &EventKind) -> Option<FsKind> {
    use notify::event::ModifyKind;
    match kind {
        EventKind::Access(_) => None,
        EventKind::Create(_) => Some(FsKind::Create),
        EventKind::Remove(_) => Some(FsKind::Remove),
        EventKind::Modify(ModifyKind::Name(_)) => Some(FsKind::Rename),
        EventKind::Modify(_) => Some(FsKind::Modify),
        EventKind::Any | EventKind::Other => Some(FsKind::Other),
    }
}

/// Why a watch could not be added.
#[derive(Debug)]
pub(super) enum WatchError {
    /// The set is at its cap.
    CapReached { cap: usize },
    /// The OS refused (inotify limit, ENOSPC, permission, a path that is gone).
    Os(String),
    /// No watcher could be created at all on this host.
    Unavailable(String),
}

impl WatchError {
    /// The failure is a resource limit (as opposed to the path simply being gone): the repo
    /// should be marked unwatched rather than retried.
    pub(super) fn is_capacity(&self) -> bool {
        !matches!(self, WatchError::Os(_))
    }
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatchError::CapReached { cap } => write!(f, "watch cap of {cap} paths reached"),
            WatchError::Os(e) => write!(f, "the OS refused the watch: {e}"),
            WatchError::Unavailable(e) => write!(f, "no filesystem watcher: {e}"),
        }
    }
}

/// The OS-facing half, behind a trait so the refcounting and cap logic is testable without
/// inotify.
trait Backend: Send {
    fn watch(&mut self, path: &Path) -> Result<(), WatchError>;
    fn unwatch(&mut self, path: &Path);
}

struct NotifyBackend {
    watcher: RecommendedWatcher,
}

impl Backend for NotifyBackend {
    fn watch(&mut self, path: &Path) -> Result<(), WatchError> {
        self.watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| match &e.kind {
                notify::ErrorKind::MaxFilesWatch => WatchError::CapReached { cap: 0 },
                notify::ErrorKind::Io(io) if io.raw_os_error() == Some(libc::ENOSPC) => {
                    WatchError::CapReached { cap: 0 }
                }
                _ => WatchError::Os(e.to_string()),
            })
    }

    fn unwatch(&mut self, path: &Path) {
        let _ = self.watcher.unwatch(path);
    }
}

/// Refcounted, capped set of watched paths. Owned by the server core.
pub(super) struct WatchSet {
    backend: Option<Box<dyn Backend>>,
    unavailable: Option<String>,
    counts: HashMap<PathBuf, usize>,
    cap: usize,
}

impl WatchSet {
    /// A set whose OS watcher delivers into `events` as [`Event::FsChange`]. If the host has no
    /// watcher the set still exists and every `acquire` fails with
    /// [`WatchError::Unavailable`].
    pub(super) fn new(events: Sender<Event>) -> Self {
        let handler = move |result: notify::Result<notify::Event>| {
            let event = match result {
                Ok(event) => match classify(&event.kind) {
                    Some(kind) => Event::FsChange {
                        paths: event.paths,
                        kind,
                    },
                    None => return,
                },
                // The watcher reports its own failures (queue overflow) as errors: nothing is
                // known about what changed.
                Err(_) => Event::FsChange {
                    paths: Vec::new(),
                    kind: FsKind::Rescan,
                },
            };
            // The core is gone (shutdown): nothing to tell.
            let _ = events.send(event);
        };
        match notify::recommended_watcher(handler) {
            Ok(watcher) => {
                Self::with_backend(Box::new(NotifyBackend { watcher }), DEFAULT_WATCH_CAP)
            }
            Err(e) => {
                log::warn!("Daemon: no filesystem watcher on this host: {e}");
                Self {
                    backend: None,
                    unavailable: Some(e.to_string()),
                    counts: HashMap::new(),
                    cap: DEFAULT_WATCH_CAP,
                }
            }
        }
    }

    fn with_backend(backend: Box<dyn Backend>, cap: usize) -> Self {
        Self {
            backend: Some(backend),
            unavailable: None,
            counts: HashMap::new(),
            cap,
        }
    }

    /// A set with no watcher, for a core that has no event sender (unit tests of the dispatch).
    pub(super) fn disabled() -> Self {
        Self {
            backend: None,
            unavailable: Some("watching is disabled".into()),
            counts: HashMap::new(),
            cap: DEFAULT_WATCH_CAP,
        }
    }

    /// Adds one reference to `path`, starting the OS watch on the first.
    pub(super) fn acquire(&mut self, path: &Path) -> Result<(), WatchError> {
        if let Some(count) = self.counts.get_mut(path) {
            *count += 1;
            return Ok(());
        }
        if self.counts.len() >= self.cap {
            return Err(WatchError::CapReached { cap: self.cap });
        }
        let Some(backend) = self.backend.as_mut() else {
            return Err(WatchError::Unavailable(
                self.unavailable.clone().unwrap_or_default(),
            ));
        };
        backend.watch(path).map_err(|e| match e {
            WatchError::CapReached { .. } => WatchError::CapReached { cap: self.cap },
            other => other,
        })?;
        self.counts.insert(path.to_path_buf(), 1);
        Ok(())
    }

    /// Drops one reference to `path`; the OS watch ends with the last. Releasing a path that is
    /// not watched is a no-op.
    pub(super) fn release(&mut self, path: &Path) {
        let Some(count) = self.counts.get_mut(path) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            self.counts.remove(path);
            if let Some(backend) = self.backend.as_mut() {
                backend.unwatch(path);
            }
        }
    }

    #[allow(dead_code)] // consumer API for the packages that follow
    pub(super) fn is_watched(&self, path: &Path) -> bool {
        self.counts.contains_key(path)
    }

    /// Number of distinct OS watches.
    #[allow(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.counts.len()
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
