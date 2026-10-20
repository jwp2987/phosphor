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
/// The longest head searched for junk ahead of residue.
const MAX_SPLIT_SEARCH: usize = 1024;
const BRACKETED_PASTE_MARKERS: [&str; 2] = ["\u{1b}[200~", "\u{1b}[201~"];
const INPUT_REPORT_REQUESTS: [&[u8]; 2] = [b"\x1bi", b"\x1b1"];
/// PowerShell's kill-whole-line (`Alt+2`, BackwardDeleteLine), what its commands are written with.
const POWERSHELL_KILL_LINE: &str = "\u{1b}2";

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
    /// The line was emptied by a write (Ctrl-K Ctrl-U, or Enter) but nothing confirmed it is
    /// empty: a shell can put text back (`histverify`, `print -z`, `read -e -i`) and a vi-mode
    /// Ctrl-U does not kill the whole line. A command starting (preexec) or a report that is
    /// exactly what was written confirms it.
    unconfirmed: bool,
    /// An Enter was written since the line was last emptied, so anything written after it
    /// belongs to a line the ledger no longer models.
    after_enter: bool,
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

    #[cfg(test)]
    fn tracked_len(&self) -> usize {
        self.user_text.len()
    }

    /// Forgets replies that were recorded when generated but never written to the pty (a stale
    /// reply the event loop dropped, #829), so they cannot explain text later.
    pub fn forget_replies(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let mut start = 0;
        let mut pieces = Vec::new();
        for (i, ch) in text.char_indices() {
            if ch == '\u{1b}' && i > start {
                pieces.push(&text[start..i]);
                start = i;
            }
        }
        pieces.push(&text[start..]);
        for piece in pieces.into_iter().filter(|p| !p.is_empty()) {
            if let Some(index) = self.replies.iter().rposition(|r| r == piece) {
                self.replies.remove(index);
            }
        }
    }

    /// Records one write to the pty, whatever its origin (typed keys, a released hold, a
    /// command, an agent's input, a synced pane).
    pub fn note_input(&mut self, bytes: &[u8]) {
        if INPUT_REPORT_REQUESTS.contains(&bytes) {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.user_untracked = true;
            return;
        };
        let mut skip_until = 0;
        for (index, ch) in text.char_indices() {
            if index < skip_until {
                continue;
            }
            if ch == '\u{1b}' && text[index..].starts_with(POWERSHELL_KILL_LINE) {
                skip_until = index + POWERSHELL_KILL_LINE.len();
                self.clear_line();
                self.unconfirmed = true;
                continue;
            }
            // Bracketed paste markers only frame the text between them (commands are written
            // that way); what is pasted lands in the line like typed text.
            if ch == '\u{1b}'
                && let Some(marker) = BRACKETED_PASTE_MARKERS
                    .iter()
                    .find(|m| text[index..].starts_with(**m))
            {
                skip_until = index + marker.len();
                continue;
            }
            let after_kill_forward =
                std::mem::replace(&mut self.after_kill_forward, ch == '\u{0b}');
            // A Ctrl-K not followed by Ctrl-U edited the line at an unknown cursor position.
            if after_kill_forward && ch != '\u{15}' {
                self.user_untracked = true;
            }
            match ch {
                // Enter runs the line, or a shell puts it back; either way the line is not
                // known to be empty until a command starts.
                '\r' | '\n' => {
                    self.user_text.clear();
                    self.unconfirmed = true;
                    self.after_enter = true;
                }
                // Resolved by the next character.
                '\u{0b}' => {}
                // Ctrl-K then Ctrl-U (the prefix every command is written with) kills the
                // line, which recovers tracking but must be confirmed.
                '\u{15}' if after_kill_forward => {
                    self.clear_line();
                    self.unconfirmed = true;
                }
                // Ctrl-P is what the bootstrap binds to kill-whole-line, and what every command
                // is written with ahead of its text.
                '\u{10}' => {
                    self.clear_line();
                    self.unconfirmed = true;
                }
                // Backspace at the end of the tracked text (fish commands end with space, backspace).
                '\u{7f}' | '\u{8}' if !self.after_enter => {
                    if !self.user_untracked {
                        self.user_text.pop();
                    }
                }
                c if c.is_control() => self.user_untracked = true,
                _ if self.after_enter => self.user_untracked = true,
                c if !self.user_untracked => {
                    if self.user_text.len() >= MAX_USER_BYTES {
                        // Too much to model; stop holding on to it.
                        self.user_untracked = true;
                        self.user_text.clear();
                    } else {
                        self.user_text.push(c);
                    }
                }
                _ => {}
            }
        }
    }

    fn clear_line(&mut self) {
        self.user_text.clear();
        self.user_untracked = false;
        self.after_kill_forward = false;
        self.unconfirmed = false;
        self.after_enter = false;
    }

    /// Whether every byte that could be in the shell's line is accounted for.
    fn line_is_known(&self) -> bool {
        !self.user_untracked && !self.unconfirmed && !self.after_kill_forward
    }

    /// A command started. Replies from before it were read (or killed) at the prompt.
    pub fn on_command_start(&mut self) {
        self.replies.clear();
        // The line was consumed by the command that started, so it is empty again.
        if !self.user_untracked && !self.after_kill_forward {
            self.unconfirmed = false;
            self.after_enter = false;
        }
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
        self.clear_line();
    }

    /// The tail of `report` after the residue, or `None` when there is nothing to drop.
    fn strip_residue<'a>(&self, report: &'a str) -> Option<&'a str> {
        if self.replies.is_empty() || !self.line_is_known() || report.is_empty() {
            return None;
        }
        let head_len = report.strip_suffix(self.user_text.as_str())?.len();
        if head_len == 0 {
            return None;
        }
        let head = &report[..head_len];
        // Residue alone, or (for a device control string reply) junk ahead of residue that
        // contains a real stretch of the payload.
        let accepted = self.tails_explain(head).is_some()
            || head.len() <= MAX_SPLIT_SEARCH
                && self.replies.iter().any(|r| r.starts_with("\u{1b}P"))
                && head
                    .char_indices()
                    .skip(1)
                    .any(|(i, _)| self.tails_explain(&head[i..]) == Some(true));
        accepted.then(|| &report[head_len..])
    }

    /// Whether `head` is spelled entirely by the tails of replies, in order: what a line editor
    /// leaves behind after it swallowed the escape prefix of each. `Some(true)` when that
    /// included a stretch of at least two characters of a device control string reply's payload
    /// (`>|Phosphor(`...), which is what zle's `ESC P` leaves.
    fn tails_explain(&self, head: &str) -> Option<bool> {
        let mut rest = head;
        let mut dcs_payload = false;
        for reply in &self.replies {
            if rest.is_empty() {
                break;
            }
            // The longest tail of this reply that starts what is left; a reply the shell
            // consumed whole leaves nothing, so it may match nothing.
            if let Some(len) = reply
                .char_indices()
                .find_map(|(i, _)| residue_len(rest, &reply[i..]))
            {
                dcs_payload |= reply.starts_with("\u{1b}P") && rest[..len].chars().count() >= 2;
                rest = &rest[len..];
            }
        }
        rest.is_empty().then_some(dcs_payload)
    }
}

/// How much of the start of `rest` is what a line editor left of `tail`, a tail of a reply, or
/// `None`. The tail may be there verbatim, or without the control characters a line editor
/// consumes (the ESC that introduces a sequence is gone with its prefix, and the BEL that
/// terminates an OSC reply never reaches the line), or, for a reply ended by ST (`ESC \`), without
/// the whole terminator.
fn residue_len(rest: &str, tail: &str) -> Option<usize> {
    if rest.starts_with(tail) {
        return Some(tail.len());
    }
    let plain: String = tail.chars().filter(|c| !c.is_control()).collect();
    let without_st: String = tail
        .strip_suffix("\u{1b}\\")
        .map(|t| t.chars().filter(|c| !c.is_control()).collect())
        .unwrap_or_default();
    [plain, without_st]
        .into_iter()
        .find(|candidate| !candidate.is_empty() && rest.starts_with(candidate.as_str()))
        .map(|candidate| candidate.len())
}

#[cfg(test)]
#[path = "reply_ledger_tests.rs"]
mod tests;
