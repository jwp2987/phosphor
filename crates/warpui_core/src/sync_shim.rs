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

    pub struct RwLock<T>(inner::RwLock<T>);

    pub type RwLockReadGuard<'a, T> = inner::RwLockReadGuard<'a, T>;
    pub type RwLockWriteGuard<'a, T> = inner::RwLockWriteGuard<'a, T>;

    impl<T> RwLock<T> {
        pub fn new(value: T) -> Self {
            Self(inner::RwLock::new(value))
        }

        pub fn read(&self) -> RwLockReadGuard<'_, T> {
            self.0.read().unwrap()
        }

        /// The checkers' read locks do not queue behind a waiting writer, so a nested read is
        /// as safe as with parking_lot's `read_recursive`.
        pub fn read_recursive(&self) -> RwLockReadGuard<'_, T> {
            self.0.read().unwrap()
        }

        /// As parking_lot's `try_read_recursive`: `None` while a writer holds it.
        pub fn try_read_recursive(&self) -> Option<RwLockReadGuard<'_, T>> {
            self.0.try_read().ok()
        }

        pub fn write(&self) -> RwLockWriteGuard<'_, T> {
            self.0.write().unwrap()
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
