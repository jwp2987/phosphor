//! Who owns the user's keystrokes around a command submitted from the input editor: the pty or
//! the editor (`FeatureFlag::TypeaheadHandoff`, #826/#827).
//!
//! From the moment such a command is written until the shell has been asked for its line buffer,
//! every key the user types is written to the pty, where the shell queues it as typeahead. The
//! request (the shell's input reporting sequence, `ESC i`) is written only once no Enter-terminated
//! line can still be queued in the tty, the shell is at its prompt and no routed user write is
//! still on its way to the pty. Writing it is the switch to the editor: no user byte is ever
//! written after a request, so the shell's report -- which also clears the shell's line -- covers
//! every byte the user typed.
//!
//! This module is pure. `TerminalModel` owns one [`TypeaheadHandoff`] under the model lock; the
//! terminal view, the input editor and `PtyController` feed it events and act on its decisions.

use std::mem;
use std::time::Duration;

use instant::Instant;

use crate::ai::agent::AIAgentActionId;
use crate::ai::agent::conversation::AIConversationId;

/// Who owns the user's keystrokes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum HandoffState {
    /// The input editor owns input, and keys are routed by the rules that predate the hand-off.
    #[default]
    EditorOwned,
    /// The pty owns input: every key the user types is written to it.
    PtyOwned {
        /// Whether bytes were written since the last line terminator, i.e. whether the shell's
        /// current line may hold user input.
        wrote_user_bytes: bool,
        /// Line terminators written since this state was entered, including the one that ends
        /// the submitted command.
        enters_written: u32,
        /// Prompts (precmd hooks) seen since this state was entered.
        precmds_seen: u32,
    },
    /// The request was written. The editor owns input and the shell's report is outstanding.
    AwaitingReport,
    /// Enter was pressed while the report was outstanding. Keys typed since are held here, in
    /// order, and follow the deferred command to the pty once it has been written.
    HoldAfterEnter {
        held: Vec<u8>,
        /// Whether the deferred submit has run (after the report arrived).
        released: bool,
    },
}

/// Where a user write goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserBytesRoute {
    /// Write it as before the hand-off existed.
    Write,
    /// Write it, and report it to `PtyController` as a hand-off user write, which is in flight
    /// until `PtyController` has processed it (see [`TypeaheadHandoff::on_user_write_flushed`]).
    WriteCounted,
    /// It was held; nothing is written now.
    Held,
}

/// What `PtyController` does when the shell's line editor becomes active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveDecision {
    /// The hand-off does not own this prompt: request and drain as before the hand-off existed.
    Legacy,
    /// Drain queued writes without a request.
    Drain,
    /// Drain queued writes (the shell's line holds no user bytes) and, if no precmd or preexec
    /// arrives for a while, reset the counts and decide again: an Enter the counts consider
    /// queued may have been consumed by a program (`read`, a password prompt, a subshell's
    /// `exit`) and will never be answered by a prompt.
    DrainAndWait {
        /// The activity generation a stale-count reset is valid for.
        activity_generation: u64,
    },
    /// The editor took over without a request, because no user byte can be in the shell's line.
    /// Drain queued writes.
    Committed,
    /// Write the request ahead of the queued writes, then drain them. The editor owns input from
    /// now on.
    SendRequest,
    /// A request is not safe yet. Queued writes stay queued (they could kill the user's line);
    /// retry when the blocking condition clears, or reset stale counts after a quiet period.
    Wait {
        /// The activity generation a stale-count reset is valid for.
        activity_generation: u64,
    },
}

/// What the view does with a typeahead report from the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportDisposition {
    /// Not a hand-off report: insert it as before the hand-off existed.
    InsertLegacy,
    /// Insert it at the front of the editor; the editor now owns input.
    Insert,
    /// Insert it, then run the Enter that was deferred while it was outstanding.
    InsertThenSubmit,
    /// Leave it stored and insert it when the editor next takes over, so it is not wiped with a
    /// submitted command's text.
    Stash,
}

/// Where keys held behind a deferred Enter go when the submit that the report triggered wrote no
/// command (the report is in, so nothing is outstanding in the tty any more).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeldKeysRelease {
    /// One unfinished line (no Enter): back into the editor, for a submit that stays a shell
    /// command.
    ToEditor(Vec<u8>),
    /// Shell lines (an Enter in them): to the pty as typed. Never into the editor, where the
    /// next Enter would submit them as one multi-line buffer -- to the agent, if natural-language
    /// detection took it for a query (#826 N1).
    ToPty(Vec<u8>),
}

