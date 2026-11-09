//! Pure command builders and output parsers for the no-helper ("shell mode") backend, shared by
//! the app's `ShellBackend` and (later) the helper's git code so both build the same argv and
//! parse the same output.
//!
//! Nothing here does I/O, spawns a process, reads a clock or names warpui: every function maps
//! strings to strings (or bytes to a parsed value) and is unit-tested with golden values. That is
//! what lets the same builders run against a real ssh host, a local `sh`, and the helper.
//!
//! * [`frame`]: the framed output envelope (sentinel-delimited sections plus an exit code).
//! * [`shell_kind`]: [`ShellKind`] and the `sh -c` wrapping a non-POSIX login shell needs.
//! * [`base64`]: the base64 codec and the host base64 tool probe (binary-safe transfer).
//! * [`git`]: git argv builders, parsers and algorithms over a `GitRunner` (P5).
//! * [`predicates`]: "may this command change HEAD / the cwd", the refresh-coordinator inputs.
//! * [`read`]: batch file reads (`ReadFileContext`), shared by the helper and the shell backend.
//!
//! Wave H, P0.

pub mod base64;
pub mod chunk;
pub mod frame;
pub mod git;
pub mod predicates;
pub mod read;
pub mod shell_kind;
pub mod write;

pub use frame::{Frame, FrameError, ParsedSection, Script, StderrMode};
pub use shell_kind::ShellKind;

/// The cap on what one shell call may return, in bytes (the same number as the app's
/// `MAX_DIFF_READ_BYTES`). A section bigger than this is an error, never a silent truncation.
pub const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

/// Quotes `s` as one POSIX single-quoted word.
pub fn sh_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
