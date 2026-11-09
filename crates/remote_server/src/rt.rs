//! The few runtime pieces the always-compiled part of this crate needs, without naming warpui.
//!
//! With the `client` feature (the app) these ARE warpui's: same types, same timer, so nothing
//! in the client changes behaviour. Without it (the bundled `phosphor-remote` helper, built with
//! `--no-default-features --features daemon`) they are small stand-ins over `async-io` and
//! `futures`, so the helper's dependency tree contains no warpui at all (Wave H, #1804).

/// Records an instant profiling mark. A no-op in the helper; warpui's Tracy mark in the app.
#[cfg(feature = "client")]
macro_rules! mark {
    ($name:literal) => {
        warpui::profiling::mark!($name)
    };
}

/// Records an instant profiling mark. A no-op in the helper; warpui's Tracy mark in the app.
#[cfg(not(feature = "client"))]
macro_rules! mark {
    ($name:literal) => {};
}

pub(crate) use mark;

#[cfg(feature = "client")]
pub use warpui::r#async::{BoxFuture, FutureExt, TimeoutError};

#[cfg(not(feature = "client"))]
pub use standalone::{BoxFuture, FutureExt, TimeoutError};

#[cfg(not(feature = "client"))]
mod standalone {
    use std::future::Future;
    use std::time::Duration;

    use futures::{FutureExt as _, pin_mut};

    /// A boxed, sendable future (same alias warpui re-exports).
    pub type BoxFuture<'a, T> = futures::future::BoxFuture<'a, T>;

    /// A future did not finish within its time limit.
    #[derive(Debug)]
    pub struct TimeoutError;

    pub trait FutureExt: Future {
        /// Converts a future into one that gives up with [`TimeoutError`] after `timeout`.
        ///
        /// The timeout can only fire while the future is at an await point, exactly like
        /// warpui's version of this trait.
        fn with_timeout(
            self,
            timeout: Duration,
        ) -> impl Future<Output = Result<<Self as Future>::Output, TimeoutError>>;
    }

    impl<F: Future> FutureExt for F {
        async fn with_timeout(
            self,
            timeout: Duration,
        ) -> Result<<Self as Future>::Output, TimeoutError> {
            let fut = self.fuse();
            pin_mut!(fut);
            let mut timer = async_io::Timer::after(timeout).fuse();
            futures::select! {
                value = fut => Ok(value),
                _ = timer => Err(TimeoutError),
            }
        }
    }
}

/// The stand-ins are only compiled without `client`, so these run under
/// `cargo test -p remote_server --no-default-features --features daemon --lib rt::`.
#[cfg(all(test, not(feature = "client")))]
mod tests {
    use std::time::Duration;

    use super::FutureExt as _;

    #[test]
    fn with_timeout_passes_through_a_finished_future() {
        let out = async_io::block_on(async { 7 }.with_timeout(Duration::from_secs(5)));
        assert_eq!(out.unwrap(), 7);
    }

    #[test]
    fn with_timeout_gives_up_on_a_future_that_never_finishes() {
        let out = async_io::block_on(
            std::future::pending::<()>().with_timeout(Duration::from_millis(30)),
        );
        assert!(out.is_err());
    }
}