/// What a Ctrl-C written while the pty owns input means for the hand-off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptOutcome {
    /// The pty did not own input.
    NotOwned,
    /// A command was running; it and any queued typeahead lines die, and the next prompt hands
    /// input back to the editor.
    CommandRunning,
    /// The shell was reading a line, which the interrupt discards. Some shells answer with a
    /// prompt cycle, so the caller waits briefly for one before handing input to the editor.
    AtPrompt,
}

/// Copyable summary of [`HandoffState`], so transitions can be decided without borrowing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Editor,
    Pty {
        wrote_user_bytes: bool,
        enters_written: u32,
        precmds_seen: u32,
    },
    AwaitingReport,
    Hold {
        released: bool,
    },
}

/// The hand-off state of one terminal. See the module documentation.
#[derive(Debug, Default)]
pub struct TypeaheadHandoff {
    state: HandoffState,
    /// User writes routed to the pty that `PtyController` has not processed yet.
    writes_in_flight: u32,
    /// Bumped on every precmd and preexec, so a stale-count reset armed before either is void.
    activity_generation: u64,
    /// The agent action (and its conversation) whose command `PtyController` holds behind the
    /// user's typeahead, if the held command is an agent's. Its block is not started yet, so
    /// this is the only place a cancel of that action, or a stop of that conversation, can find
    /// it.
    held_agent_command: Option<HeldAgentCommand>,
    /// When the editor last took input over from the pty.
    editor_took_over_at: Option<Instant>,
    /// A key applied within that window moved focus to the editor itself; the resulting focus
    /// event must not end the window.
    focus_moved_by_takeover_key: bool,
}

/// Identifies an agent's command held behind the user's typeahead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldAgentCommand {
    pub action_id: AIAgentActionId,
    pub conversation_id: AIConversationId,
}

impl TypeaheadHandoff {
    pub fn state(&self) -> &HandoffState {
        &self.state
    }

    /// Whether keys typed on the terminal go to the pty (or are held for it) rather than being
    /// routed by the pre-hand-off rules.
    pub fn routes_keys_to_pty(&self) -> bool {
        matches!(
            self.state,
            HandoffState::PtyOwned { .. } | HandoffState::HoldAfterEnter { .. }
        )
    }

    /// Whether the terminal, rather than the input editor, should hold focus.
    pub fn wants_terminal_focus(&self) -> bool {
        self.routes_keys_to_pty()
    }

    pub fn is_pty_owned(&self) -> bool {
        matches!(self.state, HandoffState::PtyOwned { .. })
    }

    /// Whether the editor owns input, so stored typeahead can be inserted into it.
    pub fn editor_owns_input(&self) -> bool {
        matches!(
            self.state,
            HandoffState::EditorOwned | HandoffState::AwaitingReport
        )
    }

    /// Whether `PtyController` must hold its queued writes: the shell's line may hold user bytes
    /// that a queued command's kill prefix would destroy before they were reported.
    pub fn holds_queued_writes(&self) -> bool {
        matches!(
            self.state,
            HandoffState::PtyOwned {
                wrote_user_bytes: true,
                ..
            }
        )
    }

    /// Whether the pty owns input and some Enter written to it is not answered by a prompt
    /// yet, i.e. the shell may still have user lines to run. A command that starts its own
    /// block when written must wait meanwhile, or the user lines' output would land in it.
    pub fn has_unanswered_lines(&self) -> bool {
        matches!(
            self.owner(),
            Owner::Pty {
                enters_written,
                precmds_seen,
                ..
            } if enters_written > precmds_seen
        )
    }

    pub fn held_agent_command(&self) -> Option<&HeldAgentCommand> {
        self.held_agent_command.as_ref()
    }

    pub fn held_command_action(&self) -> Option<&AIAgentActionId> {
        self.held_agent_command.as_ref().map(|held| &held.action_id)
    }

    pub fn set_held_agent_command(&mut self, held: Option<HeldAgentCommand>) {
        self.held_agent_command = held;
    }

    pub fn writes_in_flight(&self) -> u32 {
        self.writes_in_flight
    }

