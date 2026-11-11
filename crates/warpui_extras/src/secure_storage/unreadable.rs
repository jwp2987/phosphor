//! Process-wide "a stored value exists but could not be opened" record (#2095, follow-up to
//! #1932).
//!
//! The Linux fallback keeps each value in an encrypted file. A file that is there but cannot be
//! opened (it was sealed with the install secret in the keyring and the keyring does not answer,
//! its key file is lost or was replaced, it is damaged, or a newer build wrote it) used to read as
//! "not found": the user saw empty credentials and nothing said why. The backend now records the
//! event here, and the app turns the first one of a run into one notice.
//!
//! Only the fact is recorded: a count. No key name, no path and nothing of a value ever enters
//! this module, so nothing of them can reach the notice.

use std::sync::Mutex;

struct State {
    /// How many reads found a store they could not open, this run.
    count: u64,
    listeners: Vec<async_channel::Sender<()>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    count: 0,
    listeners: Vec::new(),
});

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Records one read that found a store it could not open. Returns `true` (and tells the
/// subscribers) only for the first one of the run, so the app notifies once per launch.
pub fn record() -> bool {
    let mut state = lock();
    state.count += 1;
    if state.count > 1 {
        return false;
    }
    state
        .listeners
        .retain(|listener| listener.try_send(()).is_ok());
    true
}

/// Whether any read of this run found a store it could not open.
pub fn seen() -> bool {
    lock().count > 0
}

/// How many reads of this run found a store they could not open (for the log).
pub fn count() -> u64 {
    lock().count
}

/// A stream that yields once, when the first such read happens after this call (read [`seen`]
/// for what happened before it).
pub fn subscribe() -> async_channel::Receiver<()> {
    let (tx, rx) = async_channel::unbounded();
    lock().listeners.push(tx);
    rx
}

/// Test hook: nothing seen, nobody listening.
#[doc(hidden)]
pub fn reset_for_test() {
    let mut state = lock();
    state.count = 0;
    state.listeners.clear();
}

/// Serializes this crate's tests that read or change the process-wide record. It is the
/// availability tests' lock: the backend tests that hold that one are the ones that can record
/// here.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    super::availability::test_guard()
}

#[cfg(test)]
#[path = "unreadable_record_tests.rs"]
mod tests;
