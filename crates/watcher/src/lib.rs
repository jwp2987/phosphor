use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError, channel},
    thread,
    time::{Duration, Instant},
};

pub mod home_watcher;
pub use home_watcher::{HomeDirectoryWatcher, HomeDirectoryWatcherEvent};

use anyhow::Result;
use futures::channel::oneshot;
use notify_debouncer_full::{
    DebounceEventHandler, DebounceEventResult, DebouncedEvent, Debouncer, NoCache,
    new_debouncer_opt,
    notify::{
        self, EventKind, RecommendedWatcher, RecursiveMode, WatchFilter, Watcher as _,
        event::{ModifyKind, RenameMode},
    },
};
use warpui::{Entity, ModelContext};

/// Whether a raw watcher event can change what [`deduplicate_and_merge_raw_notifier_events`]
/// reports. Everything else (file opens/closes, metadata-only changes, backend meta events) must
/// not start a debounce window, or an unrelated read of a watched file would wake the thread.
/// Renames are kept in every form; [`normalize_renames`] removes the redundant halves of pairs.
fn is_relevant_event(event: &notify::Event) -> bool {
    if event.paths.is_empty() {
        return false;
    }
    match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any) => true,
        EventKind::Modify(ModifyKind::Name(_)) => true,
        _ => false,
    }
}

/// Collapses a paired rename to the single event that carries both names.
///
/// Backends such as inotify report a rename `A -> B` as `From(A)`, `To(B)` and then `Both(A, B)`.
/// [`deduplicate_and_merge_raw_notifier_events`] understands each form, but replaying all three
/// records the move twice and a chained rename `A -> B -> C` then overwrites the source: the
/// second `Both(B, C)` would be recorded as `C <- B` after `From/To` already produced `C <- A`.
/// So for every `Both(a, b)` the `From(a)` and `To(b)` halves that precede it are removed. A
/// backend that reports only `Both` is untouched, and an unpaired `From` or `To` (a move into or
/// out of the watched tree) is kept for the merge to treat as before.
fn normalize_renames(events: Vec<DebouncedEvent>) -> Vec<DebouncedEvent> {
    let is_name = |event: &DebouncedEvent, mode: RenameMode| matches!(event.event.kind, EventKind::Modify(ModifyKind::Name(m)) if m == mode);
    let mut drop = vec![false; events.len()];
    for (index, event) in events.iter().enumerate() {
        if !is_name(event, RenameMode::Both) || event.event.paths.len() < 2 {
            continue;
        }
        let (from, to) = (&event.event.paths[0], &event.event.paths[1]);
        let find = |mode: RenameMode, path: &PathBuf, drop: &[bool]| {
            (0..index).rev().find(|&i| {
                !drop[i] && is_name(&events[i], mode) && events[i].event.paths.first() == Some(path)
            })
        };
        if let Some(i) = find(RenameMode::From, from, &drop) {
            drop[i] = true;
        }
        if let Some(i) = find(RenameMode::To, to, &drop) {
            drop[i] = true;
        }
    }
    events
        .into_iter()
        .zip(drop)
        .filter_map(|(event, dropped)| (!dropped).then_some(event))
        .collect()
}