    fn owner(&self) -> Owner {
        match &self.state {
            HandoffState::EditorOwned => Owner::Editor,
            HandoffState::PtyOwned {
                wrote_user_bytes,
                enters_written,
                precmds_seen,
            } => Owner::Pty {
                wrote_user_bytes: *wrote_user_bytes,
                enters_written: *enters_written,
                precmds_seen: *precmds_seen,
            },
            HandoffState::AwaitingReport => Owner::AwaitingReport,
            HandoffState::HoldAfterEnter { released, .. } => Owner::Hold {
                released: *released,
            },
        }
    }

    /// A command from the input editor was accepted and written (or queued) to the pty. Returns
    /// the keys held since a deferred Enter, which must be written right after the command.
    pub fn on_command_written(&mut self) -> Vec<u8> {
        let (held, enters_before, precmds_seen) = match mem::take(&mut self.state) {
            HandoffState::HoldAfterEnter { held, .. } => (held, 0, 0),
            // Lines the user already queued are still ahead of this command in the tty.
            HandoffState::PtyOwned {
                enters_written,
                precmds_seen,
                ..
            } => (Vec::new(), enters_written, precmds_seen),
            HandoffState::EditorOwned | HandoffState::AwaitingReport => (Vec::new(), 0, 0),
        };
        let (held_enters, wrote_user_bytes) = scan_line_terminators(&held, false);
        self.state = HandoffState::PtyOwned {
            wrote_user_bytes,
            enters_written: enters_before.saturating_add(1).saturating_add(held_enters),
            precmds_seen,
        };
        held
    }

    /// A command the hand-off does not own (a queued-prompt-panel command, an agent command, an
    /// in-band command) was written while the pty owns input. Its Enter is counted, so its
    /// precmd does not make lines the user queued ahead of it look answered. Its kill prefix and
    /// Enter end whatever line it lands on.
    pub fn on_uncounted_command_written(&mut self) {
        if let HandoffState::PtyOwned {
            wrote_user_bytes,
            enters_written,
            ..
        } = &mut self.state
        {
            *enters_written = enters_written.saturating_add(1);
            *wrote_user_bytes = false;
        }
    }

    /// The shell emitted a precmd hook (model thread).
    pub fn on_precmd(&mut self) {
        self.activity_generation = self.activity_generation.wrapping_add(1);
        if let HandoffState::PtyOwned { precmds_seen, .. } = &mut self.state {
            *precmds_seen = precmds_seen.saturating_add(1);
        }
    }

    /// The shell emitted a preexec hook (model thread). `shell_started` is whether the shell
    /// started this command itself (a typeahead line), rather than the app writing it. Returns
    /// whether the pty took over input.
    pub fn on_preexec(&mut self, shell_started: bool) -> bool {
        self.activity_generation = self.activity_generation.wrapping_add(1);
        if shell_started && self.state == HandoffState::EditorOwned {
            self.state = HandoffState::PtyOwned {
                wrote_user_bytes: false,
                enters_written: 1,
                precmds_seen: 0,
            };
            return true;
        }
        false
    }

    /// Hands input back to the editor without a request when nothing the user typed can be in
    /// the shell's line. Called on the main thread once a prompt was seen, after the editor has
    /// been reinitialized for the new block. Returns whether the editor took over.
    pub fn try_commit_at_prompt(&mut self) -> bool {
        match self.owner() {
            Owner::Pty {
                wrote_user_bytes: false,
                enters_written,
                precmds_seen,
            } if precmds_seen >= enters_written => {
                self.state = HandoffState::EditorOwned;
                true
            }
            Owner::Pty { .. } | Owner::Editor | Owner::AwaitingReport | Owner::Hold { .. } => false,
        }
    }

