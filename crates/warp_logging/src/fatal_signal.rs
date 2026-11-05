//! One log line for a fatal signal (#1782).
//!
//! appci r18u26 (macOS, `shell_nu`): the app died with no shutdown line, no panic line and no
//! crash report; only the system log said `exited due to SIGBUS | sent by exc handler`. The app
//! log simply ended. This handler writes one `[ERROR] fatal signal SIGBUS (10) code=.. addr=..
//! thread='..'` line to the active log file (and stderr) before the signal takes its course, so
//! the next death names the signal, the fault address and the thread that took it.
//!
//! # Signal safety
//!
//! The handler only calls async-signal-safe functions (`time`, `open`, `write`, `close`,
//! `sigaction`, `raise`) plus `pthread_getname_np`, which reads the thread's own name field. It
//! allocates nothing: the log path is a `CString` prepared at install time and the line is built
//! in a stack buffer.
//!
//! # Chaining
//!
//! The handler restores the action that was installed before it (for SIGSEGV / SIGBUS that is
//! Rust's stack-overflow handler, which prints "has overflowed its stack" and aborts) and
//! returns. A hardware fault re-executes the faulting instruction and faults again into that
//! previous action with the real context, so stack-overflow detection is unchanged. A signal
//! sent with `kill` / `raise` (no fault to repeat) is raised again so it is not swallowed.
//!
//! Kill switch: `PHOSPHOR_FATAL_SIGNAL_LOG=0`.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::OnceLock;

/// The signals that end the process through a fault or an abort.
const FATAL_SIGNALS: [libc::c_int; 6] = [
    libc::SIGBUS,
    libc::SIGSEGV,
    libc::SIGILL,
    libc::SIGFPE,
    libc::SIGABRT,
    libc::SIGTRAP,
];

struct Installed {
    log_path: CString,
    previous: [(libc::c_int, libc::sigaction); FATAL_SIGNALS.len()],
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

/// Whether `PHOSPHOR_FATAL_SIGNAL_LOG` leaves the handler on (anything but `0` / `false` / `off`).
pub(crate) fn enabled_by(value: Option<&str>) -> bool {
    !matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("0" | "false" | "off" | "no")
    )
}

/// Installs the fatal-signal line for the log file at `log_path`. Once per process; a later
/// call does nothing.
pub(crate) fn install(log_path: &Path) {
    if !enabled_by(std::env::var("PHOSPHOR_FATAL_SIGNAL_LOG").ok().as_deref()) {
        return;
    }
    let Ok(log_path) = CString::new(log_path.as_os_str().as_bytes()) else {
        return;
    };
    if INSTALLED.get().is_some() {
        return;
    }
    // SAFETY: a zeroed `sigaction` is a valid "default action, empty mask" value; every entry is
    // overwritten with the current action below before it is used.
    let mut previous: [(libc::c_int, libc::sigaction); FATAL_SIGNALS.len()] =
        FATAL_SIGNALS.map(|sig| (sig, unsafe { std::mem::zeroed() }));
    for (sig, old) in previous.iter_mut() {
        // SAFETY: querying (null new action) only fills `old`.
        if unsafe { libc::sigaction(*sig, std::ptr::null(), old) } != 0 {
            return;
        }
    }
    if INSTALLED.set(Installed { log_path, previous }).is_err() {
        return;
    }
    for sig in FATAL_SIGNALS {
        // SAFETY: a zeroed `sigaction` with an empty mask, our handler and SA_SIGINFO |
        // SA_ONSTACK (so a stack overflow runs it on the thread's alternate stack, as Rust's).
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            let handler: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) =
                on_fatal_signal;
            action.sa_sigaction = handler as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
}

extern "C" fn on_fatal_signal(
    sig: libc::c_int,
    info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
    let Some(installed) = INSTALLED.get() else {
        return;
    };
    // SAFETY: the kernel passes a valid siginfo with SA_SIGINFO; null is handled.
    let (code, addr) = if info.is_null() {
        (0, 0)
    } else {
        unsafe { ((*info).si_code, (*info).si_addr() as usize) }
    };
    let mut name = [0 as libc::c_char; 64];
    // SAFETY: the buffer is 64 bytes and NUL-terminated by the call (or left zeroed on failure).
    let name_len = unsafe {
        if libc::pthread_getname_np(libc::pthread_self(), name.as_mut_ptr(), name.len()) == 0 {
            name.iter().position(|&c| c == 0).unwrap_or(0)
        } else {
            0
        }
    };
    // SAFETY: `name_len` bytes of `name` were written by `pthread_getname_np`.
    let name_bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(name.as_ptr().cast::<u8>(), name_len) };
    // macOS leaves the main thread unnamed: say which one it is.
    #[cfg(target_os = "macos")]
    // SAFETY: `pthread_main_np` only reads the calling thread's identity.
    let name_bytes: &[u8] = if name_bytes.is_empty() && unsafe { libc::pthread_main_np() } == 1 {
        b"main"
    } else {
        name_bytes
    };
    // SAFETY: `time(NULL)` and `getpid` are async-signal-safe.
    let now = unsafe { libc::time(std::ptr::null_mut()) } as i64;
    let pid = unsafe { libc::getpid() } as u64;
    let mut line = LineBuf::new();
    format_line(&mut line, now, sig, code, addr, name_bytes, pid);
    // SAFETY: open/write/close with a valid NUL-terminated path and an in-bounds buffer.
    unsafe {
        let fd = libc::open(
            installed.log_path.as_ptr(),
            libc::O_WRONLY | libc::O_APPEND | libc::O_CLOEXEC,
        );
        if fd >= 0 {
            libc::write(fd, line.bytes().as_ptr().cast(), line.bytes().len());
            libc::close(fd);
        }
        libc::write(
            libc::STDERR_FILENO,
            line.bytes().as_ptr().cast(),
            line.bytes().len(),
        );
    }
    // Hand the signal to whatever was installed before us.
    if let Some((_, old)) = installed.previous.iter().find(|(s, _)| *s == sig) {
        // SAFETY: restoring an action previously returned by `sigaction`.
        unsafe { libc::sigaction(sig, old, std::ptr::null_mut()) };
    }
    if sent_by_a_process(code) {
        // No faulting instruction will repeat it: deliver it again once this handler returns
        // (the signal is blocked while it runs).
        // SAFETY: `raise` is async-signal-safe.
        unsafe { libc::raise(sig) };
    }
}

