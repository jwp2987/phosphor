//! One filesystem watcher for the whole process.
//!
//! Every [`crate::BulkFilesystemWatcher`] used to own three threads (command, debouncer, and the
//! platform watcher's own event loop), and the app creates five or six of them at startup. Here
//! they share a single platform watcher, one command thread and one dispatch thread; a handle is a
//! subscription into that.
//!
//! * The command thread owns the platform watcher and performs the (potentially slow) register and
//!   unregister calls, so callers never block on them.
//! * The dispatch thread routes each raw event to the subscribers whose roots cover it and runs
//!   each subscriber's debounce window. The windows are exactly the per-handle windows of
//!   [`crate::run_debounce_loop`] (opened by the first relevant event, flushed when the duration
//!   has elapsed); the thread only waits for the nearest one. No timer is added.
//!
//! Each root keeps the filter it was registered with, because the platform watcher applies it per
//! root. That is only sound while roots do not overlap, so a path that overlaps an existing root
//! (see [`conflicts`]) is registered on a private per-subscription watcher instead and logged. The
//! shared watcher is the Linux path; other platforms report paths in forms (canonicalised, 8.3,
//! case-folded) that lexical routing cannot rely on, and keep the per-handle watchers.
//!
//! The last subscription dropped ends both threads; the next one starts them again.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, Weak,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use notify_debouncer_full::{
    DebouncedEvent,
    notify::{self, RecommendedWatcher, RecursiveMode, Watcher as _},
};

use crate::{
    BackgroundFileWatcherCommand, WatcherEventHandler, absorb_raw, deliver, is_relevant_event,
    spawn_private_watcher,
};

/// Environment variable that restores per-handle watchers when set to `0`.
pub(crate) const SHARED_WATCHER_ENV: &str = "PHOSPHOR_SHARED_WATCHER";

/// Whether the shared watcher is wanted. On by default on Linux only; `0` turns it off.
pub(crate) fn shared_watcher_enabled(env: Option<&str>) -> bool {
    cfg!(any(target_os = "linux", target_os = "android"))
        && !env.is_some_and(|value| value.trim() == "0")
}

type SubscriberId = u64;

/// A registered root, as the router and the overlap check see it.
#[derive(Clone)]
struct Root {
    /// The path as registered: platform events are reported under this form.
    path: PathBuf,
    /// The resolved form, for overlap checks only (symlinks can alias two registered paths).
    resolved: PathBuf,
    recursive: bool,
}

impl Root {
    fn new(path: &Path, mode: RecursiveMode) -> Self {
        Self {
            path: path.to_path_buf(),
            resolved: std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
            recursive: matches!(mode, RecursiveMode::Recursive),
        }
    }

    /// Whether an event at `event_path` belongs to this root: anywhere below it when recursive,
    /// otherwise the root itself and its immediate children.
    fn covers(&self, event_path: &Path) -> bool {
        if self.recursive {
            event_path.starts_with(&self.path)
        } else {
            event_path == self.path || event_path.parent() == Some(self.path.as_path())
        }
    }
}

/// Two roots conflict when the platform watcher cannot hold both with independent filters: the
/// same directory, or a recursive root that contains the other. A non-recursive root does not
/// reach into a root nested below it.
fn conflicts(a: &Root, b: &Root) -> bool {
    a.resolved == b.resolved
        || (a.recursive && b.resolved.starts_with(&a.resolved))
        || (b.recursive && a.resolved.starts_with(&b.resolved))
}

type Routes = Arc<Mutex<HashMap<SubscriberId, Vec<Root>>>>;

pub(crate) enum SharedCommand {
    NewSubscriber {
        id: SubscriberId,
        debounce: Duration,
        handler: WatcherEventHandler,
    },
    Path {
        id: SubscriberId,
        command: BackgroundFileWatcherCommand,
    },
    DropSubscriber {
        id: SubscriberId,
    },
}

enum DispatchMessage {
    Raw(notify::Result<notify::Event>),
    AddSubscriber {
        id: SubscriberId,
        debounce: Duration,
        handler: WatcherEventHandler,
    },
    RemoveSubscriber(SubscriberId),
}

/// The process-wide watcher: the sending half of the command thread.
pub(crate) struct SharedFsWatcher {
    commands: mpsc::Sender<SharedCommand>,
    next_id: AtomicUsize,
    /// Number of this instance's threads still running (for tests and the thread census).
    alive: Arc<AtomicUsize>,
}

/// The live instance, if any. Weak so that the last subscription ends the threads.
static GLOBAL: Mutex<Weak<SharedFsWatcher>> = Mutex::new(Weak::new());

impl SharedFsWatcher {
    /// The process-wide instance, started if no subscription holds one.
    pub(crate) fn acquire() -> Result<Arc<Self>> {
        let mut slot = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = slot.upgrade() {
            return Ok(existing);
        }
        let created = Arc::new(Self::start()?);
        *slot = Arc::downgrade(&created);
        Ok(created)
    }

