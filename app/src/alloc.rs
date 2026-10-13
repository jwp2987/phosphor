//! Configuration for the global allocator.

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "dhat_heap_profiling")] {
        #[global_allocator]
        static GLOBAL: dhat::Alloc = dhat::Alloc;
    } else if #[cfg(feature = "jemalloc")] {
        #[global_allocator]
        static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
    }
}

/// Tunes glibc malloc for this app's allocation pattern. Must run before the process spawns its
/// first thread: `M_ARENA_MAX` only bounds arenas created after it is set.
///
/// **Why (#816).** Every new block transiently allocates ~365 KiB of grid rows (four
/// `GridHandler`s, each a full `rows x cols` cell matrix) and frees most of it when the block
/// finishes. The live heap grows only ~22 KiB per block (dhat: ~13 KiB of retained rows plus the
/// ~9 KiB `Block` in the block list), but under glibc defaults RSS grew ~113 KiB per block: the
/// churn lands in per-thread arenas (up to 8 x cores of them) and freed memory is kept rather than
/// returned. Measured over 1000 trivial blocks on the Xvfb harness (RSS in MiB at start / 250 /
/// 1000 blocks, one run each):
///
/// | allocator | start | 250 | 1000 | per block |
/// |---|---|---|---|---|
/// | glibc defaults (upstream) | 524 | 605 | 688 | ~113 KiB |
/// | jemalloc (`LD_PRELOAD`) | 452 | 468 | 492 | ~33 KiB |
/// | glibc, `MALLOC_ARENA_MAX=2` + `MALLOC_TRIM_THRESHOLD_=131072` | 398 | 443 | 461 | ~25 KiB |
///
/// Typing latency was identical across all three. This function applies the third row
/// programmatically so every glibc build gets it without environment variables; the two knobs are
/// separate calls so either can be dropped if single-knob measurements show it is not needed.
///
/// A knob the user already set — through its legacy variable (`MALLOC_ARENA_MAX`,
/// `MALLOC_TRIM_THRESHOLD_`) or through `GLIBC_TUNABLES` — is left alone: glibc applies those at
/// init, and a later `mallopt` would silently overwrite the user's choice.
///
/// Upstream Warp runs with glibc defaults; this is a deliberate improvement recorded in
/// `DECLINED.md` (IMPROVED).
#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
pub(crate) fn tune_glibc_malloc() {
    let tunables = std::env::var("GLIBC_TUNABLES").ok();
    let tunables = tunables.as_deref();

    // Caps the number of malloc arenas. With glibc's default (8 per core) each thread that
    // allocates grid rows can get its own arena, and memory freed into one arena is not reusable
    // by threads bound to another, so transient per-block churn ratchets RSS up across arenas.
    if !knob_set_by_user(
        std::env::var_os("MALLOC_ARENA_MAX").is_some(),
        tunables,
        "glibc.malloc.arena_max",
    ) {
        // SAFETY: `mallopt` only adjusts allocator parameters. It is called before any thread is
        // spawned, so no allocation can race with the change.
        let _ = unsafe { libc::mallopt(libc::M_ARENA_MAX, 2) };
    }

    // Pins the trim threshold at glibc's initial 128 KiB. By default glibc raises it (to twice
    // the mmap threshold, up to 64 MiB) each time a large mmapped chunk is freed, and also raises
    // the mmap threshold so later large allocations come from the heap instead — so after the
    // first few blocks, freed grid rows stay resident. Setting the knob explicitly, here or via
    // the environment variable that was measured, disables that dynamic adjustment for both.
    if !knob_set_by_user(
        std::env::var_os("MALLOC_TRIM_THRESHOLD_").is_some(),
        tunables,
        "glibc.malloc.trim_threshold",
    ) {
        // SAFETY: as above — a parameter change made before any thread exists.
        let _ = unsafe { libc::mallopt(libc::M_TRIM_THRESHOLD, 128 * 1024) };
    }
}

/// Whether the user configured a malloc knob, either through its legacy environment variable or
/// as `tunable` in the colon-separated `name=value` list of `GLIBC_TUNABLES`.
#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
fn knob_set_by_user(legacy_var_set: bool, glibc_tunables: Option<&str>, tunable: &str) -> bool {
    legacy_var_set
        || glibc_tunables.is_some_and(|tunables| {
            tunables.split(':').any(|entry| {
                entry
                    .split_once('=')
                    .is_some_and(|(name, _)| name == tunable)
            })
        })
}

#[cfg(all(
    test,
    target_os = "linux",
    target_env = "gnu",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
#[path = "alloc_tests.rs"]
mod tests;
