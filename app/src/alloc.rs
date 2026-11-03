//! Configuration for the global allocator.

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "dhat_heap_profiling")] {
        #[global_allocator]
        static GLOBAL: dhat::Alloc = dhat::Alloc;
    } else if #[cfg(feature = "jemalloc")] {
        #[global_allocator]
        static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
    } else if #[cfg(all(target_env = "musl", feature = "mimalloc"))] {
        #[global_allocator]
        static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
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

/// Hands freed heap pages back to the OS (`malloc_trim`) after a large release, e.g. closing an
/// editor that held hundreds of MB (#1407). glibc only returns memory from the top of a heap on
/// `free`, so a big transient structure leaves RSS high even though it is free; this walks every
/// arena and releases the free pages in the middle too. Runs on a worker (it takes each arena's
/// lock while it walks); callers use it only after a large release, never per allocation.
#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
pub(crate) fn trim_free_memory() {
    if !heap_trim_enabled() {
        return;
    }
    // SAFETY: `malloc_trim` is thread-safe and only releases memory glibc already considers free.
    let _ = unsafe { libc::malloc_trim(0) };
}

/// macOS (#1607): the system allocator keeps freed tiny/small-region pages resident until memory
/// pressure, so a file-sized transient (opening a 7 MB file) stays in the footprint after it is
/// freed. `malloc_zone_pressure_relief(NULL, 0)` asks every zone to hand back all the free pages it
/// can, the call the OS itself makes under memory pressure. Same callers and rules as the glibc
/// arm: a worker thread, only after a large release.
#[cfg(all(
    target_os = "macos",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
pub(crate) fn trim_free_memory() {
    unsafe extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
    }
    if !heap_trim_enabled() {
        return;
    }
    // SAFETY: a null zone means "all zones"; the call only returns pages the allocator already
    // considers free and is thread-safe (libmalloc takes each zone's lock).
    let released = unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
    log::debug!(target: "editor_memory", "malloc_zone_pressure_relief released {released} bytes");
}

/// No-op elsewhere: jemalloc and the remaining platforms' allocators return memory on their own.
#[cfg(not(any(
    all(
        target_os = "linux",
        target_env = "gnu",
        not(feature = "jemalloc"),
        not(feature = "dhat_heap_profiling")
    ),
    all(
        target_os = "macos",
        not(feature = "jemalloc"),
        not(feature = "dhat_heap_profiling")
    )
)))]
pub(crate) fn trim_free_memory() {}

/// [`trim_free_memory`] plus one `input_order hop=heap_trim why=<why> ms=<ms> t=<end>` line (only while
/// input-order logging is on), so the appci editor_memory trim A/B can judge the UI items that ran
/// DURING the trim (#1607). `why` is `open` (after a file-sized layout) or `close`.
pub(crate) fn trim_free_memory_timed(why: &'static str) {
    let start = std::time::Instant::now();
    trim_free_memory();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    warpui::input_order::log_hop("heap_trim", || format!("why={why} ms={ms:.2}"));
}

/// Kill switch for [`trim_free_memory`]: `PHOSPHOR_HEAP_TRIM=0` turns every heap trim off (close
/// and open alike). Read once.
#[allow(dead_code)]
fn heap_trim_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| env_flag_enabled(std::env::var("PHOSPHOR_HEAP_TRIM").ok().as_deref()))
}

/// Opt-in for the trim after a large file was laid out in an editor (#1607):
/// `PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1` enables it; unset keeps the open path as it was (trim on
/// close only). Off by default because a trim takes the allocator's arena / magazine locks while it
/// walks the heap, and a UI-thread allocation that waits on them is a stall nobody has measured
/// yet; the appci editor_memory case A/Bs it (`hop=ui_item` maxima after open). Read once.
pub(crate) fn trim_after_large_open_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        env_flag_opted_in(
            std::env::var("PHOSPHOR_EDITOR_TRIM_AFTER_OPEN")
                .ok()
                .as_deref(),
        )
    })
}

/// A default-on switch: unset or anything but `0` / `false` / `off` means enabled.
fn env_flag_enabled(value: Option<&str>) -> bool {
    !matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("0") | Some("false") | Some("off")
    )
}

/// A default-off switch: only `1` / `true` / `on` enable it.
fn env_flag_opted_in(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("on")
    )
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

#[cfg(test)]
mod switch_tests {
    use super::{env_flag_enabled, env_flag_opted_in};

    #[test]
    fn trim_switches_default_on_and_turn_off_only_on_an_explicit_off() {
        assert!(env_flag_enabled(None));
        assert!(env_flag_enabled(Some("1")));
        assert!(env_flag_enabled(Some("")));
        assert!(!env_flag_enabled(Some("0")));
        assert!(!env_flag_enabled(Some(" off ")));
        assert!(!env_flag_enabled(Some("FALSE")));
    }

    #[test]
    fn trim_after_open_is_opt_in() {
        assert!(!env_flag_opted_in(None));
        assert!(!env_flag_opted_in(Some("")));
        assert!(!env_flag_opted_in(Some("0")));
        assert!(!env_flag_opted_in(Some("yes please")));
        assert!(env_flag_opted_in(Some("1")));
        assert!(env_flag_opted_in(Some(" ON ")));
        assert!(env_flag_opted_in(Some("true")));
    }
}
