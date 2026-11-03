//! Thin wrapper over the Tracy profiler client (#1169).
//!
//! Every call site is one line, and with the `profiling` cargo feature OFF (the default, and the
//! only state release bundles may be built in) every macro here expands to nothing, so there is
//! no code, no dependency and no branch left behind.
//!
//! With the feature on, call [`init`] once at startup. The Tracy client is built with
//! `ondemand`, so it records nothing until a Tracy viewer connects (see `docs/profiling.md`).
//!
//! Zone names reuse the `input_order` hop names (`key_down`, `typed_characters`, `submit`,
//! `pty_write`, `pty_read`, `frame`) so a Tracy capture and a `RUST_LOG=warp::input_order=debug`
//! log describe the same keystroke in the same words.
//!
//! * [`zone!`] opens a timed span that ends when the enclosing block does. It holds a `!Send`
//!   guard: do not use it in an `async` block that is held across an `.await`; use [`mark!`].
//! * [`mark!`] records an instant message, safe anywhere.
//! * [`present_frame`] runs a present call and then marks the end of the frame, exactly once.
//!
//! Names must be string literals.

#[cfg(feature = "profiling")]
pub use tracy_client;

/// Starts the Tracy client. A no-op without the `profiling` feature. Idempotent.
pub fn init() {
    #[cfg(feature = "profiling")]
    {
        // The client lives for the rest of the process; there is no shutdown to pair with.
        std::mem::forget(tracy_client::Client::start());
    }
}

/// Opens a Tracy zone named `$name` lasting to the end of the enclosing block.
#[cfg(feature = "profiling")]
#[macro_export]
macro_rules! zone {
    ($name:literal) => {
        let _phosphor_zone = $crate::profiling::tracy_client::span!($name);
    };
}

/// Opens a Tracy zone named `$name` lasting to the end of the enclosing block.
#[cfg(not(feature = "profiling"))]
#[macro_export]
macro_rules! zone {
    ($name:literal) => {};
}

/// Records an instant Tracy message named `$name`.
#[cfg(feature = "profiling")]
#[macro_export]
macro_rules! mark {
    ($name:literal) => {
        if let Some(client) = $crate::profiling::tracy_client::Client::running() {
            client.message($name, 0);
        }
    };
}

/// Records an instant Tracy message named `$name`.
#[cfg(not(feature = "profiling"))]
#[macro_export]
macro_rules! mark {
    ($name:literal) => {};
}

pub use crate::{mark, zone};

/// Marks the end of a presented frame to Tracy. A no-op without the `profiling` feature.
#[inline]
pub fn frame_mark() {
    #[cfg(feature = "profiling")]
    tracy_client::frame_mark();
    #[cfg(test)]
    tests::FRAME_MARKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Runs `present` (the call that hands the finished frame to the display) and then marks the
/// frame boundary, once per call, whatever `present` returns.
#[inline]
pub fn present_frame<R>(present: impl FnOnce() -> R) -> R {
    let result = present();
    frame_mark();
    result
}

#[cfg(test)]
#[path = "profiling_tests.rs"]
mod tests;
