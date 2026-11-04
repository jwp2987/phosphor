//! Windows-specific utilities.

/// Attaches the current process to the console of the parent process.
///
/// This is useful for command-line interfaces that need to ensure all standard
/// output gets printed correctly when run from a terminal.
pub fn attach_to_parent_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Asks Windows for a 1 ms timer resolution for this process (#1260).
///
/// Without it every timer wait rounds up to the 15.6 ms system tick: `Thread::sleep(1 ms)` takes
/// 15.7 ms, and so does every async timer and reactor poll timeout built on the same waits. The
/// 10 ms debounce between a typed key and the frame that shows it fired 14-27 ms late, so a key
/// reached the screen after a second frame instead of the first. Since Windows 10 2004 the
/// request applies to this process only and is dropped when it exits (or, on newer builds, while
/// its windows are hidden), so it is not a system-wide power setting.
pub fn request_high_resolution_timer() {
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn timeBeginPeriod(period: u32) -> u32;
    }
    // SAFETY: `timeBeginPeriod` takes and returns plain integers; a failure (TIMERR_NOCANDO) just
    // leaves the default resolution.
    let _ = unsafe { timeBeginPeriod(1) };
}
