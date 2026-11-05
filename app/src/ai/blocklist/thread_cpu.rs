//! The CPU time a thread has actually run (#1651).
//!
//! The Enter's verdict budget caps the classifier's own work. Measured on a wall clock, time the
//! classifier's thread spends runnable but waiting for a CPU (a loaded machine, a busy build)
//! counted against it, so a slow machine handed a paste back that a fast one would have routed.
//! These clocks only advance while the thread is on a CPU:
//!
//! * Linux / Android: `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` for the calling thread, and the
//!   thread's `pthread_getcpuclockid` clock for a reader on another thread;
//! * macOS: `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` for the calling thread, and
//!   `thread_info(THREAD_BASIC_INFO)` on the thread's mach port for a reader on another thread;
//! * Windows: `GetThreadTimes` (kernel + user time), on `GetCurrentThread()` for the calling
//!   thread and on a handle opened for the thread for a reader on another thread. Windows books
//!   thread time per scheduler tick, so it moves in steps of ~15.6 ms.
//!
//! Elsewhere (and if a call fails) there is no thread clock: [`current_thread_cpu`] returns
//! `None` and the caller falls back to wall time.

use std::time::Duration;

/// CPU time (user + system) the calling thread has run so far, or `None` where the platform has
/// no per-thread CPU clock or the call failed.
pub fn current_thread_cpu() -> Option<Duration> {
    imp::current_thread_cpu()
}

/// A reader of one thread's CPU clock that another thread may use. Taken on the measured thread
/// with [`ThreadCpuProbe::current`]. It is only valid while that thread is alive: read it only
/// while the measured thread is known to be running (for [`super::compute_clock::ComputeClock`],
/// inside a poll, under the lock the poll's end takes).
#[derive(Clone, Copy, Debug)]
pub struct ThreadCpuProbe(imp::Probe);

impl ThreadCpuProbe {
    /// A probe of the calling thread, or `None` where the platform cannot read another thread's
    /// CPU clock.
    pub fn current() -> Option<Self> {
        imp::probe_current().map(Self)
    }

