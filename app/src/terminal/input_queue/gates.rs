//! The gates of the input queue (#1353 design, section 2.2).
//!
//! A gate closes the queue from one position on: the entry it was opened at, and everything
//! behind it, wait. Every gate has a named EVENT that opens it again; the optional cap is only a
//! give-up safety net for an event that may never arrive (it is requested through
//! `Host::arm_give_up`, fires at most once and is guarded by the gate's token), never the thing
//! correctness depends on.

use std::time::Duration;

use super::{EntryId, SignalKind};

/// What a gate waits for. Each variant names the legacy buffer it will replace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GateKind {
    /// A pty- or editor-bound paste is being read off-thread (Q4, Q4b).
    PasteReading,
    /// An Enter waits for the autodetect verdict (Q1).
    EnterVerdict,
    /// An Enter waits for the Tab completion in flight (Q1). Its own kind since phase 5b; it used
    /// to share `EnterVerdict` with an id as the only difference.
    TabCompletion,
    /// A paced CLI-agent submit writes its pieces (phase 3): keys typed meanwhile wait behind the
    /// whole submit. Its own kind since phase 5b; it used to borrow `PasteReading` with a key
    /// above every paste id.
    CliSubmit,
    /// The hand-off asked the shell for its line (`ESC i`) and waits for the report (Q3a).
    HandoffReport,
    /// An Enter arrived while the last command may already have finished (Q3b); phase 5b keeps the
    /// held Enter and the keys behind it here (`view/paste_queue.rs`), keyed by the hand-off's hold
    /// id and ended by the hand-off's decision (a command, the report, a signal), never a timer.
    PromptSeen,
    /// ConPTY: between a precmd and the shell's line editor becoming active (Q3c); phase 5 keeps
    /// the held keys here (`view/paste_queue.rs`), opened by the line editor's bracketed-paste signal.
    LineEditorActive,
    /// A subshell warpify setup was announced and is unfinished (Q6a).
    SubshellSetup,
    /// PowerShell warpify setup (Q6a, line-by-line delivery).
    PowershellSetup,
    /// The paced subshell bootstrap lines are being written (Q6b).
    SubshellBootstrapWrite,
    /// An `ssh` connect is in flight until the remote shell's first prompt (Q7, Q5).
    SshConnect,
    /// A held editor key that reads the display map (cursor motion, deletion by display position)
    /// is waiting for the map to catch up with the edits applied before it in the same update
    /// (#1389). Replaces the editor's separate replay list on the shared queue: every key behind
    /// the waiting one stays behind it.
    EditorDisplayMap,
    /// #2107: the Up arrow opened the inline history menu on a history too long to list at once
    /// (the list builds off-thread), so the recalled line is not in the buffer yet. Every key typed
    /// meanwhile waits here, in order, and is applied to the recalled line once the list has landed
    /// and its first row has put that line in the buffer. Without it the keys edited the EMPTY
    /// buffer and the recalled line then replaced them. Opened only when the menu's list is
    /// pending (a small history answers inline and never opens it).
    HistoryRecall,
}

/// #1571: the key flag of an `EnterVerdict` gate that holds the keys typed behind an Enter the
/// shell could not take yet (`Input::hold_refused_unready_enter`). It is an Enter gate in every
/// other respect (the table's H5 row, the editor's behind-Enter queueing, the after-Enter release),
/// so it reuses that kind; the flag tells the editor to hold its editing keys behind it as well
/// (they edit the NEXT line, never the held one) and is never set on a verdict wait's id.
pub const UNREADY_ENTER_KEY_FLAG: u64 = 1 << 62;

/// Whether `key` names the gate of an Enter held because the shell was not ready (#1571).
pub fn is_unready_enter_key(key: u64) -> bool {
    key & UNREADY_ENTER_KEY_FLAG != 0
}

/// #1652: the key flag of the gate of an Enter that waits for the multi-line submit dialog's
/// answer. Always set together with [`UNREADY_ENTER_KEY_FLAG`] (the keys behind it are the NEXT
/// line's, as behind an unready Enter); it adds one rule: an editor key with no tty form is
/// queued as an editor key rather than acting at once on (or being dropped from) the waiting
/// buffer, since every key typed while the dialog is open counts.
pub const MULTILINE_DIALOG_KEY_FLAG: u64 = 1 << 61;

