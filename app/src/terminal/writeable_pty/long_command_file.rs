//! Runs a very long PowerShell command from a file instead of typing it into PSReadLine (#1106).
//!
//! PSReadLine re-renders its whole buffer on every key, and Windows PowerShell 5.1's PSReadLine
//! 2.0 cannot render a line taller than the console buffer: `SetCursorPosition` is called with a
//! negative row, the key handler throws, the command is never submitted, and at ~1 MB it falls
//! into an "Oops, something went wrong" crash loop. Windows PowerShell has no bracketed paste, so
//! the command is typed key by key whatever the app does. A command past [`MAX_TYPED_BYTES`] is
//! therefore written to a temp `.ps1` and the shell is sent a short line that dot-sources it
//! (same scope as a typed command). The bootstrap's history filter and preexec hook know the
//! file name ([`FILE_PREFIX`]) and keep the launcher line out of history and report the real
//! command text.

use std::io::Write;

use tempfile::TempPath;

/// Longest command still typed into the line editor. 2,000 and 4,000 characters were fine on
/// Windows PowerShell 5.1; 8,000 failed. Kept well under the first size known to be safe so a
/// narrow window (more wrapped rows per character) is covered too.
pub(super) const MAX_TYPED_BYTES: usize = 1024;

/// Must match the `.phosphor-cmd-` pattern in `bootstrap/pwsh.ps1`.
pub(super) const FILE_PREFIX: &str = ".phosphor-cmd-";

/// Whether `command` has to go through a file rather than the line editor.
pub(super) fn needs_file(command: &str) -> bool {
    command.len() > MAX_TYPED_BYTES
}

/// The line that dot-sources `path`. Single-quoted, every quote PowerShell accepts doubled.
pub(super) fn launcher_line(path: &str) -> String {
    let mut line = String::from(". '");
    for c in path.chars() {
        line.push(c);
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            line.push(c);
        }
    }
    line.push('\'');
    line
}

/// Writes `command` to a temp `.ps1` (UTF-8 with a BOM, which Windows PowerShell 5.1 needs to
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

#[cfg(test)]
#[path = "long_command_file_tests.rs"]
mod tests;