    /// CPU time the probed thread has run so far, or `None` if the read failed.
    pub fn read(&self) -> Option<Duration> {
        imp::probe_read(self.0)
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn read_clock(clock: libc::clockid_t) -> Option<Duration> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out-pointer for the duration of the call.
    let ok = unsafe { libc::clock_gettime(clock, &mut ts) } == 0;
    ok.then(|| {
        Duration::new(
            ts.tv_sec.max(0) as u64,
            ts.tv_nsec.clamp(0, 999_999_999) as u32,
        )
    })
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod imp {
    use std::time::Duration;

    pub type Probe = libc::clockid_t;

    pub fn current_thread_cpu() -> Option<Duration> {
        super::read_clock(libc::CLOCK_THREAD_CPUTIME_ID)
    }

    pub fn probe_current() -> Option<Probe> {
        let mut clock: libc::clockid_t = 0;
        // SAFETY: `pthread_self()` is the calling (live) thread and `clock` a valid out-pointer.
        let ok = unsafe { libc::pthread_getcpuclockid(libc::pthread_self(), &mut clock) } == 0;
        ok.then_some(clock)
    }

    pub fn probe_read(clock: Probe) -> Option<Duration> {
        super::read_clock(clock)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::time::Duration;

    pub type Probe = libc::mach_port_t;

    pub fn current_thread_cpu() -> Option<Duration> {
        super::read_clock(libc::CLOCK_THREAD_CPUTIME_ID)
    }

    pub fn probe_current() -> Option<Probe> {
        // SAFETY: `pthread_self()` is the calling (live) thread. The port is borrowed from the
        // pthread (no reference is added), so there is nothing to release.
        let port = unsafe { libc::pthread_mach_thread_np(libc::pthread_self()) };
        // MACH_PORT_NULL
        (port != 0).then_some(port)
    }

    pub fn probe_read(port: Probe) -> Option<Duration> {
        // SAFETY: a zeroed `thread_basic_info` is a valid value; `count` holds its size in
        // `integer_t`s, as `thread_info` requires, and both pointers outlive the call.
        let mut info: libc::thread_basic_info = unsafe { std::mem::zeroed() };
        let mut count = libc::THREAD_BASIC_INFO_COUNT;
        let kr = unsafe {
            libc::thread_info(
                port,
                libc::THREAD_BASIC_INFO as libc::thread_flavor_t,
                &mut info as *mut libc::thread_basic_info as libc::thread_info_t,
                &mut count,
            )
        };
        if kr != libc::KERN_SUCCESS {
            return None;
        }
        let time = |t: libc::time_value_t| {
            Duration::from_secs(t.seconds.max(0) as u64)
                + Duration::from_micros(t.microseconds.max(0) as u64)
        };
        Some(time(info.user_time) + time(info.system_time))
    }
}

#[cfg(windows)]
mod imp {
    use std::time::Duration;

    /// A Win32 `HANDLE`, as `isize` like the other kernel32 declarations in this crate
    /// (`terminal::writeable_pty::command_pipe`), so the shared `CloseHandle` declaration agrees.
    type Handle = isize;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> Handle;
        fn GetCurrentThreadId() -> u32;
        fn OpenThread(desired_access: u32, inherit_handle: i32, thread_id: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetThreadTimes(
            thread: Handle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }

    const THREAD_QUERY_LIMITED_INFORMATION: u32 = 0x0800;

    pub type Probe = Handle;

    fn thread_times(thread: Handle) -> Option<Duration> {
        let (mut creation, mut exit, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        // SAFETY: the four out-pointers are valid for the call; `thread` is a live thread handle
        // (the current-thread pseudo handle, or one this module opened and has not closed).
        let ok =
            unsafe { GetThreadTimes(thread, &mut creation, &mut exit, &mut kernel, &mut user) }
                != 0;
        if !ok {
            return None;
        }
        // FILETIME durations are in 100 ns units.
        let units = |t: FileTime| (u64::from(t.high) << 32) | u64::from(t.low);
        Some(Duration::from_nanos(
            units(kernel)
                .saturating_add(units(user))
                .saturating_mul(100),
        ))
    }

    pub fn current_thread_cpu() -> Option<Duration> {
        // SAFETY: `GetCurrentThread` returns a pseudo handle that needs no closing.
        thread_times(unsafe { GetCurrentThread() })
    }

    /// A real (not pseudo) handle to the owning thread, opened once per thread and closed when
    /// the thread exits.
    struct OwnThreadHandle(Handle);

    impl Drop for OwnThreadHandle {
        fn drop(&mut self) {
            if self.0 != 0 {
                // SAFETY: the handle was opened by `OpenThread` below and is closed only here.
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    thread_local! {
        static OWN_THREAD_HANDLE: OwnThreadHandle = OwnThreadHandle(
            // SAFETY: plain FFI call; a failure returns NULL (0), which is never used.
            unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, GetCurrentThreadId()) },
        );
    }

    pub fn probe_current() -> Option<Probe> {
        OWN_THREAD_HANDLE
            .try_with(|handle| handle.0)
            .ok()
            .filter(|handle| *handle != 0)
    }

    pub fn probe_read(handle: Probe) -> Option<Duration> {
        thread_times(handle)
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
)))]
mod imp {
    use std::time::Duration;

    /// Never constructed: this platform has no thread CPU clock.
    #[allow(dead_code)]
    #[derive(Clone, Copy, Debug)]
    pub struct Probe;

    pub fn current_thread_cpu() -> Option<Duration> {
        None
    }

    pub fn probe_current() -> Option<Probe> {
        None
    }

    pub fn probe_read(_: Probe) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
#[path = "thread_cpu_tests.rs"]
mod tests;