    /// The shell's line editor became active. This is the one place a request is decided, on
    /// the main thread and under the model lock, so it is serialized with every routing
    /// decision. `at_prompt` is whether the lifecycle phase is still at the prompt;
    /// `can_request` is whether the shell supports input reporting.
    pub fn on_line_editor_active(&mut self, at_prompt: bool, can_request: bool) -> ActiveDecision {
        match self.owner() {
            Owner::Editor => ActiveDecision::Legacy,
            Owner::AwaitingReport | Owner::Hold { .. } => ActiveDecision::Drain,
            Owner::Pty {
                wrote_user_bytes,
                enters_written,
                precmds_seen,
            } => {
                let no_line_queued = precmds_seen >= enters_written;
                if !wrote_user_bytes {
                    if no_line_queued {
                        self.state = HandoffState::EditorOwned;
                        ActiveDecision::Committed
                    } else {
                        ActiveDecision::DrainAndWait {
                            activity_generation: self.activity_generation,
                        }
                    }
                } else if !can_request {
                    self.state = HandoffState::EditorOwned;
                    ActiveDecision::Committed
                } else if no_line_queued && at_prompt && self.writes_in_flight == 0 {
                    self.state = HandoffState::AwaitingReport;
                    ActiveDecision::SendRequest
                } else {
                    ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    }
                }
            }
        }
    }

    /// A routed user write. In [`HandoffState::PtyOwned`] it is counted, both as line content
    /// and as in flight; in [`HandoffState::HoldAfterEnter`] it is held.
    pub fn on_user_bytes(&mut self, bytes: &[u8]) -> UserBytesRoute {
        self.route_user_bytes(bytes, true)
    }

    /// A routed user write while a full-screen program (the alternate screen) runs. It reads the
    /// tty raw, so the bytes never reach the shell's line: they are in flight, but neither line
    /// content nor Enters for the shell (vim's `:wq\r` is answered by no prompt).
    pub fn on_user_bytes_to_full_screen_program(&mut self, bytes: &[u8]) -> UserBytesRoute {
        self.route_user_bytes(bytes, false)
    }

    fn route_user_bytes(&mut self, bytes: &[u8], reaches_shell_line: bool) -> UserBytesRoute {
        match &mut self.state {
            HandoffState::PtyOwned {
                wrote_user_bytes,
                enters_written,
                ..
            } => {
                let mut wrote_an_enter = false;
                if reaches_shell_line {
                    let (enters, dirty) = scan_line_terminators(bytes, *wrote_user_bytes);
                    *enters_written = enters_written.saturating_add(enters);
                    *wrote_user_bytes = dirty;
                    wrote_an_enter = enters > 0;
                }
                // An Enter is activity too: a quiet-period reset armed before it must not
                // treat the line it ends as answered (a password typed after the timer was
                // armed could otherwise land in the editor).
                if wrote_an_enter {
                    self.activity_generation = self.activity_generation.wrapping_add(1);
                }
                self.writes_in_flight = self.writes_in_flight.saturating_add(1);
                UserBytesRoute::WriteCounted
            }
            HandoffState::HoldAfterEnter { held, .. } => {
                held.extend_from_slice(bytes);
                UserBytesRoute::Held
            }
            HandoffState::EditorOwned | HandoffState::AwaitingReport => UserBytesRoute::Write,
        }
    }

    /// `PtyController` processed a counted user write. Returns how many are still in flight.
    pub fn on_user_write_flushed(&mut self) -> u32 {
        self.writes_in_flight = self.writes_in_flight.saturating_sub(1);
        self.writes_in_flight
    }

    /// No precmd or preexec arrived for a while after a [`ActiveDecision::Wait`]: a line the
    /// counts still consider queued was consumed by something other than the shell (a program
    /// reading stdin, an empty or continued line). Trust the quiet and treat every line as
    /// answered. The in-flight count is left alone: those writes are real. Returns whether the
    /// counts were reset.
    pub fn reset_stale_counts(&mut self, activity_generation: u64) -> bool {
        if activity_generation != self.activity_generation {
            return false;
        }
        match &mut self.state {
            HandoffState::PtyOwned {
                enters_written,
                precmds_seen,
                ..
            } => {
                *precmds_seen = *enters_written;
                true
            }
            HandoffState::EditorOwned
            | HandoffState::AwaitingReport
            | HandoffState::HoldAfterEnter { .. } => false,
        }
    }

    /// Enter was pressed in the editor. Returns whether the submit must be deferred because a
    /// report is outstanding; keys typed from now on are held.
    pub fn defer_enter(&mut self) -> bool {
        if self.state != HandoffState::AwaitingReport {
            return false;
        }
        self.state = HandoffState::HoldAfterEnter {
            held: Vec::new(),
            released: false,
        };
        true
    }

    /// The shell reported its line buffer.
    pub fn on_report(&mut self) -> ReportDisposition {
        match self.owner() {
            Owner::Editor => ReportDisposition::InsertLegacy,
            Owner::AwaitingReport => {
                self.state = HandoffState::EditorOwned;
                ReportDisposition::Insert
            }
            Owner::Hold { released: false } => {
                self.mark_released();
                ReportDisposition::InsertThenSubmit
            }
            Owner::Hold { released: true } | Owner::Pty { .. } => ReportDisposition::Stash,
        }
    }

    /// The submit run after the report wrote no command: release the keys held behind it.
    /// Returns `None` unless that is the state (keys held, deferred submit already run).
    pub fn release_after_unsubmitted_enter(&mut self) -> Option<HeldKeysRelease> {
        if self.owner() != (Owner::Hold { released: true }) {
            return None;
        }
        match mem::take(&mut self.state) {
            HandoffState::HoldAfterEnter { held, .. } => {
                if held.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
                    Some(HeldKeysRelease::ToPty(held))
                } else {
                    Some(HeldKeysRelease::ToEditor(held))
                }
            }
            other => {
                self.state = other;
                None
            }
        }
    }

    /// The session ended: nothing can answer an outstanding request or run held keys.
    pub fn reset_on_exit(&mut self) {
        *self = Self::default();
    }

    /// Ends the window opened by [`Self::note_editor_took_over`]: focus reached the editor, or
    /// the user clicked or selected on the terminal, so later keys there mean what they always
    /// did.
    pub fn end_editor_takeover_window(&mut self) {
        self.editor_took_over_at = None;
    }

    /// A key the window applied is about to move focus to the editor.
    pub fn note_focus_moved_by_takeover_key(&mut self) {
        self.focus_moved_by_takeover_key = true;
    }

    /// The editor gained focus: ends the window, unless a key the window applied moved focus
    /// there itself.
    pub fn on_editor_focused(&mut self) {
        if !mem::take(&mut self.focus_moved_by_takeover_key) {
            self.end_editor_takeover_window();
        }
    }

    /// Records that the editor just took input over (`now`), so a non-printable key that still
    /// reaches the terminal before focus follows can be applied to the editor.
    pub fn note_editor_took_over(&mut self, now: Instant) {
        self.editor_took_over_at = Some(now);
    }

    /// Whether the editor took input over within `window` before `now`.
    pub fn editor_took_over_within(&self, window: Duration, now: Instant) -> bool {
        self.editor_owns_input()
            && self
                .editor_took_over_at
                .is_some_and(|at| now.saturating_duration_since(at) <= window)
    }

    /// A Ctrl-C was written while the pty owned input. The tty discards queued typeahead and the
    /// shell discards its line, so nothing the user typed survives it.
    pub fn on_interrupt(&mut self) -> InterruptOutcome {
        match self.owner() {
            Owner::Pty {
                enters_written,
                precmds_seen,
                ..
            } => {
                let at_prompt = precmds_seen >= enters_written;
                self.state = HandoffState::PtyOwned {
                    wrote_user_bytes: false,
                    enters_written: if at_prompt {
                        precmds_seen
                    } else {
                        precmds_seen.saturating_add(1)
                    },
                    precmds_seen,
                };
                if at_prompt {
                    InterruptOutcome::AtPrompt
                } else {
                    InterruptOutcome::CommandRunning
                }
            }
            Owner::Editor | Owner::AwaitingReport | Owner::Hold { .. } => {
                InterruptOutcome::NotOwned
            }
        }
    }

    /// Ctrl-C while keys are held behind a deferred Enter cancels both: the deferred submit is
    /// dropped and the held keys with it. No command is running (the deferred one was never
    /// written), so nothing is written to the pty. Returns whether there was anything to cancel.
    pub fn cancel_deferred_enter(&mut self) -> bool {
        match self.owner() {
            // The report is still outstanding; the editor keeps waiting for it.
            Owner::Hold { released: false } => {
                self.state = HandoffState::AwaitingReport;
                true
            }
            // The deferred submit already ran without writing a command.
            Owner::Hold { released: true } => {
                self.state = HandoffState::EditorOwned;
                true
            }
            Owner::Editor | Owner::Pty { .. } | Owner::AwaitingReport => false,
        }
    }

    /// Whether Ctrl-D (EOT) should go to the pty: a command may be running or queued to read it,
    /// or the shell's line holds typed bytes. At the prompt with an empty line, EOT would exit the
    /// shell, so it is not written.
    pub fn ctrl_d_reaches_a_reader(&self) -> bool {
        match self.owner() {
            Owner::Pty {
                wrote_user_bytes,
                enters_written,
                precmds_seen,
            } => wrote_user_bytes || enters_written > precmds_seen,
            Owner::Hold { .. } => true,
            Owner::Editor | Owner::AwaitingReport => false,
        }
    }

    /// Whether the pty owns input at the prompt with user bytes in the shell's line, so only a
    /// request can hand input to the editor.
    pub fn needs_request_at_prompt(&self) -> bool {
        matches!(
            self.owner(),
            Owner::Pty {
                wrote_user_bytes: true,
                enters_written,
                precmds_seen,
            } if precmds_seen >= enters_written
        )
    }

    fn mark_released(&mut self) {
        if let HandoffState::HoldAfterEnter { released, .. } = &mut self.state {
            *released = true;
        }
    }
}

