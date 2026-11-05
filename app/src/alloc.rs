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
/// Returns `None`: glibc does not say how much it released.
#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
pub(crate) fn trim_free_memory() -> Option<u64> {
    if !heap_trim_enabled() {
        return None;
    }
    // SAFETY: `malloc_trim` is thread-safe and only releases memory glibc already considers free.
    let _ = unsafe { libc::malloc_trim(0) };
    None
}

/// macOS (#1607): the system allocator keeps freed tiny/small-region pages resident until memory
/// pressure, so a file-sized transient (opening a 7 MB file) stays in the footprint after it is
/// freed. `malloc_zone_pressure_relief(NULL, 0)` asks every zone to hand back all the free pages it
/// can, the call the OS itself makes under memory pressure. Same callers and rules as the glibc
/// arm: a worker thread, only after a large release. Returns the bytes libmalloc says it released.
///
/// What it cannot do (#1607, appci r18u23 Mac): libmalloc already marks most freed pages
/// `MADV_FREE_REUSABLE` when they are freed, which takes them out of `phys_footprint` at once but
/// leaves them in `resident_size` (`ps` RSS) until the kernel reclaims them under pressure. This
/// call releases nothing more for those pages (r18u23: `freed_estimate=0` on every close while
/// RSS stayed ~170 MB over the baseline and `phys_footprint` fell below it); the
/// `resident` / `reusable` fields of the `heap_trim` line measure that share.
#[cfg(all(
    target_os = "macos",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
pub(crate) fn trim_free_memory() -> Option<u64> {
    unsafe extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
    }
    if !heap_trim_enabled() {
        return None;
    }
    // SAFETY: a null zone means "all zones"; the call only returns pages the allocator already
    // considers free and is thread-safe (libmalloc takes each zone's lock).
    let released = unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
    log::debug!(target: "editor_memory", "malloc_zone_pressure_relief released {released} bytes");
    Some(released as u64)
}

/// Windows (#1607): Rust's system allocator there is `HeapAlloc` on the process heap
/// (`GetProcessHeap`), and the UCRT's `malloc` uses the same heap, so `_heapmin` would only repeat
/// this call. The NT heap keeps freed segments committed; `HeapCompact` coalesces the free blocks
/// and decommits the large free ones, which is what lets the working set and private bytes fall
/// after a 7 MB file's editor is dropped. Only the process heap is compacted: another heap in the
/// process may have been created with `HEAP_NO_SERIALIZE`, and compacting it from this worker
/// would race its owner. Same callers and rules as the other arms: a worker, after a large release.
/// Returns `None`: `HeapCompact` reports the largest free block, not what it decommitted.
#[cfg(all(
    target_os = "windows",
    not(feature = "jemalloc"),
    not(feature = "dhat_heap_profiling")
))]
pub(crate) fn trim_free_memory() -> Option<u64> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcessHeap() -> *mut std::ffi::c_void;
        fn HeapCompact(heap: *mut std::ffi::c_void, flags: u32) -> usize;
    }
    if !heap_trim_enabled() {
        return None;
    }
    // SAFETY: `GetProcessHeap` returns the process heap's handle (never closed); the process heap
    // is serialized, so `HeapCompact` takes its lock and only touches blocks that are free.
    let heap = unsafe { GetProcessHeap() };
    if heap.is_null() {
        return None;
    }
    // SAFETY: as above; flags 0 = serialized access.
    let largest_free = unsafe { HeapCompact(heap, 0) };
    log::debug!(target: "editor_memory", "HeapCompact largest free block {largest_free} bytes");
    None
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
    ),
    all(
        target_os = "windows",
        not(feature = "jemalloc"),
        not(feature = "dhat_heap_profiling")
    )
)))]
pub(crate) fn trim_free_memory() -> Option<u64> {
    None
}

