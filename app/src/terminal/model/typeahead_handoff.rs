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

/// How long after a command was written (or a prompt or preexec seen) an Enter the user types
/// while the pty owns input may still be a line typed at a prompt the app has not noticed yet
/// (#933). The shell logs its precmd before the app has processed it, so a line typed and
/// Entered right after a command finished reaches the pty while the app still counts the command
/// as running; written as typeahead it would run as a shell command, unclassified. Such an Enter
/// is held until the prompt arrives, then routed like an Enter in the editor; if no prompt comes
/// within this window the command is running and the Enter is written as typeahead.
pub const YOUNG_ENTER_WINDOW: Duration = Duration::from_millis(600);

/// The longest a young line's deferred Enter waits for the verdict of its input-type detection
/// (#933). A verdict that has not come by then means the classifier is not going to answer; the
/// text stays in the editor, unsubmitted.
pub const YOUNG_DETECTION_CAP: Duration = Duration::from_secs(5);

/// What the view does with the deferred Enter of a young line whose text must be classified
/// before it is submitted (#933). It is never submitted on an inherited or default input type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoungSubmit {
    /// The verdict for the exact buffer text has landed: run the Enter.
    Submit,
    /// Keep waiting for the verdict.
    Wait,
    /// The classifier cannot answer: keep the text in the editor and let the user press Enter.
    KeepInEditor,
}

/// Decides [`YoungSubmit`]. `can_run` is whether a classification can run at all;
/// `verdict_pending` whether the verdict for the buffer has not landed yet.
pub fn young_submit_decision(
    can_run: bool,
    verdict_pending: bool,
    waited: Duration,
) -> YoungSubmit {
    if !can_run {
        YoungSubmit::KeepInEditor
    } else if !verdict_pending {
        YoungSubmit::Submit
    } else if waited >= YOUNG_DETECTION_CAP {
        YoungSubmit::KeepInEditor
    } else {
        YoungSubmit::Wait
    }
}

