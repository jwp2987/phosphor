use std::collections::HashSet;
/// A model for operating on files.
///
/// Allows opening and saving files in a single, central model.  Subscribers can watch for content
/// when files are loaded, and request that content be saved to disk.
use std::future::Future;
use std::io;
use std::ops::Range;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use remote_server::client::RemoteServerClient;
use remote_server::manager::RemoteServerManager;
use warp_core::HostId;
use warp_util::standardized_path::StandardizedPath;

use futures::channel::oneshot;
use futures::io::{AsyncBufReadExt, BufReader};
use futures::{FutureExt, StreamExt};

use async_channel::Sender;
use notify_debouncer_full::notify::{RecursiveMode, WatchFilter};
use repo_metadata::{
    repositories::DetectedRepositories,
    repository::{RepositorySubscriber, SubscriberId},
    CanonicalizedPath, Repository, RepositoryUpdate, RepositoryWatchMode,
};
use warp_util::content_version::ContentVersion;
use warp_util::file::FileSaveError;
use warp_util::file::{FileId, FileLoadError};
use warpui::ModelHandle;
use warpui::{r#async::SpawnedFutureHandle, AppContext, Entity, ModelContext, SingletonEntity};
use watcher::{BulkFilesystemWatcher, BulkFilesystemWatcherEvent};

pub mod text_file_reader;
pub use text_file_reader::{TextFileReadResult, TextFileSegment};

#[derive(Debug)]
pub enum FileModelEvent {
    FileLoaded {
        content: String,
        id: FileId,
        version: ContentVersion,
    },
    FailedToLoad {
        id: FileId,
        error: Rc<FileLoadError>,
    },
    FileSaved {
        id: FileId,
        version: ContentVersion,
    },
    FailedToSave {
        id: FileId,
        error: Rc<FileSaveError>,
    },
    FileUpdated {
        id: FileId,
        content: String,
        base_version: ContentVersion,
        new_version: ContentVersion,
    },
}

impl FileModelEvent {
    pub fn file_id(&self) -> FileId {
        match self {
            Self::FileLoaded { id, .. } => *id,
            Self::FailedToLoad { id, .. } => *id,
            Self::FileSaved { id, .. } => *id,
            Self::FailedToSave { id, .. } => *id,
            Self::FileUpdated { id, .. } => *id,
        }
    }
}

/// The state a file must still be in on disk for [`FileModel::save_if_unchanged`]
/// to overwrite it.
///
/// A caller that derived `content` from a *snapshot* of a file — an AI diff, a
/// refactor preview, anything computed against a copy read earlier — cannot use
/// plain [`FileModel::save`], because that writes the whole snapshot-derived
/// buffer unconditionally and silently discards whatever else touched the file
/// in between. This type is how such a caller states the pre-image its content
/// was computed against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedDiskState {
    /// The file must not exist. For a write that *creates* a file which was
    /// verified absent when the change was proposed.
    Absent,
    /// The file must currently hold this text.
    ///
    /// Compared after normalising line endings to LF on both sides — see
    /// `normalize_to_lf` for why the comparison is not byte-exact.
    Content(String),
}

/// The outcome of comparing the file on disk against the caller's expected
/// pre-image, computed before a guarded write and *acted on*, never merely
/// logged.
#[derive(Debug, PartialEq, Eq)]
enum PreWriteVerdict {
    /// Disk matches what the caller expected; the write may proceed.
    Proceed,
    /// Disk does not match, or could not be established to match. The write
    /// must be abandoned and the message reported to the user.
    Refuse(String),
}

