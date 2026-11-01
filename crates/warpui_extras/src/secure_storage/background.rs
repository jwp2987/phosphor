//! Ordered, off-thread access to secure storage (#1309).
//!
//! The registered [`super::Model`] is a `Box<dyn SecureStorage>`, which is not `Send`, and every
//! call on it can block on D-Bus / the macOS Keychain / Credential Manager (seconds if the keyring
//! is locked and prompting). Reading it on the UI thread at launch, and writing it on every save,
//! stalled the window.
//!
//! The `register*` functions therefore also record a *factory* that builds a fresh, equivalent
//! handle. [`run`] sends a closure to one lazily-started worker thread that owns such a handle and
//! executes closures strictly in submission order, so a write queued before a read is observed by
//! it and two writes land in the order they were made. Results come back through a
//! [`BackgroundResult`], which a model awaits with `ctx.spawn`.
//!
//! When no factory is registered (`register_noop`, `register_unavailable`, or a test that installs
//! its own in-memory handle on the app) [`run`] returns `None` and the caller keeps its original
//! synchronous path against `ctx.secure_storage()`; those backends never block.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::{Model, SecureStorage};

type RunFn = Box<dyn FnOnce(&dyn SecureStorage) + Send + 'static>;
type AbandonFn = Arc<dyn Fn() + Send + Sync + 'static>;
type Factory = Arc<dyn Fn() -> Model + Send + Sync + 'static>;

/// One queued closure plus the means to resolve its caller's result as "gave up" without running
/// it (or while it is stuck running).
struct Job {
    run: RunFn,
    abandon: AbandonFn,
}

/// Default for how long one secure-storage call may run before the worker is abandoned
/// (`PHOSPHOR_SECURE_STORAGE_GIVE_UP_SECS`; `0` disables the give-up). A locked keyring that is
/// showing an unlock prompt can legitimately take a while, so this is generous: it is a safety
/// net for a call that never returns, not pacing.
const DEFAULT_GIVE_UP: Duration = Duration::from_secs(30);

static GIVE_UP_OVERRIDE: Mutex<Option<Duration>> = Mutex::new(None);

/// Test hook: override the give-up duration used by workers started afterwards.
#[doc(hidden)]
pub fn set_give_up_for_test(give_up: Option<Duration>) {
    *GIVE_UP_OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = give_up;
}

fn give_up_duration() -> Option<Duration> {
    if let Some(d) = *GIVE_UP_OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) {
        return Some(d);
    }
    match std::env::var("PHOSPHOR_SECURE_STORAGE_GIVE_UP_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
        None => Some(DEFAULT_GIVE_UP),
    }
}

/// What the watchdog of one worker generation watches.
struct Monitor {
    state: Mutex<MonitorState>,
    changed: Condvar,
}

#[derive(Default)]
struct MonitorState {
    /// The call currently executing on the worker, with when it started.
    running: Option<(Instant, AbandonFn)>,
    /// The worker gave up on / was retired; it must not run anything else.
    abandoned: bool,
    /// The worker thread exited normally.
    finished: bool,
}

#[derive(Default)]
struct State {
    factory: Option<Factory>,
    tx: Option<async_channel::Sender<Job>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    factory: None,
    tx: None,
});

static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Generation of the worker `STATE.tx` belongs to.
static CURRENT_GENERATION: AtomicU64 = AtomicU64::new(0);

/// The eventual output of a closure submitted with [`run`].
pub struct BackgroundResult<R>(async_channel::Receiver<R>);

impl<R> BackgroundResult<R> {
    /// Resolves to the closure's output, or `None` if the worker went away without running it or
    /// gave up on it (a call stuck for longer than the give-up duration).
    pub async fn wait(self) -> Option<R> {
        self.0.recv().await.ok()
    }
}

/// Records how to build a fresh handle on the worker thread. Replaces any earlier factory (and
/// retires the previous worker once its queue drains).
pub(super) fn set_factory(factory: impl Fn() -> Model + Send + Sync + 'static) {
    install_factory(Some(Arc::new(factory)));
}