/// [`trim_free_memory`] plus one `input_order hop=heap_trim why=<why> ms=<ms> freed_estimate=<bytes>
/// t=<end>` line (only while input-order logging is on), so the appci editor_memory trim A/B can judge
/// the UI items that ran DURING the trim, and one `editor_memory heap_trim why=<why> ms=<ms>
/// freed_estimate=<bytes>` line (target `editor_memory`, debug) that the case reads next to
/// `buffer_dropped` (#1607). `why` is `open` (after a file-sized layout) or `close`.
/// `freed_estimate` is the process footprint (macOS phys_footprint, Linux RSS + swap, Windows
/// private bytes) before minus after the trim: other threads allocate meanwhile, so it is an
/// estimate and can be negative. Nothing is logged when `PHOSPHOR_HEAP_TRIM=0` turned trims off.
/// The `editor_memory` line also carries what the allocator says it released (`released=<bytes>`,
/// macOS) and, on macOS, `resident=<bytes> reusable=<bytes>` after the trim: the share of RSS that
/// is freed-but-reusable malloc memory (#1607), which only the kernel takes back.
pub(crate) fn trim_free_memory_timed(why: &'static str) {
    if !heap_trim_enabled() {
        return;
    }
    let before = footprint_bytes();
    let start = std::time::Instant::now();
    let released = trim_free_memory();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    let freed_estimate = before as i128 - footprint_bytes() as i128;
    warpui::input_order::log_hop("heap_trim", || {
        format!("why={why} ms={ms:.2} freed_estimate={freed_estimate}")
    });
    log::debug!(
        target: "editor_memory",
        "editor_memory heap_trim why={why} ms={ms:.2} freed_estimate={freed_estimate}{}",
        trim_detail(released)
    );
}

/// The extra `key=value` fields of the `editor_memory heap_trim` line: ` released=<bytes>` when
/// the allocator reports it, and on macOS ` resident=<bytes> reusable=<bytes>` (#1607).
fn trim_detail(released: Option<u64>) -> String {
    let mut out = String::new();
    if let Some(released) = released {
        out.push_str(&format!(" released={released}"));
    }
    #[cfg(target_os = "macos")]
    {
        if let Some((resident, reusable)) =
            crate::system::memory_footprint::resident_and_reusable_bytes()
        {
            out.push_str(&format!(" resident={resident} reusable={reusable}"));
        }
    }
    out
}

/// The process footprint in bytes (0 where it cannot be read), for `freed_estimate`.
fn footprint_bytes() -> u64 {
    #[cfg(not(target_family = "wasm"))]
    {
        crate::system::memory_footprint::memory_footprint_bytes()
    }
    #[cfg(target_family = "wasm")]
    {
        0
    }
}

/// Folds the close trims of a burst of closes into at most one running pass plus one follow-up
/// pass (#1607). A pass that is already running may have walked the heap before the latest drop
/// freed its memory, so a request that arrives while one runs schedules exactly one more pass
/// after it; any further requests in that window ride on that same follow-up. Event-driven: no
/// timer, the drop event is the trigger.
pub(crate) struct TrimCoalescer(std::sync::atomic::AtomicU8);

const TRIM_IDLE: u8 = 0;
const TRIM_RUNNING: u8 = 1;
const TRIM_RERUN: u8 = 2;

impl TrimCoalescer {
    pub(crate) const fn new() -> Self {
        Self(std::sync::atomic::AtomicU8::new(TRIM_IDLE))
    }

    /// Registers a request. `true`: nothing was running, so the caller must start a worker that
    /// runs passes until [`Self::finish_pass`] returns `false`. `false`: folded into the running
    /// worker's follow-up pass.
    pub(crate) fn request(&self) -> bool {
        use std::sync::atomic::Ordering;
        let mut current = self.0.load(Ordering::Acquire);
        loop {
            let next = if current == TRIM_IDLE {
                TRIM_RUNNING
            } else {
                TRIM_RERUN
            };
            match self
                .0
                .compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return current == TRIM_IDLE,
                Err(actual) => current = actual,
            }
        }
    }

    /// Called by the worker after each pass: `true` when a request arrived during the pass and
    /// one more pass must run, `false` when the worker is done (the state is idle again).
    pub(crate) fn finish_pass(&self) -> bool {
        use std::sync::atomic::Ordering;
        let mut current = self.0.load(Ordering::Acquire);
        loop {
            let (next, again) = if current == TRIM_RERUN {
                (TRIM_RUNNING, true)
            } else {
                (TRIM_IDLE, false)
            };
            match self
                .0
                .compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return again,
                Err(actual) => current = actual,
            }
        }
    }
}

