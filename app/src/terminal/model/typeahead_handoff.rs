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
        /// Whether the deferred submit has run (after the report arrived, or its deadline).
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

/// What the view does when the report deadline of a request expires.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeadlineOutcome {
    /// The deadline belongs to an earlier request, or the report already arrived.
    Stale,
    /// No report and no Enter: the editor simply keeps input.
    ReleaseEditor,
    /// Run the deferred Enter without the report; a late report will be stashed.
    SubmitWithoutReport,
    /// The deferred Enter ran but wrote no command. Hand the held keys back to the editor.
    GiveUp { held: Vec<u8> },
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
    /// Identifies the latest request, so a report deadline armed for an earlier one is void.
    request_generation: u64,
    /// The request whose report deadline the view has not armed yet.
    deadline_to_arm: Option<u64>,
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
        self.deadline_to_arm = None;
        held
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
                        ActiveDecision::Drain
                    }
                } else if !can_request {
                    self.state = HandoffState::EditorOwned;
                    ActiveDecision::Committed
                } else if no_line_queued && at_prompt && self.writes_in_flight == 0 {
                    self.state = HandoffState::AwaitingReport;
                    self.request_generation = self.request_generation.wrapping_add(1);
                    self.deadline_to_arm = Some(self.request_generation);
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
        match &mut self.state {
            HandoffState::PtyOwned {
                wrote_user_bytes,
                enters_written,
                ..
            } => {
                let (enters, dirty) = scan_line_terminators(bytes, *wrote_user_bytes);
                *enters_written = enters_written.saturating_add(enters);
                *wrote_user_bytes = dirty;
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
    /// answered. Returns whether the counts were reset.
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
                self.writes_in_flight = 0;
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

    /// Returns the request whose report deadline the view has to arm, once.
    pub fn take_report_deadline_to_arm(&mut self) -> Option<u64> {
        self.deadline_to_arm.take()
    }

    /// The report deadline armed for `request_generation` expired.
    pub fn on_report_deadline(&mut self, request_generation: u64) -> DeadlineOutcome {
        if request_generation != self.request_generation {
            return DeadlineOutcome::Stale;
        }
        match self.owner() {
            Owner::AwaitingReport => {
                self.state = HandoffState::EditorOwned;
                DeadlineOutcome::ReleaseEditor
            }
            Owner::Hold { released: false } => {
                self.mark_released();
                DeadlineOutcome::SubmitWithoutReport
            }
            Owner::Hold { released: true } => match mem::take(&mut self.state) {
                HandoffState::HoldAfterEnter { held, .. } => DeadlineOutcome::GiveUp { held },
                other => {
                    self.state = other;
                    DeadlineOutcome::Stale
                }
            },
            Owner::Editor | Owner::Pty { .. } => DeadlineOutcome::Stale,
        }
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

    fn mark_released(&mut self) {
        if let HandoffState::HoldAfterEnter { released, .. } = &mut self.state {
            *released = true;
        }
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