/// Whether `key` names the gate of an Enter waiting for the multi-line dialog (#1652).
pub fn is_multiline_dialog_key(key: u64) -> bool {
    key & MULTILINE_DIALOG_KEY_FLAG != 0
}

/// What a signal (^C, ^D, ^Z, ^\) does to the entries held behind the gate it meets (design 2.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalPlan {
    /// The paste behind the gate is cancelled (never delivered); the entries typed before the
    /// signal are delivered in order, then the signal.
    CancelPasteThenFlush,
    /// Everything held is delivered in typed order, then the signal goes last.
    FlushThenSignal,
    /// Everything held is discarded (the tty flushes its input on ^C, or the Enter is voided),
    /// then the signal goes.
    DiscardThenSignal,
    /// The signal goes first; the gate is released and the held entries follow it.
    SignalThenRelease,
    /// The signal goes first; the held entries STAY held until the gate's own event.
    SignalThenKeepGate,
}

impl GateKind {
    pub const ALL: [GateKind; 13] = [
        GateKind::PasteReading,
        GateKind::EnterVerdict,
        GateKind::TabCompletion,
        GateKind::CliSubmit,
        GateKind::HandoffReport,
        GateKind::PromptSeen,
        GateKind::LineEditorActive,
        GateKind::SubshellSetup,
        GateKind::PowershellSetup,
        GateKind::SubshellBootstrapWrite,
        GateKind::SshConnect,
        GateKind::EditorDisplayMap,
        GateKind::HistoryRecall,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GateKind::PasteReading => "paste_reading",
            GateKind::EnterVerdict => "enter_verdict",
            GateKind::TabCompletion => "tab_completion",
            GateKind::CliSubmit => "cli_submit",
            GateKind::HandoffReport => "handoff_report",
            GateKind::PromptSeen => "prompt_seen",
            GateKind::LineEditorActive => "line_editor_active",
            GateKind::SubshellSetup => "subshell_setup",
            GateKind::PowershellSetup => "powershell_setup",
            GateKind::SubshellBootstrapWrite => "subshell_bootstrap_write",
            GateKind::SshConnect => "ssh_connect",
            GateKind::EditorDisplayMap => "editor_display_map",
            GateKind::HistoryRecall => "history_recall",
        }
    }

    /// The named event that closes the queue on this gate (for the log and the doc table).
    pub fn opened_by(self) -> &'static str {
        match self {
            GateKind::PasteReading => "begin_paste",
            GateKind::EnterVerdict => "enter_without_settled_verdict",
            GateKind::TabCompletion => "enter_during_tab_completion",
            GateKind::CliSubmit => "cli_submit_started",
            GateKind::HandoffReport => "handoff_report_requested",
            GateKind::PromptSeen => "young_enter",
            GateKind::LineEditorActive => "precmd_on_conpty",
            GateKind::SubshellSetup => "subshell_setup_announced",
            GateKind::PowershellSetup => "powershell_setup_announced",
            GateKind::SubshellBootstrapWrite => "bootstrap_write_started",
            GateKind::SshConnect => "ssh_connect_started",
            GateKind::EditorDisplayMap => "editor_key_needs_fresh_display_map",
            GateKind::HistoryRecall => "history_menu_opened_with_pending_list",
        }
    }

    /// The named event that opens the gate again. These are what the owner rule wants correctness
    /// to hang on.
    pub fn closing_event(self) -> &'static str {
        match self {
            GateKind::PasteReading => "paste_read_result",
            GateKind::EnterVerdict => "verdict_ready_or_give_up_submit",
            GateKind::TabCompletion => "tab_completion_done_or_give_up_submit",
            GateKind::CliSubmit => "cli_submit_written",
            GateKind::HandoffReport => "report_received",
            // #1614: the report of the request the prompt made due, or the command the held keys
            // turn out to be typeahead for (a Preexec, a program taking the tty, a command written).
            GateKind::PromptSeen => "report_or_command_running",
            GateKind::LineEditorActive => "line_editor_active",
            GateKind::SubshellSetup => "bootstrapped_then_precmd_or_abandoned",
            GateKind::PowershellSetup => "powershell_prompt_ready",
            GateKind::SubshellBootstrapWrite => "last_bootstrap_line_written",
            GateKind::SshConnect => "gate_released_and_first_remote_precmd",
            GateKind::EditorDisplayMap => "replay_step_dispatched",
            // The first row of the menu's list has put the recalled line in the buffer; also the
            // list found empty, the menu closed or left, or ^C cancelling the recall.
            GateKind::HistoryRecall => "history_list_ready_and_first_row_selected",
        }
    }

    /// The signal rule per gate kind: the table of design 2.5 as code, so a new gate cannot
    /// forget to decide.
    pub fn signal_plan(self) -> SignalPlan {
        match self {
            // A submit holds no paste entry, so the plan only flushes what was typed before the
            // signal, as it did when the submit's gate was a `PasteReading` one.
            GateKind::PasteReading | GateKind::CliSubmit => SignalPlan::CancelPasteThenFlush,
            // ^C with no Enter held ahead of it: the recall is cancelled and the edits typed
            // meanwhile are discarded (at a prompt ^C discards the line anyway). With an Enter
            // held ahead the editor does not offer the signal: it waits in order (see
            // `editor_interrupt_meets_gate`).
            GateKind::EnterVerdict
            | GateKind::TabCompletion
            | GateKind::SshConnect
            | GateKind::HistoryRecall => SignalPlan::DiscardThenSignal,
            GateKind::HandoffReport
            | GateKind::PromptSeen
            | GateKind::LineEditorActive
            | GateKind::EditorDisplayMap => SignalPlan::FlushThenSignal,
            GateKind::SubshellSetup | GateKind::SubshellBootstrapWrite => {
                SignalPlan::SignalThenRelease
            }
            GateKind::PowershellSetup => SignalPlan::SignalThenKeepGate,
        }
    }

    /// The give-up cap proposed by the design (existing values kept as caps only). `None` means
    /// the gate has no cap of its own: the subshell / PowerShell setup gates are ended by the
    /// setup's events or by the ONE setup net in the view (`SUBSHELL_SETUP_QUIET_NET`, 30 s of no
    /// progress; `SUBSHELL_SETUP_CEILING`, 120 s; #1711), which hands their keys back.
    pub fn default_cap(self) -> Option<Duration> {
        match self {
            GateKind::PasteReading => Some(Duration::from_secs(3)),
            GateKind::EnterVerdict | GateKind::TabCompletion => Some(Duration::from_millis(250)),
            // The opener supplies the net (a submit is a few hundred ms to a few seconds).
            GateKind::CliSubmit => Some(Duration::from_secs(20)),
            GateKind::HandoffReport => Some(Duration::from_secs(3)),
            GateKind::PromptSeen => Some(Duration::from_millis(600)),
            // The hand-off's give-up cap since #1349 (the line editor's own signal opens it).
            GateKind::LineEditorActive => Some(Duration::from_millis(1500)),
            GateKind::SubshellSetup
            | GateKind::PowershellSetup
            | GateKind::SubshellBootstrapWrite => None,
            GateKind::SshConnect => Some(Duration::from_secs(5)),
            // The deferred replay step runs as soon as this update's effects have flushed; this
            // only guards a step that never runs (logged when it fires).
            GateKind::EditorDisplayMap => Some(Duration::from_secs(1)),
            // Just above the menu's mixer `initial_results_timeout` (120 s, `inline_history/view.rs`):
            // the list's own limit ends the wait first in every case, this only guards a recall
            // whose list event never came. The give-up closes the menu, then replays the held
            // keys in order on the unchanged buffer (none dropped or reordered).
            GateKind::HistoryRecall => Some(Duration::from_secs(125)),
        }
    }
}

