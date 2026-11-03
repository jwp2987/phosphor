//! Synchronization primitives that model checkers can swap (dev-only; nothing here changes a
//! normal build).
//!
//! In every normal build this module re-exports `parking_lot`'s `Mutex`, `RwLock` and `Condvar`
//! and `std`'s atomics, so code that imports them from here is byte-identical to code that
//! imported them directly. Under `RUSTFLAGS="--cfg shuttle"` or `RUSTFLAGS="--cfg loom"` the
//! same names resolve to thin wrappers over the checker's primitives, with `parking_lot`'s
//! calling convention (`lock()` returns the guard, `read_recursive`, `wait(&mut guard)`), so the
//! code under test needs no other change.
//!
//! The model builds do not model time: [`Condvar::wait_until`] waits for a notification and
//! never times out. A model test must therefore make sure the notification comes.
//!
//! Run the model tests of a crate (a model build compiles only the model tests of the modules
//! that have them; filter by `model_tests::` to skip the rest of the crate's tests, which use real
//! primitives outside a model):
//!
//! ```text
//! RUSTFLAGS="--cfg shuttle" cargo test -p warpui_core --lib --release -- model_tests::
//! RUSTFLAGS="--cfg loom"    cargo test -p warpui_core --lib --release -- model_tests::
//! ```
//!
//! [`check`] runs a test body under the active checker: shuttle explores randomly scheduled
//! interleavings, loom explores all of them (keep loom bodies tiny: two or three threads, a
//! handful of operations each).

#[cfg(not(any(shuttle, loom)))]
pub use parking_lot::{
    Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, WaitTimeoutResult,
};

#[cfg(not(any(shuttle, loom)))]
pub mod atomic {
    pub use std::sync::atomic::*;
}

#[cfg(any(shuttle, loom))]
pub use model::*;

