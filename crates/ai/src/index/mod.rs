mod file_outline;
pub mod full_source_code_embedding;
pub mod locations;
pub const DEFAULT_SYNC_REQUESTS_PER_MIN: u32 = 600;

#[cfg(feature = "local_fs")]
pub use file_outline::build_outline;

pub use file_outline::{Outline, Symbol};
pub use repo_metadata::{BuildTreeError, DirectoryEntry, Entry, FileId, FileMetadata};

cfg_if::cfg_if! {
    if #[cfg(feature = "local_fs")] {
        pub use repo_metadata::entry::is_git_internal_path;
        pub use repo_metadata::{
            matches_gitignores, path_passes_filters,
        };
    }
}

#[cfg(feature = "local_fs")]
use native::*;
#[cfg(not(feature = "local_fs"))]
use wasm::*;

#[cfg(feature = "local_fs")]
mod native {
    use std::thread::available_parallelism;

    pub(super) const MAX_PARALLEL_THREADS: usize = 2;

    /// Number of parse threads for a machine with `cores` hardware threads. The outline
    /// parse is background work that must never compete with the UI thread, so it gets
    /// one thread on 8 cores or fewer and at most `MAX_PARALLEL_THREADS` otherwise.
    pub(super) fn parse_thread_count(cores: usize) -> usize {
        if cores <= 8 {
            1
        } else {
            (cores / 2).clamp(1, MAX_PARALLEL_THREADS)
        }
    }

    /// Lowers the calling thread to background priority so the parse yields to the UI.
    /// Best effort: failures are ignored.
    fn lower_current_thread_priority() {
        #[cfg(target_os = "linux")]
        {
            // On Linux `setpriority(PRIO_PROCESS, tid, ..)` applies to a single thread.
            // SAFETY: plain syscalls with no pointer arguments.
            unsafe {
                let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
                libc::setpriority(libc::PRIO_PROCESS, tid, 19);
            }
        }
        #[cfg(target_os = "macos")]
        {
            // `qos_class_t` is a `repr(u32)` enum; QOS_CLASS_BACKGROUND = 0x09.
            extern "C" {
                fn pthread_set_qos_class_self_np(class: u32, relative_priority: i32) -> i32;
            }
            // SAFETY: affects only the calling thread; no pointer arguments.
            unsafe {
                pthread_set_qos_class_self_np(0x09, 0);
            }
        }
    }

    fn create_thread_pool() -> Option<rayon::ThreadPool> {
        let num_threads = available_parallelism()
            .map(|parallelism| parse_thread_count(parallelism.get()))
            .unwrap_or(1);

        rayon::ThreadPoolBuilder::new()
            .thread_name(|index| format!("warp-code-indexing-{index}"))
            .num_threads(num_threads)
            .start_handler(|_| lower_current_thread_priority())
            .build()
            .ok()
    }

    lazy_static::lazy_static! {
        pub(super) static ref THREADPOOL: Option<rayon::ThreadPool> = create_thread_pool();
    }
}

#[cfg(not(feature = "local_fs"))]
mod wasm {
    lazy_static::lazy_static! {
        pub(super) static ref THREADPOOL: Option<rayon::ThreadPool> = None;
    }
}

#[cfg(all(test, feature = "local_fs"))]
#[path = "mod_tests.rs"]
mod tests;