/// Event-driven debounce loop for one watcher.
///
/// Blocks on `rx` with no timer while nothing happens, so an idle watcher costs zero wakeups
/// (the debouncer this replaces polled on a fixed tick for the whole session: ~19 wakeups/s
/// across the app's five watchers at idle). The first relevant event opens a window of
/// `timeout`; everything that arrives inside it is flushed together, in arrival order, when the
/// window closes, so a burst of churn costs at most one flush per `timeout`. Returns when the
/// sending side (the platform watcher) is dropped, after flushing what is pending.
fn run_debounce_loop(
    rx: &mpsc::Receiver<notify::Result<notify::Event>>,
    timeout: Duration,
    mut flush: impl FnMut(Vec<DebouncedEvent>, Vec<notify::Error>),
) {
    let mut flush =
        move |events: Vec<DebouncedEvent>, errors| flush(normalize_renames(events), errors);
    let mut events: Vec<DebouncedEvent> = Vec::new();
    let mut errors: Vec<notify::Error> = Vec::new();
    fn absorb(
        item: notify::Result<notify::Event>,
        events: &mut Vec<DebouncedEvent>,
        errors: &mut Vec<notify::Error>,
    ) {
        match item {
            Ok(event) if is_relevant_event(&event) => {
                events.push(DebouncedEvent::new(event, Instant::now()))
            }
            Ok(_) => {}
            Err(error) => errors.push(error),
        }
    }

    'outer: loop {
        // Idle: no timer, wake only for an event.
        match rx.recv() {
            Ok(item) => absorb(item, &mut events, &mut errors),
            Err(_) => break,
        }
        if events.is_empty() && errors.is_empty() {
            continue;
        }
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(item) => absorb(item, &mut events, &mut errors),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    flush(std::mem::take(&mut events), std::mem::take(&mut errors));
                    break 'outer;
                }
            }
        }
        flush(std::mem::take(&mut events), std::mem::take(&mut errors));
    }
}

#[derive(Debug)]
enum BackgroundFileWatcherCommand {
    AddPath {
        path: PathBuf,
        filter: WatchFilter,
        response: oneshot::Sender<Result<()>>,
        recursive_mode: RecursiveMode,
    },
    RemovePath {
        path: PathBuf,
        response: oneshot::Sender<Result<()>>,
    },
}

/// Environment variable that restores the polling `notify_debouncer_full` debouncer in place of
/// the event-driven loop (any value other than empty or `0`).
const LEGACY_DEBOUNCER_ENV: &str = "PHOSPHOR_WATCHER_LEGACY_DEBOUNCER";

fn legacy_debouncer_requested() -> bool {
    std::env::var(LEGACY_DEBOUNCER_ENV).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Shortest tick for the legacy debouncer's polling thread.
const MIN_DEBOUNCER_TICK: Duration = Duration::from_millis(250);

/// Tick of the legacy debouncer: half the timeout, at least `MIN_DEBOUNCER_TICK`, never above the
/// timeout (which `new_debouncer_opt` rejects).
fn debouncer_tick(timeout: Duration) -> Duration {
    (timeout / 2).max(MIN_DEBOUNCER_TICK).min(timeout)
}

/// The platform watcher plus whichever debouncing sits behind it.
enum Notifier {
    /// Raw watcher feeding [`run_debounce_loop`] on its own thread.
    EventDriven(RecommendedWatcher),
    /// `notify_debouncer_full`'s polling debouncer (the kill-switch path).
    Legacy(Debouncer<RecommendedWatcher, NoCache>),
}

impl Notifier {
    fn watch_filtered(
        &mut self,
        path: &Path,
        mode: RecursiveMode,
        filter: WatchFilter,
    ) -> notify::Result<()> {
        match self {
            Notifier::EventDriven(w) => w.watch_filtered(path, mode, filter),
            Notifier::Legacy(d) => d.watch_filtered(path, mode, filter),
        }
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Notifier::EventDriven(w) => w.unwatch(path),
            Notifier::Legacy(d) => d.unwatch(path),
        }
    }
}

struct BackgroundFileWatcher {
    notifier: Notifier,
    rx: mpsc::Receiver<BackgroundFileWatcherCommand>,
}

impl BackgroundFileWatcher {
    fn new(
        debounce_duration: Duration,
        handler: WatcherEventHandler,
        rx: mpsc::Receiver<BackgroundFileWatcherCommand>,
    ) -> Result<Self> {
        if legacy_debouncer_requested() {
            log::info!("{LEGACY_DEBOUNCER_ENV} set: using the polling filesystem debouncer");
            let debouncer = new_debouncer_opt(
                debounce_duration,
                Some(debouncer_tick(debounce_duration)),
                handler,
                NoCache,
                notify::Config::default(),
            )?;
            return Ok(Self {
                notifier: Notifier::Legacy(debouncer),
                rx,
            });
        }

        let (event_tx, event_rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let notifier = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                let _ = event_tx.send(event);
            },
            notify::Config::default(),
        )?;

