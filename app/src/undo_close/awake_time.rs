//! A clock that only counts time the machine was awake, for the undo-close grace period.
//!
//! A closed tab should stay restorable for its grace period of *use*. A laptop that sleeps
//! between the close and the undo must not find the item already expired on wake. Linux
//! (`CLOCK_MONOTONIC`) and macOS (`mach_absolute_time`) monotonic clocks already stop during
//! suspend. Windows' `QueryPerformanceCounter`, which `std::time::Instant` uses there, keeps
//! counting, so Windows reads the interrupt-time clock that excludes sleep instead.
//!
//! There is no portable suspend/resume event to subscribe to, so the expiry timer asks this
//! clock when it fires and re-arms for whatever grace is left ([`remaining_grace`]) rather than
//! trusting that the whole timer interval was spent awake.

use std::time::Duration;

/// Time the machine has been awake, from an arbitrary origin. Only differences are meaningful.
pub(super) fn awake_now() -> Duration {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            // Interrupt time in 100 ns units, not counting time spent in sleep/hibernate.
            fn QueryUnbiasedInterruptTime(unbiased_time: *mut u64) -> i32;
        }
        let mut ticks: u64 = 0;
        // SAFETY: `ticks` is a valid out-pointer for the duration of the call.
        if unsafe { QueryUnbiasedInterruptTime(&mut ticks) } != 0 {
            return Duration::from_nanos(ticks.saturating_mul(100));
        }
    }
    monotonic_since_process_start()
}

fn monotonic_since_process_start() -> Duration {
    use std::sync::OnceLock;
    static ORIGIN: OnceLock<instant::Instant> = OnceLock::new();
    ORIGIN.get_or_init(instant::Instant::now).elapsed()
}

/// How much of `grace` is left after `awake_elapsed` of awake time, or `None` when it has all been
/// spent and the item should expire.
pub(super) fn remaining_grace(grace: Duration, awake_elapsed: Duration) -> Option<Duration> {
    grace
        .checked_sub(awake_elapsed)
        .filter(|left| !left.is_zero())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_secs(60);

    #[test]
    fn grace_is_spent_only_by_awake_time() {
        // A timer that fired after the machine slept most of the interval: the awake clock only
        // advanced 5 s, so 55 s remain and the item must not expire yet.
        assert_eq!(
            remaining_grace(GRACE, Duration::from_secs(5)),
            Some(Duration::from_secs(55))
        );
    }

    #[test]
    fn expires_once_the_awake_time_covers_the_grace() {
        assert_eq!(remaining_grace(GRACE, GRACE), None);
        assert_eq!(remaining_grace(GRACE, GRACE + Duration::from_secs(1)), None);
        assert_eq!(remaining_grace(GRACE, Duration::ZERO), Some(GRACE));
    }

    #[test]
    fn awake_clock_does_not_run_backwards() {
        let a = awake_now();
        let b = awake_now();
        assert!(b >= a);
    }
}