/// `const fn` in a normal build, a plain `fn` under a model checker (whose primitives cannot be
/// built in a `const` context).
#[macro_export]
macro_rules! model_const_fn {
    ($(#[$attr:meta])* $vis:vis fn $name:ident($($arg:ident: $argty:ty),*) -> $ret:ty $body:block) => {
        #[cfg(not(any(shuttle, loom)))]
        $(#[$attr])* $vis const fn $name($($arg: $argty),*) -> $ret $body
        #[cfg(any(shuttle, loom))]
        $(#[$attr])* $vis fn $name($($arg: $argty),*) -> $ret $body
    };
}

/// A `static` in a normal build, a lazily built `static` under a model checker (whose
/// primitives cannot be built in a `const` context; a model test builds its own instance
/// instead of touching the static).
#[macro_export]
macro_rules! model_static {
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = $init:expr;) => {
        #[cfg(not(any(shuttle, loom)))]
        $(#[$attr])* $vis static $name: $ty = $init;
        #[cfg(any(shuttle, loom))]
        $(#[$attr])* $vis static $name: std::sync::LazyLock<$ty> = std::sync::LazyLock::new(|| $init);
    };
}

#[cfg(any(shuttle, loom))]
mod model {
    use std::ops::{Deref, DerefMut};

    #[cfg(loom)]
    use loom::sync as inner;
    #[cfg(shuttle)]
    use shuttle::sync as inner;

    pub use inner::Arc;
    pub use inner::atomic;
    #[cfg(loom)]
    pub use loom::thread;
    #[cfg(shuttle)]
    pub use shuttle::thread;

    /// Runs `body` under the checker: random schedules (shuttle) or every schedule (loom).
    #[cfg(shuttle)]
    pub fn check<F: Fn() + Send + Sync + 'static>(body: F) {
        shuttle::check_random(body, 2_000);
    }

    /// Runs `body` under the checker: random schedules (shuttle) or every schedule (loom).
    #[cfg(loom)]
    pub fn check<F: Fn() + Send + Sync + 'static>(body: F) {
        loom::model(body);
    }

    pub struct Mutex<T>(inner::Mutex<T>);

    impl<T> Mutex<T> {
        pub fn new(value: T) -> Self {
            Self(inner::Mutex::new(value))
        }

        pub fn lock(&self) -> MutexGuard<'_, T> {
            MutexGuard {
                guard: Some(self.0.lock().unwrap()),
            }
        }
    }

    impl<T: Default> Default for Mutex<T> {
        fn default() -> Self {
            Self::new(T::default())
        }
    }

    impl<T> std::fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Mutex { .. }")
        }
    }

    /// The guard is an `Option` so [`Condvar::wait`] can hand it to the checker's condvar and
    /// put the re-acquired guard back, behind the `&mut` parking_lot's `wait` takes.
    pub struct MutexGuard<'a, T> {
        guard: Option<inner::MutexGuard<'a, T>>,
    }

    impl<T> Deref for MutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.guard.as_ref().expect("guard is present outside wait")
        }
    }

    impl<T> DerefMut for MutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            self.guard.as_mut().expect("guard is present outside wait")
        }
    }

    #[derive(Default)]
    pub struct Condvar(inner::Condvar);

    /// Never timed out: the model does not model time.
    #[derive(Clone, Copy, Debug)]
    pub struct WaitTimeoutResult;

    impl WaitTimeoutResult {
        pub fn timed_out(&self) -> bool {
            false
        }
    }

    impl Condvar {
        pub fn new() -> Self {
            Self(inner::Condvar::new())
        }

        pub fn notify_one(&self) {
            self.0.notify_one();
        }

        pub fn notify_all(&self) {
            self.0.notify_all();
        }

        pub fn wait<T>(&self, guard: &mut MutexGuard<'_, T>) {
            let taken = guard.guard.take().expect("guard is present outside wait");
            guard.guard = Some(self.0.wait(taken).unwrap());
        }

        /// Waits for a notification; the deadline is ignored (see the module documentation).
        pub fn wait_until<T>(
            &self,
            guard: &mut MutexGuard<'_, T>,
            _deadline: std::time::Instant,
        ) -> WaitTimeoutResult {
            self.wait(guard);
            WaitTimeoutResult
        }
    }

    /// parking_lot's `RwLock`, modelled: a mutex-and-condvar lock with per-thread ownership, so
    /// it can do what the checkers' own `RwLock` cannot. Both checkers panic when a thread takes a
    /// lock it already holds, a read included, while parking_lot's `read_recursive` on a thread
    /// that already reads succeeds even with a writer waiting (that is what it is for). The
    /// checkers' lock therefore failed a nested read the normal build performs safely; this one
    /// grants it and still panics on every acquisition that deadlocks with parking_lot:
    ///
    /// - `write` by a thread that holds the lock (a read or the write);
    /// - `read` or `read_recursive` by the thread that holds the write;
    /// - a nested plain `read` (parking_lot's queues behind a waiting writer).
    ///
    /// Blocking is modelled with the checker's condvar, so a cross-thread deadlock is still a
    /// deadlock the checker reports.
    pub struct RwLock<T> {
        state: inner::Mutex<RwState>,
        changed: inner::Condvar,
        data: std::cell::UnsafeCell<T>,
    }

    // SAFETY: `data` is reached only through a guard; `state` grants one writer or any number of
    // readers, never both, exactly as for `std::sync::RwLock`.
    unsafe impl<T: Send> Send for RwLock<T> {}
    unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

    #[derive(Default)]
    struct RwState {
        writer: Option<thread::ThreadId>,
        /// Each reading thread with how many read guards it holds.
        readers: Vec<(thread::ThreadId, usize)>,
    }

    impl RwState {
        fn reads_held_by(&self, me: thread::ThreadId) -> usize {
            self.readers
                .iter()
                .find(|(id, _)| *id == me)
                .map_or(0, |(_, count)| *count)
        }

        fn add_reader(&mut self, me: thread::ThreadId) {
            match self.readers.iter_mut().find(|(id, _)| *id == me) {
                Some((_, count)) => *count += 1,
                None => self.readers.push((me, 1)),
            }
        }

        fn remove_reader(&mut self, me: thread::ThreadId) {
            if let Some(index) = self.readers.iter().position(|(id, _)| *id == me) {
                self.readers[index].1 -= 1;
                if self.readers[index].1 == 0 {
                    self.readers.swap_remove(index);
                }
            }
        }
    }

    pub struct RwLockReadGuard<'a, T> {
        lock: &'a RwLock<T>,
        reader: thread::ThreadId,
    }

    pub struct RwLockWriteGuard<'a, T> {
        lock: &'a RwLock<T>,
    }

    impl<T> RwLock<T> {
        pub fn new(value: T) -> Self {
            Self {
                state: inner::Mutex::new(RwState::default()),
                changed: inner::Condvar::new(),
                data: std::cell::UnsafeCell::new(value),
            }
        }

        fn state(&self) -> inner::MutexGuard<'_, RwState> {
            // A panic is the test's failure; a guard dropped while it unwinds must still release.
            self.state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        fn acquire_read(&self, recursive: bool) -> RwLockReadGuard<'_, T> {
            let me = thread::current().id();
            let mut state = self.state();
            if state.writer == Some(me) {
                drop(state);
                panic!("deadlock! a thread read an RwLock while holding its write lock");
            }
            if !recursive && state.reads_held_by(me) > 0 {
                drop(state);
                panic!(
                    "deadlock! a nested non-recursive read of an RwLock (parking_lot's `read` \
                     queues behind a waiting writer); use `read_recursive`"
                );
            }
            // A thread that already reads is not made to wait for a writer: that writer waits for
            // this thread's guards, so waiting would be the deadlock `read_recursive` avoids.
            while state.writer.is_some() {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            state.add_reader(me);
            RwLockReadGuard {
                lock: self,
                reader: me,
            }
        }

        pub fn read(&self) -> RwLockReadGuard<'_, T> {
            self.acquire_read(false)
        }

        /// As parking_lot's `read_recursive`: a thread that already reads gets another guard.
        pub fn read_recursive(&self) -> RwLockReadGuard<'_, T> {
            self.acquire_read(true)
        }

        /// As parking_lot's `try_read_recursive`: `None` while a writer holds it.
        pub fn try_read_recursive(&self) -> Option<RwLockReadGuard<'_, T>> {
            let me = thread::current().id();
            let mut state = self.state();
            if state.writer.is_some() {
                return None;
            }
            state.add_reader(me);
            Some(RwLockReadGuard {
                lock: self,
                reader: me,
            })
        }

        pub fn write(&self) -> RwLockWriteGuard<'_, T> {
            let me = thread::current().id();
            let mut state = self.state();
            if state.writer == Some(me) || state.reads_held_by(me) > 0 {
                drop(state);
                panic!("deadlock! a thread took an RwLock's write lock while holding that lock");
            }
            while state.writer.is_some() || !state.readers.is_empty() {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            state.writer = Some(me);
            RwLockWriteGuard { lock: self }
        }
    }

    impl<T> Deref for RwLockReadGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            // SAFETY: this guard is a registered reader, so no writer exists until it drops.
            unsafe { &*self.lock.data.get() }
        }
    }

    impl<T> Drop for RwLockReadGuard<'_, T> {
        fn drop(&mut self) {
            let mut state = self.lock.state();
            state.remove_reader(self.reader);
            drop(state);
            self.lock.changed.notify_all();
        }
    }

    impl<T> Deref for RwLockWriteGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            // SAFETY: this guard is the only writer and no reader exists until it drops.
            unsafe { &*self.lock.data.get() }
        }
    }

    impl<T> DerefMut for RwLockWriteGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            // SAFETY: as for `deref`; `&mut self` makes the borrow unique.
            unsafe { &mut *self.lock.data.get() }
        }
    }

    impl<T> Drop for RwLockWriteGuard<'_, T> {
        fn drop(&mut self) {
            let mut state = self.lock.state();
            state.writer = None;
            drop(state);
            self.lock.changed.notify_all();
        }
    }

    impl<T: Default> Default for RwLock<T> {
        fn default() -> Self {
            Self::new(T::default())
        }
    }

    impl<T> std::fmt::Debug for RwLock<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("RwLock { .. }")
        }
    }
}

// Interleaving tests of the modelled `RwLock` itself.
#[cfg(all(test, any(shuttle, loom)))]
#[path = "sync_shim_model_tests.rs"]
mod model_tests;
