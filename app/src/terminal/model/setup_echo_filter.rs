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
//! The first line is preceded by ` ^K ^U` (clear the line editor). A cooked tty echoes that too,
//! before the line: the space, `^K` (`ECHOCTL`), then `^U` (the kill character) rubs the three
//! columns out again with `\b \b` each (`ECHOKE`). Linux and macOS echo exactly
//! [`LINE_CLEAR_ECHO`] (captured on both, #1038); without it in the expectation the filter matched
//! the leading space, met `^` and stood down, leaving the whole script on screen.
//!
//! On macOS the tty input queue holds 1024 bytes (`TTYHOG`), so while the shell is not reading,
//! the echo stops part-way through the second line and the rest is written (and echoed by the
//! shell's line editor, not the tty) only once the shell reads. A half-matched line can therefore
//! stay half matched for the whole stall; [`SetupEchoFilter::is_mid_line`] says so.
//!
//! Conservative by construction: a character that does not continue the echo of the line being
//! matched is output like any other. If it arrives while a line is half matched the filter stands
//! down entirely (whatever follows is not the plain echo this expects); before any character of a
//! line matched, foreign output (an rc banner) is passed through and the filter stays armed.
//! The prefix echo is the exception that is matched tentatively: an rc that printed
//! `Loading plugins... ` without a newline puts it after text on the same row, and the spaces in
//! that text look like its start. Characters held back as a possible prefix are handed back
//! (and shown) as soon as they cannot be one.

use std::collections::VecDeque;

/// What a cooked tty (`ECHO ECHOCTL ECHOKE`, the default on Linux and macOS) echoes for the
/// ` ^K ^U` written before the first setup line: the space, `^K` printed as a caret pair, then
/// the kill character rubbing out those three columns.
pub const LINE_CLEAR_ECHO: &str = " ^K\u{8} \u{8}\u{8} \u{8}\u{8} \u{8}";

#[derive(Debug, Default)]
pub struct SetupEchoFilter {
    /// What is still expected after the line-clear prefix: the characters of each written line,
    /// then `\r`, `\n`.
    expected: VecDeque<char>,
    /// Characters of the current line swallowed so far, once past the line-clear prefix. Zero
    /// means no line is half matched.
    matched_in_line: usize,
    /// The line-clear prefix echo still to be found ahead of `expected` ([`LINE_CLEAR_ECHO`]), or
    /// empty. It may follow foreign text on the same row (an rc's `Loading... ` without a
    /// newline), so it is matched tentatively, see [`SetupEchoFilter::swallow`].
    prefix: Vec<char>,
    /// The characters held back as a possible start of `prefix`.
    tentative: Vec<char>,
    /// Characters held back in `tentative` that turned out not to be the echo, in order. The
    /// caller outputs them (see [`SetupEchoFilter::take_unmatched`]).
    unmatched: Vec<char>,
}

impl SetupEchoFilter {
    /// Expects the echo of `line` followed by Enter. Lines are queued in the order written.
    pub fn expect_line(&mut self, line: &str) {
        self.expected.extend(line.chars());
        self.expected.push_back('\r');
        self.expected.push_back('\n');
    }

    /// Expects the tty's echo of the ` ^K ^U` line-clearing prefix ([`LINE_CLEAR_ECHO`]) before
    /// the first line. Call it before [`Self::expect_line`]. Text before the prefix is passed
    /// through, also when it shares characters with it (the space of `Loading plugins`, a
    /// trailing space); once the whole prefix matched, a divergence stands the filter down like
    /// any half-matched line.
    pub fn expect_line_clear_prefix(&mut self) {
        self.prefix = LINE_CLEAR_ECHO.chars().collect();
        self.tentative.clear();
    }

    /// Whether part of a line's echo was swallowed and the rest is still due: the last thing the
    /// tty printed is the setup's own echo, not a prompt of the shell's. A prefix only partly
    /// seen does not count: its first characters may be the shell's own output.
    pub fn is_mid_line(&self) -> bool {
        self.matched_in_line > 0 && self.is_armed()
    }

    /// Stops filtering and forgets what was expected. Characters held back as a possible prefix
    /// are handed back through [`Self::take_unmatched`].
    pub fn disarm(&mut self) {
        self.expected.clear();
        self.matched_in_line = 0;
        self.prefix.clear();
        self.unmatched.append(&mut self.tentative);
    }

    pub fn is_armed(&self) -> bool {
        !self.expected.is_empty()
    }

    /// Characters the filter held back and released as not being the echo, in output order.
    /// They precede the character just passed to [`Self::swallow`]; the caller outputs them
    /// first, then that character if it was not swallowed.
    pub fn take_unmatched(&mut self) -> Vec<char> {
        std::mem::take(&mut self.unmatched)
    }

    /// Returns whether `c` (a printable character, `\r`, `\n` or a backspace `\u{8}`) is the
    /// expected echo and must be swallowed. Afterwards [`Self::take_unmatched`] may hold
    /// earlier characters to output before it.
    pub fn swallow(&mut self, c: char) -> bool {
        if !self.prefix.is_empty() {
            return self.swallow_prefix(c);
        }
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

    /// Matches `c` against the line-clear prefix. Held-back characters that can no longer start
    /// the prefix are released, oldest first, until what is held is again a start of it.
    fn swallow_prefix(&mut self, c: char) -> bool {
        self.tentative.push(c);
        while !self.prefix.starts_with(&self.tentative) {
            self.unmatched.push(self.tentative.remove(0));
        }
        if self.tentative.is_empty() {
            // `c` itself is not the echo: it is output by the caller, not from `unmatched`.
            self.unmatched.pop();
            return false;
        }
        if self.tentative.len() == self.prefix.len() {
            // The whole prefix echo arrived: from here on it is part of the first line.
            self.matched_in_line = self.prefix.len();
            self.prefix.clear();
            self.tentative.clear();
        }
        true
    }
}

#[cfg(test)]
#[path = "setup_echo_filter_tests.rs"]
mod tests;
