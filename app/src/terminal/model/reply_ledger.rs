//! Tells a terminal-query reply that leaked into the shell's line apart from typed text (#921).
//!
//! A reply the app writes to the pty (cursor position report, XTVERSION, device attributes, ...)
//! is read by the shell's line editor when the program that asked has already exited. Readline
//! and zle consume the escape prefix and leave a residue behind -- a bare `R` for `ESC [ 24 ; 1 R`
//! -- which the typeahead report then hands to the input editor as if the user had typed it. The
//! shell cannot strip a bare `R` without eating `Rscript`, but the app wrote both the reply and
//! every byte the user typed, so it can tell them apart exactly: what precedes the user's own
//! bytes in the report, and is spelled by the tail of a reply the app wrote, is that reply's
//! residue.
//!
//! Conservative by construction. Anything not fully explained -- control characters in what the
//! user typed, a report that does not end with it, a head that is not made of reply tails -- is
//! left alone for the shell-side strip to deal with.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;

/// The ledger as the terminal model and the pty event loop share it.
pub type SharedReplyLedger = Arc<Mutex<ReplyLedger>>;

/// Replies kept. A reply is a few bytes; the cap only bounds a program that queries in a loop.
const MAX_REPLIES: usize = 64;
/// User bytes kept; past this the typed text is no longer tracked and nothing is filtered.
const MAX_USER_BYTES: usize = 4096;
/// The shells' input reporting request (`ESC i`). It reports the line; it adds nothing to it.
const INPUT_REPORT_REQUEST: &[u8] = b"\x1bi";

/// Fails safe: the ledger only ever drops text when every byte that could be in the shell's line
/// is accounted for. Every pty write goes through [`ReplyLedger::note_input`], which either
/// appends printable text to what the user typed or marks the line untracked.
#[derive(Debug, Default)]
pub struct ReplyLedger {
    /// Replies written to the pty since the last report or command start, oldest first, one
    /// per escape sequence.
    replies: VecDeque<String>,
    /// The printable text written to the pty since the line was last known to be empty.
    user_text: String,
    /// Something was written that cannot be modelled (a control key, an escape sequence,
    /// invalid UTF-8, too much text), or a report left the line unexplained. Nothing is
    /// filtered until the line is known to be empty again.
    user_untracked: bool,
    /// The previous character written was Ctrl-K; with Ctrl-U it kills the whole line.
    after_kill_forward: bool,
}

impl ReplyLedger {
    /// Records bytes the app queued as a reply to a terminal query. A piece that does not start
    /// a new escape sequence continues the previous reply.
    pub fn note_replies(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(bytes);
        let mut start = 0;
        for (i, ch) in text.char_indices() {
            if ch == '\u{1b}' && i > start {
                self.push_reply(&text[start..i]);
                start = i;
            }
        }
        self.push_reply(&text[start..]);
    }

    fn push_reply(&mut self, piece: &str) {
        if piece.is_empty() {
            return;
        }
        if !piece.starts_with('\u{1b}')
            && let Some(last) = self.replies.back_mut()
        {
            last.push_str(piece);
            return;
        }
        if self.replies.len() == MAX_REPLIES {
            self.replies.pop_front();
        }
        self.replies.push_back(piece.to_owned());
    }

    /// Records one write to the pty, whatever its origin (typed keys, a released hold, a
    /// command, an agent's input, a synced pane).
    pub fn note_input(&mut self, bytes: &[u8]) {
        if bytes == INPUT_REPORT_REQUEST {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.user_untracked = true;
            return;
        };
        for ch in text.chars() {
            let after_kill_forward =
                std::mem::replace(&mut self.after_kill_forward, ch == '\u{0b}');
            // A Ctrl-K not followed by Ctrl-U edited the line at an unknown cursor position.
            if after_kill_forward && ch != '\u{15}' {
                self.user_untracked = true;
            }
            match ch {
                // Enter runs the line: what was in it is gone, and the line starts empty.
                '\r' | '\n' => self.line_is_empty(),
                // Resolved by the next character.
                '\u{0b}' => {}
                // Ctrl-K then Ctrl-U (the prefix every command is written with) kills the line.
                '\u{15}' if after_kill_forward => self.line_is_empty(),
                c if c.is_control() => self.user_untracked = true,
                c => self.user_text.push(c),
            }
        }
        if self.user_text.len() > MAX_USER_BYTES {
            self.user_untracked = true;
        }
    }