/// Control bytes that signal or end input (^C, ^Z, ^\\, ^D). They are never held: they drop a
/// held Enter and are written at once.
fn is_signal_byte(byte: &u8) -> bool {
    matches!(byte, 0x03 | 0x04 | 0x1a | 0x1c)
}

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
    /// Insert it, then run the Enter that was deferred while it was outstanding, as a shell
    /// command (shell typeahead).
    InsertThenSubmit,
    /// As [`Self::InsertThenSubmit`], for a line the user began typing right after a command,
    /// before the app knew the shell was back at its prompt (#933): the Enter is routed like any
    /// Enter in the editor.
    InsertThenSubmitYoung,
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
    /// Prompts the model has seen whose precmd event the terminal view has not handled yet.
    /// The view's handling of a prompt reinitializes the input editor (the finished block's
    /// `BlockCompleted`), wiping what the editor holds. Giving the editor input before that has
    /// run lets it take keys that the reinitialization then erases (#934), so no request is
    /// written, and the editor does not take over, until this is zero.
    precmds_pending_view: u32,
    /// Whether a terminal view is attached and handles each prompt's precmd event, so
    /// `precmds_pending_view` is worth counting. Without one (a controller driven on its own)
    /// nothing would ever clear it.
    tracks_view_precmds: bool,
    /// The last command written, prompt or preexec seen (see [`YOUNG_ENTER_WINDOW`]).
    last_activity_at: Option<Instant>,
    /// The user's Enter, and every key typed after it, held while the pty owns input and the
    /// command that was written may already have finished (#933). Nothing in it was written.
    young_hold: Option<Vec<u8>>,
    /// A hold was just created, so the view must arm its expiry timer.
    young_hold_started: bool,
    /// Identifies the current hold; a timer armed for an earlier one must not release it.
    young_hold_id: u64,
    /// Whether the line the user is typing began within [`YOUNG_ENTER_WINDOW`] of the last
    /// activity, i.e. before the app could know the shell was at its prompt (#933).
    young_line: bool,
    /// Whether the Enter deferred behind the report ends such a line.
    deferred_young: bool,
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
        ) || self.holds_young_enter_behind_request()
    }

    /// The request is outstanding and the user's Enter is held behind it (#933): keys typed now
    /// must queue behind that Enter, not reach the editor ahead of it, so the terminal keeps
    /// routing them (into the hold).
    fn holds_young_enter_behind_request(&self) -> bool {
        self.state == HandoffState::AwaitingReport && self.young_hold.is_some()
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

    /// Whether text the editor receives now will be followed by the shell's report in front of
    /// it (or never reaches the editor at all), i.e. the editor cannot know the quote or bracket
    /// context of what it is given (#949). The editor must insert typed symbols literally: an
    /// auto-inserted closer would pair with an opener that is in the report, not the editor.
    pub fn editor_text_precedes_report(&self) -> bool {
        !matches!(self.state, HandoffState::EditorOwned)
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
        // An Enter held behind the request follows the command as typeahead (#933).
        let pending_hold = self.young_hold.take();
        self.young_line = false;
        self.deferred_young = false;
        let (held, enters_before, precmds_seen) = match mem::take(&mut self.state) {
            HandoffState::HoldAfterEnter { held, .. } => (held, 0, 0),
            // Lines the user already queued are still ahead of this command in the tty.
            HandoffState::PtyOwned {
                enters_written,
                precmds_seen,
                ..
            } => (
                pending_hold.unwrap_or_default(),
                enters_written,
                precmds_seen,
            ),
            HandoffState::EditorOwned | HandoffState::AwaitingReport => (Vec::new(), 0, 0),
        };
        self.last_activity_at = Some(Instant::now());
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
        // Its kill prefix ends the line a held Enter belongs to.
        self.young_hold = None;
        self.young_line = false;
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

    /// A prompt was applied to the model; the view has yet to handle its precmd event (model
    /// thread). See [`Self::precmds_pending_view`](TypeaheadHandoff).
    pub fn note_precmd_pending_view(&mut self) {
        if !self.tracks_view_precmds {
            return;
        }
        self.precmds_pending_view = self.precmds_pending_view.saturating_add(1);
    }

    /// A terminal view is attached and will call [`Self::on_precmd_viewed`] for every prompt.
    pub fn track_view_precmds(&mut self) {
        self.tracks_view_precmds = true;
    }

    /// The view handled a precmd event, after reinitializing the editor for the finished block
    /// (main thread).
    pub fn on_precmd_viewed(&mut self) {
        self.precmds_pending_view = self.precmds_pending_view.saturating_sub(1);
    }

    /// The shell emitted a precmd hook (model thread).
    pub fn on_precmd(&mut self) {
        self.activity_generation = self.activity_generation.wrapping_add(1);
        self.last_activity_at = Some(Instant::now());
        if let HandoffState::PtyOwned { precmds_seen, .. } = &mut self.state {
            *precmds_seen = precmds_seen.saturating_add(1);
        }
    }

    /// The shell emitted a preexec hook (model thread). `shell_started` is whether the shell
    /// started this command itself (a typeahead line), rather than the app writing it. Returns
    /// whether the pty took over input.
    pub fn on_preexec(&mut self, shell_started: bool) -> bool {
        self.activity_generation = self.activity_generation.wrapping_add(1);
        self.last_activity_at = Some(Instant::now());
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
                if no_line_queued && self.precmds_pending_view > 0 {
                    // The editor has yet to be reinitialized for the finished block, which
                    // would erase whatever it took meanwhile. Keys keep going to the pty; the
                    // view's handling of the prompt decides again (#934).
                    return ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    };
                }
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
                    if self.young_hold.is_some() {
                        // The hold's expiry writes the Enter; nothing can ask the shell for its line.
                        return ActiveDecision::Drain;
                    }
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
        // A signal or EOF byte is never held: it drops a held Enter and goes out at once.
        if bytes.iter().any(is_signal_byte) {
            self.young_hold = None;
            return self.route_user_bytes_unheld(bytes, reaches_shell_line);
        }
        self.drop_stale_hold();
        // #933: keys typed behind a held Enter stay behind it, until the report brings the
        // line back and the Enter is run (or until the hold expires). Keys the editor takes
        // while the request is outstanding do not pass through here and can land ahead of the
        // held Enter; that window is the report's latency.
        if let Some(hold) = &mut self.young_hold {
            hold.extend_from_slice(bytes);
            return UserBytesRoute::Held;
        }
        if self.should_hold_young_enter(bytes, reaches_shell_line, Instant::now()) {
            self.young_hold = Some(bytes.to_vec());
            self.young_hold_id = self.young_hold_id.wrapping_add(1);
            self.young_hold_started = true;
            return UserBytesRoute::Held;
        }
        self.route_user_bytes_unheld(bytes, reaches_shell_line)
    }

    /// A hold only lives in `PtyOwned` and `AwaitingReport`; any other state has run, flushed
    /// or dropped it.
    fn drop_stale_hold(&mut self) {
        if !matches!(
            self.state,
            HandoffState::PtyOwned { .. } | HandoffState::AwaitingReport
        ) {
            self.young_hold = None;
        }
    }

    /// Whether `bytes` are an Enter the user typed on a line that has content, while the only
    /// line the shell has not answered is the command that was just written, and that command is
    /// younger than [`YOUNG_ENTER_WINDOW`] (see there).
    fn should_hold_young_enter(
        &self,
        bytes: &[u8],
        reaches_shell_line: bool,
        now: Instant,
    ) -> bool {
        matches!(
            self.state,
            HandoffState::PtyOwned {
                wrote_user_bytes: true,
                enters_written,
                precmds_seen,
            } if reaches_shell_line
                // No line queued behind the one just written: the shell is running it, or is
                // back at its prompt (`precmds_seen` caught up) with the line still waiting for
                // the request that hands it to the editor.
                && enters_written <= precmds_seen.saturating_add(1)
                && matches!(bytes.first(), Some(b'\r' | b'\n'))
                && (self.young_line
                    || self.last_activity_at.is_some_and(|at| {
                        now.saturating_duration_since(at) < YOUNG_ENTER_WINDOW
                    }))
        )
    }

    /// Makes the last command look older than [`YOUNG_ENTER_WINDOW`], for tests that model
    /// typeahead for a command that has been running a while.
    #[cfg(test)]
    pub(crate) fn age_last_activity_for_test(&mut self) {
        self.last_activity_at = None;
    }

    /// Whether the Enter deferred behind the report has not run yet: the keys typed since are
    /// held behind it. False once it ran, or was cancelled.
    pub fn deferred_submit_pending(&self) -> bool {
        matches!(
            self.state,
            HandoffState::HoldAfterEnter { released: true, .. }
        )
    }

    /// Whether an Enter is held, for tests.
    #[cfg(test)]
    pub(crate) fn has_young_hold(&self) -> bool {
        self.young_hold.is_some()
    }

    /// Takes the "a hold was just created" flag, returning the hold's id so the view arms its
    /// expiry timer once.
    pub fn take_young_hold_started(&mut self) -> Option<u64> {
        mem::take(&mut self.young_hold_started).then_some(self.young_hold_id)
    }

    /// The window of hold `id` ran out with no prompt: the command is running, so the held Enter
    /// and the keys behind it are typeahead for it. Returns the bytes to write, counted as a
    /// routed user write. `None` when that hold is gone (a later one has another id), or the
    /// request already went out (the report will bring the line back and run the Enter).
    pub fn expire_young_hold(&mut self, id: u64) -> Option<Vec<u8>> {
        self.drop_stale_hold();
        if id != self.young_hold_id || !matches!(self.state, HandoffState::PtyOwned { .. }) {
            return None;
        }
        let bytes = self.young_hold.take()?;
        self.route_user_bytes_unheld(&bytes, true);
        Some(bytes)
    }

    fn route_user_bytes_unheld(
        &mut self,
        bytes: &[u8],
        reaches_shell_line: bool,
    ) -> UserBytesRoute {
        match &mut self.state {
            HandoffState::PtyOwned {
                wrote_user_bytes,
                enters_written,
                ..
            } => {
                let mut wrote_an_enter = false;
                if reaches_shell_line {
                    if !*wrote_user_bytes
                        && !matches!(bytes.first(), Some(b'\r' | b'\n'))
                        && self.last_activity_at.is_some_and(|at| {
                            Instant::now().saturating_duration_since(at) < YOUNG_ENTER_WINDOW
                        })
                    {
                        self.young_line = true;
                    }
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
        self.deferred_young = self.young_line;
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
                // #933: the user's Enter was held behind the request; run it now, in the editor,
                // with the keys typed after it held behind it.
                if let Some(mut held) = self.young_hold.take() {
                    let rest = held.split_off(1);
                    self.state = HandoffState::HoldAfterEnter {
                        held: rest,
                        released: true,
                    };
                    self.young_line = false;
                    return ReportDisposition::InsertThenSubmitYoung;
                }
                self.young_line = false;
                self.state = HandoffState::EditorOwned;
                ReportDisposition::Insert
            }
            Owner::Hold { released: false } => {
                self.mark_released();
                if mem::take(&mut self.deferred_young) {
                    self.young_line = false;
                    ReportDisposition::InsertThenSubmitYoung
                } else {
                    ReportDisposition::InsertThenSubmit
                }
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
        *self = Self {
            tracks_view_precmds: self.tracks_view_precmds,
            ..Self::default()
        };
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
        self.young_hold = None;
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
            // #933: an Enter held behind the request is cancelled with its keys.
            Owner::AwaitingReport => self.young_hold.take().is_some(),
            Owner::Editor | Owner::Pty { .. } => false,
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

/// Whether text released into the editor should force the next Enter to run as a shell command.
/// Empty text inserts nothing, so a flag set for it would stay armed and misroute a later
/// natural-language Enter to the shell.
pub fn forces_shell_submit(text: &str) -> bool {
    !text.is_empty()
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