    /// Starts a separate instance (the global one is [`Self::acquire`]).
    pub(crate) fn start() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel::<DispatchMessage>();
        let callback_tx = event_tx.clone();
        let watcher = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                let _ = callback_tx.send(DispatchMessage::Raw(event));
            },
            notify::Config::default(),
        )?;
        let (commands, command_rx) = mpsc::channel::<SharedCommand>();
        let routes: Routes = Arc::default();
        let alive = Arc::new(AtomicUsize::new(0));

        let dispatch_routes = routes.clone();
        let dispatch_guard = AliveGuard::new(&alive);
        thread::Builder::new()
            .name("Shared FS Dispatch".into())
            .spawn(move || {
                let _guard = dispatch_guard;
                run_dispatch(&event_rx, &dispatch_routes);
            })?;

        let command_guard = AliveGuard::new(&alive);
        thread::Builder::new()
            .name("Shared FS Commands".into())
            .spawn(move || {
                let _guard = command_guard;
                run_commands(&command_rx, watcher, &routes, &event_tx);
            })?;

        Ok(Self {
            commands,
            next_id: AtomicUsize::new(1),
            alive,
        })
    }

    #[cfg(test)]
    pub(crate) fn alive_threads(&self) -> usize {
        self.alive.load(Ordering::SeqCst)
    }

    /// Adds a subscriber with its own debounce window; events for its roots go to `handler`.
    pub(crate) fn subscribe(
        self: &Arc<Self>,
        debounce: Duration,
        handler: WatcherEventHandler,
    ) -> Subscription {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) as SubscriberId;
        let _ = self.commands.send(SharedCommand::NewSubscriber {
            id,
            debounce,
            handler,
        });
        Subscription {
            shared: self.clone(),
            id,
        }
    }
}

/// Decrements the instance's live-thread count when a thread exits, however it exits.
struct AliveGuard(Arc<AtomicUsize>);

impl AliveGuard {
    fn new(alive: &Arc<AtomicUsize>) -> Self {
        alive.fetch_add(1, Ordering::SeqCst);
        Self(alive.clone())
    }
}

impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// One handle's membership. Dropping it unregisters its roots and ends its debounce window.
pub(crate) struct Subscription {
    shared: Arc<SharedFsWatcher>,
    id: SubscriberId,
}

impl Subscription {
    pub(crate) fn send(&self, command: BackgroundFileWatcherCommand) -> Result<()> {
        self.shared
            .commands
            .send(SharedCommand::Path {
                id: self.id,
                command,
            })
            .map_err(|_| anyhow::anyhow!("the shared filesystem watcher has stopped"))
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let _ = self
            .shared
            .commands
            .send(SharedCommand::DropSubscriber { id: self.id });
    }
}

struct SubscriberEntry {
    debounce: Duration,
    handler: WatcherEventHandler,
    /// A private watcher for the paths that conflicted with a shared root, started on first use.
    fallback: Option<mpsc::Sender<BackgroundFileWatcherCommand>>,
}

fn run_commands(
    rx: &mpsc::Receiver<SharedCommand>,
    mut watcher: RecommendedWatcher,
    routes: &Routes,
    dispatch: &mpsc::Sender<DispatchMessage>,
) {
    fn lock(routes: &Routes) -> MutexGuard<'_, HashMap<SubscriberId, Vec<Root>>> {
        routes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    let mut subscribers: HashMap<SubscriberId, SubscriberEntry> = HashMap::new();
    while let Ok(command) = rx.recv() {
        match command {
            SharedCommand::NewSubscriber {
                id,
                debounce,
                handler,
            } => {
                lock(routes).insert(id, Vec::new());
                let _ = dispatch.send(DispatchMessage::AddSubscriber {
                    id,
                    debounce,
                    handler: handler.clone(),
                });
                subscribers.insert(
                    id,
                    SubscriberEntry {
                        debounce,
                        handler,
                        fallback: None,
                    },
                );
            }
            SharedCommand::Path { id, command } => {
                let Some(entry) = subscribers.get_mut(&id) else {
                    continue;
                };
                match command {
                    BackgroundFileWatcherCommand::AddPath {
                        path,
                        filter,
                        response,
                        recursive_mode,
                    } => {
                        let root = Root::new(&path, recursive_mode);
                        let conflict = lock(routes)
                            .values()
                            .flatten()
                            .any(|other| conflicts(other, &root));
                        if conflict {
                            log::info!(
                                "shared filesystem watcher: {path:?} overlaps a watched root; \
                                 using a private watcher for it"
                            );
                            let fallback = entry.fallback.get_or_insert_with(|| {
                                spawn_private_watcher(entry.debounce, entry.handler.clone())
                            });
                            let forwarded = BackgroundFileWatcherCommand::AddPath {
                                path,
                                filter,
                                response,
                                recursive_mode,
                            };
                            if let Err(mpsc::SendError(BackgroundFileWatcherCommand::AddPath {
                                response,
                                ..
                            })) = fallback.send(forwarded)
                            {
                                let _ = response.send(Err(anyhow::anyhow!(
                                    "the private filesystem watcher has stopped"
                                )));
                            }
                            continue;
                        }
                        // Routable before the first event can arrive.
                        lock(routes).entry(id).or_default().push(root);
                        let result = watcher
                            .watch_filtered(&path, recursive_mode, filter)
                            .inspect_err(|err| log::warn!("Failed to watch path: {err:?}"))
                            .map_err(anyhow::Error::new);
                        if result.is_err() {
                            if let Some(roots) = lock(routes).get_mut(&id) {
                                roots.retain(|r| r.path != path);
                            }
                        }
                        let _ = response.send(result);
                    }
                    BackgroundFileWatcherCommand::RemovePath { path, response } => {
                        let shared_root = lock(routes)
                            .get(&id)
                            .is_some_and(|roots| roots.iter().any(|r| r.path == path));
                        if !shared_root {
                            if let Some(fallback) = &entry.fallback {
                                if let Err(mpsc::SendError(
                                    BackgroundFileWatcherCommand::RemovePath { response, .. },
                                )) = fallback.send(BackgroundFileWatcherCommand::RemovePath {
                                    path,
                                    response,
                                }) {
                                    let _ = response.send(Err(anyhow::anyhow!(
                                        "the private filesystem watcher has stopped"
                                    )));
                                }
                                continue;
                            }
                        }
                        let result = watcher
                            .unwatch(&path)
                            .inspect_err(|err| log::warn!("Failed to remove repo watcher: {err:?}"))
                            .map_err(anyhow::Error::new);
                        if let Some(roots) = lock(routes).get_mut(&id) {
                            roots.retain(|r| r.path != path);
                        }
                        let _ = response.send(result);
                    }
                }
            }
            SharedCommand::DropSubscriber { id } => {
                let roots = lock(routes).remove(&id).unwrap_or_default();
                for root in roots {
                    let _ = watcher.unwatch(&root.path);
                }
                // Dropping the entry drops the private watcher's command sender, which ends it.
                subscribers.remove(&id);
                let _ = dispatch.send(DispatchMessage::RemoveSubscriber(id));
            }
        }
    }
    log::debug!("Shared filesystem watcher command stream closed");
}