    fn line_is_empty(&mut self) {
        self.user_text.clear();
        self.user_untracked = false;
        self.after_kill_forward = false;
    }

    /// A command started. Replies from before it were read (or killed) at the prompt.
    pub fn on_command_start(&mut self) {
        self.replies.clear();
    }

    /// Returns `report` without the residue of replies this app wrote.
    pub fn filter_report(&mut self, report: String) -> String {
        let dropped = self.strip_residue(&report).map(str::to_owned);
        let outcome = match dropped {
            Some(rest) => {
                log::info!(
                    "Dropped leaked terminal-query reply residue from the shell's input report \
                     ({} -> {} bytes)",
                    report.len(),
                    rest.len()
                );
                self.clear();
                return rest;
            }
            None => report,
        };
        // Nothing dropped. The line is accounted for only if the report is exactly what was
        // written; otherwise bytes written while the report was in flight (or consumed by a
        // program) make later reports unexplainable.
        let accounted =
            !self.user_untracked && !self.after_kill_forward && outcome == self.user_text;
        if accounted {
            self.clear();
        } else {
            if !self.replies.is_empty() && !outcome.is_empty() {
                log::info!(
                    "Kept the shell's input report ({} bytes, {} replies written, {} bytes typed, \
                     typed text untracked: {})",
                    outcome.len(),
                    self.replies.len(),
                    self.user_text.len(),
                    self.user_untracked
                );
            }
            self.user_untracked = true;
        }
        outcome
    }

    fn clear(&mut self) {
        self.replies.clear();
        self.line_is_empty();
    }

    /// The tail of `report` after the residue, or `None` when there is nothing to drop.
    fn strip_residue<'a>(&self, report: &'a str) -> Option<&'a str> {
        if self.replies.is_empty()
            || self.user_untracked
            || self.after_kill_forward
            || report.is_empty()
        {
            return None;
        }
        let head_len = report.strip_suffix(self.user_text.as_str())?.len();
        if head_len == 0 {
            return None;
        }
        let head = &report[..head_len];
        (self.is_made_of_reply_tails(head) || self.is_junk_then_reply_tails(head))
            .then(|| &report[head_len..])
    }

    /// Whether `head` is spelled entirely by the tails of replies, in order: what a line editor
    /// leaves behind after it swallowed the escape prefix of each.
    fn is_made_of_reply_tails(&self, head: &str) -> bool {
        let mut rest = head;
        for reply in &self.replies {
            if rest.is_empty() {
                break;
            }
            // The longest tail of this reply that starts what is left; a reply the shell
            // consumed whole leaves nothing, so it may match nothing.
            if let Some(tail) = reply
                .char_indices()
                .map(|(i, _)| &reply[i..])
                .find(|tail| rest.starts_with(tail))
            {
                rest = &rest[tail.len()..];
            }
        }
        rest.is_empty()
    }

    /// Whether `head` is some junk followed by the tails of replies, when a device control
    /// string reply was written. zle binds `ESC P` to `history-search-backward`, so the line
    /// holds a recalled history line (or whatever it did to the text before it) ahead of the
    /// payload; none of it was typed, since every typed byte is accounted for separately.
    fn is_junk_then_reply_tails(&self, head: &str) -> bool {
        const MAX_SPLIT_SEARCH: usize = 1024;
        let has_dcs = self.replies.iter().any(|r| r.starts_with("\u{1b}P"));
        has_dcs
            && head.len() <= MAX_SPLIT_SEARCH
            && head
                .char_indices()
                .skip(1)
                .any(|(i, _)| self.is_made_of_reply_tails(&head[i..]))
    }
}

#[cfg(test)]
#[path = "reply_ledger_tests.rs"]
mod tests;