        let mut handler = handler;
        thread::Builder::new()
            .name("Filesystem Debouncer".into())
            .spawn(move || {
                run_debounce_loop(&event_rx, debounce_duration, |events, errors| {
                    if !events.is_empty() {
                        handler.handle_event(Ok(events));
                    }
                    if !errors.is_empty() {
                        handler.handle_event(Err(errors));
                    }
                });
            })?;

        Ok(Self {
            notifier: Notifier::EventDriven(notifier),
            rx,
        })
    }

    /// Listen to streamed in commands to modify the internal notifier state.
    fn run(mut self) {
        while let Ok(res) = self.rx.recv() {
            match res {
                BackgroundFileWatcherCommand::AddPath {
                    path,
                    filter,
                    response,
                    recursive_mode,
                } => {
                    let _ = response.send(
                        self.notifier
                            .watch_filtered(&path, recursive_mode, filter)
                            .inspect_err(|err| {
                                log::warn!("Failed to watch path: {err:?}");
                            })
                            .map_err(anyhow::Error::new),
                    );
                }
                BackgroundFileWatcherCommand::RemovePath { path, response } => {
                    let _ = response.send(
                        self.notifier
                            .unwatch(&path)
                            .inspect_err(|err| {
                                log::warn!("Failed to remove repo watcher: {err:?}");
                            })
                            .map_err(anyhow::Error::new),
                    );
                }
            }
        }
        log::debug!("File watcher stream closed")
    }
}

#[derive(Clone, Default, Debug)]
pub struct BulkFilesystemWatcherEvent {
    /// Paths that were created.
    pub added: HashSet<PathBuf>,

    /// Paths whose contents were modified.
    pub modified: HashSet<PathBuf>,

    /// List of paths that should be removed.
    pub deleted: HashSet<PathBuf>,

    /// Mapping from rename target to rename source.
    pub moved: HashMap<PathBuf, PathBuf>,
}

impl BulkFilesystemWatcherEvent {
    /// Iterator over paths that were added or modified.
    pub fn added_or_updated_iter(&self) -> impl Iterator<Item = &PathBuf> {
        self.added.iter().chain(self.modified.iter())
    }

    /// Returns an owned set of paths that were added or modified.
    ///
    /// Prefer `added_or_updated_iter` when you don't need ownership.
    pub fn added_or_updated_set(&self) -> HashSet<PathBuf> {
        self.added_or_updated_iter().cloned().collect()
    }

    fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.modified.is_empty()
            && self.deleted.is_empty()
            && self.moved.is_empty()
    }
}

/// Validates a path before it reaches the platform watcher.
///
/// An empty path is not a location, and the platform backends handle it badly enough to be fatal:
/// on macOS `notify` cannot build a CoreFoundation URL for it and then releases a null `CFError`,
/// which traps and takes the whole process down. Rejecting it here keeps a caller's bad path from
/// being a crash.
fn ensure_watchable_path(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        anyhow::bail!("the filesystem watcher cannot watch an empty path");
    }
    Ok(path.to_path_buf())
}

/// Model for watching for all file / folder changes under target directories.
/// The updates are debounced with a configurable duration.
pub struct BulkFilesystemWatcher {
    tx: mpsc::Sender<BackgroundFileWatcherCommand>,
}

impl BulkFilesystemWatcher {
    pub fn new(debounce_duration: Duration, ctx: &mut ModelContext<Self>) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let (bg_tx, bg_rx) = channel();