/// Normalises `\r\n` and lone `\r` to `\n`.
///
/// Mirrors `editor::multiline::MultilineString::<LF>::apply`, which is what
/// produced the expected pre-image on the caller's side (a code editor's diff
/// base is stored LF-normalised regardless of the file's real line endings).
/// Reimplemented here rather than depended on: `warp_files` sits below `editor`
/// in the crate graph and a three-line normaliser is cheaper than that edge.
///
/// The comparison is therefore *not* byte-exact, and that is deliberate. A
/// guarded write still emits the caller's own line endings, so a pure
/// CRLF-to-LF conversion by some external tool is discarded by the write
/// whether or not the check notices it. Treating it as a conflict would fail
/// every accept on a CRLF file while protecting nothing.
fn normalize_to_lf(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Decides whether a guarded write may proceed, given what the caller expected
/// to find on disk and what reading the file actually produced.
///
/// Split out from the write task so the decision is unit-testable without a
/// filesystem — in particular the "the file could not be read at all" arm,
/// which is the one that must never be confused with "nothing is there, go
/// ahead". An unreadable file is a file whose contents we cannot vouch for, and
/// overwriting it would be exactly the data loss this guard exists to prevent.
fn check_pre_image(
    path: &Path,
    expected: &ExpectedDiskState,
    read_result: Result<String, io::Error>,
) -> PreWriteVerdict {
    let display = path.display();
    match (expected, read_result) {
        (ExpectedDiskState::Absent, Err(err)) if err.kind() == io::ErrorKind::NotFound => {
            PreWriteVerdict::Proceed
        }
        (ExpectedDiskState::Absent, Ok(_)) => PreWriteVerdict::Refuse(format!(
            "{display} was created by something else after this change was proposed, \
             so it was not overwritten. Re-run the request to work from the current file."
        )),
        (ExpectedDiskState::Absent, Err(err)) => PreWriteVerdict::Refuse(format!(
            "{display} could not be read to check whether it already exists ({err}), \
             so it was not written. Nothing was changed."
        )),
        (ExpectedDiskState::Content(expected), Ok(actual)) => {
            if normalize_to_lf(expected) == normalize_to_lf(&actual) {
                PreWriteVerdict::Proceed
            } else {
                PreWriteVerdict::Refuse(format!(
                    "{display} changed on disk after this change was proposed, \
                     so it was not overwritten and your edits are intact. \
                     Re-run the request to work from the current file."
                ))
            }
        }
        (ExpectedDiskState::Content(_), Err(err)) if err.kind() == io::ErrorKind::NotFound => {
            PreWriteVerdict::Refuse(format!(
                "{display} was deleted after this change was proposed, so it was not \
                 recreated. Re-run the request to work from the current file."
            ))
        }
        (ExpectedDiskState::Content(_), Err(err)) => PreWriteVerdict::Refuse(format!(
            "{display} could not be read to check whether it changed since this change \
             was proposed ({err}), so it was not overwritten. Nothing was changed."
        )),
    }
}

/// The remote counterpart of [`check_pre_image`]: `Some(message)` refuses the
/// write, `None` lets it proceed.
///
/// Generic over the read error so it can be exercised without a live
/// `RemoteServerClient`; production passes `remote_server`'s client error.
///
/// This is deliberately weaker than [`check_pre_image`] in exactly one place.
/// The remote read reports a missing file as an ordinary operation failure with
/// no distinguishable kind, so an [`ExpectedDiskState::Absent`] check cannot
/// tell "not there, as expected" from "the read itself failed". Refusing on
/// error would make it impossible to create any file on a remote host; the
/// error case therefore proceeds, and says so in the log rather than passing in
/// silence. The exposure is bounded: a creation diff is only ever offered after
/// the same channel reported the file absent moments earlier, and a genuine
/// transport failure fails the following write too, so nothing is clobbered.
/// Every other case — and every local case — refuses on an unreadable file.
fn remote_pre_image_refusal<E: std::fmt::Display>(
    path: &str,
    expected: &ExpectedDiskState,
    read_result: Result<Vec<u8>, E>,
) -> Option<String> {
    match (expected, read_result) {
        (ExpectedDiskState::Absent, Ok(bytes)) if bytes.is_empty() => None,
        (ExpectedDiskState::Absent, Ok(_)) => Some(format!(
            "{path} was created by something else after this change was proposed, \
             so it was not overwritten. Re-run the request to work from the current file."
        )),
        (ExpectedDiskState::Absent, Err(err)) => {
            log::warn!(
                "Could not confirm that remote file {path} is still absent ({err}); \
                 creating it anyway, because the remote read cannot distinguish a \
                 missing file from a failed read"
            );
            None
        }
        (ExpectedDiskState::Content(expected), Ok(bytes)) => match String::from_utf8(bytes) {
            Ok(actual) if normalize_to_lf(expected) == normalize_to_lf(&actual) => None,
            Ok(_) => Some(format!(
                "{path} changed on the remote host after this change was proposed, \
                 so it was not overwritten and your edits are intact. \
                 Re-run the request to work from the current file."
            )),
            Err(_) => Some(format!(
                "{path} is not valid UTF-8 on the remote host, so it could not be \
                 checked against the text this change was proposed from and was not \
                 overwritten. Nothing was changed."
            )),
        },
        (ExpectedDiskState::Content(_), Err(err)) => Some(format!(
            "{path} could not be read from the remote host to check whether it changed \
             since this change was proposed ({err}), so it was not overwritten. \
             Nothing was changed."
        )),
    }
}

/// Tracks how a file is being watched for changes.
#[derive(Clone, PartialEq, Eq, Default)]
enum WatcherType {
    /// File is not being watched.
    #[default]
    None,
    /// File is watched via an individual watcher on the directory recorded here.
    ///
    /// The registered path is stored rather than re-derived, so unregistration cannot drift from
    /// registration, and so a file whose watcher was never registered is never unregistered.
    Individual(PathBuf),
    /// File is watched via repository-level subscription.
    Repository,
}

impl WatcherType {
    /// The directory this file is individually watched through, if any.
    fn individual_watch_path(&self) -> Option<&Path> {
        match self {
            WatcherType::Individual(path) => Some(path),
            WatcherType::None | WatcherType::Repository => None,
        }
    }
}

/// Per-file backing store.
/// Remote files dispatch through [`RemoteServerClient`] via [`RemoteServerManager`].
enum FileBackend {
    Local(LocalFile),
    Remote {
        /// Identifies the remote host. The actual client is looked up from
        /// [`RemoteServerManager`] at call time, which naturally handles
        /// disconnect (lookup returns `Err`) without holding an `Arc` alive
        /// per file.
        host_id: HostId,
        /// Platform-aware path on the remote host.
        path: StandardizedPath,
    },
}

impl FileBackend {
    fn as_local(&self) -> Option<&LocalFile> {
        match self {
            FileBackend::Local(f) => Some(f),
            FileBackend::Remote { .. } => None,
        }
    }

    fn version(&self) -> Option<ContentVersion> {
        match self {
            FileBackend::Local(f) => f.version,
            FileBackend::Remote { .. } => None,
        }
    }

    fn set_version(&mut self, version: ContentVersion) {
        match self {
            FileBackend::Local(f) => f.version = Some(version),
            FileBackend::Remote { .. } => {}
        }
    }
}

#[derive(Default)]
struct LocalFile {
    path: Option<PathBuf>,
    version: Option<ContentVersion>,
    /// How this file is being watched for changes.
    watcher_type: WatcherType,
}

impl LocalFile {
    fn should_receive_update_for_path(&self, path: &Path) -> bool {
        self.path.as_deref() == Some(path) && self.subscribes_to_updates() && self.version.is_some()
    }
}

/// Tracks an active subscription to a repository for file change events.
struct RepositorySubscription {
    repository: ModelHandle<Repository>,
    /// The subscriber ID, set once the async subscription completes.
    subscriber_id: Option<SubscriberId>,
}

impl LocalFile {
    fn new(path: PathBuf) -> Self {
        // If we cannot canonicalize the path, it could be because the file does not exist on disk yet.
        // In this case, keep its original input path.
        let canonicalized_path = CanonicalizedPath::try_from(&path)
            .map(|path| path.as_path_buf().to_path_buf())
            .unwrap_or(path);
        Self {
            path: Some(canonicalized_path),
            version: None,
            watcher_type: WatcherType::None,
        }
    }

    /// The directory this file would be individually watched through. See
    /// [`FileModel::watch_path_for`].
    fn watch_path(&self) -> Option<PathBuf> {
        FileModel::watch_path_for(self.path.as_deref()?)
    }

    /// Returns true if this file is subscribed to receive update events.
    fn subscribes_to_updates(&self) -> bool {
        self.watcher_type != WatcherType::None
    }
}

/// Tracks files by ID with O(1) path reference counting.
///
/// Encapsulates the file map and path refcount together so callers
/// cannot forget to maintain the refcount invariant.
#[derive(Default)]
struct FileState {
    files: HashMap<FileId, FileBackend>,
    /// Tracks how many FileIds reference each path, for O(1) "path still used" checks.
    path_refcount: HashMap<PathBuf, usize>,
}

impl FileState {
    fn insert_local(&mut self, file_id: FileId, local_file: LocalFile) {
        if let Some(ref path) = local_file.path {
            *self.path_refcount.entry(path.clone()).or_insert(0) += 1;
        }
        self.files.insert(file_id, FileBackend::Local(local_file));
    }

    fn insert_remote(&mut self, file_id: FileId, host_id: HostId, path: StandardizedPath) {
        self.files
            .insert(file_id, FileBackend::Remote { host_id, path });
    }

    /// Removes a file and returns the backend along with whether the local path
    /// is still referenced (always `false` for remote files).
    fn remove(&mut self, file_id: FileId) -> Option<(FileBackend, bool)> {
        let backend = self.files.remove(&file_id)?;
        let path_still_used = match &backend {
            FileBackend::Local(file) => {
                if let Some(ref path) = file.path {
                    match self.path_refcount.get_mut(path) {
                        Some(count) => {
                            *count -= 1;
                            if *count == 0 {
                                self.path_refcount.remove(path);
                                false
                            } else {
                                true
                            }
                        }
                        None => false,
                    }
                } else {
                    false
                }
            }
            FileBackend::Remote { .. } => false,
        };
        Some((backend, path_still_used))
    }

    fn get(&self, file_id: FileId) -> Option<&FileBackend> {
        self.files.get(&file_id)
    }

    fn get_mut(&mut self, file_id: FileId) -> Option<&mut FileBackend> {
        self.files.get_mut(&file_id)
    }

    fn get_local(&self, file_id: FileId) -> Option<&LocalFile> {
        self.get(file_id).and_then(FileBackend::as_local)
    }

    fn local_values(&self) -> impl Iterator<Item = &LocalFile> {
        self.files.values().filter_map(FileBackend::as_local)
    }

    fn local_iter_mut(&mut self) -> impl Iterator<Item = (&FileId, &mut LocalFile)> {
        self.files
            .iter_mut()
            .filter_map(|(id, backend)| match backend {
                FileBackend::Local(f) => Some((id, f)),
                FileBackend::Remote { .. } => None,
            })
    }
}

/// Tracks which file paths belong to which repository roots, with O(1) repo cleanup checks.
///
/// Encapsulates the path-to-repo mapping and repo path count together so callers
/// cannot forget to maintain the count invariant.
#[derive(Default)]
struct RepoPathMappingState {
    /// Maps file path to its repository root for quick lookup during cleanup.
    path_to_repo: HashMap<PathBuf, PathBuf>,
    /// Tracks how many distinct paths reference each repo root.
    repo_path_count: HashMap<PathBuf, usize>,
}

impl RepoPathMappingState {
    fn insert(&mut self, path: PathBuf, repo_root: PathBuf) {
        // Only increment count if this is a genuinely new path entry.
        if self.path_to_repo.insert(path, repo_root.clone()).is_none() {
            *self.repo_path_count.entry(repo_root).or_insert(0) += 1;
        }
    }

    fn remove(&mut self, path: &Path) -> Option<(PathBuf, bool)> {
        if let Some(repo_root) = self.path_to_repo.remove(path) {
            let mut unused_repo = true;
            if let Some(count) = self.repo_path_count.get_mut(&repo_root) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.repo_path_count.remove(&repo_root);
                } else {
                    unused_repo = false;
                }
            }

            Some((repo_root, unused_repo))
        } else {
            None
        }
    }

    /// Returns all paths mapped to the given repository root.
    fn paths_for_repo(&self, repo_root: &Path) -> Vec<PathBuf> {
        self.path_to_repo
            .iter()
            .filter(|(_, root)| root.as_path() == repo_root)
            .map(|(path, _)| path.clone())
            .collect()
    }
}