/// What a signal does at a gate, per signal (#1424, OD-1): the plan depends on the signal as well
/// as on the gate, because the tty flushes pending input on INTR, QUIT and SUSP but not on EOF.
/// The legacy [`SignalPlan`] is per gate only and stays what `InputQueue::apply_signal` runs; this
/// is the table's form, with the cases the legacy code handles outside the queue (the hand-off's
/// deferred Enter, the subshell bootstrap stash, the held keys' void) named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // used only by tests
pub enum GateSignalPlan {
    /// The paste is cancelled, the keys typed before the signal are delivered, then the signal.
    CancelPasteThenFlush,
    /// Everything held is delivered in typed order, then the signal goes last.
    FlushThenSignal,
    /// Everything held is discarded, then the signal goes.
    DiscardThenSignal,
    /// The signal goes first; the gate is released and the held entries follow it.
    SignalThenRelease,
    /// The signal goes first; the held entries stay held until the gate's own event.
    SignalThenKeepGate,
    /// The deferred Enter is cancelled first (`cancel_deferred_enter`), the keys held behind it
    /// are dropped, then the signal goes (the HandoffReport gate for INTR, SUSP, QUIT).
    CancelDeferredEnterThenSignal,
    /// The signal waits behind the gate like a key, in typed order (^D behind a deferred Enter:
    /// EOF is not flushed by the tty, so it must go out after the keys before it).
    WaitInOrder,
    /// The keys held at a prompt hold are void (the tty flushes them), then the signal goes.
    VoidHeldThenSignal,
    /// A signal meeting the bootstrap-write gate as a queue ENTRY (a ^Z / ^\ in `table` mode,
    /// `SubshellPrep::HeldInOrder`): it waits for the last bootstrap line, so it does not cut a
    /// half-run script. A Ctrl-C / Ctrl-D never reaches the gate this way since #1711 review F1:
    /// the writer ends the setup and writes it at once (`abort_in_flight_subshell_setup`).
    StashUntilBootstrapDone,
}