        // Note that we keep the file watcher in the background since registering and unregistering file path
        // involves fs calls.
        if let Err(e) = thread::Builder::new()
            .name("Bulk Filesystem Watcher".into())
            .spawn(move || {
                match BackgroundFileWatcher::new(
                    debounce_duration,
                    WatcherEventHandler { tx },
                    bg_rx,
                ) {
                    Ok(watcher) => watcher.run(),
                    Err(e) => {
                        // The thread exits, which drops `bg_rx` and the
                        // event-channel sender. Subsequent `register_path` /
                        // `unregister_path` calls will get `SendError` and
                        // log a warning — the app continues without file
                        // watching.
                        log::error!(
                            "Failed to create filesystem watcher, \
                             file watching will be disabled: {e:?}"
                        );
                    }
                }
            })
        {
            log::error!("Failed to spawn thread for background file watcher {e:?}");
        }
        ctx.spawn_stream_local(rx, Self::handle_watcher_event, |_, _| {});

        Self { tx: bg_tx }
    }

    pub fn new_for_test() -> Self {
        let (bg_tx, _) = channel();
        Self { tx: bg_tx }
    }

    /// Stop watching a path. The returned future resolves once the path is fully unregistered.
    /// Awaiting the future is *not* required for the path to be unregistered.
    pub fn unregister_path(&mut self, path: &Path) -> impl Future<Output = Result<()>> + use<> {
        let (tx, rx) = oneshot::channel();
        let send_result = ensure_watchable_path(path).and_then(|path| {
            self.tx
                .send(BackgroundFileWatcherCommand::RemovePath { path, response: tx })
                .map_err(anyhow::Error::new)
        });

        if let Err(error) = &send_result {
            log::warn!("Failed to unregister watched path: {error:#}");
        }

        async move {
            send_result?;
            rx.await.map_err(anyhow::Error::new)??;
            Ok(())
        }
    }

    /// Add a new path to watch. The returned future resolves once the path is fully registered.
    /// Awaiting the future is *not* required for the path to be registered.
    pub fn register_path(
        &mut self,
        path: &Path,
        watch_filter: WatchFilter,
        recursive_mode: RecursiveMode,
    ) -> impl Future<Output = Result<()>> + use<> {
        let (tx, rx) = oneshot::channel();
        let send_result = ensure_watchable_path(path).and_then(|path| {
            self.tx
                .send(BackgroundFileWatcherCommand::AddPath {
                    path,
                    filter: watch_filter,
                    response: tx,
                    recursive_mode,
                })
                .map_err(anyhow::Error::new)
        });

        if let Err(error) = &send_result {
            log::warn!("Failed to register watched path: {error:#}");
        }

        async move {
            send_result?;
            rx.await.map_err(anyhow::Error::new)??;
            Ok(())
        }
    }

    fn handle_watcher_event(
        &mut self,
        event: BulkFilesystemWatcherEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        ctx.emit(event);
    }
}

impl Entity for BulkFilesystemWatcher {
    type Event = BulkFilesystemWatcherEvent;
}

struct WatcherEventHandler {
    tx: async_channel::Sender<BulkFilesystemWatcherEvent>,
}

impl DebounceEventHandler for WatcherEventHandler {
    fn handle_event(&mut self, result: DebounceEventResult) {
        match result {
            Ok(debounce_events) => {
                if let Ok(config_event) =
                    deduplicate_and_merge_raw_notifier_events(&debounce_events)
                {
                    if let Err(e) = self.tx.try_send(config_event) {
                        log::warn!("Failed to send WatcherEvent: {e:?}");
                    }
                }
            }
            Err(e) => {
                log::warn!("Received error in repo watcher: {e:?}");
            }
        }
    }
}

