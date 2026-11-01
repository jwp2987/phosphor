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

use std::collections::VecDeque;
use std::mem;
use std::sync::Arc;
use std::time::Duration;

use instant::Instant;

use crate::ai::agent::AIAgentActionId;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::input_model::ENTER_VERDICT_BUDGET;

/// How long after a command was written (or a prompt or preexec seen) an Enter the user types
/// while the pty owns input may still be a line typed at a prompt the app has not noticed yet
/// (#933). The shell logs its precmd before the app has processed it, so a line typed and
/// Entered right after a command finished reaches the pty while the app still counts the command
/// as running; written as typeahead it would run as a shell command, unclassified. Such an Enter
/// is held until the prompt arrives, then routed like an Enter in the editor; if no prompt comes
/// within this window the command is running and the Enter is written as typeahead.
pub const YOUNG_ENTER_WINDOW: Duration = Duration::from_millis(600);

/// The shortest a held young Enter waits for the prompt. The window runs from the last activity
/// (command write, precmd, preexec), not from the Enter, so an Enter typed late in the window
/// waits only for what is left of it; this floor is the lag between the shell logging its precmd
/// and the app processing it, which is all the race in [`YOUNG_ENTER_WINDOW`] needs.
pub const YOUNG_ENTER_MIN_HOLD: Duration = Duration::from_millis(150);

/// The shortest a young Enter is held once round trips have been measured: the lag between the
/// shell logging its precmd and the app having processed it on this machine (a pty read, a parse
/// and a main-thread hop), with room for one slow frame.
pub const YOUNG_ENTER_FLOOR: Duration = Duration::from_millis(20);

/// How many round trips the hold's measured bound is the p90 of. A rolling window, so a stalled
/// machine's huge sample ages out instead of inflating the hold for the session.
const RTT_WINDOW: usize = 16;

/// The hold is this many measured round trips (p90). The late precmd of #933 was emitted before
/// the Enter was typed, so it is in flight for at most one trip (one way, plus the shell's own
/// work, which a request's round trip includes); two more trips absorb jitter.
const RTT_HOLD_FACTOR: u32 = 3;

/// Asks the platform whether the shell provably cannot be at its prompt: a plain program such as
/// `cat` owns the foreground, or a program is reading a password (canonical tty, echo off), in
/// the top-level shell or a Phosphorized subshell. Unix only; where nothing is installed, or the
/// answer is unknown (over ssh the local tty says nothing about the remote shell), it is `false`.
#[derive(Clone, Default)]
pub struct ForegroundProbe(Option<Arc<dyn Fn() -> bool + Send + Sync>>);

impl ForegroundProbe {
    pub fn new(probe: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self(Some(probe))
    }

    fn foreground_is_plain_program(&self) -> bool {
        self.0.as_ref().is_some_and(|probe| probe())
    }
}

impl std::fmt::Debug for ForegroundProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "ForegroundProbe(installed)"
        } else {
            "ForegroundProbe(none)"
        })
    }
}

/// On a ConPTY session (#1162): how long the user must have stopped typing before the hand-off
/// asks the shell for a line that holds raw-typed keys. A request (`ESC i`) landing between two
/// keys of a line the user is still typing is written into the middle of that line's raw
/// writes, and ConPTY / MSYS readline lose the line. Well above the 10-25 ms between keys of
/// fast typing, below the stale-count reset (250 ms) that re-runs a waiting decision.
pub const CONPTY_MIDLINE_QUIET: Duration = Duration::from_millis(120);

/// On a ConPTY session (#1162): the longest raw user keys typed between a prompt's precmd and the
/// shell's line editor becoming active are held. Git Bash's readline switches the tty mode,
/// enables bracketed paste and draws the prompt right after the prompt hook (measured: a key
/// written ~6 ms before that start-up output vanished below the app). The line editor's own
/// signal normally ends the hold within ~50 ms; after this long the keys are written anyway.
pub const CONPTY_PROMPT_HOLD_EXPIRY: Duration = Duration::from_millis(150);

/// Environment variable that turns the ConPTY prompt hold off when set to `0` (#1162).
pub const CONPTY_PROMPT_HOLD_ENV: &str = "PHOSPHOR_CONPTY_PROMPT_HOLD";

/// Whether the ConPTY prompt hold is on by default: it is, unless the kill switch says `0`.
pub fn conpty_prompt_hold_env_enabled() -> bool {
    std::env::var(CONPTY_PROMPT_HOLD_ENV).map_or(true, |value| value.trim() != "0")
}

