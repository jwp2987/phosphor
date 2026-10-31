//! Delivers a very large command to a bash that has no bracketed paste as one short line (#1129).
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

/// Whether `command` should be sent through [`wrapper_line`].
pub(super) fn needs_wrapper(command: &str) -> bool {
    command.len() >= MIN_WRAPPED_BYTES
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