/// One subscriber's debounce window.
struct SubscriberState {
    debounce: Duration,
    handler: WatcherEventHandler,
    events: Vec<DebouncedEvent>,
    errors: Vec<notify::Error>,
    /// When the open window closes; `None` while idle.
    deadline: Option<Instant>,
}

impl SubscriberState {
    fn absorb(&mut self, item: notify::Result<notify::Event>) {
        absorb_raw(item, &mut self.events, &mut self.errors);
        if self.deadline.is_none() && !(self.events.is_empty() && self.errors.is_empty()) {
            self.deadline = Some(Instant::now() + self.debounce);
        }
    }

    fn flush(&mut self) {
        self.deadline = None;
        deliver(
            &mut self.handler,
            std::mem::take(&mut self.events),
            std::mem::take(&mut self.errors),
        );
    }
}

fn run_dispatch(rx: &mpsc::Receiver<DispatchMessage>, routes: &Routes) {
    let mut subscribers: HashMap<SubscriberId, SubscriberState> = HashMap::new();
    loop {
        let nearest = subscribers.values().filter_map(|s| s.deadline).min();
        let message = match nearest {
            // Idle: no timer, wake only for a message.
            None => match rx.recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            },
            Some(deadline) => {
                match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(message) => Some(message),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        };
        match message {
            Some(DispatchMessage::AddSubscriber {
                id,
                debounce,
                handler,
            }) => {
                subscribers.insert(
                    id,
                    SubscriberState {
                        debounce,
                        handler,
                        events: Vec::new(),
                        errors: Vec::new(),
                        deadline: None,
                    },
                );
            }
            Some(DispatchMessage::RemoveSubscriber(id)) => {
                if let Some(mut state) = subscribers.remove(&id) {
                    state.flush();
                }
            }
            Some(DispatchMessage::Raw(Ok(event))) => {
                if !is_relevant_event(&event) {
                    continue;
                }
                let routes = routes.lock().unwrap_or_else(|e| e.into_inner());
                for (id, roots) in routes.iter() {
                    if !event
                        .paths
                        .iter()
                        .any(|p| roots.iter().any(|root| root.covers(p)))
                    {
                        continue;
                    }
                    if let Some(state) = subscribers.get_mut(id) {
                        state.absorb(Ok(event.clone()));
                    }
                }
            }
            Some(DispatchMessage::Raw(Err(error))) => {
                // Errors carry no reliable root, so every subscriber hears them (they are logged).
                for state in subscribers.values_mut() {
                    state.absorb(Err(notify::Error::generic(&error.to_string())));
                }
            }
            None => {}
        }
        let now = Instant::now();
        for state in subscribers.values_mut() {
            if state.deadline.is_some_and(|deadline| deadline <= now) {
                state.flush();
            }
        }
    }
    for state in subscribers.values_mut() {
        state.flush();
    }
}

#[cfg(test)]
#[path = "shared_tests.rs"]
mod tests;
