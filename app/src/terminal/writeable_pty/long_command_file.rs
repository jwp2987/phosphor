//! Runs a very long PowerShell command through a launcher line instead of typing it into
//! PSReadLine (#1106), with the command text carried on the session's named pipe (#1592).
//!
//! PSReadLine re-renders its whole buffer on every key, and Windows PowerShell 5.1's PSReadLine
//! 2.0 cannot render a line taller than the console buffer: `SetCursorPosition` is called with a
//! negative row, the key handler throws, the command is never submitted, and at ~1 MB it falls
//! into an "Oops, something went wrong" crash loop. Windows PowerShell has no bracketed paste, so
//! the command is typed key by key whatever the app does. A command past [`MAX_TYPED_BYTES`] is
//! therefore served on the pipe (see `command_pipe`; commands can contain secrets, so the text
//! never touches the disk) and the shell is sent a short launcher line, [`LAUNCHER_LINE`], whose
//! function `Warp-Run-LongCommand` (in `bootstrap/pwsh.ps1`) reads the pipe and returns a script
//! block that the dot runs in the typed line's own scope. The launcher is not `Alt+3`'s
//! `Insert`: that would put the whole command in PSReadLine's buffer, which is exactly what
//! cannot be rendered. It is not a dot-sourced `.ps1` either: that would be refused under the
//! Restricted execution policy of a stock Windows install, whereas a script block made from a
//! string is not subject to it. The bootstrap keeps the launcher out of history (so a long
//! command is not on the Up arrow) and reports the launcher to preexec; the app displays the real
//! command (`set_command_display_override`).

/// Longest command still typed into the line editor. 2,000 and 4,000 characters were fine on
/// Windows PowerShell 5.1; 8,000 failed. Kept well under the first size known to be safe so a
/// narrow window (more wrapped rows per character) is covered too.
pub(super) const MAX_TYPED_BYTES: usize = 1024;

/// The two bytes (`ESC 3`, Alt+3) that make the bootstrap's key handler read the command from the
/// session's named pipe (see `command_pipe`), replace the line with it and run it (#1581). The
/// same ConPTY translation as `Alt+1`/`Alt+2`.
pub(super) const INSERT_CHORD: &[u8] = b"\x1b3";

/// Whether `command` has to go through the launcher rather than the line editor.
pub(super) fn needs_launcher(command: &str) -> bool {
    command.len() > MAX_TYPED_BYTES
}

/// The launcher line typed for a long command. It carries no command text and no path: the
/// function reads the pipe named by `$global:_warpCmdPipe`. Must match `bootstrap/pwsh.ps1`.
pub(super) const LAUNCHER_LINE: &str = ". (Warp-Run-LongCommand)";

/// Whether `command` can be inserted by the `Alt+3` chord instead of typed: one line, short
/// enough to be typed at all, and free of control characters (typed, a Tab would run completion
/// and an ESC would start a chord; the chord's `Insert` would take them literally, which is not
/// what the typed path does, so those stay typed).
pub(super) fn can_insert_via_pipe(command: &str) -> bool {
    !command.is_empty() && !needs_launcher(command) && !command.chars().any(char::is_control)
}

/// `PHOSPHOR_PWSH_FAST_INSERT=0` turns the chord path off (the kill switch); the old name
/// `PHOSPHOR_PWSH_FILE_INSERT=0` is still honoured. Anything else, or unset, leaves it on.
pub(super) fn fast_insert_enabled() -> bool {
    fast_insert_enabled_for(
        std::env::var("PHOSPHOR_PWSH_FAST_INSERT").ok().as_deref(),
        std::env::var("PHOSPHOR_PWSH_FILE_INSERT").ok().as_deref(),
    )
}

/// The kill switch's decision for the two variables' values (`None` = unset).
pub(super) fn fast_insert_enabled_for(value: Option<&str>, legacy: Option<&str>) -> bool {
    [value, legacy]
        .into_iter()
        .flatten()
        .all(|value| value.trim() != "0")
}

#[cfg(test)]
#[path = "long_command_file_tests.rs"]
mod tests;
