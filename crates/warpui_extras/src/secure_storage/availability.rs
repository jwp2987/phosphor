//! Process-wide "can the OS keyring be used" state (#1805).
//!
//! Every secure-storage handle in the process (the one registered on the app, and the fresh one
//! each secure-storage worker builds) shares this state. Without it each handle discovered on
//! its own that no Secret Service answers: the UI handle and every worker generation repeated
//! the failing D-Bus round trip (up to the bus's 120 s service-activation timeout each), and each
//! logged "Failed to acquire default Secret Service collection".
//!
//! The state starts [`Availability::Unknown`]. The first failure to reach the keyring moves it to
//! [`Availability::Unavailable`] for the rest of the run; backends that consult it (the Linux
//! Secret Service backend) then go straight to their fallback without touching D-Bus. It leaves
//! `Unavailable` only by an explicit user action ([`retry`]) or when a keyring call that was
//! still outstanding turns out to have succeeded after all ([`mark_available`]: the service
//! appeared).
//!
//! Each transition is published to [`subscribe`]rs, which is how the app shows a single notice.

use std::sync::Mutex;

/// Whether the OS keyring is usable in this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Nothing has tried the keyring yet (or the user asked to retry).
    Unknown,
    /// A keyring call succeeded.
    Available,
    /// The keyring could not be reached or opened; stored credentials come from the fallback
    /// (where the platform has one) or are unavailable this run.
    Unavailable,
}

struct State {
    availability: Availability,
    listeners: Vec<async_channel::Sender<Availability>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    availability: Availability::Unknown,
    listeners: Vec::new(),
});

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Moves to `next`; returns `true` (and notifies subscribers) only if that changed the state.
fn transition(next: Availability) -> bool {
    let mut state = lock();
    if state.availability == next {
        return false;
    }
    state.availability = next;
    state
        .listeners
        .retain(|listener| listener.try_send(next).is_ok());
    true
}

/// The current state.
pub fn current() -> Availability {
    lock().availability
}

/// Whether a backend should try the keyring at all. `false` once it is known to be unavailable,
/// so a run without a keyring makes the failing call once, not once per lookup.
pub fn should_contact_keyring() -> bool {
    current() != Availability::Unavailable
}

/// Records that the keyring could not be used. Returns `true` only for the call that changed the
/// state, so the caller logs (and the app notifies) once per run, not once per lookup.
pub fn mark_unavailable() -> bool {
    transition(Availability::Unavailable)
}

/// Records that a keyring call succeeded. Returns `true` if that changed the state.
pub fn mark_available() -> bool {
    transition(Availability::Available)
}

/// An explicit user request to try the keyring again: `Unavailable` goes back to `Unknown`, so
/// the next lookup contacts the keyring once more. Returns `true` if it changed the state.
pub fn retry() -> bool {
    let mut state = lock();
    if state.availability != Availability::Unavailable {
        return false;
    }
    state.availability = Availability::Unknown;
    state
        .listeners
        .retain(|listener| listener.try_send(Availability::Unknown).is_ok());
    true
}

/// A stream of every later transition (not the current state; read [`current`] for that).
pub fn subscribe() -> async_channel::Receiver<Availability> {
    let (tx, rx) = async_channel::unbounded();
    lock().listeners.push(tx);
    rx
}

/// Test hook: back to `Unknown`, without notifying anyone.
#[doc(hidden)]
pub fn reset_for_test() {
    lock().availability = Availability::Unknown;
}

/// Serializes this crate's tests that read or change the process-wide state, so they hold under
/// a threaded `cargo test` as well as under nextest's process-per-test.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[path = "availability_tests.rs"]
mod tests;