pub(super) fn clear_factory() {
    install_factory(None);
}

fn install_factory(factory: Option<Factory>) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.factory = factory;
    state.tx = None;
}

/// Test hook: install (or clear, with `None`) the factory used by [`run`].
#[doc(hidden)]
pub fn set_factory_for_test(factory: Option<Arc<dyn Fn() -> Model + Send + Sync + 'static>>) {
    install_factory(factory);
}

/// Whether [`run`] will execute off-thread.
pub fn is_available() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .factory
        .is_some()
}

/// Starts a worker thread (and, unless disabled, its watchdog) and returns the queue feeding it.
fn spawn_worker(factory: Factory) -> Option<async_channel::Sender<Job>> {
    let (tx, rx) = async_channel::unbounded::<Job>();
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let monitor = Arc::new(Monitor {
        state: Mutex::new(MonitorState::default()),
        changed: Condvar::new(),
    });
    let worker_monitor = monitor.clone();
    let rx_for_watch = rx.clone();
    let spawned = std::thread::Builder::new()
        .name("secure-storage".into())
        .spawn(move || {
            // Built here, on the worker, so it never has to be `Send`. Building it can block too
            // (connecting to the keyring), so the watchdog covers it as a pseudo-call.
            let noop: AbandonFn = Arc::new(|| {});
            set_running(&worker_monitor, Some((Instant::now(), noop)));
            let storage = factory();
            set_running(&worker_monitor, None);
            while let Ok(job) = rx.recv_blocking() {
                let abandoned = worker_monitor
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .abandoned;
                if abandoned {
                    // Gave up while stuck: whatever was queued behind the stuck call has already
                    // been resolved as failed; do not run it late and out of order.
                    (job.abandon)();
                    continue;
                }
                set_running(&worker_monitor, Some((Instant::now(), job.abandon.clone())));
                (job.run)(storage.as_ref());
                set_running(&worker_monitor, None);
            }
            let mut st = worker_monitor
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            st.finished = true;
            worker_monitor.changed.notify_all();
        });
    if let Err(err) = spawned {
        log::error!("Failed to start the secure-storage worker: {err}");
        return None;
    }
    if let Some(give_up) = give_up_duration() {
        // A receiver clone, not a sender: the channel must still close (and the worker exit)
        // when the factory is replaced and every sender is dropped.
        let watch_rx = rx_for_watch;
        let watchdog = std::thread::Builder::new()
            .name("secure-storage-watchdog".into())
            .spawn(move || watch(monitor, generation, give_up, watch_rx));
        if let Err(err) = watchdog {
            log::warn!("Failed to start the secure-storage watchdog: {err}");
        }
    }
    CURRENT_GENERATION.store(generation, Ordering::SeqCst);
    Some(tx)
}

fn set_running(monitor: &Monitor, running: Option<(Instant, AbandonFn)>) {
    let mut st = monitor.state.lock().unwrap_or_else(|e| e.into_inner());
    st.running = running;
    monitor.changed.notify_all();
}

/// Give-up safety net (no event exists for "the OS keyring will never answer"): when one call has
/// been running for longer than `give_up`, retire this worker so later calls get a fresh one,
/// and resolve the stuck call and everything queued behind it as failed. Logged when it fires.
fn watch(
    monitor: Arc<Monitor>,
    generation: u64,
    give_up: Duration,
    jobs: async_channel::Receiver<Job>,
) {
    let mut st = monitor.state.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if st.finished {
            return;
        }
        match st.running.as_ref().map(|(since, _)| since.elapsed()) {
            None => {
                st = monitor.changed.wait(st).unwrap_or_else(|e| e.into_inner());
            }
            Some(elapsed) if elapsed < give_up => {
                st = monitor
                    .changed
                    .wait_timeout(st, give_up - elapsed)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            Some(_) => break,
        }
    }
    st.abandoned = true;
    let stuck = st.running.take();
    drop(st);
    log::error!(
        "secure storage: a call did not return within {give_up:?}; abandoning the worker \
         (the OS keyring is probably locked or unresponsive). The stuck call and everything \
         queued behind it fail; later calls use a fresh worker."
    );
    {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
            state.tx = None;
        }
        // Closing under the lock: a concurrent `run` either queued before this (and is drained
        // below) or sees the closed channel and starts a fresh worker.
        jobs.close();
    }
    if let Some((_, abandon)) = stuck {
        abandon();
    }
    // Resolve everything queued behind the stuck call as failed now (the channel is closed, so
    // nothing new can arrive); the abandoned worker must not run it late and out of order.
    while let Ok(job) = jobs.try_recv() {
        (job.abandon)();
    }
}

