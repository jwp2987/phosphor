//! Delivers a very large command to bash as a base64 payload it decodes and sources (#1129, #1255).
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
///
/// With bracketed paste only a MULTI-LINE command is wrapped (#1191). The quadratic cost the
/// larger size guards against is readline's here-doc/multi-line paste handling; a command on one
/// line has none. Measured on a 1 MB single line: pasted as it is, the shell had it in 48-52 ms
/// (xterm, same payload), while the wrapper's 1.3 MB base64 line took 1.3-1.8 s to reach the
/// shell (3 MB: 4.3-5.4 s, vs 140 ms) and made the terminal parse the echo of the longer line
/// and then the command again for the display override.
pub(super) fn needs_wrapper_for(command: &str, bracketed_paste: bool) -> bool {
    if bracketed_paste {
        return command.len() >= MIN_WRAPPED_BYTES_BRACKETED && has_line_break(command);
    }
    command.len() >= MIN_WRAPPED_BYTES
}

/// Whether `command` spans more than one line (`\n` or `\r`).
fn has_line_break(command: &str) -> bool {
    command.bytes().any(|b| b == b'\n' || b == b'\r')
}

/// Whether `command` should be sent through [`wrapper_line`] for a shell without bracketed paste.
#[cfg(test)]
pub(super) fn needs_wrapper(command: &str) -> bool {
    needs_wrapper_for(command, false)
}

/// The widest line of folded base64 (a multiple of 4), well under macOS's 1024-byte canonical
/// line limit (`MAX_CANON`) and Linux's 4095, so a shell or tty that reads it in cooked mode
/// never truncates it either.
pub(super) const FOLD_WIDTH: usize = 960;

/// The wrapper that runs `command`, as ONE line. Only base64 characters and fixed text end up
/// inside the single quotes, so no quoting of the command is needed. Used where the shell takes
/// the command as a bracketed paste (the line editor does not redisplay per character).
pub(super) fn wrapper_line(command: &str) -> String {
    wrapper(command, false)
}

/// The wrapper for a shell WITHOUT bracketed paste, which receives the bytes as typed keys
/// (#1129). One 190 KB line made a remote macOS bash 3.2 take 86 s, about 2 KB/s: readline
/// redisplays the whole line per typed character, so the cost grows with the square of the
/// line's length. The base64 is therefore folded onto lines of at most [`FOLD_WIDTH`]
/// characters inside the same single-quoted string; the shell sees a quoted string that is
/// still open at each newline (a continuation line, no command runs until the closing quote),
/// and `base64` ignores the newlines when it decodes. Total redisplay work becomes
/// proportional to size x [`FOLD_WIDTH`].
pub(super) fn wrapper_lines_for_typed_input(command: &str) -> String {
    wrapper(command, true)
}

fn wrapper(command: &str, fold: bool) -> String {
    let script = format!("rm -f \"$__pc\"; unset __pc\n{command}\n");
    let mut encoded = STANDARD.encode(script);
    if fold {
        // Base64 is ASCII, so byte chunks are character chunks.
        let mut folded = String::with_capacity(encoded.len() + encoded.len() / FOLD_WIDTH + 1);
        for (i, chunk) in encoded.as_bytes().chunks(FOLD_WIDTH).enumerate() {
            if i > 0 {
                folded.push('\n');
            }
            folded.push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
        }
        encoded = folded;
    }
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
