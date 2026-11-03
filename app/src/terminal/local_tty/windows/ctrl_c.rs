//! Shells started in a pseudoconsole must be able to receive Ctrl-C (#1570).
//!
//! Windows keeps a per-process "ignore Ctrl-C" flag (`CONSOLE_IGNORE_CTRL_C` in the process
//! parameters). `SetConsoleCtrlHandler(NULL, TRUE)` sets it, `CREATE_NEW_PROCESS_GROUP` sets it
//! for the new process, and every child inherits it. A process with the flag set never runs its
//! Ctrl-C handlers: the console still turns the 0x03 Phosphor writes into a `CTRL_C_EVENT`, but
//! Windows PowerShell (and cmd.exe, and any console program that relies on the console's Ctrl-C)
//! ignores it, so `Start-Sleep 30` runs its full 30 s. Git Bash is not affected: MSYS reads the
//! 0x03 as a character and raises SIGINT itself.
//!
//! Phosphor inherits the flag whenever whatever launched it had it: a CI runner that starts jobs
//! in a new process group (the appci harness, where Ctrl-C stopped the PowerShell sleep in
//! neither Phosphor nor the conhost reference), a launcher or a service wrapper. The shell then
//! inherits it from Phosphor. A terminal's shell must get Ctrl-C regardless of how the terminal
//! itself was started, so the flag is cleared once, before the first shell is spawned.
//!
//! Clearing it only changes Phosphor's own handling of a Ctrl-C from a console it is attached to,
//! which the GUI app is not: `SetConsoleCtrlHandler(NULL, FALSE)` restores the default for this
//! process, the same as when it is launched from Explorer. `PHOSPHOR_CONPTY_CTRL_C_RESET=0` keeps
//! the inherited flag (the old behaviour), for comparison.

use std::sync::Once;

use windows::Win32::System::Console::SetConsoleCtrlHandler;

/// The kill switch: `PHOSPHOR_CONPTY_CTRL_C_RESET=0` (or `off`/`false`) keeps the inherited flag.
pub(super) const KILL_SWITCH_ENV: &str = "PHOSPHOR_CONPTY_CTRL_C_RESET";

/// Whether the inherited ignore-Ctrl-C flag is cleared, given the kill switch's value.
pub(super) fn reset_enabled(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// Clears this process's ignore-Ctrl-C flag once, so every shell spawned after it can be
/// interrupted (see the module docs). Cheap after the first call.
pub(super) fn ensure_children_receive_ctrl_c() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if !reset_enabled(std::env::var(KILL_SWITCH_ENV).ok().as_deref()) {
            log::info!(
                "{KILL_SWITCH_ENV}=0: shells inherit this process's Ctrl-C disposition (#1570)"
            );
            return;
        }
        // SAFETY: a null handler with `FALSE` only clears the calling process's ignore-Ctrl-C
        // flag; it registers nothing and touches no memory of ours.
        match unsafe { SetConsoleCtrlHandler(None, false) } {
            Ok(()) => log::info!(
                "cleared any inherited ignore-Ctrl-C flag so pseudoconsole shells receive Ctrl-C \
                 (#1570; {KILL_SWITCH_ENV}=0 keeps it)"
            ),
            Err(error) => log::warn!(
                "could not clear the inherited ignore-Ctrl-C flag ({error}); a shell started \
                 from a process that ignores Ctrl-C will ignore it too (#1570)"
            ),
        }
    });
}

#[cfg(test)]
#[path = "ctrl_c_tests.rs"]
mod tests;
