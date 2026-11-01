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

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use super::{Model, SecureStorage};

type Job = Box<dyn FnOnce(&dyn SecureStorage) + Send + 'static>;
type Factory = Arc<dyn Fn() -> Model + Send + Sync + 'static>;

#[derive(Default)]
struct State {
    factory: Option<Factory>,
    tx: Option<Sender<Job>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    factory: None,
    tx: None,
});

/// The eventual output of a closure submitted with [`run`].
pub struct BackgroundResult<R>(async_channel::Receiver<R>);

impl<R> BackgroundResult<R> {
    /// Resolves to the closure's output, or `None` if the worker went away without running it.
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

/// Runs `job` against secure storage on the worker thread, after every previously submitted job.
/// Returns `None` (without running `job`) if there is no worker; the caller then does the work
/// synchronously on its own handle.
pub fn run<R: Send + 'static>(
    job: impl FnOnce(&dyn SecureStorage) -> R + Send + 'static,
) -> Option<BackgroundResult<R>> {
    let (result_tx, result_rx) = async_channel::bounded(1);
    let mut job: Option<Job> = Some(Box::new(move |storage: &dyn SecureStorage| {
        let _ = result_tx.try_send(job(storage));
    }));

    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let factory = state.factory.clone()?;
    if state.tx.is_none() {
        let (tx, rx) = mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("secure-storage".into())
            .spawn(move || {
                // Built here, on the worker, so it never has to be `Send`.
                let storage = factory();
                while let Ok(job) = rx.recv() {
                    job(storage.as_ref());
                }
            });
        if let Err(err) = spawned {
            log::error!("Failed to start the secure-storage worker: {err}");
            return None;
        }
        state.tx = Some(tx);
    }
    let tx = state.tx.as_ref()?;
    // Sending while holding the lock keeps submission order identical to call order.
    match tx.send(job.take()?) {
        Ok(()) => Some(BackgroundResult(result_rx)),
        Err(_) => {
            state.tx = None;
            None
        }
    }
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
    let queued = run(move |storage| {
        if let Some(job) = for_worker.lock().ok().and_then(|mut j| j.take()) {
            let _ = tx.try_send(job(storage));
        }
    });
    match queued {
        Some(_) => Ok(BackgroundResult(rx)),
        None => {
            let job = slot.lock().ok().and_then(|mut j| j.take());
            let job = job.expect("a declined job is still in its slot");
            Err(job(fallback))
        }
    }
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
