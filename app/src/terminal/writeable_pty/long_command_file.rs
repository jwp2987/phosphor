//! Runs a very long PowerShell command from a file instead of typing it into PSReadLine (#1106).
//!
//! PSReadLine re-renders its whole buffer on every key, and Windows PowerShell 5.1's PSReadLine
//! 2.0 cannot render a line taller than the console buffer: `SetCursorPosition` is called with a
//! negative row, the key handler throws, the command is never submitted, and at ~1 MB it falls
//! into an "Oops, something went wrong" crash loop. Windows PowerShell has no bracketed paste, so
//! the command is typed key by key whatever the app does. A command past [`MAX_TYPED_BYTES`] is
//! therefore written to a temp file and the shell is sent a short launcher line,
//! `. (Warp-Run-LongCommand '<path>')`, defined in `bootstrap/pwsh.ps1`. The function reads and
//! deletes the file and returns a script block that the dot runs in the typed line's own scope.
//! It is not a dot-sourced `.ps1`: that would be refused under the Restricted execution policy of
//! a stock Windows install (the app starts PowerShell without `-ExecutionPolicy`), whereas a
//! script block made from a string is not subject to it. The bootstrap keeps the launcher out of
//! history (so a long command is not on the Up arrow) and reports the real command to preexec.
//! The file is also removed when [`TempPath`] drops, as a fallback if it was never read.

use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::TempPath;

/// Longest command still typed into the line editor. 2,000 and 4,000 characters were fine on
/// Windows PowerShell 5.1; 8,000 failed. Kept well under the first size known to be safe so a
/// narrow window (more wrapped rows per character) is covered too.
pub(super) const MAX_TYPED_BYTES: usize = 1024;

/// Must match the `.phosphor-cmd-` pattern in `bootstrap/pwsh.ps1`.
pub(super) const FILE_PREFIX: &str = ".phosphor-cmd-";

/// The two bytes (`ESC 3`, Alt+3) that make the bootstrap's key handler read the session file,
/// replace the line with it and run it (#1581). The same ConPTY translation as `Alt+1`/`Alt+2`.
pub(super) const INSERT_CHORD: &[u8] = b"\x1b3";

/// Whether `command` has to go through a file rather than the line editor.
pub(super) fn needs_file(command: &str) -> bool {
    command.len() > MAX_TYPED_BYTES
}

/// The launcher line for `path`. Single-quoted, every quote character PowerShell accepts doubled
/// (the bootstrap undoes exactly this).
pub(super) fn launcher_line(path: &str) -> String {
    let mut line = String::from(". (Warp-Run-LongCommand '");
    for c in path.chars() {
        line.push(c);
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            line.push(c);
        }
    }
    line.push_str("')");
    line
}

/// Writes `command` to a temp file (UTF-8 with a BOM, which Windows PowerShell 5.1 needs to
/// read it as UTF-8) and returns it with the launcher line. `None` on any failure, in which case
/// the caller types the command as before.
pub(super) fn write_command_file(command: &str) -> Option<(TempPath, String)> {
    let file = tempfile::Builder::new()
        .prefix(FILE_PREFIX)
        .suffix(".ps1")
        .tempfile()
        .map_err(|err| log::warn!("Could not create a temp file for a long command: {err:#}"))
        .ok()?;
    let (mut handle, path) = file.into_parts();
    let written = handle
        .write_all(&[0xEF, 0xBB, 0xBF])
        .and_then(|()| handle.write_all(command.as_bytes()))
        .and_then(|()| handle.flush());
    drop(handle);
    if let Err(err) = written {
        log::warn!("Could not write a long command to a temp file: {err:#}");
        return None;
    }
    let line = launcher_line(path.to_str()?);
    Some((path, line))
}

/// The fixed per-session file the PowerShell insert chord reads (#1581):
/// `<temp>\.phosphor-cmd-<session_id>`. Must match the `Alt+3` handler in `bootstrap/pwsh.ps1`,
/// which builds the same path from `[IO.Path]::GetTempPath()` and `$global:_warpSessionId`.
pub(super) fn session_file_path(dir: &Path, session_id: u64) -> PathBuf {
    dir.join(format!("{FILE_PREFIX}{session_id}"))
}

/// Writes `command` (UTF-8 with a BOM) to the session's fixed file in `dir`, replacing a stale
/// one, and returns its path. The file is removed first and created with `create_new`, so a
/// leftover or a planted link is never written through. `None` on any failure; the caller then
/// types the command as before. Written synchronously: the chord that makes the shell read it is
/// only queued after this returns.
pub(super) fn write_session_command_file(
    dir: &Path,
    session_id: u64,
    command: &str,
) -> Option<PathBuf> {
    let path = session_file_path(dir, session_id);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            log::warn!("Could not replace the PowerShell command file: {err:#}");
            return None;
        }
    }
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut handle| {
            handle.write_all(&[0xEF, 0xBB, 0xBF])?;
            handle.write_all(command.as_bytes())?;
            handle.flush()
        });
    if let Err(err) = written {
        log::warn!("Could not write the PowerShell command file: {err:#}");
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(path)
}

/// Whether `command` can be inserted by the `Alt+3` chord instead of typed: one line, short
/// enough to be typed at all, and free of control characters (typed, a Tab would run completion
/// and an ESC would start a chord; the chord's `Insert` would take them literally, which is not
/// what the typed path does, so those stay typed).
pub(super) fn can_insert_from_file(command: &str) -> bool {
    !command.is_empty() && !needs_file(command) && !command.chars().any(char::is_control)
}

/// `PHOSPHOR_PWSH_FILE_INSERT=0` turns the chord path off (the kill switch); anything else, or
/// unset, leaves it on.
pub(super) fn file_insert_enabled() -> bool {
    file_insert_enabled_for(std::env::var("PHOSPHOR_PWSH_FILE_INSERT").ok().as_deref())
}

/// The kill switch's decision for the variable's value (`None` = unset).
pub(super) fn file_insert_enabled_for(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim() != "0")
}

#[cfg(test)]
#[path = "long_command_file_tests.rs"]
mod tests;
