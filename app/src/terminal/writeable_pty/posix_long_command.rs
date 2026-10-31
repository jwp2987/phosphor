//! Delivers a very large command to bash as one short line (#1129, #1255).
//!
//! Bash older than 4.4, and 4.4 and 5.0 where the user's readline config leaves
//! `enable-bracketed-paste` off, get a command key by key. Typed that way a multi-hundred-KB
//! here-doc (an agent writing a file over ssh) slows down and, past roughly 850 KB, stopped
//! answering in a measurement on Linux bash 5.3 with bracketed paste turned off. The command is
//! instead base64-encoded onto one line that the shell decodes into a temp file and sources.
//!
//! The file's first line removes the file, so it is gone before the command starts: an `exit`, a
//! ^C or a failing command leaves nothing behind. Sourcing runs the command in the shell itself,
//! as typing it would, and its exit status is the line's. The block shows the original command
//! (`set_command_display_override`, as for PowerShell's launcher).
//!
//! zsh is not handled: it has had bracketed paste since 5.1, and its line editor is slow on a
//! single very long line, which this would make worse.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// Commands below this size are typed as before. Typing was measured fine up to ~800 KB; this is
/// well below, so that nothing ordinary changes and only the sizes that risk a stall are routed.
pub(super) const MIN_WRAPPED_BYTES: usize = 128 * 1024;

/// The size from which a command is wrapped even when the shell has bracketed paste (#1255).
/// A multi-line command (a here-doc) pasted in one bracketed chunk is processed by readline and
/// bash's here-doc reader at a rate that grows faster than its size: measured on bash 5.3, 256 KB
/// took 0.2 s, 1 MB 1.6 s and 10 MB over 50 s, while the same bytes as one base64 line took 1.0 s
/// and 9.7 s. Below this size the command goes in as pasted, as before.
pub(super) const MIN_WRAPPED_BYTES_BRACKETED: usize = 512 * 1024;

/// Whether `command` should be sent through [`wrapper_line`], given whether the shell has
/// bracketed paste turned on.
pub(super) fn needs_wrapper_for(command: &str, bracketed_paste: bool) -> bool {
    let min = if bracketed_paste {
        MIN_WRAPPED_BYTES_BRACKETED
    } else {
        MIN_WRAPPED_BYTES
    };
    command.len() >= min
}

/// Whether `command` should be sent through [`wrapper_line`] for a shell without bracketed paste.
#[cfg(test)]
pub(super) fn needs_wrapper(command: &str) -> bool {
    needs_wrapper_for(command, false)
}

/// The one-line wrapper that runs `command`. Only base64 characters and fixed text end up
/// inside the single quotes, so no quoting of the command is needed.
pub(super) fn wrapper_line(command: &str) -> String {
    let script = format!("rm -f \"$__pc\"; unset __pc\n{command}\n");
    let encoded = STANDARD.encode(script);
    // `base64 -D` is what macOS before 13 calls decode.
    format!(
        "__pc=$(mktemp \"${{TMPDIR:-/tmp}}/.phosphor-cmd-XXXXXX\") && {{ \
         if printf %s '{encoded}' | {{ base64 -d 2>/dev/null || base64 -D; }} > \"$__pc\"; \
         then . \"$__pc\"; \
         else rm -f \"$__pc\"; unset __pc; \
         echo 'phosphor: could not decode a long command' >&2; false; fi; }}"
    )
}

#[cfg(test)]
#[path = "posix_long_command_tests.rs"]
mod tests;
