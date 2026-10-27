//! The tty's echo of keys the user typed ahead into the pty (#1033).
//!
//! Under the typeahead hand-off, keys typed after a command was submitted from the input editor
//! are written to the pty straight away, where the shell picks them up at its next prompt. While
//! a command runs -- and in the moments around it, from the shell accepting the line until its
//! preexec hook, and from the command's exit until the line editor takes the terminal again --
//! the tty is in cooked mode with `ECHO` on, so the kernel echoes each of those keys into the
//! output stream the moment it is written. That echo lands wherever the stream is at the time:
//! after the command line in the header, at the end of the command's output, or in the early
//! output that becomes the next block's header. The shell then re-reads the keys at its prompt,
//! so the same text shows up a second time where it belongs. A regular terminal shows both
//! copies; a block list should not.
//!
//! The echo cannot be recognised from the output text alone: program output can contain the
//! same characters. [`TypedEcho`] therefore records the keys the app itself wrote to the pty as
//! typeahead, in order, and claims printed characters as their echo only when
//! - they are the next character whose echo is still due,
//! - the shell's line editor is not active (it re-draws its line itself, in raw mode, while
//!   bracketed paste is enabled; only the tty echoes while it is off), and
//! - they start a line, or directly continue an echo run in the same grid.
//!
//! A run is only removed once the keys are known to be the shell's and not the running
//! program's (see [`TypedEcho::take_runs_of_unterminated_line`] and friends), and only where the
//! grid still holds exactly the run's text at the recorded place (see
//! `GridHandler::strip_echo_run`).

use std::collections::VecDeque;

use unicode_width::UnicodeWidthChar as _;

use super::block::BlockId;

#[cfg(test)]
#[path = "typed_echo_tests.rs"]
mod tests;

/// Bound on every queue kept here: typeahead is short, and a long paste needs no echo tracking.
const MAX_TRACKED: usize = 1024;

/// How many due characters a new echo run may skip (see [`TypedEcho::classify`]).
const MAX_SKIPPED: usize = 32;

/// Which grid of a block an echo run was printed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EchoGrid {
    /// The prompt-and-command (header) grid.
    Command,
    /// The output grid (also the only grid of a background block).
    Output,
}

/// A position in a grid: (absolute row, column), the row counting history rows.
pub type EchoPoint = (usize, usize);

/// Where an echo run sits in its grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EchoSpan {
    /// The cell holding the run's first character.
    pub start: EchoPoint,
    /// The point where the character after the run's last one would be printed.
    pub end: EchoPoint,
}

/// A run of consecutive cells holding the echo of consecutive typed characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EchoRun {
    pub block: BlockId,
    pub grid: EchoGrid,
    pub text: String,
    pub first_seq: u64,
    pub last_seq: u64,
    /// The run's place in its grid. `None` once its cells were copied to another grid (the early
    /// output that becomes a shell-run command's header); it can then only be removed as the
    /// very end of that grid.
    pub span: Option<EchoSpan>,
    /// Whether it was printed after the shell accepted a line and before that line's command
    /// exited: the window between the line editor letting go of the terminal and the preexec
    /// hook, in which keys may still be for the command about to run.
    pub after_accept: bool,
}

/// The decision, taken before a character is printed, that it is the echo of a typed key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EchoMatch {
    /// How many due characters it accounts for: this one and any skipped before it.
    pops: usize,
    seq: u64,
    continues_run: bool,
}

/// Records the user's typed-ahead keys and the cells their tty echo was printed in.
#[derive(Debug, Default)]
pub struct TypedEcho {
    next_seq: u64,
    /// Typed characters whose echo has not been seen yet, oldest first. An Enter is `'\r'`.
    due: VecDeque<(u64, char)>,
    /// Typed characters not yet attributed to a shell command line, oldest first.
    typed: VecDeque<(u64, char)>,
    /// The sequence number the line being typed starts at (after the last Enter).
    line_start: u64,
    /// Whether the shell's line editor holds the terminal (bracketed paste is enabled). The tty
    /// does not echo then, and whatever is printed is the line editor's own drawing.
    line_editor_active: bool,
    /// Whether the shell was ever seen enabling bracketed paste. Without that marker the line
    /// editor's drawing cannot be told from the tty's echo, so nothing is claimed.
    line_editor_marker_seen: bool,
    /// Whether the line editor let go of the terminal (accepted a line) since the last command
    /// exited.
    accepted: bool,
    runs: VecDeque<EchoRun>,
}