/// The longest a young line's deferred Enter waits for the verdict of its input-type detection
/// (#933). A verdict that has not come by then means the classifier is not going to answer; the
/// Enter then submits on the Enter-time rules without it (#1174), never kept in the editor.
///
/// The same local, generator-free classifier the editor Enter waits [`ENTER_VERDICT_BUDGET`] for
/// (#1158); the young line's parse only starts when the report lands (no debounce head start) and
/// the line is rare, so it gets a few budgets, not the seconds it had before #1158.
pub const YOUNG_DETECTION_CAP: Duration = ENTER_VERDICT_BUDGET.saturating_mul(5);

/// What the view does with the deferred Enter of a young line whose text must be classified
/// before it is submitted (#933). It is never submitted on an inherited or default input type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoungSubmit {
    /// The verdict for the exact buffer text has landed: run the Enter.
    Submit,
    /// Keep waiting for the verdict.
    Wait,
    /// No classifier can run at all (no session): keep the text in the editor and let the user
    /// press Enter.
    KeepInEditor,
    /// A classifier exists but did not answer within [`YOUNG_DETECTION_CAP`] (#1174): drop the
    /// request and submit on the Enter-time rules, exactly as an Enter whose verdict is late
    /// does. An Enter is never swallowed.
    SubmitWithoutVerdict,
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
        YoungSubmit::SubmitWithoutVerdict
    } else {
        YoungSubmit::Wait
    }
}