/// Whether `si_code` says the signal came from `kill` / `raise` / `sigqueue` rather than a fault:
/// `<= 0` on Linux (SI_USER 0, SI_QUEUE -1, SI_TKILL -6), `0x1000x` on macOS (SI_USER 0x10001).
pub(crate) fn sent_by_a_process(code: libc::c_int) -> bool {
    code <= 0 || code >= 0x10000
}

pub(crate) fn signal_name(sig: libc::c_int) -> &'static str {
    match sig {
        libc::SIGBUS => "SIGBUS",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGILL => "SIGILL",
        libc::SIGFPE => "SIGFPE",
        libc::SIGABRT => "SIGABRT",
        libc::SIGTRAP => "SIGTRAP",
        _ => "signal",
    }
}

/// A fixed-size line buffer: no allocation in signal context. Text past its end is dropped.
pub(crate) struct LineBuf {
    buf: [u8; 512],
    len: usize,
}

impl LineBuf {
    pub(crate) fn new() -> Self {
        Self {
            buf: [0; 512],
            len: 0,
        }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    fn push(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
    }

    fn push_dec(&mut self, mut value: u64, min_width: usize) {
        let mut digits = [0u8; 20];
        let mut i = digits.len();
        loop {
            i -= 1;
            digits[i] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 && digits.len() - i >= min_width {
                break;
            }
        }
        self.push(&digits[i..]);
    }

    fn push_signed(&mut self, value: i64) {
        if value < 0 {
            self.push(b"-");
        }
        self.push_dec(value.unsigned_abs(), 1);
    }

    fn push_hex(&mut self, mut value: usize) {
        let mut digits = [0u8; 16];
        let mut i = digits.len();
        loop {
            i -= 1;
            digits[i] = b"0123456789abcdef"[value & 0xf];
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        self.push(b"0x");
        self.push(&digits[i..]);
    }
}

/// `(year, month, day, hour, minute, second)` in UTC for a Unix time (Hinnant's civil-from-days).
pub(crate) fn utc_civil(secs: i64) -> (i64, u64, u64, u64, u64, u64) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400) as u64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u64;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u64;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, rem / 3_600, rem % 3_600 / 60, rem % 60)
}

/// The fatal-signal line, in the file log's own `<UTC> [ERROR] ...` form so the harness's crash
/// patterns (`SIGBUS`, `SIGSEGV`, ...) and its time slicing find it.
pub(crate) fn format_line(
    out: &mut LineBuf,
    unix_secs: i64,
    sig: libc::c_int,
    code: libc::c_int,
    addr: usize,
    thread: &[u8],
    pid: u64,
) {
    let (year, month, day, hour, minute, second) = utc_civil(unix_secs);
    out.push_signed(year);
    out.push(b"-");
    out.push_dec(month, 2);
    out.push(b"-");
    out.push_dec(day, 2);
    out.push(b"T");
    out.push_dec(hour, 2);
    out.push(b":");
    out.push_dec(minute, 2);
    out.push(b":");
    out.push_dec(second, 2);
    out.push(b"Z [ERROR] fatal signal ");
    out.push(signal_name(sig).as_bytes());
    out.push(b" (");
    out.push_signed(i64::from(sig));
    out.push(b") code=");
    out.push_signed(i64::from(code));
    out.push(b" addr=");
    out.push_hex(addr);
    out.push(b" thread='");
    out.push(if thread.is_empty() {
        b"<unnamed>".as_slice()
    } else {
        thread
    });
    out.push(b"' pid=");
    out.push_dec(pid, 1);
    out.push(if sent_by_a_process(code) {
        b" (sent by a process); the process is about to die\n".as_slice()
    } else {
        b" (fault); the process is about to die\n".as_slice()
    });
}

#[cfg(test)]
#[path = "fatal_signal_tests.rs"]
mod tests;
