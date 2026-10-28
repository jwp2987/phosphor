//! Phosphor (#1052): one place that turns arbitrary text into a single-line tab title.
//!
//! A tab or sidebar title is laid out as one line, and the text layout drops line breaks, so a
//! title that still contains them renders with its lines glued together (a pasted
//! `echo PL1` / `echo PL2` / `echo PL3` became "echo PL1echo PL2echo PL3" once the block
//! finished). Every title source that may carry a line break goes through
//! [`single_line_title`], which keeps the first non-blank line and marks the rest with an
//! ellipsis — the same "first line…" form the bash and PowerShell bootstraps give the running
//! command's OSC title (#935), so the running and finished titles agree.

use std::borrow::Cow;

/// The marker appended to a title whose text continued on further lines.
pub const MULTILINE_TITLE_ELLIPSIS: char = '\u{2026}';

/// Returns `text` as a single-line title.
///
/// Text without a line break (`\n` or `\r`) is returned unchanged. Otherwise the result is the
/// first non-blank line, trimmed, followed by [`MULTILINE_TITLE_ELLIPSIS`] when any later line
/// is non-blank; blank lines (including the empty pieces of a `\r\n` pair and a trailing
/// newline) never earn the ellipsis. Text made only of blank lines becomes the empty string.
/// No length truncation happens here: clipping a long line is the renderer's job.
pub fn single_line_title(text: &str) -> Cow<'_, str> {
    if !text.contains(['\n', '\r']) {
        return Cow::Borrowed(text);
    }
    let mut lines = text
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return Cow::Borrowed("");
    };
    if lines.next().is_some() {
        Cow::Owned(format!("{first}{MULTILINE_TITLE_ELLIPSIS}"))
    } else {
        Cow::Borrowed(first)
    }
}

/// Returns the running title of a command that spans more than one non-blank line: its first
/// line and [`MULTILINE_TITLE_ELLIPSIS`], or `None` for a single-line command.
///
/// Phosphor (#1093): a shell's own title for a running command cannot carry this. bash runs a
/// pasted block line by line and reports only the first line to its preexec hook, zsh's hook
/// titles with the command word, and fish's `fish_title` joins the lines with spaces. The block's
/// command text is the one source that has every line, so the app derives the title from it.
pub fn multiline_command_title(command: &str) -> Option<String> {
    let mut lines = command
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let first = lines.next()?;
    lines
        .next()
        .is_some()
        .then(|| format!("{first}{MULTILINE_TITLE_ELLIPSIS}"))
}

#[cfg(test)]
#[path = "tab_title_tests.rs"]
mod tests;