/// Queues a job; `false` (job dropped unrun) if there is no worker.
fn enqueue(run: RunFn, abandon: AbandonFn) -> bool {
    let mut job = Some(Job { run, abandon });
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(factory) = state.factory.clone() else {
        return false;
    };
    // Two attempts: the first can hit a worker that was abandoned an instant ago.
    for _ in 0..2 {
        if state.tx.is_none() {
            match spawn_worker(factory.clone()) {
                Some(tx) => state.tx = Some(tx),
                None => return false,
            }
        }
        let Some(tx) = state.tx.as_ref() else {
            return false;
        };
        // Sending while holding the lock keeps submission order identical to call order.
        match tx.try_send(job.take().expect("job present")) {
            Ok(()) => return true,
            Err(async_channel::TrySendError::Closed(j))
            | Err(async_channel::TrySendError::Full(j)) => {
                job = Some(j);
                state.tx = None;
            }
        }
    }
    false
}

/// Runs `job` against secure storage on the worker thread, after every previously submitted job.
/// Returns `None` (without running `job`) if there is no worker; the caller then does the work
/// synchronously on its own handle.
///
/// If a call on the worker never returns, after the give-up duration (30 s by default, see
/// `PHOSPHOR_SECURE_STORAGE_GIVE_UP_SECS`) the worker is abandoned: the stuck call and every call
/// queued behind it resolve to `None` from [`BackgroundResult::wait`], and later calls get a
/// fresh worker. The stuck OS call itself cannot be cancelled; if it ever returns, its result is
/// discarded (a stuck *write* may still land late).
pub fn run<R: Send + 'static>(
    job: impl FnOnce(&dyn SecureStorage) -> R + Send + 'static,
) -> Option<BackgroundResult<R>> {
    let (result_tx, result_rx) = async_channel::bounded(1);
    let closer = result_tx.clone();
    let queued = enqueue(
        Box::new(move |storage: &dyn SecureStorage| {
            let _ = result_tx.try_send(job(storage));
        }),
        Arc::new(move || {
            closer.close();
        }),
    );
    queued.then_some(BackgroundResult(result_rx))
}

/// Like [`run`], but when there is no worker the job runs right now, synchronously, against
/// `fallback` (the caller's own handle) and its output comes back as `Err`.
pub fn run_or_inline<R: Send + 'static, F>(
    job: F,
    fallback: &dyn SecureStorage,
) -> Result<BackgroundResult<R>, R>
where
    F: FnOnce(&dyn SecureStorage) -> R + Send + 'static,
{
    let slot = Arc::new(Mutex::new(Some(job)));
    let for_worker = slot.clone();
    let (tx, rx) = async_channel::bounded(1);
    let closer = tx.clone();
    let queued = enqueue(
        Box::new(move |storage: &dyn SecureStorage| {
            if let Some(job) = for_worker.lock().ok().and_then(|mut j| j.take()) {
                let _ = tx.try_send(job(storage));
            }
        }),
        Arc::new(move || {
            closer.close();
        }),
    );
    if queued {
        Ok(BackgroundResult(rx))
    } else {
        let job = slot.lock().ok().and_then(|mut j| j.take());
        let job = job.expect("a declined job is still in its slot");
        Err(job(fallback))
    }
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
