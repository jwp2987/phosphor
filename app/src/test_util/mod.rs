pub mod ai_agent_tasks;
pub mod blockgrid;
pub mod settings;
pub mod terminal;
mod virtual_fs;

pub use blockgrid::mock_blockgrid;
pub use terminal::add_window_with_terminal;
pub use virtual_fs::{Stub, VirtualFS};

/// Wall-clock give-up for [`wait_for`].
pub(crate) const WAIT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// Pumps the foreground executor until `done` holds or [`WAIT_DEADLINE`] passes, and returns
/// `done`'s final value. Tests wait for a condition with this, never with a fixed count of
/// `yield_now` turns (#2212, #2219): a count measures how many turns the executor got, which on a
/// loaded runner is not enough for the hops that resolve the state. It returns the moment `done`
/// holds, so the fast case is unchanged; a state that never arrives still fails, after the
/// deadline, on the caller's own assertion.
pub(crate) async fn wait_for(mut done: impl FnMut() -> bool) -> bool {
    let started = std::time::Instant::now();
    loop {
        if done() {
            return true;
        }
        if started.elapsed() >= WAIT_DEADLINE {
            return done();
        }
        futures_lite::future::yield_now().await;
        // Let other threads (background executor, timers) make progress between turns.
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
}

macro_rules! assert_eventually {
    ($cond:expr_2021, $($arg:tt)+) => {
        $crate::test_util::assert_eventually!(20 => $cond, $($arg)+);
    };
    // Run the condition up to ticks times, yielding to the executor in between.  If it does
    // not become true, this panics with the provided format string + args.
    ($ticks:literal => $cond:expr_2021, $($arg:tt)+) => {{
        let mut pass = false;
        for _ in 0..$ticks {
            if $cond {
                pass = true;
                break;
            }
            warpui::r#async::Timer::after(std::time::Duration::from_millis(5)).await;
        }
        if !pass {
            panic!("{}", format_args!($($arg)+));
        }
    }};
}
pub(crate) use assert_eventually;
