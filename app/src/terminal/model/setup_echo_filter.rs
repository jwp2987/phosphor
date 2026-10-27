//! Keeps the tty's echo of the typed subshell setup script out of the block (#1038).
//!
//! The app types the setup script (`__hc0=...`, `_wbs=...` lines) into the new shell. Normally the
//! shell reads it within milliseconds and the setup's `InitShell` hook resets the block, so the
//! echoed text is never on screen for long. When the shell is still busy (an rc file that sleeps,
//! a `Password:` prompt, #1014) the script sits in the tty queue and the *kernel* echoes it into
//! the running block, where it stays for as long as the stall lasts.
//!
//! The app knows exactly what it wrote, so it can recognise the echo: the same characters, then
//! the CR LF the tty turns each Enter into. This filter swallows that, and only that.
//!
//! Conservative by construction: a character that does not continue the echo of the line being
//! matched is output like any other. If it arrives while a line is half matched the filter stands
//! down entirely (whatever follows is not the plain echo this expects); before any character of a
//! line matched, foreign output (an rc banner, `^K` from the line-clearing prefix) is passed
//! through and the filter stays armed.

use std::collections::VecDeque;

#[derive(Debug, Default)]
pub struct SetupEchoFilter {
    /// What is still expected: the characters of each written line, then `\r`, `\n`.
    expected: VecDeque<char>,
    /// Characters of the current line swallowed so far. Zero means no line is half matched.
    matched_in_line: usize,
}

impl SetupEchoFilter {
    /// Expects the echo of `line` followed by Enter. Lines are queued in the order written.
    pub fn expect_line(&mut self, line: &str) {
        self.expected.extend(line.chars());
        self.expected.push_back('\r');
        self.expected.push_back('\n');
    }

    /// Stops filtering and forgets what was expected.
    pub fn disarm(&mut self) {
        self.expected.clear();
        self.matched_in_line = 0;
    }

    pub fn is_armed(&self) -> bool {
        !self.expected.is_empty()
    }

    /// Returns whether `c` (a printable character, `\r` or `\n`) is the expected echo and must be
    /// swallowed.
    pub fn swallow(&mut self, c: char) -> bool {
        if self.expected.front() == Some(&c) {
            self.expected.pop_front();
            self.matched_in_line = if c == '\n' {
                0
            } else {
                self.matched_in_line + 1
            };
            return true;
        }
        if self.matched_in_line > 0 {
            self.disarm();
        }
        false
    }
}

#[cfg(test)]
#[path = "setup_echo_filter_tests.rs"]
mod tests;
