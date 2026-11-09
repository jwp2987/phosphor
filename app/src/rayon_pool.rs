//! Caps rayon's global thread pool.
//!
//! rayon's global pool is lazy and defaults to one thread per logical CPU. Every `par_iter` in
//! the app (editor snapshots, file hydration, edits) would otherwise grow it to the machine's
//! full core count, each worker holding a stack and showing up as an idle thread. The cap is
//! applied once at startup, before any `par_iter` can initialise the pool.
//!
//! `PHOSPHOR_RAYON_THREADS=<n>` sets the cap; `0` leaves rayon's own default in place (the kill
//! switch). Unset or unparsable uses [`DEFAULT_RAYON_THREADS`].

/// Built-in cap, pending box measurements.
const DEFAULT_RAYON_THREADS: usize = 8;

/// Environment variable naming the cap (`0` = rayon default).
const RAYON_THREADS_ENV: &str = "PHOSPHOR_RAYON_THREADS";

/// The thread count to ask rayon for, or `None` to leave its default alone.
fn rayon_thread_cap(env: Option<&str>, ncpus: usize) -> Option<usize> {
    let requested = match env.map(str::trim) {
        Some(value) if !value.is_empty() => value.parse::<usize>().unwrap_or(DEFAULT_RAYON_THREADS),
        _ => DEFAULT_RAYON_THREADS,
    };
    if requested == 0 {
        return None;
    }
    Some(requested.min(ncpus.max(1)))
}

/// Applies the cap to rayon's global pool. Safe to call more than once: only the first call that
/// reaches an uninitialised pool has any effect.
pub(crate) fn configure_global_pool() {
    let ncpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let Some(threads) = rayon_thread_cap(std::env::var(RAYON_THREADS_ENV).ok().as_deref(), ncpus)
    else {
        log::info!("{RAYON_THREADS_ENV}=0: leaving rayon's default thread count");
        return;
    };
    match rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("warp-rayon-{index}"))
        .build_global()
    {
        Ok(()) => log::debug!("rayon global pool capped at {threads} threads"),
        // Already initialised (a test, or a dependency got there first): nothing to do.
        Err(error) => log::debug!("rayon global pool left as is: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_capped_by_cpu_count() {
        assert_eq!(rayon_thread_cap(None, 32), Some(DEFAULT_RAYON_THREADS));
        assert_eq!(rayon_thread_cap(None, 4), Some(4));
        assert_eq!(rayon_thread_cap(None, 0), Some(1));
    }

    #[test]
    fn explicit_value_is_used_and_clamped() {
        assert_eq!(rayon_thread_cap(Some("3"), 16), Some(3));
        assert_eq!(rayon_thread_cap(Some(" 12 "), 16), Some(12));
        assert_eq!(rayon_thread_cap(Some("64"), 16), Some(16));
    }

    #[test]
    fn zero_is_the_kill_switch() {
        assert_eq!(rayon_thread_cap(Some("0"), 16), None);
    }

    #[test]
    fn junk_and_empty_fall_back_to_the_default() {
        assert_eq!(
            rayon_thread_cap(Some("lots"), 32),
            Some(DEFAULT_RAYON_THREADS)
        );
        assert_eq!(rayon_thread_cap(Some(""), 32), Some(DEFAULT_RAYON_THREADS));
    }

    #[test]
    fn configuring_twice_does_not_panic() {
        configure_global_pool();
        configure_global_pool();
        assert!(rayon::current_num_threads() >= 1);
    }
}