impl TypedEcho {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records bytes the app wrote to the pty as the user's typeahead. Printable single-width
    /// characters and Enters are tracked; anything else (editing keys, escape sequences, control
    /// characters) changes what the tty echoes and what the line holds, so tracking restarts
    /// after it.
    pub fn record_typed(&mut self, bytes: &[u8]) {
        for c in String::from_utf8_lossy(bytes).chars() {
            if c == '\r' || c == '\n' {
                self.push_typed('\r');
                self.line_start = self.next_seq;
            } else if !c.is_control() && c.width() == Some(1) && c != char::REPLACEMENT_CHARACTER {
                self.push_typed(c);
            } else {
                self.forget_typed();
            }
        }
    }

    fn push_typed(&mut self, c: char) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.due.push_back((seq, c));
        self.typed.push_back((seq, c));
        while self.due.len() > MAX_TRACKED {
            self.due.pop_front();
        }
        while self.typed.len() > MAX_TRACKED {
            self.typed.pop_front();
        }
    }

    /// Stops expecting any echo and forgets the typed text. Recorded runs stay, but none of them
    /// can become removable through text typed before this point any more.
    pub fn forget_typed(&mut self) {
        self.due.clear();
        self.typed.clear();
        self.line_start = self.next_seq;
    }

    /// The shell enabled (`true`) or disabled bracketed paste.
    pub fn set_line_editor_active(&mut self, active: bool) {
        self.line_editor_active = active;
        if active {
            self.line_editor_marker_seen = true;
        } else {
            self.accepted = true;
        }
    }

    /// The running command exited: until the line editor accepts the next line, no command can
    /// read what is typed.
    pub fn on_command_finished(&mut self) {
        self.accepted = false;
    }

    /// A cheap check, run for every printed character, of whether [`Self::classify`] could claim
    /// `c` at all.
    pub fn may_match(&self, c: char) -> bool {
        !self.line_editor_active
            && self.line_editor_marker_seen
            && self
                .due
                .iter()
                .take(MAX_SKIPPED + 1)
                .any(|&(_, due)| due == c)
    }

    /// Whether `c`, about to be printed at `point` in `target`'s grid, is the echo of a due
    /// typed character. `target` is `None` when the grid belongs to a block that does not exist
    /// yet (a background block created by this very character). `at_line_start` is whether
    /// `point` is the first column of a line (not of a soft-wrapped continuation).
    ///
    /// A run continues only with the very next due character, right after the run in the same
    /// grid. A new run starts only at a line start, and may skip due characters: keys that
    /// reached the tty while the shell's line editor read it raw are never echoed (the app
    /// cannot place its writes exactly against the shell's mode switches), and an Enter's echo
    /// is the line break itself. Echo comes in typing order, so skipped keys can no longer be
    /// echoed after this one.
    pub fn classify(
        &self,
        c: char,
        target: Option<(BlockId, EchoGrid)>,
        point: EchoPoint,
        at_line_start: bool,
    ) -> Option<EchoMatch> {
        if self.line_editor_active || !self.line_editor_marker_seen {
            return None;
        }
        let &(first_seq, first) = self.due.front()?;
        if first == c {
            let continues_run = self.runs.back().is_some_and(|run| {
                target
                    .as_ref()
                    .is_some_and(|(block, grid)| &run.block == block && run.grid == *grid)
                    && run.last_seq + 1 == first_seq
                    && run.span.is_some_and(|span| span.end == point)
            });
            if continues_run || at_line_start {
                return Some(EchoMatch {
                    pops: 1,
                    seq: first_seq,
                    continues_run,
                });
            }
            return None;
        }
        if !at_line_start {
            return None;
        }
        let (index, &(seq, _)) = self
            .due
            .iter()
            .enumerate()
            .take(MAX_SKIPPED + 1)
            .find(|(_, (_, due))| *due == c)?;
        Some(EchoMatch {
            pops: index + 1,
            seq,
            continues_run: false,
        })
    }

    /// Records that `c` was printed at `point` of `block`'s `grid` as the echo `m` (from
    /// [`Self::classify`]); `next` is where the following character will be printed.
    pub fn commit(
        &mut self,
        m: EchoMatch,
        block: BlockId,
        grid: EchoGrid,
        c: char,
        point: EchoPoint,
        next: EchoPoint,
    ) {
        for _ in 0..m.pops {
            self.due.pop_front();
        }
        if m.continues_run {
            if let Some(run) = self.runs.back_mut() {
                run.text.push(c);
                run.last_seq = m.seq;
                if let Some(span) = &mut run.span {
                    span.end = next;
                }
                return;
            }
        }
        self.runs.push_back(EchoRun {
            block,
            grid,
            text: c.to_string(),
            first_seq: m.seq,
            last_seq: m.seq,
            span: Some(EchoSpan {
                start: point,
                end: next,
            }),
            after_accept: self.accepted,
        });
        while self.runs.len() > MAX_TRACKED {
            self.runs.pop_front();
        }
    }

    /// Takes the runs in `block` that echo the line still being typed (no Enter written after
    /// them). Called when `block`'s command exits: a program in canonical mode only ever reads
    /// whole lines, so it did not consume those keys and the shell gets them.
    pub fn take_runs_of_unterminated_line(&mut self, block: &BlockId) -> Vec<EchoRun> {
        let line_start = self.line_start;
        self.take_runs(|run| &run.block == block && run.first_seq >= line_start)
    }

    /// Takes the runs in `block` printed after a command exited and before the shell accepted
    /// another line. Used for the block of early output: nothing could read those keys but the
    /// shell.
    pub fn take_runs_before_accept(&mut self, block: &BlockId) -> Vec<EchoRun> {
        self.take_runs(|run| &run.block == block && !run.after_accept)
    }

    /// The shell ran `command` from its own line (typed ahead, not written by the editor).
    /// Finds the first typed line that spells it and takes the runs echoing that line; lines
    /// typed before it went to something else and are forgotten. Nothing is taken when no typed
    /// line spells the command.
    pub fn take_runs_of_shell_run_line(&mut self, command: &str) -> Vec<EchoRun> {
        let mut line = String::new();
        let mut line_first: Option<u64> = None;
        let mut found: Option<(u64, u64)> = None;
        for &(seq, c) in &self.typed {
            if c == '\r' {
                if line == command {
                    found = Some((line_first.unwrap_or(seq), seq));
                    break;
                }
                line.clear();
                line_first = None;
            } else {
                line_first.get_or_insert(seq);
                line.push(c);
            }
        }
        let Some((first, enter)) = found else {
            return Vec::new();
        };
        self.typed.retain(|&(seq, _)| seq > enter);
        self.due.retain(|&(seq, _)| seq > enter);
        self.take_runs(|run| run.first_seq >= first && run.last_seq < enter)
    }

    /// The shell reported (and cleared) its line, `reported`. Every key written so far is
    /// accounted for. When the line being typed spells the report, the runs echoing it are
    /// returned; either way all typed text is forgotten.
    pub fn take_runs_of_reported_line(&mut self, reported: &str) -> Vec<EchoRun> {
        let line_start = self.line_start;
        let unterminated: String = self
            .typed
            .iter()
            .filter(|(seq, _)| *seq >= line_start)
            .map(|(_, c)| *c)
            .collect();
        let runs = if !unterminated.is_empty() && unterminated == reported {
            self.take_runs(|run| run.first_seq >= line_start)
        } else {
            Vec::new()
        };
        self.forget_typed();
        runs
    }

    /// The early output block `from` was copied into `to`'s header: its remaining runs now live
    /// at the end of that header, at a place no longer known.
    pub fn move_runs_to_header(&mut self, from: &BlockId, to: BlockId) {
        for run in self.runs.iter_mut().filter(|run| &run.block == from) {
            run.block = to.clone();
            run.grid = EchoGrid::Command;
            run.span = None;
        }
    }

    /// Forgets the runs recorded in `block` (it is gone).
    pub fn forget_block(&mut self, block: &BlockId) {
        self.runs.retain(|run| &run.block != block);
    }

    fn take_runs(&mut self, mut pred: impl FnMut(&EchoRun) -> bool) -> Vec<EchoRun> {
        let mut taken = Vec::new();
        let mut kept = VecDeque::with_capacity(self.runs.len());
        for run in self.runs.drain(..) {
            if pred(&run) {
                taken.push(run);
            } else {
                kept.push_back(run);
            }
        }
        self.runs = kept;
        taken
    }

    #[cfg(test)]
    pub(crate) fn runs(&self) -> impl Iterator<Item = &EchoRun> {
        self.runs.iter()
    }
}