/// The plan for `signal` meeting `gate` (the table's [`GateKind::signal_plan`], per signal).
/// Interrupt and Eof are what the legacy code does; the Suspend and Quit columns are the deliberate
/// OD-1 change (^Z and ^\ jump the queue like ^C). The ssh setup window is NOT a case of this
/// function: inside it the table's P1a row holds the signal instead (OD-5).
#[allow(dead_code)] // used by sibling unused code or shared name
pub fn signal_plan(gate: GateKind, signal: SignalKind) -> GateSignalPlan {
    let eof = signal == SignalKind::Eof;
    match gate {
        GateKind::PasteReading | GateKind::CliSubmit => GateSignalPlan::CancelPasteThenFlush,
        GateKind::EnterVerdict | GateKind::TabCompletion | GateKind::SshConnect => {
            GateSignalPlan::DiscardThenSignal
        }
        // ^D is not flushed by the tty: it waits in order like a key. The other signals cancel
        // the recall like ^C (an Enter held ahead of a ^C makes the editor queue it instead).
        GateKind::HistoryRecall if eof => GateSignalPlan::WaitInOrder,
        GateKind::HistoryRecall => GateSignalPlan::DiscardThenSignal,
        GateKind::HandoffReport if eof => GateSignalPlan::WaitInOrder,
        GateKind::HandoffReport => GateSignalPlan::CancelDeferredEnterThenSignal,
        GateKind::PromptSeen | GateKind::LineEditorActive if eof => GateSignalPlan::FlushThenSignal,
        GateKind::PromptSeen | GateKind::LineEditorActive => GateSignalPlan::VoidHeldThenSignal,
        GateKind::EditorDisplayMap => GateSignalPlan::FlushThenSignal,
        GateKind::SubshellSetup => GateSignalPlan::SignalThenRelease,
        GateKind::SubshellBootstrapWrite => GateSignalPlan::StashUntilBootstrapDone,
        GateKind::PowershellSetup => GateSignalPlan::SignalThenKeepGate,
    }
}

/// Identity of one opened gate. A cap timer carries it; a timer for a gate that already closed
/// finds nothing and does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GateToken(pub u64);

/// What a caller asks for when it opens a gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateSpec {
    pub kind: GateKind,
    /// Distinguishes two gates of one kind (paste id, wait id, connection id ...). The closing
    /// event must carry the same key.
    pub key: u64,
    pub cap: Option<Duration>,
}

impl GateSpec {
    pub fn new(kind: GateKind, key: u64) -> Self {
        Self {
            kind,
            key,
            cap: kind.default_cap(),
        }
    }

    pub fn with_cap(mut self, cap: Option<Duration>) -> Self {
        self.cap = cap;
        self
    }
}

/// An open gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gate {
    pub kind: GateKind,
    pub key: u64,
    pub token: GateToken,
    /// Entries with `id >= barrier` wait. Arrival order is the only position there is.
    pub barrier: EntryId,
    pub cap: Option<Duration>,
}