/// Dedupe and standardize the raw notifier events into a BulkFilesystemWatcherEvent.
fn deduplicate_and_merge_raw_notifier_events(
    raw_fs_events: &[DebouncedEvent],
) -> Result<BulkFilesystemWatcherEvent> {
    let mut update = BulkFilesystemWatcherEvent::default();

    let mut created: HashSet<PathBuf> = HashSet::new();
    let mut modified: HashSet<PathBuf> = HashSet::new();

    let mut rename_from = None;
    for fs_event in raw_fs_events {
        match fs_event.event.kind {
            // Create and modify should always be preserved.
            EventKind::Create(_) => created.extend(fs_event.event.paths.clone()),
            // On Windows, ReadDirectoryChangesW emits ModifyKind::Any instead of
            // ModifyKind::Data for file content changes. Handle both variants.
            EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any) => {
                modified.extend(fs_event.event.paths.clone())
            }

            // If a path is created and then removed, we should not keep this path in the update event.
            // If a path is modified / moved and then removed, we should only keep the remove event.
            EventKind::Remove(_) => {
                for path in &fs_event.event.paths {
                    if created.remove(path) {
                        continue;
                    }

                    modified.remove(path);

                    // If a path is modified, remove the source instead of the target name.
                    update.deleted.insert(match update.moved.remove(path) {
                        Some(source) => source,
                        None => path.clone(),
                    });
                }
            }

            // Note that in MacOS, deleting could be either 1) a true removal 2) moving to trash. In the
            // second case, this event will come in as a EventKind::Modify on the name with RenameMode::Any.
            // Here we count this as a OutlineUpdate::remove.
            //
            // Another case of EventKind::Modify(ModifyKind::Name(RenameMode::Any)) is when a file was renamed
            // when we turn off file map caching. Since we cannot guarantee mapping from the old name to the new name
            // (this is ordering is unfortunately not strictly persisted in the event. e.g. rename A -> B and B -> A might
            // create an event stream of rename A, rename A, rename B, rename B), we will treating them as inserts and removes
            // for now based on the current state of the file system.
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)) => {
                let is_rename = matches!(
                    fs_event.event.kind,
                    EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                );
                for path in &fs_event.event.paths {
                    // Decides whether this is a rename to or rename from based on the current state of the file system.
                    // This is not ideal since when we receive the event, the state of the file system could have changed.
                    // E.g. rename A -> B, B -> C, if we receives the first event after B is already renamed to C, this will
                    // translate the events to delete (A, B), create (C).
                    // TODO(kevin)
                    let path_exists = is_rename && path.exists();

                    if path_exists {
                        created.insert(path.clone());
                        continue;
                    }

                    if created.remove(path) {
                        continue;
                    }

                    modified.remove(path);

                    // If a path is modified, remove the source instead of the target name.
                    update.deleted.insert(match update.moved.remove(path) {
                        Some(source) => source,
                        None => path.clone(),
                    });
                }
            }

            // If a path is renamed, we should check if it has been renamed in this update before and squash
            // any sequential renames.
            EventKind::Modify(ModifyKind::Name(rename_mode)) => 'rename: {
                let paths = &fs_event.event.paths;

                let (from, to) = match rename_mode {
                    RenameMode::From if !paths.is_empty() => {
                        rename_from = Some(paths.first().expect("Checked above").clone());
                        break 'rename;
                    }
                    RenameMode::To if !paths.is_empty() && rename_from.is_some() => (
                        rename_from.take().expect("Checked above"),
                        paths.first().expect("Checked above").clone(),
                    ),
                    RenameMode::Both if paths.len() > 1 => (
                        paths.first().expect("Checked above").clone(),
                        paths.get(1).expect("Checked above").clone(),
                    ),
                    _ => break 'rename,
                };

                match update.moved.remove(&from) {
                    Some(source) => update.moved.insert(to, source),
                    None => update.moved.insert(to, from),
                };
            }
            _ => (),
        }
    }

    // A path that is created and then modified within the debounce window should be considered "added".
    for path in &created {
        modified.remove(path);
    }

    update.added = created;
    update.modified = modified;

    if update.is_empty() {
        return Err(anyhow::anyhow!("No update event produced"));
    }

    Ok(update)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