pub struct FileModel {
    file_state: FileState,
    abort_handles: HashMap<FileId, SpawnedFutureHandle>,
    watcher: ModelHandle<BulkFilesystemWatcher>,
    /// Maps repository root path to its subscription. One subscription per repo.
    repo_subscriptions: HashMap<PathBuf, RepositorySubscription>,
    repo_path_mapping: RepoPathMappingState,
    /// One-shot senders awaiting the next save/delete/rename completion for a
    /// file. Saves are event-based (dispatch returns before the write lands);
    /// [`FileModel::save_completion`] lets a caller await the actual outcome.
    save_waiters: HashMap<FileId, Vec<oneshot::Sender<Result<(), Arc<FileSaveError>>>>>,
}

impl FileModel {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let watcher =
            ctx.add_model(|ctx| BulkFilesystemWatcher::new(Duration::from_millis(200), ctx));

        ctx.subscribe_to_model(&watcher, |me, event, ctx| {
            me.handle_watcher_event(event, ctx);
        });

        Self {
            watcher,
            file_state: FileState::default(),
            abort_handles: HashMap::new(),
            repo_subscriptions: HashMap::new(),
            repo_path_mapping: RepoPathMappingState::default(),
            save_waiters: HashMap::new(),
        }
    }

    #[cfg(feature = "test-util")]
    pub fn get_future_handle(&self, file_id: FileId) -> Option<SpawnedFutureHandle> {
        self.abort_handles.get(&file_id).cloned()
    }

    /// The directory this file is currently registered with the watcher through, if any.
    #[cfg(test)]
    fn registered_watch_path(&self, file_id: FileId) -> Option<&Path> {
        self.file_state
            .get_local(file_id)?
            .watcher_type
            .individual_watch_path()
    }

    fn handle_watcher_event(
        &mut self,
        event: &BulkFilesystemWatcherEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        let updated_files = event.added_or_updated_set();
        self.reload_file_paths(updated_files, ctx);
    }

    pub fn file_path(&self, file_id: FileId) -> Option<PathBuf> {
        self.file_state
            .get_local(file_id)
            .and_then(|x| x.path.clone())
    }

    /// Register a remote file path and return a `FileId`.
    ///
    /// The returned `FileId` can be used with `save()` and `delete()` which
    /// will dispatch to the remote backend via [`RemoteServerClient`].
    pub fn register_remote_file(&mut self, host_id: HostId, path: StandardizedPath) -> FileId {
        let file_id = FileId::new();
        self.file_state.insert_remote(file_id, host_id, path);
        file_id
    }

    /// Register a file path and immediately return a FileId without loading the file.
    /// This is useful when you need a FileId in a constructor but don't need to load the file. This will
    /// also not opt-in to receive file watcher updates.
    pub fn register_file_path(
        &mut self,
        file_path: &Path,
        subscribe_to_updates: bool,
        ctx: &mut ModelContext<Self>,
    ) -> FileId {
        let file_id = FileId::new();
        let mut local_file = LocalFile::new(file_path.to_owned());

        if subscribe_to_updates {
            // Try to use repository-level watching if available
            if let Some(repo_root) = self.get_or_create_repo_subscription(file_path, ctx) {
                self.repo_path_mapping
                    .insert(file_path.to_path_buf(), repo_root);
                local_file.watcher_type = WatcherType::Repository;
            } else if let Some(watch_path) = local_file.watch_path() {
                // Fallback to an individual watcher, registered exactly the way `open` does it so
                // that both entry points unregister the same path.
                self.register_individual_watcher(&watch_path, ctx);
                local_file.watcher_type = WatcherType::Individual(watch_path);
            }
        }

        self.file_state.insert_local(file_id, local_file);

        file_id
    }

    /// Watches `watch_path` for the benefit of one individually-watched file.
    fn register_individual_watcher(&mut self, watch_path: &Path, ctx: &mut ModelContext<Self>) {
        self.watcher.update(ctx, |watcher, _ctx| {
            std::mem::drop(watcher.register_path(
                watch_path,
                WatchFilter::accept_all(),
                RecursiveMode::NonRecursive,
            ));
        });
    }

    /// Open a file to get its content asynchronously. This also opts in to receiving file watcher updates.
    pub fn open(
        &mut self,
        file_path: &Path,
        subscribe_to_updates: bool,
        ctx: &mut ModelContext<Self>,
    ) -> FileId {
        let file_id = FileId::new();
        let mut local_file = LocalFile::new(file_path.to_owned());

        // Repository watching is set up before the read; an individual watcher can only be
        // registered once the file is known to exist, so it waits for the read to succeed.
        let mut watch_individually = false;
        if subscribe_to_updates {
            if let Some(repo_root) = self.get_or_create_repo_subscription(file_path, ctx) {
                self.repo_path_mapping
                    .insert(file_path.to_path_buf(), repo_root);
                local_file.watcher_type = WatcherType::Repository;
            } else {
                watch_individually = true;
            }
        }

        let file_path_buf = file_path.to_owned();
        let future = ctx.spawn(
            async move {
                let contents = async_fs::read_to_string(&file_path_buf)
                    .await
                    .map_err(FileLoadError::from);
                (file_id, contents)
            },
            move |me, (file_id, load_result), ctx| {
                // A read can land after the caller unsubscribed (closing or reopening the file
                // cancels, but the completion may already be queued). Registering a watcher or
                // emitting for a file that is no longer tracked would resurrect state that its
                // owner already tore down.
                if me.file_state.get(file_id).is_none() {
                    return;
                }
                match load_result {
                    Ok(content) => {
                        let version = ContentVersion::new();
                        me.set_version(file_id, version);

                        // Only register an individual watcher if not using a repo subscription,
                        // and only record it once it has actually been registered.
                        if watch_individually && let Some(watch_path) = me.watch_path(file_id) {
                            me.register_individual_watcher(&watch_path, ctx);
                            if let Some(FileBackend::Local(file)) = me.file_state.get_mut(file_id) {
                                file.watcher_type = WatcherType::Individual(watch_path);
                            }
                        }

                        ctx.emit(FileModelEvent::FileLoaded {
                            content,
                            id: file_id,
                            version,
                        });
                    }
                    Err(err) => {
                        ctx.emit(FileModelEvent::FailedToLoad {
                            id: file_id,
                            error: Rc::new(err),
                        });
                    }
                }
            },
        );

        self.file_state.insert_local(file_id, local_file);

        self.abort_handles.insert(file_id, future);
        file_id
    }

    /// The directory an individually-watched file is watched through.
    ///
    /// The parent directory is watched (rather than the file) so the watch survives editors that
    /// replace a file with delete+create or rename (vim, `sed -i`, ...), which would otherwise
    /// drop the watch along with the original inode.
    ///
    /// Derived from the stored path so registration and unregistration always agree. `None` when
    /// the file's path has no usable parent - a bare relative name such as `README.md` yields an
    /// empty parent, which platform watchers resolve to Warp's own process directory.
    fn watch_path(&self, file_id: FileId) -> Option<PathBuf> {
        Self::watch_path_for(self.file_state.get_local(file_id)?.path.as_deref()?)
    }

    /// See [`Self::watch_path`].
    fn watch_path_for(path: &Path) -> Option<PathBuf> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())?;
        Some(parent.to_path_buf())
    }

    pub async fn read_content_for_file(file_path: &Path) -> Result<String, FileLoadError> {
        if !Self::file_exists(file_path).await {
            return Err(FileLoadError::DoesNotExist);
        }
        async_fs::read_to_string(file_path)
            .await
            .map_err(FileLoadError::from)
    }

    /// Asynchronously reads specific lines from a file using BufReader.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file to read
    /// * `line_numbers` - A list of 0-based line numbers to retrieve. Supports non-consecutive lines.
    ///
    /// # Returns
    /// A vector of (line_number, line_content) tuples for each requested line that exists.
    /// Lines that don't exist in the file are omitted from the result.
    pub async fn read_lines_async(
        file_path: &Path,
        line_numbers: Vec<usize>,
    ) -> Result<Vec<(usize, String)>, FileLoadError> {
        use std::collections::HashSet;

        if line_numbers.is_empty() {
            return Ok(Vec::new());
        }

        if !Self::file_exists(file_path).await {
            return Err(FileLoadError::DoesNotExist);
        }

        let requested_lines: HashSet<usize> = line_numbers.iter().copied().collect();
        let max_line = *line_numbers.iter().max().unwrap();

        let file = async_fs::File::open(file_path)
            .await
            .map_err(FileLoadError::from)?;
        let reader = BufReader::new(file);
        let mut lines_stream = reader.lines();

        let mut result = Vec::with_capacity(line_numbers.len());
        let mut current_line = 0usize;

        while let Some(line_result) = lines_stream.next().await {
            if current_line > max_line {
                break;
            }

            if requested_lines.contains(&current_line) {
                let line = line_result.map_err(FileLoadError::from)?;
                result.push((current_line, line));
            }

            current_line += 1;
        }

        Ok(result)
    }

    /// Reads a text file by async-streaming its lines, respecting a byte budget
    /// and optional line ranges. Returns [`TextFileReadResult::NotText`] if the
    /// file is not valid UTF-8.
    ///
    /// **Line ending normalization**: `\n` and `\r\n` line endings are normalized
    /// to `\n` (LF) in the returned content. Classic Mac `\r`-only line endings
    /// are **not** recognized as line separators (matching `read_line()` behavior).
    /// A trailing newline at the end of the file is preserved so that
    /// round-tripping content through this reader and writing it back does not
    /// silently drop the final newline.
    ///
    /// This is a modified version of the loop that [`futures::io::BufReader::lines()`]
    /// uses internally (i.e. repeated `read_line()` calls with newline stripping),
    /// but additionally tracks whether each line was terminated by a newline so
    /// the accumulator can preserve the file's trailing newline.
    pub async fn read_text_file(
        path: &Path,
        max_bytes: usize,
        requested_ranges: &[Range<usize>],
        last_modified: Option<SystemTime>,
    ) -> anyhow::Result<TextFileReadResult> {
        let file = match async_fs::File::open(path).await {
            Ok(file) => file,
            Err(e) => return Err(anyhow::anyhow!(e)),
        };
        let mut reader = futures::io::BufReader::new(file);

        let file_name = path.to_string_lossy().to_string();
        let mut accumulator = text_file_reader::TextFileAccumulator::new(
            file_name,
            last_modified,
            requested_ranges,
            max_bytes,
        );

        // Use `read_line()` instead of `lines()` so we can detect whether each
        // line was terminated by a newline. `lines()` strips this information,
        // which caused trailing newlines to be silently dropped.
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            let bytes_read = match reader.read_line(&mut line_buf).await {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    // Not valid UTF-8.
                    return Ok(TextFileReadResult::NotText);
                }
                Err(e) => return Err(anyhow::anyhow!(e)),
            };
            if bytes_read == 0 {
                break; // EOF
            }

            // Strip the line terminator (`\n` or `\r\n`) and record whether
            // one was present. Note: `read_line()` only splits on `\n`, so
            // standalone `\r` (classic Mac) is not treated as a line separator.
            let has_newline = line_buf.ends_with('\n');
            if has_newline {
                line_buf.pop();
            }
            if line_buf.ends_with('\r') {
                line_buf.pop();
            }

            accumulator.push_line(std::mem::take(&mut line_buf), has_newline);
        }

        let (segments, bytes_read) = accumulator.finalize();
        Ok(TextFileReadResult::Segments {
            segments,
            bytes_read,
        })
    }

    pub async fn read_file_as_binary(file_path: &Path) -> Result<Vec<u8>, FileLoadError> {
        if !Self::file_exists(file_path).await {
            return Err(FileLoadError::DoesNotExist);
        }

        async_fs::read(file_path).await.map_err(FileLoadError::from)
    }

    pub async fn file_exists(file_path: &Path) -> bool {
        async_fs::metadata(file_path).await.is_ok()
    }

    pub async fn create_file(file_path: &Path) -> Result<(), io::Error> {
        async_fs::File::create(file_path).await.map(|_| ())
    }

    /// Ensures all parent directories of the given path exist, creating them if necessary.
    pub async fn ensure_parent_directories(path: &Path) -> Result<(), io::Error> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                async_fs::create_dir_all(parent).await?;
            }
        }
        Ok(())
    }

    pub fn cancel(&mut self, file_id: FileId) {
        if let Some(future) = self.abort_handles.remove(&file_id) {
            future.abort()
        }
    }

    pub fn unsubscribe(&mut self, file_id: FileId, ctx: &mut ModelContext<Self>) {
        self.abort_handles.remove(&file_id);
        if let Some((FileBackend::Local(file), path_still_used)) = self.file_state.remove(file_id) {
            let path = file.path;
            let watcher_type = file.watcher_type;

            if let Some(ref path) = path {
                if !path_still_used {
                    match watcher_type {
                        // Unwatch exactly the directory that was registered for this file, and
                        // only when no other individually-watched file is still using it.
                        WatcherType::Individual(watch_path) => {
                            let watch_path_still_used =
                                self.file_state.local_values().any(|file| {
                                    file.watcher_type.individual_watch_path()
                                        == Some(watch_path.as_path())
                                });
                            if !watch_path_still_used {
                                self.watcher.update(ctx, |watcher, _ctx| {
                                    std::mem::drop(watcher.unregister_path(&watch_path));
                                });
                            }
                        }
                        WatcherType::Repository => {
                            if let Some((repo_root, unused_repo)) =
                                self.repo_path_mapping.remove(path)
                            {
                                if unused_repo {
                                    self.unsubscribe_from_repo(&repo_root, ctx);
                                }
                            }
                        }
                        WatcherType::None => {}
                    }
                }
            }
        }
        // Remote files have no watcher to clean up.
    }

    /// Reports a completed save/delete/rename write: updates the tracked
    /// version and emits `FileSaved`/`FailedToSave` (unchanged behavior), and
    /// additionally resolves any [`FileModel::save_completion`] waiters for the
    /// file. The single funnel every write-completion callback routes through.
    fn report_save_outcome(
        &mut self,
        file_id: FileId,
        version: ContentVersion,
        result: Result<(), FileSaveError>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Build the waiter payload while the error is still borrowable; the
        // event keeps the original error (`Rc`), so the waiter gets an
        // equivalent user-facing message wrapped in a fresh `Other`.
        let waiter_result = match &result {
            Ok(()) => Ok(()),
            Err(FileSaveError::IOError { error, path }) => Err(Arc::new(FileSaveError::Other(
                format!("Failed to save file {path:?}: {error}"),
            ))),
            Err(other) => Err(Arc::new(FileSaveError::Other(other.to_string()))),
        };
        match result {
            Ok(()) => {
                self.set_version(file_id, version);
                ctx.emit(FileModelEvent::FileSaved {
                    id: file_id,
                    version,
                });
            }
            Err(err) => ctx.emit(FileModelEvent::FailedToSave {
                id: file_id,
                error: Rc::new(err),
            }),
        }
        if let Some(waiters) = self.save_waiters.remove(&file_id) {
            for tx in waiters {
                let _ = tx.send(waiter_result.clone());
            }
        }
    }

    /// Returns a future that resolves when the next save/delete/rename write
    /// dispatched for `file_id` actually completes — `Ok(())` on success or the
    /// failure otherwise. The save methods return as soon as the write is
    /// *scheduled* (completion is reported later via `FileSaved`/`FailedToSave`),
    /// so a caller that must observe the real outcome registers this waiter
    /// before dispatching, then awaits it.
    pub fn save_completion(
        &mut self,
        file_id: FileId,
    ) -> futures::future::BoxFuture<'static, Result<(), Arc<FileSaveError>>> {
        let (tx, rx) = oneshot::channel();
        self.save_waiters.entry(file_id).or_default().push(tx);
        async move {
            rx.await.unwrap_or_else(|_| {
                Err(Arc::new(FileSaveError::Other(
                    "File save was interrupted before completion".to_owned(),
                )))
            })
        }
        .boxed()
    }

    /// Writes `content` over the file, unconditionally.
    ///
    /// This makes **no** promise about the state of the file it replaces: there
    /// is no mtime check and no version compare, and `version` is only recorded
    /// after a successful write (see `report_save_outcome`) — it is
    /// never compared against anything. Whatever is on disk is gone.
    ///
    /// That is correct for a caller whose `content` is a live buffer the user is
    /// currently typing into. It is *not* correct for a caller whose `content`
    /// was derived from a snapshot read some time ago; that caller wants
    /// [`FileModel::save_if_unchanged`].
    pub fn save(
        &mut self,
        file_id: FileId,
        content: String,
        version: ContentVersion,
        ctx: &mut ModelContext<Self>,
    ) -> Result<(), FileSaveError> {
        let backend = self
            .file_state
            .get(file_id)
            .ok_or(FileSaveError::NoFilePath(file_id))?;

        match backend {
            FileBackend::Local(_) => {
                let file_path = self
                    .file_path(file_id)
                    .ok_or(FileSaveError::NoFilePath(file_id))?;

                ctx.spawn(
                    async move {
                        if let Err(err) = Self::ensure_parent_directories(&file_path).await {
                            return Err(FileSaveError::IOError {
                                error: err,
                                path: file_path,
                            });
                        }
                        async_fs::write(&file_path, content).await.map_err(|err| {
                            FileSaveError::IOError {
                                error: err,
                                path: file_path,
                            }
                        })
                    },
                    move |me, write_result: Result<(), FileSaveError>, ctx| {
                        me.report_save_outcome(file_id, version, write_result, ctx);
                    },
                );
            }
            FileBackend::Remote { host_id, path } => {
                let client = Self::resolve_remote_client(host_id, ctx)?;
                let path = path.as_str().to_string();
                let future = async move {
                    client
                        .write_file(path, content)
                        .await
                        .map_err(|e| e.to_string())
                };
                ctx.spawn(
                    future,
                    move |me, result: Result<(), String>, ctx| {
                        me.report_save_outcome(
                            file_id,
                            version,
                            result.map_err(FileSaveError::RemoteError),
                            ctx,
                        );
                    },
                );
            }
        }

        Ok(())
    }

    /// Writes `content` over the file, but only if the file is still in the
    /// state the caller says it was when `content` was derived.
    ///
    /// The guarded counterpart to [`FileModel::save`], for callers holding
    /// snapshot-derived content. The read, the comparison and the write all
    /// happen inside a single spawned task so the check sits as close to the
    /// write as it can; this narrows the window, it does not close it — nothing
    /// short of an OS-level exclusive open could, and the point here is to stop
    /// losing minutes-old external edits, not microsecond-old ones.
    ///
    /// On a mismatch nothing is written and the failure is reported through the
    /// ordinary [`FileModelEvent::FailedToSave`] path, as
    /// [`FileSaveError::Other`] carrying a message that says the write was
    /// abandoned and why. Silently declining to write would be a different bug
    /// from silently overwriting, and just as bad: the user must learn their
    /// accept did not land.
    ///
    /// The cost is one extra read of the file per guarded write. It is paid on
    /// a user-initiated action that was already going to write the same file,
    /// so it is not on any hot path.
    ///
    /// # Why the pre-image is the content, not an mtime
    ///
    /// mtime is coarse (whole seconds on some filesystems), is preserved
    /// verbatim by tools that legitimately rewrite a file (`cp -p`, `rsync
    /// --times`, `git checkout` on some paths), and can move without the
    /// content changing at all. It would both miss real conflicts and invent
    /// fake ones. More decisively, the caller this exists for — an AI diff
    /// view — has no mtime to compare against: its pre-image is the file text
    /// the diff was computed from, captured before any `FileModel` registration
    /// happened, so there is no earlier `stat` to have recorded. The content
    /// *is* the only thing that caller can honestly assert, and comparing it
    /// costs one read that the mtime route would largely have needed anyway.
    ///
    /// The comparison is against the content itself rather than a digest of it:
    /// the full read is unavoidable either way, so hashing would only save
    /// holding one transient copy of the expected text, in exchange for a new
    /// dependency and a collision surface — a bad trade for a check whose whole
    /// job is to be trustworthy.
    ///
    /// # Divergence from the pinned oracle
    ///
    /// This is **not** a parity port. Pinned Warp `42effe840` has no guarded
    /// write at all: `42effe840:crates/warp_files/src/lib.rs:725-760` is the
    /// same unconditional `async_fs::write`, and its
    /// `42effe840:app/src/code/inline_diff.rs:220-228` calls it with the whole
    /// stale editor buffer on accept. The oracle shares the defect and we are
    /// fixing it ahead of the oracle deliberately, because the loss is silent
    /// and unrecoverable — the user's own concurrent edits, gone with no
    /// message and no undo. A re-pin must not "restore parity" by reverting
    /// this or by re-pointing `InlineDiffView::save_content` at
    /// [`FileModel::save`].
    ///
    /// # Remote files
    ///
    /// Remote writes are guarded through the same protocol the write uses, with
    /// one weaker case called out in the code: the wire read cannot distinguish
    /// "file is missing" from "read failed", so an [`ExpectedDiskState::Absent`]
    /// check whose read errors proceeds rather than blocking every remote file
    /// creation. Local `Absent` has no such hole — `NotFound` is unambiguous.
    pub fn save_if_unchanged(
        &mut self,
        file_id: FileId,
        content: String,
        expected: ExpectedDiskState,
        version: ContentVersion,
        ctx: &mut ModelContext<Self>,
    ) -> Result<(), FileSaveError> {
        let backend = self
            .file_state
            .get(file_id)
            .ok_or(FileSaveError::NoFilePath(file_id))?;

        match backend {
            FileBackend::Local(_) => {
                let file_path = self
                    .file_path(file_id)
                    .ok_or(FileSaveError::NoFilePath(file_id))?;

                ctx.spawn(
                    async move {
                        let read_result = async_fs::read_to_string(&file_path).await;
                        if let PreWriteVerdict::Refuse(message) =
                            check_pre_image(&file_path, &expected, read_result)
                        {
                            return Err(FileSaveError::Other(message));
                        }

                        if let Err(err) = Self::ensure_parent_directories(&file_path).await {
                            return Err(FileSaveError::IOError {
                                error: err,
                                path: file_path,
                            });
                        }
                        async_fs::write(&file_path, content).await.map_err(|err| {
                            FileSaveError::IOError {
                                error: err,
                                path: file_path,
                            }
                        })
                    },
                    move |me, write_result: Result<(), FileSaveError>, ctx| {
                        me.report_save_outcome(file_id, version, write_result, ctx);
                    },
                );
            }
            FileBackend::Remote { host_id, path } => {
                let client = Self::resolve_remote_client(host_id, ctx)?;
                let path = path.as_str().to_string();
                let future = async move {
                    let read_result = client.read_file_bytes(path.clone()).await;
                    if let Some(message) = remote_pre_image_refusal(&path, &expected, read_result) {
                        return Err(message);
                    }
                    client
                        .write_file(path, content)
                        .await
                        .map_err(|e| e.to_string())
                };
                ctx.spawn(
                    future,
                    move |me, result: Result<(), String>, ctx| {
                        me.report_save_outcome(
                            file_id,
                            version,
                            result.map_err(FileSaveError::RemoteError),
                            ctx,
                        );
                    },
                );
            }
        }

        Ok(())
    }

    /// Renames a file and also saves its content.
    // TODO: refactor this against [`FileModel::save`].
    pub fn rename_and_save(
        &mut self,
        file_id: FileId,
        new_path: PathBuf,
        content: String,
        version: ContentVersion,
        ctx: &mut ModelContext<Self>,
    ) -> Result<(), FileSaveError> {
        let file_path = self
            .file_path(file_id)
            .ok_or(FileSaveError::NoFilePath(file_id))?;

        ctx.spawn(
            async move {
                // Make sure the file we're renaming exists.
                if let Err(err) = async_fs::metadata(&file_path).await {
                    return Err(FileSaveError::IOError {
                        error: err,
                        path: file_path,
                    });
                }

                // Make sure the components of the path we're moving to exists.
                if let Err(err) = Self::ensure_parent_directories(&new_path).await {
                    return Err(FileSaveError::IOError {
                        error: err,
                        path: new_path,
                    });
                }

                // Write the updated contents to the old path first.
                async_fs::write(&file_path, content).await.map_err(|err| {
                    FileSaveError::IOError {
                        error: err,
                        path: file_path.clone(),
                    }
                })?;

                // Now rename.
                async_fs::rename(&file_path, &new_path)
                    .await
                    .map_err(|err| FileSaveError::IOError {
                        error: err,
                        path: file_path.clone(),
                    })
            },
            move |me, write_result: Result<(), FileSaveError>, ctx| {
                me.report_save_outcome(file_id, version, write_result, ctx);
            },
        );

        Ok(())
    }

    /// Deletes the specified file.
    pub fn delete(
        &mut self,
        file_id: FileId,
        version: ContentVersion,
        ctx: &mut ModelContext<Self>,
    ) -> Result<(), FileSaveError> {
        let backend = self
            .file_state
            .get(file_id)
            .ok_or(FileSaveError::NoFilePath(file_id))?;

        match backend {
            FileBackend::Local(_) => {
                let file_path = self
                    .file_path(file_id)
                    .ok_or(FileSaveError::NoFilePath(file_id))?;

                ctx.spawn(
                    async move {
                        if let Err(err) = Self::ensure_parent_directories(&file_path).await {
                            return Err(FileSaveError::IOError {
                                error: err,
                                path: file_path,
                            });
                        }
                        async_fs::remove_file(&file_path).await.map_err(|err| {
                            FileSaveError::IOError {
                                error: err,
                                path: file_path,
                            }
                        })
                    },
                    move |me, delete_result: Result<(), FileSaveError>, ctx| {
                        me.report_save_outcome(file_id, version, delete_result, ctx);
                    },
                );
            }
            FileBackend::Remote { host_id, path } => {
                let client = Self::resolve_remote_client(host_id, ctx)?;
                let path = path.as_str().to_string();
                let future =
                    async move { client.delete_file(path).await.map_err(|e| e.to_string()) };
                ctx.spawn(
                    future,
                    move |me, result: Result<(), String>, ctx| {
                        me.report_save_outcome(
                            file_id,
                            version,
                            result.map_err(FileSaveError::RemoteError),
                            ctx,
                        );
                    },
                );
            }
        }

        Ok(())
    }

    /// Look up the `RemoteServerClient` for a given host at call time.
    fn resolve_remote_client(
        host_id: &HostId,
        ctx: &AppContext,
    ) -> Result<std::sync::Arc<RemoteServerClient>, FileSaveError> {
        RemoteServerManager::as_ref(ctx)
            .client_for_host(host_id)
            .cloned()
            .ok_or_else(|| {
                FileSaveError::RemoteError(format!("Remote host {host_id} is not connected"))
            })
    }

    pub fn set_version(&mut self, file_id: FileId, version: ContentVersion) {
        if let Some(backend) = self.file_state.get_mut(file_id) {
            backend.set_version(version);
        }
    }

    pub fn version(&self, file_id: FileId) -> Option<ContentVersion> {
        self.file_state.get(file_id).and_then(|b| b.version())
    }

    /// Checks if a repository subscription exists for the given path, and creates one if needed.
    /// Returns the repository root path if a subscription exists or was created.
    fn get_or_create_repo_subscription(
        &mut self,
        file_path: &Path,
        ctx: &mut ModelContext<Self>,
    ) -> Option<PathBuf> {
        // Check if we already have a subscription for a repo containing this path
        for repo_root in self.repo_subscriptions.keys() {
            if file_path.starts_with(repo_root) {
                return Some(repo_root.clone());
            }
        }

        // Try to find a repository for this path
        let repository =
            DetectedRepositories::as_ref(ctx).get_watched_repo_for_path(file_path, ctx)?;

        let repo_root = repository.as_ref(ctx).root_dir().to_local_path_lossy();

        // Create a new subscription
        let (repository_update_tx, repository_update_rx) = async_channel::unbounded();
        let start = repository.update(ctx, |repo, ctx| {
            repo.start_watching(
                RepositoryWatchMode::FilesystemOnly,
                Box::new(FileRepositorySubscriber {
                    repository_update_tx,
                }),
                ctx,
            )
        });
        let subscriber_id = start.subscriber_id;

        let repo_root_for_handler = repo_root.clone();
        let repository_for_handler = repository.clone();
        ctx.spawn(start.registration_future, move |me, result, ctx| match result {
            Ok(()) => {
                log::info!(
                    "FileModel subscribed to repository: {}",
                    repo_root_for_handler.display()
                );
            }
            Err(err) => {
                log::warn!(
                    "Failed to subscribe to repository {}: {}, falling back to individual file watchers",
                    repo_root_for_handler.display(),
                    err
                );

                repository_for_handler.update(ctx, |repo, ctx| {
                    repo.stop_watching(subscriber_id, ctx);
                });

                // Remove the subscription entry since it failed
                me.repo_subscriptions.remove(&repo_root_for_handler);

                // Fall back to individual file watchers for all files that were expecting this repo
                me.fallback_to_individual_watchers(&repo_root_for_handler, ctx);
            }
        });

        // Set up the stream handler for repository updates
        let repo_root_for_stream = repo_root.clone();
        ctx.spawn_stream_local(
            repository_update_rx,
            move |me, update, ctx| {
                me.handle_repository_update(&repo_root_for_stream, update, ctx);
            },
            |_, _| {},
        );

        // Store the subscription (subscriber_id is available immediately; registration completion is async)
        self.repo_subscriptions.insert(
            repo_root.clone(),
            RepositorySubscription {
                repository,
                subscriber_id: Some(subscriber_id),
            },
        );

        Some(repo_root)
    }

    /// Handles file updates from a repository subscription.
    fn handle_repository_update(
        &mut self,
        _repo_root: &Path,
        update: RepositoryUpdate,
        ctx: &mut ModelContext<Self>,
    ) {
        if update.is_empty() {
            return;
        }

        // Collect paths that need content reloading:
        // - Added files that match tracked paths
        // - Moved files where the to_target matches a tracked path
        let mut paths_to_reload: HashSet<PathBuf> = update
            .added_or_modified()
            .filter(|target| !target.is_ignored)
            .map(|target| target.path.clone())
            .collect();

        for to_target in update.moved.keys() {
            if !to_target.is_ignored {
                paths_to_reload.insert(to_target.path.clone());
            }
        }

        // Reload content for all affected paths
        self.reload_file_paths(paths_to_reload, ctx);

        // Handle deleted files - log that the file's backing is gone
        for target in &update.deleted {
            if target.is_ignored {
                continue;
            }
            let has_file = self
                .file_state
                .local_values()
                .any(|file| file.path.as_ref() == Some(&target.path));
            if has_file {
                log::info!(
                    "File's backing was deleted, file is now orphaned: {}",
                    target.path.display()
                );
            }
        }

        // Handle moved/renamed files - log that the file's backing was moved (for from_target)
        for (to_target, from_target) in &update.moved {
            if from_target.is_ignored {
                continue;
            }
            let has_file = self
                .file_state
                .local_values()
                .any(|file| file.path.as_ref() == Some(&from_target.path));
            if has_file {
                log::info!(
                    "File's backing was moved from {} to {}, file is now orphaned",
                    from_target.path.display(),
                    to_target.path.display()
                );
            }
        }
    }

    /// Reloads file content if the given path matches any tracked file.
    fn reload_file_paths(&mut self, file_paths: HashSet<PathBuf>, ctx: &mut ModelContext<Self>) {
        // First filter to keep only the paths that are tracked.
        let matching_files: Vec<PathBuf> = file_paths
            .into_iter()
            .filter_map(|path| {
                self.file_state
                    .local_values()
                    .any(|file| file.should_receive_update_for_path(&path))
                    .then_some(path)
            })
            .collect();

        if matching_files.is_empty() {
            return;
        }

        // Autoreload modified files.
        ctx.spawn(
            async move {
                let mut res = Vec::new();
                for file_path in matching_files {
                    if let Ok(content) = async_fs::read_to_string(&file_path).await {
                        res.push((file_path, content));
                    }
                }
                res
            },
            move |me, res, ctx| {
                for (file_path, content) in res {
                    let mut emitted_event = false;
                    for (file_id, file_state) in me.file_state.local_iter_mut() {
                        // Only set the new version of a file if it has opt-in to receiving updates.
                        if file_state.should_receive_update_for_path(&file_path) {
                            let new_version = ContentVersion::new();
                            ctx.emit(FileModelEvent::FileUpdated {
                                id: *file_id,
                                content: content.clone(),
                                base_version: file_state.version.expect("Version should be some"),
                                new_version,
                            });
                            emitted_event = true;
                            file_state.version = Some(new_version);
                        }
                    }

                    if !emitted_event {
                        log::warn!(
                            "{} is changed but there is no handler for the update event",
                            file_path.display()
                        );
                    }
                }
            },
        );
    }

    /// Falls back to individual file watchers for all files that were expecting to use the given repository.
    /// Called when a repository subscription fails.
    fn fallback_to_individual_watchers(&mut self, repo_root: &Path, ctx: &mut ModelContext<Self>) {
        let affected_paths = self.repo_path_mapping.paths_for_repo(repo_root);

        for path in affected_paths {
            self.repo_path_mapping.remove(&path);

            // A file with no usable watch directory cannot fall back to an individual watcher, so
            // it stops receiving updates rather than pretending to be watched.
            let watch_path = Self::watch_path_for(&path);
            for (_, file) in self.file_state.local_iter_mut() {
                if file.path.as_ref() == Some(&path) && file.watcher_type == WatcherType::Repository
                {
                    file.watcher_type = match &watch_path {
                        Some(watch_path) => WatcherType::Individual(watch_path.clone()),
                        None => WatcherType::None,
                    };
                }
            }

            if let Some(watch_path) = watch_path {
                self.register_individual_watcher(&watch_path, ctx);
            }
        }
    }

    /// Checks if any files still reference the given repository, and unsubscribes if not.
    fn unsubscribe_from_repo(&mut self, repo_root: &Path, ctx: &mut ModelContext<Self>) {
        if let Some(subscription) = self.repo_subscriptions.remove(repo_root) {
            log::info!(
                "No more files in repository {}, unsubscribing",
                repo_root.display()
            );
            if let Some(subscriber_id) = subscription.subscriber_id {
                subscription.repository.update(ctx, |repo, ctx| {
                    repo.stop_watching(subscriber_id, ctx);
                });
            }
        }
    }
}

impl Entity for FileModel {
    type Event = FileModelEvent;
}

impl SingletonEntity for FileModel {}

/// Subscriber for repository file change events, forwarding updates to FileModel.
struct FileRepositorySubscriber {
    repository_update_tx: Sender<RepositoryUpdate>,
}

impl RepositorySubscriber for FileRepositorySubscriber {
    fn on_scan(
        &mut self,
        _repository: &Repository,
        _ctx: &mut ModelContext<Repository>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Files are already loaded when subscription starts, so we don't need to do anything.
        Box::pin(async {})
    }

    fn on_files_updated(
        &mut self,
        _repository: &Repository,
        update: &RepositoryUpdate,
        _ctx: &mut ModelContext<Repository>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let tx = self.repository_update_tx.clone();
        let update = update.clone();
        Box::pin(async move {
            let _ = tx.send(update).await;
        })
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
