//! Implementation of the [`UserPreferences`] trait using a file for
//! persistence.

use std::path::{Path, PathBuf};

use super::background_writer::{BackgroundWriter, FailureFn, WRITE_DEBOUNCE, WriteFn};
use super::{Error, in_memory::InMemoryPreferences};

/// A preferences file write (temp file + fsync + rename) slower than this is a visible stall
/// when it happens on the UI thread (#1146, #1203).
const PREFS_WRITE_SLOW: std::time::Duration = std::time::Duration::from_millis(50);
static SLOW_PREFS_WRITE: warp_util::slow_log::SlowLog = warp_util::slow_log::SlowLog::new(
    "user preferences file write",
    PREFS_WRITE_SLOW,
    warp_util::slow_log::DEFAULT_WINDOW,
);

/// An implementation of the [`UserPreferences`] trait using a file for
/// persistence.
///
/// Note that this is currently not robust to external modifications to
/// the backing file (either by another instance of the application or
/// by an end user).  This reads the file once when initialized and keeps
/// an in-memory copy of the preferences, flushing to disk after each
/// update.
///
/// Reads always see the in-memory copy, so they observe a write immediately.
/// With [`with_background_writer`](Self::with_background_writer) the disk write
/// is handed to a coalescing thread (#1203): the atomic temp + fsync + rename
/// no longer runs on the caller's (UI) thread, which on a busy disk stalled
/// Enter by up to 2.3 s. Writes of an unchanged value, and removals of an
/// absent key, are skipped without touching the disk at all.
pub struct FileBackedUserPreferences {
    /// The path to the file that backs this preferences store.
    file_path: PathBuf,

    /// A backing in-memory preferences store that we can flush to disk upon
    /// modification.
    inner: InMemoryPreferences,

    /// Writes the file off the calling thread, when enabled.
    background: Option<BackgroundWriter>,
}

impl FileBackedUserPreferences {
    /// Constructs a new file-backed user preferences store.
    ///
    /// If no file exists at the given path, an empty in-memory backing store
    /// will be used, and any modifications will trigger creation of the file
    /// (including any missing parent directories).
    ///
    /// Returns an error if something went wrong while attempting to read the
    /// existing persisted preferences at the given path.
    pub fn new(file_path: PathBuf) -> Result<Self, Error> {
        let inner = Self::initialize_in_memory_preferences(file_path.as_path())?;
        Ok(Self {
            file_path,
            inner,
            background: None,
        })
    }

    /// Moves this store's file writes onto a background thread (#1203).
    ///
    /// The document is still rendered on the calling thread (microseconds); the atomic
    /// write happens on a thread that writes only the newest rendering, at most once per
    /// debounce window. A failed write is logged (and retried by the next change) rather
    /// than returned from the setter. Call [`flush_pending`](Self::flush_pending) on quit;
    /// dropping the store flushes too.
    pub fn with_background_writer(self) -> Self {
        self.with_background_writer_debounce(WRITE_DEBOUNCE)
    }

    fn with_background_writer_debounce(mut self, debounce: std::time::Duration) -> Self {
        let path = self.file_path.clone();
        let write: WriteFn = std::sync::Arc::new(|path, data| {
            let start = std::time::Instant::now();
            let result = crate::atomic_file::write(path, data);
            SLOW_PREFS_WRITE.finish(start, || format!("path={}", path.display()));
            result
        });
        let failure_path = path.clone();
        let on_failure: FailureFn = std::sync::Arc::new(move |message| {
            log::error!(
                "Failed to save user preferences at {}: {message}",
                failure_path.display()
            );
        });
        match BackgroundWriter::new(path, debounce, write, on_failure) {
            Ok(writer) => self.background = Some(writer),
            Err(err) => log::warn!(
                "Could not start the preferences writer thread ({err}); writing preferences inline"
            ),
        }
        self
    }

    /// Blocks until every change made so far is on disk. A no-op unless
    /// [`with_background_writer`](Self::with_background_writer) was used.
    pub fn flush_pending(&self) -> Result<(), Error> {
        match &self.background {
            Some(writer) => writer
                .flush()
                .map_err(|message| Error::Unknown(anyhow::anyhow!(message))),
            None => Ok(()),
        }
    }

    /// Loads the contents of the file at the given path into an in-memory
    /// preferences store.
    ///
    /// If the file is not found, an empty store is returned.  The file is
    /// not created.
    fn initialize_in_memory_preferences(file_path: &Path) -> Result<InMemoryPreferences, Error> {
        let file_contents = match std::fs::read_to_string(file_path) {
            Ok(file_contents) => file_contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(InMemoryPreferences::default());
            }
            Err(err) => return Err(err.into()),
        };

        // If the file is empty/only whitespace, proceed with the default preferences.
        if file_contents.trim().is_empty() {
            return Ok(InMemoryPreferences::default());
        }

        let prefs = serde_json::from_str(&file_contents).map_err(anyhow::Error::new);
        if let Err(err) = &prefs {
            log::warn!("Failed to deserialize file preferences: {err:#}");
        }

        Ok(prefs?)
    }

    /// Flushes the internal in-memory preferences store to disk.
    fn flush(&self) -> Result<(), Error> {
        // Atomic replace, never truncate-in-place (#859): see
        // `crate::atomic_file`.
        let data = serde_json::to_string_pretty(&self.inner).map_err(anyhow::Error::new)?;
        let data = match &self.background {
            Some(writer) => match writer.queue(data) {
                Ok(()) => return Ok(()),
                // The writer thread is gone: write inline rather than lose the change.
                Err(data) => data,
            },
            None => data,
        };
        let start = std::time::Instant::now();
        let written = crate::atomic_file::write(&self.file_path, data);
        SLOW_PREFS_WRITE.finish(start, || format!("path={}", self.file_path.display()));
        written?;
        Ok(())
    }
}

impl super::UserPreferences for FileBackedUserPreferences {
    fn write_value(&self, key: &str, value: String) -> Result<(), super::Error> {
        // An unchanged value must not cost an fsync'd rewrite of the file (#1203).
        if self.inner.read_value(key)?.as_deref() == Some(value.as_str()) {
            return Ok(());
        }
        self.inner.write_value(key, value)?;
        self.flush()
    }

    fn read_value(&self, key: &str) -> Result<Option<String>, super::Error> {
        self.inner.read_value(key)
    }

    fn remove_value(&self, key: &str) -> Result<(), super::Error> {
        if self.inner.read_value(key)?.is_none() {
            return Ok(());
        }
        self.inner.remove_value(key)?;
        self.flush()
    }

    fn flush_pending_writes(&self) {
        if let Err(err) = self.flush_pending() {
            log::warn!("Could not flush pending preferences writes: {err}");
        }
    }
}

#[cfg(test)]
#[path = "file_backed_tests.rs"]
mod tests;