/// The text of keys handed back to the editor, as the shell's line editor would have left it:
/// Backspace (DEL, ^H) erases a character, ^U the line, ^W a word; Enter (`\r`, `\n`) becomes
/// a newline and Tab stays, so a multi-line paste survives; well-formed escape sequences (CSI,
/// SS3, OSC, as sent by arrow and function keys) are dropped whole; any other control character
/// is dropped, and a lone ESC is dropped without taking the character after it.
pub fn text_for_editor(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        i += 1;
        match ch {
            // Backspace erases within the current line only, as the line editor would.
            '\u{7f}' | '\u{8}' => {
                if !out.ends_with('\n') {
                    out.pop();
                }
            }
            // ^U: back to the start of the line.
            '\u{15}' => {
                let line_start = out.rfind('\n').map_or(0, |index| index + 1);
                out.truncate(line_start);
            }
            // ^W: trailing whitespace, then the word before it.
            '\u{17}' => {
                while out.ends_with(|last: char| last == ' ' || last == '\t') {
                    out.pop();
                }
                while out.chars().last().is_some_and(|last| !last.is_whitespace()) {
                    out.pop();
                }
            }
            '\r' | '\n' => out.push('\n'),
            '\t' => out.push('\t'),
            '\u{1b}' => i = skip_escape_sequence(&chars, i),
            ch if ch.is_control() => {}
            ch => out.push(ch),
        }
    }
    out
}