/// Control bytes that signal or end input (^C, ^Z, ^\\, ^D). They are never held: they drop a
/// held Enter and are written at once.
pub fn is_signal_byte(byte: &u8) -> bool {
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
    /// As [`Self::Insert`], for a line the user began typing right after a command, before the app
    /// knew the shell was back at its prompt (#933): not typeahead for a running program, so an
    /// Enter on it must wait for a classification of the text.
    InsertYoung,
    /// Insert it, then run the Enter that was deferred while it was outstanding, as a shell
    /// command (shell typeahead).
    InsertThenSubmit,
    /// As [`Self::InsertThenSubmit`], for a line the user began typing right after a command,
    /// before the app knew the shell was back at its prompt (#933): the Enter is routed like any
    /// Enter in the editor.
    InsertThenSubmitYoung,
    /// Insert it, but keep the Enter deferred: the shell's line still holds user bytes this
    /// report did not cover (written behind the request, #1104). A further request is written
    /// behind them, and the Enter runs on the report that answers it.
    InsertKeepDeferred,
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
    /// When the editor's last command was written, until its preexec arrives: that round trip
    /// (write, shell preexec hook, pty read, parse) is sampled like a request-to-report one
    /// (#1131), so the young hold is measured from the session's first command on, before any
    /// typeahead exists, instead of starting at `YOUNG_ENTER_MIN_HOLD`.
    command_written_at: Option<Instant>,
    /// Whether the pty's foreground process group is provably not a shell at a prompt. A line
    /// typed into such a program cannot be a #933 line typed at an unnoticed prompt.
    foreground_probe: ForegroundProbe,
    /// When each outstanding hand-off request was written, oldest first (reports come back in
    /// order, so the front is the one a report answers).
    rtt_pending: VecDeque<Instant>,
    /// The last [`RTT_WINDOW`] request-to-report round trips of this session.
    rtt_samples: VecDeque<Duration>,
    /// The user's Enter, and every key typed after it, held while the pty owns input and the
    /// command that was written may already have finished (#933). Nothing in it was written.
    young_hold: Option<Vec<u8>>,
    /// A hold was just created, so the view must arm its expiry timer.
    young_hold_started: bool,
    /// Identifies the current hold; a timer armed for an earlier one must not release it.
    young_hold_id: u64,
    /// The length of each routed write appended to `young_hold`, in typed order (#1162).
    young_hold_chunks: Vec<usize>,
    /// Whether this session's input goes through ConPTY (Windows). There a held run of keys is
    /// written back one chunk per key event, never as one write with an embedded Enter, and no
    /// request is written while the user is still typing a line (see [`CONPTY_MIDLINE_QUIET`]).
    conpty_input: bool,
    /// Whether the ConPTY prompt hold is on (see [`CONPTY_PROMPT_HOLD_EXPIRY`]); it also needs
    /// `conpty_input`, so no other platform is ever affected.
    prompt_hold_enabled: bool,
    /// When the last precmd arrived, until the shell's line editor is active (or a command
    /// starts): keys typed in between are held.
    awaiting_line_editor_since: Option<Instant>,
    /// Raw user keys held between a precmd and the line editor becoming active (#1162), in typed
    /// order. Separate from `young_hold`, which holds an Enter for a different reason.
    prompt_hold: Option<Vec<u8>>,
    /// The length of each routed write appended to `prompt_hold`.
    prompt_hold_chunks: Vec<usize>,
    /// A prompt hold was just created, so the view must arm its expiry timer.
    prompt_hold_started: bool,
    /// Identifies the current prompt hold; a timer armed for an earlier one must not release it.
    prompt_hold_id: u64,
    /// When the last precmd arrived, whether or not a hold follows (for the release hop).
    last_precmd_at: Option<Instant>,
    /// When the user last typed a byte that reached the shell's line.
    last_line_bytes_at: Option<Instant>,
    /// Whether the line the user is typing began within [`YOUNG_ENTER_WINDOW`] of the last
    /// activity, i.e. before the app could know the shell was at its prompt (#933).
    young_line: bool,
    /// User bytes were written to the pty while the editor owned input and nothing adopted them
    /// (an agent-requested command, a block from before the bootstrap precmd), so the shell's line
    /// may hold text the next legacy report puts in front of the editor's (#1089).
    untracked_line_bytes: bool,
    /// Requests written whose reports have not arrived yet. Reports come back in the order the
    /// requests were written (the tty is FIFO).
    requests_outstanding: u32,
    /// How many more reports must arrive before one answers a request written after the
    /// untracked bytes; `None` while no such request was written.
    untracked_cover_after_reports: Option<u32>,
    /// Whether the Enter deferred behind the report ends such a line.
    deferred_young: bool,
    /// Identifies a deferred Enter that waits for a report covering untracked bytes (#1104). If
    /// that report never comes (a program ate the request), the view gives the Enter up when its
    /// timer for this id fires.
    uncovered_deferral_id: Option<u64>,
    /// A deferral was just armed, so the view must start its timer.
    uncovered_deferral_started: bool,
    uncovered_deferral_seq: u64,
    /// Identifies an Enter deferred behind the hand-off's own outstanding report (#1215). If
    /// that report never comes, the view gives the Enter up when its timer for this id fires,
    /// without running it.
    report_deferral_id: Option<u64>,
    /// A report deferral was just armed, so the view must start its timer.
    report_deferral_started: bool,
    report_deferral_seq: u64,
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

    /// Marks the session's input as going through ConPTY (#1162).
    pub fn set_conpty_input(&mut self, conpty_input: bool) {
        self.conpty_input = conpty_input;
    }

    /// Turns the ConPTY prompt hold on or off (#1162). Only has an effect on a ConPTY session.
    pub fn set_conpty_prompt_hold(&mut self, enabled: bool) {
        self.prompt_hold_enabled = enabled;
    }

    fn should_hold_for_prompt(&self, reaches_shell_line: bool, now: Instant) -> bool {
        self.conpty_input
            && self.prompt_hold_enabled
            && reaches_shell_line
            && matches!(self.state, HandoffState::PtyOwned { .. })
            && self
                .awaiting_line_editor_since
                .is_some_and(|at| now.saturating_duration_since(at) < CONPTY_PROMPT_HOLD_EXPIRY)
    }

    /// Takes the "a prompt hold was just created" flag, returning its id so the view arms the
    /// expiry timer once.
    pub fn take_prompt_hold_started(&mut self) -> Option<u64> {
        mem::take(&mut self.prompt_hold_started).then_some(self.prompt_hold_id)
    }

    #[cfg(test)]
    pub(crate) fn has_prompt_hold(&self) -> bool {
        self.prompt_hold.is_some()
    }

    /// A signal or EOF byte is about to be written while keys are held for the prompt: the held
    /// keys (an Enter among them may have to run before the ^C reaches it) are returned with
    /// `bytes`, to be routed and written as one user write, in typed order.
    pub fn take_prompt_hold_before_signal(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        if !bytes.iter().any(is_signal_byte) {
            return None;
        }
        self.awaiting_line_editor_since = None;
        self.prompt_hold_chunks.clear();
        let mut held = self.prompt_hold.take()?;
        held.extend_from_slice(bytes);
        Some(held)
    }

    /// Routes the held prompt keys one chunk per original key event, in order, and returns the
    /// chunks to write. A chunk the current state absorbs (`HoldAfterEnter`) is not returned.
    fn drain_prompt_hold(&mut self) -> Vec<Vec<u8>> {
        let Some(bytes) = self.prompt_hold.take() else {
            self.prompt_hold_chunks.clear();
            return Vec::new();
        };
        let lengths = mem::take(&mut self.prompt_hold_chunks);
        let lengths = if lengths.iter().sum::<usize>() == bytes.len() {
            lengths
        } else {
            vec![bytes.len()]
        };
        let mut start = 0;
        let mut chunks = Vec::with_capacity(lengths.len());
        for len in lengths {
            let chunk = bytes[start..start + len].to_vec();
            start += len;
            match self.route_user_bytes_unheld(&chunk, true) {
                UserBytesRoute::WriteCounted => chunks.push(chunk),
                UserBytesRoute::Write => {
                    // Not counted by the route, but the writer retires every chunk it writes.
                    self.writes_in_flight = self.writes_in_flight.saturating_add(1);
                    chunks.push(chunk);
                }
                UserBytesRoute::Held => (),
            }
        }
        chunks
    }

    /// The shell's line editor became active (#1162): the held keys are due now, in typed
    /// order, ahead of any request. Called before the hand-off decides on a request, so the
    /// decision sees the keys as written. Also ends the wait for the line editor.
    pub fn release_prompt_hold_on_active(&mut self) -> Vec<Vec<u8>> {
        self.awaiting_line_editor_since = None;
        let waited_ms = self
            .last_precmd_at
            .map(|at| at.elapsed().as_secs_f64() * 1000.0);
        let chunks = self.drain_prompt_hold();
        warpui::input_order::log_hop("line_editor_active", || {
            format!(
                "since_precmd_ms={} released_chunks={}",
                waited_ms.map_or_else(|| "none".to_string(), |ms| format!("{ms:.1}")),
                chunks.len()
            )
        });
        chunks
    }

    /// The expiry of prompt hold `id` ran out with no line editor signal: the keys are written
    /// anyway. `None` when that hold is gone already.
    pub fn expire_prompt_hold(&mut self, id: u64) -> Option<Vec<Vec<u8>>> {
        if id != self.prompt_hold_id || self.prompt_hold.is_none() {
            return None;
        }
        self.awaiting_line_editor_since = None;
        let chunks = self.drain_prompt_hold();
        warpui::input_order::log_hop("prompt_hold_expired", || {
            format!("released_chunks={}", chunks.len())
        });
        Some(chunks)
    }

    /// Whether, on a ConPTY session, the user typed into the shell's line so recently that a
    /// request now would land in the middle of the line (#1162).
    fn typing_a_line_on_conpty(&self, now: Instant) -> bool {
        self.conpty_input
            && self
                .last_line_bytes_at
                .is_some_and(|at| now.saturating_duration_since(at) < CONPTY_MIDLINE_QUIET)
    }

    pub fn set_foreground_probe(&mut self, probe: ForegroundProbe) {
        self.foreground_probe = probe;
    }

    /// The p90 of the measured request-to-report round trips, `None` before any.
    fn rtt_p90(&self) -> Option<Duration> {
        if self.rtt_samples.is_empty() {
            return None;
        }
        let mut sorted: Vec<Duration> = self.rtt_samples.iter().copied().collect();
        sorted.sort_unstable();
        let index = (sorted.len() * 9).div_ceil(10).saturating_sub(1);
        sorted.get(index).copied()
    }

    /// How long the view should wait for the prompt before releasing the hold just started. The
    /// race of #933 can only be a precmd already in flight, which lands within about one round
    /// trip, so with measurements it is [`RTT_HOLD_FACTOR`] times their p90, between
    /// [`YOUNG_ENTER_FLOOR`] and what is left of [`YOUNG_ENTER_WINDOW`] counted from the last
    /// activity. Before any round trip was measured it is that remainder, never less than
    /// [`YOUNG_ENTER_MIN_HOLD`]. A prompt or report arriving first releases the hold at once.
    pub fn young_hold_wait(&self, now: Instant) -> Duration {
        let left = self.last_activity_at.map_or(Duration::ZERO, |at| {
            YOUNG_ENTER_WINDOW.saturating_sub(now.saturating_duration_since(at))
        });
        match self.rtt_p90() {
            Some(p90) => p90
                .saturating_mul(RTT_HOLD_FACTOR)
                .min(left)
                .clamp(YOUNG_ENTER_FLOOR, YOUNG_ENTER_WINDOW),
            None => left.clamp(YOUNG_ENTER_MIN_HOLD, YOUNG_ENTER_WINDOW),
        }
    }

    #[cfg(test)]
    pub(crate) fn record_rtt_for_test(&mut self, rtt: Duration) {
        self.record_rtt(rtt);
    }

    /// A new shell session (not a subshell) took the pty over, e.g. the remote shell of an `ssh`
    /// (#1131): its loop through the shell and the pipeline is unrelated to the previous
    /// session's, so the measured round trips are dropped and the next command measures afresh.
    /// A subshell (`sudo -i`, `ksu`) runs on the same machine and keeps them.
    pub fn on_new_session(&mut self) {
        self.rtt_samples.clear();
        self.rtt_pending.clear();
        self.command_written_at = None;
    }

    /// Records one measured round trip through the shell and the pty pipeline, in the rolling
    /// window the young hold is sized from.
    fn record_rtt(&mut self, rtt: Duration) {
        if self.rtt_samples.len() >= RTT_WINDOW {
            self.rtt_samples.pop_front();
        }
        self.rtt_samples.push_back(rtt);
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
        // #1089: also while user bytes the hand-off did not adopt may be in the shell's line,
        // until a report requested behind them is in: the legacy report puts them in front of
        // the editor's text just the same (`echo "a b` + ` c" > x` became `echo "a b c" > x"`).
        !matches!(self.state, HandoffState::EditorOwned) || self.untracked_line_bytes
    }

    /// User bytes that reach the shell's line are about to be written while the editor owns
    /// input and the hand-off did not adopt them (#1089; see
    /// [`Self::adopt_untracked_typeahead`] for the keys it does adopt). Only meaningful for a
    /// shell that reports its line; a no-op unless the editor owns input. A request already
    /// outstanding does not cover them: they are behind it in the tty.
    pub fn note_untracked_line_bytes(&mut self) {
        if matches!(
            self.state,
            HandoffState::EditorOwned | HandoffState::AwaitingReport
        ) {
            self.untracked_line_bytes = true;
            self.untracked_cover_after_reports = None;
        }
    }

    /// `PtyController` wrote a legacy request (the editor owns input). Its report covers every
    /// user byte written before it.
    pub fn on_legacy_request_written(&mut self) {
        self.on_request_written();
    }

    /// A request (legacy or the hand-off's own) is written: its report, once every report ahead
    /// of it is in, covers the untracked bytes written before it.
    fn on_request_written(&mut self) {
        if self.rtt_pending.len() >= RTT_WINDOW {
            self.rtt_pending.pop_front();
        }
        self.rtt_pending.push_back(Instant::now());
        self.requests_outstanding = self.requests_outstanding.saturating_add(1);
        if self.untracked_line_bytes && self.untracked_cover_after_reports.is_none() {
            self.untracked_cover_after_reports = Some(self.requests_outstanding);
        }
    }

    /// A report arrived: once it is the one that answers a request written after the untracked
    /// bytes, they are back in the editor.
    fn on_report_received(&mut self) {
        if let Some(written_at) = self.rtt_pending.pop_front() {
            self.record_rtt(written_at.elapsed());
        }
        self.requests_outstanding = self.requests_outstanding.saturating_sub(1);
        if let Some(remaining) = self.untracked_cover_after_reports.as_mut() {
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                self.forget_untracked_line_bytes();
            }
        }
    }

    /// Whether a request written after the untracked bytes is outstanding, for tests.
    #[cfg(test)]
    pub(crate) fn untracked_bytes_requested(&self) -> bool {
        self.untracked_cover_after_reports.is_some()
    }

    /// The shell's line no longer holds untracked user bytes: they were reported, or a command's
    /// kill prefix ended the line. Outstanding requests are still in the tty and stay counted.
    fn forget_untracked_line_bytes(&mut self) {
        self.untracked_line_bytes = false;
        self.untracked_cover_after_reports = None;
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
        ) || self.untracked_bytes_uncovered()
    }

    /// Untracked user bytes (#1089) are in the shell's line and no request has been written
    /// behind them (#1104): a queued command's kill prefix would destroy them unreported.
    fn untracked_bytes_uncovered(&self) -> bool {
        self.untracked_line_bytes
            && self.untracked_cover_after_reports.is_none()
            && matches!(
                self.state,
                HandoffState::EditorOwned | HandoffState::AwaitingReport
            )
    }

    /// Whether the editor owns input and a report has left untracked bytes in the shell's line
    /// that no request covers yet, so the view should ask the controller to write one (#1104).
    pub fn drain_pending(&self) -> bool {
        self.state == HandoffState::EditorOwned && self.untracked_bytes_uncovered()
    }

    /// Whether a request must be written now, because untracked bytes sit in the shell's line
    /// behind every request written so far (#1104): the shell reports its line as of the request
    /// and clears it, so what was typed after it would stay there, to be killed by the next
    /// command's kill prefix or to prefix the next line. Records the request as written when it
    /// is due. `at_prompt` and `can_request` are as for [`Self::on_line_editor_active`].
    pub fn drain_request_due(&mut self, at_prompt: bool, can_request: bool) -> bool {
        let owner_allows = matches!(
            self.state,
            HandoffState::EditorOwned
                | HandoffState::HoldAfterEnter {
                    released: false,
                    ..
                }
        );
        if !(at_prompt
            && can_request
            && owner_allows
            && self.untracked_line_bytes
            && self.untracked_cover_after_reports.is_none())
        {
            return false;
        }
        self.on_request_written();
        true
    }

    /// The id of a deferral the view must arm a patience timer for (see
    /// [`Self::give_up_uncovered_deferral`]), once.
    pub fn take_uncovered_deferral_started(&mut self) -> Option<u64> {
        if mem::take(&mut self.uncovered_deferral_started) {
            self.uncovered_deferral_id
        } else {
            None
        }
    }

    fn arm_uncovered_deferral(&mut self) {
        self.uncovered_deferral_seq = self.uncovered_deferral_seq.wrapping_add(1);
        self.uncovered_deferral_id = Some(self.uncovered_deferral_seq);
        self.uncovered_deferral_started = true;
    }

    /// The id of a report deferral the view must arm a patience timer for (see
    /// [`Self::give_up_report_deferral`]), once.
    pub fn take_report_deferral_started(&mut self) -> Option<u64> {
        if mem::take(&mut self.report_deferral_started) {
            self.report_deferral_id
        } else {
            None
        }
    }

    fn arm_report_deferral(&mut self) {
        self.report_deferral_seq = self.report_deferral_seq.wrapping_add(1);
        self.report_deferral_id = Some(self.report_deferral_seq);
        self.report_deferral_started = true;
    }

    /// The patience timer of report deferral `id` ran out (#1215): the report the Enter waited
    /// for is not coming (a program ate the request, the shell hung). The Enter is given up
    /// WITHOUT running: the shell's line still holds the head of what the user typed, so the
    /// editor's text would run as a fragment. Input goes back to the editor; a late report is then
    /// inserted in front of it like any legacy report. Returns the keys held behind the Enter,
    /// for the view to put in the editor; `None` when the deferral is over already.
    pub fn give_up_report_deferral(&mut self, id: u64) -> Option<Vec<u8>> {
        if self.report_deferral_id != Some(id) {
            return None;
        }
        let HandoffState::HoldAfterEnter {
            released: false, ..
        } = &self.state
        else {
            return None;
        };
        self.report_deferral_id = None;
        self.uncovered_deferral_id = None;
        self.deferred_young = false;
        self.young_hold = None;
        match mem::take(&mut self.state) {
            HandoffState::HoldAfterEnter { held, .. } => Some(held),
            HandoffState::EditorOwned | HandoffState::PtyOwned { .. } | HandoffState::AwaitingReport => {
                None
            }
        }
    }

    /// The patience timer of deferral `id` ran out: the report covering the untracked bytes
    /// never came (a program ate the request), so the Enter runs as it did before #1104 rather
    /// than wait forever. Returns whether it was released; the caller then runs the Enter.
    pub fn give_up_uncovered_deferral(&mut self, id: u64) -> bool {
        if self.uncovered_deferral_id != Some(id)
            || !matches!(
                self.state,
                HandoffState::HoldAfterEnter {
                    released: false,
                    ..
                }
            )
        {
            return false;
        }
        self.uncovered_deferral_id = None;
        self.forget_untracked_line_bytes();
        self.deferred_young = false;
        self.mark_released();
        true
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
        self.forget_untracked_line_bytes();
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
        let now = Instant::now();
        self.last_activity_at = Some(now);
        self.command_written_at = Some(now);
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
        self.forget_untracked_line_bytes();
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
        let now = Instant::now();
        self.last_activity_at = Some(now);
        self.last_precmd_at = Some(now);
        if self.conpty_input && self.prompt_hold_enabled {
            self.awaiting_line_editor_since = Some(now);
        }
        warpui::input_order::log_hop("precmd_seen", String::new);
        if let HandoffState::PtyOwned { precmds_seen, .. } = &mut self.state {
            *precmds_seen = precmds_seen.saturating_add(1);
        }
    }

    /// The shell emitted a preexec hook (model thread). `shell_started` is whether the shell
    /// started this command itself (a typeahead line), rather than the app writing it. Returns
    /// whether the pty took over input.
    pub fn on_preexec(&mut self, shell_started: bool) -> bool {
        self.activity_generation = self.activity_generation.wrapping_add(1);
        // The shell took a line: its line editor is not about to start.
        self.awaiting_line_editor_since = None;
        self.last_activity_at = Some(Instant::now());
        // The editor's command reached the shell: one write-to-preexec loop measured. A line the
        // shell started itself ran ahead of that command, so its preexec is not the command's and
        // the command's own would include the line's run: the sample is dropped.
        if let Some(written_at) = self.command_written_at.take()
            && !shell_started
        {
            self.record_rtt(written_at.elapsed());
        }
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

    /// The user is about to write keys to a running command the hand-off does not own (one the
    /// queued-command panel or an env-var collection wrote, or any command started while the
    /// editor owned input) (#1089). Whatever the command leaves unread is in the shell's line at
    /// the next prompt, so the hand-off takes it over exactly as it does a command submitted from
    /// the editor: the keys are counted, the request is written only behind every one of them,
    /// and the editor inserts what they typed literally until the report is in.
    ///
    /// Left to the legacy path, those keys were never counted. The legacy request could already
    /// be in the tty ahead of them (it goes out whenever the line editor is active), so the
    /// shell's report missed them and they stayed in its line, to prefix the next line run there
    /// (`)exit`) or be killed by the next command's kill prefix. Keys the editor took meanwhile
    /// were auto-paired, and the late report was placed in front of them with the cursor moved to
    /// the end: `$(uname -n)` became `$()uname -n)`.
    ///
    /// The command's own Enter is counted as unanswered, so its prompt (not an earlier one) is
    /// the one that hands input back. Returns whether ownership changed; it only does from
    /// [`HandoffState::EditorOwned`].
    pub fn adopt_untracked_typeahead(&mut self) -> bool {
        if self.state != HandoffState::EditorOwned {
            return false;
        }
        self.state = HandoffState::PtyOwned {
            wrote_user_bytes: false,
            enters_written: 1,
            precmds_seen: 0,
        };
        true
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
            Owner::Hold { released: false } if self.drain_request_due(at_prompt, can_request) => {
                ActiveDecision::SendRequest
            }
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
                } else if self.typing_a_line_on_conpty(Instant::now()) {
                    // #1162: keys of a line are still arriving raw; the request waits for a
                    // pause (the quiet-period timer or the next flushed write decides again).
                    ActiveDecision::Wait {
                        activity_generation: self.activity_generation,
                    }
                } else if no_line_queued && at_prompt && self.writes_in_flight == 0 {
                    self.state = HandoffState::AwaitingReport;
                    // Its report covers every untracked byte written before it too (#1089).
                    self.on_request_written();
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
            self.young_hold_chunks.push(bytes.len());
            // #1189: held keys are still typing; the ConPTY mid-line guard must see them, or a
            // request is written between them once the hold is released.
            if reaches_shell_line {
                self.last_line_bytes_at = Some(Instant::now());
            }
            return UserBytesRoute::Held;
        }
        // #1162: keys typed after a prompt's precmd and before its line editor is active stay
        // behind any key already held for it, never overtaking.
        if let Some(hold) = &mut self.prompt_hold {
            hold.extend_from_slice(bytes);
            self.prompt_hold_chunks.push(bytes.len());
            if reaches_shell_line {
                self.last_line_bytes_at = Some(Instant::now());
            }
            return UserBytesRoute::Held;
        }
        if self.young_hold.is_none()
            && self.should_hold_for_prompt(reaches_shell_line, Instant::now())
        {
            self.prompt_hold = Some(bytes.to_vec());
            self.prompt_hold_chunks = vec![bytes.len()];
            self.prompt_hold_id = self.prompt_hold_id.wrapping_add(1);
            self.prompt_hold_started = true;
            self.last_line_bytes_at = Some(Instant::now());
            return UserBytesRoute::Held;
        }
        if self.should_hold_young_enter(bytes, reaches_shell_line, Instant::now()) {
            self.young_hold = Some(bytes.to_vec());
            self.young_hold_chunks = vec![bytes.len()];
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
                // A plain program in the foreground means the shell is waiting on it, not at a
                // prompt: the Enter is for that program, not a line the app has yet to notice.
                && !self.foreground_probe.foreground_is_plain_program()
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

    /// Whether an Enter pressed in the editor is deferred and the keys typed since are held
    /// behind it (#1016). False once the report arrived and the deferred submit runs: that
    /// submit is itself an Enter reaching the editor, and must not be held again.
    pub fn holds_keys_behind_deferred_enter(&self) -> bool {
        matches!(
            self.state,
            HandoffState::HoldAfterEnter {
                released: false,
                ..
            }
        )
    }

    /// Puts `bytes` in front of the keys held behind the deferred Enter (#1016): they reached
    /// the editor after the Enter was pressed but before the terminal took the keys, so they
    /// precede everything held. Returns whether an Enter is deferred; if not, nothing changed.
    pub fn prepend_held_keys(&mut self, bytes: &[u8]) -> bool {
        match &mut self.state {
            HandoffState::HoldAfterEnter { held, .. } => {
                held.splice(0..0, bytes.iter().copied());
                true
            }
            _ => false,
        }
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

    /// A lone EOF (^D) is about to be written while an Enter is held: the tty keeps typeahead
    /// across it (unlike ^C, ^Z and ^\\, which flush it), so the held Enter and the keys behind
    /// it must reach the program before the EOF (#933). Returns them with the EOF, to be routed
    /// and written as one user write.
    pub fn take_young_hold_before_eof(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        if !bytes.contains(&0x04) || bytes.iter().any(|b| matches!(b, 0x03 | 0x1a | 0x1c)) {
            return None;
        }
        if !matches!(self.state, HandoffState::PtyOwned { .. }) {
            return None;
        }
        let mut held = self.young_hold.take()?;
        held.extend_from_slice(bytes);
        Some(held)
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
        self.young_hold_chunks.clear();
        self.route_user_bytes_unheld(&bytes, true);
        Some(bytes)
    }

    /// As [`Self::expire_young_hold`], but on a ConPTY session the bytes come back as the
    /// separate writes the user typed them in (#1162), each routed and counted on its own, so
    /// they reach the pty in the granularity of the controls that never lost a line. Elsewhere
    /// it is one chunk, exactly what `expire_young_hold` returns.
    pub fn expire_young_hold_chunks(&mut self, id: u64) -> Option<Vec<Vec<u8>>> {
        if !self.conpty_input {
            return self.expire_young_hold(id).map(|bytes| vec![bytes]);
        }
        self.drop_stale_hold();
        if id != self.young_hold_id || !matches!(self.state, HandoffState::PtyOwned { .. }) {
            return None;
        }
        let bytes = self.young_hold.take()?;
        let lengths = mem::take(&mut self.young_hold_chunks);
        let lengths = if lengths.iter().sum::<usize>() == bytes.len() {
            lengths
        } else {
            vec![bytes.len()]
        };
        let mut start = 0;
        let mut chunks = Vec::with_capacity(lengths.len());
        for len in lengths {
            let chunk = bytes[start..start + len].to_vec();
            start += len;
            self.route_user_bytes_unheld(&chunk, true);
            chunks.push(chunk);
        }
        Some(chunks)
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
                    self.last_line_bytes_at = Some(Instant::now());
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
                if reaches_shell_line {
                    self.last_line_bytes_at = Some(Instant::now());
                }
                UserBytesRoute::Held
            }
            // In `AwaitingReport` such a write is a key typed into a command drained behind the
            // outstanding request (a queued command written right after it): the tty holds the
            // request ahead of the key, so the report cannot miss anything typed before it, and
            // the key reaches that command, or the shell's line at that command's prompt, after
            // the report. It is not adopted from here: the editor may already hold keys typed
            // after the request, and a second report would land in front of them, out of typed
            // order. The report hands input to the editor (`on_report`), and from then on keys
            // typed into the running command are adopted (#1089). The keys of that window (the
            // report's latency) stay where they went and come back with the next prompt's legacy
            // report; until that report the editor inserts literally (the view notes them with
            // `note_untracked_line_bytes`).
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
        // #1104: also while the editor owns input but a request is outstanding that does not
        // cover untracked bytes in the shell's line: submitting now would write the kill prefix
        // and the command behind that request, the line would run without its head, and the
        // report would land in the fresh editor as a fragment.
        let behind_uncovered_report = self.state == HandoffState::EditorOwned
            && self.untracked_line_bytes
            && self.requests_outstanding > 0;
        if self.state != HandoffState::AwaitingReport && !behind_uncovered_report {
            return false;
        }
        if behind_uncovered_report {
            self.arm_uncovered_deferral();
            self.report_deferral_id = None;
        } else {
            self.uncovered_deferral_id = None;
            self.arm_report_deferral();
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
        // #1089: a report for a request written before untracked bytes does not cover them.
        self.on_report_received();
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
                let young = mem::take(&mut self.young_line);
                self.state = HandoffState::EditorOwned;
                if young {
                    ReportDisposition::InsertYoung
                } else {
                    ReportDisposition::Insert
                }
            }
            Owner::Hold { released: false } if self.untracked_line_bytes => {
                // This report left bytes behind it in the shell's line (#1104): a request
                // written after them will report them. The Enter waits for that report.
                self.report_deferral_id = None;
                self.arm_uncovered_deferral();
                ReportDisposition::InsertKeepDeferred
            }
            Owner::Hold { released: false } => {
                self.uncovered_deferral_id = None;
                self.report_deferral_id = None;
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
                self.uncovered_deferral_id = None;
                self.report_deferral_id = None;
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