static CLOSE_TRIMS: TrimCoalescer = TrimCoalescer::new();

/// Asks for one heap trim after a large editor buffer was really dropped (#1407, #1607). Returns
/// `true` when the caller must run [`run_close_trims`] on a background worker (never the UI
/// thread); `false` when the close trim is switched off or a running trim will make one more pass
/// for it. Kill switches: `PHOSPHOR_HEAP_TRIM_ON_CLOSE=0` (this trim only) or `PHOSPHOR_HEAP_TRIM=0`
/// (every trim).
pub(crate) fn request_close_trim() -> bool {
    if !close_trim_enabled() {
        log::debug!(target: "editor_memory", "editor_memory heap_trim why=close skipped=kill_switch");
        return false;
    }
    CLOSE_TRIMS.request()
}

/// The worker body for [`request_close_trim`]: trims until no request is pending.
pub(crate) fn run_close_trims() {
    loop {
        trim_free_memory_timed("close");
        if !CLOSE_TRIMS.finish_pass() {
            break;
        }
    }
}

/// Kill switch for the close trim alone (#1607): `PHOSPHOR_HEAP_TRIM_ON_CLOSE=0` turns it off,
/// default on. `PHOSPHOR_HEAP_TRIM=0` also turns it off. Read once.
pub(crate) fn close_trim_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        heap_trim_enabled()
            && env_flag_enabled(std::env::var("PHOSPHOR_HEAP_TRIM_ON_CLOSE").ok().as_deref())
    })
}

/// Kill switch for [`trim_free_memory`]: `PHOSPHOR_HEAP_TRIM=0` turns every heap trim off (close
/// and open alike). Read once.
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
    use super::{TrimCoalescer, env_flag_enabled, env_flag_opted_in, trim_detail};

    #[test]
    fn close_trim_starts_one_worker_and_folds_a_burst_into_one_more_pass() {
        let gate = TrimCoalescer::new();
        // The first drop starts the worker.
        assert!(gate.request());
        // Three more drops while it runs: none starts a second worker.
        assert!(!gate.request());
        assert!(!gate.request());
        assert!(!gate.request());
        // They are folded into exactly one follow-up pass, after which the worker stops.
        assert!(gate.finish_pass());
        assert!(!gate.finish_pass());
        // Idle again: the next close starts a fresh worker.
        assert!(gate.request());
        assert!(!gate.finish_pass());
    }

    #[test]
    fn close_trim_without_a_request_during_the_pass_runs_once() {
        let gate = TrimCoalescer::new();
        assert!(gate.request());
        assert!(!gate.finish_pass());
    }

    #[test]
    fn a_request_during_the_follow_up_pass_gets_its_own_pass() {
        let gate = TrimCoalescer::new();
        assert!(gate.request());
        assert!(!gate.request());
        assert!(gate.finish_pass()); // follow-up pass starts
        assert!(!gate.request()); // a drop during the follow-up
        assert!(gate.finish_pass()); // is not lost
        assert!(!gate.finish_pass());
    }

    #[test]
    fn trim_switches_default_on_and_turn_off_only_on_an_explicit_off() {
        assert!(env_flag_enabled(None));
        assert!(env_flag_enabled(Some("1")));
        assert!(env_flag_enabled(Some("")));
        assert!(!env_flag_enabled(Some("0")));
        assert!(!env_flag_enabled(Some(" off ")));
        assert!(!env_flag_enabled(Some("FALSE")));
    }

    /// #1607: the `editor_memory heap_trim` line carries what the allocator released (when it says)
    /// and, on macOS, the resident and freed-but-reusable bytes the appci case puts next to RSS.
    #[test]
    fn heap_trim_line_carries_released_bytes_and_the_macos_reusable_share() {
        let with = trim_detail(Some(4096));
        assert!(with.starts_with(" released=4096"), "{with}");
        let without = trim_detail(None);
        assert!(!without.contains("released="), "{without}");
        #[cfg(target_os = "macos")]
        assert!(
            without.starts_with(" resident=") && without.contains(" reusable="),
            "{without}"
        );
        #[cfg(not(target_os = "macos"))]
        assert!(without.is_empty(), "{without}");
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