/// Given `chars[start]` is the character after an ESC, returns the index after a well-formed
/// CSI (`ESC [` .. final byte), SS3 (`ESC O` + one character) or OSC (`ESC ]` .. BEL or ST)
/// sequence, or `start` itself when the ESC starts none of them.
fn skip_escape_sequence(chars: &[char], start: usize) -> usize {
    match chars.get(start) {
        Some('[') => {
            let mut index = start + 1;
            while let Some(ch) = chars.get(index) {
                index += 1;
                if ('@'..='~').contains(ch) {
                    return index;
                }
                if !(' '..='?').contains(ch) {
                    // Not a parameter or intermediate byte: not a CSI after all.
                    return start;
                }
            }
            start
        }
        Some('O') => match chars.get(start + 1) {
            Some(ch) if ('@'..='~').contains(ch) => start + 2,
            Some(_) | None => start,
        },
        Some(']') => {
            let mut index = start + 1;
            while let Some(ch) = chars.get(index) {
                index += 1;
                if *ch == '\u{7}' {
                    return index;
                }
                if *ch == '\u{1b}' && chars.get(index) == Some(&'\\') {
                    return index + 1;
                }
            }
            start
        }
        Some(_) | None => start,
    }
}

/// Counts line terminators in `bytes` and returns them with whether the line is still dirty
/// afterwards (bytes follow the last terminator), starting from `dirty`.
fn scan_line_terminators(bytes: &[u8], mut dirty: bool) -> (u32, bool) {
    let mut enters = 0u32;
    for byte in bytes {
        if matches!(byte, b'\r' | b'\n') {
            enters = enters.saturating_add(1);
            dirty = false;
        } else {
            dirty = true;
        }
    }
    (enters, dirty)
}

#[cfg(test)]
#[path = "typeahead_handoff_tests.rs"]
mod tests;
